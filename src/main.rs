use clap::{Parser, Subcommand};
use colored::*;
use rumdl::rule::{Rule, LintWarning};
use rumdl::rules::*;
use rumdl::md046_code_block_style::CodeBlockStyle;
use rumdl::md048_code_fence_style::CodeFenceStyle;
use rumdl::md049_emphasis_style::EmphasisStyle;
use rumdl::md050_strong_style::StrongStyle;
use std::fs;
use std::path::Path;
use std::process;
use walkdir::WalkDir;
use ignore;
use glob;

mod config;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Files or directories to lint
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

    /// Respect .gitignore files when scanning directories
    #[arg(long)]
    respect_gitignore: bool,

    /// Debug gitignore patterns for a specific file
    #[arg(long)]
    debug_gitignore: bool,

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,
    
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
    rules.push(Box::new(MD001HeadingIncrement::default()));
    rules.push(Box::new(MD002FirstHeadingH1::default()));
    rules.push(Box::new(MD003HeadingStyle::default()));
    rules.push(Box::new(MD004UnorderedListStyle::default()));
    rules.push(Box::new(MD005ListIndent::default()));
    rules.push(Box::new(MD006StartBullets::default()));
    rules.push(Box::new(MD007ULIndent::default()));
    
    // Configure MD008 from config if available
    let md008 = if let Some(style) = config::get_rule_config_value::<char>(&config, "MD008", "style") {
        Box::new(MD008ULStyle::new(style))
    } else {
        Box::new(MD008ULStyle::default())
    };
    rules.push(md008);
    
    rules.push(Box::new(MD009TrailingSpaces::default()));
    rules.push(Box::new(MD010NoHardTabs::default()));
    rules.push(Box::new(MD011ReversedLink::default()));
    rules.push(Box::new(MD012NoMultipleBlanks::default()));
    
    // Configure MD013 from config if available
    let md013 = {
        let line_length = config::get_rule_config_value::<usize>(&config, "MD013", "line_length")
            .unwrap_or(80);
        let code_blocks = config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks")
            .unwrap_or(true);
        let tables = config::get_rule_config_value::<bool>(&config, "MD013", "tables")
            .unwrap_or(false);
        let headings = config::get_rule_config_value::<bool>(&config, "MD013", "headings")
            .unwrap_or(true);
        let strict = config::get_rule_config_value::<bool>(&config, "MD013", "strict")
            .unwrap_or(false);
        
        Box::new(MD013LineLength::new(line_length, code_blocks, tables, headings, strict))
    };
    rules.push(md013);
    
    rules.push(Box::new(MD014CommandsShowOutput::default()));
    rules.push(Box::new(MD015NoMissingSpaceAfterListMarker::default()));
    rules.push(Box::new(MD016NoMultipleSpaceAfterListMarker::default()));
    rules.push(Box::new(MD017NoEmphasisAsHeading::default()));
    rules.push(Box::new(MD018NoMissingSpaceAtx::default()));
    rules.push(Box::new(MD019NoMultipleSpaceAtx::default()));
    rules.push(Box::new(MD020NoMissingSpaceClosedAtx::default()));
    rules.push(Box::new(MD021NoMultipleSpaceClosedAtx::default()));
    rules.push(Box::new(MD022BlanksAroundHeadings::default()));
    rules.push(Box::new(MD023HeadingStartLeft::default()));
    rules.push(Box::new(MD024MultipleHeadings::default()));
    rules.push(Box::new(MD025SingleTitle::default()));
    rules.push(Box::new(MD026NoTrailingPunctuation::default()));
    rules.push(Box::new(MD027MultipleSpacesBlockquote::default()));
    rules.push(Box::new(MD028NoBlanksBlockquote::default()));
    rules.push(Box::new(MD029OrderedListPrefix::default()));
    rules.push(Box::new(MD030ListMarkerSpace::default()));
    rules.push(Box::new(MD031BlanksAroundFences::default()));
    rules.push(Box::new(MD032BlanksAroundLists::default()));
    rules.push(Box::new(MD033NoInlineHtml::default()));
    rules.push(Box::new(MD034NoBareUrls::default()));
    rules.push(Box::new(MD035HRStyle::default()));
    rules.push(Box::new(MD036NoEmphasisOnlyFirst::default()));
    rules.push(Box::new(MD037SpacesAroundEmphasis::default()));
    rules.push(Box::new(MD038NoSpaceInCode::default()));
    rules.push(Box::new(MD039NoSpaceInLinks::default()));
    rules.push(Box::new(MD040FencedCodeLanguage::default()));
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
    
    rules.push(Box::new(MD047FileEndNewline::default()));
    
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
    
    rules.push(Box::new(MD055TablePipeStyle::default()));
    rules.push(Box::new(MD056TableColumnCount));
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
        paths: Vec::new(),
        config: None,
        fix: false,
        list_rules: true,
        disable: None,
        enable: None,
        exclude: None,
        respect_gitignore: false,
        debug_gitignore: false,
        verbose: false,
        command: None,
    });
    
    // Sort rules by name
    let mut rule_info: Vec<(&str, &str)> = rules.iter()
        .map(|rule| (rule.name(), rule.description()))
        .collect();
    rule_info.sort_by(|a, b| a.0.cmp(b.0));
    
    // Print rule names and descriptions
    for (name, description) in rule_info {
        println!("{} - {}", name, description);
    }
}

