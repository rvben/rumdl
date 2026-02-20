use clap::{Args, ValueEnum};

/// Fix mode determines exit code behavior: Check/CheckFix exit 1 on violations, Format exits 0
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FixMode {
    #[default]
    Check,
    CheckFix,
    Format,
}

/// Fail-on mode determines which severity triggers exit code 1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum FailOn {
    /// Exit 1 on any violation (info, warning, or error)
    #[default]
    Any,
    /// Exit 1 on warning or error severity violations
    Warning,
    /// Exit 1 only on error-severity violations
    Error,
    /// Always exit 0
    Never,
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
    #[arg(long, short = 'o', default_value_t, value_enum)]
    pub output: Output,

    /// Output format for linting results (default: text).
    ///
    /// Precedence: --output-format > $RUMDL_OUTPUT_FORMAT > config file > text
    #[arg(long, value_enum)]
    pub output_format: Option<OutputFormat>,

    /// Show absolute file paths instead of project-relative paths
    #[arg(long, help = "Show absolute file paths in output instead of relative paths")]
    pub show_full_path: bool,

    /// Markdown flavor to use for linting
    #[arg(
        long,
        value_enum,
        help = "Markdown flavor: standard/gfm/commonmark (default), mkdocs, mdx, quarto, obsidian, or kramdown"
    )]
    pub flavor: Option<Flavor>,

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
    #[arg(
        long,
        value_enum,
        default_value_t,
        help = "Exit code behavior: 'any' (default) exits 1 on any violation, 'warning' on warning+error, 'error' only on errors, 'never' always exits 0"
    )]
    pub fail_on: FailOn,

    #[arg(skip)]
    pub fix_mode: FixMode,

    #[arg(skip)]
    pub fail_on_mode: FailOn,
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum Output {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// One-line-per-warning with file, line, column, rule, and message (default)
    Text,
    /// Show source lines with caret underlines highlighting the violation
    Full,
    /// Minimal: file:line:col rule message
    Concise,
    /// Warnings grouped by file with a header per file
    Grouped,
    /// JSON array of all warnings (collected across files)
    Json,
    /// One JSON object per warning (streaming)
    JsonLines,
    /// GitHub Actions annotation format (::warning/::error)
    #[value(name = "github")]
    GitHub,
    /// GitLab Code Quality report (JSON)
    #[value(name = "gitlab")]
    GitLab,
    /// Pylint-compatible format
    Pylint,
    /// Azure Pipelines logging commands
    Azure,
    /// SARIF 2.1.0 for static analysis tools
    Sarif,
    /// JUnit XML for CI test reporters
    Junit,
}

impl From<OutputFormat> for rumdl_lib::output::OutputFormat {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Text => Self::Text,
            OutputFormat::Full => Self::Full,
            OutputFormat::Concise => Self::Concise,
            OutputFormat::Grouped => Self::Grouped,
            OutputFormat::Json => Self::Json,
            OutputFormat::JsonLines => Self::JsonLines,
            OutputFormat::GitHub => Self::GitHub,
            OutputFormat::GitLab => Self::GitLab,
            OutputFormat::Pylint => Self::Pylint,
            OutputFormat::Azure => Self::Azure,
            OutputFormat::Sarif => Self::Sarif,
            OutputFormat::Junit => Self::Junit,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Flavor {
    #[value(aliases(["gfm", "github", "commonmark"]))]
    Standard,
    MkDocs,
    #[allow(clippy::upper_case_acronyms)]
    MDX,
    #[value(aliases(["qmd", "rmd", "rmarkdown"]))]
    Quarto,
    Obsidian,
    #[value(alias("jekyll"))]
    Kramdown,
}

impl From<Flavor> for rumdl_lib::config::MarkdownFlavor {
    fn from(flavor: Flavor) -> Self {
        match flavor {
            Flavor::Standard => Self::Standard,
            Flavor::MkDocs => Self::MkDocs,
            Flavor::MDX => Self::MDX,
            Flavor::Quarto => Self::Quarto,
            Flavor::Obsidian => Self::Obsidian,
            Flavor::Kramdown => Self::Kramdown,
        }
    }
}
