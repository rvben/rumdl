use clap::{Args, ValueEnum};
use std::ops::{Deref, DerefMut};

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
pub struct SharedCliArgs {
    /// Disable specific rules (comma-separated)
    #[arg(short, long, help = "Disable specific rules (comma-separated)")]
    pub disable: Option<String>,

    /// Enable only specific rules (comma-separated)
    #[arg(
        short,
        long,
        visible_alias = "rules",
        help = "Enable only specific rules (comma-separated)"
    )]
    pub enable: Option<String>,

    /// Extend the list of enabled rules (additive with config)
    #[arg(long, help = "Extend the list of enabled rules (additive with config)")]
    pub extend_enable: Option<String>,

    /// Extend the list of disabled rules (additive with config)
    #[arg(long, help = "Extend the list of disabled rules (additive with config)")]
    pub extend_disable: Option<String>,

    /// Only allow these rules to be fixed (comma-separated)
    #[arg(long, help = "Only allow these rules to be fixed (comma-separated)")]
    pub fixable: Option<String>,

    /// Prevent these rules from being fixed (comma-separated)
    #[arg(long, help = "Prevent these rules from being fixed (comma-separated)")]
    pub unfixable: Option<String>,

    /// Exclude specific files or directories (comma-separated glob patterns)
    #[arg(long, help = "Exclude specific files or directories (comma-separated glob patterns)")]
    pub exclude: Option<String>,

    /// Disable all exclude patterns
    #[arg(long, help = "Disable all exclude patterns")]
    pub no_exclude: bool,

    /// Include only specific files or directories (comma-separated glob patterns)
    #[arg(
        long,
        help = "Include only specific files or directories (comma-separated glob patterns)"
    )]
    pub include: Option<String>,

    /// Respect .gitignore files when scanning directories
    #[arg(
        long,
        num_args(0..=1),
        require_equals(true),
        default_missing_value = "true",
        help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)"
    )]
    pub respect_gitignore: Option<bool>,

    /// Print diagnostics, but suppress summary lines
    #[arg(short, long, help = "Print diagnostics, but suppress summary lines")]
    pub quiet: bool,

    /// Show absolute file paths instead of project-relative paths
    #[arg(long, help = "Show absolute file paths in output instead of relative paths")]
    pub show_full_path: bool,

    /// Filename to use for stdin input (for context and error messages)
    #[arg(long, help = "Filename to use when reading from stdin (e.g., README.md)")]
    pub stdin_filename: Option<String>,

    /// Output diagnostics to stderr instead of stdout
    #[arg(long, help = "Output diagnostics to stderr instead of stdout")]
    pub stderr: bool,

    /// Disable caching (re-check all files)
    #[arg(long, help = "Disable caching (re-check all files)")]
    pub no_cache: bool,

    /// Directory to store cache files
    #[arg(
        long,
        help = "Directory to store cache files (default: .rumdl_cache, or $RUMDL_CACHE_DIR, or cache-dir in config)"
    )]
    pub cache_dir: Option<String>,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Files or directories to check (use '-' for stdin)
    #[arg(required = false)]
    pub paths: Vec<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    pub fix: bool,

    /// Show diff of what would be fixed instead of fixing files
    #[arg(
        long,
        alias = "dry-run",
        help = "Show diff of what would be fixed instead of fixing files"
    )]
    pub diff: bool,

    /// Exit with code 1 if any formatting changes would be made (like rustfmt --check)
    #[arg(
        long,
        hide = true,
        help = "Exit with code 1 if any formatting changes would be made (for CI)"
    )]
    pub check: bool,

    /// List all available rules
    #[arg(short, long, default_value = "false")]
    pub list_rules: bool,

    #[command(flatten)]
    pub shared: SharedCliArgs,

    /// Show detailed output
    #[arg(short, long, help = "Show detailed output")]
    pub verbose: bool,

    /// Show profiling information
    #[arg(long, help = "Show profiling information")]
    pub profile: bool,

    /// Show statistics summary of rule violations
    #[arg(long, help = "Show statistics summary of rule violations")]
    pub statistics: bool,

    /// Legacy alias for --output-format: text (default) or json
    #[arg(long, short = 'o', default_value_t, value_enum, hide = true)]
    pub output: Output,

    /// Output format for diagnostics (default: text).
    ///
    /// Precedence: --output-format > $RUMDL_OUTPUT_FORMAT > config file > text
    #[arg(long, value_enum)]
    pub output_format: Option<OutputFormat>,

    /// Markdown flavor to use for linting
    #[arg(
        long,
        value_enum,
        help = "Markdown flavor to use: standard (also accepts gfm/github/commonmark), mkdocs, mdx, quarto, obsidian, or kramdown"
    )]
    pub flavor: Option<Flavor>,

    /// Read from stdin instead of files
    #[arg(long, help = "Read from stdin instead of files")]
    pub stdin: bool,

    /// Suppress diagnostics and summaries
    #[arg(short, long, help = "Suppress diagnostics and summaries")]
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

