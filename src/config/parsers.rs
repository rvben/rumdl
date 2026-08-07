use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use toml_edit::DocumentMut;

use super::flavor::{MarkdownFlavor, normalize_key, warn_comma_without_brace_in_pattern};
use super::source_tracking::{
    ConfigSource, SourcedConfigFragment, SourcedHtmlConfig, SourcedScriptConfig, SourcedValue,
};
use super::types::{ConfigError, ConfigOrigin, ConfigRef, HtmlConfig, WITHHELD};
use super::validation::to_relative_display_path;

/// Describe a TOML parse failure for a config file.
///
/// Both TOML crates render an error by quoting the source line it points at,
/// with a caret under the offending column. That is the most useful form for a
/// config the user named, and the wrong form for one reached through `extends`:
/// the line belongs to a file `extends` merely pointed rumdl at, so quoting it
/// prints a stranger's file into whatever reads the error. The position still
/// locates the problem for anyone who can open the file, and discloses nothing
/// to anyone who cannot.
fn describe_toml_error(
    content: &str,
    config: ConfigRef<'_>,
    span: Option<std::ops::Range<usize>>,
    message: &str,
    rendered: &dyn std::fmt::Display,
) -> String {
    if config.may_quote_contents() {
        return rendered.to_string();
    }
    match span {
        Some(span) => {
            let (line, column) = line_and_column(content, span.start);
            format!("at line {line}, column {column}: {message}")
        }
        None => message.to_string(),
    }
}

