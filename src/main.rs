use clap::{Args, Parser, Subcommand};
use colored::*;
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use memmap2::Mmap;
use rayon::prelude::*;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::time::Instant;

use rumdl::config as rumdl_config;
use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::code_block_utils::CodeBlockStyle;
use rumdl::rules::code_fence_utils::CodeFenceStyle;
use rumdl::rules::strong_style::StrongStyle;

use rumdl_config::normalize_key;
use rumdl_config::ConfigSource;

/// Threshold for using memory-mapped I/O (1MB)
const MMAP_THRESHOLD: u64 = 1024 * 1024;

/// Efficiently read file content using memory mapping for large files
fn read_file_efficiently(path: &Path) -> Result<String, Box<dyn Error>> {
    // Get file metadata first
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size > MMAP_THRESHOLD {
        // Use memory mapping for large files
        let file = fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // Convert to string - this is still a copy but more efficient for large files
        String::from_utf8(mmap.to_vec())
            .map_err(|e| format!("Invalid UTF-8 in file {}: {}", path.display(), e).into())
    } else {
        // Use regular reading for small files
        fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file {}: {}", path.display(), e).into())
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Control colored output: auto, always, never
    #[arg(long, global = true, default_value = "auto", value_parser = ["auto", "always", "never"], help = "Control colored output: auto, always, never")]
    color: String,

    /// Legacy: allow positional paths for backwards compatibility
    #[arg(required = false, hide = true)]
    paths: Vec<String>,

    /// Path to configuration file
    #[arg(long, global = true, help = "Path to configuration file")]
    config: Option<String>,

    /// Ignore all configuration files and use built-in defaults
    #[arg(
        long,
        global = true,
        help = "Ignore all configuration files and use built-in defaults"
    )]
    no_config: bool,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false", hide = true)]
    _fix: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false", hide = true)]
    list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long, hide = true)]
    disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(short, long, hide = true)]
    enable: Option<String>,

    /// Exclude specific files or directories (comma-separated glob patterns)
    #[arg(long, hide = true)]
    exclude: Option<String>,

    /// Include only specific files or directories (comma-separated glob patterns).
    #[arg(long, hide = true)]
    include: Option<String>,

    /// Respect .gitignore files when scanning directories
    #[arg(
        long,
        default_value = "true",
        help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)",
        hide = true
    )]
    respect_gitignore: bool,

    /// Show detailed output
    #[arg(short, long, hide = true)]
    verbose: bool,

    /// Show profiling information
    #[arg(long, hide = true)]
    profile: bool,

    /// Quiet mode
    #[arg(short, long, hide = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint Markdown files and print warnings/errors
    Check(CheckArgs),
    /// Initialize a new configuration file
    Init {
        /// Generate configuration for pyproject.toml instead of .rumdl.toml
        #[arg(long)]
        pyproject: bool,
    },
    /// Show information about a rule or list all rules
    Rule {
        /// Rule name or ID (optional)
        rule: Option<String>,
    },
    /// Show configuration or query a specific key
    Config {
        #[command(subcommand)]
        subcmd: Option<ConfigSubcommand>,
        /// Show only the default configuration values
        #[arg(long, help = "Show only the default configuration values")]
        defaults: bool,
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
    },
    /// Show version information
    Version,
}

#[derive(Subcommand, Debug)]
enum ConfigSubcommand {
    /// Query a specific config key (e.g. global.exclude or MD013.line_length)
    Get { key: String },
}

#[derive(Args, Debug)]
struct CheckArgs {
    /// Files or directories to lint.
    /// If provided, these paths take precedence over include patterns.
    #[arg(required = false)]
    paths: Vec<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    _fix: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false")]
    list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long)]
    disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(short, long)]
    enable: Option<String>,

    /// Exclude specific files or directories (comma-separated glob patterns)
    #[arg(long)]
    exclude: Option<String>,

    /// Include only specific files or directories (comma-separated glob patterns).
    #[arg(long)]
    include: Option<String>,

    /// Respect .gitignore files when scanning directories
    #[arg(
        long,
        default_value = "true",
        help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)"
    )]
    respect_gitignore: bool,

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,

    /// Show profiling information
    #[arg(long)]
    profile: bool,

    /// Quiet mode
    #[arg(short, long)]
    quiet: bool,

    /// Output format: text (default) or json
    #[arg(long, short = 'o', default_value = "text")]
    output: String,

    /// Read from stdin instead of files
    #[arg(long, help = "Read from stdin instead of files")]
    stdin: bool,
}

