//! Handler for the `schema` command.

use colored::*;
use std::fs;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use crate::SchemaAction;

/// Handle the schema subcommand (print, generate, or check).
pub fn handle_schema(action: SchemaAction) {
    use schemars::schema_for;

    // Generate the schema
    let schema = schema_for!(rumdl_config::Config);

    // Post-process the schema to add additionalProperties for flattened rules
    // This allows [MD###] sections at the root level alongside [global] and [per-file-ignores]
    let mut schema_value: serde_json::Value = serde_json::to_value(&schema).unwrap_or_else(|e| {
        eprintln!("{}: Failed to convert schema to Value: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    if let Some(schema_obj) = schema_value.as_object_mut() {
        // Add additionalProperties that reference the RuleConfig definition
        // This allows any additional properties (rule names like MD013, MD007, etc.)
        // to be validated as RuleConfig objects
        schema_obj.insert(
            "additionalProperties".to_string(),
            serde_json::json!({
                "$ref": "#/$defs/RuleConfig"
            }),
        );
    }

    let schema_json = serde_json::to_string_pretty(&schema_value).unwrap_or_else(|e| {
        eprintln!("{}: Failed to serialize schema: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    match action {
        SchemaAction::Print => {
            // Print to stdout
            println!("{schema_json}");
        }
        SchemaAction::Generate => {
            // Find the schema file path (project root)
            let schema_path = get_project_schema_path();

            // Read existing schema if it exists
            let existing_schema = fs::read_to_string(&schema_path).ok();

            if existing_schema.as_ref() == Some(&schema_json) {
                println!("Schema is already up-to-date: {}", schema_path.display());
            } else {
                fs::write(&schema_path, &schema_json).unwrap_or_else(|e| {
                    eprintln!("{}: Failed to write schema file: {}", "Error".red().bold(), e);
                    exit::tool_error();
                });
                println!("Schema updated: {}", schema_path.display());
            }
        }
        SchemaAction::Check => {
            let schema_path = get_project_schema_path();
            let existing_schema = fs::read_to_string(&schema_path).unwrap_or_else(|_| {
                eprintln!("Error: Schema file not found: {}", schema_path.display());
                eprintln!("Run 'rumdl schema generate' to create it.");
                exit::tool_error();
            });

            if existing_schema != schema_json {
                eprintln!("Error: Schema is out of date: {}", schema_path.display());
                eprintln!("Run 'rumdl schema generate' to update it.");
                exit::tool_error();
            } else {
                println!("Schema is up-to-date: {}", schema_path.display());
            }
        }
    }
}

/// Get the path to the project's schema file
fn get_project_schema_path() -> std::path::PathBuf {
    // Try to find the project root by looking for Cargo.toml
    let mut current_dir = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("{}: Failed to get current directory: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return current_dir.join("rumdl.schema.json");
        }

        if !current_dir.pop() {
            // Reached filesystem root without finding Cargo.toml
            // Fall back to current directory
            return std::env::current_dir()
                .unwrap_or_else(|e| {
                    eprintln!("{}: Failed to get current directory: {}", "Error".red().bold(), e);
                    exit::tool_error();
                })
                .join("rumdl.schema.json");
        }
    }
}
