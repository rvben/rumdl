//! Output formatting and display utilities

use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::Rule;

/// Arguments for printing check results
pub struct PrintResultsArgs<'a> {
    pub args: &'a crate::CheckArgs,
    pub has_issues: bool,
    pub files_with_issues: usize,
    pub files_fixed: usize,
    pub total_issues: usize,
    pub total_issues_fixed: usize,
    pub total_fixable_issues: usize,
    pub total_files_processed: usize,
    pub duration_ms: u64,
}

/// Print summary of check/fix results
pub fn print_results_from_checkargs(params: PrintResultsArgs) {
    let PrintResultsArgs {
        args,
        has_issues,
        files_with_issues,
        files_fixed,
        total_issues,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
        duration_ms,
    } = params;
    // Choose singular or plural form of "file" based on count
    let file_text = if total_files_processed == 1 { "file" } else { "files" };
    let files_fixed_text = if files_fixed == 1 { "file" } else { "files" };

    // Show results summary
    // In fix mode, show "Fixed" message if we fixed any issues, even if all are now resolved
    let all_issues_fixed = total_issues > 0 && total_issues_fixed == total_issues;
    let should_show_fixed_message = args.fix_mode != crate::FixMode::Check && total_issues_fixed > 0;

    if has_issues {
        // If fix mode is enabled, only show the fixed summary
        if should_show_fixed_message {
            println!(
                "\n{} Fixed {}/{} issues in {} {} ({}ms)",
                "Fixed:".green().bold(),
                total_issues_fixed,
                total_issues,
                files_fixed,
                files_fixed_text,
                duration_ms
            );
        } else {
            // In non-fix mode, show issues summary with simplified count when appropriate
            let files_display = if files_with_issues == total_files_processed {
                // Just show the number if all files have issues
                format!("{files_with_issues}")
            } else {
                // Show the fraction if only some files have issues
                format!("{files_with_issues}/{total_files_processed}")
            };

            println!(
                "\n{} Found {} issues in {} {} ({}ms)",
                "Issues:".yellow(),
                total_issues,
                files_display,
                file_text,
                duration_ms
            );

            if args.fix_mode == crate::FixMode::Check && total_fixable_issues > 0 {
                // Display the exact count of fixable issues
                println!("Run `rumdl fmt` to automatically fix {total_fixable_issues} of the {total_issues} issues");
            }
        }
    } else if all_issues_fixed {
        // All issues were fixed - show success message with fix count
        // This matches markdownlint behavior: show that fixes were applied, exit 0
        println!(
            "\n{} Fixed {}/{} issues in {} {} ({}ms)",
            "Fixed:".green().bold(),
            total_issues_fixed,
            total_issues,
            files_fixed,
            files_fixed_text,
            duration_ms
        );
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

/// Format config source provenance for display
pub fn format_provenance(src: rumdl_config::ConfigSource) -> &'static str {
    match src {
        rumdl_config::ConfigSource::Cli => "CLI",
        rumdl_config::ConfigSource::UserConfig => "user config",
        rumdl_config::ConfigSource::ProjectConfig => "project config",
        rumdl_config::ConfigSource::PyprojectToml => "pyproject.toml",
        rumdl_config::ConfigSource::Default => "default",
    }
}

/// Print configuration with provenance information, excluding default values
pub fn print_config_with_provenance_no_defaults(sourced: &rumdl_config::SourcedConfig, _all_rules: &[Box<dyn Rule>]) {
    let g = &sourced.global;
    let mut all_lines = Vec::new();
    let mut has_global_section = false;

    // Build global section, filtering out defaults
    let mut global_lines = Vec::new();
    if g.enable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("enable = {:?}", g.enable.value),
            format!("[from {}]", format_provenance(g.enable.source)),
        ));
        has_global_section = true;
    }
    if g.disable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("disable = {:?}", g.disable.value),
            format!("[from {}]", format_provenance(g.disable.source)),
        ));
        has_global_section = true;
    }
    if g.exclude.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("exclude = {:?}", g.exclude.value),
            format!("[from {}]", format_provenance(g.exclude.source)),
        ));
        has_global_section = true;
    }
    if g.include.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("include = {:?}", g.include.value),
            format!("[from {}]", format_provenance(g.include.source)),
        ));
        has_global_section = true;
    }
    if g.respect_gitignore.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("respect_gitignore = {}", g.respect_gitignore.value),
            format!("[from {}]", format_provenance(g.respect_gitignore.source)),
        ));
        has_global_section = true;
    }
    if g.flavor.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("flavor = {:?}", g.flavor.value),
            format!("[from {}]", format_provenance(g.flavor.source)),
        ));
        has_global_section = true;
    }
    if g.line_length.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("line_length = {}", g.line_length.value.get()),
            format!("[from {}]", format_provenance(g.line_length.source)),
        ));
        has_global_section = true;
    }
    if g.force_exclude.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("force_exclude = {}", g.force_exclude.value),
            format!("[from {}]", format_provenance(g.force_exclude.source)),
        ));
        has_global_section = true;
    }
    if g.cache.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("cache = {}", g.cache.value),
            format!("[from {}]", format_provenance(g.cache.source)),
        ));
        has_global_section = true;
    }
    if let Some(ref output_format) = g.output_format
        && output_format.source != rumdl_config::ConfigSource::Default
    {
        global_lines.push((
            format!("output_format = {:?}", output_format.value),
            format!("[from {}]", format_provenance(output_format.source)),
        ));
        has_global_section = true;
    }
    if let Some(ref cache_dir) = g.cache_dir
        && cache_dir.source != rumdl_config::ConfigSource::Default
    {
        global_lines.push((
            format!("cache_dir = {:?}", cache_dir.value),
            format!("[from {}]", format_provenance(cache_dir.source)),
        ));
        has_global_section = true;
    }
    if g.fixable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("fixable = {:?}", g.fixable.value),
            format!("[from {}]", format_provenance(g.fixable.source)),
        ));
        has_global_section = true;
    }
    if g.unfixable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("unfixable = {:?}", g.unfixable.value),
            format!("[from {}]", format_provenance(g.unfixable.source)),
        ));
        has_global_section = true;
    }

    if has_global_section {
        all_lines.push(("[global]".to_string(), String::new()));
        all_lines.extend(global_lines);
        all_lines.push((String::new(), String::new()));
    }

    // Handle per-file ignores if non-default
    if sourced.per_file_ignores.source != rumdl_config::ConfigSource::Default
        && !sourced.per_file_ignores.value.is_empty()
    {
        all_lines.push(("[per-file-ignores]".to_string(), String::new()));
        for (pattern, rules) in &sourced.per_file_ignores.value {
            let rules_str = format!("{rules:?}");
            all_lines.push((
                format!("{pattern:?} = {rules_str}"),
                format!("[from {}]", format_provenance(sourced.per_file_ignores.source)),
            ));
        }
        all_lines.push((String::new(), String::new()));
    }

    // Handle rule configurations
    let mut rule_names: Vec<_> = sourced.rules.keys().cloned().collect();
    rule_names.sort();
    for rule_name in rule_names {
        let rule_cfg = &sourced.rules[&rule_name];
        let mut lines = Vec::new();
        let mut keys: Vec<_> = rule_cfg.values.keys().collect();
        keys.sort();
        for key in keys {
            let sv = &rule_cfg.values[key];
            // Only include non-default values
            if sv.source != rumdl_config::ConfigSource::Default {
                let value_str = match &sv.value {
                    toml::Value::Array(arr) => {
                        let vals: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                        format!("[{}]", vals.join(", "))
                    }
                    toml::Value::String(s) => format!("\"{s}\""),
                    toml::Value::Boolean(b) => b.to_string(),
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    _ => sv.value.to_string(),
                };
                lines.push((
                    format!("{key} = {value_str}"),
                    format!("[from {}]", format_provenance(sv.source)),
                ));
            }
        }
        if !lines.is_empty() {
            all_lines.push((format!("[{rule_name}]"), String::new()));
            all_lines.extend(lines);
            all_lines.push((String::new(), String::new()));
        }
    }

    // Print output
    if all_lines.is_empty() {
        // All configurations are using defaults
        println!("All configurations are using default values.");
        return;
    }

    let max_left = all_lines.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (left, right) in &all_lines {
        if left.is_empty() && right.is_empty() {
            println!();
        } else if !right.is_empty() {
            println!("{:<width$} {}", left, right.dimmed(), width = max_left);
        } else {
            println!("{left:<max_left$} {right}");
        }
    }
}

