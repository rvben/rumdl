//! Generate JSON Schema for rumdl configuration files.
//!
//! This binary generates the JSON Schema for `rumdl.toml` configuration files
//! to provide editor support with autocomplete, validation, and inline documentation.
//!
//! Usage:
//!   cargo run --bin generate_schema           # Print schema to stdout (dry run)
//!   cargo run --bin generate_schema -- --write  # Update rumdl.schema.json
//!   cargo run --bin generate_schema -- --check  # Verify schema is up-to-date

use rumdl_lib::config::Config;
use schemars::schema_for;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

#[derive(Debug, PartialEq)]
enum Mode {
    /// Print schema to stdout without writing
    DryRun,
    /// Write schema to file if changed
    Write,
    /// Check if schema is up-to-date (fail if not)
    Check,
}

impl Mode {
    fn from_args() -> Self {
        let args: Vec<String> = env::args().collect();
        if args.len() > 1 {
            match args[1].as_str() {
                "--write" | "-w" => Mode::Write,
                "--check" | "-c" => Mode::Check,
                "--dry-run" | "-d" => Mode::DryRun,
                _ => {
                    eprintln!("Usage: {} [--write|--check|--dry-run]", args[0]);
                    eprintln!("  --write, -w    Update rumdl.schema.json");
                    eprintln!("  --check, -c    Verify schema is up-to-date");
                    eprintln!("  --dry-run, -d  Print schema to stdout (default)");
                    process::exit(1);
                }
            }
        } else {
            Mode::DryRun
        }
    }
}

fn main() {
    let mode = Mode::from_args();

    // Generate the schema
    let schema = schema_for!(Config);
    let schema_json = serde_json::to_string_pretty(&schema).expect("Failed to serialize schema");

    // Get the schema file path (project root)
    let schema_path = get_schema_path();

    match mode {
        Mode::DryRun => {
            println!("{}", schema_json);
        }
        Mode::Write => {
            // Read existing schema if it exists
            let existing_schema = fs::read_to_string(&schema_path).ok();

            if existing_schema.as_ref() == Some(&schema_json) {
                println!("Schema is already up-to-date: {}", schema_path.display());
            } else {
                fs::write(&schema_path, &schema_json).expect("Failed to write schema file");
                println!("Schema updated: {}", schema_path.display());
            }
        }
        Mode::Check => {
            let existing_schema = fs::read_to_string(&schema_path).unwrap_or_else(|_| {
                eprintln!("Error: Schema file not found: {}", schema_path.display());
                eprintln!("Run with --write to generate it.");
                process::exit(1);
            });

            if existing_schema != schema_json {
                eprintln!("Error: Schema is out of date: {}", schema_path.display());
                eprintln!("Run with --write to update it.");
                process::exit(1);
            } else {
                println!("Schema is up-to-date: {}", schema_path.display());
            }
        }
    }
}

fn get_schema_path() -> PathBuf {
    // Try to find the project root by looking for Cargo.toml
    let mut current_dir = env::current_dir().expect("Failed to get current directory");

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return current_dir.join("rumdl.schema.json");
        }

        if !current_dir.pop() {
            // Reached filesystem root without finding Cargo.toml
            // Fall back to current directory
            return env::current_dir()
                .expect("Failed to get current directory")
                .join("rumdl.schema.json");
        }
    }
}
