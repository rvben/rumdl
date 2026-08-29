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

    warnings.extend(validate_code_block_tools(&sourced.code_block_tools.value));

    warnings
}

/// Warnings for `[code-block-tools.languages.*]` tool ids that name nothing rumdl can
/// run, or a tool the slot asks for something it cannot do.
///
/// A tool id that resolves to nothing is otherwise reported only by a `log::warn!`,
/// invisible at default verbosity: the run skips the tool, finds no issues and exits 0,
/// which is indistinguishable from the tool having run and been happy. That silence is
/// what makes a typo here cost an afternoon rather than a second.
///
/// Runs whether or not `enabled` is set, so a typo is caught before the switch is
/// flipped; a config with no `languages` section produces nothing either way.
fn validate_code_block_tools(config: &crate::code_block_tools::CodeBlockToolsConfig) -> Vec<ConfigValidationWarning> {
    use crate::code_block_tools::{RUMDL_BUILTIN_TOOL, ToolRegistry, ToolSlot};

    let mut warnings = Vec::new();
    if config.languages.is_empty() {
        return warnings;
    }

    let registry = ToolRegistry::new(config.tools.clone());
    // Suggestions come from the registry itself, so a tool added to it is suggestible
    // without a second list to keep in step.
    let known_tools: Vec<String> = registry.list_tools().into_iter().map(str::to_string).collect();

    for (lang, lang_config) in &config.languages {
        for (slot, slot_name, tool_ids) in [
            (ToolSlot::Lint, "lint", &lang_config.lint),
            (ToolSlot::Format, "format", &lang_config.format),
        ] {
            for tool_id in tool_ids {
                // rumdl's own markdown linting, short-circuited before tool resolution.
                if tool_id == RUMDL_BUILTIN_TOOL {
                    continue;
                }

                // A tool id and a language key are both text out of whichever file
                // supplied the section, so a section reached through `extends` is
                // described rather than quoted - the suggestion too, which would
                // otherwise say how close the withheld text came to a real id.
                let message = if registry.resolve_id(tool_id, slot).is_none() {
                    if config.values_withheld {
                        let withheld = crate::config::WITHHELD;
                        format!("Unknown tool in code-block-tools.languages.{withheld}.{slot_name}: {withheld}")
                    } else if let Some(suggestion) = suggest_similar_key(tool_id, &known_tools) {
                        format!(
                            "Unknown tool in code-block-tools.languages.{lang}.{slot_name}: {tool_id} (did you mean: {suggestion}?)"
                        )
                    } else {
                        format!("Unknown tool in code-block-tools.languages.{lang}.{slot_name}: {tool_id}")
                    }
                } else if slot == ToolSlot::Format && registry.fills_format_slot(tool_id) == Some(false) {
                    // A linter in a format slot writes diagnostics where the formatted
                    // code should go, so rumdl declines the output and the block is
                    // never formatted.
                    if config.values_withheld {
                        let withheld = crate::config::WITHHELD;
                        format!("Tool in code-block-tools.languages.{withheld}.format cannot format: {withheld}")
                    } else {
                        format!(
                            "Tool in code-block-tools.languages.{lang}.format cannot format: {tool_id} is a linter (move it to lint)"
                        )
                    }
                } else {
                    continue;
                };

                warnings.push(ConfigValidationWarning {
                    message,
                    rule: None,
                    key: None,
                });
            }
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
            return normalize_for_display(relative.to_string_lossy().to_string());
        }

        // Fall back to non-canonicalized comparison
        if let Ok(relative) = file_path.strip_prefix(&cwd) {
            return normalize_for_display(relative.to_string_lossy().to_string());
        }
    }

    // Return original if we can't make it relative
    normalize_for_display(path.to_string())
}

/// Normalize a path for output: `/` separators on every platform, and no Win32
/// verbatim prefix.
///
/// Only the platform's native separator is converted: on Windows `\` becomes `/`.
/// On Unix this is a no-op, where `\` is a legal filename character that must be
/// preserved. A config path resolved through `canonicalize` also sheds the `\\?\`
/// prefix that form carries, the same way the CLI displays linted files.
fn normalize_for_display(path: String) -> String {
    if cfg!(windows) {
        windows_display_path(&path)
    } else {
        path
    }
}