/// 1-based line and column of a byte offset in `content`, counting characters.
///
/// An offset past the end, or inside a multi-byte character, walks back to the
/// nearest boundary rather than panicking: this runs while reporting an error,
/// and a bad span is not worth a second one.
fn line_and_column(content: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &content[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

/// Parses pyproject.toml content and extracts the [tool.rumdl] section if present.
pub(super) fn parse_pyproject_toml(
    content: &str,
    path: &str,
    source: ConfigSource,
    origin: ConfigOrigin<'_>,
) -> Result<Option<SourcedConfigFragment>, ConfigError> {
    let name = origin.display_name(path);
    let display_path = ConfigRef::new(&name, origin);
    let doc: toml::Value = toml::from_str(content).map_err(|e| {
        let detail = describe_toml_error(content, display_path, e.span(), e.message(), &e);
        ConfigError::ParseError(format!("{display_path}: Failed to parse TOML: {detail}"))
    })?;
    let mut fragment = SourcedConfigFragment::default();
    let file = Some(path.to_string());

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // Parse `extends` from [tool.rumdl] level
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
        && let Some(extends_val) = rumdl_table.get("extends")
        && let Ok(extends_str) = String::deserialize(extends_val.clone())
    {
        fragment.extends = Some(extends_str);
    }

    // 1. Handle [tool.rumdl] and [tool.rumdl.global] sections
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
    {
        // Apply global options from the given table through the shared
        // global-key dispatch (`global_keys::apply_global_key`). Keys are
        // normalized first, so snake_case spellings keep working. Rule
        // sections, `extends`, and other non-global keys fall through as
        // Unrecognized and are handled by their own parsers.
        let extract_global_config = |fragment: &mut SourcedConfigFragment, table: &toml::value::Table| {
            use super::global_keys::{ApplyOutcome, apply_global_key};
            for (key, value) in table {
                let norm_key = normalize_key(key);
                match apply_global_key(
                    &mut fragment.global,
                    &norm_key,
                    value,
                    source,
                    file.as_deref(),
                    registry,
                ) {
                    ApplyOutcome::Applied => {
                        // line-length is mirrored into MD013 for backward
                        // compatibility with configs that predate the global key.
                        if norm_key == "line-length" {
                            let rule_entry = fragment.rules.entry(normalize_key("MD013")).or_default();
                            let sv = rule_entry
                                .values
                                .entry(norm_key.clone())
                                .or_insert_with(|| SourcedValue::new(value.clone(), ConfigSource::Default));
                            sv.push_override(value.clone(), source, file.clone());
                        }
                    }
                    ApplyOutcome::TypeMismatch { expected } => {
                        log::warn!("[WARN] Expected {expected} for global key '{norm_key}' in {display_path}");
                    }
                    ApplyOutcome::InvalidValue { message } => {
                        // The message names the offending value, so it can only
                        // be repeated for a file whose text may be shown. The
                        // key itself is one rumdl recognized, not text out of
                        // the file, so it is safe either way.
                        if display_path.may_quote_contents() {
                            log::warn!("[WARN] {message} in {display_path}");
                        } else {
                            log::warn!("[WARN] Invalid value for global key '{norm_key}' in {display_path}");
                        }
                    }
                    ApplyOutcome::Unrecognized => {}
                }
            }
        };

        // First, check for [tool.rumdl.global] section
        if let Some(global_table) = rumdl_table.get("global").and_then(|g| g.as_table()) {
            extract_global_config(&mut fragment, global_table);
        }

        // Also extract global options from [tool.rumdl] directly (for flat structure)
        extract_global_config(&mut fragment, rumdl_table);

        // --- Extract per-file-ignores configurations ---
        // Check both hyphenated and underscored versions for compatibility
        let per_file_ignores_key = rumdl_table
            .get("per-file-ignores")
            .or_else(|| rumdl_table.get("per_file_ignores"));

        if let Some(per_file_ignores_value) = per_file_ignores_key
            && let Some(per_file_table) = per_file_ignores_value.as_table()
        {
            let mut per_file_map = BTreeMap::new();
            for (pattern, rules_value) in per_file_table {
                warn_comma_without_brace_in_pattern(pattern, display_path);
                if let Ok(rules) = Vec::<String>::deserialize(rules_value.clone()) {
                    let normalized_rules = rules
                        .into_iter()
                        .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                        .collect();
                    per_file_map.insert(pattern.clone(), normalized_rules);
                } else {
                    let pattern = display_path.quote(pattern);
                    if display_path.may_quote_contents() {
                        log::warn!(
                            "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {display_path}, found {rules_value:?}"
                        );
                    } else {
                        log::warn!("[WARN] Expected array for per-file-ignores pattern '{pattern}' in {display_path}");
                    }
                }
            }
            fragment
                .per_file_ignores
                .push_override(per_file_map, source, file.clone());
        }

        // --- Extract per-file-flavor configurations ---
        // Check both hyphenated and underscored versions for compatibility
        let per_file_flavor_key = rumdl_table
            .get("per-file-flavor")
            .or_else(|| rumdl_table.get("per_file_flavor"));

        if let Some(per_file_flavor_value) = per_file_flavor_key
            && let Some(per_file_table) = per_file_flavor_value.as_table()
        {
            let mut per_file_map = IndexMap::new();
            for (pattern, flavor_value) in per_file_table {
                if let Ok(flavor) = MarkdownFlavor::deserialize(flavor_value.clone()) {
                    per_file_map.insert(pattern.clone(), flavor);
                } else {
                    let pattern = display_path.quote(pattern);
                    if display_path.may_quote_contents() {
                        log::warn!(
                            "[WARN] Invalid flavor for per-file-flavor pattern '{pattern}' in {display_path}, found {flavor_value:?}. Valid values: standard, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst"
                        );
                    } else {
                        log::warn!(
                            "[WARN] Invalid flavor for per-file-flavor pattern '{pattern}' in {display_path}. Valid values: standard, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst"
                        );
                    }
                }
            }
            fragment
                .per_file_flavor
                .push_override(per_file_map, source, file.clone());
        }

        // --- Extract html configurations ---
        if let Some(html_value) = rumdl_table.get("html") {
            match HtmlConfig::deserialize(html_value.clone()) {
                Ok(html_config) => {
                    let sourced_html = to_sourced_html_config(html_config, source, file.clone());
                    fragment.html = sourced_html;
                }
                Err(e) => {
                    log::warn!("[WARN] Failed to parse [html] section in {display_path}: {e}");
                }
            }
        }

        // --- Extract rule-specific configurations ---
        for (key, value) in rumdl_table {
            let norm_rule_key = normalize_key(key);

            // Skip keys already handled as global or special cases
            // Note: Only skip these if they're NOT tables (rule sections are tables)
            let is_global_key = [
                "enable",
                "disable",
                "include",
                "exclude",
                "respect_gitignore",
                "respect-gitignore",
                "force_exclude",
                "force-exclude",
                "output_format",
                "output-format",
                "fixable",
                "unfixable",
                "per-file-ignores",
                "per_file_ignores",
                "per-file-flavor",
                "per_file_flavor",
                "global",
                "flavor",
                "cache_dir",
                "cache-dir",
                "cache",
                "extend-enable",
                "extend_enable",
                "extend-disable",
                "extend_disable",
                "extends",
                "editorconfig",
                "html",
            ]
            .contains(&norm_rule_key.as_str());

            // Special handling for line-length: could be global config OR rule section
            let is_line_length_global =
                (norm_rule_key == "line-length" || norm_rule_key == "line_length") && !value.is_table();

            if is_global_key || is_line_length_global {
                continue;
            }

            // Handle [tool.rumdl.rules.MDxxx] — accepted as an alias for [tool.rumdl.MDxxx].
            if norm_rule_key == "rules" {
                if let Some(inner_table) = value.as_table() {
                    for (inner_key, inner_value) in inner_table {
                        if let Some(resolved_inner_rule) = registry.resolve_rule_name(inner_key) {
                            if let Some(inner_rule_table) = inner_value.as_table() {
                                apply_rule_table_toml(
                                    &resolved_inner_rule,
                                    inner_rule_table,
                                    &mut fragment,
                                    source,
                                    &file,
                                    display_path,
                                );
                            }
                        } else {
                            // Store without the "rules." prefix so validation's edit-distance
                            // suggestion logic sees the bare rule name, not "rules.MD999".
                            fragment.unknown_keys.push((
                                format!("[tool.rumdl.{}]", display_path.quote(inner_key)),
                                String::new(),
                                Some(display_path.to_string()),
                            ));
                        }
                    }
                }
                continue;
            }

            // Try to resolve as a rule name (handles both canonical names and aliases)
            if let Some(resolved_rule_name) = registry.resolve_rule_name(key)
                && value.is_table()
                && let Some(rule_config_table) = value.as_table()
            {
                apply_rule_table_toml(
                    &resolved_rule_name,
                    rule_config_table,
                    &mut fragment,
                    source,
                    &file,
                    display_path,
                );
            } else if registry.resolve_rule_name(key).is_none() {
                // Key is not a global/special key and not a recognized rule name
                // Track unknown keys under [tool.rumdl] for validation
                fragment.unknown_keys.push((
                    "[tool.rumdl]".to_string(),
                    display_path.quote(key).to_string(),
                    Some(display_path.to_string()),
                ));
            }
        }
    }

    // 2. Handle [tool.rumdl.MDxxx] and [tool.rumdl.rules.MDxxx] nested under [tool]
    if let Some(tool_table) = doc.get("tool").and_then(|t| t.as_table()) {
        for (key, value) in tool_table {
            // Accept both "rumdl.MDxxx" and "rumdl.rules.MDxxx" prefixes
            let rule_name = key.strip_prefix("rumdl.rules.").or_else(|| key.strip_prefix("rumdl."));
            if let Some(rule_name) = rule_name {
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        apply_rule_table_toml(
                            &resolved_rule_name,
                            rule_table,
                            &mut fragment,
                            source,
                            &file,
                            display_path,
                        );
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD") || rule_name.chars().any(char::is_alphabetic)
                {
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{}]", display_path.quote(rule_name)),
                        String::new(),
                        Some(display_path.to_string()),
                    ));
                }
            }
        }
    }

    // 3. Handle [tool.rumdl.MDxxx] and [tool.rumdl.rules.MDxxx] as top-level dotted keys
    if let Some(doc_table) = doc.as_table() {
        for (key, value) in doc_table {
            // Accept both "tool.rumdl.MDxxx" and "tool.rumdl.rules.MDxxx" prefixes
            let rule_name = key
                .strip_prefix("tool.rumdl.rules.")
                .or_else(|| key.strip_prefix("tool.rumdl."));
            if let Some(rule_name) = rule_name {
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        apply_rule_table_toml(
                            &resolved_rule_name,
                            rule_table,
                            &mut fragment,
                            source,
                            &file,
                            display_path,
                        );
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD") || rule_name.chars().any(char::is_alphabetic)
                {
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{}]", display_path.quote(rule_name)),
                        String::new(),
                        Some(display_path.to_string()),
                    ));
                }
            }
        }
    }

    // Only return Some(fragment) if any config was found
    let has_any = fragment.extends.is_some()
        || !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.extend_enable.value.is_empty()
        || !fragment.global.extend_disable.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.fixable.value.is_empty()
        || !fragment.global.unfixable.value.is_empty()
        || fragment.global.output_format.is_some()
        || fragment.global.cache_dir.is_some()
        || fragment.global.cache.source != ConfigSource::Default
        || fragment.global.flavor.source != ConfigSource::Default
        || fragment.global.respect_gitignore.source != ConfigSource::Default
        || fragment.global.force_exclude.source != ConfigSource::Default
        || fragment.global.editorconfig.source != ConfigSource::Default
        || !fragment.per_file_ignores.value.is_empty()
        || !fragment.per_file_flavor.value.is_empty()
        || !fragment.rules.is_empty();
    if has_any {
        withhold_unquotable_text(&mut fragment, display_path);
        Ok(Some(fragment))
    } else {
        Ok(None)
    }
}

