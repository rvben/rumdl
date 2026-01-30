// Use jemalloc for better memory allocation performance on Unix-like systems
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Use mimalloc on Windows for better performance
#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, shells::Shell};
use colored::*;
use core::error::Error;
use memmap2::Mmap;
use std::fs;
use std::io::{self, Write, stdout};
use std::path::Path;
use std::str::FromStr;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_block_utils::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::strong_style::StrongStyle;

use rumdl_config::ConfigSource;
use rumdl_config::normalize_key;
use rumdl_lib::rule::{FixCapability, RuleCategory};

mod cache;
mod file_processor;
mod formatter;
mod stdin_processor;
mod watch;

/// Rule metadata for JSON export (matches Ruff's output format)
#[derive(serde::Serialize)]
struct RuleInfo {
    /// Rule code (e.g., "MD001")
    code: String,
    /// Rule name in kebab-case (e.g., "heading-increment")
    name: String,
    /// All aliases for this rule
    aliases: Vec<String>,
    /// Short description of what the rule checks
    summary: String,
    /// Rule category (e.g., "heading", "list", "whitespace")
    category: String,
    /// Human-readable fix availability description
    fix: String,
    /// Fix availability: "Always", "Sometimes", or "None"
    fix_availability: String,
    /// URL to the rule documentation
    url: String,
    /// Full explanation/documentation for the rule (from docs/*.md)
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<String>,
}

/// Read rule documentation from the docs directory
fn read_rule_explanation(code: &str) -> Option<String> {
    // Try to find the docs file in common locations
    let code_lower = code.to_lowercase();
    let possible_paths = [format!("docs/{code_lower}.md"), format!("../docs/{code_lower}.md")];

    for path in &possible_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
    None
}

/// Build a map from canonical rule IDs to their aliases
fn build_rule_aliases_map() -> std::collections::HashMap<String, Vec<String>> {
    use rumdl_config::RULE_ALIAS_MAP;

    let mut aliases_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for (alias, canonical) in RULE_ALIAS_MAP.entries() {
        // Skip identity mappings (where alias == canonical)
        if *alias == *canonical {
            continue;
        }
        // Convert alias to kebab-case lowercase
        let alias_kebab = alias.to_lowercase();
        aliases_map.entry(canonical.to_string()).or_default().push(alias_kebab);
    }

    // Sort aliases for consistent output
    for aliases in aliases_map.values_mut() {
        aliases.sort();
    }

    aliases_map
}

/// Convert RuleCategory to a string for JSON output
fn category_to_string(category: RuleCategory) -> &'static str {
    match category {
        RuleCategory::Heading => "heading",
        RuleCategory::List => "list",
        RuleCategory::CodeBlock => "code-block",
        RuleCategory::Link => "link",
        RuleCategory::Image => "image",
        RuleCategory::Html => "html",
        RuleCategory::Emphasis => "emphasis",
        RuleCategory::Whitespace => "whitespace",
        RuleCategory::Blockquote => "blockquote",
        RuleCategory::Table => "table",
        RuleCategory::FrontMatter => "front-matter",
        RuleCategory::Other => "other",
    }
}

/// Convert FixCapability to human-readable strings
fn fix_capability_to_strings(capability: FixCapability) -> (&'static str, &'static str) {
    match capability {
        FixCapability::FullyFixable => ("Fix is always available.", "Always"),
        FixCapability::ConditionallyFixable => ("Fix is sometimes available.", "Sometimes"),
        FixCapability::Unfixable => ("Fix is not available.", "None"),
    }
}

/// Get the primary alias (kebab-case name) for a rule, and remaining aliases
fn get_primary_and_remaining_aliases(code: &str, aliases: &[String]) -> (String, Vec<String>) {
    if aliases.is_empty() {
        (code.to_lowercase(), Vec::new())
    } else {
        let primary = aliases[0].clone();
        let remaining: Vec<String> = aliases.iter().skip(1).cloned().collect();
        (primary, remaining)
    }
}

/// Apply CLI argument overrides to a sourced config.
/// This centralizes the logic for CLI args overriding config values,
/// ensuring consistency between regular check and watch mode.
pub fn apply_cli_overrides(sourced: &mut rumdl_config::SourcedConfig, args: &CheckArgs) {
    // Apply --flavor override if provided
    if let Some(ref flavor_str) = args.flavor
        && let Ok(flavor) = rumdl_config::MarkdownFlavor::from_str(flavor_str)
    {
        sourced.global.flavor = rumdl_config::SourcedValue::new(flavor, rumdl_config::ConfigSource::Cli);
    }

    // Apply --respect-gitignore override if provided
    // This allows CLI to override config file setting
    if let Some(respect_gitignore) = args.respect_gitignore {
        sourced.global.respect_gitignore =
            rumdl_config::SourcedValue::new(respect_gitignore, rumdl_config::ConfigSource::Cli);
    }
}

/// Threshold for using memory-mapped I/O (1MB)
const MMAP_THRESHOLD: u64 = 1024 * 1024;

/// Prompt user for input and read their response
/// Returns None if I/O errors occur (stdin closed, pipe broken, etc.)
fn prompt_user(prompt: &str) -> Option<String> {
    print!("{prompt}");
    if io::stdout().flush().is_err() {
        return None;
    }

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return None;
    }

    Some(answer)
}

