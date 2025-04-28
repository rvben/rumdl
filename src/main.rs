use clap::{Args, Parser, Subcommand};
use colored::*;
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::time::Instant;

use rumdl::config as rumdl_config;
use rumdl::rule::Rule;
use rumdl::rules::code_block_utils::CodeBlockStyle;
use rumdl::rules::code_fence_utils::CodeFenceStyle;
use rumdl::rules::strong_style::StrongStyle;
use rumdl::rules::*;
use rumdl::{MD046CodeBlockStyle, MD048CodeFenceStyle, MD049EmphasisStyle, MD050StrongStyle};
use rumdl_config::get_rule_config_value;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Legacy: allow positional paths for backwards compatibility
    #[arg(required = false, hide = true)]
    paths: Vec<String>,

    /// Configuration file path
    #[arg(short, long, hide = true)]
    config: Option<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false", hide = true)]
    fix: bool,

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
    },
    /// Show version information
    Version,
}

#[derive(Subcommand)]
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

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Fix issues automatically where possible
    #[arg(short, long, default_value = "false")]
    fix: bool,

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
}

// Helper function to apply configuration to rules that need it
fn apply_rule_configs(rules: &mut Vec<Box<dyn Rule>>, config: &rumdl_config::Config) {
    // Replace any rules that need configuration with properly configured instances

    // Replace MD013 with configured instance
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length =
            get_rule_config_value::<usize>(config, "MD013", "line_length").unwrap_or(80);
        let code_blocks =
            get_rule_config_value::<bool>(config, "MD013", "code_blocks").unwrap_or(true);
        let tables = get_rule_config_value::<bool>(config, "MD013", "tables").unwrap_or(false);
        let headings = get_rule_config_value::<bool>(config, "MD013", "headings").unwrap_or(true);
        let strict = get_rule_config_value::<bool>(config, "MD013", "strict").unwrap_or(false);

        rules[pos] = Box::new(MD013LineLength::new(
            line_length,
            code_blocks,
            tables,
            headings,
            strict,
        ));
    }

    // Replace MD043 with configured instance
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD043") {
        let mut headings =
            get_rule_config_value::<Vec<String>>(config, "MD043", "headings").unwrap_or_default();

        // Strip leading '#' and spaces from the configured headings to match the format of extracted headings
        headings = headings
            .iter()
            .map(|h| h.trim_start_matches(['#', ' ']).to_string())
            .collect();

        rules[pos] = Box::new(MD043RequiredHeadings::new(headings));
    }

    // Replace MD053 with configured instance
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD053") {
        let ignored_definitions =
            get_rule_config_value::<Vec<String>>(config, "MD053", "ignored_definitions")
                .unwrap_or_default();
        rules[pos] = Box::new(MD053LinkImageReferenceDefinitions::new(ignored_definitions));
    }

    // Add more rule configurations as needed
}