/// The option keys of a rule table that must not reach the config map, together
/// with the entry that keeps the validator's warning about them.
///
/// A key the rule recognizes is rumdl's own vocabulary, safe to name whatever
/// file it came from. One it does not recognize is text out of that file, and
/// the validator prints an unknown option back verbatim, so a key from a file
/// reached through `extends` must not be stored under its own name. Recording it
/// as an unknown key of the rule's section keeps the warning and drops the name.
///
/// A file the user named themselves is answered with the empty set, leaving that
/// path exactly as it was.
///
/// One entry covers however many keys were withheld: once the names are gone the
/// keys are indistinguishable, and repeated entries merge back into a single
/// warning anyway.
fn withhold_unknown_options<'k>(
    norm_rule_name: &str,
    keys: impl Iterator<Item = &'k str>,
    fragment: &mut SourcedConfigFragment,
    display_path: ConfigRef<'_>,
) -> BTreeSet<String> {
    if display_path.may_quote_contents() {
        return BTreeSet::new();
    }
    // The same registry the validator consults, so the two agree on what counts
    // as recognized and no key is withheld that would never have been printed.
    let Some(valid_keys) = super::registry::default_registry().config_keys_for(norm_rule_name) else {
        return BTreeSet::new();
    };
    let withheld: BTreeSet<String> = keys
        .map(normalize_key)
        .filter(|key| !valid_keys.contains(key))
        .collect();
    if !withheld.is_empty() {
        fragment.unknown_keys.push((
            format!("[{norm_rule_name}]"),
            WITHHELD.to_string(),
            Some(display_path.to_string()),
        ));
    }
    withheld
}