/// Handle the schema subcommand
fn handle_schema_command(action: SchemaAction) {
    use schemars::schema_for;

    // Generate the schema
    let schema = schema_for!(rumdl_config::Config);

    // Post-process the schema to add additionalProperties for flattened rules
    // This allows [MD###] sections at the root level alongside [global] and [per-file-ignores]
    let mut schema_value: serde_json::Value = serde_json::to_value(&schema).unwrap_or_else(|e| {
        eprintln!("{}: Failed to convert schema to Value: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    if let Some(schema_obj) = schema_value.as_object_mut() {
        // Add additionalProperties that reference the RuleConfig definition
        // This allows any additional properties (rule names like MD013, MD007, etc.)
        // to be validated as RuleConfig objects
        schema_obj.insert(
            "additionalProperties".to_string(),
            serde_json::json!({
                "$ref": "#/$defs/RuleConfig"
            }),
        );
    }

    let schema_json = serde_json::to_string_pretty(&schema_value).unwrap_or_else(|e| {
        eprintln!("{}: Failed to serialize schema: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    match action {
        SchemaAction::Print => {
            // Print to stdout
            println!("{schema_json}");
        }
        SchemaAction::Generate => {
            // Find the schema file path (project root)
            let schema_path = get_project_schema_path();

            // Read existing schema if it exists
            let existing_schema = fs::read_to_string(&schema_path).ok();

            if existing_schema.as_ref() == Some(&schema_json) {
                println!("Schema is already up-to-date: {}", schema_path.display());
            } else {
                fs::write(&schema_path, &schema_json).unwrap_or_else(|e| {
                    eprintln!("{}: Failed to write schema file: {}", "Error".red().bold(), e);
                    exit::tool_error();
                });
                println!("Schema updated: {}", schema_path.display());
            }
        }
        SchemaAction::Check => {
            let schema_path = get_project_schema_path();
            let existing_schema = fs::read_to_string(&schema_path).unwrap_or_else(|_| {
                eprintln!("Error: Schema file not found: {}", schema_path.display());
                eprintln!("Run 'rumdl schema generate' to create it.");
                exit::tool_error();
            });

            if existing_schema != schema_json {
                eprintln!("Error: Schema is out of date: {}", schema_path.display());
                eprintln!("Run 'rumdl schema generate' to update it.");
                exit::tool_error();
            } else {
                println!("Schema is up-to-date: {}", schema_path.display());
            }
        }
    }
}

/// Get the path to the project's schema file
fn get_project_schema_path() -> std::path::PathBuf {
    // Try to find the project root by looking for Cargo.toml
    let mut current_dir = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("{}: Failed to get current directory: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return current_dir.join("rumdl.schema.json");
        }

        if !current_dir.pop() {
            // Reached filesystem root without finding Cargo.toml
            // Fall back to current directory
            return std::env::current_dir()
                .unwrap_or_else(|e| {
                    eprintln!("{}: Failed to get current directory: {}", "Error".red().bold(), e);
                    exit::tool_error();
                })
                .join("rumdl.schema.json");
        }
    }
}

/// Efficiently read file content using memory mapping for large files
pub fn read_file_efficiently(path: &Path) -> Result<String, Box<dyn Error>> {
    // Get file metadata first
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size > MMAP_THRESHOLD {
        // Use memory mapping for large files
        let file = fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // Convert to string - this is still a copy but more efficient for large files
        String::from_utf8(mmap.to_vec()).map_err(|e| format!("Invalid UTF-8 in file {}: {}", path.display(), e).into())
    } else {
        // Use regular reading for small files
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e).into())
    }
}

/// Utility function to load configuration with standard CLI error handling.
/// This eliminates duplication between different CLI commands that load configuration.
/// Filter a SourcedConfig to only include non-default values
fn filter_sourced_config_to_non_defaults(
    sourced: &rumdl_config::SourcedConfig<rumdl_config::ConfigLoaded>,
) -> rumdl_config::SourcedConfig<rumdl_config::ConfigLoaded> {
    let mut filtered = rumdl_config::SourcedConfig::default();

    // Filter global config - only include fields with non-default sources
    if sourced.global.enable.source != rumdl_config::ConfigSource::Default {
        filtered.global.enable = sourced.global.enable.clone();
    }
    if sourced.global.disable.source != rumdl_config::ConfigSource::Default {
        filtered.global.disable = sourced.global.disable.clone();
    }
    if sourced.global.exclude.source != rumdl_config::ConfigSource::Default {
        filtered.global.exclude = sourced.global.exclude.clone();
    }
    if sourced.global.include.source != rumdl_config::ConfigSource::Default {
        filtered.global.include = sourced.global.include.clone();
    }
    if sourced.global.respect_gitignore.source != rumdl_config::ConfigSource::Default {
        filtered.global.respect_gitignore = sourced.global.respect_gitignore.clone();
    }
    if sourced.global.line_length.source != rumdl_config::ConfigSource::Default {
        filtered.global.line_length = sourced.global.line_length.clone();
    }
    if sourced.global.flavor.source != rumdl_config::ConfigSource::Default {
        filtered.global.flavor = sourced.global.flavor.clone();
    }
    if sourced.global.force_exclude.source != rumdl_config::ConfigSource::Default {
        filtered.global.force_exclude = sourced.global.force_exclude.clone();
    }
    if sourced.global.cache.source != rumdl_config::ConfigSource::Default {
        filtered.global.cache = sourced.global.cache.clone();
    }
    if sourced.global.fixable.source != rumdl_config::ConfigSource::Default {
        filtered.global.fixable = sourced.global.fixable.clone();
    }
    if sourced.global.unfixable.source != rumdl_config::ConfigSource::Default {
        filtered.global.unfixable = sourced.global.unfixable.clone();
    }
    if let Some(ref output_format) = sourced.global.output_format
        && output_format.source != rumdl_config::ConfigSource::Default
    {
        filtered.global.output_format = Some(output_format.clone());
    }
    if let Some(ref cache_dir) = sourced.global.cache_dir
        && cache_dir.source != rumdl_config::ConfigSource::Default
    {
        filtered.global.cache_dir = Some(cache_dir.clone());
    }

    // Filter per-file ignores
    if sourced.per_file_ignores.source != rumdl_config::ConfigSource::Default {
        filtered.per_file_ignores = sourced.per_file_ignores.clone();
    }

    // Filter rules - only include rules with at least one non-default value
    for (rule_name, rule_cfg) in &sourced.rules {
        let mut filtered_rule = rumdl_config::SourcedRuleConfig::default();
        for (key, sv) in &rule_cfg.values {
            if sv.source != rumdl_config::ConfigSource::Default {
                filtered_rule.values.insert(key.clone(), sv.clone());
            }
        }
        if !filtered_rule.values.is_empty() {
            filtered.rules.insert(rule_name.clone(), filtered_rule);
        }
    }

    // Preserve metadata
    filtered.loaded_files = sourced.loaded_files.clone();
    filtered.unknown_keys = sourced.unknown_keys.clone();
    filtered.project_root = sourced.project_root.clone();

    filtered
}

fn load_config_with_cli_error_handling(config_path: Option<&str>, isolated: bool) -> rumdl_config::SourcedConfig {
    load_config_with_cli_error_handling_with_dir(config_path, isolated, None)
}

pub fn load_config_with_cli_error_handling_with_dir(
    config_path: Option<&str>,
    isolated: bool,
    discovery_dir: Option<&std::path::Path>,
) -> rumdl_config::SourcedConfig {
    let result = if let Some(dir) = discovery_dir {
        // Canonicalize config path before changing directory
        // Otherwise relative paths will be resolved from the wrong directory
        let absolute_config_path = config_path.map(|p| {
            let path = std::path::Path::new(p);
            if path.is_absolute() {
                p.to_string()
            } else if let Ok(canonical) = std::fs::canonicalize(path) {
                canonical.to_string_lossy().to_string()
            } else {
                // If file doesn't exist yet, make it absolute relative to current dir
                std::env::current_dir()
                    .map(|cwd| cwd.join(p).to_string_lossy().to_string())
                    .unwrap_or_else(|_| p.to_string())
            }
        });

        // Temporarily change working directory for config discovery
        let original_dir = std::env::current_dir().ok();

        // Change to the discovery directory if it exists
        if dir.is_dir() {
            let _ = std::env::set_current_dir(dir);
        } else if let Some(parent) = dir.parent() {
            let _ = std::env::set_current_dir(parent);
        }

        let config_result =
            rumdl_config::SourcedConfig::load_with_discovery(absolute_config_path.as_deref(), None, isolated);

        // Restore original directory
        if let Some(orig) = original_dir {
            let _ = std::env::set_current_dir(orig);
        }

        config_result
    } else {
        rumdl_config::SourcedConfig::load_with_discovery(config_path, None, isolated)
    };

    match result {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}: {}", "Config error".red().bold(), e);
            exit::tool_error();
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Control colored output: auto, always, never
    #[arg(long, global = true, default_value = "auto", value_parser = ["auto", "always", "never"], help = "Control colored output: auto, always, never")]
    color: String,

    /// Path to configuration file
    #[arg(
        long,
        global = true,
        help = "Path to configuration file",
        conflicts_with_all = ["no_config", "isolated"]
    )]
    config: Option<String>,

    /// Ignore all configuration files and use built-in defaults
    #[arg(
        long,
        global = true,
        help = "Ignore all configuration files and use built-in defaults"
    )]
    no_config: bool,

    /// Ignore all configuration files (alias for --no-config, Ruff-compatible)
    #[arg(
        long,
        global = true,
        help = "Ignore all configuration files (alias for --no-config)",
        conflicts_with = "no_config"
    )]
    isolated: bool,
}

#[derive(Subcommand)]
enum SchemaAction {
    /// Generate/update the JSON schema file
    Generate,
    /// Check if the schema is up-to-date
    Check,
    /// Print the schema to stdout
    Print,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint Markdown files and print warnings/errors
    Check(CheckArgs),
    /// Format Markdown files (alias for check --fix)
    Fmt(CheckArgs),
    /// Initialize a new configuration file
    Init {
        /// Generate configuration for pyproject.toml instead of .rumdl.toml
        #[arg(long)]
        pyproject: bool,
    },
    /// Show information about a rule or list all rules
    Rule {
        /// Rule name or ID (optional, omit to list all rules)
        rule: Option<String>,
        /// Output format: text, json, or json-lines
        #[arg(long, short = 'o', value_name = "FORMAT", default_value = "text")]
        output_format: String,
        /// Filter to only fixable rules
        #[arg(long, short = 'f')]
        fixable: bool,
        /// Filter by category (use --list-categories to see options)
        #[arg(long, short = 'c', value_name = "CATEGORY")]
        category: Option<String>,
        /// Include full documentation in output (for json/json-lines)
        #[arg(long)]
        explain: bool,
        /// List available categories and exit
        #[arg(long)]
        list_categories: bool,
    },
    /// Explain a rule with detailed information and examples
    Explain {
        /// Rule name or ID to explain
        rule: String,
    },
    /// Show configuration or query a specific key
    Config {
        #[command(subcommand)]
        subcmd: Option<ConfigSubcommand>,
        /// Show only the default configuration values
        #[arg(long, help = "Show only the default configuration values")]
        defaults: bool,
        /// Show only non-default configuration values (exclude defaults)
        #[arg(long, help = "Show only non-default configuration values (exclude defaults)")]
        no_defaults: bool,
        #[arg(long, help = "Output format (e.g. toml, json)")]
        output: Option<String>,
    },
    /// Start the Language Server Protocol server
    Server {
        /// TCP port to listen on (for debugging)
        #[arg(long)]
        port: Option<u16>,
        /// Use stdio for communication (default)
        #[arg(long)]
        stdio: bool,
        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,
        /// Path to rumdl configuration file
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Generate or check JSON schema for rumdl.toml
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },
    /// Import and convert markdownlint configuration files
    Import {
        /// Path to markdownlint config file (JSON/YAML)
        file: String,
        /// Output file path (default: .rumdl.toml)
        #[arg(short, long)]
        output: Option<String>,
        /// Output format: toml or json
        #[arg(long, default_value = "toml")]
        format: String,
        /// Show converted config without writing to file
        #[arg(long)]
        dry_run: bool,
    },
    /// Install the rumdl VS Code extension
    Vscode {
        /// Force reinstall the current version even if already installed
        #[arg(long)]
        force: bool,
        /// Update to the latest version (only if newer version available)
        #[arg(long)]
        update: bool,
        /// Show installation status without installing
        #[arg(long)]
        status: bool,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (detected from $SHELL if omitted)
        shell: Option<Shell>,
        /// List available shells
        #[arg(long, short = 'l')]
        list: bool,
    },
    /// Clear the cache
    Clean,
    /// Show version information
    Version,
}

#[derive(Subcommand, Debug)]
enum ConfigSubcommand {
    /// Query a specific config key (e.g. global.exclude or MD013.line_length)
    Get { key: String },
    /// Show the absolute path of the configuration file that was loaded
    File,
}

/// Fix mode determines exit code behavior: Check/CheckFix exit 1 on violations, Format exits 0
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FixMode {
    #[default]
    Check,
    CheckFix,
    Format,
}