// Get a complete set of enabled rules based on CLI options and config
fn get_enabled_rules_from_checkargs(
    args: &CheckArgs,
    config: &rumdl_config::Config,
) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules using from_config only
    let all_rules: Vec<Box<dyn Rule>> = rumdl::rules::all_rules(config);

    // 2. Determine the final list of enabled rules based on precedence
    let final_rules: Vec<Box<dyn Rule>>;

    // Rule names provided via CLI flags
    let cli_enable_set: Option<HashSet<&str>> = args.enable.as_deref().map(|s| {
        s.split(',')
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .collect()
    });
    let cli_disable_set: Option<HashSet<&str>> = args.disable.as_deref().map(|s| {
        s.split(',')
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .collect()
    });

    // Rule names provided via config file
    let config_enable_set: HashSet<&str> =
        config.global.enable.iter().map(|s| s.as_str()).collect();

    let config_disable_set: HashSet<&str> =
        config.global.disable.iter().map(|s| s.as_str()).collect();

    if let Some(enabled_cli) = &cli_enable_set {
        // Normalize CLI enable values
        let enabled_cli_normalized: HashSet<String> =
            enabled_cli.iter().map(|s| normalize_key(s)).collect();
        let _all_rule_names: Vec<String> =
            all_rules.iter().map(|r| normalize_key(r.name())).collect();
        final_rules = all_rules
            .into_iter()
            .filter(|rule| enabled_cli_normalized.contains(&normalize_key(rule.name())))
            .collect();
        // Note: CLI --disable is IGNORED if CLI --enable is present.
    } else {
        // --- Case 2: No CLI --enable ---
        // Start with the configured rules.
        let mut current_rules = all_rules;

        // Step 2a: Apply config `enable` (if specified).
        // If config.enable is not empty, it acts as an *exclusive* list.
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                config_enable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Step 2b: Apply config `disable`.
        // Remove rules specified in config.disable from the current set.
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                let is_disabled = config_disable_set.contains(normalized_rule_name.as_str());
                !is_disabled // Keep if NOT disabled
            });
        }

        // Step 2c: Apply CLI `disable`.
        // Remove rules specified in cli.disable from the result of steps 2a & 2b.
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper)
                    && !disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        final_rules = current_rules; // Assign the final filtered vector
    }

    // 4. Print enabled rules if verbose
    if args.verbose {
        println!("Enabled rules:");
        for rule in &final_rules {
            println!("  - {} ({})", rule.name(), rule.description());
        }
        println!();
    }

    final_rules
}

// Find all markdown files using the `ignore` crate, returning Result
fn find_markdown_files(
    paths: &[String],
    args: &CheckArgs,
    config: &rumdl_config::Config,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut file_paths = Vec::new();

    // --- Configure ignore::WalkBuilder ---
    // Start with the first path, add others later
    let first_path = paths.first().cloned().unwrap_or_else(|| ".".to_string());
    let mut walk_builder = WalkBuilder::new(first_path);

    // Add remaining paths
    for path in paths.iter().skip(1) {
        walk_builder.add(path);
    }

    // --- Add Markdown File Type Definition ---
    let mut types_builder = ignore::types::TypesBuilder::new();
    types_builder.add_defaults(); // Add standard types
    types_builder.add("markdown", "*.md").unwrap();
    types_builder.add("markdown", "*.markdown").unwrap();
    types_builder.select("markdown"); // Select ONLY markdown for processing
    let types = types_builder.build().unwrap();
    walk_builder.types(types);
    // -----------------------------------------

    // Determine if running in discovery mode (e.g., "rumdl ." or "rumdl check ." or "rumdl check")
    // Adjusted to handle both legacy and subcommand paths
    let is_discovery_mode = paths.is_empty() || paths == ["."];

    // --- Determine Effective Include/Exclude Patterns ---

    // Include patterns: CLI > Config (only in discovery mode) > Default (only in discovery mode)
    let final_include_patterns: Vec<String> = if let Some(cli_include) = args.include.as_deref() {
        // 1. CLI --include always wins
        cli_include
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    } else if is_discovery_mode && !config.global.include.is_empty() {
        // 2. Config include is used ONLY in discovery mode if specified
        config.global.include.clone()
    } else if is_discovery_mode {
        // 3. Default include (*.md, *.markdown) ONLY in discovery mode if no CLI/Config include
        vec!["*.md".to_string(), "*.markdown".to_string()]
    } else {
        // 4. Explicit path mode: No includes applied by default. Walk starts from explicit paths.
        Vec::new()
    };

    // Exclude patterns: CLI > Config
    let final_exclude_patterns: Vec<String> = if let Some(cli_exclude) = args.exclude.as_deref() {
        cli_exclude
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        config.global.exclude.clone()
    };
    // --- End Pattern Determination ---

    // Apply overrides using the determined patterns
    if !final_include_patterns.is_empty() || !final_exclude_patterns.is_empty() {
        let mut override_builder = OverrideBuilder::new("."); // Root context

        // Add includes (these act as positive filters)
        for pattern in &final_include_patterns {
            // Important: In ignore crate, bare patterns act as includes if no exclude (!) is present.
            // If we add excludes later, these includes ensure *only* matching files are considered.
            // If no excludes are added, these effectively define the set of files to walk.
            if let Err(e) = override_builder.add(pattern) {
                eprintln!("Warning: Invalid include pattern '{}': {}", pattern, e);
            }
        }

        // Add excludes (these filter *out* files) - MUST start with '!'
        for pattern in &final_exclude_patterns {
            // Ensure exclude patterns start with '!' for ignore crate overrides
            let exclude_rule = if pattern.starts_with('!') {
                pattern.clone() // Already formatted
            } else {
                format!("!{}", pattern)
            };
            if let Err(e) = override_builder.add(&exclude_rule) {
                eprintln!("Warning: Invalid exclude pattern '{}': {}", pattern, e);
            }
        }

        // Build and apply the overrides
        match override_builder.build() {
            Ok(overrides) => {
                walk_builder.overrides(overrides);
            }
            Err(e) => {
                eprintln!("Error building path overrides: {}", e);
            }
        };
    }

    // Configure gitignore handling *SECOND*
    let use_gitignore = if args.respect_gitignore {
        true // If respect is true, always include gitignore
    } else {
        false // If respect is false, always exclude gitignore
    };

    walk_builder.ignore(use_gitignore); // Enable/disable .ignore
    walk_builder.git_ignore(use_gitignore); // Enable/disable .gitignore
    walk_builder.git_global(use_gitignore); // Enable/disable global gitignore
    walk_builder.git_exclude(use_gitignore); // Enable/disable .git/info/exclude
    walk_builder.parents(use_gitignore); // Enable/disable parent ignores
    walk_builder.hidden(true); // Keep hidden files ignored unconditionally
    walk_builder.require_git(false); // Process git ignores even if no repo detected

    // --- Execute Walk ---

    for result in walk_builder.build() {
        match result {
            Ok(entry) => {
                let path = entry.path();
                // We are primarily interested in files. ignore crate handles dir traversal.
                // Check if it's a file and if it wasn't explicitly excluded by overrides
                if path.is_file() {
                    let file_path = path.to_string_lossy().to_string();
                    // Clean the path before pushing
                    let cleaned_path = if let Some(stripped) = file_path.strip_prefix("./") {
                        stripped.to_string()
                    } else {
                        file_path
                    };
                    file_paths.push(cleaned_path);
                }
            }
            Err(err) => eprintln!("Error walking directory: {}", err),
        }
    }

    // Remove duplicate paths if WalkBuilder might yield them (e.g. multiple input paths)
    file_paths.sort();
    file_paths.dedup();

    // --- Final Explicit Markdown Filter ---
    // Ensure only files with .md or .markdown extensions are returned,
    // regardless of how ignore crate overrides interacted with type filters.
    file_paths.retain(|path_str| {
        let path = Path::new(path_str);
        path.extension()
            .map_or(false, |ext| ext == "md" || ext == "markdown")
    });
    // -------------------------------------

    Ok(file_paths) // Ensure the function returns the result
}

