use clap::{Parser, Subcommand};
use colored::*;
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use std::process;
use std::time::Instant;
use std::error::Error;

use rumdl::rule::Rule;
use rumdl::rules::code_block_utils::CodeBlockStyle;
use rumdl::rules::code_fence_utils::CodeFenceStyle;
use rumdl::rules::emphasis_style::EmphasisStyle;
use rumdl::rules::strong_style::StrongStyle;
use rumdl::rules::*;
use rumdl::{
    MD046CodeBlockStyle, MD048CodeFenceStyle, MD049EmphasisStyle, MD050StrongStyle,
};

mod config;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
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
    /// Note: When explicit paths are provided, include patterns are ignored.
    /// Use either include patterns or paths, not both.
    #[arg(long)]
    include: Option<String>,

    /// Ignore .gitignore files when scanning directories
    #[arg(long, default_value = "false", help = "Ignore .gitignore files when scanning directories (does not apply to explicitly provided paths)")]
    ignore_gitignore: bool,

    /// Respect .gitignore files when scanning directories (takes precedence over ignore_gitignore)
    #[arg(long, default_value = "true", help = "Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)")]
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

    /// Command to run
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new configuration file
    Init {
        /// Generate configuration for pyproject.toml instead of .rumdl.toml
        #[arg(long)]
        pyproject: bool,
    },
}

// Helper function to apply configuration to rules that need it
fn apply_rule_configs(rules: &mut Vec<Box<dyn Rule>>, config: &config::Config) {
    // Replace any rules that need configuration with properly configured instances

    // Replace MD013 with configured instance
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length =
            config::get_rule_config_value::<usize>(config, "MD013", "line_length").unwrap_or(80);
        let code_blocks =
            config::get_rule_config_value::<bool>(config, "MD013", "code_blocks").unwrap_or(true);
        let tables =
            config::get_rule_config_value::<bool>(config, "MD013", "tables").unwrap_or(false);
        let headings =
            config::get_rule_config_value::<bool>(config, "MD013", "headings").unwrap_or(true);
        let strict =
            config::get_rule_config_value::<bool>(config, "MD013", "strict").unwrap_or(false);

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
            config::get_rule_config_value::<Vec<String>>(config, "MD043", "headings")
                .unwrap_or_else(Vec::new);
        
        // Strip leading '#' and spaces from the configured headings to match the format of extracted headings
        headings = headings.iter().map(|h| {
            h.trim_start_matches(|c| c == '#' || c == ' ').to_string()
        }).collect();
        
        rules[pos] = Box::new(MD043RequiredHeadings::new(headings));
    }

    // Replace MD053 with configured instance
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD053") {
        let ignored_definitions =
            config::get_rule_config_value::<Vec<String>>(config, "MD053", "ignored_definitions")
                .unwrap_or_else(Vec::new);
        rules[pos] = Box::new(MD053LinkImageReferenceDefinitions::new(ignored_definitions));
    }

    // Add more rule configurations as needed
}

