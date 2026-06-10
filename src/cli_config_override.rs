//! Parser and applier for the `--config` flag.
//!
//! Each `--config` value is either a path to a config file or an inline TOML
//! `KEY = VALUE` snippet that overrides specific options. This mirrors Ruff's
//! `--config` flag, so users can do:
//!
//! ```sh
//! rumdl check --config path/to/.rumdl.toml --config 'MD013.line_length = 20'
//! ```
//!
//! Path detection: a value pointing to an existing file is treated as a path;
//! otherwise we try to parse it as a single-line TOML table.

use std::path::{Path, PathBuf};

use clap::builder::TypedValueParser;
use rumdl_lib::config::{
    ConfigSource, MarkdownFlavor, SourcedConfig, SourcedRuleConfig, SourcedValue, default_registry,
    is_global_value_key, normalize_key,
};
use rumdl_lib::types::LineLength;
use std::str::FromStr;

/// Stable name for the variant of a `toml::Value`, used in warning messages.
fn toml_value_kind(v: &toml::Value) -> &'static str {
    match v {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Datetime(_) => "datetime",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
    }
}

/// One `--config` argument: either a path or a TOML override snippet.
#[derive(Clone, Debug)]
pub enum SingleConfigArgument {
    FilePath(PathBuf),
    InlineOverride(toml::Table),
}

/// Custom clap value parser that distinguishes paths from inline TOML.
#[derive(Clone, Debug)]
pub struct ConfigArgumentParser;

impl clap::builder::ValueParserFactory for SingleConfigArgument {
    type Parser = ConfigArgumentParser;

    fn value_parser() -> Self::Parser {
        ConfigArgumentParser
    }
}

impl TypedValueParser for ConfigArgumentParser {
    type Value = SingleConfigArgument;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let Some(value_str) = value.to_str() else {
            // Non-UTF-8 input can only be a path; accept verbatim and let the
            // downstream loader report a missing-file error if needed.
            return Ok(SingleConfigArgument::FilePath(PathBuf::from(value)));
        };

        // No `=` means the user can only have meant a path — even if it
        // doesn't exist, accept it so the downstream loader can produce the
        // existing "config file not found" error (which carries the helpful
        // `--category` hint on the `rule` subcommand).
        if !value_str.contains('=') {
            return Ok(SingleConfigArgument::FilePath(PathBuf::from(value_str)));
        }

        // Has `=`: prefer a real file (rare path with `=` in name still works)
        // before treating it as inline TOML.
        let path = Path::new(value_str);
        if path.is_file() {
            return Ok(SingleConfigArgument::FilePath(path.to_path_buf()));
        }

        let toml_error = match toml::from_str::<toml::Table>(value_str) {
            Ok(table) => return Ok(SingleConfigArgument::InlineOverride(table)),
            Err(e) => e,
        };

        let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
        if let Some(a) = arg {
            err.insert(
                clap::error::ContextKind::InvalidArg,
                clap::error::ContextValue::String(a.to_string()),
            );
        }
        err.insert(
            clap::error::ContextKind::InvalidValue,
            clap::error::ContextValue::String(value_str.to_string()),
        );

        let tip_indent = " ".repeat("  tip: ".len());
        let tip = format!(
            "A `--config` value must either be a path to a TOML configuration file\n\
             {tip_indent}or an inline TOML `KEY = VALUE` pair (e.g. `MD013.line_length = 20`)\n\n\
             Failed to parse as TOML:\n{toml_error}"
        )
        .into();

        err.insert(
            clap::error::ContextKind::Suggested,
            clap::error::ContextValue::StyledStrs(vec![tip]),
        );

        Err(err)
    }
}

/// Split a list of `--config` arguments into at most one config-file path plus
/// every inline override snippet. Errors if more than one file path is given.
pub fn split_config_args(items: &[SingleConfigArgument]) -> Result<(Option<PathBuf>, Vec<toml::Table>), String> {
    let mut path: Option<PathBuf> = None;
    let mut overrides: Vec<toml::Table> = Vec::new();
    for item in items {
        match item {
            SingleConfigArgument::FilePath(p) => {
                if let Some(existing) = &path {
                    return Err(format!(
                        "multiple --config file paths given: `{}` and `{}`. Use only one config file path.",
                        existing.display(),
                        p.display()
                    ));
                }
                path = Some(p.clone());
            }
            SingleConfigArgument::InlineOverride(t) => overrides.push(t.clone()),
        }
    }
    Ok((path, overrides))
}