// Define a struct to hold the print results arguments
pub(crate) struct PrintResultsArgs<'a> {
    pub args: &'a CheckArgs,
    pub has_issues: bool,
    pub files_with_issues: usize,
    pub total_issues: usize,
    pub total_issues_fixed: usize,
    pub total_fixable_issues: usize,
    pub total_files_processed: usize,
    pub duration_ms: u64,
}

fn print_results_from_checkargs(params: PrintResultsArgs) {
    let PrintResultsArgs {
        args,
        has_issues,
        files_with_issues,
        total_issues,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
        duration_ms,
    } = params;
    // Choose singular or plural form of "file" based on count
    let file_text = if total_files_processed == 1 {
        "file"
    } else {
        "files"
    };
    let file_with_issues_text = if files_with_issues == 1 {
        "file"
    } else {
        "files"
    };

    // Show results summary
    if has_issues {
        // If fix mode is enabled, only show the fixed summary
        if args._fix && total_issues_fixed > 0 {
            println!(
                "\n{} Fixed {}/{} issues in {} {} ({}ms)",
                "Fixed:".green().bold(),
                total_issues_fixed,
                total_issues,
                files_with_issues,
                file_with_issues_text,
                duration_ms
            );
        } else {
            // In non-fix mode, show issues summary with simplified count when appropriate
            let files_display = if files_with_issues == total_files_processed {
                // Just show the number if all files have issues
                format!("{}", files_with_issues)
            } else {
                // Show the fraction if only some files have issues
                format!("{}/{}", files_with_issues, total_files_processed)
            };

            println!(
                "\n{} Found {} issues in {} {} ({}ms)",
                "Issues:".yellow().bold(),
                total_issues,
                files_display,
                file_text,
                duration_ms
            );

            if !args._fix && total_fixable_issues > 0 {
                // Display the exact count of fixable issues
                println!(
                    "Run with `--fix` to automatically fix {} of the {} issues",
                    total_fixable_issues, total_issues
                );
            }
        }
    } else {
        println!(
            "\n{} No issues found in {} {} ({}ms)",
            "Success:".green().bold(),
            total_files_processed,
            file_text,
            duration_ms
        );
    }
}

fn format_provenance(src: rumdl_config::ConfigSource) -> &'static str {
    match src {
        rumdl_config::ConfigSource::Cli => "CLI",
        rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml",
        rumdl_config::ConfigSource::PyprojectToml => "pyproject.toml",
        rumdl_config::ConfigSource::Default => "default",
        rumdl_config::ConfigSource::Markdownlint => "markdownlint",
    }
}

