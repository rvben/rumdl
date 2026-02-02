//! WebAssembly bindings for rumdl
//!
//! This module provides a `Linter` class for linting markdown content
//! in browser environments, with full configuration support including
//! rule-specific options.
//!
//! # Basic Usage
//!
//! ```javascript
//! import init, { Linter, get_version, get_available_rules } from 'rumdl-wasm';
//!
//! await init();
//!
//! // Create a linter with configuration
//! const linter = new Linter({
//!   disable: ["MD041"],       // Disable specific rules
//!   "line-length": 120,       // Set line length limit
//!   flavor: "mkdocs"          // Use MkDocs markdown flavor
//! });
//!
//! // Check for issues
//! const warnings = JSON.parse(linter.check(content));
//!
//! // Apply all fixes
//! const fixed = linter.fix(content);
//! ```
//!
//! # Rule-specific Configuration
//!
//! Rules can be configured individually using their rule name as a key:
//!
//! ```javascript
//! const linter = new Linter({
//!   "MD060": {
//!     "enabled": true,
//!     "style": "aligned"
//!   },
//!   "MD013": {
//!     "line-length": 120,
//!     "code-blocks": false,
//!     "tables": false
//!   },
//!   "MD044": {
//!     "names": ["JavaScript", "TypeScript", "React"]
//!   }
//! });
//! ```

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::config::{Config, MarkdownFlavor, RuleConfig};
use crate::fix_coordinator::FixCoordinator;
use crate::rule::{LintWarning, Severity};
use crate::rule_config_serde::{is_rule_name, json_to_rule_config_with_warnings, toml_value_to_json};
use crate::rules::{all_rules, filter_rules};
use crate::types::LineLength;
use crate::utils::utf8_offsets::{byte_column_to_char_column, byte_offset_to_char_offset, get_line_content};

/// Warning with fix range converted to character offsets for JavaScript
#[derive(Serialize)]
struct JsWarning {
    message: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<JsFix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule_name: Option<String>,
}

/// Fix with character offsets instead of byte offsets
#[derive(Serialize)]
struct JsFix {
    range: JsRange,
    replacement: String,
}

/// Range with character offsets for JavaScript
#[derive(Serialize)]
struct JsRange {
    start: usize,
    end: usize,
}

/// Convert a LintWarning to a JsWarning with character offsets
fn convert_warning_for_js(warning: &LintWarning, content: &str) -> JsWarning {
    let js_fix = warning.fix.as_ref().map(|fix| JsFix {
        range: JsRange {
            start: byte_offset_to_char_offset(content, fix.range.start),
            end: byte_offset_to_char_offset(content, fix.range.end),
        },
        replacement: fix.replacement.clone(),
    });

    // Convert byte-based columns to character-based columns
    let column = get_line_content(content, warning.line)
        .map(|line| byte_column_to_char_column(line, warning.column))
        .unwrap_or(warning.column);

    let end_column = get_line_content(content, warning.end_line)
        .map(|line| byte_column_to_char_column(line, warning.end_column))
        .unwrap_or(warning.end_column);

    JsWarning {
        message: warning.message.clone(),
        line: warning.line,
        column,
        end_line: warning.end_line,
        end_column,
        severity: warning.severity,
        fix: js_fix,
        rule_name: warning.rule_name.clone(),
    }
}

/// Initialize the WASM module with better panic messages
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Configuration options for the Linter
///
/// All fields are optional. If not specified, defaults are used.
///
/// # Rule-specific configuration
///
/// Rules can be configured individually using their rule name (e.g., "MD060")
/// as a key with an object containing rule-specific options:
///
/// ```javascript
/// const linter = new Linter({
///   "MD060": {
///     "enabled": true,
///     "style": "aligned"
///   },
///   "MD013": {
///     "line-length": 120,
///     "code-blocks": false
///   }
/// });
/// ```
#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "kebab-case", default)]
pub struct LinterConfig {
    /// Rules to disable (e.g., ["MD041", "MD013"])
    pub disable: Option<Vec<String>>,

