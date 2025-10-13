// Use jemalloc for better memory allocation performance on Unix-like systems
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Use mimalloc on Windows for better performance
#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::{Args, Parser, Subcommand};
use colored::*;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use memmap2::Mmap;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::str::FromStr;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_block_utils::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::strong_style::StrongStyle;

use rumdl_config::ConfigSource;
use rumdl_config::normalize_key;

mod formatter;
mod watch;

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
        String::from_utf8(mmap.to_vec()).map_err(|e| format!("Invalid UTF-8 in file {}: {}", path.display(), e).into())
    } else {
        // Use regular reading for small files
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e).into())
    }
}

/// Utility function to load configuration with standard CLI error handling.
/// This eliminates duplication between different CLI commands that load configuration.
fn load_config_with_cli_error_handling(config_path: Option<&str>, isolated: bool) -> rumdl_config::SourcedConfig {
    load_config_with_cli_error_handling_with_dir(config_path, isolated, None)
}

pub fn load_config_with_cli_error_handling_with_dir(
    config_path: Option<&str>,
    isolated: bool,
    discovery_dir: Option<&std::path::Path>,
) -> rumdl_config::SourcedConfig {
    let result = if let Some(dir) = discovery_dir {
        // Temporarily change working directory for config discovery
        let original_dir = std::env::current_dir().ok();

        // Change to the discovery directory if it exists
        if dir.is_dir() {
            let _ = std::env::set_current_dir(dir);
        } else if let Some(parent) = dir.parent() {
            let _ = std::env::set_current_dir(parent);
        }

        let config_result = rumdl_config::SourcedConfig::load_with_discovery(config_path, None, isolated);

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
    #[arg(long, global = true, help = "Path to configuration file")]
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
        /// Rule name or ID (optional)
        rule: Option<String>,
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

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Files or directories to lint (use '-' for stdin)
    #[arg(required = false)]
    paths: Vec<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    pub _fix: bool,

    /// Show diff of what would be fixed instead of fixing files
    #[arg(long, help = "Show diff of what would be fixed instead of fixing files")]
    diff: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false")]
    list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long)]
    disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(short, long)]
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

    /// Show statistics summary of rule violations
    #[arg(long)]
    statistics: bool,

    /// Quiet mode
    #[arg(short, long)]
    quiet: bool,

    /// Output format: text (default) or json
    #[arg(long, short = 'o', default_value = "text")]
    output: String,

    /// Output format for linting results
    #[arg(long, value_parser = ["text", "full", "concise", "grouped", "json", "json-lines", "github", "gitlab", "pylint", "azure", "sarif", "junit"],
          help = "Output format for linting results (text, full, concise, grouped, json, json-lines, github, gitlab, pylint, azure, sarif, junit)")]
    output_format: Option<String>,

    /// Read from stdin instead of files
    #[arg(long, help = "Read from stdin instead of files")]
    stdin: bool,

    /// Filename to use for stdin input (for context and error messages)
    #[arg(long, help = "Filename to use when reading from stdin (e.g., README.md)")]
    stdin_filename: Option<String>,

    /// Output linting results to stderr instead of stdout
    #[arg(long, help = "Output diagnostics to stderr instead of stdout")]
    stderr: bool,

    /// Disable all output except linting results (implies --quiet)
    #[arg(short, long, help = "Disable all output except diagnostics")]
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
}

