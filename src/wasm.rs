//! WebAssembly bindings for rumdl
//!
//! This module provides WASM-compatible functions for linting markdown content
//! in browser environments.

use wasm_bindgen::prelude::*;

use crate::config::{Config, MarkdownFlavor};
use crate::fix_coordinator::FixCoordinator;
use crate::rules::{all_rules, filter_rules};

/// Initialize the WASM module with better panic messages
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Lint markdown content and return warnings as JSON
///
/// Returns a JSON array of warnings, each with:
/// - `rule`: Rule name (e.g., "MD001")
/// - `message`: Warning message
/// - `line`: 1-indexed line number
/// - `column`: 1-indexed column number
/// - `end_line`: 1-indexed end line
/// - `end_column`: 1-indexed end column
/// - `severity`: "Error" or "Warning"
/// - `fix`: Optional fix object with `start`, `end`, `replacement`
#[wasm_bindgen]
pub fn lint_markdown(content: &str) -> String {
    let config = Config::default();
    let all = all_rules(&config);
    let rules = filter_rules(&all, &config.global);

    match crate::lint(content, &rules, false, MarkdownFlavor::Standard) {
        Ok(warnings) => serde_json::to_string(&warnings).unwrap_or_else(|_| "[]".to_string()),
        Err(e) => format!(r#"[{{"error": "{}"}}]"#, e),
    }
}

/// Apply all auto-fixes to the content and return the fixed content
///
/// Returns the content with all available fixes applied.
/// Uses the same fix coordinator as the CLI for consistent behavior.
#[wasm_bindgen]
pub fn apply_all_fixes(content: &str) -> String {
    let config = Config::default();
    let all = all_rules(&config);
    let rules = filter_rules(&all, &config.global);

    let warnings = match crate::lint(content, &rules, false, MarkdownFlavor::Standard) {
        Ok(w) => w,
        Err(_) => return content.to_string(),
    };

    // Use the fix coordinator for consistent behavior with CLI
    let coordinator = FixCoordinator::new();
    let mut fixed_content = content.to_string();

    match coordinator.apply_fixes_iterative(&rules, &warnings, &mut fixed_content, &config, 10) {
        Ok(_) => fixed_content,
        Err(_) => content.to_string(),
    }
}

/// Apply a single fix to the content
///
/// Takes a JSON-encoded fix object with `start`, `end`, `replacement` fields.
/// Returns the content with the fix applied.
#[wasm_bindgen]
pub fn apply_fix(content: &str, fix_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct JsFix {
        start: usize,
        end: usize,
        replacement: String,
    }

    match serde_json::from_str::<JsFix>(fix_json) {
        Ok(fix) => {
            let mut result = content.to_string();
            if fix.start <= fix.end && fix.end <= content.len() {
                result.replace_range(fix.start..fix.end, &fix.replacement);
            }
            result
        }
        Err(_) => content.to_string(),
    }
}

/// Get the rumdl version
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get list of available rules as JSON
///
/// Returns a JSON array of rule info objects, each with:
/// - `name`: Rule name (e.g., "MD001")
/// - `description`: Rule description
#[wasm_bindgen]
pub fn get_available_rules() -> String {
    let config = Config::default();
    let rules = all_rules(&config);

    let rule_info: Vec<serde_json::Value> = rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name(),
                "description": r.description()
            })
        })
        .collect();

    serde_json::to_string(&rule_info).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_version() {
        let version = get_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_get_available_rules() {
        let rules_json = get_available_rules();
        let rules: Vec<serde_json::Value> = serde_json::from_str(&rules_json).unwrap();
        assert!(!rules.is_empty());

        // Check that MD001 is in the list
        let has_md001 = rules.iter().any(|r| r["name"] == "MD001");
        assert!(has_md001);
    }

    #[test]
    fn test_lint_markdown_empty() {
        let result = lint_markdown("");
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_lint_markdown_with_issue() {
        // Heading increment violation: ## followed by ####
        let content = "## Level 2\n\n#### Level 4";
        let result = lint_markdown(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_apply_fix() {
        let content = "Hello World";
        let fix = r#"{"start": 6, "end": 11, "replacement": "Rust"}"#;
        let result = apply_fix(content, fix);
        assert_eq!(result, "Hello Rust");
    }

    #[test]
    fn test_apply_fix_invalid_json() {
        let content = "Hello World";
        let fix = "invalid json";
        let result = apply_fix(content, fix);
        assert_eq!(result, "Hello World"); // Returns original on error
    }

    #[test]
    fn test_apply_all_fixes() {
        // Content with trailing spaces that MD009 will fix
        let content = "Hello   \nWorld";
        let result = apply_all_fixes(content);
        // Should have trailing spaces removed
        assert!(!result.contains("   \n"));
    }

    #[test]
    fn test_apply_all_fixes_adjacent_blocks() {
        // Code block followed by table - both need blank lines around them
        // MD031: blanks around fenced code blocks
        // MD058: blanks around tables
        let content = "# Heading\n```code\nblock\n```\n| Header |\n|--------|\n| Cell   |";
        let result = apply_all_fixes(content);

        // Print for debugging
        eprintln!("=== Original ===\n{}", content);
        eprintln!("=== Fixed ===\n{}", result);
        eprintln!("=== Lines ===");
        for (i, line) in result.lines().enumerate() {
            if line.is_empty() {
                eprintln!("{}: [BLANK]", i + 1);
            } else {
                eprintln!("{}: {}", i + 1, line);
            }
        }

        // Should NOT have double blank lines (two consecutive empty lines)
        assert!(
            !result.contains("\n\n\n"),
            "Should not have double blank lines (3 consecutive newlines)"
        );

        // Should have exactly one blank line between code block and table
        // Expected: # Heading\n\n```code\nblock\n```\n\n| Header |...
        let lines: Vec<&str> = result.lines().collect();
        let mut blank_count = 0;
        let mut max_consecutive_blanks = 0;
        for line in &lines {
            if line.is_empty() {
                blank_count += 1;
                if blank_count > max_consecutive_blanks {
                    max_consecutive_blanks = blank_count;
                }
            } else {
                blank_count = 0;
            }
        }
        assert!(
            max_consecutive_blanks <= 1,
            "Should have at most 1 consecutive blank line, found {}",
            max_consecutive_blanks
        );
    }
}