fn print_config_with_provenance(sourced: &rumdl_config::SourcedConfig) {
    use colored::*;
    use rumdl::rule::Rule;
    use rumdl::rules::*;
    let g = &sourced.global;
    let mut all_lines = Vec::new();
    // [global] section
    let global_lines = vec![
        ("[global]".to_string(), String::new()),
        (
            format!("enable = {:?}", g.enable.value),
            format!("[from {}]", format_provenance(g.enable.source)),
        ),
        (
            format!("disable = {:?}", g.disable.value),
            format!("[from {}]", format_provenance(g.disable.source)),
        ),
        (
            format!("exclude = {:?}", g.exclude.value),
            format!("[from {}]", format_provenance(g.exclude.source)),
        ),
        (
            format!("include = {:?}", g.include.value),
            format!("[from {}]", format_provenance(g.include.source)),
        ),
        (
            format!("respect_gitignore = {}", g.respect_gitignore.value),
            format!("[from {}]", format_provenance(g.respect_gitignore.source)),
        ),
        (String::new(), String::new()),
    ];
    all_lines.extend(global_lines);
    // All rules, but only if they have config items
    let all_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD002FirstHeadingH1::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
        Box::new(MD005ListIndent),
        Box::new(MD006StartBullets),
        Box::new(MD007ULIndent::default()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD010NoHardTabs::default()),
        Box::new(MD011NoReversedLinks {}),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
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
        Box::new(MD031BlanksAroundFences {}),
        Box::new(MD032BlanksAroundLists {}),
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
        Box::new(MD053LinkImageReferenceDefinitions::new()),
        Box::new(MD054LinkImageStyle::default()),
        Box::new(MD055TablePipeStyle::default()),
        Box::new(MD056TableColumnCount),
        Box::new(MD058BlanksAroundTables),
    ];
    let mut rule_names: Vec<_> = all_rules.iter().map(|r| r.name().to_string()).collect();
    rule_names.sort();
    for rule_name in rule_names {
        let mut lines = Vec::new();
        let norm_rule_name = rule_name.to_ascii_uppercase(); // Use uppercase for lookup
        if let Some(rule_cfg) = sourced.rules.get(&norm_rule_name) {
            let mut keys: Vec<_> = rule_cfg.values.keys().collect();
            keys.sort();
            for key in keys {
                let sv = &rule_cfg.values[key];
                let value_str = match &sv.value {
                    toml::Value::Array(arr) => {
                        let vals: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                        format!("[{}]", vals.join(", "))
                    }
                    toml::Value::String(s) => format!("\"{}\"", s),
                    toml::Value::Boolean(b) => b.to_string(),
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    _ => sv.value.to_string(),
                };
                lines.push((
                    format!("{} = {}", key, value_str),
                    format!("[from {}]", format_provenance(sv.source)),
                ));
            }
        } else {
            // Print default config for this rule, if available
            if let Some((_, toml::Value::Table(table))) = all_rules
                .iter()
                .find(|r| r.name() == rule_name)
                .and_then(|r| r.default_config_section())
            {
                let mut keys: Vec<_> = table.keys().collect();
                keys.sort();
                for key in keys {
                    let v = &table[key];
                    let value_str = match v {
                        toml::Value::Array(arr) => {
                            let vals: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                            format!("[{}]", vals.join(", "))
                        }
                        toml::Value::String(s) => format!("\"{}\"", s),
                        toml::Value::Boolean(b) => b.to_string(),
                        toml::Value::Integer(i) => i.to_string(),
                        toml::Value::Float(f) => f.to_string(),
                        _ => v.to_string(),
                    };
                    lines.push((
                        format!("{} = {}", key, value_str),
                        format!(
                            "[from {}]",
                            format_provenance(rumdl_config::ConfigSource::Default)
                        ),
                    ));
                }
            }
        }
        if !lines.is_empty() {
            all_lines.push((format!("[{}]", rule_name), String::new()));
            all_lines.extend(lines);
            all_lines.push((String::new(), String::new()));
        }
    }
    let max_left = all_lines.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (left, right) in &all_lines {
        if left.is_empty() && right.is_empty() {
            println!();
        } else if !right.is_empty() {
            println!("{:<width$} {}", left, right.dimmed(), width = max_left);
        } else {
            println!("{:<width$} {}", left, right, width = max_left);
        }
    }
}