/// Keep the text a parsed fragment carries out of the messages raised about it,
/// when it came from a file whose contents may not be shown.
///
/// Those messages are raised long after parsing, from code that has no idea which
/// file the text came from: the validator prints an unrecognized rule name out of
/// `global.enable` and its siblings, the glob caches and the file walk print a
/// pattern they failed to compile, and a rule prints the option value it could not
/// deserialize. This runs at the last point that still knows the origin, so those
/// messages have nothing left to disclose.
///
/// Nothing about the run changes. Every entry removed here was already inert: a
/// rule name matching no rule selected nothing, and a pattern that does not compile
/// matched no file. Anything that is not provably inert is marked rather than
/// removed and still applies as written: a rule option's value, which nothing checks
/// at parse time, and an `include` pattern, which the walk rewrites before judging.
/// Names rumdl does recognize are left alone, and a file the user named themselves
/// is not touched at all.
fn withhold_unquotable_text(fragment: &mut SourcedConfigFragment, display_path: ConfigRef<'_>) {
    if display_path.may_quote_contents() {
        return;
    }

    // One placeholder stands in for however many names went: with the names gone
    // the entries are indistinguishable, and the count is itself something the
    // file did not publish.
    for list in [
        &mut fragment.global.enable,
        &mut fragment.global.disable,
        &mut fragment.global.extend_enable,
        &mut fragment.global.extend_disable,
        &mut fragment.global.fixable,
        &mut fragment.global.unfixable,
    ] {
        let before = list.value.len();
        list.value.retain(|name| super::registry::is_valid_rule_name(name));
        if list.value.len() != before {
            list.value.push(WITHHELD.to_string());
        }
    }

    // The same construction each cache performs, so a pattern is withheld here
    // only when the cache is the one that would have printed it.
    fragment.per_file_ignores.value.retain(|pattern, _| {
        let compiles = globset::Glob::new(&crate::discovery::expand_home_prefix(pattern)).is_ok();
        if !compiles {
            log::warn!("Invalid glob pattern in per-file-ignores in {display_path}: {WITHHELD}");
        }
        compiles
    });
    fragment.per_file_flavor.value.retain(|pattern, _| {
        let compiles = globset::GlobBuilder::new(&crate::discovery::expand_home_prefix(pattern))
            .literal_separator(true)
            .build()
            .is_ok();
        if !compiles {
            log::warn!("Invalid glob pattern in per-file-flavor in {display_path}: {WITHHELD}");
        }
        compiles
    });

    // An `exclude` pattern reaches its consumers exactly as written, so the helper
    // mirrors every construction they perform and a pattern is withheld here only
    // when one of them is the one that would have printed it. The warning they can
    // no longer raise is recorded in its place; theirs is always on, so this one
    // goes to the config-warning channel rather than the log.
    let mut withheld_patterns = Vec::new();
    fragment.global.exclude.value.retain(|pattern| {
        let compiles = crate::discovery::exclude_pattern_compiles(pattern);
        if !compiles {
            withheld_patterns.push(format!("Invalid exclude pattern in {display_path}: {WITHHELD}"));
        }
        compiles
    });
    fragment.load_warnings.extend(withheld_patterns);

    // An `include` pattern cannot be judged here: the walk rewrites an absolute
    // pattern as one relative to the project root, which this does not know, and
    // that strips whatever the root's own name holds. Under a directory called
    // `notes [2019-2021]` the pattern carries a character class over a descending
    // range and does not compile until the prefix comes off, so checking it here
    // would drop a pattern the walk would have used. The walk is left to judge it
    // and the mark says what it may quote when it does.
    if fragment.global.include.source != ConfigSource::Default {
        fragment.global.include_withheld = Some(display_path.to_string());
    }

    // A rule option's value reaches the rule that owns it, which deserializes the
    // whole section and quotes whatever it could not read. Only the rule knows what
    // it accepts, and it is not here, so every value this file supplies is marked;
    // the mark travels with the value, so a config that takes the key over takes
    // over what may be said about its value too.
    for rule in fragment.rules.values_mut() {
        rule.withheld_keys = rule.values.keys().cloned().collect();
    }

    // `[code-block-tools]` names tools and languages that only the registry can
    // resolve, which it does long after this. The whole section merges as one value,
    // so marking the value is enough for the mark to travel with it.
    if fragment.code_block_tools.source != ConfigSource::Default {
        fragment.code_block_tools.value.values_withheld = true;
    }
}

/// Applies a rule configuration table (in standard `toml` format) into the fragment.
/// Used for rule sections parsed from pyproject.toml, including `[tool.rumdl.MDxxx]`
/// and `[tool.rumdl.rules.MDxxx]` forms.
fn apply_rule_table_toml(
    norm_rule_name: &str,
    rule_config_table: &toml::value::Table,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: ConfigRef<'_>,
) {
    let withheld = withhold_unknown_options(
        norm_rule_name,
        rule_config_table.keys().map(String::as_str),
        fragment,
        display_path,
    );
    let rule_entry = fragment.rules.entry(norm_rule_name.to_string()).or_default();
    for (rk, rv) in rule_config_table {
        let norm_rk = normalize_key(rk);
        if withheld.contains(&norm_rk) {
            continue;
        }

        if norm_rk == "severity" {
            if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                if let Some(ref mut sv) = rule_entry.severity {
                    sv.push_override(severity, source, file.clone());
                } else {
                    rule_entry.severity = Some(SourcedValue::new(severity, source));
                }
            } else if let Some(severity_str) = rv.as_str() {
                let severity_str = display_path.quote(severity_str);
                log::warn!(
                    "[WARN] Invalid severity '{severity_str}' for rule {norm_rule_name} in {display_path}. Valid values: error, warning"
                );
            }
            continue;
        }

        let toml_val = rv.clone();
        let sv = rule_entry
            .values
            .entry(norm_rk.clone())
            .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
        sv.push_override(toml_val, source, file.clone());
    }
}

pub(super) use super::global_keys::is_global_value_key;

