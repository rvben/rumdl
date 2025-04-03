use clap::{Parser, Subcommand};
use colored::*;
use std::path::Path;
use std::process;
use std::io::{self, Write};
use std::time::Instant;
use walkdir::WalkDir;

use rumdl::rule::Rule;
use rumdl::rules::*;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

mod config;

/// Flags for optimization settings
#[derive(Debug, Clone, Copy)]
struct OptimizeFlags {
    enable_document_structure: bool,
    enable_selective_linting: bool,
    enable_parallel: bool,
}

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
    
    /// Use parallel rule execution for better performance on large files
    #[arg(long)]
    parallel: bool,
    
    /// Use document structure preprocessing for better performance
    #[arg(long)]
    structure: bool,
    
    /// Use selective rule application for better performance
    #[arg(long)]
    selective: bool,
    
    /// Use all optimizations (equivalent to --parallel --structure --selective)
    #[arg(long)]
    optimize: bool,

    /// Quiet mode - don't show banner
    #[arg(short, long)]
    quiet: bool,

    /// Command to run
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new configuration file
    Init,
}

// Get a complete set of enabled rules based on CLI options
fn get_enabled_rules(cli: &Cli) -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = Vec::new();
    
    // Add all the implemented rules
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
    rules.push(Box::new(MD013LineLength::new(80, true, false, true, false)));
    rules.push(Box::new(MD014CommandsShowOutput::default()));
    rules.push(Box::new(MD015NoMissingSpaceAfterListMarker::default()));
    rules.push(Box::new(MD016NoMultipleSpaceAfterListMarker::default()));
    rules.push(Box::new(MD017NoEmphasisAsHeading::default()));
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
    rules.push(Box::new(MD037SpacesAroundEmphasis {}));
    rules.push(Box::new(MD038NoSpaceInCode {}));
    rules.push(Box::new(MD039NoSpaceInLinks {}));
    rules.push(Box::new(MD040FencedCodeLanguage {}));
    rules.push(Box::new(MD041FirstLineHeading::default()));
    rules.push(Box::new(MD042NoEmptyLinks::new()));
    rules.push(Box::new(MD043RequiredHeadings::new(Vec::new())));
    rules.push(Box::new(MD044ProperNames::new(Vec::new(), true)));
    rules.push(Box::new(MD045NoAltText::new()));
    rules.push(Box::new(md046_code_block_style::MD046CodeBlockStyle::new(md046_code_block_style::CodeBlockStyle::Consistent)));
    rules.push(Box::new(MD047FileEndNewline::default()));
    rules.push(Box::new(md048_code_fence_style::MD048CodeFenceStyle::new(md048_code_fence_style::CodeFenceStyle::Consistent)));
    rules.push(Box::new(md049_emphasis_style::MD049EmphasisStyle::new(md049_emphasis_style::EmphasisStyle::Consistent)));
    rules.push(Box::new(md050_strong_style::MD050StrongStyle::new(md050_strong_style::StrongStyle::Consistent)));
    rules.push(Box::new(MD051LinkFragments::default()));
    
    // Apply rule enable/disable filters
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

// Print banner with version and application name
fn print_banner() {
    println!("{} v{}", "rumdl".green().bold(), env!("CARGO_PKG_VERSION"));
    println!("Rust Markdown Linter");
    println!();
}

// Find all markdown files in the provided paths
fn find_markdown_files(paths: &[String]) -> Vec<String> {
    let mut file_paths = Vec::new();
    
    for path_str in paths {
        let path = Path::new(path_str);
        
        if !path.exists() {
            eprintln!(
                "{}: Path does not exist: {}",
                "Error".red().bold(),
                path_str
            );
            process::exit(1);
        }
        
        if path.is_file() {
            // Check if file is a markdown file
            if path.extension().map_or(false, |ext| ext == "md") {
                file_paths.push(path_str.to_string());
            }
        } else if path.is_dir() {
            // Find markdown files in the directory
            let walker = WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().map_or(false, |ext| ext == "md")
                });
            
            for entry in walker {
                let file_path = entry.path().to_string_lossy().to_string();
                file_paths.push(file_path);
            }
        }
    }
    
    file_paths
}

