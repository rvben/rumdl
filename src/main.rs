use clap::{Parser, Subcommand};
use colored::*;
use ignore::WalkBuilder;
use rumdl::rules::*;
use rumdl::rules::md046_code_block_style::CodeBlockStyle;
use rumdl::rules::md048_code_fence_style::CodeFenceStyle;
use rumdl::rules::md049_emphasis_style::EmphasisStyle;
use rumdl::rules::md050_strong_style::StrongStyle;
use rumdl::rule::{LintWarning, Rule};
use std::fs;
use std::path::Path;
use std::process;
use walkdir::WalkDir;

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
    #[arg(short, long)]
    fix: bool,

    /// List all available rules
    #[arg(short, long)]
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

    /// Respect .gitignore files when scanning directories
    #[arg(long, default_value = "true")]
    respect_gitignore: bool,

    /// Debug gitignore patterns for a specific file
    #[arg(long)]
    debug_gitignore: bool,

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,
    
    /// Show profiling information
    #[arg(long)]
    profile: bool,

    /// Command to run
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new configuration file
    Init,
}

fn get_rules(opts: &Cli) -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = Vec::new();

    // Load configuration file if provided
    let config_result = match &opts.config {
        Some(path) => config::load_config(Some(path)),
        None => config::load_config(None),
    };

    // Log any configuration errors but continue with defaults
    let config = match config_result {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{}: {}", "Configuration error".yellow().bold(), err);
            config::Config::default()
        }
    };

    // Add implemented rules
    rules.push(Box::new(MD001HeadingIncrement));
    rules.push(Box::new(MD002FirstHeadingH1::default()));
    rules.push(Box::new(MD017NoEmphasisAsHeading::default()));
    rules.push(Box::new(MD036NoEmphasisOnlyFirst {}));
    rules.push(Box::new(MD003HeadingStyle::default()));
    rules.push(Box::new(MD004UnorderedListStyle::default()));
    rules.push(Box::new(MD005ListIndent));
    rules.push(Box::new(MD006StartBullets));
    rules.push(Box::new(MD007ULIndent::default()));

    // Configure MD008 from config if available
    let md008 =
        if let Some(style) = config::get_rule_config_value::<char>(&config, "MD008", "style") {
            Box::new(MD008ULStyle::new(style))
        } else {
            Box::new(MD008ULStyle::default())
        };
    rules.push(md008);

    rules.push(Box::new(MD009TrailingSpaces::default()));
    rules.push(Box::new(MD010NoHardTabs::default()));
    rules.push(Box::new(MD011ReversedLink {}));
    rules.push(Box::new(MD012NoMultipleBlanks::default()));

    // Configure MD013 from config if available
    let md013 = {
        let line_length =
            config::get_rule_config_value::<usize>(&config, "MD013", "line_length").unwrap_or(80);
        let code_blocks =
            config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks").unwrap_or(true);
        let tables =
            config::get_rule_config_value::<bool>(&config, "MD013", "tables").unwrap_or(false);
        let headings =
            config::get_rule_config_value::<bool>(&config, "MD013", "headings").unwrap_or(true);
        let strict =
            config::get_rule_config_value::<bool>(&config, "MD013", "strict").unwrap_or(false);

        Box::new(MD013LineLength::new(
            line_length,
            code_blocks,
            tables,
            headings,
            strict,
        ))
    };
    rules.push(md013);

    rules.push(Box::new(MD014CommandsShowOutput::default()));
    rules.push(Box::new(MD015NoMissingSpaceAfterListMarker::default()));
    rules.push(Box::new(MD016NoMultipleSpaceAfterListMarker::default()));
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
    rules.push(Box::new(MD037SpacesAroundEmphasis {}));
    rules.push(Box::new(MD038NoSpaceInCode {}));
    rules.push(Box::new(MD039NoSpaceInLinks {}));
    rules.push(Box::new(MD040FencedCodeLanguage {}));
    rules.push(Box::new(MD041FirstLineHeading::default()));
    rules.push(Box::new(MD042NoEmptyLinks::new()));
    rules.push(Box::new(MD043RequiredHeadings::new(Vec::new())));
    rules.push(Box::new(MD044ProperNames::new(Vec::new(), true)));
    rules.push(Box::new(MD045NoAltText::new()));

    // Configure MD046 from config if available
    let md046 = {
        let style_str = config::get_rule_config_value::<String>(&config, "MD046", "style")
            .unwrap_or_else(|| "consistent".to_string());

        let style = match style_str.to_lowercase().as_str() {
            "fenced" => CodeBlockStyle::Fenced,
            "indented" => CodeBlockStyle::Indented,
            _ => CodeBlockStyle::Consistent,
        };

        Box::new(MD046CodeBlockStyle::new(style))
    };
    rules.push(md046);

    rules.push(Box::new(MD047FileEndNewline {}));

    // Configure MD048 from config if available
    let md048 = {
        let style_str = config::get_rule_config_value::<String>(&config, "MD048", "style")
            .unwrap_or_else(|| "consistent".to_string());

        let style = match style_str.to_lowercase().as_str() {
            "backtick" => CodeFenceStyle::Backtick,
            "tilde" => CodeFenceStyle::Tilde,
            _ => CodeFenceStyle::Consistent,
        };

        Box::new(MD048CodeFenceStyle::new(style))
    };
    rules.push(md048);

    // Configure MD049 from config if available
    let md049 = {
        let style_str = config::get_rule_config_value::<String>(&config, "MD049", "style")
            .unwrap_or_else(|| "consistent".to_string());

        let style = match style_str.to_lowercase().as_str() {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            _ => EmphasisStyle::Consistent,
        };

        Box::new(MD049EmphasisStyle::new(style))
    };
    rules.push(md049);

    // Configure MD050 from config if available
    let md050 = {
        let style_str = config::get_rule_config_value::<String>(&config, "MD050", "style")
            .unwrap_or_else(|| "consistent".to_string());

        let style = match style_str.to_lowercase().as_str() {
            "asterisk" => StrongStyle::Asterisk,
            "underscore" => StrongStyle::Underscore,
            _ => StrongStyle::Consistent,
        };

        Box::new(MD050StrongStyle::new(style))
    };
    rules.push(md050);

    rules.push(Box::new(MD051LinkFragments::new()));
    rules.push(Box::new(MD052ReferenceLinkImages::new()));
    rules.push(Box::new(MD053LinkImageReferenceDefinitions::default()));

    // Use default implementation for MD054
    rules.push(Box::new(MD054LinkImageStyle::default()));

    // Configure MD055 with the style from config
    let md055 = if let Some(style) = config::get_rule_config_value::<String>(&config, "MD055", "style") {
        Box::new(MD055TablePipeStyle::new(&style))
    } else {
        Box::new(MD055TablePipeStyle::default())
    };
    rules.push(md055);
    
    rules.push(Box::new(MD056TableColumnCount));
    rules.push(Box::new(MD057ExistingRelativeLinks::new()));
    rules.push(Box::new(MD058BlanksAroundTables));

    // Filter rules based on configuration and command-line options
    // Priority: command-line options override config file settings
    let disable_rules: Vec<String> = match &opts.disable {
        Some(disable_str) => disable_str.split(',').map(String::from).collect(),
        None => config.global.disable.clone(),
    };

    let enable_rules: Vec<String> = match &opts.enable {
        Some(enable_str) => enable_str.split(',').map(String::from).collect(),
        None => config.global.enable.clone(),
    };

    // Apply the filters
    if !enable_rules.is_empty() {
        rules.retain(|rule| enable_rules.iter().any(|r| r == rule.name()));
    } else if !disable_rules.is_empty() {
        rules.retain(|rule| !disable_rules.iter().any(|r| r == rule.name()));
    }

    rules
}