    /// Rules to enable (if empty, all rules enabled except disabled)
    pub enable: Option<Vec<String>>,

    /// Line length limit (default: 80)
    pub line_length: Option<u64>,

    /// Markdown flavor: "standard", "mkdocs", "mdx", or "quarto"
    pub flavor: Option<String>,

    /// Rule-specific configurations
    /// Keys are rule names (e.g., "MD060", "MD013") and values are rule options
    #[serde(flatten)]
    pub rules: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl LinterConfig {
    /// Convert to internal Config
    fn to_config(&self) -> Config {
        let mut config = Config::default();

        // Apply disabled rules
        if let Some(ref disable) = self.disable {
            config.global.disable = disable.clone();
        }

        // Apply enabled rules
        if let Some(ref enable) = self.enable {
            config.global.enable = enable.clone();
        }

        // Apply line length
        if let Some(line_length) = self.line_length {
            config.global.line_length = LineLength::new(line_length as usize);
        }

        // Apply flavor
        config.global.flavor = self.markdown_flavor();

        // Apply rule-specific configurations
        if let Some(ref rules) = self.rules {
            for (rule_name, json_value) in rules {
                // Only process keys that look like rule names (MD###)
                if !is_rule_name(rule_name) {
                    continue;
                }

                // Convert JSON value to RuleConfig
                let result = json_to_rule_config_with_warnings(json_value);
                if let Some(rule_config) = result.config {
                    config.rules.insert(rule_name.to_ascii_uppercase(), rule_config);
                }
            }
        }

        config
    }

    /// Convert to internal Config, collecting any warnings about invalid configuration
    fn to_config_with_warnings(&self) -> (Config, Vec<String>) {
        let mut config = Config::default();
        let mut warnings = Vec::new();

        // Apply disabled rules
        if let Some(ref disable) = self.disable {
            config.global.disable = disable.clone();
        }

        // Apply enabled rules
        if let Some(ref enable) = self.enable {
            config.global.enable = enable.clone();
        }

        // Apply line length
        if let Some(line_length) = self.line_length {
            config.global.line_length = LineLength::new(line_length as usize);
        }

        // Apply flavor
        config.global.flavor = self.markdown_flavor();

        // Apply rule-specific configurations
        if let Some(ref rules) = self.rules {
            for (rule_name, json_value) in rules {
                // Only process keys that look like rule names (MD###)
                if !is_rule_name(rule_name) {
                    continue;
                }

                // Convert JSON value to RuleConfig, collecting warnings
                let result = json_to_rule_config_with_warnings(json_value);
                for warning in result.warnings {
                    warnings.push(format!("[{}] {}", rule_name.to_ascii_uppercase(), warning));
                }
                if let Some(rule_config) = result.config {
                    config.rules.insert(rule_name.to_ascii_uppercase(), rule_config);
                }
            }
        }

        (config, warnings)
    }

    /// Parse markdown flavor from config
    fn markdown_flavor(&self) -> MarkdownFlavor {
        match self.flavor.as_deref() {
            Some("mkdocs") => MarkdownFlavor::MkDocs,
            Some("mdx") => MarkdownFlavor::MDX,
            Some("quarto") => MarkdownFlavor::Quarto,
            Some("obsidian") => MarkdownFlavor::Obsidian,
            _ => MarkdownFlavor::Standard,
        }
    }
}

/// A markdown linter with configuration
///
/// Create a new `Linter` with a configuration object, then use
/// `check()` to lint content and `fix()` to auto-fix issues.
#[wasm_bindgen]
pub struct Linter {
    config: Config,
    flavor: MarkdownFlavor,
    /// Warnings generated during configuration parsing
    config_warnings: Vec<String>,
}

#[wasm_bindgen]
impl Linter {
    /// Create a new Linter with the given configuration
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration object (see LinterConfig)
    ///
    /// # Example
    ///
    /// ```javascript
    /// const linter = new Linter({
    ///   disable: ["MD041"],
    ///   "line-length": 120
    /// });
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<Linter, JsValue> {
        let linter_config: LinterConfig = if options.is_undefined() || options.is_null() {
            LinterConfig::default()
        } else {
            serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&format!("Invalid config: {}", e)))?
        };