/// Print configuration with provenance information
pub fn print_config_with_provenance(sourced: &rumdl_config::SourcedConfig, all_rules: &[Box<dyn Rule>]) {
    let g = &sourced.global;
    let mut all_lines = Vec::new();
    // [global] section
    let global_lines = vec![
        ("[global]".to_string(), String::new()),
        (
            format!("enable = {:?}", g.enable.value),
            format!("[from {}]", format_provenance(g.enable.source)),
        ),
        (
            format!("disable = {:?}", g.disable.value),
            format!("[from {}]", format_provenance(g.disable.source)),
        ),
        (
            format!("exclude = {:?}", g.exclude.value),
            format!("[from {}]", format_provenance(g.exclude.source)),
        ),
        (
            format!("include = {:?}", g.include.value),
            format!("[from {}]", format_provenance(g.include.source)),
        ),
        (
            format!("respect_gitignore = {}", g.respect_gitignore.value),
            format!("[from {}]", format_provenance(g.respect_gitignore.source)),
        ),
    ];

    // Add flavor if it's set
    let mut global_lines = global_lines;
    global_lines.push((
        format!("flavor = {:?}", g.flavor.value),
        format!("[from {}]", format_provenance(g.flavor.source)),
    ));
    global_lines.push((String::new(), String::new()));
    all_lines.extend(global_lines);

    let mut rule_names: Vec<_> = all_rules.iter().map(|r| r.name().to_string()).collect();
    rule_names.sort();
    for rule_name in rule_names {
        let mut lines = Vec::new();
        let norm_rule_name = rule_name.to_ascii_uppercase(); // Use uppercase for lookup
        if let Some(rule_cfg) = sourced.rules.get(&norm_rule_name) {
            let mut keys: Vec<_> = rule_cfg.values.keys().collect();
            keys.sort();
            for key in keys {
                let sv = &rule_cfg.values[key];
                let value_str = match &sv.value {
                    toml::Value::Array(arr) => {
                        let vals: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                        format!("[{}]", vals.join(", "))
                    }
                    toml::Value::String(s) => format!("\"{s}\""),
                    toml::Value::Boolean(b) => b.to_string(),
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    _ => sv.value.to_string(),
                };
                lines.push((
                    format!("{key} = {value_str}"),
                    format!("[from {}]", format_provenance(sv.source)),
                ));
            }
        } else {
            // Print default config for this rule, if available
            if let Some((_, toml::Value::Table(table))) = all_rules
                .iter()
                .find(|r| r.name() == rule_name)
                .and_then(|r| r.default_config_section())
            {
                let mut keys: Vec<_> = table.keys().collect();
                keys.sort();
                for key in keys {
                    let v = &table[key];
                    let value_str = match v {
                        toml::Value::Array(arr) => {
                            let vals: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                            format!("[{}]", vals.join(", "))
                        }
                        toml::Value::String(s) => format!("\"{s}\""),
                        toml::Value::Boolean(b) => b.to_string(),
                        toml::Value::Integer(i) => i.to_string(),
                        toml::Value::Float(f) => f.to_string(),
                        _ => v.to_string(),
                    };
                    lines.push((
                        format!("{key} = {value_str}"),
                        format!("[from {}]", format_provenance(rumdl_config::ConfigSource::Default)),
                    ));
                }
            }
        }
        if !lines.is_empty() {
            all_lines.push((format!("[{rule_name}]"), String::new()));
            all_lines.extend(lines);
            all_lines.push((String::new(), String::new()));
        }
    }
    let max_left = all_lines.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (left, right) in &all_lines {
        if left.is_empty() && right.is_empty() {
            println!();
        } else if !right.is_empty() {
            println!("{:<width$} {}", left, right.dimmed(), width = max_left);
        } else {
            println!("{left:<max_left$} {right}");
        }
    }
}

