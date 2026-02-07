//! Handler for the `explain` command.

use colored::*;
use std::fs;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

/// Handle the explain command: show detailed rule documentation.
pub fn handle_explain(rule_query: &str) {
    let default_config = rumdl_config::Config::default();
    let all_rules = rumdl_lib::rules::all_rules(&default_config);

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