fn format_toml_value(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            let vals: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", vals.join(", "))
        }
        toml::Value::Table(_) => "<table>".to_string(),
        toml::Value::Datetime(dt) => dt.to_string(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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
        match &cli.command {
            Some(Commands::Init { pyproject }) => {
                if *pyproject {
                    // Handle pyproject.toml initialization
                    let config_content = rumdl_config::generate_pyproject_config();

                    if Path::new("pyproject.toml").exists() {
                        // pyproject.toml exists, ask to append
                        println!("pyproject.toml already exists. Would you like to append rumdl configuration? [y/N]");
                        print!("> ");
                        io::stdout().flush().unwrap();

                        let mut answer = String::new();
                        io::stdin().read_line(&mut answer).unwrap();

                        if answer.trim().eq_ignore_ascii_case("y") {
                            // Append to existing file
                            match fs::read_to_string("pyproject.toml") {
                                Ok(content) => {
                                    // Check if [tool.rumdl] section already exists
                                    if content.contains("[tool.rumdl]") {
                                        println!("The pyproject.toml file already contains a [tool.rumdl] section.");
                                        println!("Please edit the file manually to avoid overwriting existing configuration.");
                                    }

                                    // Append with a blank line for separation
                                    let new_content =
                                        format!("{}\n\n{}", content.trim_end(), config_content);
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
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{}: Failed to read pyproject.toml: {}",
                                        "Error".red().bold(),
                                        e
                                    );
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            println!("Aborted. No changes made to pyproject.toml");
                        }
                    } else {
                        // Create new pyproject.toml with basic structure
                        let basic_content = r#"[build-system]
requires = [\"setuptools>=42\", \"wheel\"]
build-backend = \"setuptools.build_meta\"

"#;
                        let content = basic_content.to_owned() + &config_content;

                        match fs::write("pyproject.toml", content) {
                            Ok(_) => {
                                println!("Created pyproject.toml with rumdl configuration");
                            }
                            Err(e) => {
                                eprintln!(
                                    "{}: Failed to create pyproject.toml: {}",
                                    "Error".red().bold(),
                                    e
                                );
                                std::process::exit(1);
                            }
                        }
                    }
                }

                // Create default config file
                match rumdl_config::create_default_config(".rumdl.toml") {
                    Ok(_) => {
                        println!("Created default configuration file: .rumdl.toml");
                    }
                    Err(e) => {
                        eprintln!(
                            "{}: Failed to create config file: {}",
                            "Error".red().bold(),
                            e
                        );
                        std::process::exit(1);
                    }
                }
            }
            Some(Commands::Check(args)) => {
                // If --no-config is set, skip config loading
                if cli.no_config {
                    run_check(args, None, cli.no_config);
                } else {
                    run_check(args, cli.config.as_deref(), cli.no_config);
                }
            }
            Some(Commands::Rule { rule }) => {
                use rumdl::rules::*;
                let all_rules: Vec<Box<dyn Rule>> = vec![
                    Box::new(MD001HeadingIncrement),
                    Box::new(MD002FirstHeadingH1::default()),
                    Box::new(MD003HeadingStyle::default()),
                    Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
                    Box::new(MD005ListIndent),
                    Box::new(MD006StartBullets),
                    Box::new(MD007ULIndent::default()),
                    Box::new(MD009TrailingSpaces::default()),
                    Box::new(MD010NoHardTabs::default()),
                    Box::new(MD011NoReversedLinks {}),
                    Box::new(MD012NoMultipleBlanks::default()),
                    Box::new(MD013LineLength::default()),
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
                    Box::new(MD031BlanksAroundFences {}),
                    Box::new(MD032BlanksAroundLists {}),
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
                    Box::new(MD053LinkImageReferenceDefinitions::new()),
                    Box::new(MD054LinkImageStyle::default()),
                    Box::new(MD055TablePipeStyle::default()),
                    Box::new(MD056TableColumnCount),
                    Box::new(MD058BlanksAroundTables),
                ];
                if let Some(rule_query) = rule {
                    let rule_query = rule_query.to_ascii_uppercase();
                    let found = all_rules.iter().find(|r| {
                        r.name().eq_ignore_ascii_case(&rule_query)
                            || r.name().replace("MD", "") == rule_query.replace("MD", "")
                    });
                    if let Some(rule) = found {
                        println!(
                            "{} - {}\n\nDescription:\n  {}",
                            rule.name(),
                            rule.description(),
                            rule.description()
                        );
                    } else {
                        eprintln!("Rule '{}' not found.", rule_query);
                        std::process::exit(1);
                    }
                } else {
                    println!("Available rules:");
                    for rule in &all_rules {
                        println!("  {} - {}", rule.name(), rule.description());
                    }
                }
            }
            Some(Commands::Config {
                subcmd,
                defaults,
                output,
            }) => {
                // Handle 'config get' subcommand for querying a specific key
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
                                std::process::exit(1);
                            }
                        };
                        // 2. Convert to final Config once
                        let final_config: rumdl_config::Config = sourced.clone().into();

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
                                    _ => None,
                                };

                            if let Some((value, source)) = maybe_value_source {
                                println!(
                                    "{} = {} [from {}]",
                                    key,
                                    format_toml_value(&value),
                                    format_provenance(source)
                                );
                                // Successfully handled 'get', exit the command processing
                            } else {
                                eprintln!("Unknown global key: {}", field_part);
                                std::process::exit(1);
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
                                    format_toml_value(value),
                                    format_provenance(provenance)
                                );
                                // Successfully handled 'get', exit the command processing
                            } else {
                                let all_rules =
                                    rumdl::rules::all_rules(&rumdl_config::Config::default());
                                if let Some(rule) =
                                    all_rules.iter().find(|r| r.name() == section_part)
                                {
                                    if let Some((_, toml::Value::Table(table))) =
                                        rule.default_config_section()
                                    {
                                        if let Some(v) = table.get(&normalized_field) {
                                            let value_str = format_toml_value(v);
                                            println!(
                                                "{}.{} = {} [from default]",
                                                normalized_rule_name, normalized_field, value_str
                                            );
                                            // Successfully handled 'get', exit the command processing
                                            return;
                                        }
                                    }
                                }
                                eprintln!(
                                    "Unknown config key: {}.{}",
                                    normalized_rule_name, normalized_field
                                );
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("Key must be in the form global.key or MDxxx.key");
                        std::process::exit(1);
                    }
                }
                // --- Fallthrough logic for `rumdl config` (no subcommand) ---
                // This code now runs ONLY if `subcmd` is None
                else {
                    // --- CONFIG VALIDATION --- (Duplicated from original position, needs to run for display)
                    let all_rules_reg = rumdl::rules::all_rules(&rumdl_config::Config::default()); // Rename to avoid conflict
                    let registry_reg = rumdl_config::RuleRegistry::from_rules(&all_rules_reg);
                    let sourced_reg = if *defaults {
                        rumdl_config::SourcedConfig::default()
                    } else {
                        match rumdl_config::SourcedConfig::load_with_discovery(
                            cli.config.as_deref(),
                            None,
                            cli.no_config,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("{}: {}", "Config error".red().bold(), e);
                                std::process::exit(1);
                            }
                        }
                    };
                    let validation_warnings =
                        rumdl_config::validate_config_sourced(&sourced_reg, &registry_reg);
                    if !validation_warnings.is_empty() {
                        for warn in &validation_warnings {
                            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
                        }
                        // Optionally: exit with error if strict mode is enabled
                        // std::process::exit(2);
                    }
                    // --- END CONFIG VALIDATION ---

                    // Decide which config to print based on --defaults
                    let final_sourced_to_print = if *defaults {
                        rumdl_config::SourcedConfig::default()
                    } else {
                        // Reload config if not defaults (necessary because we exited early in 'get' case)
                        match rumdl_config::SourcedConfig::load_with_discovery(
                            cli.config.as_deref(),
                            None,
                            cli.no_config,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("{}: {}", "Config error".red().bold(), e);
                                std::process::exit(1);
                            }
                        }
                    };

                    // If --output toml is set, print as valid TOML
                    if output.as_deref() == Some("toml") {
                        let config_to_print = if *defaults {
                            rumdl_config::Config::default()
                        } else {
                            final_sourced_to_print.into()
                        };
                        match toml::to_string_pretty(&config_to_print) {
                            Ok(s) => println!("{}", s),
                            Err(e) => {
                                eprintln!("Failed to serialize config to TOML: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Otherwise, print the smart output with provenance annotations
                        print_config_with_provenance(&final_sourced_to_print);
                    }
                }
            }
            Some(Commands::Server {
                port,
                stdio,
                verbose,
            }) => {
                // Setup logging for the LSP server
                if *verbose {
                    env_logger::Builder::from_default_env()
                        .filter_level(log::LevelFilter::Debug)
                        .init();
                } else {
                    env_logger::Builder::from_default_env()
                        .filter_level(log::LevelFilter::Info)
                        .init();
                }

                // Start the LSP server
                let runtime =
                    tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

                runtime.block_on(async {
                    if let Some(port) = port {
                        // TCP mode for debugging
                        if let Err(e) = rumdl::lsp::start_tcp_server(*port).await {
                            eprintln!("Failed to start LSP server on port {}: {}", port, e);
                            std::process::exit(1);
                        }
                    } else {
                        // Standard LSP mode over stdio (default behavior)
                        // Note: stdio flag is for explicit documentation, behavior is the same
                        let _ = stdio; // Suppress unused variable warning
                        if let Err(e) = rumdl::lsp::start_server().await {
                            eprintln!("Failed to start LSP server: {}", e);
                            std::process::exit(1);
                        }
                    }
                });
            }
            Some(Commands::Version) => {
                // Use clap's version info
                println!("rumdl {}", env!("CARGO_PKG_VERSION"));
            }
            None => {
                // Legacy: rumdl . or rumdl [PATHS...]
                if !cli.paths.is_empty() {
                    let args = CheckArgs {
                        paths: cli.paths.clone(),
                        _fix: cli._fix,
                        list_rules: cli.list_rules,
                        disable: cli.disable.clone(),
                        enable: cli.enable.clone(),
                        exclude: cli.exclude.clone(),
                        include: cli.include.clone(),
                        respect_gitignore: cli.respect_gitignore,
                        verbose: cli.verbose,
                        profile: cli.profile,
                        quiet: cli.quiet,
                        output: "text".to_string(),
                        stdin: false,
                    };
                    eprintln!("{}: Deprecation warning: Running 'rumdl .' or 'rumdl [PATHS...]' without a subcommand is deprecated and will be removed in a future release. Please use 'rumdl check .' instead.", "[rumdl]".yellow().bold());
                    run_check(&args, cli.config.as_deref(), cli.no_config);
                } else {
                    eprintln!(
                "{}: No files or directories specified. Please provide at least one path to lint.",
                "Error".red().bold()
            );
                    std::process::exit(1);
                }
            }
        }
    });
    if let Err(e) = result {
        eprintln!("[rumdl panic handler] Uncaught panic: {:?}", e);
        std::process::exit(1);
    } else {
        Ok(())
    }
}

