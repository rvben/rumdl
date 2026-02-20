// Use jemalloc for better memory allocation performance on Unix-like systems
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Use mimalloc on Windows for better performance
#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod cli_types;
pub use cli_types::{CheckArgs, FailOn, FixMode};

mod cli_utils;
pub use cli_utils::{apply_cli_overrides, load_config_with_cli_error_handling_with_dir, read_file_efficiently};

mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::shells::Shell;
use core::error::Error;

use rumdl_lib::exit_codes::exit;

mod cache;
mod file_processor;
mod formatter;
mod resolution;
mod stdin_processor;
mod watch;

#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Control colored output
    #[arg(long, global = true, default_value_t, value_enum)]
    color: Color,

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
pub enum SchemaAction {
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
        /// Output format
        #[arg(long, short = 'o', value_name = "FORMAT", default_value_t, value_enum)]
        output_format: commands::rule::OutputFormat,
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
        /// Output format
        #[arg(long, default_value_t, value_enum)]
        format: commands::import::Format,
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
pub enum ConfigSubcommand {
    /// Query a specific config key (e.g. global.exclude or MD013.line_length)
    Get { key: String },
    /// Show the absolute path of the configuration file that was loaded
    File,
}

#[derive(Clone, Default, ValueEnum)]
enum Color {
    #[default]
    Auto,
    Always,
    Never,
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
    match cli.color {
        Color::Always => colored::control::set_override(true),
        Color::Never => colored::control::set_override(false),
        Color::Auto => colored::control::unset_override(),
    }

    // Catch panics and print a message, exit 1
    let result = std::panic::catch_unwind(|| {
        match cli.command {
            Commands::Init { pyproject } => {
                commands::init::handle_init(pyproject);
            }
            Commands::Check(mut args) => {
                args.fix_mode = if args.fix { FixMode::CheckFix } else { FixMode::Check };
                args.fail_on_mode = args.fail_on;

                let config_path = if cli.no_config || cli.isolated {
                    None
                } else {
                    cli.config.as_deref()
                };
                commands::check::run_check(&args, config_path, cli.no_config || cli.isolated);
            }
            Commands::Fmt(mut args) => {
                args.fix_mode = FixMode::Format;
                args.fail_on_mode = args.fail_on;

                // --check mode enables diff (don't write files) and will exit 1 if changes needed
                if args.check {
                    args.diff = true;
                }

                let config_path = if cli.no_config || cli.isolated {
                    None
                } else {
                    cli.config.as_deref()
                };
                commands::check::run_check(&args, config_path, cli.no_config || cli.isolated);
            }
            Commands::Rule {
                rule,
                output_format,
                fixable,
                category,
                explain,
                list_categories,
            } => {
                commands::rule::handle_rule(rule, output_format, fixable, category, explain, list_categories);
            }
            Commands::Explain { rule } => {
                commands::explain::handle_explain(&rule);
            }
            Commands::Config {
                subcmd,
                defaults,
                no_defaults,
                output,
            } => {
                commands::config::handle_config(
                    subcmd,
                    defaults,
                    no_defaults,
                    output,
                    cli.config.as_deref(),
                    cli.no_config,
                    cli.isolated,
                );
            }
            Commands::Schema { action } => {
                commands::schema::handle_schema(action);
            }
            Commands::Server {
                port,
                stdio,
                verbose,
                config,
            } => {
                commands::server::handle_server(port, stdio, verbose, config);
            }
            Commands::Import {
                file,
                output,
                format,
                dry_run,
            } => {
                commands::import::handle_import(file, output, format, dry_run);
            }
            Commands::Vscode { force, update, status } => {
                commands::vscode::handle_vscode(force, update, status);
            }
            Commands::Completions { shell, list } => {
                commands::completions::handle_completions(shell, list);
            }
            Commands::Clean => {
                commands::clean::handle_clean(cli.config.as_deref(), cli.no_config, cli.isolated);
            }
            Commands::Version => {
                commands::version::handle_version();
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