/// Fail-on mode determines which severity triggers exit code 1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailOn {
    #[default]
    Any, // Exit 1 on any violation (info, warning, or error)
    Warning, // Exit 1 on warning or error severity violations
    Error,   // Exit 1 only on error-severity violations
    Never,   // Always exit 0
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Files or directories to lint (use '-' for stdin)
    #[arg(required = false)]
    paths: Vec<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    fix: bool,

    /// Show diff of what would be fixed instead of fixing files
    #[arg(long, help = "Show diff of what would be fixed instead of fixing files")]
    diff: bool,

    /// Exit with code 1 if any formatting changes would be made (like rustfmt --check)
    #[arg(long, help = "Exit with code 1 if any formatting changes would be made (for CI)")]
    check: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false")]
    list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long)]
    disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(short, long, visible_alias = "rules")]
    enable: Option<String>,

    /// Extend the list of enabled rules (additive with config)
    #[arg(long)]
    extend_enable: Option<String>,

    /// Extend the list of disabled rules (additive with config)
    #[arg(long)]
    extend_disable: Option<String>,

    /// Exclude specific files or directories (comma-separated glob patterns)
    #[arg(long)]
    exclude: Option<String>,

    /// Disable all exclude patterns (lint all files regardless of exclude configuration)
    #[arg(long, help = "Disable all exclude patterns")]
    no_exclude: bool,

    /// Include only specific files or directories (comma-separated glob patterns).
    #[arg(long)]
    include: Option<String>,

    /// Respect .gitignore files when scanning directories
    /// When not specified, uses config file value (default: true)
    #[arg(
        long,
        num_args(0..=1),
        require_equals(true),
        default_missing_value = "true",
        help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)"
    )]
    respect_gitignore: Option<bool>,

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,

    /// Show profiling information
    #[arg(long)]
    profile: bool,

    /// Show statistics summary of rule violations
    #[arg(long)]
    statistics: bool,

    /// Print diagnostics, but nothing else
    #[arg(short, long, help = "Print diagnostics, but nothing else")]
    quiet: bool,

    /// Output format: text (default) or json
    #[arg(long, short = 'o', default_value = "text")]
    output: String,

    /// Output format for linting results
    #[arg(long, value_parser = ["text", "full", "concise", "grouped", "json", "json-lines", "github", "gitlab", "pylint", "azure", "sarif", "junit"],
          help = "Output format (default: text, or $RUMDL_OUTPUT_FORMAT, or output-format in config)")]
    output_format: Option<String>,

    /// Show absolute file paths instead of project-relative paths
    #[arg(long, help = "Show absolute file paths in output instead of relative paths")]
    show_full_path: bool,

    /// Markdown flavor to use for linting
    #[arg(long, value_parser = ["standard", "mkdocs", "mdx", "quarto"],
          help = "Markdown flavor: standard (default), mkdocs, mdx, or quarto")]
    flavor: Option<String>,

    /// Read from stdin instead of files
    #[arg(long, help = "Read from stdin instead of files")]
    stdin: bool,

    /// Filename to use for stdin input (for context and error messages)
    #[arg(long, help = "Filename to use when reading from stdin (e.g., README.md)")]
    stdin_filename: Option<String>,

    /// Output linting results to stderr instead of stdout
    #[arg(long, help = "Output diagnostics to stderr instead of stdout")]
    stderr: bool,

    /// Disable all logging (but still exit with status code upon detecting diagnostics)
    #[arg(
        short,
        long,
        help = "Disable all logging (but still exit with status code upon detecting diagnostics)"
    )]
    silent: bool,

    /// Run in watch mode by re-running whenever files change
    #[arg(short, long, help = "Run in watch mode by re-running whenever files change")]
    watch: bool,

    /// Enforce exclude patterns even for paths that are passed explicitly.
    /// By default, rumdl will lint any paths passed in directly, even if they would typically be excluded.
    /// Setting this flag will cause rumdl to respect exclusions unequivocally.
    /// This is useful for pre-commit, which explicitly passes all changed files.
    #[arg(long, help = "Enforce exclude patterns even for explicitly specified files")]
    force_exclude: bool,

    /// Disable caching of lint results
    #[arg(long, help = "Disable caching (re-check all files)")]
    no_cache: bool,

    /// Directory to store cache files
    #[arg(
        long,
        help = "Directory to store cache files (default: .rumdl_cache, or $RUMDL_CACHE_DIR, or cache-dir in config)"
    )]
    cache_dir: Option<String>,

    /// Control when to exit with code 1: any (default), warning, error, or never
    #[arg(long, value_parser = ["any", "warning", "error", "never"], default_value = "any",
          help = "Exit code behavior: 'any' (default) exits 1 on any violation, 'warning' on warning+error, 'error' only on errors, 'never' always exits 0")]
    fail_on: String,

    #[arg(skip)]
    pub fix_mode: FixMode,

    #[arg(skip)]
    pub fail_on_mode: FailOn,
}

