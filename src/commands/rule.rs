//! Handler for the `rule` command.

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::{FixCapability, Rule, RuleCategory};

/// Rule metadata for JSON export (matches Ruff's output format)
#[derive(serde::Serialize)]
struct RuleInfo {
    /// Rule code (e.g., "MD001")
    code: String,
    /// Rule name in kebab-case (e.g., "heading-increment")
    name: String,
    /// All aliases for this rule
    aliases: Vec<String>,
    /// Short description of what the rule checks
    summary: String,
    /// Rule category (e.g., "heading", "list", "whitespace")
    category: String,
    /// Human-readable fix availability description
    fix: String,
    /// Fix availability: "Always", "Sometimes", or "None"
    fix_availability: String,
    /// URL to the rule documentation
    url: String,
    /// Full explanation/documentation for the rule (from docs/*.md)
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<String>,
}

/// Handle the rule command: show info about a rule or list all rules.
pub fn handle_rule(
    rule: Option<String>,
    output_format: String,
    fixable: bool,
    category: Option<String>,
    explain: bool,
    list_categories: bool,
) {
    // Use the canonical all_rules function to avoid drift between CLI and library
    let default_config = rumdl_config::Config::default();
    let all_rules = rumdl_lib::rules::all_rules(&default_config);

    // Collect all unique categories
    let mut categories: Vec<String> = all_rules
        .iter()
        .map(|r| category_to_string(r.category()).to_string())
        .collect();
    categories.sort();
    categories.dedup();

    // Handle --list-categories
    if list_categories {
        println!("Available categories:");
        for cat in &categories {
            let count = all_rules
                .iter()
                .filter(|r| category_to_string(r.category()) == cat)
                .count();
            println!("  {cat} ({count} rules)");
        }
        return;
    }

    // Validate category if provided
    if let Some(ref cat_filter) = category {
        let cat_filter_lower = cat_filter.to_lowercase();
        if !categories.iter().any(|c| c.to_lowercase() == cat_filter_lower) {
            eprintln!("Invalid category: '{cat_filter}'");
            eprintln!("Valid categories: {}", categories.join(", "));
            exit::tool_error();
        }
    }

    let aliases_map = build_rule_aliases_map();

    // Helper to build RuleInfo from a rule
    let build_rule_info = |r: &dyn Rule, include_explanation: bool| -> RuleInfo {
        let code = r.name().to_string();
        let all_aliases = aliases_map.get(&code).cloned().unwrap_or_default();
        let (primary_name, remaining_aliases) = get_primary_and_remaining_aliases(&code, &all_aliases);
        let (fix_desc, fix_avail) = fix_capability_to_strings(r.fix_capability());
        let explanation = if include_explanation {
            read_rule_explanation(&code)
        } else {
            None
        };
        RuleInfo {
            name: primary_name,
            aliases: remaining_aliases,
            code: code.clone(),
            summary: r.description().to_string(),
            category: category_to_string(r.category()).to_string(),
            fix: fix_desc.to_string(),
            fix_availability: fix_avail.to_string(),
            url: format!("https://rumdl.dev/{}/", code.to_lowercase()),
            explanation,
        }
    };

    // Build RuleInfo for all or a specific rule
    let mut rule_infos: Vec<RuleInfo> = if let Some(rule_query) = &rule {
        let rule_query_upper = rule_query.to_ascii_uppercase();
        let found = all_rules.iter().find(|r| {
            r.name().eq_ignore_ascii_case(&rule_query_upper)
                || r.name().replace("MD", "") == rule_query_upper.replace("MD", "")
        });
        if let Some(r) = found {
            vec![build_rule_info(r.as_ref(), explain)]
        } else {
            eprintln!("Rule '{rule_query}' not found.");
            exit::tool_error();
        }
    } else {
        all_rules.iter().map(|r| build_rule_info(r.as_ref(), explain)).collect()
    };

    // Apply filters
    if fixable {
        rule_infos.retain(|info| info.fix_availability != "None");
    }
    if let Some(ref cat_filter) = category {
        let cat_filter_lower = cat_filter.to_lowercase();
        rule_infos.retain(|info| info.category.to_lowercase() == cat_filter_lower);
    }

    // Check if no rules match filters
    if rule_infos.is_empty() && rule.is_none() {
        let mut filter_desc = Vec::new();
        if fixable {
            filter_desc.push("fixable".to_string());
        }
        if let Some(ref cat) = category {
            filter_desc.push(format!("category={cat}"));
        }
        eprintln!("No rules match the specified filters: {}", filter_desc.join(", "));
        eprintln!("Try: rumdl rule --list-categories");
        exit::tool_error();
    }

    // Output based on format
    match output_format.to_lowercase().as_str() {
        "json" => {
            // For single rule query, output the object directly; for all rules, output array
            let json = if rule.is_some() && rule_infos.len() == 1 {
                serde_json::to_string_pretty(&rule_infos[0])
            } else {
                serde_json::to_string_pretty(&rule_infos)
            };
            match json {
                Ok(output) => println!("{output}"),
                Err(e) => {
                    eprintln!("Error serializing to JSON: {e}");
                    exit::tool_error();
                }
            }
        }
        "json-lines" | "jsonl" => {
            // Output one JSON object per line (newline-delimited JSON)
            for info in &rule_infos {
                match serde_json::to_string(info) {
                    Ok(line) => println!("{line}"),
                    Err(e) => {
                        eprintln!("Error serializing to JSON: {e}");
                        exit::tool_error();
                    }
                }
            }
        }
        _ => {
            if rule.is_some() {
                if let Some(info) = rule_infos.first() {
                    println!("{} - {}", info.code, info.summary);
                    println!();
                    println!("Name: {}", info.name);
                    if !info.aliases.is_empty() {
                        println!("Aliases: {}", info.aliases.join(", "));
                    }
                    println!("Category: {}", info.category);
                    println!("Fix: {}", info.fix);
                    println!("Documentation: {}", info.url);
                    if let Some(ref explanation) = info.explanation {
                        println!();
                        println!("{explanation}");
                    }
                }
            } else {
                // Show summary with optional filter info
                let filter_info = if fixable || category.is_some() {
                    let mut parts = Vec::new();
                    if fixable {
                        parts.push("fixable".to_string());
                    }
                    if let Some(ref cat) = category {
                        parts.push(format!("category={cat}"));
                    }
                    format!(" ({})", parts.join(", "))
                } else {
                    String::new()
                };
                println!("Available rules{filter_info}:");
                for info in &rule_infos {
                    println!("  {} - {}", info.code, info.summary);
                }
                println!();
                println!("Total: {} rules", rule_infos.len());
            }
        }
    }
}

