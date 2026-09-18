//! Output formatting and display utilities

use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013Config;

/// Arguments for printing check results
pub struct PrintResultsArgs<'a> {
    pub args: &'a crate::CheckArgs,
    pub has_issues: bool,
    pub files_with_issues: usize,
    pub files_fixed: usize,
    pub total_issues: usize,
    pub summary_issues_fixed: usize,
    pub total_fixable_issues: usize,
    pub total_files_processed: usize,
    pub duration_ms: u64,
    /// The run was incomplete: a file could not be read, or a code-block tool
    /// could not run under a setting of `fail`. Details are already printed to
    /// stderr. Suppresses the misleading "No issues found" success summary.
    pub had_tool_error: bool,
}

/// `singular` or `plural`, whichever agrees with `count`.
pub fn noun(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
}

/// `count` issues, as a summary states them: "1 issue", "3 issues".
pub fn issues(count: usize) -> String {
    format!("{count} {}", noun(count, "issue", "issues"))
}

/// Print summary of check/fix results
pub fn print_results_from_checkargs(params: PrintResultsArgs) {
    let PrintResultsArgs {
        args,
        has_issues,
        files_with_issues,
        files_fixed,
        total_issues,
        summary_issues_fixed,
        total_fixable_issues,
        total_files_processed,
        duration_ms,
        had_tool_error,
    } = params;
    let file_text = noun(total_files_processed, "file", "files");
    let files_fixed_text = noun(files_fixed, "file", "files");
    // A fraction reads as "N of the total", so the noun agrees with the total.
    let issue_text = noun(total_issues, "issue", "issues");
    let dry_run = args.diff || args.check;
    let change_label = if dry_run {
        "Would fix:".yellow().bold()
    } else {
        "Fixed:".green().bold()
    };

    // Show results summary
    // In fix/format mode, show a change summary whenever we changed files or would change them in dry-run mode.
    let should_show_change_message = args.fix_mode != crate::FixMode::Check && files_fixed > 0;

    if files_fixed > 0 && total_issues == 0 {
        // A format-only code-block tool can change files without emitting any
        // lint findings. Describe the file changes instead of a 0/0 fraction.
        let label = if dry_run {
            "Would format:".yellow().bold()
        } else {
            "Formatted:".green().bold()
        };
        println!("\n{label} {files_fixed} {files_fixed_text} ({duration_ms}ms)");
    } else if should_show_change_message {
        println!(
            "\n{change_label} {summary_issues_fixed}/{total_issues} {issue_text} in {files_fixed} {files_fixed_text} ({duration_ms}ms)"
        );
    } else if has_issues {
        // In non-fix mode, show issues summary with simplified count when appropriate
        let files_display = if files_with_issues == total_files_processed {
            // Just show the number if all files have issues
            format!("{files_with_issues}")
        } else {
            // Show the fraction if only some files have issues
            format!("{files_with_issues}/{total_files_processed}")
        };

        println!(
            "\n{} Found {} {} in {} {} ({}ms)",
            "Issues:".yellow(),
            total_issues,
            issue_text,
            files_display,
            file_text,
            duration_ms
        );

        if args.fix_mode == crate::FixMode::Check && total_fixable_issues > 0 {
            let fixable = if total_fixable_issues < total_issues {
                format!("{total_fixable_issues} of the {total_issues} issues")
            } else if total_issues == 1 {
                "it".to_string()
            } else {
                format!("all {total_issues} issues")
            };
            println!("Run `rumdl fmt` to automatically fix {fixable}");
        }
    } else if !had_tool_error {
        println!(
            "\n{} No issues found in {} {} ({}ms)",
            "Success:".green().bold(),
            total_files_processed,
            file_text,
            duration_ms
        );
    }

    // Part of the run did not happen: a file that could not be read, or a
    // code-block tool that could not run. Either way the counts above describe
    // less than the whole set, so say so rather than let them read as complete.
    // The individual causes were already printed to stderr.
    if had_tool_error {
        println!(
            "\n{} the run was incomplete, see the errors above ({}ms)",
            "Error:".red().bold(),
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
        rumdl_config::ConfigSource::EditorConfig => ".editorconfig",
        rumdl_config::ConfigSource::Default => "default",
    }
}

/// Format the `[from ...]` provenance label for a config value.
///
/// File-based sources show the originating file (relativized to the project
/// root), so extends chains attribute each value to the file that actually
/// set it rather than a generic "project config". Defaults and CLI flags
/// have no file and fall back to the source-kind name.
pub fn provenance_label<T>(sv: &rumdl_config::SourcedValue<T>, project_root: Option<&std::path::Path>) -> String {
    match &sv.origin {
        Some(file) => format!("[from {}]", origin_display(file, project_root)),
        None => format!("[from {}]", format_provenance(sv.source)),
    }
}

/// Render a config-file origin path for display: canonicalized (extends
/// resolution can embed `../` segments), relativized to the project root
/// when inside it, and shown as a short `../`-style path for nearby
/// out-of-tree files (the common shape for shared extends bases). Distant
/// files fall back to the canonical absolute path.
fn origin_display(file: &str, project_root: Option<&std::path::Path>) -> String {
    use std::path::{Component, Path, PathBuf};

    fn normalize(path: &Path) -> String {
        let s = path.to_string_lossy();
        if cfg!(windows) {
            s.replace('\\', "/")
        } else {
            s.to_string()
        }
    }

    let canonical = Path::new(file).canonicalize().unwrap_or_else(|_| PathBuf::from(file));

    // Relativize against the project root, falling back to the current
    // directory when no project root is known (e.g. markdownlint-only
    // discovery).
    let base = project_root
        .and_then(|root| root.canonicalize().ok())
        .or_else(|| std::env::current_dir().ok().and_then(|cwd| cwd.canonicalize().ok()));

    if let Some(base) = base {
        if let Ok(rel) = canonical.strip_prefix(&base) {
            return normalize(rel);
        }
        // Out-of-tree file: build a ../-relative path from the base.
        let path_comps: Vec<Component> = canonical.components().collect();
        let base_comps: Vec<Component> = base.components().collect();
        let common = path_comps.iter().zip(&base_comps).take_while(|(a, b)| a == b).count();
        let ups = base_comps.len() - common;
        if common > 0 && ups <= 3 {
            let mut rel = PathBuf::new();
            for _ in 0..ups {
                rel.push("..");
            }
            for comp in &path_comps[common..] {
                rel.push(comp);
            }
            return normalize(&rel);
        }
    }
    normalize(&canonical)
}

/// What a section with no entries shows under its header.
///
/// A bare header with nothing under it reads as output that got cut off rather
/// than as "nothing is configured here", and these printers exist to state the
/// effective configuration rather than leave it inferred. An empty list already
/// renders as `enable = []` for the same reason; a TOML table has no equivalent
/// spelling, so it is said in a comment.
const EMPTY_SECTION: &str = "# (none)";

/// Render the `[per-file-ignores]` section as `pattern = [rules]` lines.
fn per_file_ignores_lines(
    sourced: &rumdl_config::SourcedConfig,
    root: Option<&std::path::Path>,
) -> Vec<(String, String)> {
    let label = provenance_label(&sourced.per_file_ignores, root);
    let mut lines = vec![("[per-file-ignores]".to_string(), String::new())];
    for (pattern, rules) in &sourced.per_file_ignores.value {
        lines.push((format!("{pattern:?} = {rules:?}"), label.clone()));
    }
    if sourced.per_file_ignores.value.is_empty() {
        lines.push((EMPTY_SECTION.to_string(), label));
    }
    lines
}

/// Render the `[per-file-flavor]` section as `pattern = "flavor"` lines.
fn per_file_flavor_lines(
    sourced: &rumdl_config::SourcedConfig,
    root: Option<&std::path::Path>,
) -> Vec<(String, String)> {
    let label = provenance_label(&sourced.per_file_flavor, root);
    let mut lines = vec![("[per-file-flavor]".to_string(), String::new())];
    for (pattern, flavor) in &sourced.per_file_flavor.value {
        lines.push((format!("{pattern:?} = \"{flavor}\""), label.clone()));
    }
    if sourced.per_file_flavor.value.is_empty() {
        lines.push((EMPTY_SECTION.to_string(), label));
    }
    lines
}

/// Render the `[code-block-tools]` section.
///
/// The section nests (`[code-block-tools.languages.python]`), which the flat
/// `key = value` shape used everywhere else cannot represent, so it is rendered
/// as the TOML it would be written as and the provenance label goes on the value
/// lines. The whole section merges as a single value, so one label describes all
/// of them.
///
/// Values are shown as written even when they came from a file whose text is not
/// quoted back in warnings: this output answers a question the user asked about
/// their own configuration, the same reason rule option values are shown here in
/// full.
fn code_block_tools_lines(
    sourced: &rumdl_config::SourcedConfig,
    root: Option<&std::path::Path>,
) -> Vec<(String, String)> {
    let label = provenance_label(&sourced.code_block_tools, root);
    let mut document = toml::map::Map::new();
    let value = match toml::Value::try_from(&sourced.code_block_tools.value) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    document.insert("code-block-tools".to_string(), value);
    // `to_string`, not `to_string_pretty`: pretty rendering breaks an array over
    // several lines, and each of those lines then gets the provenance label
    // repeated beside it, closing bracket included. Every other section here
    // prints an array inline, so this one does too.
    let rendered = match toml::to_string(&toml::Value::Table(document)) {
        Ok(rendered) => rendered,
        Err(_) => return Vec::new(),
    };
    let rows: Vec<(String, String)> = rendered
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            if line.starts_with('[') {
                (line.to_string(), String::new())
            } else {
                (line.to_string(), label.clone())
            }
        })
        .collect();

    // `[code-block-tools.languages]` and its siblings are tables, so a section
    // holding no entries renders as a header with nothing under it.
    let mut lines = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        lines.push(row.clone());
        if row.0.starts_with('[') && rows.get(index + 1).is_none_or(|next| next.0.starts_with('[')) {
            lines.push((EMPTY_SECTION.to_string(), label.clone()));
        }
    }
    lines
}