/// Process markdown content from stdin
fn process_stdin(rules: &[Box<dyn Rule>], args: &CheckArgs) {
    // Read all content from stdin
    let mut content = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut content) {
        if !args.quiet {
            eprintln!("Error reading from stdin: {}", e);
        }
        process::exit(1);
    }

    // Create a lint context for the stdin content
    let ctx = LintContext::new(&content);
    let mut all_warnings = Vec::new();

    // Run all enabled rules on the content
    for rule in rules {
        match rule.check(&ctx) {
            Ok(warnings) => {
                // Set file path to "<stdin>" for all warnings
                // The warnings already have the correct character ranges
                all_warnings.extend(warnings);
            }
            Err(e) => {
                if !args.quiet {
                    eprintln!("Error running rule {}: {}", rule.name(), e);
                }
            }
        }
    }

    // Output results
    if args.output == "json" {
        // For JSON output, modify warnings to show "<stdin>" as filename
        let mut json_warnings = Vec::new();
        for warning in all_warnings {
            let mut json_warning = serde_json::to_value(&warning).unwrap();
            if let Some(obj) = json_warning.as_object_mut() {
                obj.insert(
                    "file".to_string(),
                    serde_json::Value::String("<stdin>".to_string()),
                );
            }
            json_warnings.push(json_warning);
        }
        println!("{}", serde_json::to_string_pretty(&json_warnings).unwrap());
    } else {
        // Text output
        let has_issues = !all_warnings.is_empty();
        if has_issues {
            for warning in &all_warnings {
                let rule_name = warning.rule_name.unwrap_or("unknown");
                println!(
                    "<stdin>:{}:{}: {} {}",
                    warning.line.to_string().cyan(),
                    warning.column.to_string().cyan(),
                    format!("[{:5}]", rule_name).yellow(), // Align rule names consistently
                    warning.message
                );
            }
        }

        if !args.quiet {
            if has_issues {
                println!("\nFound {} issue(s) in stdin", all_warnings.len());
            } else {
                println!("No issues found in stdin");
            }
        }

        // Exit with error code if issues found
        if has_issues {
            process::exit(1);
        }
    }
}