// Get a complete set of enabled rules based on CLI options and config
fn get_enabled_rules_from_checkargs(
    args: &CheckArgs,
    config: &rumdl_config::Config,
) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules
    let mut all_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD002FirstHeadingH1::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD005ListIndent),
        Box::new(MD006StartBullets),
        Box::new(MD007ULIndent::default()),
        Box::new(MD008ULStyle::default()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD010NoHardTabs::default()),
        Box::new(MD011NoReversedLinks {}),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD015NoMissingSpaceAfterListMarker::default()),
        Box::new(MD016NoMultipleSpaceAfterListMarker::default()),
        Box::new(MD017NoEmphasisAsHeading),
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
        Box::new(MD029OrderedListMarker::default()),
        Box::new(MD030ListMarkerSpace::default()),
        Box::new(MD031BlanksAroundFences {}),
        Box::new(MD032BlanksAroundLists {}),
        Box::new(MD033NoInlineHtml::default()),
        Box::new(MD034NoBareUrls {}),
        Box::new(MD035HRStyle::default()),
        Box::new(MD036NoEmphasisOnlyFirst {}),
        Box::new(MD037SpacesAroundEmphasis),
        Box::new(MD038NoSpaceInCode::default()),
        Box::new(MD039NoSpaceInLinks),
        Box::new(MD040FencedCodeLanguage {}),
        Box::new(MD041FirstLineHeading::default()),
        Box::new(MD042NoEmptyLinks::new()),
        Box::new(MD043RequiredHeadings::new(Vec::new())),
        Box::new(MD044ProperNames::new(Vec::new(), true)),
        Box::new(MD045NoAltText::new()),
        Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
        Box::new(MD047FileEndNewline),
        Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
        Box::new(MD049EmphasisStyle::default()),
        Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
        Box::new(MD051LinkFragments),
        Box::new(MD052ReferenceLinkImages),
        Box::new(MD053LinkImageReferenceDefinitions::new(Vec::new())),
        Box::new(MD054LinkImageStyle::default()),
        Box::new(MD055TablePipeStyle::default()),
        Box::new(MD056TableColumnCount),
        Box::new(MD057ExistingRelativeLinks::default()),
        Box::new(MD058BlanksAroundTables),
    ];

    // 2. Apply specific rule parameter configurations (e.g., line length for MD013)
    // This modifies the instances within all_rules
    apply_rule_configs(&mut all_rules, config);

    // 3. Determine the final list of enabled rules based on precedence
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
        // --- Case 1: CLI --enable provided ---
        // CLI --enable overrides *everything* (config enable/disable are ignored).
        // Filter all_rules directly based on CLI --enable.
        final_rules = all_rules
            .into_iter()
            .filter(|rule| enabled_cli.contains(rule.name()))
            .collect();
        // Note: CLI --disable is IGNORED if CLI --enable is present.
    } else {
        // --- Case 2: No CLI --enable ---
        // Start with all (already configured) rules.
        let mut enabled_rules = all_rules;

        // Step 2a: Apply config `enable` (if specified).
        // If config.enable is not empty, it acts as an *exclusive* list.
        if !config_enable_set.is_empty() {
            enabled_rules.retain(|rule| config_enable_set.contains(rule.name()));
        }

        // Step 2b: Apply config `disable`.
        // Remove rules specified in config.disable from the current set.
        if !config_disable_set.is_empty() {
            enabled_rules.retain(|rule| !config_disable_set.contains(rule.name()));
        }

        // Step 2c: Apply CLI `disable`.
        // Remove rules specified in cli.disable from the result of steps 2a & 2b.
        if let Some(disabled_cli) = &cli_disable_set {
            enabled_rules.retain(|rule| !disabled_cli.contains(rule.name()));
        }

        final_rules = enabled_rules;
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

    // Determine if running in discovery mode (e.g., "rumdl ." vs explicit path mode
    let is_discovery_mode = paths.len() == 1 && paths[0] == ".";

    // Determine effective patterns based on CLI overrides
    let exclude_patterns = if let Some(exclude_str) = args.exclude.as_deref() {
        // If CLI exclude is given, IT REPLACES config excludes
        exclude_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        // Otherwise, use config excludes
        config.global.exclude.clone() // Already Vec<String>
    };

    let include_patterns = if is_discovery_mode {
        if let Some(include_str) = args.include.as_deref() {
            // If CLI include is given, IT REPLACES config includes
            include_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            // Otherwise, use config includes
            config.global.include.clone() // Already Vec<String>
        }
    } else {
        // Includes are ignored if explicit paths are given (not discovery mode)
        Vec::new()
    };

    // --- Determine Effective Include Patterns ---
    let default_markdown_patterns = vec!["*.md".to_string(), "*.markdown".to_string()];

    let effective_include_patterns = if is_discovery_mode {
        if include_patterns.is_empty() {
            // Discovery mode, no user includes: Use defaults
            default_markdown_patterns
        } else {
            // Discovery mode, user includes provided: Use user's patterns.
            // The type filter added earlier already restricts to MD files.
            // If user includes non-MD, type filter handles it.
            // If user includes specific MD, this override refines it.
            include_patterns
        }
    } else {
        // Explicit path mode: Include patterns are ignored, rely on type filter + explicit paths.
        // Return empty vec here, as overrides shouldn't add includes in this mode.
        Vec::new()
    };

    // Apply overrides from effective patterns
    let has_include_patterns = !effective_include_patterns.is_empty(); // Use effective patterns
    let has_exclude_patterns = !exclude_patterns.is_empty();

    if has_include_patterns || has_exclude_patterns {
        // Initialize OverrideBuilder
        let mut override_builder = OverrideBuilder::new("."); // Root context for patterns

        // Add includes first (using effective patterns)
        if has_include_patterns {
            for pattern in &effective_include_patterns {
                // Use effective patterns
                if let Err(e) = override_builder.add(pattern) {
                    eprintln!("[Effective] Warning: Invalid include pattern '{pattern}': {e}");
                }
            }
        }
        // Add excludes second (as ignore rules !pattern)
        if has_exclude_patterns {
            for pattern in &exclude_patterns {
                let exclude_rule = format!("!{}", pattern);
                if let Err(e) = override_builder.add(&exclude_rule) {
                    // Log the original pattern, not the modified rule
                    eprintln!("[Effective] Warning: Invalid exclude pattern '{pattern}': {e}");
                }
            }
        }
        // Build and apply the overrides
        let overrides = override_builder.build()?;
        walk_builder.overrides(overrides);
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
        if args.fix && total_issues_fixed > 0 {
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

            if !args.fix && total_fixable_issues > 0 {
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

fn to_toml_string_vec_string(v: &[String]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|s| format!("{:?}", s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn main() {
    let _timer = rumdl::profiling::ScopedTimer::new("main");

    let cli = Cli::parse();

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
                                    return;
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
                return;
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
            run_check(args);
        }
        Some(Commands::Rule { rule }) => {
            use rumdl::rules::*;
            let all_rules: Vec<Box<dyn Rule>> = vec![
                Box::new(MD001HeadingIncrement),
                Box::new(MD002FirstHeadingH1::default()),
                Box::new(MD003HeadingStyle::default()),
                Box::new(MD004UnorderedListStyle::default()),
                Box::new(MD005ListIndent),
                Box::new(MD006StartBullets),
                Box::new(MD007ULIndent::default()),
                Box::new(MD008ULStyle::default()),
                Box::new(MD009TrailingSpaces::default()),
                Box::new(MD010NoHardTabs::default()),
                Box::new(MD011NoReversedLinks {}),
                Box::new(MD012NoMultipleBlanks::default()),
                Box::new(MD013LineLength::default()),
                Box::new(MD015NoMissingSpaceAfterListMarker::default()),
                Box::new(MD016NoMultipleSpaceAfterListMarker::default()),
                Box::new(MD017NoEmphasisAsHeading),
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
                Box::new(MD029OrderedListMarker::default()),
                Box::new(MD030ListMarkerSpace::default()),
                Box::new(MD031BlanksAroundFences {}),
                Box::new(MD032BlanksAroundLists {}),
                Box::new(MD033NoInlineHtml::default()),
                Box::new(MD034NoBareUrls {}),
                Box::new(MD035HRStyle::default()),
                Box::new(MD036NoEmphasisOnlyFirst {}),
                Box::new(MD037SpacesAroundEmphasis),
                Box::new(MD038NoSpaceInCode::default()),
                Box::new(MD039NoSpaceInLinks),
                Box::new(MD040FencedCodeLanguage {}),
                Box::new(MD041FirstLineHeading::default()),
                Box::new(MD042NoEmptyLinks::new()),
                Box::new(MD043RequiredHeadings::new(Vec::new())),
                Box::new(MD044ProperNames::new(Vec::new(), true)),
                Box::new(MD045NoAltText::new()),
                Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
                Box::new(MD047FileEndNewline),
                Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
                Box::new(MD049EmphasisStyle::default()),
                Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
                Box::new(MD051LinkFragments),
                Box::new(MD052ReferenceLinkImages),
                Box::new(MD053LinkImageReferenceDefinitions::new(Vec::new())),
                Box::new(MD054LinkImageStyle::default()),
                Box::new(MD055TablePipeStyle::default()),
                Box::new(MD056TableColumnCount),
                Box::new(MD057ExistingRelativeLinks::default()),
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
        Some(Commands::Config { subcmd, defaults }) => {
            if let Some(ConfigSubcommand::Get { key }) = subcmd {
                // Support keys like global.exclude or MD013.line_length
                let sourced =
                    rumdl_config::SourcedConfig::load_sourced_config(cli.config.as_deref(), None);
                let parts: Vec<&str> = key.split('.').collect();
                if parts.len() == 2 {
                    let (section, field) = (parts[0], parts[1]);
                    if section.eq_ignore_ascii_case("global") {
                        // Global key
                        match field {
                            "enable" => {
                                let val = &sourced.global.enable.value;
                                let src_str_colored = match sourced.global.enable.source {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = to_toml_string_vec_string(val).yellow();
                                let key_str = "global.enable".cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                            "disable" => {
                                let val = &sourced.global.disable.value;
                                let src_str_colored = match sourced.global.disable.source {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = to_toml_string_vec_string(val).yellow();
                                let key_str = "global.disable".cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                            "exclude" => {
                                let val = &sourced.global.exclude.value;
                                let src_str_colored = match sourced.global.exclude.source {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = to_toml_string_vec_string(val).yellow();
                                let key_str = "global.exclude".cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                            "include" => {
                                let val = &sourced.global.include.value;
                                let src_str_colored = match sourced.global.include.source {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = to_toml_string_vec_string(val).yellow();
                                let key_str = "global.include".cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                            "respect_gitignore" => {
                                let val = sourced.global.respect_gitignore.value;
                                let src_str_colored = match sourced.global.respect_gitignore.source
                                {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = val.to_string().yellow();
                                let key_str = "global.respect_gitignore".cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                            _ => {
                                eprintln!("Unknown global key: {}", field);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Rule key
                        let rule_name = section;
                        let rule_cfg = sourced.rules.get(rule_name);
                        if let Some(rule_cfg) = rule_cfg {
                            if let Some(sv) = rule_cfg.values.get(field) {
                                let src_str_colored = match sv.source {
                                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                                    rumdl_config::ConfigSource::PyprojectToml => {
                                        "pyproject.toml".magenta()
                                    }
                                    rumdl_config::ConfigSource::Default => "default".yellow(),
                                };
                                let value_str = match &sv.value {
                                    toml::Value::Array(arr) => {
                                        let vals: Vec<String> =
                                            arr.iter().map(|v| v.to_string()).collect();
                                        format!("[{}]", vals.join(", ")).yellow()
                                    }
                                    toml::Value::String(s) => format!("\"{}\"", s).yellow(),
                                    toml::Value::Boolean(b) => b.to_string().yellow(),
                                    toml::Value::Integer(i) => i.to_string().yellow(),
                                    toml::Value::Float(f) => f.to_string().yellow(),
                                    _ => sv.value.to_string().yellow(),
                                };
                                let key_str = format!("{}.{}", rule_name, field).cyan();
                                println!("{} = {} [from {}]", key_str, value_str, src_str_colored);
                                return;
                            }
                        }
                        eprintln!("Unknown rule key: {}.{}", rule_name, field);
                        std::process::exit(1);
                    }
                } else {
                    eprintln!("Key must be in the form global.key or MDxxx.key");
                    std::process::exit(1);
                }
            }
            if *defaults {
                use rumdl::rules::*;
                use std::collections::BTreeMap;
                use toml::Value;
                // Build default global config
                let global = Value::try_from(rumdl_config::GlobalConfig::default())
                    .unwrap_or(Value::Table(Default::default()));
                // Build default rule configs
                let mut rule_map = BTreeMap::new();
                let all_rules: Vec<Box<dyn Rule>> = vec![
                    Box::new(MD001HeadingIncrement),
                    Box::new(MD002FirstHeadingH1::default()),
                    Box::new(MD003HeadingStyle::default()),
                    Box::new(MD004UnorderedListStyle::default()),
                    Box::new(MD005ListIndent),
                    Box::new(MD006StartBullets),
                    Box::new(MD007ULIndent::default()),
                    Box::new(MD008ULStyle::default()),
                    Box::new(MD009TrailingSpaces::default()),
                    Box::new(MD010NoHardTabs::default()),
                    Box::new(MD011NoReversedLinks {}),
                    Box::new(MD012NoMultipleBlanks::default()),
                    Box::new(MD013LineLength::default()),
                    Box::new(MD015NoMissingSpaceAfterListMarker::default()),
                    Box::new(MD016NoMultipleSpaceAfterListMarker::default()),
                    Box::new(MD017NoEmphasisAsHeading),
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
                    Box::new(MD029OrderedListMarker::default()),
                    Box::new(MD030ListMarkerSpace::default()),
                    Box::new(MD031BlanksAroundFences {}),
                    Box::new(MD032BlanksAroundLists {}),
                    Box::new(MD033NoInlineHtml::default()),
                    Box::new(MD034NoBareUrls {}),
                    Box::new(MD035HRStyle::default()),
                    Box::new(MD036NoEmphasisOnlyFirst {}),
                    Box::new(MD037SpacesAroundEmphasis),
                    Box::new(MD038NoSpaceInCode::default()),
                    Box::new(MD039NoSpaceInLinks),
                    Box::new(MD040FencedCodeLanguage {}),
                    Box::new(MD041FirstLineHeading::default()),
                    Box::new(MD042NoEmptyLinks::new()),
                    Box::new(MD043RequiredHeadings::new(Vec::new())),
                    Box::new(MD044ProperNames::new(Vec::new(), true)),
                    Box::new(MD045NoAltText::new()),
                    Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
                    Box::new(MD047FileEndNewline),
                    Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
                    Box::new(MD049EmphasisStyle::default()),
                    Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
                    Box::new(MD051LinkFragments),
                    Box::new(MD052ReferenceLinkImages),
                    Box::new(MD053LinkImageReferenceDefinitions::new(Vec::new())),
                    Box::new(MD054LinkImageStyle::default()),
                    Box::new(MD055TablePipeStyle::default()),
                    Box::new(MD056TableColumnCount),
                    Box::new(MD057ExistingRelativeLinks::default()),
                    Box::new(MD058BlanksAroundTables),
                ];
                for rule in all_rules {
                    if let Some((name, value)) = rule.default_config_section() {
                        rule_map.insert(name, value);
                    }
                }
                // Print [global] section first
                let mut global_table = toml::map::Map::new();
                global_table.insert("global".to_string(), global);
                println!("{}", toml::to_string_pretty(&Value::Table(global_table)).unwrap());
                // Then print each rule section
                let mut rule_names: Vec<_> = rule_map.keys().collect();
                rule_names.sort();
                for rule_name in rule_names {
                    let mut rule_table = toml::map::Map::new();
                    rule_table.insert(rule_name.clone(), rule_map[rule_name].clone());
                    println!("{}", toml::to_string_pretty(&Value::Table(rule_table)).unwrap());
                }
                return;
            }
            // Start with default global config
            use rumdl::rules::*;
            use std::collections::BTreeMap;
            use toml::Value;
            let mut rule_map = BTreeMap::new();
            let all_rules: Vec<Box<dyn Rule>> = vec![
                Box::new(MD001HeadingIncrement),
                Box::new(MD002FirstHeadingH1::default()),
                Box::new(MD003HeadingStyle::default()),
                Box::new(MD004UnorderedListStyle::default()),
                Box::new(MD005ListIndent),
                Box::new(MD006StartBullets),
                Box::new(MD007ULIndent::default()),
                Box::new(MD008ULStyle::default()),
                Box::new(MD009TrailingSpaces::default()),
                Box::new(MD010NoHardTabs::default()),
                Box::new(MD011NoReversedLinks {}),
                Box::new(MD012NoMultipleBlanks::default()),
                Box::new(MD013LineLength::default()),
                Box::new(MD015NoMissingSpaceAfterListMarker::default()),
                Box::new(MD016NoMultipleSpaceAfterListMarker::default()),
                Box::new(MD017NoEmphasisAsHeading),
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
                Box::new(MD029OrderedListMarker::default()),
                Box::new(MD030ListMarkerSpace::default()),
                Box::new(MD031BlanksAroundFences {}),
                Box::new(MD032BlanksAroundLists {}),
                Box::new(MD033NoInlineHtml::default()),
                Box::new(MD034NoBareUrls {}),
                Box::new(MD035HRStyle::default()),
                Box::new(MD036NoEmphasisOnlyFirst {}),
                Box::new(MD037SpacesAroundEmphasis),
                Box::new(MD038NoSpaceInCode::default()),
                Box::new(MD039NoSpaceInLinks),
                Box::new(MD040FencedCodeLanguage {}),
                Box::new(MD041FirstLineHeading::default()),
                Box::new(MD042NoEmptyLinks::new()),
                Box::new(MD043RequiredHeadings::new(Vec::new())),
                Box::new(MD044ProperNames::new(Vec::new(), true)),
                Box::new(MD045NoAltText::new()),
                Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
                Box::new(MD047FileEndNewline),
                Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
                Box::new(MD049EmphasisStyle::default()),
                Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
                Box::new(MD051LinkFragments),
                Box::new(MD052ReferenceLinkImages),
                Box::new(MD053LinkImageReferenceDefinitions::new(Vec::new())),
                Box::new(MD054LinkImageStyle::default()),
                Box::new(MD055TablePipeStyle::default()),
                Box::new(MD056TableColumnCount),
                Box::new(MD057ExistingRelativeLinks::default()),
                Box::new(MD058BlanksAroundTables),
            ];
            for rule in all_rules {
                if let Some((name, value)) = rule.default_config_section() {
                    rule_map.insert(name, value);
                }
            }
            let mut global = Value::try_from(rumdl_config::GlobalConfig::default())
                .unwrap_or(Value::Table(Default::default()));
            let sourced =
                rumdl_config::SourcedConfig::load_sourced_config(cli.config.as_deref(), None);
            // Overlay global
            if let Some(global_table) = global.as_table_mut() {
                let mut sourced_global_table = toml::map::Map::new();
                sourced_global_table.insert(
                    "enable".to_string(),
                    Value::Array(
                        sourced
                            .global
                            .enable
                            .value
                            .iter()
                            .map(|s| Value::String(s.clone()))
                            .collect(),
                    ),
                );
                sourced_global_table.insert(
                    "disable".to_string(),
                    Value::Array(
                        sourced
                            .global
                            .disable
                            .value
                            .iter()
                            .map(|s| Value::String(s.clone()))
                            .collect(),
                    ),
                );
                sourced_global_table.insert(
                    "exclude".to_string(),
                    Value::Array(
                        sourced
                            .global
                            .exclude
                            .value
                            .iter()
                            .map(|s| Value::String(s.clone()))
                            .collect(),
                    ),
                );
                sourced_global_table.insert(
                    "include".to_string(),
                    Value::Array(
                        sourced
                            .global
                            .include
                            .value
                            .iter()
                            .map(|s| Value::String(s.clone()))
                            .collect(),
                    ),
                );
                sourced_global_table.insert(
                    "respect_gitignore".to_string(),
                    Value::Boolean(sourced.global.respect_gitignore.value),
                );
                for (k, v) in sourced_global_table {
                    global_table.insert(k, v);
                }
            }
            for (rule_name, rule_defaults) in rule_map.iter_mut() {
                if let Some(sourced_rule) = sourced.rules.get(rule_name) {
                    if let Some(defaults_table) = rule_defaults.as_table_mut() {
                        for (k, sv) in &sourced_rule.values {
                            defaults_table.insert(k.clone(), sv.value.clone());
                        }
                    }
                }
            }
            // Annotated config output (after all variables are defined)
            // --- Collect all lines for alignment ---
            let mut all_lines = Vec::new();
            // --- GLOBAL SECTION ---
            let mut global_section_lines = Vec::new();
            let global_keys = [
                ("enable", &sourced.global.enable, "vec"),
                ("disable", &sourced.global.disable, "vec"),
                ("exclude", &sourced.global.exclude, "vec"),
                ("include", &sourced.global.include, "vec"),
            ];
            for (key, sourced_val, kind) in &global_keys {
                let value_str = match *kind {
                    "vec" => to_toml_string_vec_string(&sourced_val.value).yellow(),
                    _ => "<unknown>".yellow(),
                };
                let src_str_colored = match sourced_val.source {
                    rumdl_config::ConfigSource::Cli => "CLI".green(),
                    rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                    rumdl_config::ConfigSource::PyprojectToml => "pyproject.toml".magenta(),
                    rumdl_config::ConfigSource::Default => "default".yellow(),
                };
                let key_str = key.cyan();
                global_section_lines.push((
                    format!("  {} = {}", key_str, value_str),
                    format!("[from {}]", src_str_colored),
                ));
            }
            let sourced_val = &sourced.global.respect_gitignore;
            let value_str = sourced_val.value.to_string().yellow();
            let src_str_colored = match sourced_val.source {
                rumdl_config::ConfigSource::Cli => "CLI".green(),
                rumdl_config::ConfigSource::RumdlToml => ".rumdl.toml".blue(),
                rumdl_config::ConfigSource::PyprojectToml => "pyproject.toml".magenta(),
                rumdl_config::ConfigSource::Default => "default".yellow(),
            };
            let key_str = "respect_gitignore".cyan();
            global_section_lines.push((
                format!("  {} = {}", key_str, value_str),
                format!("[from {}]", src_str_colored),
            ));
            // Add section marker for printing
            all_lines.push(("[global]".bold().underline().to_string(), String::new()));
            all_lines.extend(global_section_lines);
            // --- RULE SECTIONS ---
            let mut rule_names: Vec<_> = rule_map.keys().cloned().collect();
            rule_names.sort();
            for rule_name in rule_names {
                let rule_defaults = rule_map.get(&rule_name).unwrap();
                let sourced_rule = sourced.rules.get(&rule_name);
                if let Some(defaults_table) = rule_defaults.as_table() {
                    // Add section marker for printing
                    all_lines.push((
                        format!("\n{}", format!("[{}]", rule_name).bold().underline()),
                        String::new(),
                    ));
                    let mut keys: Vec<_> = defaults_table.keys().collect();
                    keys.sort();
                    for key in keys {
                        let default_val = &defaults_table[key];
                        let (value, src_str_colored) = if let Some(sourced_rule) = sourced_rule
                        {
                            if let Some(sv) = sourced_rule.values.get(key) {
                                (
                                    sv.value.clone(),
                                    match sv.source {
                                        rumdl_config::ConfigSource::Cli => "CLI".green(),
                                        rumdl_config::ConfigSource::RumdlToml => {
                                            ".rumdl.toml".blue()
                                        }
                                        rumdl_config::ConfigSource::PyprojectToml => {
                                            "pyproject.toml".magenta()
                                        }
                                        rumdl_config::ConfigSource::Default => {
                                            "default".yellow()
                                        }
                                    },
                                )
                            } else {
                                (default_val.clone(), "default".yellow())
                            }
                        } else {
                            (default_val.clone(), "default".yellow())
                        };
                        let value_str = match value {
                            Value::Array(arr) => {
                                let vals: Vec<String> =
                                    arr.into_iter().map(|v| v.to_string()).collect();
                                format!("[{}]", vals.join(", ")).yellow()
                            }
                            Value::String(s) => format!("\"{}\"", s).yellow(),
                            Value::Boolean(b) => b.to_string().yellow(),
                            Value::Integer(i) => i.to_string().yellow(),
                            Value::Float(f) => f.to_string().yellow(),
                            _ => value.to_string().yellow(),
                        };
                        let key_str = key.cyan();
                        all_lines.push((
                            format!("  {} = {}", key_str, value_str),
                            format!("[from {}]", src_str_colored),
                        ));
                    }
                }
            }
            // --- Print all lines with global alignment ---
            let max_left = all_lines.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
            for (left, right) in &all_lines {
                if right.is_empty() {
                    println!("{}", left);
                } else {
                    println!("{:<width$} {}", left, right, width = max_left);
                }
            }
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
                    config: cli.config.clone(),
                    fix: cli.fix,
                    list_rules: cli.list_rules,
                    disable: cli.disable.clone(),
                    enable: cli.enable.clone(),
                    exclude: cli.exclude.clone(),
                    include: cli.include.clone(),
                    respect_gitignore: cli.respect_gitignore,
                    verbose: cli.verbose,
                    profile: cli.profile,
                    quiet: cli.quiet,
                };
                eprintln!("{}: Deprecation warning: Running 'rumdl .' or 'rumdl [PATHS...]' without a subcommand is deprecated and will be removed in a future release. Please use 'rumdl check .' instead.", "[rumdl]".yellow().bold());
                run_check(&args);
            } else {
                eprintln!(
            "{}: No files or directories specified. Please provide at least one path to lint.",
            "Error".red().bold()
        );
                std::process::exit(1);
            }
        }
    }
}

fn run_check(args: &CheckArgs) {
    // Load configuration
    let config = match rumdl_config::load_config(args.config.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            if args.config.is_some() {
                eprintln!("{}: {}", "Error".red().bold(), e);
                std::process::exit(1);
            }
            rumdl_config::Config::default()
        }
    };

    // Initialize rules with configuration
    let enabled_rules = get_enabled_rules_from_checkargs(args, &config);

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

    // Confirm with the user if we're fixing a large number of files
    if args.fix && file_paths.len() > 10 && !args.quiet {
        println!(
            "You are about to fix {} files. This will modify files in-place.",
            file_paths.len()
        );
        print!("Continue? [y/N] ");
        io::stdout().flush().unwrap();

        let mut answer = String::new();
        io::stdin().read_line(&mut answer).unwrap();

        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return;
        }
    }

    let start_time = Instant::now();

    // Process files sequentially
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
            args.fix,
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
    fix: bool,
    verbose: bool,
    quiet: bool,
) -> (bool, usize, usize, usize) {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        println!("Processing file: {}", file_path);
    }

    // Read file content
    let mut content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {}: {}", file_path, e);
            }
            return (false, 0, 0, 0);
        }
    };
    let _read_time = start_time.elapsed();

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
                if fix {
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
                format!("[{}]", rule_name).yellow(),
                warning.message,
                fix_indicator.green()
            );
        }
    }

    // Fix issues if requested
    let mut warnings_fixed = 0;
    if fix {
        // Skip rules that don't have fixes
        for rule in rules {
            if all_warnings
                .iter()
                .any(|w| w.rule_name == Some(rule.name()) && w.fix.is_some())
            {
                match rule.fix(&content) {
                    Ok(fixed_content) => {
                        if fixed_content != content {
                            content = fixed_content;
                            // Apply fixes for this rule - we consider all warnings for the rule fixed
                            warnings_fixed += all_warnings
                                .iter()
                                .filter(|w| w.rule_name == Some(rule.name()) && w.fix.is_some())
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
