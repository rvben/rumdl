use clap::Parser;
use rumdl::rule::Rule;
use rumdl::rules::{
    MD001HeadingIncrement, MD002FirstHeadingH1, MD003HeadingStyle, MD004UnorderedListStyle,
    MD005ListIndent, MD006StartBullets, MD007ULIndent, MD008ULStyle,
    MD009TrailingSpaces, MD010NoHardTabs, MD011ReversedLink, MD012NoMultipleBlanks,
    MD013LineLength, MD014CommandsShowOutput, MD015NoMissingSpaceAfterListMarker,
    MD016NoMultipleSpaceAfterListMarker, MD017NoEmphasisAsHeading, MD018NoMissingSpaceAtx,
    MD019NoMultipleSpaceAtx, MD020NoMissingSpaceClosedAtx, MD021NoMultipleSpaceClosedAtx,
    MD022BlanksAroundHeadings, MD023HeadingStartLeft, MD024MultipleHeadings, MD025SingleTitle,
    MD026NoTrailingPunctuation, MD027MultipleSpacesBlockquote, MD028NoBlanksBlockquote,
    MD029OrderedListPrefix, MD030ListMarkerSpace, MD031BlanksAroundFences, MD032BlanksAroundLists,
    MD033NoInlineHtml, MD034NoBareUrls, MD035HRStyle, MD036NoEmphasisOnlyFirst,
    MD037SpacesAroundEmphasis, MD038NoSpaceInCode, MD039NoSpaceInLinks,
    MD040FencedCodeLanguage, MD041FirstLineHeading, MD042NoEmptyLinks, MD043RequiredHeadings,
    MD044ProperNames, MD045NoAltText, MD046CodeBlockStyle, MD047FileEndNewline,
    MD048CodeFenceStyle, MD049EmphasisStyle, MD050StrongStyle, MD051LinkFragments,
    MD052ReferenceLinkImages, MD053LinkImageReferenceDefinitions,
};
use rumdl::md046_code_block_style::CodeBlockStyle;
use rumdl::md048_code_fence_style::CodeFenceStyle;
use rumdl::md049_emphasis_style::EmphasisStyle;
use rumdl::md050_strong_style::StrongStyle;
use std::fs;
use std::path::Path;
use std::process;
use colored::Colorize;
use walkdir;

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

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,
}

fn get_rules(opts: &Cli) -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = Vec::new();

    // Add implemented rules
    rules.push(Box::new(MD001HeadingIncrement::default()));
    rules.push(Box::new(MD002FirstHeadingH1::default()));
    rules.push(Box::new(MD003HeadingStyle::default()));
    rules.push(Box::new(MD004UnorderedListStyle::default()));
    rules.push(Box::new(MD005ListIndent::default()));
    rules.push(Box::new(MD006StartBullets::default()));
    rules.push(Box::new(MD007ULIndent::default()));
    rules.push(Box::new(MD008ULStyle::default()));
    rules.push(Box::new(MD009TrailingSpaces::default()));
    rules.push(Box::new(MD010NoHardTabs::default()));
    rules.push(Box::new(MD011ReversedLink::default()));
    rules.push(Box::new(MD012NoMultipleBlanks::default()));
    rules.push(Box::new(MD013LineLength::default()));
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
    rules.push(Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)));
    rules.push(Box::new(MD047FileEndNewline::default()));
    rules.push(Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)));
    rules.push(Box::new(MD049EmphasisStyle::new(EmphasisStyle::Consistent)));
    rules.push(Box::new(MD050StrongStyle::new(StrongStyle::Consistent)));
    rules.push(Box::new(MD051LinkFragments::new()));
    rules.push(Box::new(MD052ReferenceLinkImages::new()));
    rules.push(Box::new(MD053LinkImageReferenceDefinitions::default()));

    // Filter rules based on enable/disable options
    if let Some(enable) = &opts.enable {
        let enabled_rules: Vec<&str> = enable.split(',').collect();
        rules.retain(|rule| enabled_rules.contains(&rule.name()));
    } else if let Some(disable) = &opts.disable {
        let disabled_rules: Vec<&str> = disable.split(',').collect();
        rules.retain(|rule| !disabled_rules.contains(&rule.name()));
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
        verbose: false,
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
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("{}: {}", "Error reading file".red().bold(), format!("{}: {}", path, err));
            return (false, 0, 0);
        }
    };
    
    let mut has_warnings = false;
    let mut fixed_content = content.clone();
    let mut total_warnings = 0;
    let mut total_fixed = 0;
    let mut all_warnings = Vec::new();
    
    // Collect all warnings first
    for rule in rules {
        match rule.check(&content) {
            Ok(warnings) => {
                if !warnings.is_empty() {
                    has_warnings = true;
                    total_warnings += warnings.len();
                    
                    for warning in warnings {
                        all_warnings.push((rule.name(), warning));
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
    
    // Sort warnings by line and column
    all_warnings.sort_by(|a, b| {
        let line_cmp = a.1.line.cmp(&b.1.line);
        if line_cmp == std::cmp::Ordering::Equal {
            a.1.column.cmp(&b.1.column)
        } else {
            line_cmp
        }
    });
    
    // Display warnings in a clean format
    if !all_warnings.is_empty() {
        for (rule_name, warning) in &all_warnings {
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
    
    // Apply fixes if requested
    if fix && has_warnings {
        for rule in rules {
            match rule.check(&fixed_content) {
                Ok(warnings) => {
                    if !warnings.is_empty() {
                        match rule.fix(&fixed_content) {
                            Ok(new_content) => {
                                let fixed_count = warnings.len();
                                total_fixed += fixed_count;
                                fixed_content = new_content;
                            }
                            Err(err) => {
                                eprintln!("  {} {}: {}", 
                                    "Error fixing issues with".red().bold(), 
                                    rule.name().yellow(), 
                                    err);
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
        
        // Write fixed content back to file
        if total_fixed > 0 {
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

fn main() {
    let cli = Cli::parse();
    
    if cli.list_rules {
        list_available_rules();
        return;
    }
    
    if cli.paths.is_empty() {
        eprintln!("{}: No paths provided. Please specify at least one file or directory to lint.", "Error".red().bold());
        process::exit(1);
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
    
    for path_str in &cli.paths {
        let path = Path::new(path_str);
        
        if !path.exists() {
            eprintln!("{}: Path does not exist: {}", "Error".red().bold(), path_str);
            has_issues = true;
            continue;
        }
        
        if path.is_file() {
            total_files_processed += 1;
            let (file_has_issues, issues_found, issues_fixed) = process_file(path_str, &rules, cli.fix, cli.verbose);
            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                total_issues_found += issues_found;
                total_issues_fixed += issues_fixed;
            }
        } else if path.is_dir() {
            // Process directory recursively using walkdir
            match walkdir::WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && e.path().extension().map_or(false, |ext| ext == "md"))
            {
                dir_iter => {
                    for entry in dir_iter {
                        total_files_processed += 1;
                        let file_path = entry.path().to_string_lossy().to_string();
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
                
            println!("Run with {} to automatically fix issues", "`--fix`".green());
        }
        process::exit(1);
    } else if total_files_processed > 0 {
        println!("{} No issues found", "✓".green().bold());
    }
} 