        let (config, config_warnings) = linter_config.to_config_with_warnings();

        Ok(Linter {
            config,
            flavor: linter_config.markdown_flavor(),
            config_warnings,
        })
    }

    /// Get any warnings generated during configuration parsing
    ///
    /// Returns a JSON array of warning strings. Each warning is prefixed
    /// with the rule name (e.g., "[MD060] Invalid severity: critical").
    ///
    /// Useful for debugging configuration issues or providing user feedback.
    pub fn get_config_warnings(&self) -> String {
        serde_json::to_string(&self.config_warnings).unwrap_or_else(|_| "[]".to_string())
    }

    /// Lint markdown content and return warnings as JSON
    ///
    /// Returns a JSON array of warnings, each with:
    /// - `rule_name`: Rule name (e.g., "MD001")
    /// - `message`: Warning message
    /// - `line`: 1-indexed line number
    /// - `column`: 1-indexed column number
    /// - `fix`: Optional fix object with `range.start`, `range.end`, `replacement`
    ///
    /// Note: Fix ranges use character offsets (not byte offsets) for JavaScript compatibility.
    /// This is important for multi-byte UTF-8 characters like `æ` or emoji.
    pub fn check(&self, content: &str) -> String {
        let all = all_rules(&self.config);
        let rules = filter_rules(&all, &self.config.global);

        match crate::lint(content, &rules, false, self.flavor, Some(&self.config)) {
            Ok(warnings) => {
                // Convert byte offsets to character offsets for JavaScript
                let js_warnings: Vec<JsWarning> = warnings.iter().map(|w| convert_warning_for_js(w, content)).collect();
                serde_json::to_string(&js_warnings).unwrap_or_else(|_| "[]".to_string())
            }
            Err(e) => format!(r#"[{{"error": "{}"}}]"#, e),
        }
    }

    /// Apply all auto-fixes to the content and return the fixed content
    ///
    /// Uses the same fix coordinator as the CLI for consistent behavior.
    pub fn fix(&self, content: &str) -> String {
        let all = all_rules(&self.config);
        let rules = filter_rules(&all, &self.config.global);

        let warnings = match crate::lint(content, &rules, false, self.flavor, Some(&self.config)) {
            Ok(w) => w,
            Err(_) => return content.to_string(),
        };

        let coordinator = FixCoordinator::new();
        let mut fixed_content = content.to_string();

        // WASM doesn't have file paths, so use None (falls back to global flavor)
        match coordinator.apply_fixes_iterative(&rules, &warnings, &mut fixed_content, &self.config, 10, None) {
            Ok(_) => fixed_content,
            Err(_) => content.to_string(),
        }
    }

    /// Get the current configuration as JSON
    ///
    /// Returns an object with global settings and rule-specific configurations.
    pub fn get_config(&self) -> String {
        // Convert rule configs to JSON-serializable format
        let rules_json: serde_json::Map<String, serde_json::Value> = self
            .config
            .rules
            .iter()
            .map(|(name, rule_config)| {
                let values: serde_json::Map<String, serde_json::Value> = rule_config
                    .values
                    .iter()
                    .filter_map(|(k, v)| toml_value_to_json(v).map(|json_val| (k.clone(), json_val)))
                    .collect();
                (name.clone(), serde_json::Value::Object(values))
            })
            .collect();

        serde_json::json!({
            "disable": self.config.global.disable,
            "enable": self.config.global.enable,
            "line_length": self.config.global.line_length.get(),
            "flavor": match self.flavor {
                MarkdownFlavor::Standard => "standard",
                MarkdownFlavor::MkDocs => "mkdocs",
                MarkdownFlavor::MDX => "mdx",
                MarkdownFlavor::Quarto => "quarto",
                MarkdownFlavor::Obsidian => "obsidian",
            },
            "rules": rules_json
        })
        .to_string()
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

        let has_md001 = rules.iter().any(|r| r["name"] == "MD001");
        assert!(has_md001);
    }

    #[test]
    fn test_linter_default_config() {
        let config = LinterConfig::default();
        assert!(config.disable.is_none());
        assert!(config.enable.is_none());
        assert!(config.line_length.is_none());
        assert!(config.flavor.is_none());
    }

    #[test]
    fn test_linter_config_to_config() {
        let config = LinterConfig {
            disable: Some(vec!["MD041".to_string()]),
            enable: None,
            line_length: Some(100),
            flavor: Some("mkdocs".to_string()),
        };

        let internal = config.to_config();
        assert!(internal.global.disable.contains(&"MD041".to_string()));
        assert_eq!(internal.global.line_length.get(), 100);
    }

    #[test]
    fn test_linter_config_flavor() {
        assert_eq!(
            LinterConfig {
                flavor: Some("standard".to_string()),
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::Standard
        );
        assert_eq!(
            LinterConfig {
                flavor: Some("mkdocs".to_string()),
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::MkDocs
        );
        assert_eq!(
            LinterConfig {
                flavor: Some("mdx".to_string()),
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::MDX
        );
        assert_eq!(
            LinterConfig {
                flavor: Some("quarto".to_string()),
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::Quarto
        );
        assert_eq!(
            LinterConfig {
                flavor: Some("obsidian".to_string()),
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::Obsidian
        );
        assert_eq!(
            LinterConfig {
                flavor: None,
                ..Default::default()
            }
            .markdown_flavor(),
            MarkdownFlavor::Standard
        );
    }

    /// This test ensures all MarkdownFlavor variants are handled in WASM.
    /// If a new flavor is added to the enum, this test will fail to compile
    /// until the WASM code is updated.
    #[test]
    fn test_all_flavors_handled_in_wasm() {
        // Exhaustive match ensures compile-time check for new variants
        let flavors = [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::MDX,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
        ];

        for flavor in flavors {
            // Verify round-trip: flavor -> string -> flavor
            let flavor_str = match flavor {
                MarkdownFlavor::Standard => "standard",
                MarkdownFlavor::MkDocs => "mkdocs",
                MarkdownFlavor::MDX => "mdx",
                MarkdownFlavor::Quarto => "quarto",
                MarkdownFlavor::Obsidian => "obsidian",
            };

            let config = LinterConfig {
                flavor: Some(flavor_str.to_string()),
                ..Default::default()
            };

            assert_eq!(
                config.markdown_flavor(),
                flavor,
                "Round-trip failed for flavor: {:?}",
                flavor
            );
        }
    }

    #[test]
    fn test_linter_check_empty() {
        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check("");
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_linter_check_with_issue() {
        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        // Heading increment violation: ## followed by ####
        let content = "## Level 2\n\n#### Level 4";
        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_linter_check_with_disabled_rule() {
        let config = LinterConfig {
            disable: Some(vec!["MD001".to_string()]),
            ..Default::default()
        };
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        // This would normally trigger MD001 (heading increment)
        let content = "## Level 2\n\n#### Level 4";
        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // MD001 should be disabled
        let has_md001 = warnings.iter().any(|w| w["rule_name"] == "MD001");
        assert!(!has_md001, "MD001 should be disabled");
    }

    #[test]
    fn test_linter_fix() {
        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        // Content with trailing spaces that MD009 will fix
        let content = "Hello   \nWorld";
        let result = linter.fix(content);
        assert!(!result.contains("   \n"));
    }

    #[test]
    fn test_linter_fix_adjacent_blocks() {
        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let content = "# Heading\n```code\nblock\n```\n| Header |\n|--------|\n| Cell   |";
        let result = linter.fix(content);

        // Should NOT have double blank lines
        assert!(!result.contains("\n\n\n"), "Should not have double blank lines");
    }

    #[test]
    fn test_linter_get_config() {
        let config = LinterConfig {
            disable: Some(vec!["MD041".to_string()]),
            flavor: Some("mkdocs".to_string()),
            ..Default::default()
        };
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.get_config();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["flavor"], "mkdocs");
        assert!(
            parsed["disable"]
                .as_array()
                .unwrap()
                .contains(&serde_json::Value::String("MD041".to_string()))
        );
    }

    // byte_offset_to_char_offset tests are in utils/utf8_offsets.rs

    #[test]
    fn test_check_norwegian_letter_fix_offset() {
        // This is the exact bug case: Norwegian letter at end of file without trailing newline
        let content = "# Heading\n\nContent with Norwegian letter \"æ\".";
        assert_eq!(content.len(), 46); // 46 bytes (æ is 2 bytes)
        assert_eq!(content.chars().count(), 45); // 45 characters (æ is 1 char)

        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // Should have MD047 warning (missing trailing newline)
        let md047 = warnings.iter().find(|w| w["rule_name"] == "MD047");
        assert!(md047.is_some(), "Should have MD047 warning");

        // The fix range should use character offsets, not byte offsets
        let fix = md047.unwrap()["fix"].as_object().unwrap();
        let range = fix["range"].as_object().unwrap();

        // Character offset should be 45 (not byte offset 46)
        assert_eq!(
            range["start"].as_u64().unwrap(),
            45,
            "Fix start should be character offset 45, not byte offset 46"
        );
        assert_eq!(
            range["end"].as_u64().unwrap(),
            45,
            "Fix end should be character offset 45"
        );
    }

    #[test]
    fn test_fix_norwegian_letter() {
        // Verify the fix() method works correctly with Norwegian letters
        let content = "# Heading\n\nContent with Norwegian letter \"æ\".";

        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let fixed = linter.fix(content);

        // Should add trailing newline
        assert!(fixed.ends_with('\n'), "Should end with newline");
        assert_eq!(fixed, "# Heading\n\nContent with Norwegian letter \"æ\".\n");
    }

    #[test]
    fn test_check_norwegian_letter_column_offset() {
        // This tests the column conversion fix for rvben/obsidian-rumdl#4
        // The bug was that column was byte-based (36) but should be char-based (35)
        let content = "# Heading\n\nContent with Norwegian letter \"æ\".";

        // Line 3 is "Content with Norwegian letter \"æ\"."
        // Bytes: 35 (æ is 2 bytes), Chars: 34 (æ is 1 char)
        // MD047 reports column at position after last char
        // Byte column would be 36, char column should be 35

        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        let md047 = warnings.iter().find(|w| w["rule_name"] == "MD047");
        assert!(md047.is_some(), "Should have MD047 warning");

        let warning = md047.unwrap();

        // Column should be character-based (35), not byte-based (36)
        assert_eq!(
            warning["column"].as_u64().unwrap(),
            35,
            "Column should be char offset 35, not byte offset 36"
        );
        assert_eq!(
            warning["end_column"].as_u64().unwrap(),
            35,
            "End column should also be char offset 35"
        );

        // Verify line is correct
        assert_eq!(warning["line"].as_u64().unwrap(), 3);
        assert_eq!(warning["end_line"].as_u64().unwrap(), 3);
    }

    #[test]
    fn test_check_multiple_multibyte_chars_column() {
        // Test with multiple multi-byte characters to ensure column conversion works
        // throughout a line, not just at the end
        let content = "# æøå\n\nLine with æ and ø here.";

        let config = LinterConfig {
            disable: Some(vec!["MD047".to_string()]), // Disable MD047 to focus on other warnings
            ..Default::default()
        };
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // Any warning on line 1 should have correct character-based column
        // The heading "# æøå" is 6 bytes but 5 characters
        for warning in &warnings {
            let line = warning["line"].as_u64().unwrap();
            let column = warning["column"].as_u64().unwrap();

            if line == 1 {
                // Column should never exceed character count + 1
                // "# æøå" has 5 chars, so max column is 6
                assert!(column <= 6, "Column {column} on line 1 exceeds char count (max 6)");
            }
        }
    }

    #[test]
    fn test_check_emoji_column() {
        // Test with emoji (4-byte UTF-8) to verify column conversion
        let content = "# Test 👋\n\nHello";

        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // Verify any warnings have character-based columns
        // Line 1 "# Test 👋" is 11 bytes but 8 characters
        for warning in &warnings {
            let line = warning["line"].as_u64().unwrap();
            let column = warning["column"].as_u64().unwrap();

            if line == 1 {
                assert!(
                    column <= 9, // 8 chars + 1 for position after
                    "Column {column} on line 1 with emoji should be char-based (max 9), not byte-based"
                );
            }
        }
    }

    #[test]
    fn test_check_japanese_column() {
        // Test with Japanese characters (3-byte UTF-8 each)
        let content = "# 日本語\n\nTest";

        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // Line 1 "# 日本語" is 11 bytes but 5 characters
        for warning in &warnings {
            let line = warning["line"].as_u64().unwrap();
            let column = warning["column"].as_u64().unwrap();

            if line == 1 {
                assert!(
                    column <= 6, // 5 chars + 1 for position after
                    "Column {column} on line 1 with Japanese should be char-based (max 6), not byte-based (would be 12)"
                );
            }
        }
    }

    // ========== WASM Rule Configuration Integration Tests ==========
    // Note: Unit tests for is_rule_name and json_to_rule_config are in rule_config_serde.rs

    #[test]
    fn test_linter_config_with_rule_configs() {
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "enabled": true,
                "style": "aligned"
            }),
        );
        rules.insert(
            "MD013".to_string(),
            serde_json::json!({
                "line-length": 120,
                "code-blocks": false
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let internal = config.to_config();

        // Check MD060 config was applied
        let md060 = internal.rules.get("MD060");
        assert!(md060.is_some(), "MD060 should be in rules");
        let md060_config = md060.unwrap();
        assert_eq!(md060_config.values.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(
            md060_config.values.get("style"),
            Some(&toml::Value::String("aligned".to_string()))
        );

        // Check MD013 config was applied
        let md013 = internal.rules.get("MD013");
        assert!(md013.is_some(), "MD013 should be in rules");
        let md013_config = md013.unwrap();
        assert_eq!(md013_config.values.get("line-length"), Some(&toml::Value::Integer(120)));
        assert_eq!(
            md013_config.values.get("code-blocks"),
            Some(&toml::Value::Boolean(false))
        );
    }

    #[test]
    fn test_linter_config_rule_name_case_normalization() {
        // Rule names should be normalized to uppercase
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "md060".to_string(), // lowercase
            serde_json::json!({ "enabled": true }),
        );
        rules.insert(
            "Md013".to_string(), // mixed case
            serde_json::json!({ "enabled": true }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let internal = config.to_config();

        // Both should be normalized to uppercase
        assert!(internal.rules.contains_key("MD060"), "MD060 should be uppercase");
        assert!(internal.rules.contains_key("MD013"), "MD013 should be uppercase");
    }

    #[test]
    fn test_linter_config_ignores_non_rule_keys() {
        // Non-rule keys in the rules map should be ignored
        let mut rules = std::collections::HashMap::new();
        rules.insert("MD060".to_string(), serde_json::json!({ "enabled": true }));
        rules.insert("not-a-rule".to_string(), serde_json::json!({ "value": 123 }));
        rules.insert("global".to_string(), serde_json::json!({ "key": "value" }));

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let internal = config.to_config();

        // Only MD060 should be in rules
        assert!(internal.rules.contains_key("MD060"));
        assert!(!internal.rules.contains_key("not-a-rule"));
        assert!(!internal.rules.contains_key("global"));
    }

    #[test]
    fn test_get_config_includes_rules() {
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "enabled": true,
                "style": "aligned"
            }),
        );

        let config = LinterConfig {
            disable: Some(vec!["MD041".to_string()]),
            rules: Some(rules),
            flavor: Some("mkdocs".to_string()),
            ..Default::default()
        };

        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.get_config();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Check global settings
        assert_eq!(parsed["flavor"], "mkdocs");

        // Check rules are included
        assert!(parsed["rules"].is_object(), "rules should be an object");
        let rules_obj = parsed["rules"].as_object().unwrap();
        assert!(rules_obj.contains_key("MD060"), "MD060 should be in rules");

        let md060 = &rules_obj["MD060"];
        assert_eq!(md060["enabled"], true);
        assert_eq!(md060["style"], "aligned");
    }

    #[test]
    fn test_linter_config_deserializes_from_json() {
        // Test that serde deserializes the config correctly including flattened rules
        let json = serde_json::json!({
            "disable": ["MD041"],
            "line-length": 100,
            "flavor": "mkdocs",
            "MD060": {
                "enabled": true,
                "style": "aligned"
            },
            "MD013": {
                "tables": false
            }
        });

        let config: LinterConfig = serde_json::from_value(json).unwrap();

        assert_eq!(config.disable, Some(vec!["MD041".to_string()]));
        assert_eq!(config.line_length, Some(100));
        assert_eq!(config.flavor, Some("mkdocs".to_string()));

        let rules = config.rules.as_ref().unwrap();
        assert!(rules.contains_key("MD060"));
        assert!(rules.contains_key("MD013"));

        let md060 = &rules["MD060"];
        assert_eq!(md060["enabled"], true);
        assert_eq!(md060["style"], "aligned");
    }

    #[test]
    fn test_linter_with_md044_names_config() {
        // Test MD044 proper names configuration (array values)
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD044".to_string(),
            serde_json::json!({
                "names": ["JavaScript", "TypeScript", "GitHub"],
                "code-blocks": false
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let internal = config.to_config();

        let md044 = internal.rules.get("MD044").unwrap();
        let names = md044.values.get("names").unwrap();

        // Verify the array was converted correctly
        if let toml::Value::Array(arr) = names {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], toml::Value::String("JavaScript".to_string()));
            assert_eq!(arr[1], toml::Value::String("TypeScript".to_string()));
            assert_eq!(arr[2], toml::Value::String("GitHub".to_string()));
        } else {
            panic!("names should be an array");
        }
    }

    #[test]
    fn test_linter_check_with_md060_config() {
        // Integration test: verify MD060 config affects linting behavior
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "enabled": true,
                "style": "aligned"
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        // Table with inconsistent formatting (should trigger MD060 with aligned style)
        let content = "# Heading\n\n| a | b |\n|---|---|\n|1|2|";

        let result = linter.check(content);
        let warnings: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        // Should have MD060 warning because table cells aren't aligned
        let has_md060 = warnings.iter().any(|w| w["rule_name"] == "MD060");
        assert!(has_md060, "Should have MD060 warning for unaligned table");
    }

    #[test]
    fn test_linter_fix_with_rule_config() {
        // Integration test: verify rule config affects fix behavior
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "enabled": true,
                "style": "compact"
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        // Table with extra spacing
        let content = "# Heading\n\n| a  |  b |\n|---|---|\n| 1 | 2 |";

        let fixed = linter.fix(content);

        // With compact style, the table should have minimal spacing
        assert!(
            fixed.contains("|a|b|") || fixed.contains("| a | b |"),
            "Table should be formatted according to MD060 config"
        );
    }

    #[test]
    fn test_linter_config_empty_rules() {
        // Empty rules map should work fine
        let config = LinterConfig {
            rules: Some(std::collections::HashMap::new()),
            ..Default::default()
        };

        let internal = config.to_config();
        assert!(internal.rules.is_empty());
    }

    #[test]
    fn test_linter_config_no_rules() {
        // None rules should work fine
        let config = LinterConfig {
            rules: None,
            ..Default::default()
        };

        let internal = config.to_config();
        assert!(internal.rules.is_empty());
    }

    #[test]
    fn test_config_warnings_valid_config() {
        // Valid config should produce no warnings
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "enabled": true,
                "style": "aligned",
                "severity": "warning"
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let (_, warnings) = config.to_config_with_warnings();
        assert!(warnings.is_empty(), "Valid config should produce no warnings");
    }

    #[test]
    fn test_config_warnings_invalid_severity() {
        // Invalid severity should produce a warning
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "severity": "critical"  // Invalid - should be error/warning/info
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let (internal, warnings) = config.to_config_with_warnings();

        // Config should still be applied (minus invalid severity)
        assert!(internal.rules.contains_key("MD060"));

        // Should have a warning about invalid severity
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("[MD060]"), "Warning should include rule name");
        assert!(warnings[0].contains("severity"), "Warning should mention severity");
        assert!(warnings[0].contains("critical"), "Warning should mention invalid value");
    }

    #[test]
    fn test_config_warnings_invalid_value_type() {
        // Invalid value type should produce a warning
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD013".to_string(),
            serde_json::json!({
                "line-length": "not-a-number"  // Should be a number
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let (internal, warnings) = config.to_config_with_warnings();

        // Config should still be applied for valid values
        assert!(internal.rules.contains_key("MD013"));

        // Should have a warning about invalid value type
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("[MD013]"), "Warning should include rule name");
        assert!(warnings[0].contains("line-length"), "Warning should mention field name");
    }

    #[test]
    fn test_config_warnings_multiple_rules() {
        // Multiple rules with issues should produce multiple warnings
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "MD060".to_string(),
            serde_json::json!({
                "severity": "fatal"  // Invalid
            }),
        );
        rules.insert(
            "MD013".to_string(),
            serde_json::json!({
                "severity": "bad"  // Also invalid
            }),
        );

        let config = LinterConfig {
            rules: Some(rules),
            ..Default::default()
        };

        let (_, warnings) = config.to_config_with_warnings();

        // Should have 2 warnings, one for each rule
        assert_eq!(warnings.len(), 2, "Should have warnings for both rules");
        let has_md060_warning = warnings.iter().any(|w| w.contains("[MD060]"));
        let has_md013_warning = warnings.iter().any(|w| w.contains("[MD013]"));
        assert!(has_md060_warning, "Should have MD060 warning");
        assert!(has_md013_warning, "Should have MD013 warning");
    }

    #[test]
    fn test_linter_get_config_warnings() {
        // Test the get_config_warnings() method returns JSON array
        let config = LinterConfig {
            rules: Some(std::collections::HashMap::new()),
            ..Default::default()
        };
        let (internal_config, _) = config.to_config_with_warnings();

        let linter = Linter {
            config: internal_config,
            flavor: config.markdown_flavor(),
            config_warnings: vec!["[MD060] Invalid severity: test".to_string()],
        };

        let result = linter.get_config_warnings();
        let warnings: Vec<String> = serde_json::from_str(&result).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0], "[MD060] Invalid severity: test");
    }

    #[test]
    fn test_linter_get_config_warnings_empty() {
        // Empty warnings should return empty JSON array
        let config = LinterConfig::default();
        let linter = Linter {
            config: config.to_config(),
            flavor: config.markdown_flavor(),
            config_warnings: Vec::new(),
        };

        let result = linter.get_config_warnings();
        let warnings: Vec<String> = serde_json::from_str(&result).unwrap();
        assert!(warnings.is_empty());
    }
}
