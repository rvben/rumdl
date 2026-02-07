//! Handler for the `config` command.

use colored::*;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use rumdl_config::ConfigSource;
use rumdl_config::normalize_key;

use crate::ConfigSubcommand;
use crate::cli_utils::load_config_with_cli_error_handling;
use crate::formatter;

/// Handle the config command: show or query configuration.
pub fn handle_config(
    subcmd: Option<ConfigSubcommand>,
    defaults: bool,
    no_defaults: bool,
    output: Option<String>,
    config_path: Option<&str>,
    no_config: bool,
    isolated: bool,
) {
    // Validate mutual exclusivity of --defaults and --no-defaults
    if defaults && no_defaults {
        eprintln!(
            "{}: Cannot use both --defaults and --no-defaults flags together",
            "Error".red().bold()
        );
        exit::tool_error();
    }

    // Handle config subcommands
    if let Some(ConfigSubcommand::Get { key }) = subcmd {
        handle_config_get(&key, config_path, no_config);
    } else if let Some(ConfigSubcommand::File) = subcmd {
        handle_config_file(config_path, no_config, isolated);
    } else {
        // No subcommand: display full config
        handle_config_display(defaults, no_defaults, output, config_path, no_config, isolated);
    }
}

fn handle_config_get(key: &str, config_path: Option<&str>, no_config: bool) {
    if let Some((section_part, field_part)) = key.split_once('.') {
        // 1. Load the full SourcedConfig once
        let sourced = match rumdl_config::SourcedConfig::load_with_discovery(config_path, None, no_config) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {}", "Config error".red().bold(), e);
                exit::tool_error();
            }
        };
        // 2. Convert to final Config once (config-get doesn't need validation warnings)
        let final_config: rumdl_config::Config = sourced.clone().into_validated_unchecked().into();

        let normalized_field = normalize_key(field_part);

        // Handle GLOBAL keys
        if section_part.eq_ignore_ascii_case("global") {
            let maybe_value_source: Option<(toml::Value, ConfigSource)> = match normalized_field.as_str() {
                "enable" => Some((
                    toml::Value::Array(
                        final_config
                            .global
                            .enable
                            .iter()
                            .map(|s| toml::Value::String(s.clone()))
                            .collect(),
                    ),
                    sourced.global.enable.source,
                )),
                "disable" => Some((
                    toml::Value::Array(
                        final_config
                            .global
                            .disable
                            .iter()
                            .map(|s| toml::Value::String(s.clone()))
                            .collect(),
                    ),
                    sourced.global.disable.source,
                )),
                "exclude" => Some((
                    toml::Value::Array(
                        final_config
                            .global
                            .exclude
                            .iter()
                            .map(|s| toml::Value::String(s.clone()))
                            .collect(),
                    ),
                    sourced.global.exclude.source,
                )),
                "include" => Some((
                    toml::Value::Array(
                        final_config
                            .global
                            .include
                            .iter()
                            .map(|s| toml::Value::String(s.clone()))
                            .collect(),
                    ),
                    sourced.global.include.source,
                )),
                "respect-gitignore" => Some((
                    toml::Value::Boolean(final_config.global.respect_gitignore),
                    sourced.global.respect_gitignore.source,
                )),
                "output-format" | "output_format" => {
                    if let Some(ref output_format) = final_config.global.output_format {
                        Some((
                            toml::Value::String(output_format.clone()),
                            sourced
                                .global
                                .output_format
                                .as_ref()
                                .map(|v| v.source)
                                .unwrap_or(ConfigSource::Default),
                        ))
                    } else {
                        None
                    }
                }
                "flavor" => Some((
                    toml::Value::String(format!("{:?}", final_config.global.flavor).to_lowercase()),
                    sourced.global.flavor.source,
                )),
                _ => None,
            };

            if let Some((value, source)) = maybe_value_source {
                println!(
                    "{} = {} [from {}]",
                    key,
                    formatter::format_toml_value(&value),
                    formatter::format_provenance(source)
                );
            } else {
                eprintln!("Unknown global key: {field_part}");
                exit::tool_error();
            }
        }
        // Handle RULE keys (MDxxx.field)
        else {
            let normalized_rule_name = normalize_key(section_part);

            // Try to get the value from the final config first
            let final_value: Option<&toml::Value> = final_config
                .rules
                .get(&normalized_rule_name)
                .and_then(|rule_cfg| rule_cfg.values.get(&normalized_field));

            if let Some(value) = final_value {
                let provenance = sourced
                    .rules
                    .get(&normalized_rule_name)
                    .and_then(|sc| sc.values.get(&normalized_field))
                    .map_or(ConfigSource::Default, |sv| sv.source);

                println!(
                    "{}.{} = {} [from {}]",
                    normalized_rule_name,
                    normalized_field,
                    formatter::format_toml_value(value),
                    formatter::format_provenance(provenance)
                );
            } else {
                let registry = rumdl_config::default_registry();
                if let Some(v) = registry.expected_value_for(&normalized_rule_name, &normalized_field) {
                    let value_str = formatter::format_toml_value(v);
                    println!("{normalized_rule_name}.{normalized_field} = {value_str} [from default]");
                    return;
                }
                eprintln!("Unknown config key: {normalized_rule_name}.{normalized_field}");
                exit::tool_error();
            }
        }
    } else {
        eprintln!("Key must be in the form global.key or MDxxx.key");
        exit::tool_error();
    }
}