/// Resolve a user-provided option key to its canonical form for the given rule.
///
/// Tries, in order: the registry's named-alias map (e.g. `enable_reflow` →
/// `reflow`), then a direct hit in the schema, then snake/kebab variants.
/// Falls back to the input as-is if nothing matches (so unknown keys still
/// flow through to the existing config validator and surface as warnings).
fn canonical_option_key(rule: &str, key: &str) -> String {
    let registry = default_registry();

    // Named aliases from the rule itself: alias -> canonical.
    if let Some(aliases) = registry.rule_aliases.get(rule)
        && let Some(canonical) = aliases.get(key)
    {
        return canonical.clone();
    }

    // Look in the rule's schema for the canonical form.
    if let Some(schema) = registry.rule_schemas.get(rule) {
        if schema.contains_key(key) {
            return key.to_string();
        }
        let kebab = key.replace('_', "-");
        if schema.contains_key(&kebab) {
            return kebab;
        }
        let snake = key.replace('-', "_");
        if schema.contains_key(&snake) {
            return snake;
        }
        let normalized = normalize_key(key);
        if schema.contains_key(&normalized) {
            return normalized;
        }
    }

    key.to_string()
}

/// Variants of an option key that could collide on the same field after
/// deserialization (kebab/snake/normalized + every named alias mapped to the
/// canonical key).
fn option_key_variants(rule: &str, canonical_opt: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    out.insert(canonical_opt.to_string());
    out.insert(canonical_opt.replace('_', "-"));
    out.insert(canonical_opt.replace('-', "_"));
    out.insert(normalize_key(canonical_opt));

    let registry = default_registry();
    if let Some(aliases) = registry.rule_aliases.get(rule) {
        for (alias, canonical) in aliases {
            if canonical == canonical_opt {
                out.insert(alias.clone());
                out.insert(alias.replace('_', "-"));
                out.insert(alias.replace('-', "_"));
                out.insert(normalize_key(alias));
            }
        }
    }
    out
}

/// Apply inline `--config '...'` overrides to a sourced config.
///
/// Each top-level entry is dispatched based on its key:
/// - `RULE.opt = value` (where `RULE` resolves to a known rule) → rule-level override
/// - `global.opt = value` (explicit `[global]` table) → global override
/// - bare `opt = value` where `opt` is a known global key → global override
/// - everything else → recorded in `unknown_keys` so the existing validator surfaces a warning
///
/// Overrides land at `ConfigSource::Cli` precedence (the highest), so they win
/// over anything loaded from config files.
pub fn apply_inline_overrides(sourced: &mut SourcedConfig, overrides: &[toml::Table]) {
    let registry = default_registry();
    for table in overrides {
        for (top_key, top_value) in table {
            apply_top_level_entry(sourced, top_key, top_value, registry);
        }
    }
}

fn apply_top_level_entry(
    sourced: &mut SourcedConfig,
    top_key: &str,
    top_value: &toml::Value,
    registry: &rumdl_lib::config::RuleRegistry,
) {
    // Explicit `[global]` table: every entry inside is a global override.
    if normalize_key(top_key) == "global" {
        if let toml::Value::Table(globals) = top_value {
            for (gk, gv) in globals {
                apply_global_override(sourced, gk, gv);
            }
        }
        return;
    }

    // Discriminate by value shape, not by key alone — `line-length` is both a
    // global option and a known alias for MD013, so a bare scalar must take
    // the global path while `[line-length] line_length = 20` (a table) means
    // the rule.
    match top_value {
        toml::Value::Table(opts) => {
            if let Some(canonical) = registry.resolve_rule_name(top_key) {
                apply_rule_override(sourced, &canonical, opts);
            } else {
                // Unknown rule section — surface via the same warning path as
                // config files.
                sourced.unknown_keys.push((format!("[{top_key}]"), String::new(), None));
            }
        }
        _ => {
            let normalized = normalize_key(top_key);
            if is_global_value_key(&normalized) {
                apply_global_override(sourced, &normalized, top_value);
            } else {
                sourced
                    .unknown_keys
                    .push(("[global]".to_string(), top_key.to_string(), None));
            }
        }
    }
}

fn apply_rule_override(sourced: &mut SourcedConfig, canonical_rule: &str, opts: &toml::Table) {
    let entry = sourced
        .rules
        .entry(canonical_rule.to_string())
        .or_insert_with(SourcedRuleConfig::default);

    for (opt_key, opt_value) in opts {
        let canonical_opt = canonical_option_key(canonical_rule, opt_key);

        // Remove any other variants of this option that might already be
        // present (e.g. `line-length` when overriding `line_length`).
        // Otherwise serde sees both keys and errors out with "duplicate
        // field" because the canonical and alias forms collide.
        let variants = option_key_variants(canonical_rule, &canonical_opt);
        entry
            .values
            .retain(|k, _| !variants.contains(k.as_str()) || k == &canonical_opt);

        let sv = entry
            .values
            .entry(canonical_opt.clone())
            .or_insert_with(|| SourcedValue::new(opt_value.clone(), ConfigSource::Default));
        sv.merge_override(opt_value.clone(), ConfigSource::Cli, None);
    }
}