/// Format a TOML value for display
pub fn format_toml_value(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => format!("\"{s}\""),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            let vals: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", vals.join(", "))
        }
        toml::Value::Table(_) => "<table>".to_string(),
        toml::Value::Datetime(dt) => dt.to_string(),
    }
}

/// Print statistics about lint warnings by rule
pub fn print_statistics(warnings: &[rumdl_lib::rule::LintWarning]) {
    use std::collections::HashMap;

    // Group warnings by rule name
    let mut rule_counts: HashMap<&str, usize> = HashMap::new();
    let mut fixable_counts: HashMap<&str, usize> = HashMap::new();

    for warning in warnings {
        let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
        *rule_counts.entry(rule_name).or_insert(0) += 1;

        if warning.fix.is_some() {
            *fixable_counts.entry(rule_name).or_insert(0) += 1;
        }
    }

    // Sort rules by count (descending)
    let mut sorted_rules: Vec<_> = rule_counts.iter().collect();
    sorted_rules.sort_by(|a, b| b.1.cmp(a.1));

    println!("\n{}", "Rule Violation Statistics:".bold().underline());
    println!("{:<8} {:<12} {:<8} Percentage", "Rule", "Violations", "Fixable");
    println!("{}", "-".repeat(50));

    let total_warnings = warnings.len();
    for (rule, count) in sorted_rules {
        let fixable = fixable_counts.get(rule).unwrap_or(&0);
        let percentage = (*count as f64 / total_warnings as f64) * 100.0;

        println!(
            "{:<8} {:<12} {:<8} {:>6.1}%",
            rule,
            count,
            if *fixable > 0 {
                format!("{fixable}")
            } else {
                "-".to_string()
            },
            percentage
        );
    }

    println!("{}", "-".repeat(50));
    println!(
        "{:<8} {:<12} {:<8} {:>6.1}%",
        "Total",
        total_warnings,
        fixable_counts.values().sum::<usize>(),
        100.0
    );
}

