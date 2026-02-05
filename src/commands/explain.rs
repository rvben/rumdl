//! Handler for the `explain` command.

use colored::*;
use std::fs;

use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_block_utils::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::strong_style::StrongStyle;
use rumdl_lib::rules::*;

/// Handle the explain command: show detailed rule documentation.
pub fn handle_explain(rule_query: &str) {
    // Get all rules
    let all_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
        Box::new(MD005ListIndent::default()),
        Box::new(MD007ULIndent::default()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD010NoHardTabs::default()),
        Box::new(MD011NoReversedLinks {}),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD014CommandsShowOutput::default()),
        Box::new(MD018NoMissingSpaceAtx::new()),
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
        Box::new(MD029OrderedListPrefix::default()),
        Box::new(MD030ListMarkerSpace::default()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD033NoInlineHtml::default()),
        Box::new(MD034NoBareUrls {}),
        Box::new(MD035HRStyle::default()),
        Box::new(MD036NoEmphasisAsHeading::new(".,;:!?".to_string())),
        Box::new(MD037NoSpaceInEmphasis),
        Box::new(MD038NoSpaceInCode::default()),
        Box::new(MD039NoSpaceInLinks),
        Box::new(MD040FencedCodeLanguage::default()),
        Box::new(MD041FirstLineHeading::default()),
        Box::new(MD042NoEmptyLinks::new()),
        Box::new(MD043RequiredHeadings::new(Vec::new())),
        Box::new(MD044ProperNames::new(Vec::new(), true)),
        Box::new(MD045NoAltText::new()),
        Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)),
        Box::new(MD047SingleTrailingNewline),
        Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)),
        Box::new(MD049EmphasisStyle::default()),
        Box::new(MD050StrongStyle::new(StrongStyle::Consistent)),
        Box::new(MD051LinkFragments::new()),
        Box::new(MD052ReferenceLinkImages::new()),
        Box::new(MD053LinkImageReferenceDefinitions::default()),
        Box::new(MD054LinkImageStyle::default()),
        Box::new(MD055TablePipeStyle::default()),
        Box::new(MD056TableColumnCount),
        Box::new(MD057ExistingRelativeLinks::default()),
        Box::new(MD058BlanksAroundTables::default()),
        Box::new(MD059LinkText::default()),
        Box::new(MD060TableFormat::default()),
        Box::new(MD061ForbiddenTerms::default()),
        Box::new(MD062LinkDestinationWhitespace::new()),
        Box::new(MD063HeadingCapitalization::default()),
        Box::new(MD064NoMultipleConsecutiveSpaces::new()),
        Box::new(MD065BlanksAroundHorizontalRules),
        Box::new(MD066FootnoteValidation),
        Box::new(MD067FootnoteDefinitionOrder),
        Box::new(MD068EmptyFootnoteDefinition),
    ];

    // Find the rule
    let rule_query_upper = rule_query.to_ascii_uppercase();
    let found = all_rules.iter().find(|r| {
        r.name().eq_ignore_ascii_case(&rule_query_upper)
            || r.name().replace("MD", "") == rule_query_upper.replace("MD", "")
    });

    if let Some(rule) = found {
        let rule_name = rule.name();
        let rule_id = rule_name.to_lowercase();

        // Print basic info
        println!("{}", format!("{} - {}", rule_name, rule.description()).bold());
        println!();

        // Try to load detailed documentation from docs/
        let doc_path = format!("docs/{rule_id}.md");
        match fs::read_to_string(&doc_path) {
            Ok(doc_content) => {
                // Parse and display the documentation
                let lines: Vec<&str> = doc_content.lines().collect();
                let mut in_example = false;

                for line in lines.iter().skip(1) {
                    // Skip the title line
                    if line.starts_with("## ") {
                        println!("\n{}", line.trim_start_matches("## ").bold().underline());
                    } else if line.starts_with("### ") {
                        println!("\n{}", line.trim_start_matches("### ").bold());
                    } else if line.starts_with("```") {
                        println!("{}", line.dimmed());
                        in_example = !in_example;
                    } else if in_example {
                        if line.contains("<!-- Good -->") {
                            println!("{}", "Good:".green());
                        } else if line.contains("<!-- Bad -->") {
                            println!("{}", "Bad:".red());
                        } else {
                            println!("  {line}");
                        }
                    } else if !line.trim().is_empty() {
                        println!("{line}");
                    } else {
                        println!();
                    }
                }

                // Add a note about configuration
                if let Some((_, config_section)) = rule.default_config_section() {
                    println!("\n{}", "Default Configuration:".bold());
                    println!("{}", format!("[{rule_name}]").dimmed());
                    if let Ok(config_str) = toml::to_string_pretty(&config_section) {
                        for line in config_str.lines() {
                            println!("{}", line.dimmed());
                        }
                    }
                }
            }
            Err(_) => {
                // Fallback to basic information
                println!("Category: {:?}", rule.category());
                println!();
                println!("This rule helps maintain consistent Markdown formatting.");
                println!();
                println!("For more information, see the documentation at:");
                println!("  https://rumdl.dev/{rule_id}/");
            }
        }
    } else {
        eprintln!("{}: Rule '{}' not found.", "Error".red().bold(), rule_query);
        eprintln!("\nUse 'rumdl rule' to see all available rules.");
        exit::tool_error();
    }
}