/// Parse a single global config key-value pair and store it in the fragment.
/// Used by both the `[global]` section handler and the top-level key handler.
/// Delegates the per-key typing and setting to `global_keys::apply_global_key`,
/// keeping only the toml_edit bridging and warning phrasing here.
fn parse_global_key(
    norm_key: &str,
    value_item: &toml_edit::Item,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: ConfigRef<'_>,
    registry: &super::registry::RuleRegistry,
) -> bool {
    use super::global_keys::{ApplyOutcome, apply_global_key};

    if !super::global_keys::is_global_value_key(norm_key) {
        return false;
    }

    let Some(edit_value) = value_item.as_value() else {
        log::warn!(
            "[WARN] Expected a value for global key '{}' in {}, found {}",
            norm_key,
            display_path,
            value_item.type_name()
        );
        return true;
    };

    let value = toml_edit_value_to_toml(edit_value);
    match apply_global_key(
        &mut fragment.global,
        norm_key,
        &value,
        source,
        file.as_deref(),
        registry,
    ) {
        ApplyOutcome::Applied => {}
        ApplyOutcome::TypeMismatch { expected } => {
            log::warn!(
                "[WARN] Expected {} for global key '{}' in {}, found {}",
                expected,
                norm_key,
                display_path,
                value_item.type_name()
            );
        }
        ApplyOutcome::InvalidValue { message } => {
            // See the matching arm in `parse_pyproject_toml`: the message names
            // the value, the key does not come out of the file.
            if display_path.may_quote_contents() {
                log::warn!("[WARN] {message} in {display_path}");
            } else {
                log::warn!("[WARN] Invalid value for global key '{norm_key}' in {display_path}");
            }
        }
        ApplyOutcome::Unrecognized => unreachable!("guarded by is_global_value_key above"),
    }
    true
}

