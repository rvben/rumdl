use super::flavor::{ConfigLoaded, ConfigValidated};
use super::registry::{RULE_ALIAS_MAP, RuleRegistry, is_valid_rule_name, resolve_rule_name_alias};
use super::source_tracking::{ConfigValidationWarning, SourcedConfig, SourcedRuleConfig};
use std::collections::BTreeMap;
use std::path::Path;

/// Validates rule names from CLI flags against the known rule set.
/// Returns warnings for unknown rules with "did you mean" suggestions.
///
/// This provides consistent validation between config files and CLI flags.
/// Unknown rules are warned about but don't cause failures.
pub fn validate_cli_rule_names(
    enable: Option<&str>,
    disable: Option<&str>,
    extend_enable: Option<&str>,
    extend_disable: Option<&str>,
    fixable: Option<&str>,
    unfixable: Option<&str>,
) -> Vec<ConfigValidationWarning> {
    let mut warnings = Vec::new();
    let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(std::string::ToString::to_string).collect();

    let validate_list = |input: &str, flag_name: &str, warnings: &mut Vec<ConfigValidationWarning>| {
        for name in input.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            // Check for special "all" value (case-insensitive)
            if name.eq_ignore_ascii_case("all") {
                continue;
            }
            if resolve_rule_name_alias(name).is_none() {
                let message = if let Some(suggestion) = suggest_similar_key(name, &all_rule_names) {
                    let formatted = if suggestion.starts_with("MD") {
                        suggestion
                    } else {
                        suggestion.to_lowercase()
                    };
                    format!("Unknown rule in {flag_name}: {name} (did you mean: {formatted}?)")
                } else {
                    format!("Unknown rule in {flag_name}: {name}")
                };
                warnings.push(ConfigValidationWarning {
                    message,
                    rule: Some(name.to_string()),
                    key: None,
                });
            }
        }
    };

    if let Some(e) = enable {
        validate_list(e, "--enable", &mut warnings);
    }
    if let Some(d) = disable {
        validate_list(d, "--disable", &mut warnings);
    }
    if let Some(ee) = extend_enable {
        validate_list(ee, "--extend-enable", &mut warnings);
    }
    if let Some(ed) = extend_disable {
        validate_list(ed, "--extend-disable", &mut warnings);
    }
    if let Some(f) = fixable {
        validate_list(f, "--fixable", &mut warnings);
    }
    if let Some(u) = unfixable {
        validate_list(u, "--unfixable", &mut warnings);
    }

    warnings
}

/// Internal validation function that works with any SourcedConfig state.
/// This is used by both the public `validate_config_sourced` and the typestate `validate()` method.
pub(super) fn validate_config_sourced_internal<S>(
    sourced: &SourcedConfig<S>,
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    let mut warnings = validate_config_sourced_impl(&sourced.rules, &sourced.unknown_keys, registry);

    // Validate enable/disable arrays in [global] section
    let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(std::string::ToString::to_string).collect();

    for rule_name in &sourced.global.enable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.enable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.enable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    for rule_name in &sourced.global.disable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.disable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.disable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    for rule_name in &sourced.global.extend_enable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.extend-enable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.extend-enable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    for rule_name in &sourced.global.extend_disable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.extend-disable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.extend-disable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    for rule_name in &sourced.global.fixable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.fixable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.fixable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    for rule_name in &sourced.global.unfixable.value {
        if !is_valid_rule_name(rule_name) {
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                let formatted = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in global.unfixable: {rule_name} (did you mean: {formatted}?)")
            } else {
                format!("Unknown rule in global.unfixable: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.clone()),
                key: None,
            });
        }
    }

    warnings
}