fn run_check(args: &CheckArgs, global_config_path: Option<&str>, no_config: bool) {
    // 1. Load sourced config (for provenance and validation)
    let sourced =
        match rumdl_config::SourcedConfig::load_with_discovery(global_config_path, None, no_config)
        {
            Ok(sourced) => sourced,
            Err(e) => {
                // Syntax error or type mismatch: fail and exit
                eprintln!("{}: {}", "Config error".red().bold(), e);
                std::process::exit(1);
            }
        };

    // 2. Validate config (unknown keys/rules/options)
    let all_rules = rumdl::rules::all_rules(&rumdl_config::Config::default());
    let registry = rumdl_config::RuleRegistry::from_rules(&all_rules);
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
    if !validation_warnings.is_empty() {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
        // Do NOT exit; continue with valid config
    }

    // 3. Convert to Config for the rest of the linter
    let config: rumdl_config::Config = sourced.into();

    // Initialize rules with configuration
    let enabled_rules = get_enabled_rules_from_checkargs(args, &config);

    // Handle stdin input
    if args.stdin {
        process_stdin(&enabled_rules, args);
        return;
    }

    // Find all markdown files to check
    let file_paths = match find_markdown_files(&args.paths, args, &config) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!(
                "{}: Failed to find markdown files: {}",
                "Error".red().bold(),
                e
            );
            process::exit(1);
        }
    };
    if file_paths.is_empty() {
        if !args.quiet {
            println!("No markdown files found to check.");
        }
        return;
    }

    // JSON output mode: collect all warnings and print as JSON
    if args.output == "json" {
        let mut all_warnings = Vec::new();
        for file_path in &file_paths {
            let warnings = process_file_collect_warnings(
                file_path,
                &enabled_rules,
                args._fix,
                args.verbose,
                args.quiet,
            );
            all_warnings.extend(warnings);
        }
        println!("{}", serde_json::to_string_pretty(&all_warnings).unwrap());
        return;
    }

    let start_time = Instant::now();

    // Choose processing strategy based on file count and fix mode
    let use_parallel = file_paths.len() > 1 && !args._fix; // Don't parallelize fixes due to file I/O conflicts

    let (
        has_issues,
        files_with_issues,
        total_issues,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
    ) = if use_parallel {
        // Parallel processing for multiple files without fixes
        let enabled_rules_arc = Arc::new(enabled_rules);

        let results: Vec<_> = file_paths
            .par_iter()
            .map(|file_path| {
                process_file(
                    file_path,
                    &enabled_rules_arc,
                    args._fix,
                    args.verbose,
                    args.quiet,
                )
            })
            .collect();

        // Aggregate results
        let mut has_issues = false;
        let mut files_with_issues = 0;
        let mut total_issues = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let total_files_processed = results.len();

        for (file_has_issues, issues_found, issues_fixed, fixable_issues) in results {
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                total_issues += issues_found;
            }
        }

        (
            has_issues,
            files_with_issues,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
        )
    } else {
        // Sequential processing for single files or when fixing
        let mut has_issues = false;
        let mut files_with_issues = 0;
        let mut total_issues = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let mut total_files_processed = 0;

        for file_path in &file_paths {
            let (file_has_issues, issues_found, issues_fixed, fixable_issues) = process_file(
                file_path,
                &enabled_rules,
                args._fix,
                args.verbose,
                args.quiet,
            );

            total_files_processed += 1;
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                total_issues += issues_found;
            }
        }

        (
            has_issues,
            files_with_issues,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
        )
    };

    let duration = start_time.elapsed();
    let duration_ms = duration.as_secs() * 1000 + duration.subsec_millis() as u64;

    // Print results summary if not in quiet mode
    if !args.quiet {
        print_results_from_checkargs(PrintResultsArgs {
            args,
            has_issues,
            files_with_issues,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
            duration_ms,
        });
    }

    // Print profiling information if enabled and not in quiet mode
    if args.profile && !args.quiet {
        match std::panic::catch_unwind(rumdl::profiling::get_report) {
            Ok(report) => println!("\n{}", report),
            Err(_) => println!("\nProfiling information not available"),
        }
    }

    // Exit with non-zero status if issues were found
    if has_issues {
        std::process::exit(1);
    }
}