// Process file operation
fn process_file(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    fix: bool,
    verbose: bool,
    _optimize_flags: OptimizeFlags,
) -> (bool, usize, usize) {
    if verbose {
        println!("Processing file: {}", file_path);
    }
    
    // Read file content
    let mut content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!(
                "{}: Could not read file {}: {}",
                "Error".red().bold(),
                file_path,
                err
            );
            return (false, 0, 0);
        }
    };
    
    // Use the standard lint function - we've fixed the debug output in the lint functions
    let warnings_result = rumdl::lint(&content, rules);
    
    // Combine all warnings
    let mut all_warnings = match warnings_result {
        Ok(warnings) => warnings,
        Err(_) => Vec::new()
    };
    
    // Sort warnings by line number, then column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });
    
    let total_warnings = all_warnings.len();
    
    // If no warnings, return early
    if total_warnings == 0 {
        return (false, 0, 0);
    }
    
    // Print warnings - only print the warning location and message, not the content
    for warning in &all_warnings {
        let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
        
        // Add fix indicator if this warning has a fix
        let fix_indicator = if warning.fix.is_some() { " [*]" } else { "" };
        
        // Print the warning in the format: file:line:column: [rule] message [*]
        println!(
            "{}:{}:{}: {} {}{}",
            file_path.blue().underline(),
            warning.line.to_string().cyan(),
            warning.column.to_string().cyan(),
            format!("[{}]", rule_name).yellow(),
            warning.message,
            fix_indicator
        );
    }
    
    // Fix issues if requested
    let mut warnings_fixed = 0;
    if fix {
        for rule in rules {
            // Skip rules that don't have fixes
            if all_warnings.iter().any(|w| {
                w.rule_name.as_deref() == Some(rule.name()) && w.fix.is_some()
            }) {
                match rule.fix(&content) {
                    Ok(fixed_content) => {
                        if fixed_content != content {
                            content = fixed_content;
                            // Apply fixes for this rule - we consider all warnings for the rule fixed
                            warnings_fixed += all_warnings.iter()
                                .filter(|w| w.rule_name.as_deref() == Some(rule.name()) && w.fix.is_some())
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
    
    (true, total_warnings, warnings_fixed)
}

#[cfg(feature = "parallel")]
fn process_files_parallel(
    files: &[String],
    rules: &[Box<dyn Rule + Sync>],
    fix: bool,
    verbose: bool,
    _optimize_flags: OptimizeFlags,
) -> (bool, usize, usize, usize, usize) {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    
    let has_issues = AtomicBool::new(false);
    let files_with_issues = AtomicUsize::new(0);
    let total_issues_found = AtomicUsize::new(0);
    let total_issues_fixed = AtomicUsize::new(0);
    let total_files_processed = AtomicUsize::new(0);
    
    // Process files in parallel
    files.par_iter().for_each(|file_path| {
        // Convert Rule + Sync to just Rule for process_file
        let rules_ref = unsafe {
            // This is safe because we're just removing the Sync trait bound
            // without changing the underlying type
            let rules_ptr = rules as *const [Box<dyn Rule + Sync>] as *const [Box<dyn Rule>];
            &*rules_ptr
        };
        
        // Silence any output from the file content during processing in parallel mode
        // We only want to see the warnings, not the file contents
        let (file_has_issues, issues_found, issues_fixed) = 
            process_file(file_path, rules_ref, fix, verbose, _optimize_flags);
        
        if file_has_issues {
            has_issues.store(true, Ordering::Relaxed);
            files_with_issues.fetch_add(1, Ordering::Relaxed);
        }
        
        total_issues_found.fetch_add(issues_found, Ordering::Relaxed);
        total_issues_fixed.fetch_add(issues_fixed, Ordering::Relaxed);
        total_files_processed.fetch_add(1, Ordering::Relaxed);
    });
    
    // Print fix summary if fixes were applied
    let fixed_count = total_issues_fixed.load(Ordering::Relaxed);
    if fix && fixed_count > 0 {
        println!("\n{} Fixed {} issues in {} files",
                 "Fixed:".green().bold(),
                 fixed_count,
                 files_with_issues.load(Ordering::Relaxed));
    }
    
    (
        has_issues.load(Ordering::Relaxed),
        total_files_processed.load(Ordering::Relaxed),
        files_with_issues.load(Ordering::Relaxed),
        total_issues_found.load(Ordering::Relaxed),
        fixed_count,
    )
}

fn main() {
    let timer = rumdl::profiling::ScopedTimer::new("main");
    
    // Initialize CLI and parse arguments
    let cli = Cli::parse();

    // Handle init command separately
    if let Some(Commands::Init) = cli.command {
        // Create default config file
        match config::create_default_config(".rumdl.toml") {
            Ok(_) => {
                println!("Created default configuration file: .rumdl.toml");
                return;
            }
            Err(e) => {
                eprintln!("{}: Failed to create config file: {}", "Error".red().bold(), e);
                std::process::exit(1);
            }
        }
    }
    
    // If no paths provided and not using a subcommand, print error and exit
    if cli.paths.is_empty() {
        eprintln!("{}: No files or directories specified. Please provide at least one path to lint.", "Error".red().bold());
        std::process::exit(1);
    }
    
    // Print banner unless quiet mode is enabled
    if !cli.quiet {
        print_banner();
    }
    
    // Create optimization flags - we'll simplify this since parallel is disabled
    let optimize_flags = OptimizeFlags {
        enable_document_structure: false, // Disable structure optimization
        enable_selective_linting: false,  // Disable selective linting
        enable_parallel: false,           // Disable parallel processing
    };
    
    // Initialize rules
    let enabled_rules = get_enabled_rules(&cli);
    
    // Find all markdown files to check
    let file_paths = find_markdown_files(&cli.paths);
    if file_paths.is_empty() {
        println!("No markdown files found to check.");
        return;
    }
    
    // Confirm with the user if we're fixing a large number of files
    if cli.fix && file_paths.len() > 10 {
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
    
    println!("Checking {} files...", file_paths.len());
    
    let start_time = Instant::now();
    
    let (has_issues, total_files, files_with_issues, total_issues, fixed_count) = if cli.parallel || cli.optimize {
        #[cfg(feature = "parallel")]
        {
            // Convert rules to have Sync bound
            let sync_rules: Vec<Box<dyn Rule + Sync>> = unsafe {
                // This is safe because we're just adding the Sync trait bound,
                // which is valid for our rules when using parallel feature
                std::mem::transmute(enabled_rules)
            };
            
            process_files_parallel(&file_paths, &sync_rules, cli.fix, cli.verbose, optimize_flags)
        }
        
        #[cfg(not(feature = "parallel"))]
        {
            eprintln!("Parallel processing requires the 'parallel' feature to be enabled.");
            eprintln!("Compile with --features parallel or use regular processing.");
            
            // Fall back to regular processing
            let mut has_issues = false;
            let mut files_with_issues = 0;
            let mut total_issues = 0;
            let mut total_issues_fixed = 0;
            let mut total_files_processed = 0;
            
            for file_path in &file_paths {
                let (file_has_issues, issues_found, issues_fixed) = 
                    process_file(file_path, &enabled_rules, cli.fix, cli.verbose, optimize_flags);
                
                total_files_processed += 1;
                total_issues_fixed += issues_fixed;
                
                if file_has_issues {
                    has_issues = true;
                    files_with_issues += 1;
                    total_issues += issues_found;
                }
            }
            
            // Print fix summary if fixes were applied
            if cli.fix && total_issues_fixed > 0 {
                println!("\n{} Fixed {} issues in {} files",
                         "Fixed:".green().bold(),
                         total_issues_fixed,
                         files_with_issues);
            }
            
            (has_issues, total_files_processed, files_with_issues, total_issues, total_issues_fixed)
        }
    } else {
        let mut has_issues = false;
        let mut files_with_issues = 0;
        let mut total_issues = 0;
        let mut total_issues_fixed = 0;
        let mut total_files_processed = 0;
        
        for file_path in &file_paths {
            let (file_has_issues, issues_found, issues_fixed) = 
                process_file(file_path, &enabled_rules, cli.fix, cli.verbose, optimize_flags);
            
            total_files_processed += 1;
            total_issues_fixed += issues_fixed;
            
            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                total_issues += issues_found;
            }
        }
        
        // Print fix summary if fixes were applied
        if cli.fix && total_issues_fixed > 0 {
            println!("\n{} Fixed {} issues in {} files",
                     "Fixed:".green().bold(),
                     total_issues_fixed,
                     files_with_issues);
        }
        
        (has_issues, total_files_processed, files_with_issues, total_issues, total_issues_fixed)
    };
    
    let duration = start_time.elapsed();
    let duration_ms = duration.as_secs() * 1000 + duration.subsec_millis() as u64;
    
    // Show results summary
    if has_issues {
        println!("\n{} Found {} issues in {}/{} files ({}ms)",
                 "Issues:".yellow().bold(),
                 total_issues,
                 files_with_issues,
                 total_files,
                 duration_ms);
    } else {
        println!("\n{} No issues found in {} files ({}ms)",
                 "Success:".green().bold(),
                 total_files,
                 duration_ms);
    }
    
    // Print profiling information if enabled
    if cli.profile {
        // Try to get profiling information and handle any errors
        match std::panic::catch_unwind(|| {
            rumdl::profiling::get_report()
        }) {
            Ok(report) => println!("\n{}", report),
            Err(_) => println!("\nProfiling information not available")
        }
    }
    
    // Exit with non-zero status if issues were found
    if has_issues {
        std::process::exit(1);
    }
} 