/// Parses rumdl.toml / .rumdl.toml content.
pub(super) fn parse_rumdl_toml(
    content: &str,
    path: &str,
    source: ConfigSource,
    origin: ConfigOrigin<'_>,
) -> Result<SourcedConfigFragment, ConfigError> {
    let name = origin.display_name(path);
    let display_path = ConfigRef::new(&name, origin);
    let doc = content.parse::<DocumentMut>().map_err(|e| {
        let detail = describe_toml_error(content, display_path, e.span(), e.message(), &e);
        ConfigError::ParseError(format!("{display_path}: Failed to parse TOML: {detail}"))
    })?;
    let mut fragment = SourcedConfigFragment::default();
    // source parameter provided by caller
    let file = Some(path.to_string());

    // Parse top-level `extends` key (not inside any section)
    if let Some(extends_item) = doc.get("extends")
        && let Some(extends_val) = extends_item.as_value()
        && let Some(extends_str) = extends_val.as_str()
    {
        fragment.extends = Some(extends_str.to_string());
    }

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // Handle top-level global keys (ruff-style shorthand).
    // These are parsed BEFORE [global] so that explicit [global] section values
    // take precedence via push_override.
    for (key, item) in doc.iter() {
        if item.is_value() {
            let norm_key = normalize_key(key);
            if is_global_value_key(&norm_key) {
                let handled = parse_global_key(&norm_key, item, &mut fragment, source, &file, display_path, registry);
                debug_assert!(
                    handled,
                    "Key '{norm_key}' is in GLOBAL_VALUE_KEYS but not handled by parse_global_key"
                );
            }
        }
    }

    // Handle [global] section (overrides top-level shorthand keys above)
    if let Some(global_item) = doc.get("global")
        && let Some(global_table) = global_item.as_table()
    {
        for (key, value_item) in global_table {
            let norm_key = normalize_key(key);
            if !parse_global_key(
                &norm_key,
                value_item,
                &mut fragment,
                source,
                &file,
                display_path,
                registry,
            ) {
                // Track unknown global keys for validation
                let key = display_path.quote(key);
                fragment
                    .unknown_keys
                    .push(("[global]".to_string(), key.to_string(), Some(display_path.to_string())));
                log::warn!("[WARN] Unknown key in [global] section of {display_path}: {key}");
            }
        }
    }

    // Handle [per-file-ignores] section
    if let Some(per_file_item) = doc.get("per-file-ignores")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = BTreeMap::new();
        for (pattern, value_item) in per_file_table {
            warn_comma_without_brace_in_pattern(pattern, display_path);
            if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                let rules: Vec<String> = formatted_array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
                    .collect();
                per_file_map.insert(pattern.to_string(), rules);
            } else {
                let pattern = display_path.quote(pattern);
                let type_name = value_item.type_name();
                log::warn!(
                    "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {display_path}, found {type_name}"
                );
            }
        }
        fragment
            .per_file_ignores
            .push_override(per_file_map, source, file.clone());
    }

    // Handle [per-file-flavor] section
    if let Some(per_file_item) = doc.get("per-file-flavor")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = IndexMap::new();
        for (pattern, value_item) in per_file_table {
            if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                let flavor_str = formatted_string.value();
                match MarkdownFlavor::deserialize(toml::Value::String(flavor_str.clone())) {
                    Ok(flavor) => {
                        per_file_map.insert(pattern.to_string(), flavor);
                    }
                    Err(_) => {
                        let flavor_str = display_path.quote(flavor_str);
                        let pattern = display_path.quote(pattern);
                        log::warn!(
                            "[WARN] Invalid flavor '{flavor_str}' for pattern '{pattern}' in {display_path}. Valid values: standard, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst"
                        );
                    }
                }
            } else {
                let pattern = display_path.quote(pattern);
                let type_name = value_item.type_name();
                log::warn!(
                    "[WARN] Expected string for per-file-flavor pattern '{pattern}' in {display_path}, found {type_name}"
                );
            }
        }
        fragment
            .per_file_flavor
            .push_override(per_file_map, source, file.clone());
    }

    // Handle [code-block-tools] section
    if let Some(cbt_item) = doc.get("code-block-tools")
        && let Some(cbt_table) = cbt_item.as_table()
    {
        // Convert the table to a proper TOML document for deserialization
        // We need to create a new document with just this section, properly formatted
        let mut cbt_doc = toml_edit::DocumentMut::new();
        for (key, value) in cbt_table {
            cbt_doc[key] = value.clone();
        }
        let cbt_toml_str = cbt_doc.to_string();
        match toml::from_str::<crate::code_block_tools::CodeBlockToolsConfig>(&cbt_toml_str) {
            Ok(cbt_config) => {
                fragment
                    .code_block_tools
                    .push_override(cbt_config, source, file.clone());
            }
            Err(e) => {
                // The rendered error quotes the section it was given, which is a
                // re-serialization of the file's own text.
                if display_path.may_quote_contents() {
                    log::warn!("[WARN] Failed to parse [code-block-tools] section in {display_path}: {e}");
                } else {
                    log::warn!("[WARN] Failed to parse [code-block-tools] section in {display_path}");
                }
            }
        }
    }

    // Handle [html] section
    if let Some(html_item) = doc.get("html")
        && let Some(html_table) = html_item.as_table()
    {
        let mut html_doc = toml_edit::DocumentMut::new();
        for (key, value) in html_table {
            html_doc[key] = value.clone();
        }
        let html_toml_str = html_doc.to_string();
        match toml::from_str::<HtmlConfig>(&html_toml_str) {
            Ok(html_config) => {
                let sourced_html = to_sourced_html_config(html_config, source, file.clone());
                fragment.html = sourced_html;
            }
            Err(e) => {
                log::warn!("[WARN] Failed to parse [html] section in {display_path}: {e}");
            }
        }
    }

    // Rule-specific: all other top-level tables
    for (key, item) in doc.iter() {
        // Skip known special sections and top-level value keys (already handled above)
        if key == "global"
            || key == "per-file-ignores"
            || key == "per-file-flavor"
            || key == "code-block-tools"
            || key == "extends"
            || key == "html"
        {
            continue;
        }

        // Skip top-level value keys that were already parsed as global config
        if item.is_value() {
            let norm_key = normalize_key(key);
            if is_global_value_key(&norm_key) {
                continue;
            }
        }

        // Handle [rules.MDxxx] wrapper — accepted as an alias for flat [MDxxx] sections.
        // This lets users port markdownlint configs that group rules under a [rules] heading.
        if key == "rules" {
            if let Some(rules_tbl) = item.as_table() {
                for (inner_key, inner_item) in rules_tbl {
                    if let Some(norm_inner_rule) = registry.resolve_rule_name(inner_key) {
                        if let Some(inner_tbl) = inner_item.as_table() {
                            apply_rule_table_toml_edit(
                                &norm_inner_rule,
                                inner_tbl,
                                &mut fragment,
                                source,
                                &file,
                                display_path,
                            );
                        } else {
                            let inner_key = display_path.quote(inner_key);
                            let type_name = inner_item.type_name();
                            log::warn!(
                                "[WARN] Expected table for rule '{inner_key}' in [rules] section of {display_path}, found {type_name}; ignoring"
                            );
                        }
                    } else {
                        // Store without the "rules." prefix so validation's edit-distance
                        // suggestion logic sees the bare rule name (e.g. "MD999"), not "rules.MD999".
                        fragment.unknown_keys.push((
                            format!("[{}]", display_path.quote(inner_key)),
                            String::new(),
                            Some(display_path.to_string()),
                        ));
                    }
                }
            }
            continue;
        }

        // Resolve rule name (handles both canonical names like "MD004" and aliases like "ul-style")
        let Some(norm_rule_name) = registry.resolve_rule_name(key) else {
            // Unknown rule - always track it for validation and suggestions
            fragment.unknown_keys.push((
                format!("[{}]", display_path.quote(key)),
                String::new(),
                Some(display_path.to_string()),
            ));
            continue;
        };

        if let Some(tbl) = item.as_table() {
            apply_rule_table_toml_edit(&norm_rule_name, tbl, &mut fragment, source, &file, display_path);
        } else if item.is_value() {
            let key = display_path.quote(key);
            log::warn!(
                "[WARN] Ignoring top-level value key in {display_path}: '{key}'. Expected a table like [{key}]."
            );
        }
    }

    withhold_unquotable_text(&mut fragment, display_path);
    Ok(fragment)
}

/// Converts a `toml_edit` value into the equivalent plain `toml::Value`,
/// preserving the TOML type it was written as.
fn toml_edit_value_to_toml(value: &toml_edit::Value) -> toml::Value {
    match value {
        toml_edit::Value::String(s) => toml::Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => toml::Value::Integer(*i.value()),
        toml_edit::Value::Float(f) => toml::Value::Float(*f.value()),
        toml_edit::Value::Boolean(b) => toml::Value::Boolean(*b.value()),
        toml_edit::Value::Datetime(d) => toml::Value::Datetime(*d.value()),
        toml_edit::Value::Array(arr) => toml::Value::Array(arr.iter().map(toml_edit_value_to_toml).collect()),
        toml_edit::Value::InlineTable(t) => toml::Value::Table(
            t.iter()
                .map(|(k, v)| (k.to_string(), toml_edit_value_to_toml(v)))
                .collect(),
        ),
    }
}