// Get a complete set of enabled rules based on CLI options and config
fn get_enabled_rules_from_checkargs(args: &CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules using from_config only
    let all_rules: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config);

    // 2. Determine the final list of enabled rules based on precedence
    let final_rules: Vec<Box<dyn Rule>>;

    // Rule names provided via CLI flags
    let cli_enable_set: Option<HashSet<&str>> = args
        .enable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_disable_set: Option<HashSet<&str>> = args
        .disable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_extend_enable_set: Option<HashSet<&str>> = args
        .extend_enable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_extend_disable_set: Option<HashSet<&str>> = args
        .extend_disable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());

    // Rule names provided via config file
    let config_enable_set: HashSet<&str> = config.global.enable.iter().map(|s| s.as_str()).collect();

    let config_disable_set: HashSet<&str> = config.global.disable.iter().map(|s| s.as_str()).collect();

    if let Some(enabled_cli) = &cli_enable_set {
        // CLI --enable completely overrides config (ruff --select behavior)
        let enabled_cli_normalized: HashSet<String> = enabled_cli.iter().map(|s| normalize_key(s)).collect();
        let _all_rule_names: Vec<String> = all_rules.iter().map(|r| normalize_key(r.name())).collect();
        let mut filtered_rules = all_rules
            .into_iter()
            .filter(|rule| enabled_cli_normalized.contains(&normalize_key(rule.name())))
            .collect::<Vec<_>>();

        // Apply CLI --disable to remove rules from the enabled set (ruff-like behavior)
        if let Some(disabled_cli) = &cli_disable_set {
            filtered_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        final_rules = filtered_rules;
    } else if cli_extend_enable_set.is_some() || cli_extend_disable_set.is_some() {
        // Handle extend flags (additive with config)
        let mut current_rules = all_rules;

        // Start with config enable if present
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                config_enable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Add CLI extend-enable rules
        if let Some(extend_enabled_cli) = &cli_extend_enable_set {
            // If we started with all rules (no config enable), keep all rules
            // If we started with config enable, we need to re-filter with extended set
            if !config_enable_set.is_empty() {
                let mut extended_enable_set = config_enable_set.clone();
                for rule in extend_enabled_cli {
                    extended_enable_set.insert(rule);
                }

                // Re-filter with extended set
                current_rules = rumdl_lib::rules::all_rules(config)
                    .into_iter()
                    .filter(|rule| {
                        let normalized_rule_name = normalize_key(rule.name());
                        extended_enable_set.contains(normalized_rule_name.as_str())
                    })
                    .collect();
            }
        }

        // Apply config disable
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                !config_disable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Apply CLI extend-disable
        if let Some(extend_disabled_cli) = &cli_extend_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !extend_disabled_cli.contains(rule_name_upper)
                    && !extend_disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        // Apply CLI disable
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        final_rules = current_rules;
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
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
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

    // Exclude patterns: CLI > Config (but disabled if --no-exclude is set)
    let final_exclude_patterns: Vec<String> = if args.no_exclude {
        Vec::new() // Disable all exclusions
    } else if let Some(cli_exclude) = args.exclude.as_deref() {
        cli_exclude
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        config.global.exclude.clone()
    };

    // Debug: Log exclude patterns
    if args.verbose {
        eprintln!("Exclude patterns: {final_exclude_patterns:?}");
    }
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
                eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
            }
        }

        // Add excludes (these filter *out* files) - MUST start with '!'
        for pattern in &final_exclude_patterns {
            // Ensure exclude patterns start with '!' for ignore crate overrides
            let exclude_rule = if pattern.starts_with('!') {
                pattern.clone() // Already formatted
            } else {
                format!("!{pattern}")
            };
            if let Err(e) = override_builder.add(&exclude_rule) {
                eprintln!("Warning: Invalid exclude pattern '{pattern}': {e}");
            }
        }

        // Build and apply the overrides
        match override_builder.build() {
            Ok(overrides) => {
                walk_builder.overrides(overrides);
            }
            Err(e) => {
                eprintln!("Error building path overrides: {e}");
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
    walk_builder.hidden(false); // Include hidden files and directories
    walk_builder.require_git(false); // Process git ignores even if no repo detected

    // Add support for .markdownlintignore file
    walk_builder.add_custom_ignore_filename(".markdownlintignore");

    // --- Pre-check for explicit file paths ---
    // If not in discovery mode, validate that specified paths exist
    if !is_discovery_mode {
        let mut processed_explicit_files = false;

        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(format!("File not found: {path_str}").into());
            }
            // If it's a file, check if it's a markdown file
            if path.is_file()
                && let Some(ext) = path.extension()
                && (ext == "md" || ext == "markdown")
            {
                processed_explicit_files = true;
                // Convert to relative path for pattern matching
                // This ensures patterns like "docs/*" work with both relative and absolute paths
                let cleaned_path = if path.is_absolute() {
                    // Try to make it relative to the current directory
                    // Use canonicalized paths to handle symlinks (e.g., /tmp -> /private/tmp on macOS)
                    if let Ok(cwd) = std::env::current_dir() {
                        // Canonicalize both paths to resolve symlinks
                        if let (Ok(canonical_cwd), Ok(canonical_path)) = (cwd.canonicalize(), path.canonicalize()) {
                            if let Ok(relative) = canonical_path.strip_prefix(&canonical_cwd) {
                                relative.to_string_lossy().to_string()
                            } else {
                                // Path is absolute but not under cwd, keep as-is
                                path_str.clone()
                            }
                        } else {
                            // Canonicalization failed, keep path as-is
                            path_str.clone()
                        }
                    } else {
                        path_str.clone()
                    }
                } else if let Some(stripped) = path_str.strip_prefix("./") {
                    stripped.to_string()
                } else {
                    path_str.clone()
                };

                // Check if this file should be excluded based on exclude patterns
                // This is the default behavior to match user expectations and avoid
                // duplication between rumdl config and pre-commit config (issue #99)
                if !final_exclude_patterns.is_empty() {
                    let mut matching_pattern: Option<&str> = None;
                    for pattern in &final_exclude_patterns {
                        // Use globset for pattern matching
                        if let Ok(glob) = globset::Glob::new(pattern) {
                            let matcher = glob.compile_matcher();
                            if matcher.is_match(&cleaned_path) {
                                matching_pattern = Some(pattern);
                                break;
                            }
                        }
                    }
                    if let Some(pattern) = matching_pattern {
                        // Always print a warning when excluding explicitly provided files
                        // This matches ESLint's behavior and helps users understand why the file wasn't linted
                        eprintln!(
                            "warning: {cleaned_path} ignored because of exclude pattern '{pattern}'. Use --no-exclude to override"
                        );
                    } else {
                        file_paths.push(cleaned_path);
                    }
                } else {
                    // No exclude patterns, add the file
                    file_paths.push(cleaned_path);
                }
            }
        }

        // If we processed explicit files, return the results (even if empty due to exclusions)
        // This prevents the walker from running when explicit files were provided
        if processed_explicit_files {
            file_paths.sort();
            file_paths.dedup();
            return Ok(file_paths);
        }
    }

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
            Err(err) => {
                // Only show generic walking errors for directories, not for missing files
                if is_discovery_mode {
                    eprintln!("Error walking directory: {err}");
                }
            }
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
        path.extension().is_some_and(|ext| ext == "md" || ext == "markdown")
    });
    // -------------------------------------

    Ok(file_paths) // Ensure the function returns the result
}

