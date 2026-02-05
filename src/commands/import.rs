//! Handler for the `import` command.

use colored::*;
use std::fs;
use std::path::Path;

use rumdl_lib::exit_codes::exit;

/// Handle the import command: convert markdownlint config to rumdl format.
pub fn handle_import(file: String, output: Option<String>, format: String, dry_run: bool) {
    use rumdl_lib::markdownlint_config;

    // Load the markdownlint config file
    let ml_config = match markdownlint_config::load_markdownlint_config(&file) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}: {}", "Import error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Convert to rumdl config format
    let fragment = ml_config.map_to_sourced_rumdl_config_fragment(Some(&file));

    // Determine if we're outputting to pyproject.toml
    let is_pyproject = output
        .as_ref()
        .is_some_and(|p| p.ends_with("pyproject.toml") || p == "pyproject.toml");

    // Generate the output
    let output_content = match format.as_str() {
        "toml" => generate_toml_output(&fragment, is_pyproject),
        "json" => generate_json_output(&fragment),
        _ => {
            eprintln!(
                "{}: Unsupported format '{}'. Use 'toml' or 'json'.",
                "Error".red().bold(),
                format
            );
            exit::tool_error();
        }
    };

    if dry_run {
        // Just print the converted config
        println!("{output_content}");
    } else {
        // Write to output file
        let output_path = output.as_deref().unwrap_or(if format == "json" {
            "rumdl-config.json"
        } else {
            ".rumdl.toml"
        });

        if Path::new(output_path).exists() {
            eprintln!("{}: Output file '{}' already exists", "Error".red().bold(), output_path);
            exit::tool_error();
        }

        match fs::write(output_path, output_content) {
            Ok(()) => {
                println!("Converted markdownlint config from '{file}' to '{output_path}'");
                println!("You can now use: rumdl check --config {output_path} .");
            }
            Err(e) => {
                eprintln!("{}: Failed to write to '{}': {}", "Error".red().bold(), output_path, e);
                exit::tool_error();
            }
        }
    }
}

fn generate_toml_output(fragment: &rumdl_lib::config::SourcedConfigFragment, is_pyproject: bool) -> String {
    let mut output = String::new();

    // For pyproject.toml, wrap everything in [tool.rumdl]
    let section_prefix = if is_pyproject { "tool.rumdl." } else { "" };

    // Add global settings if any
    if !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || fragment.global.line_length.value.get() != 80
    {
        output.push_str(&format!("[{section_prefix}global]\n"));
        if !fragment.global.enable.value.is_empty() {
            output.push_str(&format!("enable = {:?}\n", fragment.global.enable.value));
        }
        if !fragment.global.disable.value.is_empty() {
            output.push_str(&format!("disable = {:?}\n", fragment.global.disable.value));
        }
        if !fragment.global.exclude.value.is_empty() {
            output.push_str(&format!("exclude = {:?}\n", fragment.global.exclude.value));
        }
        if !fragment.global.include.value.is_empty() {
            output.push_str(&format!("include = {:?}\n", fragment.global.include.value));
        }
        if fragment.global.line_length.value.get() != 80 {
            output.push_str(&format!("line_length = {}\n", fragment.global.line_length.value.get()));
        }
        output.push('\n');
    }

    // Add rule-specific settings
    for (rule_name, rule_config) in &fragment.rules {
        if !rule_config.values.is_empty() {
            output.push_str(&format!("[{section_prefix}{rule_name}]\n"));
            for (key, sourced_value) in &rule_config.values {
                // Skip the generic "value" key if we have more specific keys
                if key == "value" && rule_config.values.len() > 1 {
                    continue;
                }

                format_toml_value_line(&mut output, key, &sourced_value.value);
            }
            output.push('\n');
        }
    }
    output
}

fn format_toml_value_line(output: &mut String, key: &str, value: &toml::Value) {
    match value {
        toml::Value::String(s) => output.push_str(&format!("{key} = \"{s}\"\n")),
        toml::Value::Integer(i) => output.push_str(&format!("{key} = {i}\n")),
        toml::Value::Float(f) => output.push_str(&format!("{key} = {f}\n")),
        toml::Value::Boolean(b) => output.push_str(&format!("{key} = {b}\n")),
        toml::Value::Array(arr) => {
            // Format arrays properly for TOML
            let arr_str = arr
                .iter()
                .map(|v| match v {
                    toml::Value::String(s) => format!("\"{s}\""),
                    _ => format!("{v}"),
                })
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!("{key} = [{arr_str}]\n"));
        }
        _ => {
            // Use proper TOML serialization for complex values
            if let Ok(toml_str) = toml::to_string_pretty(value) {
                let clean_value = toml_str.trim();
                if !clean_value.starts_with('[') {
                    output.push_str(&format!("{key} = {clean_value}"));
                } else {
                    output.push_str(&format!("{key} = {value:?}\n"));
                }
            } else {
                output.push_str(&format!("{key} = {value:?}\n"));
            }
        }
    }
}

fn generate_json_output(fragment: &rumdl_lib::config::SourcedConfigFragment) -> String {
    let mut json_config = serde_json::Map::new();

    // Add global settings
    if !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || fragment.global.line_length.value.get() != 80
    {
        let mut global = serde_json::Map::new();
        if !fragment.global.enable.value.is_empty() {
            global.insert(
                "enable".to_string(),
                serde_json::Value::Array(
                    fragment
                        .global
                        .enable
                        .value
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if !fragment.global.disable.value.is_empty() {
            global.insert(
                "disable".to_string(),
                serde_json::Value::Array(
                    fragment
                        .global
                        .disable
                        .value
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if !fragment.global.exclude.value.is_empty() {
            global.insert(
                "exclude".to_string(),
                serde_json::Value::Array(
                    fragment
                        .global
                        .exclude
                        .value
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if !fragment.global.include.value.is_empty() {
            global.insert(
                "include".to_string(),
                serde_json::Value::Array(
                    fragment
                        .global
                        .include
                        .value
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if fragment.global.line_length.value.get() != 80 {
            global.insert(
                "line_length".to_string(),
                serde_json::Value::Number(serde_json::Number::from(fragment.global.line_length.value.get())),
            );
        }
        json_config.insert("global".to_string(), serde_json::Value::Object(global));
    }

    // Add rule-specific settings
    for (rule_name, rule_config) in &fragment.rules {
        if !rule_config.values.is_empty() {
            let mut rule_obj = serde_json::Map::new();
            for (key, sourced_value) in &rule_config.values {
                if let Ok(json_value) = serde_json::to_value(&sourced_value.value) {
                    rule_obj.insert(key.clone(), json_value);
                }
            }
            json_config.insert(rule_name.clone(), serde_json::Value::Object(rule_obj));
        }
    }

    serde_json::to_string_pretty(&json_config).unwrap_or_else(|e| {
        eprintln!("{}: Failed to serialize to JSON: {}", "Error".red().bold(), e);
        exit::tool_error();
    })
}