/// Converts a rule option's table into a `toml::value::Table`.
///
/// The keys of a map-valued option are data (a language name, a glob), not
/// configuration keys, so they are carried over as written rather than normalized.
fn rule_option_table(table: &toml_edit::Table) -> toml::value::Table {
    table
        .iter()
        .filter_map(|(key, item)| Some((key.to_string(), rule_option_value(item)?)))
        .collect()
}

/// The value a rule option holds, or `None` when the item carries no value.
///
/// Every TOML shape a rule option can take is converted, including the map forms
/// (`preferred-aliases = { Shell = "sh" }` and its `[MD040.preferred-aliases]`
/// sub-table spelling), so that a map option set in a config file reaches serde
/// instead of being dropped.
fn rule_option_value(item: &toml_edit::Item) -> Option<toml::Value> {
    match item {
        toml_edit::Item::Value(value) => Some(toml_edit_value_to_toml(value)),
        toml_edit::Item::Table(table) => Some(toml::Value::Table(rule_option_table(table))),
        toml_edit::Item::ArrayOfTables(tables) => Some(toml::Value::Array(
            tables
                .iter()
                .map(|table| toml::Value::Table(rule_option_table(table)))
                .collect(),
        )),
        toml_edit::Item::None => None,
    }
}

/// Applies a rule configuration table (in toml_edit format) into the fragment.
/// Used for both `[MDxxx]` and `[rules.MDxxx]` top-level table forms in rumdl.toml.
fn apply_rule_table_toml_edit(
    norm_rule_name: &str,
    tbl: &toml_edit::Table,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: ConfigRef<'_>,
) {
    let withheld = withhold_unknown_options(norm_rule_name, tbl.iter().map(|(key, _)| key), fragment, display_path);
    let rule_entry = fragment.rules.entry(norm_rule_name.to_string()).or_default();
    for (rk, rv_item) in tbl {
        let norm_rk = normalize_key(rk);
        if withheld.contains(&norm_rk) {
            continue;
        }

        // Special handling for severity - store in rule_entry.severity
        if norm_rk == "severity" {
            if let Some(toml_edit::Value::String(formatted_string)) = rv_item.as_value() {
                let severity_str = formatted_string.value();
                match crate::rule::Severity::deserialize(toml::Value::String(severity_str.clone())) {
                    Ok(severity) => {
                        if let Some(ref mut sv) = rule_entry.severity {
                            sv.push_override(severity, source, file.clone());
                        } else {
                            rule_entry.severity = Some(SourcedValue::new(severity, source));
                        }
                    }
                    Err(_) => {
                        let severity_str = display_path.quote(severity_str);
                        log::warn!(
                            "[WARN] Invalid severity '{severity_str}' for rule {norm_rule_name} in {display_path}. Valid values: error, warning"
                        );
                    }
                }
            }
            continue; // Skip regular value processing for severity
        }

        let maybe_toml_val = rule_option_value(rv_item);
        if maybe_toml_val.is_none() {
            let norm_rk = display_path.quote(&norm_rk);
            log::warn!(
                "[WARN] Skipping non-value item for key '{norm_rule_name}.{norm_rk}' in {display_path}. Expected simple value."
            );
        }
        if let Some(toml_val) = maybe_toml_val {
            let sv = rule_entry
                .values
                .entry(norm_rk.clone())
                .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
            sv.push_override(toml_val, source, file.clone());
        }
    }
}

/// Loads and converts a markdownlint config file (.json or .yaml) into a SourcedConfigFragment.
pub(super) fn load_from_markdownlint(path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    let display_path = to_relative_display_path(path);
    // Use the unified loader from markdownlint_config.rs
    let ml_config = crate::markdownlint_config::load_markdownlint_config(path)
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: {e}")))?;
    Ok(ml_config.map_to_sourced_rumdl_config_fragment(Some(path)))
}