/// Offer to install the VS Code extension during init
fn offer_vscode_extension_install() {
    use rumdl_lib::vscode::VsCodeExtension;

    // Check if we're in an integrated terminal
    if let Some((cmd, editor_name)) = VsCodeExtension::current_editor_from_env() {
        println!("\nDetected you're using {}.", editor_name.green());
        println!("Would you like to install the rumdl extension? [Y/n]");
        print!("> ");
        io::stdout().flush().unwrap();

        let mut answer = String::new();
        io::stdin().read_line(&mut answer).unwrap();

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
                print!("> ");
                io::stdout().flush().unwrap();

                let mut answer = String::new();
                io::stdin().read_line(&mut answer).unwrap();

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
                print!("> ");
                io::stdout().flush().unwrap();

                let mut answer = String::new();
                io::stdin().read_line(&mut answer).unwrap();
                let answer = answer.trim().to_lowercase();

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
        match cli.command {
            Commands::Init { pyproject } => {
                if pyproject {
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
                                        println!(
                                            "Please edit the file manually to avoid overwriting existing configuration."
                                        );
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
requires = [\"setuptools>=42\", \"wheel\"]
build-backend = \"setuptools.build_meta\"

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
                }

                // Create default config file
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
            Commands::Check(args) => {
                // If --no-config or --isolated is set, skip config loading
                if cli.no_config || cli.isolated {
                    run_check(&args, None, cli.no_config || cli.isolated);
                } else {
                    run_check(&args, cli.config.as_deref(), cli.no_config || cli.isolated);
                }
            }
            Commands::Fmt(mut args) => {
                // fmt is an alias for check --fix
                args._fix = true;
                // If --no-config or --isolated is set, skip config loading
                if cli.no_config || cli.isolated {
                    run_check(&args, None, cli.no_config || cli.isolated);
                } else {
                    run_check(&args, cli.config.as_deref(), cli.no_config || cli.isolated);
                }
            }
            Commands::Rule { rule } => {
                use rumdl_lib::rules::*;
                let all_rules: Vec<Box<dyn Rule>> = vec![
                    Box::new(MD001HeadingIncrement),
                    Box::new(MD002FirstHeadingH1::default()),
                    Box::new(MD003HeadingStyle::default()),
                    Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
                    Box::new(MD005ListIndent::default()),
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
                    Box::new(MD058BlanksAroundTables::default()),
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
                        eprintln!("Rule '{rule_query}' not found.");
                        exit::tool_error();
                    }
                } else {
                    println!("Available rules:");
                    for rule in &all_rules {
                        println!("  {} - {}", rule.name(), rule.description());
                    }
                }
            }
            Commands::Explain { rule } => {
                handle_explain_command(&rule);
            }
            Commands::Config {
                subcmd,
                defaults,
                output,
            } => {
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

                    // Decide which config to print based on --defaults
                    let final_sourced_to_print = sourced_reg;

                    // If --output toml is set, print as valid TOML
                    if output.as_deref() == Some("toml") {
                        if defaults {
                            // For defaults with TOML output, generate a complete default config
                            let mut default_config = rumdl_config::Config::default();

                            // Add all rule default configurations
                            for rule in &all_rules_reg {
                                if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section() {
                                    let rule_config = rumdl_config::RuleConfig {
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
                        } else {
                            let config_to_print: rumdl_config::Config = final_sourced_to_print.into();
                            match toml::to_string_pretty(&config_to_print) {
                                Ok(s) => println!("{s}"),
                                Err(e) => {
                                    eprintln!("Failed to serialize config to TOML: {e}");
                                    exit::tool_error();
                                }
                            }
                        }
                    } else {
                        // Otherwise, print the smart output with provenance annotations
                        formatter::print_config_with_provenance(&final_sourced_to_print);
                    }
                }
            }
            Commands::Server { port, stdio, verbose } => {
                // Setup logging for the LSP server
                if verbose {
                    env_logger::Builder::from_default_env()
                        .filter_level(log::LevelFilter::Debug)
                        .init();
                } else {
                    env_logger::Builder::from_default_env()
                        .filter_level(log::LevelFilter::Info)
                        .init();
                }

                // Start the LSP server
                let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

                runtime.block_on(async {
                    if let Some(port) = port {
                        // TCP mode for debugging
                        if let Err(e) = rumdl_lib::lsp::start_tcp_server(port).await {
                            eprintln!("Failed to start LSP server on port {port}: {e}");
                            exit::tool_error();
                        }
                    } else {
                        // Standard LSP mode over stdio (default behavior)
                        // Note: stdio flag is for explicit documentation, behavior is the same
                        let _ = stdio; // Suppress unused variable warning
                        if let Err(e) = rumdl_lib::lsp::start_server().await {
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

                // Generate the output
                let output_content = match format.as_str() {
                    "toml" => {
                        // Convert to TOML format
                        let mut output = String::new();

                        // Add global settings if any
                        if !fragment.global.enable.value.is_empty()
                            || !fragment.global.disable.value.is_empty()
                            || !fragment.global.exclude.value.is_empty()
                            || !fragment.global.include.value.is_empty()
                            || fragment.global.line_length.value != 80
                        {
                            output.push_str("[global]\n");
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
                            if fragment.global.line_length.value != 80 {
                                output.push_str(&format!("line_length = {}\n", fragment.global.line_length.value));
                            }
                            output.push('\n');
                        }

                        // Add rule-specific settings
                        for (rule_name, rule_config) in &fragment.rules {
                            if !rule_config.values.is_empty() {
                                output.push_str(&format!("[{rule_name}]\n"));
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
                            || fragment.global.line_length.value != 80
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
                            if fragment.global.line_length.value != 80 {
                                global.insert(
                                    "line_length".to_string(),
                                    serde_json::Value::Number(serde_json::Number::from(
                                        fragment.global.line_length.value,
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

/// Process markdown content from stdin
fn process_stdin(rules: &[Box<dyn Rule>], args: &CheckArgs, config: &rumdl_config::Config) {
    use rumdl_lib::output::{OutputFormat, OutputWriter};

    // If silent mode is enabled, also enable quiet mode
    let quiet = args.quiet || args.silent;

    // In check mode without --fix, diagnostics should go to stderr by default
    // In fix mode, fixed content goes to stdout, so diagnostics also go to stdout unless --stderr is specified
    let use_stderr = if args._fix {
        args.stderr
    } else {
        true // Check mode: diagnostics to stderr by default
    };
    // Create output writer for linting results
    let output_writer = OutputWriter::new(use_stderr, quiet, args.silent);

    // Determine output format
    let output_format_str = args
        .output_format
        .as_deref()
        .or(config.global.output_format.as_deref())
        .or_else(|| {
            // Legacy support: map --output json to --output-format json
            if args.output == "json" { Some("json") } else { None }
        })
        .unwrap_or("text");

    let output_format = match OutputFormat::from_str(output_format_str) {
        Ok(fmt) => fmt,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Read all content from stdin
    let mut content = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut content) {
        if !args.silent {
            eprintln!("Error reading from stdin: {e}");
        }
        exit::violations_found();
    }

    // Detect original line ending before any processing (I/O boundary)
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

    // Determine the filename to use for display and context
    let display_filename = args.stdin_filename.as_deref().unwrap_or("<stdin>");

    // Set RUMDL_FILE_PATH if stdin-filename is provided
    // This allows rules like MD057 to know the file location for relative path checking
    if let Some(ref filename) = args.stdin_filename {
        unsafe {
            std::env::set_var("RUMDL_FILE_PATH", filename);
        }
    }

    // Create a lint context for the stdin content
    let ctx = LintContext::new(&content, config.markdown_flavor());
    let mut all_warnings = Vec::new();

    // Run all enabled rules on the content
    for rule in rules {
        match rule.check(&ctx) {
            Ok(warnings) => {
                all_warnings.extend(warnings);
            }
            Err(e) => {
                if !args.silent {
                    eprintln!("Error running rule {}: {}", rule.name(), e);
                }
            }
        }
    }

    // Sort warnings by line/column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let has_issues = !all_warnings.is_empty();

    // Apply fixes if requested
    if args._fix {
        if has_issues {
            let mut fixed_content = content.clone();
            let warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut fixed_content, quiet, config);

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content = rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending);

            // Output the fixed content to stdout
            print!("{output_content}");

            // Re-check the fixed content to see if any issues remain
            let fixed_ctx = LintContext::new(&fixed_content, config.markdown_flavor());
            let mut remaining_warnings = Vec::new();
            for rule in rules {
                if let Ok(warnings) = rule.check(&fixed_ctx) {
                    remaining_warnings.extend(warnings);
                }
            }

            // Only show diagnostics to stderr if not in quiet mode
            if !quiet && !remaining_warnings.is_empty() {
                let formatter = output_format.create_formatter();
                let formatted = formatter.format_warnings(&remaining_warnings, display_filename);
                eprintln!("{formatted}");
                eprintln!(
                    "\n{} issue(s) fixed, {} issue(s) remaining",
                    warnings_fixed,
                    remaining_warnings.len()
                );
            }

            // Exit with success if all issues were fixed, error if issues remain
            if !remaining_warnings.is_empty() {
                exit::violations_found();
            }
        } else {
            // No issues found, output the original content unchanged
            print!("{content}");
        }

        // Clean up environment variable
        if args.stdin_filename.is_some() {
            unsafe {
                std::env::remove_var("RUMDL_FILE_PATH");
            }
        }
        return;
    }

    // Normal check mode (no fix) - output diagnostics
    // For formats that need collection
    if matches!(
        output_format,
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit
    ) {
        let file_warnings = vec![(display_filename.to_string(), all_warnings)];
        let output = match output_format {
            OutputFormat::Json => rumdl_lib::output::formatters::json::format_all_warnings_as_json(&file_warnings),
            OutputFormat::GitLab => rumdl_lib::output::formatters::gitlab::format_gitlab_report(&file_warnings),
            OutputFormat::Sarif => rumdl_lib::output::formatters::sarif::format_sarif_report(&file_warnings),
            OutputFormat::Junit => rumdl_lib::output::formatters::junit::format_junit_report(&file_warnings, 0),
            _ => unreachable!(),
        };

        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });
    } else {
        // Use formatter for line-by-line output
        let formatter = output_format.create_formatter();
        if !all_warnings.is_empty() {
            let formatted = formatter.format_warnings(&all_warnings, display_filename);
            output_writer.writeln(&formatted).unwrap_or_else(|e| {
                eprintln!("Error writing output: {e}");
            });
        }

        // Print summary if not quiet
        if !quiet {
            if has_issues {
                output_writer
                    .writeln(&format!(
                        "\nFound {} issue(s) in {}",
                        all_warnings.len(),
                        display_filename
                    ))
                    .ok();
            } else {
                output_writer
                    .writeln(&format!("No issues found in {display_filename}"))
                    .ok();
            }
        }
    }

    // Clean up environment variable
    if args.stdin_filename.is_some() {
        unsafe {
            std::env::remove_var("RUMDL_FILE_PATH");
        }
    }

    // Exit with error code if issues found
    if has_issues {
        exit::violations_found();
    }
}

fn run_check(args: &CheckArgs, global_config_path: Option<&str>, isolated: bool) {
    // If silent mode is enabled, also enable quiet mode
    let quiet = args.quiet || args.silent;

    // Validate mutually exclusive options
    if args.diff && args._fix {
        eprintln!("{}: --diff and --fix cannot be used together", "Error".red().bold());
        eprintln!("Use --diff to preview changes, or --fix to apply them");
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
    let sourced = load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

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

    // 4. Convert to Config for the rest of the linter
    let config: rumdl_config::Config = sourced.into();

    // Perform the check and exit if issues were found
    let has_issues = watch::perform_check_run(args, &config, quiet);
    if has_issues {
        exit::violations_found();
    }
}

// Handle explain command
fn handle_explain_command(rule_query: &str) {
    use rumdl_lib::rules::*;

    // Get all rules
    let all_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD002FirstHeadingH1::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
        Box::new(MD005ListIndent::default()),
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
        Box::new(MD058BlanksAroundTables::default()),
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
                println!("  https://github.com/rvben/rumdl/blob/main/docs/{rule_id}.md");
            }
        }
    } else {
        eprintln!("{}: Rule '{}' not found.", "Error".red().bold(), rule_query);
        eprintln!("\nUse 'rumdl rule' to see all available rules.");
        exit::tool_error();
    }
}

// Helper function to check if a rule is actually fixable based on configuration
fn is_rule_actually_fixable(config: &rumdl_config::Config, rule_name: &str) -> bool {
    // Check unfixable list
    if config
        .global
        .unfixable
        .iter()
        .any(|r| r.eq_ignore_ascii_case(rule_name))
    {
        return false;
    }

    // Check fixable list if specified
    if !config.global.fixable.is_empty() {
        return config.global.fixable.iter().any(|r| r.eq_ignore_ascii_case(rule_name));
    }

    true
}

// Process file with output formatter
#[allow(clippy::too_many_arguments)]
fn process_file_with_formatter(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    diff: bool,
    verbose: bool,
    quiet: bool,
    output_format: &rumdl_lib::output::OutputFormat,
    output_writer: &rumdl_lib::output::OutputWriter,
    config: &rumdl_config::Config,
) -> (bool, usize, usize, usize, Vec<rumdl_lib::rule::LintWarning>) {
    let formatter = output_format.create_formatter();

    // Call the original process_file_inner to get warnings and original line ending
    let (all_warnings, mut content, total_warnings, fixable_warnings, original_line_ending) =
        process_file_inner(file_path, rules, verbose, quiet, config);

    if total_warnings == 0 {
        return (false, 0, 0, 0, Vec::new());
    }

    // Format and output warnings
    if !quiet && !_fix {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings(&unfixable_warnings, file_path);
                if !formatted.is_empty() {
                    output_writer.writeln(&formatted).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                }
            }
        } else {
            // In check mode, show all warnings with [*] for fixable issues
            let formatted = formatter.format_warnings(&all_warnings, file_path);
            if !formatted.is_empty() {
                output_writer.writeln(&formatted).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }
    }

    // Handle diff mode or fix mode
    let mut warnings_fixed = 0;
    if diff {
        // In diff mode, apply fixes to a copy and show diff
        let original_content = content.clone();
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, true, config);

        if warnings_fixed > 0 {
            let diff_output = formatter::generate_diff(&original_content, &content, file_path);
            output_writer.writeln(&diff_output).unwrap_or_else(|e| {
                eprintln!("Error writing diff output: {e}");
            });
        }

        // Don't actually write the file in diff mode
        return (total_warnings > 0, total_warnings, 0, fixable_warnings, all_warnings);
    } else if _fix {
        // Apply fixes using Fix Coordinator
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, quiet, config);

        // Write fixed content back to file
        if warnings_fixed > 0 {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending);

            if let Err(err) = std::fs::write(file_path, &content_to_write)
                && !quiet
            {
                eprintln!(
                    "{} Failed to write fixed content to file {}: {}",
                    "Error:".red().bold(),
                    file_path,
                    err
                );
            }
        }

        // In fix mode, show warnings with [fixed] for issues that were fixed
        if !quiet {
            // Re-lint the fixed content to see which warnings remain
            let fixed_ctx = LintContext::new(&content, config.markdown_flavor());
            let mut remaining_warnings = Vec::new();

            for rule in rules {
                if let Ok(rule_warnings) = rule.check(&fixed_ctx) {
                    remaining_warnings.extend(rule_warnings);
                }
            }

            // Create a custom formatter that shows [fixed] instead of [*]
            let mut output = String::new();
            for warning in &all_warnings {
                let rule_name = warning.rule_name.unwrap_or("unknown");

                // Check if the rule is actually fixable based on configuration
                let is_fixable = is_rule_actually_fixable(config, rule_name);

                let was_fixed = warning.fix.is_some()
                    && is_fixable
                    && !remaining_warnings.iter().any(|w| {
                        w.line == warning.line && w.column == warning.column && w.rule_name == warning.rule_name
                    });

                let fix_indicator = if warning.fix.is_some() {
                    if !is_fixable {
                        " [unfixable]".yellow().to_string()
                    } else if was_fixed {
                        " [fixed]".green().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Format: file:line:column: [rule] message [fixed/*/]
                // Use colors similar to TextFormatter
                let line = format!(
                    "{}:{}:{}: {} {}{}",
                    file_path.blue().underline(),
                    warning.line.to_string().cyan(),
                    warning.column.to_string().cyan(),
                    format!("[{rule_name:5}]").yellow(),
                    warning.message,
                    fix_indicator
                );

                output.push_str(&line);
                output.push('\n');
            }

            if !output.is_empty() {
                output.pop(); // Remove trailing newline
                output_writer.writeln(&output).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }
    }

    (true, total_warnings, warnings_fixed, fixable_warnings, all_warnings)
}

// Inner processing logic that returns warnings
fn process_file_inner(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    config: &rumdl_config::Config,
) -> (
    Vec<rumdl_lib::rule::LintWarning>,
    String,
    usize,
    usize,
    rumdl_lib::utils::LineEnding,
) {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        println!("Processing file: {file_path}");
    }

    // Read file content efficiently
    let mut content = match read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return (Vec::new(), String::new(), 0, 0, rumdl_lib::utils::LineEnding::Lf);
        }
    };

    // Detect original line ending before any processing
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return (Vec::new(), String::new(), 0, 0, original_line_ending);
    }

    let lint_start = Instant::now();
    // Set the environment variable for the file path
    // This allows rules like MD057 to know which file is being processed
    unsafe {
        std::env::set_var("RUMDL_FILE_PATH", file_path);
    }

    // Filter rules based on per-file-ignores configuration
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    // Use the standard lint function with the configured flavor
    let warnings_result = rumdl_lib::lint(&content, &filtered_rules, verbose, config.markdown_flavor());

    // Clear the environment variable after processing
    unsafe {
        std::env::remove_var("RUMDL_FILE_PATH");
    }

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

    // Count fixable issues (excluding unfixable rules)
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| w.fix.is_some() && w.rule_name.is_some_and(|name| is_rule_actually_fixable(config, name)))
        .count();

    let lint_end_time = Instant::now();
    let lint_time = lint_end_time.duration_since(lint_start);

    if verbose && !quiet {
        println!("Linting took: {lint_time:?}");
    }

    let total_time = start_time.elapsed();
    if verbose && !quiet {
        println!("Total processing time for {file_path}: {total_time:?}");
    }

    (
        all_warnings,
        content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
    )
}

// Apply fixes using the new Fix Coordinator (when enabled)
fn apply_fixes_coordinated(
    rules: &[Box<dyn Rule>],
    all_warnings: &[rumdl_lib::rule::LintWarning],
    content: &mut String,
    quiet: bool,
    config: &rumdl_config::Config,
) -> usize {
    use rumdl_lib::fix_coordinator::FixCoordinator;
    use std::time::Instant;

    let start = Instant::now();
    let coordinator = FixCoordinator::new();

    // Apply fixes iteratively (up to 10 iterations to ensure convergence)
    match coordinator.apply_fixes_iterative(rules, all_warnings, content, config, 10) {
        Ok((rules_applied, iterations, ctx_creations, fixed_rule_names)) => {
            let elapsed = start.elapsed();

            if std::env::var("RUMDL_DEBUG_FIX_PERF").is_ok() {
                eprintln!("DEBUG: Fix Coordinator used");
                eprintln!("DEBUG: Iterations: {iterations}");
                eprintln!("DEBUG: Rules applied: {rules_applied}");
                eprintln!("DEBUG: LintContext creations: {ctx_creations}");
                eprintln!("DEBUG: Total time: {elapsed:?}");
            }

            // Count warnings for the rules that were successfully applied
            all_warnings
                .iter()
                .filter(|w| w.rule_name.map(|name| fixed_rule_names.contains(name)).unwrap_or(false))
                .count()
        }
        Err(e) => {
            if !quiet {
                eprintln!("Warning: Fix coordinator failed: {e}");
            }
            0
        }
    }
}

fn process_file_collect_warnings(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    verbose: bool,
    quiet: bool,
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    if verbose && !quiet {
        println!("Processing file: {file_path}");
    }

    // Read file content efficiently
    let content = match read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return Vec::new();
        }
    };

    // Filter rules based on per-file-ignores configuration
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    unsafe {
        std::env::set_var("RUMDL_FILE_PATH", file_path);
    }
    let warnings_result = rumdl_lib::lint(&content, &filtered_rules, verbose, config.markdown_flavor());
    unsafe {
        std::env::remove_var("RUMDL_FILE_PATH");
    }
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