/// Print configuration with provenance information, excluding default values
pub fn print_config_with_provenance_no_defaults(sourced: &rumdl_config::SourcedConfig, _all_rules: &[Box<dyn Rule>]) {
    let g = &sourced.global;
    let root = sourced.project_root.as_deref();
    let mut all_lines = Vec::new();
    let mut has_global_section = false;

    // Build global section, filtering out defaults
    let mut global_lines = Vec::new();
    if g.enable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("enable = {:?}", g.enable.value),
            provenance_label(&g.enable, root),
        ));
        has_global_section = true;
    }
    if g.disable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("disable = {:?}", g.disable.value),
            provenance_label(&g.disable, root),
        ));
        has_global_section = true;
    }
    if g.exclude.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("exclude = {:?}", g.exclude.value),
            provenance_label(&g.exclude, root),
        ));
        has_global_section = true;
    }
    if g.include.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("include = {:?}", g.include.value),
            provenance_label(&g.include, root),
        ));
        has_global_section = true;
    }
    if g.respect_gitignore.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("respect_gitignore = {}", g.respect_gitignore.value),
            provenance_label(&g.respect_gitignore, root),
        ));
        has_global_section = true;
    }
    if g.flavor.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("flavor = \"{}\"", g.flavor.value),
            provenance_label(&g.flavor, root),
        ));
        has_global_section = true;
    }
    if g.line_length.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("line_length = {}", g.line_length.value.get()),
            provenance_label(&g.line_length, root),
        ));
        has_global_section = true;
    }
    if g.force_exclude.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("force_exclude = {}", g.force_exclude.value),
            provenance_label(&g.force_exclude, root),
        ));
        has_global_section = true;
    }
    if g.cache.source != rumdl_config::ConfigSource::Default {
        global_lines.push((format!("cache = {}", g.cache.value), provenance_label(&g.cache, root)));
        has_global_section = true;
    }
    if g.editorconfig.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("editorconfig = {}", g.editorconfig.value),
            provenance_label(&g.editorconfig, root),
        ));
        has_global_section = true;
    }
    if let Some(ref output_format) = g.output_format
        && output_format.source != rumdl_config::ConfigSource::Default
    {
        global_lines.push((
            format!("output_format = {:?}", output_format.value),
            provenance_label(output_format, root),
        ));
        has_global_section = true;
    }
    if let Some(ref cache_dir) = g.cache_dir
        && cache_dir.source != rumdl_config::ConfigSource::Default
    {
        global_lines.push((
            format!("cache_dir = {:?}", cache_dir.value),
            provenance_label(cache_dir, root),
        ));
        has_global_section = true;
    }
    if g.fixable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("fixable = {:?}", g.fixable.value),
            provenance_label(&g.fixable, root),
        ));
        has_global_section = true;
    }
    if g.unfixable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("unfixable = {:?}", g.unfixable.value),
            provenance_label(&g.unfixable, root),
        ));
        has_global_section = true;
    }
    if g.extend_enable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("extend_enable = {:?}", g.extend_enable.value),
            provenance_label(&g.extend_enable, root),
        ));
        has_global_section = true;
    }
    if g.extend_disable.source != rumdl_config::ConfigSource::Default {
        global_lines.push((
            format!("extend_disable = {:?}", g.extend_disable.value),
            provenance_label(&g.extend_disable, root),
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
        all_lines.extend(per_file_ignores_lines(sourced, root));
        all_lines.push((String::new(), String::new()));
    }

    // Handle per-file flavors if non-default
    if sourced.per_file_flavor.source != rumdl_config::ConfigSource::Default
        && !sourced.per_file_flavor.value.is_empty()
    {
        all_lines.extend(per_file_flavor_lines(sourced, root));
        all_lines.push((String::new(), String::new()));
    }

    // Handle code block tools if non-default
    if sourced.code_block_tools.source != rumdl_config::ConfigSource::Default {
        all_lines.extend(code_block_tools_lines(sourced, root));
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
                lines.push((format!("{key} = {value_str}"), provenance_label(sv, root)));
            }
        }
        // MD013's effective limit differs from its default exactly when the
        // global setting is not itself the default, which is the condition this
        // listing of non-default values is selecting on.
        if sourced.global.line_length.source != rumdl_config::ConfigSource::Default {
            apply_inherited_md013_line_length(&rule_name, &mut lines, sourced, root);
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

/// The `[MD013]` `line-length` line when the `[global]` setting supplies it.
///
/// MD013 measures against `[global] line-length` unless it sets its own, so the
/// value printed under `[MD013]` is neither the option's default nor necessarily
/// what the rule's own section says. The number comes from the same
/// [`MD013Config::from_document_config`] the rule is built with, and the
/// provenance from the global setting it was taken from.
fn inherited_md013_line_length(
    sourced: &rumdl_config::SourcedConfig,
    root: Option<&std::path::Path>,
) -> Option<(String, String)> {
    let config: rumdl_config::Config = sourced.clone().into_validated_unchecked().into();
    if !rumdl_lib::rule_config_serde::load_rule_config::<MD013Config>(&config).line_length_is_default() {
        return None;
    }
    let resolved = MD013Config::from_document_config(&config).line_length;
    Some((
        format!("line-length = {}", resolved.get()),
        provenance_label(&sourced.global.line_length, root),
    ))
}

/// Replace a rule's `line-length` line with the value inherited from `[global]`.
///
/// A no-op for every rule but MD013, and for an MD013 that sets its own limit.
fn apply_inherited_md013_line_length(
    rule_name: &str,
    lines: &mut Vec<(String, String)>,
    sourced: &rumdl_config::SourcedConfig,
    root: Option<&std::path::Path>,
) {
    if rule_name != "MD013" {
        return;
    }
    let Some(inherited) = inherited_md013_line_length(sourced, root) else {
        return;
    };
    // Both spellings reach here: a section prints the key as the user wrote it.
    lines.retain(|(text, _)| !(text.starts_with("line-length =") || text.starts_with("line_length =")));
    lines.push(inherited);
    lines.sort_by(|a, b| a.0.cmp(&b.0));
}

/// Print configuration with provenance information
pub fn print_config_with_provenance(sourced: &rumdl_config::SourcedConfig, all_rules: &[Box<dyn Rule>]) {
    let g = &sourced.global;
    let root = sourced.project_root.as_deref();
    let mut all_lines = Vec::new();
    // [global] section
    let global_lines = vec![
        ("[global]".to_string(), String::new()),
        (
            format!("enable = {:?}", g.enable.value),
            provenance_label(&g.enable, root),
        ),
        (
            format!("disable = {:?}", g.disable.value),
            provenance_label(&g.disable, root),
        ),
        (
            format!("exclude = {:?}", g.exclude.value),
            provenance_label(&g.exclude, root),
        ),
        (
            format!("include = {:?}", g.include.value),
            provenance_label(&g.include, root),
        ),
        (
            format!("respect_gitignore = {}", g.respect_gitignore.value),
            provenance_label(&g.respect_gitignore, root),
        ),
        (
            format!("editorconfig = {}", g.editorconfig.value),
            provenance_label(&g.editorconfig, root),
        ),
    ];

    // Add flavor if it's set
    let mut global_lines = global_lines;
    global_lines.push((
        format!("flavor = \"{}\"", g.flavor.value),
        format!("[from {}]", format_provenance(g.flavor.source)),
    ));
    global_lines.push((
        format!("line_length = {}", g.line_length.value.get()),
        provenance_label(&g.line_length, root),
    ));
    global_lines.push((
        format!("force_exclude = {}", g.force_exclude.value),
        provenance_label(&g.force_exclude, root),
    ));
    global_lines.push((format!("cache = {}", g.cache.value), provenance_label(&g.cache, root)));
    global_lines.push((
        format!("fixable = {:?}", g.fixable.value),
        provenance_label(&g.fixable, root),
    ));
    global_lines.push((
        format!("unfixable = {:?}", g.unfixable.value),
        provenance_label(&g.unfixable, root),
    ));
    global_lines.push((
        format!("extend_enable = {:?}", g.extend_enable.value),
        provenance_label(&g.extend_enable, root),
    ));
    global_lines.push((
        format!("extend_disable = {:?}", g.extend_disable.value),
        provenance_label(&g.extend_disable, root),
    ));
    // `output_format` and `cache_dir` have no default to stand in for an unset
    // value, so they are shown only once something has set them.
    if let Some(ref output_format) = g.output_format {
        global_lines.push((
            format!("output_format = {:?}", output_format.value),
            provenance_label(output_format, root),
        ));
    }
    if let Some(ref cache_dir) = g.cache_dir {
        global_lines.push((
            format!("cache_dir = {:?}", cache_dir.value),
            provenance_label(cache_dir, root),
        ));
    }
    global_lines.push((String::new(), String::new()));
    all_lines.extend(global_lines);

    // The remaining sections are always shown, defaults included: this output is
    // the whole effective configuration, and a section left out reads as one that
    // does not exist.
    all_lines.extend(per_file_ignores_lines(sourced, root));
    all_lines.push((String::new(), String::new()));
    all_lines.extend(per_file_flavor_lines(sourced, root));
    all_lines.push((String::new(), String::new()));
    all_lines.extend(code_block_tools_lines(sourced, root));
    all_lines.push((String::new(), String::new()));

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
                lines.push((format!("{key} = {value_str}"), provenance_label(sv, root)));
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
        apply_inherited_md013_line_length(&rule_name, &mut lines, sourced, root);
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

/// Unified diff from `original` to `modified`, in the form `diff -u` writes and
/// `patch -p0` or `git apply -p0` read: `file_path` names both sides, each hunk
/// carries three lines of context, and a side that does not end in a newline is
/// marked `\ No newline at end of file`. Empty when the two are identical.
///
/// A line ends only at `\n`, where `patch` and `git apply` end it, so a lone
/// `\r` stays inside its line and a `\r\n` ending is kept whole.
pub fn generate_diff(original: &str, modified: &str, file_path: &str) -> String {
    use std::fmt::Write as _;

    let old: Vec<&str> = original.split_inclusive('\n').collect();
    let new: Vec<&str> = modified.split_inclusive('\n').collect();
    let diff = similar::TextDiff::configure().diff_slices(&old, &new);
    let mut patch = String::new();
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        if patch.is_empty() {
            let _ = write!(patch, "--- {file_path}\n+++ {file_path}\n");
        }
        let _ = writeln!(patch, "{}", hunk.header());
        for change in hunk.iter_changes() {
            let line = change.value();
            let _ = write!(patch, "{}{line}", change.tag());
            // The marker is decided here rather than by `similar`, which also
            // counts a trailing `\r` as a line ending.
            if !line.ends_with('\n') {
                patch.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines `1\n` through `n\n`.
    fn numbered(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("{i}\n")).collect()
    }

    fn hunk_headers(diff: &str) -> Vec<&str> {
        diff.lines().filter(|line| line.starts_with("@@")).collect()
    }

    #[test]
    fn test_generate_diff_is_empty_for_identical_content() {
        let content = "# Title\n\ntext\n";
        assert_eq!(generate_diff(content, content, "doc.md"), "");
    }

    #[test]
    fn test_generate_diff_modified_line() {
        assert_eq!(
            generate_diff("a\nb\nc\n", "a\nB\nc\n", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
        );
    }

    #[test]
    fn test_generate_diff_inserted_line_leaves_later_lines_as_context() {
        let original = numbered(8);
        let mut modified = original.clone();
        modified.insert(4, "new\n".to_string());
        assert_eq!(
            generate_diff(&original.concat(), &modified.concat(), "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -2,6 +2,7 @@\n 2\n 3\n 4\n+new\n 5\n 6\n 7\n"
        );
    }

    #[test]
    fn test_generate_diff_deleted_line_leaves_later_lines_as_context() {
        let original = numbered(8);
        let mut modified = original.clone();
        modified.remove(4);
        assert_eq!(
            generate_diff(&original.concat(), &modified.concat(), "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -2,7 +2,6 @@\n 2\n 3\n 4\n-5\n 6\n 7\n 8\n"
        );
    }

    #[test]
    fn test_generate_diff_marks_an_added_final_newline() {
        assert_eq!(
            generate_diff("# T\n\ntext", "# T\n\ntext\n", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1,3 +1,3 @@\n # T\n \n-text\n\\ No newline at end of file\n+text\n"
        );
    }

    #[test]
    fn test_generate_diff_marks_a_removed_final_newline() {
        assert_eq!(
            generate_diff("a\nb\n", "a\nb", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1,2 +1,2 @@\n a\n-b\n+b\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn test_generate_diff_keeps_crlf_line_endings() {
        assert_eq!(
            generate_diff("a\r\nb\r\n", "a\r\nB\r\n", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1,2 +1,2 @@\n a\r\n-b\r\n+B\r\n"
        );
    }

    #[test]
    fn test_generate_diff_keeps_a_lone_carriage_return_inside_its_line() {
        assert_eq!(
            generate_diff("a\rb\nc\n", "a\rB\nc\n", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1,2 +1,2 @@\n-a\rb\n+a\rB\n c\n"
        );
    }

    #[test]
    fn test_generate_diff_marks_a_final_line_ending_in_a_lone_carriage_return() {
        assert_eq!(
            generate_diff("a\n", "a\nb\r", "doc.md"),
            "--- doc.md\n+++ doc.md\n@@ -1 +1,2 @@\n a\n+b\r\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn test_generate_diff_joins_changes_whose_context_touches() {
        let original = numbered(20);
        let mut modified = original.clone();
        modified[1] = "X\n".to_string();
        modified[8] = "Y\n".to_string();
        let diff = generate_diff(&original.concat(), &modified.concat(), "doc.md");
        assert_eq!(hunk_headers(&diff), ["@@ -1,12 +1,12 @@"], "{diff}");
    }

    #[test]
    fn test_generate_diff_separates_changes_whose_context_does_not_touch() {
        let original = numbered(20);
        let mut modified = original.clone();
        modified[1] = "X\n".to_string();
        modified[9] = "Y\n".to_string();
        let diff = generate_diff(&original.concat(), &modified.concat(), "doc.md");
        assert_eq!(hunk_headers(&diff), ["@@ -1,5 +1,5 @@", "@@ -7,7 +7,7 @@"], "{diff}");
    }

    #[test]
    fn test_generate_diff_names_a_nested_path_on_both_sides() {
        let diff = generate_diff("a\n", "b\n", "docs/guide/doc.md");
        assert!(
            diff.starts_with("--- docs/guide/doc.md\n+++ docs/guide/doc.md\n@@ "),
            "{diff}"
        );
    }

    #[test]
    fn test_format_toml_value_string_is_quoted() {
        let val = toml::Value::String("hello world".to_string());
        assert_eq!(format_toml_value(&val), "\"hello world\"");
    }

    #[test]
    fn test_format_toml_value_integer() {
        let val = toml::Value::Integer(42);
        assert_eq!(format_toml_value(&val), "42");
    }

    #[test]
    fn test_format_toml_value_boolean_true() {
        assert_eq!(format_toml_value(&toml::Value::Boolean(true)), "true");
    }

    #[test]
    fn test_format_toml_value_boolean_false() {
        assert_eq!(format_toml_value(&toml::Value::Boolean(false)), "false");
    }

    #[test]
    fn test_format_toml_value_array_of_strings() {
        let val = toml::Value::Array(vec![
            toml::Value::String("a".to_string()),
            toml::Value::String("b".to_string()),
        ]);
        assert_eq!(format_toml_value(&val), r#"["a", "b"]"#);
    }

    #[test]
    fn test_format_toml_value_empty_array() {
        let val = toml::Value::Array(vec![]);
        assert_eq!(format_toml_value(&val), "[]");
    }

    #[test]
    fn test_format_toml_value_table_is_placeholder() {
        let val = toml::Value::Table(toml::map::Map::new());
        assert_eq!(format_toml_value(&val), "<table>");
    }

    #[test]
    fn test_format_toml_value_nested_array() {
        let val = toml::Value::Array(vec![
            toml::Value::Integer(1),
            toml::Value::Integer(2),
            toml::Value::Integer(3),
        ]);
        assert_eq!(format_toml_value(&val), "[1, 2, 3]");
    }
}