fn handle_config_file(config_path: Option<&str>, no_config: bool, isolated: bool) {
    let sourced = load_config_with_cli_error_handling(config_path, no_config || isolated);

    if sourced.loaded_files.is_empty() {
        if no_config || isolated {
            println!("No configuration file loaded (--no-config/--isolated specified)");
        } else {
            println!("No configuration file found (using defaults)");
        }
    } else {
        // Convert relative paths to absolute paths
        for file_path in &sourced.loaded_files {
            match std::fs::canonicalize(file_path) {
                Ok(absolute_path) => {
                    println!("{}", absolute_path.display());
                }
                Err(_) => {
                    // If canonicalize fails, it might be a file that doesn't exist anymore
                    // or a relative path that can't be resolved. Just print as-is.
                    println!("{file_path}");
                }
            }
        }
    }
}

fn handle_config_display(
    defaults: bool,
    no_defaults: bool,
    output: Option<String>,
    config_path: Option<&str>,
    no_config: bool,
    isolated: bool,
) {
    let registry = rumdl_config::default_registry();
    let all_rules_reg = rumdl_lib::rules::all_rules(&rumdl_config::Config::default());
    let sourced_reg = if defaults {
        // For defaults, create a SourcedConfig that includes all rule defaults
        let mut default_sourced = rumdl_config::SourcedConfig::default();

        // Add default configurations from all rules
        for rule in &all_rules_reg {
            if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section() {
                let mut rule_config = rumdl_config::SourcedRuleConfig::default();
                for (key, value) in table {
                    rule_config.values.insert(
                        key.clone(),
                        rumdl_config::SourcedValue::new(value.clone(), rumdl_config::ConfigSource::Default),
                    );
                }
                default_sourced.rules.insert(rule_name.to_uppercase(), rule_config);
            }
        }

        default_sourced
    } else {
        load_config_with_cli_error_handling(config_path, no_config || isolated)
    };
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced_reg, registry);
    if !validation_warnings.is_empty() {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
    }

    // Decide which config to print based on --defaults and --no-defaults
    let final_sourced_to_print = sourced_reg;

    // Handle output format (toml, json, or smart output)
    match output.as_deref() {
        Some("toml") => {
            if defaults {
                // For defaults with TOML output, generate a complete default config
                let mut default_config = rumdl_config::Config::default();

                // Add all rule default configurations
                for rule in &all_rules_reg {
                    if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section() {
                        let rule_config = rumdl_config::RuleConfig {
                            severity: None,
                            values: table.into_iter().collect(),
                        };
                        default_config.rules.insert(rule_name.to_uppercase(), rule_config);
                    }
                }

                match toml::to_string_pretty(&default_config) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to TOML: {e}");
                        exit::tool_error();
                    }
                }
            } else if no_defaults {
                // For --no-defaults with TOML output, filter to non-defaults
                let filtered_sourced = filter_sourced_config_to_non_defaults(&final_sourced_to_print);
                let config_to_print: rumdl_config::Config = filtered_sourced.into_validated_unchecked().into();
                match toml::to_string_pretty(&config_to_print) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to TOML: {e}");
                        exit::tool_error();
                    }
                }
            } else {
                let config_to_print: rumdl_config::Config = final_sourced_to_print.into_validated_unchecked().into();
                match toml::to_string_pretty(&config_to_print) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to TOML: {e}");
                        exit::tool_error();
                    }
                }
            }
        }
        Some("json") => {
            if defaults {
                // For defaults with JSON output, generate a complete default config
                let mut default_config = rumdl_config::Config::default();

                // Add all rule default configurations
                for rule in &all_rules_reg {
                    if let Some((rule_name, toml::Value::Table(table))) = rule.default_config_section() {
                        let rule_config = rumdl_config::RuleConfig {
                            severity: None,
                            values: table.into_iter().collect(),
                        };
                        default_config.rules.insert(rule_name.to_uppercase(), rule_config);
                    }
                }

                match serde_json::to_string_pretty(&default_config) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to JSON: {e}");
                        exit::tool_error();
                    }
                }
            } else if no_defaults {
                // For --no-defaults with JSON output, filter to non-defaults
                let filtered_sourced = filter_sourced_config_to_non_defaults(&final_sourced_to_print);
                let config_to_print: rumdl_config::Config = filtered_sourced.into_validated_unchecked().into();
                match serde_json::to_string_pretty(&config_to_print) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to JSON: {e}");
                        exit::tool_error();
                    }
                }
            } else {
                let config_to_print: rumdl_config::Config = final_sourced_to_print.into_validated_unchecked().into();
                match serde_json::to_string_pretty(&config_to_print) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Failed to serialize config to JSON: {e}");
                        exit::tool_error();
                    }
                }
            }
        }
        _ => {
            // Otherwise, print the smart output with provenance annotations
            if no_defaults {
                formatter::print_config_with_provenance_no_defaults(&final_sourced_to_print, &all_rules_reg);
            } else {
                formatter::print_config_with_provenance(&final_sourced_to_print, &all_rules_reg);
            }
        }
    }
}