fn list_available_rules() {
    println!("Available rules:");

    // Create a temporary instance of all rules to get their names and descriptions
    let rules = get_rules(&Cli {
        paths: vec![],
        config: None,
        fix: false,
        list_rules: true,
        disable: None,
        enable: None,
        exclude: None,
        include: None,
        respect_gitignore: false,
        debug_gitignore: false,
        verbose: false,
        profile: false,
        command: None,
    });

    // Sort rules by name
    let mut rule_info: Vec<(&str, &str)> = rules
        .iter()
        .map(|rule| (rule.name(), rule.description()))
        .collect();
    rule_info.sort_by(|a, b| a.0.cmp(b.0));

    // Print rule names and descriptions
    for (name, description) in rule_info {
        println!("{} - {}", name, description);
    }
}

// Helper function to display paths without "./" prefix
fn display_path(path: &str) -> &str {
    if path.starts_with("./") {
        &path[2..]
    } else {
        path
    }
}

// Process a single file
fn process_file(
    path: &str,
    rules: &[Box<dyn Rule>],
    fix: bool,
    verbose: bool,
) -> (bool, usize, usize) {
    let _timer = rumdl::profiling::ScopedTimer::new(&format!("process_file:{}", path));

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!(
                "{}: {}: {}",
                "Error reading file".red().bold(),
                display_path(path).blue().underline(),
                err
            );
            return (false, 0, 0);
        }
    };

    let mut has_warnings = false;
    let mut total_warnings = 0;
    let mut total_fixed = 0;
    let mut all_warnings: Vec<(&'static str, &Box<dyn Rule>, LintWarning)> = Vec::new();

    // Special handling for MD057 rule - create a new instance with the file path
    // Check for MD057 link existence warnings separately
    let path_obj = std::path::Path::new(path);
    let md057_rule = MD057ExistingRelativeLinks::new().with_path(path_obj);
    
    // Check MD057 separately with the path set
    match md057_rule.check(&content) {
        Ok(warnings) => {
            if !warnings.is_empty() {
                // Filter out warnings for lines where the rule is disabled
                let filtered_warnings: Vec<LintWarning> = warnings
                    .into_iter()
                    .filter(|warning| {
                        !rumdl::rule::is_rule_disabled_at_line(
                            &content,
                            md057_rule.name(),
                            warning.line - 1,
                        )
                    })
                    .collect();

                if !filtered_warnings.is_empty() {
                    has_warnings = true;
                    total_warnings += filtered_warnings.len();

                    for warning in filtered_warnings {
                        // Use a reference to the temporary md057_rule for the all_warnings collection
                        // This is safe because we're only using it for reporting, not for long-term storage
                        all_warnings.push((md057_rule.name(), 
                            // This is a bit of a hack, but it works because we only use the rule to get its name
                            // in the reporting logic
                            rules.iter().find(|r| r.name() == "MD057").unwrap_or(&rules[0]), 
                            warning));
                    }
                }
            }
        }
        Err(err) => {
            eprintln!(
                "{}: {} on file {}: {}",
                "Error".red().bold(),
                md057_rule.name().yellow(),
                display_path(path).blue().underline(),
                err
            );
        }
    }

    {
        let _timer = rumdl::profiling::ScopedTimer::new(&format!("check_rules:{}", path));

        // Collect all warnings first
        for rule in rules {
            // Skip MD057 rule as we've already processed it separately
            if rule.name() == "MD057" {
                continue;
            }
            
            let _rule_timer =
                rumdl::profiling::ScopedTimer::new(&format!("rule:{}:{}", rule.name(), path));

            match rule.check(&content) {
                Ok(warnings) => {
                    if !warnings.is_empty() {
                        // Filter out warnings for lines where the rule is disabled
                        let filtered_warnings: Vec<LintWarning> = warnings
                            .into_iter()
                            .filter(|warning| {
                                !rumdl::rule::is_rule_disabled_at_line(
                                    &content,
                                    rule.name(),
                                    warning.line - 1,
                                )
                            })
                            .collect();

                        if !filtered_warnings.is_empty() {
                            has_warnings = true;
                            total_warnings += filtered_warnings.len();

                            for warning in filtered_warnings {
                                all_warnings.push((rule.name(), rule, warning));
                            }
                        }
                    }
                }
                Err(err) => {
                    eprintln!(
                        "{}: {} on file {}: {}",
                        "Error".red().bold(),
                        rule.name().yellow(),
                        display_path(path).blue().underline(),
                        err
                    );
                }
            }
        }
    }

    // Sort warnings by line and column
    all_warnings.sort_by(|a, b| {
        let line_cmp = a.2.line.cmp(&b.2.line);
        if line_cmp == std::cmp::Ordering::Equal {
            a.2.column.cmp(&b.2.column)
        } else {
            line_cmp
        }
    });

    // Display warnings in a clean format
    if !all_warnings.is_empty() {
        for (rule_name, _, warning) in &all_warnings {
            let fixable = warning.fix.is_some();
            let fix_indicator = if fixable && fix {
                "[fixed]".green()
            } else if fixable {
                "[*]".green()
            } else {
                "".normal()
            };

            println!(
                "{}:{}:{}: {} {} {}",
                display_path(path).blue().underline(),
                warning.line.to_string().cyan(),
                warning.column.to_string().cyan(),
                format!("[{}]", rule_name).yellow(),
                warning.message,
                fix_indicator
            );
        }
    } else if verbose {
        println!(
            "{}: No issues found in {}",
            "Success".green().bold(),
            display_path(path).blue().underline()
        );
    }

    // Apply fixes in a single pass if requested
    if fix && has_warnings {
        let _timer = rumdl::profiling::ScopedTimer::new(&format!("fix_file:{}", path));

        let mut fixed_content = content.clone();
        let mut fixed_warnings = 0;

        // Define a rule application order that reduces duplication issues
        // Process emphasis and heading rules first, then other rules
        let mut rule_priorities: Vec<&'static str> = vec![
            // Process emphasis rules first
            "MD017", // NoEmphasisAsHeading
            "MD036", // NoEmphasisOnlyFirst
            
            // Then heading style rules 
            "MD003", // HeadingStyle
            
            // Then all other rules
        ];
        
        // Add remaining rules in their original order
        for rule in rules {
            let name = rule.name();
            if !rule_priorities.contains(&name) {
                rule_priorities.push(name);
            }
        }
        
        // Apply fixes in priority order
        for rule_name in rule_priorities {
            // Skip rules that don't exist in our loaded rules
            let rule = match rules.iter().find(|r| r.name() == rule_name) {
                Some(r) => r,
                None => continue,
            };
            
            // Skip rules with no warnings
            let rule_warnings: Vec<&LintWarning> = all_warnings
                .iter()
                .filter_map(|(name, _, warning)| {
                    if name == &rule_name && warning.fix.is_some() {
                        Some(warning)
                    } else {
                        None
                    }
                })
                .collect();
                
            if rule_warnings.is_empty() {
                continue;
            }
            
            // Apply the rule's fix
            let _fix_timer = rumdl::profiling::ScopedTimer::new(&format!("fix_rule:{}:{}", rule_name, path));
            
            match rule.fix(&fixed_content) {
                Ok(new_content) => {
                    if new_content != fixed_content {
                        fixed_content = new_content;
                        fixed_warnings += rule_warnings.len();
                        
                        // After each significant rule, recheck all warnings to avoid duplicate fixes
                        if rule_name == "MD017" || rule_name == "MD036" || rule_name == "MD003" {
                            // Re-check for warnings with the updated content
                            all_warnings.clear();
                            let _check_timer = rumdl::profiling::ScopedTimer::new(&format!("recheck_rules:{}", path));
                            
                            for rule in rules {
                                let _rule_timer = rumdl::profiling::ScopedTimer::new(&format!("rule:{}:{}", rule.name(), path));
                                
                                match rule.check(&fixed_content) {
                                    Ok(warnings) => {
                                        for warning in warnings {
                                            all_warnings.push((rule.name(), rule, warning));
                                        }
                                    }
                                    Err(err) => {
                                        if verbose {
                                            eprintln!(
                                                "{} checking rule {} for {}: {}",
                                                "Error".red().bold(),
                                                rule.name().cyan(),
                                                display_path(path).blue().underline(),
                                                err
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    if verbose {
                        eprintln!(
                            "{} applying fix for rule {} to {}: {}",
                            "Error".red().bold(),
                            rule_name.cyan(),
                            display_path(path).blue().underline(),
                            err
                        );
                    }
                }
            }
        }

        // Write fixed content back to file
        if fixed_warnings > 0 {
            total_fixed = fixed_warnings;
            match fs::write(path, fixed_content) {
                Ok(_) => {}
                Err(err) => {
                    eprintln!(
                        "{} {}: {}",
                        "Error writing fixed content to".red().bold(),
                        display_path(path).blue().underline(),
                        err
                    );
                }
            }
        }
    }

    // Return whether there were warnings, total warnings, and total fixed
    (has_warnings, total_warnings, total_fixed)
}

// Helper function to test if a file would be ignored by .gitignore
fn debug_gitignore_test(path: &str, verbose: bool) {
    use ignore::WalkBuilder;
    use std::fs;
    use std::path::Path;

    let file_path = Path::new(path);

    println!("Testing gitignore patterns for: {}", path);

    // First, check if the file exists
    if !file_path.exists() {
        println!("File does not exist: {}", path);
        return;
    }

    // Read the .gitignore file if it exists
    let mut gitignore_patterns = Vec::new();
    if let Ok(content) = fs::read_to_string(".gitignore") {
        println!("\nFound .gitignore file with the following patterns:");
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                println!("  - {}", trimmed);
                gitignore_patterns.push(trimmed.to_string());
            }
        }
    } else {
        println!("No .gitignore file found in the current directory.");
    }

    // Create a walker that respects .gitignore files
    let walker = WalkBuilder::new(".")
        .hidden(true)
        .git_global(true)
        .git_ignore(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".gitignore")
        .build();

    // Check if the file is in the walker's output
    let mut found_in_walker = false;
    let canonical_path = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());

    for entry in walker.flatten() {
        let entry_canonical = entry
            .path()
            .canonicalize()
            .unwrap_or_else(|_| entry.path().to_path_buf());
        if entry_canonical == canonical_path {
            found_in_walker = true;
            break;
        }
    }

    if found_in_walker {
        println!("\nFile would NOT be ignored by gitignore");
    } else {
        println!("\nFile would be IGNORED by gitignore");

        // Try to determine which pattern is causing the file to be ignored
        for pattern in &gitignore_patterns {
            if pattern.contains('*') {
                // For glob patterns, use the glob crate
                if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                    if glob_pattern.matches(path) {
                        println!("  - Matched by pattern: {}", pattern);
                    }
                }
            } else {
                // For simple patterns, check if the path contains the pattern
                if path.contains(pattern) {
                    println!("  - Matched by pattern: {}", pattern);
                }
            }
        }
    }

    if verbose {
        println!("\nSample of files that would be processed (not ignored):");

        // Create a new walker to list some files that would be processed
        let walker = WalkBuilder::new(".")
            .hidden(true)
            .git_global(true)
            .git_ignore(true)
            .git_exclude(true)
            .add_custom_ignore_filename(".gitignore")
            .build();

        let mut count = 0;
        for entry in walker.flatten() {
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                println!("  - {}", entry.path().display());
                count += 1;
                if count >= 10 {
                    println!("  ... and more");
                    break;
                }
            }
        }
    }

    println!();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _timer = rumdl::profiling::ScopedTimer::new("main");

    let mut cli = Cli::parse();

    // Load config early to use its values
    let config = config::load_config(cli.config.as_deref()).unwrap_or_else(|_| config::Config::default());

    // Use config value for respect_gitignore if not set in CLI
    if !cli.respect_gitignore {
        cli.respect_gitignore = config.global.respect_gitignore;
    }

    if cli.list_rules {
        list_available_rules();
        return Ok(());
    }

    // Handle init command to create a default configuration file
    if let Some(Commands::Init) = cli.command {
        let config_path = ".rumdl.toml";
        match config::create_default_config(config_path) {
            Ok(_) => {
                println!(
                    "{}: Created default configuration file at {}",
                    "Success".green().bold(),
                    config_path
                );
                println!("You can now customize the configuration to suit your needs.");
                return Ok(());
            }
            Err(err) => {
                eprintln!(
                    "{}: Failed to create configuration file: {}",
                    "Error".red().bold(),
                    err
                );
                process::exit(1);
            }
        }
    }

    // Process include patterns from CLI and config
    let mut include_patterns = Vec::new();

    // Add include patterns from CLI (overrides config) or from config if no CLI patterns
    if let Some(include_str) = &cli.include {
        include_patterns.extend(include_str.split(',').map(|s| s.trim().to_string()));
    } else if let Ok(ref loaded_config) = config::load_config(cli.config.as_deref()) {
        include_patterns.extend(loaded_config.global.include.clone());
    }

    // Only require paths if we're not running a subcommand and no include patterns in config
    if cli.paths.is_empty() && cli.command.is_none() && include_patterns.is_empty() {
        eprintln!(
            "{}: No paths provided. Please specify at least one file or directory to lint.",
            "Error".red().bold()
        );
        process::exit(1);
    }

    // If debug_gitignore is enabled, test gitignore patterns for the specified files
    if cli.debug_gitignore {
        for path in &cli.paths {
            debug_gitignore_test(path, cli.verbose);
        }
        return Ok(());
    }

    let rules = get_rules(&cli);
    if rules.is_empty() {
        eprintln!("{}: No rules selected to run.", "Error".red().bold());
        process::exit(1);
    }

    let mut has_issues = false;
    let mut files_with_issues = 0;
    let mut total_files_processed = 0;
    let mut total_issues_found = 0;
    let mut total_issues_fixed = 0;

    // Process exclude patterns from CLI and config
    let mut exclude_patterns = Vec::new();

    // Add exclude patterns from config
    if let Ok(ref loaded_config) = config::load_config(cli.config.as_deref()) {
        exclude_patterns.extend(loaded_config.global.exclude.clone());
    }

    // Add exclude patterns from CLI (overrides config)
    if let Some(exclude_str) = &cli.exclude {
        exclude_patterns.extend(exclude_str.split(',').map(|s| s.trim().to_string()));
    }

    // Remove duplicates from exclude_patterns
    exclude_patterns.sort();
    exclude_patterns.dedup();

    // Remove duplicates from include_patterns
    include_patterns.sort();
    include_patterns.dedup();

    // User friendliness improvement: When CLI paths are provided, don't use include patterns for filtering
    // Only apply include patterns when no CLI paths are provided
    let use_include_patterns = cli.paths.is_empty();

    // Warn user if they combined --include flag with paths
    if !cli.paths.is_empty() && !include_patterns.is_empty() {
        println!("{}: You've specified both paths and --include patterns.", "Warning".yellow().bold());
        println!("The explicit paths will take precedence, and include patterns will be ignored.");
        println!("For best results, use either paths OR include patterns, not both.");
        println!();
    }

    if cli.verbose {
        if !exclude_patterns.is_empty() {
            println!("Excluding the following patterns:");
            for pattern in &exclude_patterns {
                println!("  - {}", pattern);
            }
            println!();
        }

        if !include_patterns.is_empty() && use_include_patterns {
            println!("Including only the following patterns:");
            for pattern in &include_patterns {
                println!("  - {}", pattern);
            }
            println!();
        } else if !include_patterns.is_empty() {
            println!("Include patterns specified but not applied because explicit paths were provided:");
            for pattern in &include_patterns {
                println!("  - {}", pattern);
            }
            println!();
        }
    }

    // If no CLI paths are provided but include patterns exist, process the current directory
    if cli.paths.is_empty() && !include_patterns.is_empty() {
        // Use current directory as the path
        let path_str = ".";
        let path = Path::new(path_str);

        // Process directory recursively
        if cli.respect_gitignore {
            // Create a walker that respects .gitignore files
            let walker = WalkBuilder::new(path)
                .follow_links(true)
                .hidden(true)
                .git_global(true)
                .git_ignore(true)
                .git_exclude(true)
                .add_custom_ignore_filename(".gitignore")
                .build()
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().map_or(false, |ft| ft.is_file())
                        && e.path().extension().map_or(false, |ext| ext == "md")
                });

            for entry in walker {
                let file_path = entry.path().to_string_lossy();

                // Check if file is excluded based on patterns
                let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                // Check if file should be included based on patterns
                let included = rumdl::should_include(&file_path, &include_patterns);

                if excluded || !included {
                    if cli.verbose {
                        if excluded {
                            println!("Skipping excluded file: {}", file_path);
                        } else if !included {
                            println!("Skipping file not matching include patterns: {}", file_path);
                        }
                    }
                    continue;
                }

                total_files_processed += 1;
                let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                if has_warnings {
                    has_issues = true;
                    files_with_issues += 1;
                    total_issues_found += warnings_found;
                    total_issues_fixed += warnings_fixed;
                }
            }
        } else {
            // Use walkdir if respect_gitignore is disabled
            let dir_iter = WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().map_or(false, |ext| ext == "md")
                });

            for entry in dir_iter {
                let file_path = entry.path().to_string_lossy();

                // Check if file is excluded based on patterns
                let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                // Check if file should be included based on patterns
                let included = rumdl::should_include(&file_path, &include_patterns);

                if excluded || !included {
                    if cli.verbose {
                        if excluded {
                            println!("Skipping excluded file: {}", file_path);
                        } else if !included {
                            println!("Skipping file not matching include patterns: {}", file_path);
                        }
                    }
                    continue;
                }

                total_files_processed += 1;
                let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                if has_warnings {
                    has_issues = true;
                    files_with_issues += 1;
                    total_issues_found += warnings_found;
                    total_issues_fixed += warnings_fixed;
                }
            }
        }
    }

    for path_str in &cli.paths {
        let path = Path::new(path_str);

        if !path.exists() {
            eprintln!(
                "{}: Path does not exist: {}",
                "Error".red().bold(),
                display_path(path_str).blue().underline()
            );
            has_issues = true;
            continue;
        }

        if path.is_file() {
            // Check if file is excluded based on patterns
            let excluded = rumdl::should_exclude(path_str, &exclude_patterns, cli.respect_gitignore);
            // Only apply include patterns when no CLI paths are provided directly
            let included = if use_include_patterns {
                rumdl::should_include(path_str, &include_patterns)
            } else {
                true
            };

            if excluded || !included {
                if cli.verbose {
                    if excluded {
                        println!("Skipping excluded file: {}", path_str);
                    } else if !included {
                        println!("Skipping file not matching include patterns: {}", path_str);
                    }
                }
                continue;
            }

            // Check if file should be ignored by gitignore when respect_gitignore is enabled
            if cli.respect_gitignore {
                // Create a walker that respects .gitignore files
                let walker = WalkBuilder::new(path)
                    .follow_links(true)
                    .hidden(true)
                    .git_global(true)
                    .git_ignore(true)
                    .git_exclude(true)
                    .add_custom_ignore_filename(".gitignore")
                    .build()
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_type().map_or(false, |ft| ft.is_file())
                            && e.path().extension().map_or(false, |ext| ext == "md")
                    });

                for entry in walker {
                    let file_path = entry.path().to_string_lossy();

                    // Check if file is excluded based on patterns
                    let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                    // Only apply include patterns when no CLI paths are provided directly
                    let included = if use_include_patterns {
                        rumdl::should_include(&file_path, &include_patterns)
                    } else {
                        true
                    };

                    if excluded || !included {
                        if cli.verbose {
                            if excluded {
                                println!("Skipping excluded file: {}", file_path);
                            } else if !included {
                                println!("Skipping file not matching include patterns: {}", file_path);
                            }
                        }
                        continue;
                    }

                    total_files_processed += 1;
                    let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                    if has_warnings {
                        has_issues = true;
                        files_with_issues += 1;
                        total_issues_found += warnings_found;
                        total_issues_fixed += warnings_fixed;
                    }
                }
            } else {
                // Use walkdir if respect_gitignore is disabled
                let dir_iter = WalkDir::new(path)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_type().is_file()
                            && e.path().extension().map_or(false, |ext| ext == "md")
                    });

                for entry in dir_iter {
                    let file_path = entry.path().to_string_lossy();

                    // Check if file is excluded based on patterns
                    let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                    // Only apply include patterns when no CLI paths are provided directly
                    let included = if use_include_patterns {
                        rumdl::should_include(&file_path, &include_patterns)
                    } else {
                        true
                    };

                    if excluded || !included {
                        if cli.verbose {
                            if excluded {
                                println!("Skipping excluded file: {}", file_path);
                            } else if !included {
                                println!("Skipping file not matching include patterns: {}", file_path);
                            }
                        }
                        continue;
                    }

                    total_files_processed += 1;
                    let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                    if has_warnings {
                        has_issues = true;
                        files_with_issues += 1;
                        total_issues_found += warnings_found;
                        total_issues_fixed += warnings_fixed;
                    }
                }
            }
        } else if path.is_dir() {
            // Process directory recursively
            // If respect_gitignore is enabled, use the ignore crate's WalkBuilder
            if cli.respect_gitignore {
                // Create a walker that respects .gitignore files
                let walker = WalkBuilder::new(path)
                    .follow_links(true)
                    .hidden(true)
                    .git_global(true)
                    .git_ignore(true)
                    .git_exclude(true)
                    .add_custom_ignore_filename(".gitignore")
                    .build()
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_type().map_or(false, |ft| ft.is_file())
                            && e.path().extension().map_or(false, |ext| ext == "md")
                    });

                for entry in walker {
                    let file_path = entry.path().to_string_lossy();

                    // Check if file is excluded based on patterns
                    let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                    // Only apply include patterns when no CLI paths are provided directly
                    let included = if use_include_patterns {
                        rumdl::should_include(&file_path, &include_patterns)
                    } else {
                        true
                    };

                    if excluded || !included {
                        if cli.verbose {
                            if excluded {
                                println!("Skipping excluded file: {}", file_path);
                            } else if !included {
                                println!("Skipping file not matching include patterns: {}", file_path);
                            }
                        }
                        continue;
                    }

                    total_files_processed += 1;
                    let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                    if has_warnings {
                        has_issues = true;
                        files_with_issues += 1;
                        total_issues_found += warnings_found;
                        total_issues_fixed += warnings_fixed;
                    }
                }
            } else {
                // Use walkdir if respect_gitignore is disabled
                let dir_iter = WalkDir::new(path)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_type().is_file()
                            && e.path().extension().map_or(false, |ext| ext == "md")
                    });

                for entry in dir_iter {
                    let file_path = entry.path().to_string_lossy();

                    // Check if file is excluded based on patterns
                    let excluded = rumdl::should_exclude(&file_path, &exclude_patterns, cli.respect_gitignore);
                    // Only apply include patterns when no CLI paths are provided directly
                    let included = if use_include_patterns {
                        rumdl::should_include(&file_path, &include_patterns)
                    } else {
                        true
                    };

                    if excluded || !included {
                        if cli.verbose {
                            if excluded {
                                println!("Skipping excluded file: {}", file_path);
                            } else if !included {
                                println!("Skipping file not matching include patterns: {}", file_path);
                            }
                        }
                        continue;
                    }

                    total_files_processed += 1;
                    let (has_warnings, warnings_found, warnings_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                    if has_warnings {
                        has_issues = true;
                        files_with_issues += 1;
                        total_issues_found += warnings_found;
                        total_issues_fixed += warnings_fixed;
                    }
                }
            }
        }
    }

    // Print a single, concise Ruff-like summary
    if has_issues {
        if cli.fix {
            println!(
                "\nFixed {} {} in {} {}",
                total_issues_fixed,
                if total_issues_fixed == 1 {
                    "issue"
                } else {
                    "issues"
                },
                files_with_issues,
                if files_with_issues == 1 {
                    "file"
                } else {
                    "files"
                }
            );
        } else {
            println!(
                "\nFound {} {} in {} {} ({} {} checked)",
                total_issues_found,
                if total_issues_found == 1 {
                    "issue"
                } else {
                    "issues"
                },
                files_with_issues,
                if files_with_issues == 1 {
                    "file"
                } else {
                    "files"
                },
                total_files_processed,
                if total_files_processed == 1 {
                    "file"
                } else {
                    "files"
                }
            );
            println!("Run with `--fix` to automatically fix issues");
        }

        // Print profiling information if profile flag is set
        if cli.profile {
            println!("\n{}", rumdl::get_profiling_report());
        }

        process::exit(1);
    } else {
        println!("{} No issues found", "✓".green().bold());

        // Print profiling information if profile flag is set
        if cli.profile {
            println!("\n{}", rumdl::get_profiling_report());
        }
    }

    Ok(())
}
