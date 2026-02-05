use clap::Args;

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
    pub paths: Vec<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    pub fix: bool,

    /// Show diff of what would be fixed instead of fixing files
    #[arg(long, help = "Show diff of what would be fixed instead of fixing files")]
    pub diff: bool,

    /// Exit with code 1 if any formatting changes would be made (like rustfmt --check)
    #[arg(long, help = "Exit with code 1 if any formatting changes would be made (for CI)")]
    pub check: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false")]
    pub list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long)]
    pub disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(short, long, visible_alias = "rules")]
    pub enable: Option<String>,

    /// Extend the list of enabled rules (additive with config)
    #[arg(long)]
    pub extend_enable: Option<String>,

    /// Extend the list of disabled rules (additive with config)
    #[arg(long)]
    pub extend_disable: Option<String>,

    /// Exclude specific files or directories (comma-separated glob patterns)
    #[arg(long)]
    pub exclude: Option<String>,

    /// Disable all exclude patterns (lint all files regardless of exclude configuration)
    #[arg(long, help = "Disable all exclude patterns")]
    pub no_exclude: bool,

    /// Include only specific files or directories (comma-separated glob patterns).
    #[arg(long)]
    pub include: Option<String>,

    /// Respect .gitignore files when scanning directories
    /// When not specified, uses config file value (default: true)
    #[arg(
        long,
        num_args(0..=1),
        require_equals(true),
        default_missing_value = "true",
        help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)"
    )]
    pub respect_gitignore: Option<bool>,

    /// Show detailed output
    #[arg(short, long)]
    pub verbose: bool,

    /// Show profiling information
    #[arg(long)]
    pub profile: bool,

    /// Show statistics summary of rule violations
    #[arg(long)]
    pub statistics: bool,

    /// Print diagnostics, but nothing else
    #[arg(short, long, help = "Print diagnostics, but nothing else")]
    pub quiet: bool,

    /// Output format: text (default) or json
    #[arg(long, short = 'o', default_value = "text")]
    pub output: String,

    /// Output format for linting results
    #[arg(long, value_parser = ["text", "full", "concise", "grouped", "json", "json-lines", "github", "gitlab", "pylint", "azure", "sarif", "junit"],
          help = "Output format (default: text, or $RUMDL_OUTPUT_FORMAT, or output-format in config)")]
    pub output_format: Option<String>,

    /// Show absolute file paths instead of project-relative paths
    #[arg(long, help = "Show absolute file paths in output instead of relative paths")]
    pub show_full_path: bool,

    /// Markdown flavor to use for linting
    #[arg(long, value_parser = ["standard", "mkdocs", "mdx", "quarto", "obsidian"],
          help = "Markdown flavor: standard (default), mkdocs, mdx, quarto, or obsidian")]
    pub flavor: Option<String>,

    /// Read from stdin instead of files
    #[arg(long, help = "Read from stdin instead of files")]
    pub stdin: bool,

    /// Filename to use for stdin input (for context and error messages)
    #[arg(long, help = "Filename to use when reading from stdin (e.g., README.md)")]
    pub stdin_filename: Option<String>,

    /// Output linting results to stderr instead of stdout
    #[arg(long, help = "Output diagnostics to stderr instead of stdout")]
    pub stderr: bool,

    /// Disable all logging (but still exit with status code upon detecting diagnostics)
    #[arg(
        short,
        long,
        help = "Disable all logging (but still exit with status code upon detecting diagnostics)"
    )]
    pub silent: bool,

    /// Run in watch mode by re-running whenever files change
    #[arg(short, long, help = "Run in watch mode by re-running whenever files change")]
    pub watch: bool,

    /// Enforce exclude patterns even for paths that are passed explicitly.
    /// By default, rumdl will lint any paths passed in directly, even if they would typically be excluded.
    /// Setting this flag will cause rumdl to respect exclusions unequivocally.
    /// This is useful for pre-commit, which explicitly passes all changed files.
    #[arg(long, help = "Enforce exclude patterns even for explicitly specified files")]
    pub force_exclude: bool,

    /// Disable caching of lint results
    #[arg(long, help = "Disable caching (re-check all files)")]
    pub no_cache: bool,

    /// Directory to store cache files
    #[arg(
        long,
        help = "Directory to store cache files (default: .rumdl_cache, or $RUMDL_CACHE_DIR, or cache-dir in config)"
    )]
    pub cache_dir: Option<String>,

    /// Control when to exit with code 1: any (default), warning, error, or never
    #[arg(long, value_parser = ["any", "warning", "error", "never"], default_value = "any",
          help = "Exit code behavior: 'any' (default) exits 1 on any violation, 'warning' on warning+error, 'error' only on errors, 'never' always exits 0")]
    pub fail_on: String,

    #[arg(skip)]
    pub fix_mode: FixMode,

    #[arg(skip)]
    pub fail_on_mode: FailOn,
}