fn to_sourced_html_config(config: HtmlConfig, source: ConfigSource, origin: Option<String>) -> SourcedHtmlConfig {
    SourcedHtmlConfig {
        enabled: SourcedValue {
            value: config.enabled,
            source,
            origin: origin.clone(),
        },
        print_width: SourcedValue {
            value: config.print_width,
            source,
            origin: origin.clone(),
        },
        indent_width: SourcedValue {
            value: config.indent_width,
            source,
            origin: origin.clone(),
        },
        use_tabs: SourcedValue {
            value: config.use_tabs,
            source,
            origin: origin.clone(),
        },
        quotes: SourcedValue {
            value: config.quotes,
            source,
            origin: origin.clone(),
        },
        script: SourcedScriptConfig {
            enabled: SourcedValue {
                value: config.script.enabled,
                source,
                origin: origin.clone(),
            },
            semi_colons: SourcedValue {
                value: config.script.semi_colons,
                source,
                origin: origin.clone(),
            },
            quote_style: SourcedValue {
                value: config.script.quote_style,
                source,
                origin: origin.clone(),
            },
        },
        format_comments_as_markdown: SourcedValue {
            value: config.format_comments_as_markdown,
            source,
            origin,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> Option<SourcedConfigFragment> {
        // Match the production call path: pyproject.toml is loaded with
        // ConfigSource::PyprojectToml (see source_from_filename).
        parse_pyproject_toml(
            content,
            "pyproject.toml",
            ConfigSource::PyprojectToml,
            ConfigOrigin::Direct,
        )
        .unwrap()
    }

    #[test]
    fn pyproject_with_only_flavor_is_kept() {
        let fragment = parse("[tool.rumdl]\nflavor = \"mkdocs\"\n")
            .expect("[tool.rumdl] with only `flavor` must not be discarded");
        assert_eq!(fragment.global.flavor.value, MarkdownFlavor::MkDocs);
        assert_ne!(fragment.global.flavor.source, ConfigSource::Default);
    }

    #[test]
    fn pyproject_with_only_respect_gitignore_is_kept() {
        let fragment = parse("[tool.rumdl]\nrespect-gitignore = false\n")
            .expect("[tool.rumdl] with only `respect-gitignore` must not be discarded");
        assert!(!fragment.global.respect_gitignore.value);
    }

    #[test]
    fn pyproject_with_only_force_exclude_is_kept() {
        let fragment = parse("[tool.rumdl]\nforce-exclude = true\n")
            .expect("[tool.rumdl] with only `force-exclude` must not be discarded");
        assert!(fragment.global.force_exclude.value);
    }

    #[test]
    fn pyproject_with_only_cache_true_is_kept() {
        // cache defaults to true, so an explicit `cache = true` must be
        // detected via its source, not by a value heuristic.
        let fragment =
            parse("[tool.rumdl]\ncache = true\n").expect("[tool.rumdl] with only `cache = true` must not be discarded");
        assert!(fragment.global.cache.value);
        assert_ne!(fragment.global.cache.source, ConfigSource::Default);
    }

    #[test]
    fn pyproject_with_no_rumdl_section_is_none() {
        assert!(parse("[tool.black]\nline-length = 88\n").is_none());
    }

    fn rule_option(content: &str, rule: &str, option: &str) -> Option<toml::Value> {
        let fragment = parse_rumdl_toml(
            content,
            ".rumdl.toml",
            ConfigSource::ProjectConfig,
            ConfigOrigin::Direct,
        )
        .unwrap();
        fragment
            .rules
            .get(rule)
            .and_then(|entry| entry.values.get(option))
            .map(|sourced| sourced.value.clone())
    }

    #[test]
    fn rumdl_toml_keeps_an_inline_table_option() {
        let value = rule_option(
            "[MD040]\npreferred-aliases = { Shell = \"sh\" }\n",
            "MD040",
            "preferred-aliases",
        )
        .expect("an inline table option must reach the fragment");
        assert_eq!(value["Shell"].as_str(), Some("sh"));
    }

    #[test]
    fn rumdl_toml_keeps_a_sub_table_option() {
        let value = rule_option(
            "[MD040]\n[MD040.preferred-aliases]\nShell = \"sh\"\n",
            "MD040",
            "preferred-aliases",
        )
        .expect("a sub-table option must reach the fragment");
        assert_eq!(value["Shell"].as_str(), Some("sh"));
    }

    #[test]
    fn rumdl_toml_keeps_map_keys_as_written() {
        // Map keys name languages, so normalizing them the way option keys are
        // normalized would rewrite the user's data.
        let value = rule_option(
            "[MD040]\npreferred-aliases = { \"Objective-C\" = \"objc\", JSON_with_comments = \"jsonc\" }\n",
            "MD040",
            "preferred-aliases",
        )
        .expect("an inline table option must reach the fragment");
        assert_eq!(value["Objective-C"].as_str(), Some("objc"));
        assert_eq!(value["JSON_with_comments"].as_str(), Some("jsonc"));
    }

    #[test]
    fn rumdl_toml_keeps_scalar_and_array_options() {
        let array = rule_option(
            "[MD040]\nallowed-languages = [\"rust\", \"python\"]\n",
            "MD040",
            "allowed-languages",
        )
        .expect("an array option must reach the fragment");
        assert_eq!(
            array.as_array().map(Vec::len),
            Some(2),
            "array elements must survive conversion"
        );

        let scalar = rule_option("[MD013]\nline-length = 120\n", "MD013", "line-length")
            .expect("a scalar option must reach the fragment");
        assert_eq!(scalar.as_integer(), Some(120));
    }

    #[test]
    fn rumdl_toml_keeps_the_type_an_option_was_written_as() {
        // A datetime stays a datetime rather than collapsing into a string, so a
        // rule reading it sees the type its config declares.
        let value = rule_option("[MD001]\nwhen = 1979-05-27T07:32:00Z\n", "MD001", "when")
            .expect("a datetime option must reach the fragment");
        assert!(
            matches!(value, toml::Value::Datetime(_)),
            "a datetime option keeps its type, got {value:?}"
        );
    }

    #[test]
    fn rumdl_toml_keeps_nested_values_inside_an_array_option() {
        let value = rule_option(
            "[MD040]\nallowed-languages = [[\"rust\"], { name = \"python\" }]\n",
            "MD040",
            "allowed-languages",
        )
        .expect("an array option must reach the fragment");
        let items = value.as_array().expect("the option is an array");
        assert_eq!(items[0].as_array().and_then(|inner| inner[0].as_str()), Some("rust"));
        assert_eq!(items[1]["name"].as_str(), Some("python"));
    }
}