/// Offer to install the VS Code extension during init
fn offer_vscode_extension_install() {
    use rumdl_lib::vscode::VsCodeExtension;

    // Check if we're in an integrated terminal
    if let Some((cmd, editor_name)) = VsCodeExtension::current_editor_from_env() {
        println!("\nDetected you're using {}.", editor_name.green());
        println!("Would you like to install the rumdl extension? [Y/n]");

        let Some(answer) = prompt_user("> ") else {
            return; // I/O error, exit gracefully
        };

        if answer.trim().is_empty() || answer.trim().eq_ignore_ascii_case("y") {
            match VsCodeExtension::with_command(cmd) {
                Ok(vscode) => {
                    if let Err(e) = vscode.install(false) {
                        eprintln!("{}: {}", "Error".red().bold(), e);
                    }
                }
                Err(e) => {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
            }
        }
    } else {
        // Check for available editors
        let available_editors = VsCodeExtension::find_all_editors();

        match available_editors.len() {
            0 => {
                // No editors found, skip silently
            }
            1 => {
                // Single editor found
                let (cmd, editor_name) = available_editors[0];
                println!("\n{} detected.", editor_name.green());
                println!("Would you like to install the rumdl extension for real-time linting? [y/N]");

                let Some(answer) = prompt_user("> ") else {
                    return; // I/O error, exit gracefully
                };

                if answer.trim().eq_ignore_ascii_case("y") {
                    match VsCodeExtension::with_command(cmd) {
                        Ok(vscode) => {
                            if let Err(e) = vscode.install(false) {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                        Err(e) => {
                            eprintln!("{}: {}", "Error".red().bold(), e);
                        }
                    }
                }
            }
            _ => {
                // Multiple editors found
                println!("\nMultiple VS Code-compatible editors found:");
                for (i, (_, editor_name)) in available_editors.iter().enumerate() {
                    println!("  {}. {}", i + 1, editor_name);
                }
                println!(
                    "\nInstall the rumdl extension? [1-{}/a=all/n=none]:",
                    available_editors.len()
                );

                let Some(response) = prompt_user("> ") else {
                    return; // I/O error, exit gracefully
                };
                let answer = response.trim().to_lowercase();

                if answer == "a" || answer == "all" {
                    // Install in all editors
                    for (cmd, editor_name) in &available_editors {
                        println!("\nInstalling for {editor_name}...");
                        match VsCodeExtension::with_command(cmd) {
                            Ok(vscode) => {
                                if let Err(e) = vscode.install(false) {
                                    eprintln!("{}: {}", "Error".red().bold(), e);
                                }
                            }
                            Err(e) => {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                    }
                } else if let Ok(num) = answer.parse::<usize>()
                    && num > 0
                    && num <= available_editors.len()
                {
                    let (cmd, editor_name) = available_editors[num - 1];
                    println!("\nInstalling for {editor_name}...");
                    match VsCodeExtension::with_command(cmd) {
                        Ok(vscode) => {
                            if let Err(e) = vscode.install(false) {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                        Err(e) => {
                            eprintln!("{}: {}", "Error".red().bold(), e);
                        }
                    }
                }
            }
        }
    }

    println!("\nSetup complete! You can now:");
    println!("  • Run {} to lint your Markdown files", "rumdl check .".cyan());
    println!("  • Open your editor to see real-time linting");
}

/// Calculate total size and count of files in a directory recursively
fn calculate_directory_stats(path: &Path) -> io::Result<(u64, usize)> {
    let mut total_size = 0u64;
    let mut file_count = 0usize;

    fn visit_dir(path: &Path, total_size: &mut u64, file_count: &mut usize) -> io::Result<()> {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, total_size, file_count)?;
                } else if let Ok(metadata) = entry.metadata() {
                    *total_size += metadata.len();
                    *file_count += 1;
                }
            }
        }
        Ok(())
    }

    visit_dir(path, &mut total_size, &mut file_count)?;
    Ok((total_size, file_count))
}

/// Format bytes into human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Resolve cache directory with same logic as check command
fn resolve_cache_directory(cli: &Cli) -> std::path::PathBuf {
    // Load config to get cache_dir setting
    let sourced = match rumdl_config::SourcedConfig::load_with_discovery(
        cli.config.as_deref(),
        None,
        cli.no_config || cli.isolated,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {}", "Config error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Get cache_dir from config
    let cache_dir_from_config = sourced
        .global
        .cache_dir
        .as_ref()
        .map(|sv| std::path::PathBuf::from(&sv.value));

    let project_root = sourced.project_root.clone();

    // Resolve cache directory with precedence: env var → config → default
    let mut cache_dir = std::env::var("RUMDL_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or(cache_dir_from_config)
        .unwrap_or_else(|| std::path::PathBuf::from(".rumdl_cache"));

    // If cache_dir is relative and we have a project root, resolve relative to project root
    if cache_dir.is_relative()
        && let Some(root) = project_root
    {
        cache_dir = root.join(&cache_dir);
    }

    cache_dir
}

/// Handle the completions command
fn handle_completions_command(shell: Option<Shell>, list: bool) {
    const AVAILABLE_SHELLS: &[(&str, &str)] = &[
        ("bash", "Bourne Again SHell"),
        ("zsh", "Z shell"),
        ("fish", "Friendly Interactive SHell"),
        ("powershell", "PowerShell"),
        ("elvish", "Elvish shell"),
    ];

    if list {
        println!("Available shells:");
        for (name, description) in AVAILABLE_SHELLS {
            println!("  {name:<12} {description}");
        }
        return;
    }

    let shell = match shell {
        Some(s) => s,
        None => detect_shell_from_env().unwrap_or_else(|| {
            eprintln!(
                "{}: Could not detect shell from $SHELL environment variable",
                "Error".red().bold()
            );
            eprintln!();
            eprintln!("Please specify a shell explicitly:");
            eprintln!("  rumdl completions bash");
            eprintln!("  rumdl completions zsh");
            eprintln!("  rumdl completions fish");
            eprintln!("  rumdl completions powershell");
            eprintln!("  rumdl completions elvish");
            eprintln!();
            eprintln!("Or use --list to see all available shells");
            exit::tool_error();
        }),
    };

    generate(shell, &mut Cli::command(), "rumdl", &mut stdout());
}

fn detect_shell_from_env() -> Option<Shell> {
    let shell_path = std::env::var("SHELL").ok()?;
    let shell_name = std::path::Path::new(&shell_path).file_name()?.to_str()?;

    match shell_name {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        "elvish" => Some(Shell::Elvish),
        _ => None,
    }
}

/// Handle the clean command
fn handle_clean_command(cli: &Cli) {
    let cache_dir = resolve_cache_directory(cli);

    // Check if cache directory exists
    if !cache_dir.exists() {
        println!(
            "{} {} ({})",
            "No cache found at".yellow().bold(),
            cache_dir.display(),
            "nothing to clean".dimmed()
        );
        return;
    }

    // Calculate cache stats before deletion
    match calculate_directory_stats(&cache_dir) {
        Ok((size, file_count)) => {
            if size == 0 && file_count == 0 {
                println!(
                    "{} {} ({})",
                    "Cache is empty at".yellow().bold(),
                    cache_dir.display(),
                    "nothing to clean".dimmed()
                );
                // Still remove the directory structure
                let cache_instance = cache::LintCache::new(cache_dir.clone(), true);
                let _ = cache_instance.clear();
                return;
            }

            // Create cache instance and clear
            let cache_instance = cache::LintCache::new(cache_dir.clone(), true);

            match cache_instance.clear() {
                Ok(_) => {
                    println!("{} {}", "Cleared cache:".green().bold(), cache_dir.display());
                    println!(
                        "  {} {} {} {}",
                        "Removed".dimmed(),
                        format_size(size).cyan(),
                        "across".dimmed(),
                        format!("{file_count} files").cyan()
                    );
                }
                Err(e) => {
                    eprintln!("{}: {}", "Error clearing cache".red().bold(), e);
                    eprintln!("  Cache location: {}", cache_dir.display());
                    exit::tool_error();
                }
            }
        }
        Err(e) => {
            eprintln!("{}: {}", "Error reading cache directory".red().bold(), e);
            eprintln!("  Cache location: {}", cache_dir.display());
            exit::tool_error();
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Reset SIGPIPE to default behavior on Unix so piping to `head` etc. works correctly.
    // Without this, Rust ignores SIGPIPE and `println!` panics on broken pipe.
    #[cfg(unix)]
    {
        // SAFETY: Setting SIGPIPE to SIG_DFL is standard practice for CLI tools
        // that produce output meant to be piped. This is safe and idiomatic.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }

    // Initialize logging from RUST_LOG environment variable
    // This allows users to debug config discovery with: RUST_LOG=debug rumdl check ...
    env_logger::Builder::from_default_env()
        .format_timestamp(None)
        .format_target(false)
        .init();

    let cli = Cli::parse();

    // Set color override globally based on --color flag
    match cli.color.as_str() {
        "always" => colored::control::set_override(true),
        "never" => colored::control::set_override(false),
        "auto" => colored::control::unset_override(),
        _ => colored::control::unset_override(),
    }

    // Catch panics and print a message, exit 1
    let result = std::panic::catch_unwind(|| {
        match cli.command {
            Commands::Init { pyproject } => {
                if pyproject {
                    // Handle pyproject.toml initialization
                    let config_content = rumdl_config::generate_pyproject_config();

                    if Path::new("pyproject.toml").exists() {
                        // pyproject.toml exists, ask to append
                        println!("pyproject.toml already exists. Would you like to append rumdl configuration? [y/N]");

                        let Some(answer) = prompt_user("> ") else {
                            eprintln!("Error: Failed to read user input");
                            exit::tool_error();
                        };

                        if answer.trim().eq_ignore_ascii_case("y") {
                            // Append to existing file
                            match fs::read_to_string("pyproject.toml") {
                                Ok(content) => {
                                    // Check if [tool.rumdl] section already exists
                                    if content.contains("[tool.rumdl]") {
                                        println!("The pyproject.toml file already contains a [tool.rumdl] section.");
                                        println!(
                                            "Please edit the file manually to avoid overwriting existing configuration."
                                        );
                                        return;
                                    }

                                    // Append with a blank line for separation
                                    let new_content = format!("{}\n\n{}", content.trim_end(), config_content);
                                    match fs::write("pyproject.toml", new_content) {
                                        Ok(_) => {
                                            println!("Added rumdl configuration to pyproject.toml")
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "{}: Failed to update pyproject.toml: {}",
                                                "Error".red().bold(),
                                                e
                                            );
                                            exit::tool_error();
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{}: Failed to read pyproject.toml: {}", "Error".red().bold(), e);
                                    exit::tool_error();
                                }
                            }
                        } else {
                            println!("Aborted. No changes made to pyproject.toml");
                        }
                    } else {
                        // Create new pyproject.toml with basic structure
                        let basic_content = r#"[build-system]
requires = ["setuptools>=42", "wheel"]
build-backend = "setuptools.build_meta"

"#;
                        let content = basic_content.to_owned() + &config_content;

                        match fs::write("pyproject.toml", content) {
                            Ok(_) => {
                                println!("Created pyproject.toml with rumdl configuration");
                            }
                            Err(e) => {
                                eprintln!("{}: Failed to create pyproject.toml: {}", "Error".red().bold(), e);
                                exit::tool_error();
                            }
                        }
                    }
                } else {
                    // Create default .rumdl.toml config file
                    match rumdl_config::create_default_config(".rumdl.toml") {
                        Ok(_) => {
                            println!("Created default configuration file: .rumdl.toml");

                            // Offer to install VS Code extension
                            offer_vscode_extension_install();
                        }
                        Err(e) => {
                            eprintln!("{}: Failed to create config file: {}", "Error".red().bold(), e);
                            exit::tool_error();
                        }
                    }
                }
            }
            Commands::Check(mut args) => {
                args.fix_mode = if args.fix { FixMode::CheckFix } else { FixMode::Check };
                args.fail_on_mode = match args.fail_on.as_str() {
                    "error" => FailOn::Error,
                    "warning" => FailOn::Warning,
                    "never" => FailOn::Never,
                    _ => FailOn::Any,
                };

                if cli.no_config || cli.isolated {
                    run_check(&args, None, cli.no_config || cli.isolated);
                } else {
                    run_check(&args, cli.config.as_deref(), cli.no_config || cli.isolated);
                }
            }
            Commands::Fmt(mut args) => {
                args.fix_mode = FixMode::Format;
                args.fail_on_mode = match args.fail_on.as_str() {
                    "error" => FailOn::Error,
                    "warning" => FailOn::Warning,
                    "never" => FailOn::Never,
                    _ => FailOn::Any,
                };

                // --check mode enables diff (don't write files) and will exit 1 if changes needed
                if args.check {
                    args.diff = true;
                }

                if cli.no_config || cli.isolated {
                    run_check(&args, None, cli.no_config || cli.isolated);
                } else {
                    run_check(&args, cli.config.as_deref(), cli.no_config || cli.isolated);
                }
            }
            Commands::Rule {
                rule,
                output_format,
                fixable,
                category,
                explain,
                list_categories,
            } => {
                // Use the canonical all_rules function to avoid drift between CLI and library
                let default_config = rumdl_lib::config::Config::default();
                let all_rules = rumdl_lib::rules::all_rules(&default_config);

                // Collect all unique categories
                let mut categories: Vec<String> = all_rules
                    .iter()
                    .map(|r| category_to_string(r.category()).to_string())
                    .collect();
                categories.sort();
                categories.dedup();

                // Handle --list-categories
                if list_categories {
                    println!("Available categories:");
                    for cat in &categories {
                        let count = all_rules
                            .iter()
                            .filter(|r| category_to_string(r.category()) == cat)
                            .count();
                        println!("  {cat} ({count} rules)");
                    }
                    return;
                }

                // Validate category if provided
                if let Some(ref cat_filter) = category {
                    let cat_filter_lower = cat_filter.to_lowercase();
                    if !categories.iter().any(|c| c.to_lowercase() == cat_filter_lower) {
                        eprintln!("Invalid category: '{cat_filter}'");
                        eprintln!("Valid categories: {}", categories.join(", "));
                        exit::tool_error();
                    }
                }

                let aliases_map = build_rule_aliases_map();

                // Helper to build RuleInfo from a rule
                let build_rule_info = |r: &dyn Rule, include_explanation: bool| -> RuleInfo {
                    let code = r.name().to_string();
                    let all_aliases = aliases_map.get(&code).cloned().unwrap_or_default();
                    let (primary_name, remaining_aliases) = get_primary_and_remaining_aliases(&code, &all_aliases);
                    let (fix_desc, fix_avail) = fix_capability_to_strings(r.fix_capability());
                    let explanation = if include_explanation {
                        read_rule_explanation(&code)
                    } else {
                        None
                    };
                    RuleInfo {
                        name: primary_name,
                        aliases: remaining_aliases,
                        code: code.clone(),
                        summary: r.description().to_string(),
                        category: category_to_string(r.category()).to_string(),
                        fix: fix_desc.to_string(),
                        fix_availability: fix_avail.to_string(),
                        url: format!("https://rumdl.dev/{}/", code.to_lowercase()),
                        explanation,
                    }
                };

                // Build RuleInfo for all or a specific rule
                let mut rule_infos: Vec<RuleInfo> = if let Some(rule_query) = &rule {
                    let rule_query_upper = rule_query.to_ascii_uppercase();
                    let found = all_rules.iter().find(|r| {
                        r.name().eq_ignore_ascii_case(&rule_query_upper)
                            || r.name().replace("MD", "") == rule_query_upper.replace("MD", "")
                    });
                    if let Some(r) = found {
                        vec![build_rule_info(r.as_ref(), explain)]
                    } else {
                        eprintln!("Rule '{rule_query}' not found.");
                        exit::tool_error();
                    }
                } else {
                    all_rules.iter().map(|r| build_rule_info(r.as_ref(), explain)).collect()
                };

                // Apply filters
                if fixable {
                    rule_infos.retain(|info| info.fix_availability != "None");
                }
                if let Some(ref cat_filter) = category {
                    let cat_filter_lower = cat_filter.to_lowercase();
                    rule_infos.retain(|info| info.category.to_lowercase() == cat_filter_lower);
                }

                // Check if no rules match filters
                if rule_infos.is_empty() && rule.is_none() {
                    let mut filter_desc = Vec::new();
                    if fixable {
                        filter_desc.push("fixable".to_string());
                    }
                    if let Some(ref cat) = category {
                        filter_desc.push(format!("category={cat}"));
                    }
                    eprintln!("No rules match the specified filters: {}", filter_desc.join(", "));
                    eprintln!("Try: rumdl rule --list-categories");
                    exit::tool_error();
                }

                // Output based on format
                match output_format.to_lowercase().as_str() {
                    "json" => {
                        // For single rule query, output the object directly; for all rules, output array
                        let json = if rule.is_some() && rule_infos.len() == 1 {
                            serde_json::to_string_pretty(&rule_infos[0])
                        } else {
                            serde_json::to_string_pretty(&rule_infos)
                        };
                        match json {
                            Ok(output) => println!("{output}"),
                            Err(e) => {
                                eprintln!("Error serializing to JSON: {e}");
                                exit::tool_error();
                            }
                        }
                    }
                    "json-lines" | "jsonl" => {
                        // Output one JSON object per line (newline-delimited JSON)
                        for info in &rule_infos {
                            match serde_json::to_string(info) {
                                Ok(line) => println!("{line}"),
                                Err(e) => {
                                    eprintln!("Error serializing to JSON: {e}");
                                    exit::tool_error();
                                }
                            }
                        }
                    }
                    _ => {
                        if rule.is_some() {
                            if let Some(info) = rule_infos.first() {
                                println!("{} - {}", info.code, info.summary);
                                println!();
                                println!("Name: {}", info.name);
                                if !info.aliases.is_empty() {
                                    println!("Aliases: {}", info.aliases.join(", "));
                                }
                                println!("Category: {}", info.category);
                                println!("Fix: {}", info.fix);
                                println!("Documentation: {}", info.url);
                                if let Some(ref explanation) = info.explanation {
                                    println!();
                                    println!("{explanation}");
                                }
                            }
                        } else {
                            // Show summary with optional filter info
                            let filter_info = if fixable || category.is_some() {
                                let mut parts = Vec::new();
                                if fixable {
                                    parts.push("fixable".to_string());
                                }
                                if let Some(ref cat) = category {
                                    parts.push(format!("category={cat}"));
                                }
                                format!(" ({})", parts.join(", "))
                            } else {
                                String::new()
                            };
                            println!("Available rules{filter_info}:");
                            for info in &rule_infos {
                                println!("  {} - {}", info.code, info.summary);
                            }
                            println!();
                            println!("Total: {} rules", rule_infos.len());
                        }
                    }
                }
            }
            Commands::Explain { rule } => {
                handle_explain_command(&rule);
            }
            Commands::Config {
                subcmd,
                defaults,
                no_defaults,
                output,
            } => {
                // Validate mutual exclusivity of --defaults and --no-defaults
                if defaults && no_defaults {
                    eprintln!(
                        "{}: Cannot use both --defaults and --no-defaults flags together",
                        "Error".red().bold()
                    );
                    exit::tool_error();
                }

                // Handle config subcommands
                if let Some(ConfigSubcommand::Get { key }) = subcmd {
                    if let Some((section_part, field_part)) = key.split_once('.') {
                        // 1. Load the full SourcedConfig once
                        let sourced = match rumdl_config::SourcedConfig::load_with_discovery(
                            cli.config.as_deref(),
                            None,
                            cli.no_config,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("{}: {}", "Config error".red().bold(), e);
                                exit::tool_error();
                            }
                        };
                        // 2. Convert to final Config once (config-get doesn't need validation warnings)
                        let final_config: rumdl_config::Config = sourced.clone().into_validated_unchecked().into();

                        let normalized_field = normalize_key(field_part);

                        // Handle GLOBAL keys
                        if section_part.eq_ignore_ascii_case("global") {
                            let maybe_value_source: Option<(toml::Value, ConfigSource)> =
                                match normalized_field.as_str() {
                                    "enable" => Some((
                                        toml::Value::Array(
                                            final_config
                                                .global
                                                .enable
                                                .iter()
                                                .map(|s| toml::Value::String(s.clone()))
                                                .collect(),
                                        ),
                                        sourced.global.enable.source,
                                    )),
                                    "disable" => Some((
                                        toml::Value::Array(
                                            final_config
                                                .global
                                                .disable
                                                .iter()
                                                .map(|s| toml::Value::String(s.clone()))
                                                .collect(),
                                        ),
                                        sourced.global.disable.source,
                                    )),
                                    "exclude" => Some((
                                        toml::Value::Array(
                                            final_config
                                                .global
                                                .exclude
                                                .iter()
                                                .map(|s| toml::Value::String(s.clone()))
                                                .collect(),
                                        ),
                                        sourced.global.exclude.source,
                                    )),
                                    "include" => Some((
                                        toml::Value::Array(
                                            final_config
                                                .global
                                                .include
                                                .iter()
                                                .map(|s| toml::Value::String(s.clone()))
                                                .collect(),
                                        ),
                                        sourced.global.include.source,
                                    )),
                                    "respect-gitignore" => Some((
                                        toml::Value::Boolean(final_config.global.respect_gitignore),
                                        sourced.global.respect_gitignore.source,
                                    )),
                                    "output-format" | "output_format" => {
                                        if let Some(ref output_format) = final_config.global.output_format {
                                            Some((
                                                toml::Value::String(output_format.clone()),
                                                sourced
                                                    .global
                                                    .output_format
                                                    .as_ref()
                                                    .map(|v| v.source)
                                                    .unwrap_or(ConfigSource::Default),
                                            ))
                                        } else {
                                            None
                                        }
                                    }
                                    "flavor" => Some((
                                        toml::Value::String(format!("{:?}", final_config.global.flavor).to_lowercase()),
                                        sourced.global.flavor.source,
                                    )),
                                    _ => None,
                                };

                            if let Some((value, source)) = maybe_value_source {
                                println!(
                                    "{} = {} [from {}]",
                                    key,
                                    formatter::format_toml_value(&value),
                                    formatter::format_provenance(source)
                                );
                                // Successfully handled 'get', exit the command processing
                            } else {
                                eprintln!("Unknown global key: {field_part}");
                                exit::tool_error();
                            }
                        }
                        // Handle RULE keys (MDxxx.field)
                        else {
                            let normalized_rule_name = normalize_key(section_part);

                            // Try to get the value from the final config first
                            let final_value: Option<&toml::Value> = final_config
                                .rules
                                .get(&normalized_rule_name)
                                .and_then(|rule_cfg| rule_cfg.values.get(&normalized_field));

                            if let Some(value) = final_value {
                                let provenance = sourced
                                    .rules
                                    .get(&normalized_rule_name)
                                    .and_then(|sc| sc.values.get(&normalized_field))
                                    .map_or(ConfigSource::Default, |sv| sv.source);

                                println!(
                                    "{}.{} = {} [from {}]",
                                    normalized_rule_name,
                                    normalized_field,
                                    formatter::format_toml_value(value),
                                    formatter::format_provenance(provenance)
                                );
                                // Successfully handled 'get', exit the command processing
                            } else {
                                let all_rules = rumdl_lib::rules::all_rules(&rumdl_config::Config::default());
                                if let Some(rule) = all_rules.iter().find(|r| r.name() == section_part)
                                    && let Some((_, toml::Value::Table(table))) = rule.default_config_section()
                                    && let Some(v) = table.get(&normalized_field)
                                {
                                    let value_str = formatter::format_toml_value(v);
                                    println!("{normalized_rule_name}.{normalized_field} = {value_str} [from default]");
                                    // Successfully handled 'get', exit the command processing
                                    return;
                                }
                                eprintln!("Unknown config key: {normalized_rule_name}.{normalized_field}");
                                exit::tool_error();
                            }
                        }
                    } else {
                        eprintln!("Key must be in the form global.key or MDxxx.key");
                        exit::tool_error();
                    }
                }
                // Handle 'config file' subcommand for showing config file path
                else if let Some(ConfigSubcommand::File) = subcmd {
                    let sourced =
                        load_config_with_cli_error_handling(cli.config.as_deref(), cli.no_config || cli.isolated);

                    if sourced.loaded_files.is_empty() {
                        if cli.no_config || cli.isolated {
                            println!("No configuration file loaded (--no-config/--isolated specified)");
                        } else {
                            println!("No configuration file found (using defaults)");
                        }
                    } else {
                        // Convert relative paths to absolute paths
                        for file_path in &sourced.loaded_files {
                            match std::fs::canonicalize(file_path) {
                                Ok(absolute_path) => {
                                    println!("{}", absolute_path.display());
                                }
                                Err(_) => {
                                    // If canonicalize fails, it might be a file that doesn't exist anymore
                                    // or a relative path that can't be resolved. Just print as-is.
                                    println!("{file_path}");
                                }
                            }
                        }
                    }
                }
                // --- Fallthrough logic for `rumdl config` (no subcommand) ---
                // This code now runs ONLY if `subcmd` is None
                else {
                    // --- CONFIG VALIDATION --- (Duplicated from original position, needs to run for display)
                    let all_rules_reg = rumdl_lib::rules::all_rules(&rumdl_config::Config::default()); // Rename to avoid conflict
                    let registry_reg = rumdl_config::RuleRegistry::from_rules(&all_rules_reg);
                    let sourced_reg = if defaults {
                        // For defaults, create a SourcedConfig that includes all rule defaults
                        let mut default_sourced = rumdl_config::SourcedConfig::default();

                        // Add default configurations from all rules
                        for rule in &all_rules_reg {
                            if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section() {
                                let mut rule_config = rumdl_config::SourcedRuleConfig::default();
                                for (key, value) in table {
                                    rule_config.values.insert(
                                        key.clone(),
                                        rumdl_config::SourcedValue::new(
                                            value.clone(),
                                            rumdl_config::ConfigSource::Default,
                                        ),
                                    );
                                }
                                default_sourced.rules.insert(rule_name.to_uppercase(), rule_config);
                            }
                        }

                        default_sourced
                    } else {
                        load_config_with_cli_error_handling(cli.config.as_deref(), cli.no_config || cli.isolated)
                    };
                    let validation_warnings = rumdl_config::validate_config_sourced(&sourced_reg, &registry_reg);
                    if !validation_warnings.is_empty() {
                        for warn in &validation_warnings {
                            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
                        }
                        // Optionally: exit with error if strict mode is enabled
                        // std::process::exit(2);
                    }
                    // --- END CONFIG VALIDATION ---

                    // Decide which config to print based on --defaults and --no-defaults
                    let final_sourced_to_print = sourced_reg;

                    // Handle output format (toml, json, or smart output)
                    match output.as_deref() {
                        Some("toml") => {
                            if defaults {
                                // For defaults with TOML output, generate a complete default config
                                let mut default_config = rumdl_config::Config::default();

                                // Add all rule default configurations
                                for rule in &all_rules_reg {
                                    if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section()
                                    {
                                        let rule_config = rumdl_config::RuleConfig {
                                            severity: None,
                                            values: table.into_iter().collect(),
                                        };
                                        default_config.rules.insert(rule_name.to_uppercase(), rule_config);
                                    }
                                }

                                match toml::to_string_pretty(&default_config) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to TOML: {e}");
                                        exit::tool_error();
                                    }
                                }
                            } else if no_defaults {
                                // For --no-defaults with TOML output, filter to non-defaults
                                let filtered_sourced = filter_sourced_config_to_non_defaults(&final_sourced_to_print);
                                let config_to_print: rumdl_config::Config =
                                    filtered_sourced.into_validated_unchecked().into();
                                match toml::to_string_pretty(&config_to_print) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to TOML: {e}");
                                        exit::tool_error();
                                    }
                                }
                            } else {
                                let config_to_print: rumdl_config::Config =
                                    final_sourced_to_print.into_validated_unchecked().into();
                                match toml::to_string_pretty(&config_to_print) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to TOML: {e}");
                                        exit::tool_error();
                                    }
                                }
                            }
                        }
                        Some("json") => {
                            if defaults {
                                // For defaults with JSON output, generate a complete default config
                                let mut default_config = rumdl_config::Config::default();

                                // Add all rule default configurations
                                for rule in &all_rules_reg {
                                    if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section()
                                    {
                                        let rule_config = rumdl_config::RuleConfig {
                                            severity: None,
                                            values: table.into_iter().collect(),
                                        };
                                        default_config.rules.insert(rule_name.to_uppercase(), rule_config);
                                    }
                                }

                                match serde_json::to_string_pretty(&default_config) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to JSON: {e}");
                                        exit::tool_error();
                                    }
                                }
                            } else if no_defaults {
                                // For --no-defaults with JSON output, filter to non-defaults
                                let filtered_sourced = filter_sourced_config_to_non_defaults(&final_sourced_to_print);
                                let config_to_print: rumdl_config::Config =
                                    filtered_sourced.into_validated_unchecked().into();
                                match serde_json::to_string_pretty(&config_to_print) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to JSON: {e}");
                                        exit::tool_error();
                                    }
                                }
                            } else {
                                let config_to_print: rumdl_config::Config =
                                    final_sourced_to_print.into_validated_unchecked().into();
                                match serde_json::to_string_pretty(&config_to_print) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("Failed to serialize config to JSON: {e}");
                                        exit::tool_error();
                                    }
                                }
                            }
                        }
                        _ => {
                            // Otherwise, print the smart output with provenance annotations
                            if no_defaults {
                                formatter::print_config_with_provenance_no_defaults(
                                    &final_sourced_to_print,
                                    &all_rules_reg,
                                );
                            } else {
                                formatter::print_config_with_provenance(&final_sourced_to_print, &all_rules_reg);
                            }
                        }
                    }
                }
            }
            Commands::Schema { action } => {
                handle_schema_command(action);
            }
            Commands::Server {
                port,
                stdio,
                verbose,
                config,
            } => {
                // If verbose flag is set, increase log level to Debug
                // (logging is already initialized in main() via RUST_LOG)
                if verbose {
                    log::set_max_level(log::LevelFilter::Debug);
                }

                // Validate config file exists if provided
                if let Some(config_path) = &config
                    && !std::path::Path::new(config_path).exists()
                {
                    eprintln!(
                        "{}: Configuration file not found: {}",
                        "Error".red().bold(),
                        config_path
                    );
                    exit::tool_error();
                }

                // Start the LSP server
                let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|e| {
                    eprintln!("{}: Failed to create Tokio runtime: {}", "Error".red().bold(), e);
                    exit::tool_error();
                });

                runtime.block_on(async {
                    if let Some(port) = port {
                        // TCP mode for debugging
                        if let Err(e) = rumdl_lib::lsp::start_tcp_server(port, config.as_deref()).await {
                            eprintln!("Failed to start LSP server on port {port}: {e}");
                            exit::tool_error();
                        }
                    } else {
                        // Standard LSP mode over stdio (default behavior)
                        // Note: stdio flag is for explicit documentation, behavior is the same
                        let _ = stdio; // Suppress unused variable warning
                        if let Err(e) = rumdl_lib::lsp::start_server(config.as_deref()).await {
                            eprintln!("Failed to start LSP server: {e}");
                            exit::tool_error();
                        }
                    }
                });
            }
            Commands::Import {
                file,
                output,
                format,
                dry_run,
            } => {
                use rumdl_lib::markdownlint_config;

                // Load the markdownlint config file
                let ml_config = match markdownlint_config::load_markdownlint_config(&file) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("{}: {}", "Import error".red().bold(), e);
                        exit::tool_error();
                    }
                };

                // Convert to rumdl config format
                let fragment = ml_config.map_to_sourced_rumdl_config_fragment(Some(&file));

                // Determine if we're outputting to pyproject.toml
                let is_pyproject = output
                    .as_ref()
                    .is_some_and(|p| p.ends_with("pyproject.toml") || p == "pyproject.toml");

                // Generate the output
                let output_content = match format.as_str() {
                    "toml" => {
                        // Convert to TOML format
                        let mut output = String::new();

                        // For pyproject.toml, wrap everything in [tool.rumdl]
                        let section_prefix = if is_pyproject { "tool.rumdl." } else { "" };

                        // Add global settings if any
                        if !fragment.global.enable.value.is_empty()
                            || !fragment.global.disable.value.is_empty()
                            || !fragment.global.exclude.value.is_empty()
                            || !fragment.global.include.value.is_empty()
                            || fragment.global.line_length.value.get() != 80
                        {
                            output.push_str(&format!("[{section_prefix}global]\n"));
                            if !fragment.global.enable.value.is_empty() {
                                output.push_str(&format!("enable = {:?}\n", fragment.global.enable.value));
                            }
                            if !fragment.global.disable.value.is_empty() {
                                output.push_str(&format!("disable = {:?}\n", fragment.global.disable.value));
                            }
                            if !fragment.global.exclude.value.is_empty() {
                                output.push_str(&format!("exclude = {:?}\n", fragment.global.exclude.value));
                            }
                            if !fragment.global.include.value.is_empty() {
                                output.push_str(&format!("include = {:?}\n", fragment.global.include.value));
                            }
                            if fragment.global.line_length.value.get() != 80 {
                                output
                                    .push_str(&format!("line_length = {}\n", fragment.global.line_length.value.get()));
                            }
                            output.push('\n');
                        }

                        // Add rule-specific settings
                        for (rule_name, rule_config) in &fragment.rules {
                            if !rule_config.values.is_empty() {
                                output.push_str(&format!("[{section_prefix}{rule_name}]\n"));
                                for (key, sourced_value) in &rule_config.values {
                                    // Skip the generic "value" key if we have more specific keys
                                    if key == "value" && rule_config.values.len() > 1 {
                                        continue;
                                    }

                                    match &sourced_value.value {
                                        toml::Value::String(s) => output.push_str(&format!("{key} = \"{s}\"\n")),
                                        toml::Value::Integer(i) => output.push_str(&format!("{key} = {i}\n")),
                                        toml::Value::Float(f) => output.push_str(&format!("{key} = {f}\n")),
                                        toml::Value::Boolean(b) => output.push_str(&format!("{key} = {b}\n")),
                                        toml::Value::Array(arr) => {
                                            // Format arrays properly for TOML
                                            let arr_str = arr
                                                .iter()
                                                .map(|v| match v {
                                                    toml::Value::String(s) => format!("\"{s}\""),
                                                    _ => format!("{v}"),
                                                })
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            output.push_str(&format!("{key} = [{arr_str}]\n"));
                                        }
                                        _ => {
                                            // Use proper TOML serialization for complex values
                                            if let Ok(toml_str) = toml::to_string_pretty(&sourced_value.value) {
                                                // Remove the table wrapper if it's just a value
                                                let clean_value = toml_str.trim();
                                                if !clean_value.starts_with('[') {
                                                    output.push_str(&format!("{key} = {clean_value}"));
                                                } else {
                                                    output.push_str(&format!("{} = {:?}\n", key, sourced_value.value));
                                                }
                                            } else {
                                                output.push_str(&format!("{} = {:?}\n", key, sourced_value.value));
                                            }
                                        }
                                    }
                                }
                                output.push('\n');
                            }
                        }
                        output
                    }
                    "json" => {
                        // Convert to JSON format (similar to pyproject.toml structure)
                        let mut json_config = serde_json::Map::new();

                        // Add global settings
                        if !fragment.global.enable.value.is_empty()
                            || !fragment.global.disable.value.is_empty()
                            || !fragment.global.exclude.value.is_empty()
                            || !fragment.global.include.value.is_empty()
                            || fragment.global.line_length.value.get() != 80
                        {
                            let mut global = serde_json::Map::new();
                            if !fragment.global.enable.value.is_empty() {
                                global.insert(
                                    "enable".to_string(),
                                    serde_json::Value::Array(
                                        fragment
                                            .global
                                            .enable
                                            .value
                                            .iter()
                                            .map(|s| serde_json::Value::String(s.clone()))
                                            .collect(),
                                    ),
                                );
                            }
                            if !fragment.global.disable.value.is_empty() {
                                global.insert(
                                    "disable".to_string(),
                                    serde_json::Value::Array(
                                        fragment
                                            .global
                                            .disable
                                            .value
                                            .iter()
                                            .map(|s| serde_json::Value::String(s.clone()))
                                            .collect(),
                                    ),
                                );
                            }
                            if !fragment.global.exclude.value.is_empty() {
                                global.insert(
                                    "exclude".to_string(),
                                    serde_json::Value::Array(
                                        fragment
                                            .global
                                            .exclude
                                            .value
                                            .iter()
                                            .map(|s| serde_json::Value::String(s.clone()))
                                            .collect(),
                                    ),
                                );
                            }
                            if !fragment.global.include.value.is_empty() {
                                global.insert(
                                    "include".to_string(),
                                    serde_json::Value::Array(
                                        fragment
                                            .global
                                            .include
                                            .value
                                            .iter()
                                            .map(|s| serde_json::Value::String(s.clone()))
                                            .collect(),
                                    ),
                                );
                            }
                            if fragment.global.line_length.value.get() != 80 {
                                global.insert(
                                    "line_length".to_string(),
                                    serde_json::Value::Number(serde_json::Number::from(
                                        fragment.global.line_length.value.get(),
                                    )),
                                );
                            }
                            json_config.insert("global".to_string(), serde_json::Value::Object(global));
                        }

                        // Add rule-specific settings
                        for (rule_name, rule_config) in &fragment.rules {
                            if !rule_config.values.is_empty() {
                                let mut rule_obj = serde_json::Map::new();
                                for (key, sourced_value) in &rule_config.values {
                                    if let Ok(json_value) = serde_json::to_value(&sourced_value.value) {
                                        rule_obj.insert(key.clone(), json_value);
                                    }
                                }
                                json_config.insert(rule_name.clone(), serde_json::Value::Object(rule_obj));
                            }
                        }

                        serde_json::to_string_pretty(&json_config).unwrap_or_else(|e| {
                            eprintln!("{}: Failed to serialize to JSON: {}", "Error".red().bold(), e);
                            exit::tool_error();
                        })
                    }
                    _ => {
                        eprintln!(
                            "{}: Unsupported format '{}'. Use 'toml' or 'json'.",
                            "Error".red().bold(),
                            format
                        );
                        exit::tool_error();
                    }
                };

                if dry_run {
                    // Just print the converted config
                    println!("{output_content}");
                } else {
                    // Write to output file
                    let output_path = output.as_deref().unwrap_or(if format == "json" {
                        "rumdl-config.json"
                    } else {
                        ".rumdl.toml"
                    });

                    if Path::new(output_path).exists() {
                        eprintln!("{}: Output file '{}' already exists", "Error".red().bold(), output_path);
                        exit::tool_error();
                    }

                    match fs::write(output_path, output_content) {
                        Ok(_) => {
                            println!("Converted markdownlint config from '{file}' to '{output_path}'");
                            println!("You can now use: rumdl check --config {output_path} .");
                        }
                        Err(e) => {
                            eprintln!("{}: Failed to write to '{}': {}", "Error".red().bold(), output_path, e);
                            exit::tool_error();
                        }
                    }
                }
            }
            Commands::Vscode { force, update, status } => {
                // Handle VS Code extension installation
                match rumdl_lib::vscode::handle_vscode_command(force, update, status) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("{}: {}", "Error".red().bold(), e);
                        exit::tool_error();
                    }
                }
            }
            Commands::Completions { shell, list } => {
                handle_completions_command(shell, list);
            }
            Commands::Clean => {
                handle_clean_command(&cli);
            }
            Commands::Version => {
                // Use clap's version info
                println!("rumdl {}", env!("CARGO_PKG_VERSION"));
            }
        }
    });
    if let Err(e) = result {
        eprintln!("[rumdl panic handler] Uncaught panic: {e:?}");
        exit::tool_error();
    } else {
        Ok(())
    }
}