/// Filter a SourcedConfig to only include non-default values
fn filter_sourced_config_to_non_defaults(
    sourced: &rumdl_config::SourcedConfig<rumdl_config::ConfigLoaded>,
) -> rumdl_config::SourcedConfig<rumdl_config::ConfigLoaded> {
    let mut filtered = rumdl_config::SourcedConfig::default();

    // Filter global config - only include fields with non-default sources
    if sourced.global.enable.source != rumdl_config::ConfigSource::Default {
        filtered.global.enable = sourced.global.enable.clone();
    }
    if sourced.global.disable.source != rumdl_config::ConfigSource::Default {
        filtered.global.disable = sourced.global.disable.clone();
    }
    if sourced.global.exclude.source != rumdl_config::ConfigSource::Default {
        filtered.global.exclude = sourced.global.exclude.clone();
    }
    if sourced.global.include.source != rumdl_config::ConfigSource::Default {
        filtered.global.include = sourced.global.include.clone();
    }
    if sourced.global.respect_gitignore.source != rumdl_config::ConfigSource::Default {
        filtered.global.respect_gitignore = sourced.global.respect_gitignore.clone();
    }
    if sourced.global.line_length.source != rumdl_config::ConfigSource::Default {
        filtered.global.line_length = sourced.global.line_length.clone();
    }
    if sourced.global.flavor.source != rumdl_config::ConfigSource::Default {
        filtered.global.flavor = sourced.global.flavor.clone();
    }
    if sourced.global.force_exclude.source != rumdl_config::ConfigSource::Default {
        filtered.global.force_exclude = sourced.global.force_exclude.clone();
    }
    if sourced.global.cache.source != rumdl_config::ConfigSource::Default {
        filtered.global.cache = sourced.global.cache.clone();
    }
    if sourced.global.fixable.source != rumdl_config::ConfigSource::Default {
        filtered.global.fixable = sourced.global.fixable.clone();
    }
    if sourced.global.unfixable.source != rumdl_config::ConfigSource::Default {
        filtered.global.unfixable = sourced.global.unfixable.clone();
    }
    if let Some(ref output_format) = sourced.global.output_format
        && output_format.source != rumdl_config::ConfigSource::Default
    {
        filtered.global.output_format = Some(output_format.clone());
    }
    if let Some(ref cache_dir) = sourced.global.cache_dir
        && cache_dir.source != rumdl_config::ConfigSource::Default
    {
        filtered.global.cache_dir = Some(cache_dir.clone());
    }

    // Filter per-file ignores
    if sourced.per_file_ignores.source != rumdl_config::ConfigSource::Default {
        filtered.per_file_ignores = sourced.per_file_ignores.clone();
    }

    // Filter rules - only include rules with at least one non-default value
    for (rule_name, rule_cfg) in &sourced.rules {
        let mut filtered_rule = rumdl_config::SourcedRuleConfig::default();
        for (key, sv) in &rule_cfg.values {
            if sv.source != rumdl_config::ConfigSource::Default {
                filtered_rule.values.insert(key.clone(), sv.clone());
            }
        }
        if !filtered_rule.values.is_empty() {
            filtered.rules.insert(rule_name.clone(), filtered_rule);
        }
    }

    // Preserve metadata
    filtered.loaded_files = sourced.loaded_files.clone();
    filtered.unknown_keys = sourced.unknown_keys.clone();
    filtered.project_root = sourced.project_root.clone();

    filtered
}