// Process file operation
fn process_file(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    verbose: bool,
    quiet: bool,
) -> (bool, usize, usize, usize) {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        println!("Processing file: {}", file_path);
    }

    // Read file content efficiently
    let mut content = match read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {}: {}", file_path, e);
            }
            return (false, 0, 0, 0);
        }
    };

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return (false, 0, 0, 0);
    }

    let lint_start = Instant::now();
    // Set the environment variable for the file path
    // This allows rules like MD057 to know which file is being processed
    std::env::set_var("RUMDL_FILE_PATH", file_path);

    // Use the standard lint function
    let warnings_result = rumdl::lint(&content, rules, verbose);

    // Clear the environment variable after processing
    std::env::remove_var("RUMDL_FILE_PATH");

    // Combine all warnings
    let mut all_warnings = warnings_result.unwrap_or_default();

    // Sort warnings by line number, then column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let total_warnings = all_warnings.len();

    // Count fixable issues
    let fixable_warnings = all_warnings.iter().filter(|w| w.fix.is_some()).count();

    // If no warnings, return early
    if total_warnings == 0 {
        return (false, 0, 0, 0);
    }

    // Print warnings regardless of fix mode (unless in quiet mode)
    if !quiet {
        // Print the individual warnings
        for warning in &all_warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Add fix indicator if this warning has a fix
            let fix_indicator = if warning.fix.is_some() {
                if _fix {
                    " [fixed]"
                } else {
                    " [*]"
                }
            } else {
                ""
            };

            // Print the warning in the format: file:line:column: [rule] message [*]
            println!(
                "{}:{}:{}: {} {}{}",
                file_path.blue().underline(),
                warning.line.to_string().cyan(),
                warning.column.to_string().cyan(),
                format!("[{:5}]", rule_name).yellow(), // Pad rule name to 5 characters for alignment
                warning.message,
                fix_indicator.green()
            );
        }
    }

    // Fix issues if requested
    let mut warnings_fixed = 0;
    if _fix {
        // Apply fixes for rules that have warnings, regardless of whether individual warnings have fixes
        for rule in rules {
            if all_warnings
                .iter()
                .any(|w| w.rule_name == Some(rule.name()))
            {
                let ctx = LintContext::new(&content);
                match rule.fix(&ctx) {
                    Ok(fixed_content) => {
                        if fixed_content != content {
                            content = fixed_content;
                            // Apply fixes for this rule - we consider all warnings for the rule fixed
                            warnings_fixed += all_warnings
                                .iter()
                                .filter(|w| w.rule_name == Some(rule.name()))
                                .count();
                        }
                    }
                    Err(err) => {
                        if !quiet {
                            eprintln!(
                                "{} Failed to apply fix for rule {}: {}",
                                "Warning:".yellow().bold(),
                                rule.name(),
                                err
                            );
                        }
                    }
                }
            }
        }

        // Write fixed content back to file
        if warnings_fixed > 0 {
            if let Err(err) = std::fs::write(file_path, &content) {
                if !quiet {
                    eprintln!(
                        "{} Failed to write fixed content to file {}: {}",
                        "Error:".red().bold(),
                        file_path,
                        err
                    );
                }
            }
        }
    }

    let lint_end_time = Instant::now();
    let lint_time = lint_end_time.duration_since(lint_start);

    if verbose && !quiet {
        println!("Linting took: {:?}", lint_time);
    }

    let total_time = start_time.elapsed();
    if verbose && !quiet {
        println!("Total processing time for {}: {:?}", file_path, total_time);
    }

    (true, total_warnings, warnings_fixed, fixable_warnings)
}

fn process_file_collect_warnings(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    verbose: bool,
    quiet: bool,
) -> Vec<rumdl::rule::LintWarning> {
    if verbose && !quiet {
        println!("Processing file: {}", file_path);
    }

    // Read file content efficiently
    let content = match read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {}: {}", file_path, e);
            }
            return Vec::new();
        }
    };

    std::env::set_var("RUMDL_FILE_PATH", file_path);
    let warnings_result = rumdl::lint(&content, rules, verbose);
    std::env::remove_var("RUMDL_FILE_PATH");
    let mut all_warnings = warnings_result.unwrap_or_default();
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });
    all_warnings
}