/// Apply a single global config entry. Mirrors the per-key dispatch in
/// `parsers::parse_global_key`, but operates on `toml::Value` directly so we
/// don't need the toml_edit-based fragment loader for inline CLI input.
fn apply_global_override(sourced: &mut SourcedConfig, key: &str, value: &toml::Value) {
    let registry = default_registry();
    let normalized = normalize_key(key);
    let g = &mut sourced.global;

    let mismatched = |expected: &str| {
        log::warn!(
            "[--config] expected {expected} for global key '{normalized}', got {}",
            toml_value_kind(value)
        );
    };

    let resolve_rule_list = |arr: &Vec<toml::Value>| -> Vec<String> {
        arr.iter()
            .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
            .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
            .collect()
    };

    let to_strings = |arr: &Vec<toml::Value>| -> Vec<String> {
        arr.iter()
            .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
            .collect()
    };

    match normalized.as_str() {
        "enable" => {
            if let toml::Value::Array(a) = value {
                g.enable.push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "disable" => {
            if let toml::Value::Array(a) = value {
                g.disable.push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "extend-enable" => {
            if let toml::Value::Array(a) = value {
                g.extend_enable
                    .push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "extend-disable" => {
            if let toml::Value::Array(a) = value {
                g.extend_disable
                    .push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "include" => {
            if let toml::Value::Array(a) = value {
                g.include.push_override(to_strings(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "exclude" => {
            if let toml::Value::Array(a) = value {
                g.exclude.push_override(to_strings(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "fixable" => {
            if let toml::Value::Array(a) = value {
                g.fixable.push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "unfixable" => {
            if let toml::Value::Array(a) = value {
                g.unfixable.push_override(resolve_rule_list(a), ConfigSource::Cli, None);
            } else {
                mismatched("array");
            }
        }
        "respect-gitignore" => {
            if let Some(b) = value.as_bool() {
                g.respect_gitignore.push_override(b, ConfigSource::Cli, None);
            } else {
                mismatched("boolean");
            }
        }
        "force-exclude" => {
            if let Some(b) = value.as_bool() {
                g.force_exclude.push_override(b, ConfigSource::Cli, None);
            } else {
                mismatched("boolean");
            }
        }
        "cache" => {
            if let Some(b) = value.as_bool() {
                g.cache.push_override(b, ConfigSource::Cli, None);
            } else {
                mismatched("boolean");
            }
        }
        "line-length" => {
            if let Some(n) = value.as_integer() {
                g.line_length
                    .push_override(LineLength::new(n.max(0) as usize), ConfigSource::Cli, None);
            } else {
                mismatched("integer");
            }
        }
        "output-format" => {
            if let Some(s) = value.as_str() {
                let val = s.to_string();
                if let Some(sv) = g.output_format.as_mut() {
                    sv.push_override(val, ConfigSource::Cli, None);
                } else {
                    g.output_format = Some(SourcedValue::new(val, ConfigSource::Cli));
                }
            } else {
                mismatched("string");
            }
        }
        "cache-dir" => {
            if let Some(s) = value.as_str() {
                let val = s.to_string();
                if let Some(sv) = g.cache_dir.as_mut() {
                    sv.push_override(val, ConfigSource::Cli, None);
                } else {
                    g.cache_dir = Some(SourcedValue::new(val, ConfigSource::Cli));
                }
            } else {
                mismatched("string");
            }
        }
        "flavor" => {
            if let Some(s) = value.as_str() {
                if let Ok(flavor) = MarkdownFlavor::from_str(s) {
                    g.flavor.push_override(flavor, ConfigSource::Cli, None);
                } else {
                    log::warn!("[--config] unknown markdown flavor '{s}'");
                }
            } else {
                mismatched("string");
            }
        }
        _ => {
            // Unknown global key — record so the existing validator surfaces a
            // "Unknown global option" warning with did-you-mean suggestions.
            sourced
                .unknown_keys
                .push(("[global]".to_string(), key.to_string(), None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(snippet: &str) -> toml::Table {
        toml::from_str(snippet).expect("test TOML must parse")
    }

    fn applied(snippet: &str) -> SourcedConfig {
        let mut sourced = SourcedConfig::default();
        apply_inline_overrides(&mut sourced, &[parse(snippet)]);
        sourced
    }

    #[test]
    fn rule_dotted_key_lands_in_rules_with_cli_source() {
        let sourced = applied("MD013.line_length = 20");
        let rule = sourced.rules.get("MD013").expect("MD013 entry created");
        let lv = rule.values.get("line-length").expect("canonical kebab key");
        assert_eq!(lv.value.as_integer(), Some(20));
        assert_eq!(lv.source, ConfigSource::Cli);
    }

    #[test]
    fn rule_alias_resolves_to_canonical() {
        let sourced = applied("line-length.line_length = 40");
        // `line-length` is a known alias for MD013.
        assert!(sourced.rules.contains_key("MD013"));
    }

    #[test]
    fn bare_global_key_lands_in_global_not_rules() {
        let sourced = applied("line-length = 100");
        assert_eq!(sourced.global.line_length.value.get(), 100);
        assert_eq!(sourced.global.line_length.source, ConfigSource::Cli);
        assert!(
            !sourced.rules.contains_key("MD013"),
            "bare line-length should NOT create an MD013 entry"
        );
    }

    #[test]
    fn explicit_global_table_routes_to_global() {
        let sourced = applied("global.line-length = 50");
        assert_eq!(sourced.global.line_length.value.get(), 50);
    }

    #[test]
    fn array_for_disable_resolves_aliases() {
        let sourced = applied(r#"disable = ["line-length", "MD003"]"#);
        // disable resolves rule aliases so "line-length" -> "MD013".
        let v = &sourced.global.disable.value;
        assert!(v.contains(&"MD013".to_string()));
        assert!(v.contains(&"MD003".to_string()));
    }

    #[test]
    fn type_mismatch_for_global_is_silent_no_panic() {
        // String for line-length should NOT panic and NOT corrupt the value.
        let sourced = applied(r#"line-length = "huge""#);
        // Default remains unchanged.
        assert_eq!(sourced.global.line_length.source, ConfigSource::Default);
    }

    #[test]
    fn unknown_top_level_key_records_unknown() {
        let sourced = applied("definitely_not_a_setting = 1");
        let entry = sourced
            .unknown_keys
            .iter()
            .find(|(s, k, _)| s == "[global]" && k == "definitely_not_a_setting");
        assert!(entry.is_some(), "unknown top-level key should be recorded");
    }

    #[test]
    fn unknown_rule_id_records_unknown_section() {
        let sourced = applied("MD9999.foo = 1");
        let entry = sourced
            .unknown_keys
            .iter()
            .find(|(s, k, _)| s == "[MD9999]" && k.is_empty());
        assert!(entry.is_some(), "unknown rule should be recorded as unknown section");
    }

    #[test]
    fn cli_overrides_beat_lower_precedence_sources() {
        let mut sourced = SourcedConfig::default();
        // Simulate a value loaded from a project config file.
        sourced
            .global
            .line_length
            .merge_override(LineLength::new(80), ConfigSource::ProjectConfig, None);
        apply_inline_overrides(&mut sourced, &[parse("line-length = 200")]);
        assert_eq!(sourced.global.line_length.value.get(), 200);
        assert_eq!(sourced.global.line_length.source, ConfigSource::Cli);
    }

    #[test]
    fn collision_kebab_and_snake_does_not_duplicate() {
        // Pre-seed a rule entry with the kebab form (as a config file would).
        let mut sourced = SourcedConfig::default();
        sourced.rules.entry("MD013".to_string()).or_default().values.insert(
            "line-length".to_string(),
            SourcedValue::new(toml::Value::Integer(80), ConfigSource::ProjectConfig),
        );
        // Inline override using the snake form must REPLACE, not duplicate.
        apply_inline_overrides(&mut sourced, &[parse("MD013.line_length = 20")]);
        let rule = sourced.rules.get("MD013").unwrap();
        assert_eq!(
            rule.values.len(),
            1,
            "kebab/snake variants must collapse to one key, got: {:?}",
            rule.values.keys().collect::<Vec<_>>()
        );
        assert_eq!(rule.values["line-length"].value.as_integer(), Some(20));
    }

    #[test]
    fn split_rejects_two_file_paths() {
        let args = vec![
            SingleConfigArgument::FilePath(PathBuf::from("a.toml")),
            SingleConfigArgument::FilePath(PathBuf::from("b.toml")),
        ];
        assert!(split_config_args(&args).is_err());
    }

    #[test]
    fn split_accepts_one_path_plus_overrides() {
        let args = vec![
            SingleConfigArgument::FilePath(PathBuf::from("a.toml")),
            SingleConfigArgument::InlineOverride(parse("MD013.line_length = 20")),
            SingleConfigArgument::InlineOverride(parse("line-length = 200")),
        ];
        let (path, overrides) = split_config_args(&args).unwrap();
        assert_eq!(path, Some(PathBuf::from("a.toml")));
        assert_eq!(overrides.len(), 2);
    }
}