// Get a complete set of enabled rules based on CLI options and config
fn get_enabled_rules(cli: &Cli, config: &config::Config) -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = Vec::new();

    // Add all the implemented rules with default configuration
    rules.push(Box::new(MD001HeadingIncrement));
    rules.push(Box::new(MD002FirstHeadingH1::default()));
    rules.push(Box::new(MD003HeadingStyle::default()));
    rules.push(Box::new(MD004UnorderedListStyle::default()));
    rules.push(Box::new(MD005ListIndent));
    rules.push(Box::new(MD006StartBullets));
    rules.push(Box::new(MD007ULIndent::default()));
    rules.push(Box::new(MD009TrailingSpaces::default()));
    rules.push(Box::new(MD010NoHardTabs::default()));
    rules.push(Box::new(MD011ReversedLink {}));
    rules.push(Box::new(MD012NoMultipleBlanks::default()));
    rules.push(Box::new(MD013LineLength::default()));
    rules.push(Box::new(MD014CommandsShowOutput::default()));
    rules.push(Box::new(MD015NoMissingSpaceAfterListMarker::default()));
    rules.push(Box::new(MD016NoMultipleSpaceAfterListMarker::default()));
    rules.push(Box::new(MD017NoEmphasisAsHeading));
    rules.push(Box::new(MD018NoMissingSpaceAtx {}));
    rules.push(Box::new(MD019NoMultipleSpaceAtx {}));
    rules.push(Box::new(MD020NoMissingSpaceClosedAtx {}));
    rules.push(Box::new(MD021NoMultipleSpaceClosedAtx {}));
    rules.push(Box::new(MD022BlanksAroundHeadings::default()));
    rules.push(Box::new(MD023HeadingStartLeft {}));
    rules.push(Box::new(MD024MultipleHeadings::default()));
    rules.push(Box::new(MD025SingleTitle::default()));
    rules.push(Box::new(MD026NoTrailingPunctuation::default()));
    rules.push(Box::new(MD027MultipleSpacesBlockquote {}));
    rules.push(Box::new(MD028NoBlanksBlockquote {}));
    rules.push(Box::new(MD029OrderedListPrefix::default()));
    rules.push(Box::new(MD030ListMarkerSpace::default()));
    rules.push(Box::new(MD031BlanksAroundFences {}));
    rules.push(Box::new(MD032BlanksAroundLists {}));
    rules.push(Box::new(MD033NoInlineHtml::default()));
    rules.push(Box::new(MD034NoBareUrls {}));
    rules.push(Box::new(MD035HRStyle::default()));
    rules.push(Box::new(MD036NoEmphasisOnlyFirst {}));
    rules.push(Box::new(MD037SpacesAroundEmphasis::default()));
    rules.push(Box::new(MD038NoSpaceInCode::default()));
    rules.push(Box::new(MD039NoSpaceInLinks::default()));
    rules.push(Box::new(MD040FencedCodeLanguage {}));
    rules.push(Box::new(MD041FirstLineHeading::default()));
    rules.push(Box::new(MD042NoEmptyLinks::new()));
    rules.push(Box::new(MD043RequiredHeadings::new(Vec::new())));
    rules.push(Box::new(MD044ProperNames::new(Vec::new(), true)));
    rules.push(Box::new(MD045NoAltText::new()));
    rules.push(Box::new(MD046CodeBlockStyle::new(
        CodeBlockStyle::Consistent,
    )));
    rules.push(Box::new(MD047FileEndNewline));
    rules.push(Box::new(MD048CodeFenceStyle::new(
        CodeFenceStyle::Consistent,
    )));
    rules.push(Box::new(MD049EmphasisStyle::new(EmphasisStyle::Consistent)));
    rules.push(Box::new(MD050StrongStyle::new(StrongStyle::Consistent)));
    rules.push(Box::new(MD051LinkFragments));
    rules.push(Box::new(MD052ReferenceLinkImages::default()));
    rules.push(Box::new(MD053LinkImageReferenceDefinitions::default()));
    rules.push(Box::new(MD054LinkImageStyle::default()));
    rules.push(Box::new(MD055TablePipeStyle::default()));
    rules.push(Box::new(MD056TableColumnCount::default()));
    rules.push(Box::new(MD057ExistingRelativeLinks::default()));
    rules.push(Box::new(MD058BlanksAroundTables::default()));

    // Apply configuration to rules that need it
    apply_rule_configs(&mut rules, config);

    // Apply config disable first
    if !config.global.disable.is_empty() {
        rules.retain(|rule| !config.global.disable.iter().any(|name| name == rule.name()));
    }

    // Apply config enable if provided (exclusive list)
    if !config.global.enable.is_empty() {
        rules.retain(|rule| config.global.enable.iter().any(|name| name == rule.name()));
    }

    // Apply rule enable/disable from CLI (overrides config)
    if let Some(enable) = &cli.enable {
        // Only keep rules that are in the enable list
        let enabled_rules: Vec<&str> = enable.split(',').map(|s| s.trim()).collect();
        rules.retain(|rule| enabled_rules.contains(&rule.name()));
    } else if let Some(disable) = &cli.disable {
        // Remove rules that are in the disable list
        let disabled_rules: Vec<&str> = disable.split(',').map(|s| s.trim()).collect();
        rules.retain(|rule| !disabled_rules.contains(&rule.name()));
    }

    // Print enabled rules if verbose
    if cli.verbose {
        println!("Enabled rules:");
        for rule in &rules {
            println!("  - {} ({})", rule.name(), rule.description());
        }
        println!();
    }

    rules
}