/// Core validation implementation that doesn't depend on SourcedConfig type parameter.
fn validate_config_sourced_impl(
    rules: &BTreeMap<String, SourcedRuleConfig>,
    unknown_keys: &[(String, String, Option<String>)],
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    let mut warnings = Vec::new();
    let known_rules = registry.rule_names();
    // 1. Unknown rules
    for rule in rules.keys() {
        if !known_rules.contains(rule) {
            // Include both canonical names AND aliases for fuzzy matching
            let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(std::string::ToString::to_string).collect();
            let message = if let Some(suggestion) = suggest_similar_key(rule, &all_rule_names) {
                // Convert alias suggestions to lowercase for better UX (MD001 stays uppercase, ul-style becomes lowercase)
                let formatted_suggestion = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in config: {rule} (did you mean: {formatted_suggestion}?)")
            } else {
                format!("Unknown rule in config: {rule}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule.clone()),
                key: None,
            });
        }
    }
    // 2. Unknown options and type mismatches
    for (rule, rule_cfg) in rules {
        if let Some(valid_keys) = registry.config_keys_for(rule) {
            for key in rule_cfg.values.keys() {
                if !valid_keys.contains(key) {
                    let valid_keys_vec: Vec<String> = valid_keys.iter().cloned().collect();
                    let message = if let Some(suggestion) = suggest_similar_key(key, &valid_keys_vec) {
                        format!("Unknown option for rule {rule}: {key} (did you mean: {suggestion}?)")
                    } else {
                        format!("Unknown option for rule {rule}: {key}")
                    };
                    warnings.push(ConfigValidationWarning {
                        message,
                        rule: Some(rule.clone()),
                        key: Some(key.clone()),
                    });
                } else {
                    // Type check: compare type of value to type of default
                    if let Some(expected) = registry.expected_value_for(rule, key) {
                        let actual = &rule_cfg.values[key].value;
                        if !toml_value_type_matches(expected, actual) {
                            warnings.push(ConfigValidationWarning {
                                message: format!(
                                    "Type mismatch for {}.{}: expected {}, got {}",
                                    rule,
                                    key,
                                    toml_type_name(expected),
                                    toml_type_name(actual)
                                ),
                                rule: Some(rule.clone()),
                                key: Some(key.clone()),
                            });
                        }
                    }
                }
            }
        }
    }
    // 3. Unknown global options (from unknown_keys). Suggestions come from the
    // dispatch table itself, so a newly added global key is suggestible without a
    // second list to keep in step, plus the keys holding a table or a path rather
    // than a plain value.
    let known_global_keys: Vec<String> = super::global_keys::GLOBAL_VALUE_KEYS
        .iter()
        .map(|k| (*k).to_string())
        .chain(
            ["per-file-ignores", "per-file-flavor", "extends"]
                .into_iter()
                .map(str::to_string),
        )
        .collect();

    for (section, key, display_name) in unknown_keys {
        // Already display-ready: the parser decided how this file may be named.
        let display_path = display_name.as_ref();

        if section.contains("[global]") || section.contains("[tool.rumdl]") {
            let message = if let Some(suggestion) = suggest_similar_key(key, &known_global_keys) {
                if let Some(path) = display_path {
                    format!("Unknown global option in {path}: {key} (did you mean: {suggestion}?)")
                } else {
                    format!("Unknown global option: {key} (did you mean: {suggestion}?)")
                }
            } else if let Some(path) = display_path {
                format!("Unknown global option in {path}: {key}")
            } else {
                format!("Unknown global option: {key}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: None,
                key: Some(key.clone()),
            });
        } else if !key.is_empty() {
            // An option of a rule rumdl knows, recorded here instead of in the
            // config map because it came from a file whose text may not be
            // shown. Naming that file is what makes the warning actionable.
            let rule_name = section.trim_matches(|c| c == '[' || c == ']');
            let message = if let Some(path) = display_path {
                format!("Unknown option for rule {rule_name} in {path}: {key}")
            } else {
                format!("Unknown option for rule {rule_name}: {key}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule_name.to_string()),
                key: Some(key.clone()),
            });
        } else {
            // Unknown rule section - suggest similar rule names
            let rule_name = section.trim_matches(|c| c == '[' || c == ']');
            let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(std::string::ToString::to_string).collect();
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                // Convert alias suggestions to lowercase for better UX (MD001 stays uppercase, ul-style becomes lowercase)
                let formatted_suggestion = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                if let Some(path) = display_path {
                    format!("Unknown rule in {path}: {rule_name} (did you mean: {formatted_suggestion}?)")
                } else {
                    format!("Unknown rule in config: {rule_name} (did you mean: {formatted_suggestion}?)")
                }
            } else if let Some(path) = display_path {
                format!("Unknown rule in {path}: {rule_name}")
            } else {
                format!("Unknown rule in config: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: None,
                key: None,
            });
        }
    }
    warnings
}

/// Convert a file path to a display-friendly relative path.
///
/// Tries to make the path relative to the current working directory.
/// If that fails, returns the original path unchanged. The result uses `/`
/// separators for consistent output across platforms.
pub(super) fn to_relative_display_path(path: &str) -> String {
    let file_path = Path::new(path);

    // Try to make relative to CWD
    if let Ok(cwd) = std::env::current_dir() {
        // Try with canonicalized paths first (handles symlinks)
        if let (Ok(canonical_file), Ok(canonical_cwd)) = (file_path.canonicalize(), cwd.canonicalize())
            && let Ok(relative) = canonical_file.strip_prefix(&canonical_cwd)
        {
            return normalize_separators(relative.to_string_lossy().to_string());
        }

        // Fall back to non-canonicalized comparison
        if let Ok(relative) = file_path.strip_prefix(&cwd) {
            return normalize_separators(relative.to_string_lossy().to_string());
        }
    }

    // Return original if we can't make it relative
    normalize_separators(path.to_string())
}