fn run_check(args: &CheckArgs, global_config_path: Option<&str>, isolated: bool) {
    let quiet = args.quiet;
    let silent = args.silent;

    // Validate mutually exclusive options
    if args.diff && args.fix {
        eprintln!("{}: --diff and --fix cannot be used together", "Error".red().bold());
        eprintln!("Use --diff to preview changes, or --fix to apply them");
        exit::tool_error();
    }

    if args.check && args.fix {
        eprintln!("{}: --check and --fix cannot be used together", "Error".red().bold());
        eprintln!("Use --check to verify formatting without changes, or --fix to apply them");
        exit::tool_error();
    }

    // Warn about deprecated --force-exclude flag
    if args.force_exclude {
        eprintln!(
            "{}: --force-exclude is deprecated and has no effect",
            "warning".yellow().bold()
        );
        eprintln!("Exclude patterns are now always respected by default (as of v0.0.156)");
        eprintln!("Use --no-exclude if you want to disable exclusions");
    }

    // Check for watch mode
    if args.watch {
        watch::run_watch_mode(args, global_config_path, isolated, quiet);
        return;
    }

    // 1. Determine the directory for config discovery
    // Use the first target path for config discovery if it's a directory
    // Otherwise use current directory to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = if !args.paths.is_empty() {
        let first_path = std::path::Path::new(&args.paths[0]);
        if first_path.is_dir() {
            Some(first_path)
        } else {
            first_path.parent().filter(|&parent| parent.is_dir())
        }
    } else {
        None
    };

    // 2. Load sourced config (for provenance and validation)
    let mut sourced = load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // 3. Validate configuration
    let all_rules = rumdl_lib::rules::all_rules(&rumdl_config::Config::default());
    let registry = rumdl_config::RuleRegistry::from_rules(&all_rules);
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
    if !validation_warnings.is_empty() && !args.silent {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
        // Do NOT exit; continue with valid config
    }

    // 3b. Validate CLI rule names
    let cli_warnings = rumdl_config::validate_cli_rule_names(
        args.enable.as_deref(),
        args.disable.as_deref(),
        args.extend_enable.as_deref(),
        args.extend_disable.as_deref(),
    );
    if !cli_warnings.is_empty() && !args.silent {
        for warn in &cli_warnings {
            eprintln!("\x1b[33m[cli warning]\x1b[0m {}", warn.message);
        }
    }

    // 3c. Apply CLI argument overrides (e.g., --flavor)
    apply_cli_overrides(&mut sourced, args);

    // 4. Extract cache_dir and project_root before converting sourced
    let cache_dir_from_config = sourced
        .global
        .cache_dir
        .as_ref()
        .map(|sv| std::path::PathBuf::from(&sv.value));

    let project_root = sourced.project_root.clone();

    // 5. Convert to Config for the rest of the linter
    // Validation warnings are already printed above, so we use into_validated_unchecked
    let config: rumdl_config::Config = sourced.into_validated_unchecked().into();

    // 6. Initialize cache if enabled
    // CLI --no-cache flag takes precedence over config
    let cache_enabled = !args.no_cache && config.global.cache;

    // Resolve cache directory with precedence: CLI → env var → config → default
    let mut cache_dir = args
        .cache_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("RUMDL_CACHE_DIR").ok().map(std::path::PathBuf::from))
        .or(cache_dir_from_config)
        .unwrap_or_else(|| std::path::PathBuf::from(".rumdl_cache"));

    // If cache_dir is relative and we have a project root, resolve relative to project root
    // This ensures cache is created at project root, not CWD (fixes issue #159)
    if cache_dir.is_relative()
        && let Some(ref root) = project_root
    {
        cache_dir = root.join(&cache_dir);
    }

    let cache = if cache_enabled {
        let cache_instance = cache::LintCache::new(cache_dir.clone(), cache_enabled);

        // Initialize cache directory structure
        if let Err(e) = cache_instance.init() {
            if !silent {
                eprintln!("Warning: Failed to initialize cache: {e}");
            }
            // Continue without cache
            None
        } else {
            // Wrap in Arc<Mutex<>> for thread-safe sharing across parallel workers
            Some(std::sync::Arc::new(std::sync::Mutex::new(cache_instance)))
        }
    } else {
        None
    };

    // Use the same cache directory for workspace index cache (when cache is enabled)
    let workspace_cache_dir = if cache_enabled { Some(cache_dir.as_path()) } else { None };

    let (has_issues, has_warnings, has_errors, total_issues_fixed) = watch::perform_check_run(
        args,
        &config,
        quiet,
        cache,
        workspace_cache_dir,
        project_root.as_deref(),
    );

    // In --check mode (for fmt), exit with code 1 if any formatting changes would be made
    if args.check && total_issues_fixed > 0 {
        exit::violations_found();
    }

    // Determine if we should fail based on --fail-on setting
    let should_fail = match args.fail_on_mode {
        FailOn::Never => false,
        FailOn::Error => has_errors,
        FailOn::Warning => has_warnings,
        FailOn::Any => has_issues,
    };

    if should_fail && args.fix_mode != FixMode::Format {
        exit::violations_found();
    }
}