// Find all markdown files using the `ignore` crate, returning Result
fn find_markdown_files(paths: &[String], cli: &Cli, config: &config::Config) -> Result<Vec<String>, Box<dyn Error>> {
    let mut file_paths = Vec::new();

    // --- Configure ignore::WalkBuilder ---
    // Start with the first path, add others later
    let first_path = paths.get(0).cloned().unwrap_or_else(|| ".".to_string());
    let mut walk_builder = WalkBuilder::new(first_path);

    // Add remaining paths
    for path in paths.iter().skip(1) {
        walk_builder.add(path);
    }

    // Determine if running in discovery mode (e.g., "rumdl .") vs explicit path mode
    let is_discovery_mode = paths.len() == 1 && paths[0] == ".";

    // Determine effective patterns based on CLI overrides
    let exclude_patterns = if let Some(exclude_str) = cli.exclude.as_deref() {
        // If CLI exclude is given, IT REPLACES config excludes
        exclude_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else {
        // Otherwise, use config excludes
        config.global.exclude.clone() // Already Vec<String>
    };

    let include_patterns = if is_discovery_mode {
        if let Some(include_str) = cli.include.as_deref() {
            // If CLI include is given, IT REPLACES config includes
             include_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else {
            // Otherwise, use config includes
            config.global.include.clone() // Already Vec<String>
        }
    } else {
        // Includes are ignored if explicit paths are given (not discovery mode)
        Vec::new()
    };

    // Apply overrides from effective patterns (reverted: apply regardless of discovery mode)
    let has_include_patterns = !include_patterns.is_empty();
    let has_exclude_patterns = !exclude_patterns.is_empty();

    if has_include_patterns || has_exclude_patterns {
        // Revert to initializing OverrideBuilder with "."
        let mut override_builder = OverrideBuilder::new("."); // Root context for patterns

        // Add includes first
        if has_include_patterns {
            for pattern in &include_patterns {
                // No need to split again, already done above
                if let Err(e) = override_builder.add(pattern) {
                    eprintln!("[Effective] Warning: Invalid include pattern '{}': {}", pattern, e);
                }
            }
        }
        // Add excludes second (as ignore rules !pattern)
        if has_exclude_patterns {
            for pattern in &exclude_patterns {
                 // Revert back to adding the pattern prefixed with '!'
                if let Err(e) = override_builder.add(&format!("!{}", pattern)) {
                    eprintln!("[Effective] Warning: Invalid exclude pattern '{}': {}", pattern, e);
                }
            }
        }
        let overrides = override_builder.build()?;
        walk_builder.overrides(overrides);
    }

    // Configure gitignore handling *SECOND*
    let use_gitignore = if cli.respect_gitignore {
        !cli.ignore_gitignore // If respect is true, only ignore if ignore_gitignore is false
    } else {
        false // If respect is false, always ignore gitignore
    };
    
    walk_builder.ignore(use_gitignore);      // Enable/disable .ignore
    walk_builder.git_ignore(use_gitignore); // Enable/disable .gitignore
    walk_builder.git_global(use_gitignore);  // Enable/disable global gitignore
    walk_builder.git_exclude(use_gitignore); // Enable/disable .git/info/exclude
    walk_builder.parents(use_gitignore);        // Enable/disable parent ignores
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
                    let cleaned_path = if file_path.starts_with("./") {
                        file_path[2..].to_string()
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

    // Note: The complex logic for warning about excluded explicit paths is removed
    // as the `ignore` crate handles this implicitly.
    // We could potentially add it back by tracking inputs vs outputs if needed.

    Ok(file_paths)
}

// Function to print linting results and summary
fn print_results(
    cli: &Cli,
    has_issues: bool,
    files_with_issues: usize,
    total_issues: usize,
    total_issues_fixed: usize,
    total_fixable_issues: usize,
    total_files_processed: usize,
    duration_ms: u64,
) {
    // Skip all output in quiet mode except warnings (which are printed during file processing)
    if cli.quiet {
        return;
    }

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
        if cli.fix && total_issues_fixed > 0 {
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

            if !cli.fix && total_fixable_issues > 0 {
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

        // Write fixed content back to file
        if warnings_fixed > 0 {
            if let Err(err) = std::fs::write(file_path, &content) {
                eprintln!(
                    "{} Failed to write fixed content to file {}: {}",
                    "Error:".red().bold(),
                    file_path,
                    err
                );
            }
        }
    }

    let lint_end_time = Instant::now();
    let lint_time = lint_end_time.duration_since(lint_start);

    if verbose {
        println!("Linting took: {:?}", lint_time);
    }

    let total_time = start_time.elapsed();
    if verbose {
        println!("Total processing time for {}: {:?}", file_path, total_time);
    }

    (true, total_warnings, warnings_fixed, fixable_warnings)
}

fn main() {
    let _timer = rumdl::profiling::ScopedTimer::new("main");

    // Initialize CLI and parse arguments
    let cli = Cli::parse();

    // Handle init command separately
    if let Some(Commands::Init { pyproject }) = cli.command {
        if pyproject {
            // Handle pyproject.toml initialization
            let config_content = config::generate_pyproject_config();
            
            if Path::new("pyproject.toml").exists() {
                // pyproject.toml exists, ask to append
                if !cli.quiet {
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
                                let new_content = format!("{}\n\n{}", content.trim_end(), config_content);
                                match fs::write("pyproject.toml", new_content) {
                                    Ok(_) => println!("Added rumdl configuration to pyproject.toml"),
                                    Err(e) => {
                                        eprintln!(
                                            "{}: Failed to update pyproject.toml: {}",
                                            "Error".red().bold(),
                                            e
                                        );
                                        std::process::exit(1);
                                    }
                                }
                            },
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
                        if !cli.quiet {
                            println!("Created pyproject.toml with rumdl configuration");
                        }
                    },
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
        match config::create_default_config(".rumdl.toml") {
            Ok(_) => {
                if !cli.quiet {
                    println!("Created default configuration file: .rumdl.toml");
                }
                return;
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

    // If no paths provided and not using a subcommand, print error and exit
    if cli.paths.is_empty() {
        eprintln!(
            "{}: No files or directories specified. Please provide at least one path to lint.",
            "Error".red().bold()
        );
        std::process::exit(1);
    }

    // Load configuration
    let config = match config::load_config(cli.config.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            // Only show error if a specific config file was provided
            if cli.config.is_some() {
                eprintln!("{}: {}", "Error".red().bold(), e);
                std::process::exit(1);
            }
            // Otherwise use default config
            config::Config::default()
        }
    };

    // Initialize rules with configuration
    let enabled_rules = get_enabled_rules(&cli, &config);

    // Find all markdown files to check
    let file_paths = match find_markdown_files(&cli.paths, &cli, &config) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{}: Failed to find markdown files: {}", "Error".red().bold(), e);
            process::exit(1);
        }
    };
    if file_paths.is_empty() {
        if !cli.quiet {
            println!("No markdown files found to check.");
        }
        return;
    }

    // Confirm with the user if we're fixing a large number of files
    if cli.fix && file_paths.len() > 10 && !cli.quiet {
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
        let (file_has_issues, issues_found, issues_fixed, fixable_issues) =
            process_file(file_path, &enabled_rules, cli.fix, cli.verbose, cli.quiet);

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
    print_results(
        &cli,
        has_issues,
        files_with_issues,
        total_issues,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
        duration_ms,
    );

    // Print profiling information if enabled and not in quiet mode
    if cli.profile && !cli.quiet {
        // Try to get profiling information and handle any errors
        match std::panic::catch_unwind(|| rumdl::profiling::get_report()) {
            Ok(report) => println!("\n{}", report),
            Err(_) => println!("\nProfiling information not available"),
        }
    }

    // Exit with non-zero status if issues were found
    if has_issues {
        std::process::exit(1);
    }
}