/// The Windows half of [`normalize_for_display`]: pure string logic, so it is
/// tested on every platform.
pub(super) fn windows_display_path(path: &str) -> String {
    crate::discovery::strip_verbatim_prefix(path).replace('\\', "/")
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

#[cfg(test)]
mod code_block_tool_tests {
    use crate::code_block_tools::{CodeBlockToolsConfig, LanguageToolConfig, ToolDefinition};

    fn config_with(lang: &str, lint: &[&str], format: &[&str]) -> CodeBlockToolsConfig {
        let mut config = CodeBlockToolsConfig {
            enabled: true,
            ..Default::default()
        };
        config.languages.insert(
            lang.to_string(),
            LanguageToolConfig {
                lint: lint.iter().map(|s| (*s).to_string()).collect(),
                format: format.iter().map(|s| (*s).to_string()).collect(),
                ..Default::default()
            },
        );
        config
    }

    fn messages(config: &CodeBlockToolsConfig) -> Vec<String> {
        super::validate_code_block_tools(config)
            .into_iter()
            .map(|w| w.message)
            .collect()
    }

    #[test]
    fn an_unknown_tool_id_is_reported_with_a_suggestion() {
        let messages = messages(&config_with("python", &[], &["blackk"]));
        assert_eq!(
            messages,
            vec!["Unknown tool in code-block-tools.languages.python.format: blackk (did you mean: black?)"]
        );
    }

    #[test]
    fn a_resolvable_tool_id_is_not_reported() {
        // The control for the test above: same shape, one letter apart, silent.
        assert!(messages(&config_with("python", &["ruff:check"], &["black"])).is_empty());
    }

    #[test]
    fn a_bare_name_resolving_through_a_variant_is_not_reported() {
        // `terraform` is registered as `terraform:format`, and a lint slot answers
        // through the same entry by comparing the formatter's output.
        assert!(messages(&config_with("terraform", &["terraform"], &["terraform"])).is_empty());
    }

    #[test]
    fn a_linter_in_a_format_slot_is_reported() {
        let messages = messages(&config_with("python", &[], &["ruff:check"]));
        assert_eq!(
            messages,
            vec![
                "Tool in code-block-tools.languages.python.format cannot format: ruff:check is a linter (move it to lint)"
            ]
        );
    }

    #[test]
    fn a_linter_in_a_lint_slot_is_not_reported() {
        assert!(messages(&config_with("python", &["ruff:check"], &[])).is_empty());
    }

    #[test]
    fn a_user_tool_shadowing_a_builtin_linter_may_format() {
        // The user wrote this command, so rumdl has no opinion about what it does -
        // and must not answer from the built-in `ruff:check` it shadows.
        let mut config = config_with("python", &[], &["ruff:check"]);
        config.tools.insert(
            "ruff:check".to_string(),
            ToolDefinition {
                command: vec!["my-formatter".to_string(), "-".to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );
        assert!(messages(&config).is_empty());
    }

    #[test]
    fn a_user_tool_is_a_suggestion_candidate() {
        let mut config = config_with("python", &[], &["my-formater"]);
        config.tools.insert(
            "my-formatter".to_string(),
            ToolDefinition {
                command: vec!["my-formatter".to_string(), "-".to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );
        assert_eq!(
            messages(&config),
            vec!["Unknown tool in code-block-tools.languages.python.format: my-formater (did you mean: my-formatter?)"]
        );
    }

    #[test]
    fn rumdls_own_markdown_linting_is_not_an_unknown_tool() {
        assert!(messages(&config_with("markdown", &["rumdl"], &["rumdl"])).is_empty());
    }

    #[test]
    fn a_withheld_section_names_neither_the_tool_nor_the_language() {
        let mut config = config_with("python", &["blackk"], &["ruff:check"]);
        config.values_withheld = true;
        let messages = messages(&config);
        assert_eq!(
            messages,
            vec![
                "Unknown tool in code-block-tools.languages.<withheld>.lint: <withheld>",
                "Tool in code-block-tools.languages.<withheld>.format cannot format: <withheld>",
            ]
        );
        // A suggestion would say how close the withheld text came to a real id.
        for message in &messages {
            assert!(!message.contains("black"), "withheld text is inferable from: {message}");
            assert!(!message.contains("ruff"), "withheld text is inferable from: {message}");
            assert!(
                !message.contains("python"),
                "withheld text is inferable from: {message}"
            );
        }
    }
}
