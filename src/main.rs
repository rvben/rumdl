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
use memmap2::Mmap;
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_block_utils::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::strong_style::StrongStyle;

use rumdl_config::ConfigSource;
use rumdl_config::normalize_key;

mod file_processor;
mod formatter;
mod stdin_processor;
mod watch;

/// Threshold for using memory-mapped I/O (1MB)
const MMAP_THRESHOLD: u64 = 1024 * 1024;

/// Handle the schema subcommand
fn handle_schema_command(action: SchemaAction) {
    use schemars::schema_for;

    // Generate the schema
    let schema = schema_for!(rumdl_config::Config);
    let schema_json = serde_json::to_string_pretty(&schema).expect("Failed to serialize schema");

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
                fs::write(&schema_path, &schema_json).expect("Failed to write schema file");
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
    let mut current_dir = std::env::current_dir().expect("Failed to get current directory");

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return current_dir.join("rumdl.schema.json");
        }

        if !current_dir.pop() {
            // Reached filesystem root without finding Cargo.toml
            // Fall back to current directory
            return std::env::current_dir()
                .expect("Failed to get current directory")
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
            Commands::Schema { action } => {
                handle_schema_command(action);
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