#[derive(Args, Debug)]
pub struct FmtArgs {
    /// Files or directories to format (use '-' for stdin)
    #[arg(required = false)]
    pub paths: Vec<String>,

    /// Show diff of what would be formatted instead of rewriting files
    #[arg(
        long,
        alias = "dry-run",
        help = "Show diff of what would be formatted instead of rewriting files"
    )]
    pub diff: bool,

    /// Exit with code 1 if any formatting changes would be made (for CI)
    #[arg(long, help = "Exit with code 1 if any formatting changes would be made (for CI)")]
    pub check: bool,

    /// Hidden compatibility flag from check
    #[arg(short, long, hide = true, default_value = "false")]
    pub list_rules: bool,

    #[command(flatten)]
    pub shared: SharedCliArgs,

    /// Show detailed formatter output
    #[arg(short, long, help = "Show detailed formatter output")]
    pub verbose: bool,

    /// Hidden compatibility flag from check
    #[arg(long, hide = true)]
    pub profile: bool,

    /// Hidden compatibility flag from check
    #[arg(long, hide = true)]
    pub statistics: bool,

    /// Hidden legacy alias for --output-format
    #[arg(long, short = 'o', default_value_t, value_enum, hide = true)]
    pub output: Output,

    /// Output format for remaining diagnostics (default: text).
    ///
    /// Precedence: --output-format > $RUMDL_OUTPUT_FORMAT > config file > text
    #[arg(long, value_enum)]
    pub output_format: Option<OutputFormat>,

    /// Markdown flavor to use while formatting
    #[arg(
        long,
        value_enum,
        help = "Markdown flavor to use while formatting: standard (also accepts gfm/github/commonmark), mkdocs, mdx, quarto, obsidian, or kramdown"
    )]
    pub flavor: Option<Flavor>,

    /// Read Markdown from stdin instead of files
    #[arg(long, help = "Read Markdown from stdin instead of files")]
    pub stdin: bool,

    /// Suppress diagnostics and summaries; only formatted content is emitted in stdin/stdout mode
    #[arg(
        short,
        long,
        help = "Suppress diagnostics and summaries; only formatted content is emitted in stdin/stdout mode"
    )]
    pub silent: bool,

    /// Re-run formatting whenever files change
    #[arg(short, long, help = "Re-run formatting whenever files change")]
    pub watch: bool,

    /// Hidden deprecated compatibility flag from check
    #[arg(long, hide = true)]
    pub force_exclude: bool,

    /// Hidden compatibility flag; fmt always exits with formatter-style semantics
    #[arg(long, value_enum, default_value_t, hide = true)]
    pub fail_on: FailOn,
}

impl From<FmtArgs> for CheckArgs {
    fn from(args: FmtArgs) -> Self {
        Self {
            paths: args.paths,
            // `fmt` activates fixing via `FixMode::Format` set in main, not via this flag.
            // The flag is intentionally `false` so the check-dispatch path does not
            // independently enable `FixMode::CheckFix`.
            fix: false,
            diff: args.diff,
            check: args.check,
            list_rules: args.list_rules,
            shared: args.shared,
            verbose: args.verbose,
            profile: args.profile,
            statistics: args.statistics,
            output: args.output,
            output_format: args.output_format,
            flavor: args.flavor,
            stdin: args.stdin,
            silent: args.silent,
            watch: args.watch,
            force_exclude: args.force_exclude,
            fail_on: args.fail_on,
            fix_mode: FixMode::default(),
            fail_on_mode: FailOn::default(),
        }
    }
}

impl Deref for CheckArgs {
    type Target = SharedCliArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl DerefMut for CheckArgs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shared
    }
}

impl Deref for FmtArgs {
    type Target = SharedCliArgs;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl DerefMut for FmtArgs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shared
    }
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