// Handle explain command
fn handle_explain_command(rule_query: &str) {
    use rumdl_lib::rules::*;

    // Get all rules
    let all_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
        Box::new(MD005ListIndent::default()),
        Box::new(MD007ULIndent::default()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD010NoHardTabs::default()),
        Box::new(MD011NoReversedLinks {}),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD014CommandsShowOutput::default()),
        Box::new(MD018NoMissingSpaceAtx {}),
        Box::new(MD019NoMultipleSpaceAtx {}),
        Box::new(MD020NoMissingSpaceClosedAtx {}),
        Box::new(MD021NoMultipleSpaceClosedAtx {}),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD023HeadingStartLeft {}),
        Box::new(MD024NoDuplicateHeading::default()),
        Box::new(MD025SingleTitle::default()),
        Box::new(MD026NoTrailingPunctuation::default()),
        Box::new(MD027MultipleSpacesBlockquote {}),
        Box::new(MD028NoBlanksBlockquote {}),
        Box::new(MD029OrderedListPrefix::default()),
        Box::new(MD030ListMarkerSpace::default()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD033NoInlineHtml::default()),
        Box::new(MD034NoBareUrls {}),
        Box::new(MD035HRStyle::default()),
        Box::new(MD036NoEmphasisAsHeading::new(".,;:!?".to_string())),
        Box::new(MD037NoSpaceInEmphasis),
        Box::new(MD038NoSpaceInCode::default()),
        Box::new(MD039NoSpaceInLinks),
        Box::new(MD040FencedCodeLanguage {}),
        Box::new(MD041FirstLineHeading::default()),
        Box::new(MD042NoEmptyLinks::new()),
        Box::new(MD043RequiredHeadings::new(Vec::new())),
        Box::new(MD044ProperNames::new(Vec::new(), true)),
        Box::new(MD045NoAltText::new()),
        Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
        Box::new(MD047SingleTrailingNewline),
        Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
        Box::new(MD049EmphasisStyle::default()),
        Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
        Box::new(MD051LinkFragments::new()),
        Box::new(MD052ReferenceLinkImages::new()),
        Box::new(MD053LinkImageReferenceDefinitions::default()),
        Box::new(MD054LinkImageStyle::default()),
        Box::new(MD055TablePipeStyle::default()),
        Box::new(MD056TableColumnCount),
        Box::new(MD057ExistingRelativeLinks::default()),
        Box::new(MD058BlanksAroundTables::default()),
        Box::new(MD059LinkText::default()),
        Box::new(MD060TableFormat::default()),
        Box::new(MD061ForbiddenTerms::default()),
        Box::new(MD062LinkDestinationWhitespace::new()),
        Box::new(MD063HeadingCapitalization::default()),
        Box::new(MD064NoMultipleConsecutiveSpaces::new()),
        Box::new(MD065BlanksAroundHorizontalRules),
        Box::new(MD066FootnoteValidation),
        Box::new(MD067FootnoteDefinitionOrder),
        Box::new(MD068EmptyFootnoteDefinition),
    ];

    // Find the rule
    let rule_query_upper = rule_query.to_ascii_uppercase();
    let found = all_rules.iter().find(|r| {
        r.name().eq_ignore_ascii_case(&rule_query_upper)
            || r.name().replace("MD", "") == rule_query_upper.replace("MD", "")
    });

    if let Some(rule) = found {
        let rule_name = rule.name();
        let rule_id = rule_name.to_lowercase();

        // Print basic info
        println!("{}", format!("{} - {}", rule_name, rule.description()).bold());
        println!();

        // Try to load detailed documentation from docs/
        let doc_path = format!("docs/{rule_id}.md");
        match fs::read_to_string(&doc_path) {
            Ok(doc_content) => {
                // Parse and display the documentation
                let lines: Vec<&str> = doc_content.lines().collect();
                let mut in_example = false;

                for line in lines.iter().skip(1) {
                    // Skip the title line
                    if line.starts_with("## ") {
                        println!("\n{}", line.trim_start_matches("## ").bold().underline());
                    } else if line.starts_with("### ") {
                        println!("\n{}", line.trim_start_matches("### ").bold());
                    } else if line.starts_with("```") {
                        println!("{}", line.dimmed());
                        in_example = !in_example;
                    } else if in_example {
                        if line.contains("<!-- Good -->") {
                            println!("{}", "✓ Good:".green());
                        } else if line.contains("<!-- Bad -->") {
                            println!("{}", "✗ Bad:".red());
                        } else {
                            println!("  {line}");
                        }
                    } else if !line.trim().is_empty() {
                        println!("{line}");
                    } else {
                        println!();
                    }
                }

                // Add a note about configuration
                if let Some((_, config_section)) = rule.default_config_section() {
                    println!("\n{}", "Default Configuration:".bold());
                    println!("{}", format!("[{rule_name}]").dimmed());
                    if let Ok(config_str) = toml::to_string_pretty(&config_section) {
                        for line in config_str.lines() {
                            println!("{}", line.dimmed());
                        }
                    }
                }
            }
            Err(_) => {
                // Fallback to basic information
                println!("Category: {:?}", rule.category());
                println!();
                println!("This rule helps maintain consistent Markdown formatting.");
                println!();
                println!("For more information, see the documentation at:");
                println!("  https://rumdl.dev/{rule_id}/");
            }
        }
    } else {
        eprintln!("{}: Rule '{}' not found.", "Error".red().bold(), rule_query);
        eprintln!("\nUse 'rumdl rule' to see all available rules.");
        exit::tool_error();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_calculate_directory_stats_empty() {
        let temp_dir = TempDir::new().unwrap();
        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_calculate_directory_stats_with_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
        assert_eq!(count, 2);
    }

    #[test]
    fn test_calculate_directory_stats_nested() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested directories
        let nested = temp_dir.path().join("nested");
        fs::create_dir(&nested).unwrap();

        fs::write(temp_dir.path().join("file1.txt"), "abc").unwrap();
        fs::write(nested.join("file2.txt"), "defgh").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 8); // "abc" (3) + "defgh" (5)
        assert_eq!(count, 2);
    }

    #[test]
    fn test_calculate_directory_stats_deeply_nested() {
        let temp_dir = TempDir::new().unwrap();

        // Create deeply nested structure
        let level1 = temp_dir.path().join("level1");
        let level2 = level1.join("level2");
        let level3 = level2.join("level3");
        fs::create_dir_all(&level3).unwrap();

        fs::write(temp_dir.path().join("root.txt"), "1").unwrap();
        fs::write(level1.join("l1.txt"), "12").unwrap();
        fs::write(level2.join("l2.txt"), "123").unwrap();
        fs::write(level3.join("l3.txt"), "1234").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 10); // 1 + 2 + 3 + 4
        assert_eq!(count, 4);
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(42), "42 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1024 * 10), "10.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
        assert_eq!(format_size(1024 * 1024 * 5), "5.00 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 2 + 512 * 1024 * 1024), "2.50 GB");
    }

    #[test]
    fn test_format_size_terabytes() {
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024 * 3), "3.00 TB");
    }

    #[test]
    fn test_format_size_edge_cases() {
        // Just under next unit
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024 * 1024 - 1), "1024.00 KB");

        // Exact boundaries
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_format_size_realistic_cache_sizes() {
        // Small cache
        assert_eq!(format_size(458), "458 B");

        // Medium cache
        assert_eq!(format_size(156_234), "152.57 KB");

        // Large cache (like the Ruff issue)
        assert_eq!(format_size(1_500_000_000), "1.40 GB");
    }
}