/// Read rule documentation from the docs directory
fn read_rule_explanation(code: &str) -> Option<String> {
    // Try to find the docs file in common locations
    let code_lower = code.to_lowercase();
    let possible_paths = [format!("docs/{code_lower}.md"), format!("../docs/{code_lower}.md")];

    for path in &possible_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
    None
}

/// Build a map from canonical rule IDs to their aliases
fn build_rule_aliases_map() -> std::collections::HashMap<String, Vec<String>> {
    use rumdl_config::RULE_ALIAS_MAP;

    let mut aliases_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for (alias, canonical) in RULE_ALIAS_MAP.entries() {
        // Skip identity mappings (where alias == canonical)
        if *alias == *canonical {
            continue;
        }
        // Convert alias to kebab-case lowercase
        let alias_kebab = alias.to_lowercase();
        aliases_map.entry(canonical.to_string()).or_default().push(alias_kebab);
    }

    // Sort aliases for consistent output
    for aliases in aliases_map.values_mut() {
        aliases.sort();
    }

    aliases_map
}

/// Convert RuleCategory to a string for JSON output
fn category_to_string(category: RuleCategory) -> &'static str {
    match category {
        RuleCategory::Heading => "heading",
        RuleCategory::List => "list",
        RuleCategory::CodeBlock => "code-block",
        RuleCategory::Link => "link",
        RuleCategory::Image => "image",
        RuleCategory::Html => "html",
        RuleCategory::Emphasis => "emphasis",
        RuleCategory::Whitespace => "whitespace",
        RuleCategory::Blockquote => "blockquote",
        RuleCategory::Table => "table",
        RuleCategory::FrontMatter => "front-matter",
        RuleCategory::Other => "other",
    }
}

/// Convert FixCapability to human-readable strings
fn fix_capability_to_strings(capability: FixCapability) -> (&'static str, &'static str) {
    match capability {
        FixCapability::FullyFixable => ("Fix is always available.", "Always"),
        FixCapability::ConditionallyFixable => ("Fix is sometimes available.", "Sometimes"),
        FixCapability::Unfixable => ("Fix is not available.", "None"),
    }
}

/// Get the primary alias (kebab-case name) for a rule, and remaining aliases
fn get_primary_and_remaining_aliases(code: &str, aliases: &[String]) -> (String, Vec<String>) {
    if aliases.is_empty() {
        (code.to_lowercase(), Vec::new())
    } else {
        let primary = aliases[0].clone();
        let remaining: Vec<String> = aliases.iter().skip(1).cloned().collect();
        (primary, remaining)
    }
}