fn process_file(path: &str, rules: &[Box<dyn Rule>], fix: bool, verbose: bool) -> (bool, usize, usize) {
    let _timer = rumdl::profiling::ScopedTimer::new(&format!("process_file:{}", path));
    
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("{}: {}", "Error reading file".red().bold(), format!("{}: {}", path, err));
            return (false, 0, 0);
        }
    };
    
    let mut has_warnings = false;
    let mut total_warnings = 0;
    let mut total_fixed = 0;
    let mut all_warnings = Vec::new();
    
    {
        let _timer = rumdl::profiling::ScopedTimer::new(&format!("check_rules:{}", path));
        
        // Collect all warnings first
        for rule in rules {
            let _rule_timer = rumdl::profiling::ScopedTimer::new(&format!("rule:{}:{}", rule.name(), path));
            
            match rule.check(&content) {
                Ok(warnings) => {
                    if !warnings.is_empty() {
                        // Filter out warnings for lines where the rule is disabled
                        let filtered_warnings: Vec<LintWarning> = warnings.into_iter()
                            .filter(|warning| !rumdl::rule::is_rule_disabled_at_line(&content, rule.name(), warning.line - 1))
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
                    eprintln!("{}: {} on file {}: {}", 
                        "Error".red().bold(), 
                        rule.name().yellow(), 
                        path.blue().underline(), 
                        err);
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
            let fix_indicator = if fixable && fix { "[fixed]".green() } else if fixable { "[*]".green() } else { "".normal() };
            
            println!("{}:{}:{}: {} {} {}", 
                path.blue().underline(),
                warning.line.to_string().cyan(),
                warning.column.to_string().cyan(),
                format!("[{}]", rule_name).yellow(),
                warning.message,
                fix_indicator);
        }
    } else {
        if verbose {
            println!("{} No issues found in {}", "✓".green(), path.blue().underline());
        }
    }
    
    // Apply fixes in a single pass if requested
    if fix && has_warnings {
        let _timer = rumdl::profiling::ScopedTimer::new(&format!("fix_file:{}", path));
        
        let mut fixed_content = content.clone();
        let mut fixed_warnings = 0;
        
        // Group all warnings by rule, then apply each rule's fixes in a single operation
        let mut rule_to_warnings: std::collections::HashMap<&'static str, Vec<&LintWarning>> = std::collections::HashMap::new();
        
        for (rule_name, _, warning) in &all_warnings {
            if warning.fix.is_some() {
                rule_to_warnings.entry(rule_name).or_insert_with(Vec::new).push(warning);
            }
        }
        
        // Apply fixes for each rule
        for (rule_name, warnings) in rule_to_warnings {
            // Find the rule by name
            if let Some(rule) = rules.iter().find(|r| r.name() == rule_name) {
                let _fix_timer = rumdl::profiling::ScopedTimer::new(&format!("fix_rule:{}:{}", rule_name, path));
                
                match rule.fix(&fixed_content) {
                    Ok(new_content) => {
                        fixed_warnings += warnings.len();
                        fixed_content = new_content;
                    }
                    Err(err) => {
                        eprintln!("  {} {}: {}", 
                            "Error fixing issues with".red().bold(), 
                            rule_name.yellow(), 
                            err);
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
                    eprintln!("{} {}: {}", 
                        "Error writing fixed content to".red().bold(), 
                        path.blue().underline(), 
                        err);
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
    use std::path::Path;
    use std::fs;

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
    let canonical_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    
    for entry_result in walker {
        if let Ok(entry) = entry_result {
            let entry_canonical = entry.path().canonicalize().unwrap_or_else(|_| entry.path().to_path_buf());
            if entry_canonical == canonical_path {
                found_in_walker = true;
                break;
            }
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
        for entry_result in walker {
            if let Ok(entry) = entry_result {
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
    }
    
    println!();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _timer = rumdl::profiling::ScopedTimer::new("main");
    
    let cli = Cli::parse();
    
    if cli.list_rules {
        list_available_rules();
        return Ok(());
    }
    
    // Handle init command to create a default configuration file
    if let Some(Commands::Init) = cli.command {
        let config_path = ".rumdl.toml";
        match config::create_default_config(config_path) {
            Ok(_) => {
                println!("{}: Created default configuration file at {}", "Success".green().bold(), config_path);
                println!("You can now customize the configuration to suit your needs.");
                return Ok(());
            }
            Err(err) => {
                eprintln!("{}: Failed to create configuration file: {}", "Error".red().bold(), err);
                process::exit(1);
            }
        }
    }
    
    // Only require paths if we're not running a subcommand
    if cli.paths.is_empty() && cli.command.is_none() {
        eprintln!("{}: No paths provided. Please specify at least one file or directory to lint.", "Error".red().bold());
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
    let mut respect_gitignore = cli.respect_gitignore;
    
    // Add exclude patterns from config
    if let Ok(config) = config::load_config(cli.config.as_deref()) {
        exclude_patterns.extend(config.global.exclude.clone());
        
        // Check if respect_gitignore is set in config (and not overridden in CLI)
        if config.global.respect_gitignore && !cli.respect_gitignore {
            respect_gitignore = true;
        }
    }
    
    // Add exclude patterns from CLI (overrides config)
    if let Some(exclude_str) = &cli.exclude {
        exclude_patterns.extend(exclude_str.split(',').map(|s| s.trim().to_string()));
    }
    
    // Remove duplicates from exclude_patterns
    exclude_patterns.sort();
    exclude_patterns.dedup();
    
    if cli.verbose && !exclude_patterns.is_empty() {
        println!("Excluding the following patterns:");
        for pattern in &exclude_patterns {
            println!("  - {}", pattern);
        }
        println!();
    }
    
    for path_str in &cli.paths {
        let path = Path::new(path_str);
        
        if !path.exists() {
            eprintln!("{}: Path does not exist: {}", "Error".red().bold(), path_str);
            has_issues = true;
            continue;
        }
        
        if path.is_file() {
            // Check if file is excluded based on patterns
            let excluded = rumdl::should_exclude(path_str, &exclude_patterns);
            
            if excluded {
                if cli.verbose {
                    println!("Skipping excluded file: {}", path_str);
                }
                continue;
            }
            
            // Check if file should be ignored by gitignore when respect_gitignore is enabled
            if respect_gitignore {
                use ignore::WalkBuilder;
                
                let file_path = Path::new(path_str);
                let canonical_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
                
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
                
                for entry_result in walker {
                    if let Ok(entry) = entry_result {
                        let entry_canonical = entry.path().canonicalize().unwrap_or_else(|_| entry.path().to_path_buf());
                        if entry_canonical == canonical_path {
                            found_in_walker = true;
                            break;
                        }
                    }
                }
                
                if !found_in_walker {
                    if cli.verbose {
                        println!("Skipping file ignored by gitignore: {}", path_str);
                    }
                    continue;
                }
            }
            
            total_files_processed += 1;
            let (file_has_issues, issues_found, issues_fixed) = process_file(path_str, &rules, cli.fix, cli.verbose);
            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                total_issues_found += issues_found;
                total_issues_fixed += issues_fixed;
            }
        } else if path.is_dir() {
            // Process directory recursively
            // If respect_gitignore is enabled, use the ignore crate's WalkBuilder
            if respect_gitignore {
                use ignore::WalkBuilder;
                
                // Create a walker that respects .gitignore files
                let walker = WalkBuilder::new(path)
                    .hidden(true)
                    .git_global(true)
                    .git_ignore(true)
                    .git_exclude(true)
                    .add_custom_ignore_filename(".gitignore")
                    .build();
                
                for entry_result in walker {
                    if let Ok(entry) = entry_result {
                        // Only process Markdown files
                        if entry.file_type().map_or(false, |ft| ft.is_file()) 
                            && entry.path().extension().map_or(false, |ext| ext == "md") {
                            
                            let file_path = entry.path().to_string_lossy().to_string();
                            
                            // Check if file is excluded based on patterns
                            let excluded = rumdl::should_exclude(&file_path, &exclude_patterns);
                            
                            if excluded {
                                if cli.verbose {
                                    println!("Skipping excluded file: {}", file_path);
                                }
                                continue;
                            }
                            
                            total_files_processed += 1;
                            let (file_has_issues, issues_found, issues_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                            if file_has_issues {
                                has_issues = true;
                                files_with_issues += 1;
                                total_issues_found += issues_found;
                                total_issues_fixed += issues_fixed;
                            }
                        }
                    }
                }
            } else {
                // Use walkdir if respect_gitignore is disabled
                match WalkDir::new(path)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file() && e.path().extension().map_or(false, |ext| ext == "md"))
                {
                    dir_iter => {
                        for entry in dir_iter {
                            let file_path = entry.path().to_string_lossy().to_string();
                            
                            // Check if file is excluded based on patterns
                            let excluded = rumdl::should_exclude(&file_path, &exclude_patterns);
                            
                            if excluded {
                                if cli.verbose {
                                    println!("Skipping excluded file: {}", file_path);
                                }
                                continue;
                            }
                            
                            total_files_processed += 1;
                            let (file_has_issues, issues_found, issues_fixed) = process_file(&file_path, &rules, cli.fix, cli.verbose);
                            if file_has_issues {
                                has_issues = true;
                                files_with_issues += 1;
                                total_issues_found += issues_found;
                                total_issues_fixed += issues_fixed;
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Print a single, concise Ruff-like summary
    if has_issues {
        if cli.fix {
            println!("\nFixed {} {} in {} {}", 
                total_issues_fixed,
                if total_issues_fixed == 1 { "issue" } else { "issues" },
                files_with_issues, 
                if files_with_issues == 1 { "file" } else { "files" });
        } else {
            println!("\nFound {} {} in {} {} ({} {} checked)", 
                total_issues_found,
                if total_issues_found == 1 { "issue" } else { "issues" },
                files_with_issues, 
                if files_with_issues == 1 { "file" } else { "files" },
                total_files_processed,
                if total_files_processed == 1 { "file" } else { "files" });
            println!("Run with `--fix` to automatically fix issues");
        }
        
        // Print profiling information if verbose or debug mode
        if cli.verbose {
            println!("\n{}", rumdl::get_profiling_report());
        }
        
        process::exit(1);
    } else {
        println!("{} No issues found", "✓".green().bold());
        
        // Print profiling information if verbose or debug mode
        if cli.verbose {
            println!("\n{}", rumdl::get_profiling_report());
        }
    }
    
    Ok(())
} 