/// Normalize path separators to `/` for consistent cross-platform output.
///
/// Only the platform's native separator is converted: on Windows `\` becomes `/`.
/// On Unix this is a no-op, where `\` is a legal filename character that must be
/// preserved.
fn normalize_separators(path: String) -> String {
    if cfg!(windows) { path.replace('\\', "/") } else { path }
}

/// Validate a loaded config against the rule registry, using SourcedConfig for unknown key tracking.
///
/// This is the legacy API that works with `SourcedConfig<ConfigLoaded>`.
/// For new code, prefer using `sourced.validate(&registry)` which returns a
/// `SourcedConfig<ConfigValidated>` that can be converted to `Config`.
pub fn validate_config_sourced(
    sourced: &SourcedConfig<ConfigLoaded>,
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    validate_config_sourced_internal(sourced, registry)
}

/// Validate a config that has already been validated (no-op, returns stored warnings).
///
/// This exists for API consistency - validated configs already have their warnings stored.
pub fn validate_config_sourced_validated(
    sourced: &SourcedConfig<ConfigValidated>,
    _registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    sourced.validation_warnings.clone()
}

fn toml_type_name(val: &toml::Value) -> &'static str {
    match val {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
        toml::Value::Datetime(_) => "datetime",
    }
}

/// Calculate Levenshtein distance between two strings (simple implementation)
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row = vec![0; len2 + 1];

    for i in 1..=len1 {
        curr_row[0] = i;
        for j in 1..=len2 {
            let cost = usize::from(s1_chars[i - 1] != s2_chars[j - 1]);
            curr_row[j] = (prev_row[j] + 1)          // deletion
                .min(curr_row[j - 1] + 1)            // insertion
                .min(prev_row[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

/// Suggest a similar key from a list of valid keys using fuzzy matching
///
/// Several keys are routinely the same distance from a typo, so the closest one
/// alone does not name a single answer. Ties go to the smaller key, which makes
/// the suggestion depend on the key set rather than on the order the caller
/// happens to hold it in.
pub fn suggest_similar_key(unknown: &str, valid_keys: &[String]) -> Option<String> {
    let unknown_lower = unknown.to_lowercase();
    let max_distance = 2.max(unknown.len() / 3); // Allow up to 2 edits or 30% of string length

    let mut best_match: Option<(&String, usize)> = None;

    for valid in valid_keys {
        let valid_lower = valid.to_lowercase();
        let distance = levenshtein_distance(&unknown_lower, &valid_lower);

        if distance > max_distance {
            continue;
        }
        let is_better = match &best_match {
            Some((best_key, best_dist)) => distance < *best_dist || (distance == *best_dist && valid < *best_key),
            None => true,
        };
        if is_better {
            best_match = Some((valid, distance));
        }
    }

    best_match.map(|(key, _)| key.clone())
}

fn toml_value_type_matches(expected: &toml::Value, actual: &toml::Value) -> bool {
    use toml::Value::{Array, Boolean, Datetime, Float, Integer, String, Table};
    match (expected, actual) {
        (String(_), String(_)) => true,
        (Integer(_), Integer(_)) => true,
        (Float(_), Float(_)) => true,
        (Boolean(_), Boolean(_)) => true,
        (Array(_), Array(_)) => true,
        (Table(_), Table(_)) => true,
        (Datetime(_), Datetime(_)) => true,
        // Allow integer for float
        (Float(_), Integer(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod suggestion_tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_closer_key_wins_over_an_earlier_one() {
        let candidates = keys(&["MD049", "MD013"]);
        assert_eq!(suggest_similar_key("MD01", &candidates), Some("MD013".to_string()));
    }

    #[test]
    fn equally_close_keys_resolve_to_the_smaller_name() {
        // Both are two substitutions away from MD999, so only the tie-break
        // decides which one the user is shown.
        for order in [["MD049", "MD009"], ["MD009", "MD049"]] {
            assert_eq!(
                suggest_similar_key("MD999", &keys(&order)),
                Some("MD009".to_string()),
                "suggestion changed with the caller's key order: {order:?}"
            );
        }
    }

    #[test]
    fn a_key_beyond_the_edit_budget_is_no_suggestion() {
        assert_eq!(suggest_similar_key("MD999", &keys(&["line-length"])), None);
    }
}