/// Generate a unified diff between original and modified content
pub fn generate_diff(original: &str, modified: &str, file_path: &str) -> String {
    let mut diff = String::new();

    // Create diff header
    diff.push_str(&format!("--- {file_path}\n"));
    diff.push_str(&format!("+++ {file_path} (fixed)\n"));

    let original_lines: Vec<&str> = original.lines().collect();
    let modified_lines: Vec<&str> = modified.lines().collect();

    // Simple line-by-line diff (could be improved with a proper diff algorithm)
    let max_lines = original_lines.len().max(modified_lines.len());
    let mut in_diff_block = false;
    let mut diff_start = 0;
    let mut changes = Vec::new();

    for i in 0..max_lines {
        let orig_line = original_lines.get(i).copied().unwrap_or("");
        let mod_line = modified_lines.get(i).copied().unwrap_or("");

        if orig_line != mod_line {
            if !in_diff_block {
                in_diff_block = true;
                diff_start = i.saturating_sub(3); // Include 3 lines of context before
            }
        } else if in_diff_block {
            // End of diff block, include 3 lines of context after
            let diff_end = (i + 3).min(max_lines);
            changes.push((diff_start, diff_end));
            in_diff_block = false;
        }
    }

    // Handle case where diff extends to the end of file
    if in_diff_block {
        changes.push((diff_start, max_lines));
    }

    // Generate unified diff format for each change block
    if changes.is_empty() {
        diff.push_str("No changes\n");
    } else {
        for (start, end) in changes {
            diff.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                start + 1,
                end - start,
                start + 1,
                end - start
            ));

            for i in start..end {
                let orig_line = original_lines.get(i).copied().unwrap_or("");
                let mod_line = modified_lines.get(i).copied().unwrap_or("");

                if i >= original_lines.len() {
                    // Line only in modified
                    diff.push_str(&format!("+{mod_line}\n"));
                } else if i >= modified_lines.len() {
                    // Line only in original
                    diff.push_str(&format!("-{orig_line}\n"));
                } else if orig_line == mod_line {
                    // Context line
                    diff.push_str(&format!(" {orig_line}\n"));
                } else {
                    // Changed line
                    diff.push_str(&format!("-{orig_line}\n"));
                    diff.push_str(&format!("+{mod_line}\n"));
                }
            }
        }
    }

    diff
}
