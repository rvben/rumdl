//! LSP type definitions and utilities for rumdl
//!
//! This module contains LSP-specific types and utilities for rumdl,
//! following the Language Server Protocol specification.

use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::*;

/// Configuration for the rumdl LSP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RumdlLspConfig {
    /// Path to rumdl configuration file
    pub config_path: Option<String>,
    /// Enable/disable real-time linting
    pub enable_linting: bool,
    /// Enable/disable auto-fixing on save
    pub enable_auto_fix: bool,
    /// Rules to disable in the LSP server
    pub disable_rules: Vec<String>,
}

impl Default for RumdlLspConfig {
    fn default() -> Self {
        Self {
            config_path: None,
            enable_linting: true,
            enable_auto_fix: false,
            disable_rules: Vec::new(),
        }
    }
}

/// Convert rumdl warnings to LSP diagnostics
pub fn warning_to_diagnostic(warning: &crate::rule::LintWarning) -> Diagnostic {
    let start_position = Position {
        line: (warning.line.saturating_sub(1)) as u32,
        character: (warning.column.saturating_sub(1)) as u32,
    };

    // Use proper range from warning
    let end_position = Position {
        line: (warning.end_line.saturating_sub(1)) as u32,
        character: (warning.end_column.saturating_sub(1)) as u32,
    };

    let severity = match warning.severity {
        crate::rule::Severity::Error => DiagnosticSeverity::ERROR,
        crate::rule::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    // Create clickable link to rule documentation
    let code_description = warning.rule_name.as_ref().and_then(|rule_name| {
        // Create a link to the rule documentation
        Url::parse(&format!(
            "https://github.com/rvben/rumdl/blob/main/docs/{}.md",
            rule_name.to_lowercase()
        ))
        .ok()
        .map(|href| CodeDescription { href })
    });

    Diagnostic {
        range: Range {
            start: start_position,
            end: end_position,
        },
        severity: Some(severity),
        code: warning.rule_name.map(|s| NumberOrString::String(s.to_string())),
        source: Some("rumdl".to_string()),
        message: warning.message.clone(),
        related_information: None,
        tags: None,
        code_description,
        data: None,
    }
}

/// Convert byte range to LSP range
fn byte_range_to_lsp_range(text: &str, byte_range: std::ops::Range<usize>) -> Option<Range> {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut byte_pos = 0;

    let mut start_pos = None;
    let mut end_pos = None;

    for ch in text.chars() {
        if byte_pos == byte_range.start {
            start_pos = Some(Position { line, character });
        }
        if byte_pos == byte_range.end {
            end_pos = Some(Position { line, character });
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }

        byte_pos += ch.len_utf8();
    }

    // Handle end position at EOF
    if byte_pos == byte_range.end && end_pos.is_none() {
        end_pos = Some(Position { line, character });
    }

    match (start_pos, end_pos) {
        (Some(start), Some(end)) => Some(Range { start, end }),
        _ => None,
    }
}

/// Create a code action from a rumdl warning with fix
pub fn warning_to_code_action(
    warning: &crate::rule::LintWarning,
    uri: &Url,
    document_text: &str,
) -> Option<CodeAction> {
    if let Some(fix) = &warning.fix {
        // Convert fix range (byte offsets) to LSP positions
        let range = byte_range_to_lsp_range(document_text, fix.range.clone())?;

        let edit = TextEdit {
            range,
            new_text: fix.replacement.clone(),
        };

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), vec![edit]);

        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        };

        Some(CodeAction {
            title: format!("Fix: {}", warning.message),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![warning_to_diagnostic(warning)]),
            edit: Some(workspace_edit),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, LintWarning, Severity};

    #[test]
    fn test_rumdl_lsp_config_default() {
        let config = RumdlLspConfig::default();
        assert_eq!(config.config_path, None);
        assert!(config.enable_linting);
        assert!(!config.enable_auto_fix);
        assert!(config.disable_rules.is_empty());
    }

    #[test]
    fn test_rumdl_lsp_config_serialization() {
        let config = RumdlLspConfig {
            config_path: Some("/path/to/config.toml".to_string()),
            enable_linting: false,
            enable_auto_fix: true,
            disable_rules: vec!["MD001".to_string(), "MD013".to_string()],
        };

        // Test serialization
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"config_path\":\"/path/to/config.toml\""));
        assert!(json.contains("\"enable_linting\":false"));
        assert!(json.contains("\"enable_auto_fix\":true"));
        assert!(json.contains("\"MD001\""));
        assert!(json.contains("\"MD013\""));

        // Test deserialization
        let deserialized: RumdlLspConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.config_path, config.config_path);
        assert_eq!(deserialized.enable_linting, config.enable_linting);
        assert_eq!(deserialized.enable_auto_fix, config.enable_auto_fix);
        assert_eq!(deserialized.disable_rules, config.disable_rules);
    }

    #[test]
    fn test_warning_to_diagnostic_basic() {
        let warning = LintWarning {
            line: 5,
            column: 10,
            end_line: 5,
            end_column: 15,
            rule_name: Some("MD001"),
            message: "Test warning message".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);

        assert_eq!(diagnostic.range.start.line, 4); // 0-indexed
        assert_eq!(diagnostic.range.start.character, 9); // 0-indexed
        assert_eq!(diagnostic.range.end.line, 4);
        assert_eq!(diagnostic.range.end.character, 14);
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.source, Some("rumdl".to_string()));
        assert_eq!(diagnostic.message, "Test warning message");
        assert_eq!(diagnostic.code, Some(NumberOrString::String("MD001".to_string())));
    }

    #[test]
    fn test_warning_to_diagnostic_error_severity() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD002"),
            message: "Error message".to_string(),
            severity: Severity::Error,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn test_warning_to_diagnostic_no_rule_name() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: None,
            message: "Generic warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.code, None);
        assert!(diagnostic.code_description.is_none());
    }

    #[test]
    fn test_warning_to_diagnostic_edge_cases() {
        // Test with 0 line/column (should saturate to 0)
        let warning = LintWarning {
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
            rule_name: Some("MD001"),
            message: "Edge case".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.range.start.line, 0);
        assert_eq!(diagnostic.range.start.character, 0);
    }

    #[test]
    fn test_byte_range_to_lsp_range_simple() {
        let text = "Hello\nWorld";
        let range = byte_range_to_lsp_range(text, 0..5).unwrap();

        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_byte_range_to_lsp_range_multiline() {
        let text = "Hello\nWorld\nTest";
        let range = byte_range_to_lsp_range(text, 6..11).unwrap(); // "World"

        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_byte_range_to_lsp_range_unicode() {
        let text = "Hello 世界\nTest";
        // "世界" starts at byte 6 and each character is 3 bytes
        let range = byte_range_to_lsp_range(text, 6..12).unwrap();

        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 8); // 2 unicode characters
    }

    #[test]
    fn test_byte_range_to_lsp_range_eof() {
        let text = "Hello";
        let range = byte_range_to_lsp_range(text, 0..5).unwrap();

        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_byte_range_to_lsp_range_invalid() {
        let text = "Hello";
        // Out of bounds range
        let range = byte_range_to_lsp_range(text, 10..15);
        assert!(range.is_none());
    }

    #[test]
    fn test_warning_to_code_action_with_fix() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Missing space".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "Fixed".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        let action = warning_to_code_action(&warning, &uri, document_text).unwrap();

        assert_eq!(action.title, "Fix: Missing space");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.is_preferred, Some(true));

        let changes = action.edit.unwrap().changes.unwrap();
        let edits = &changes[&uri];
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "Fixed");
    }

    #[test]
    fn test_warning_to_code_action_no_fix() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "No fix available".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        let action = warning_to_code_action(&warning, &uri, document_text);
        assert!(action.is_none());
    }

    #[test]
    fn test_warning_to_code_action_multiline_fix() {
        let warning = LintWarning {
            line: 2,
            column: 1,
            end_line: 3,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Multiline fix".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 6..16, // "World\nTest"
                replacement: "Fixed\nContent".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello\nWorld\nTest Line";

        let action = warning_to_code_action(&warning, &uri, document_text).unwrap();

        let changes = action.edit.unwrap().changes.unwrap();
        let edits = &changes[&uri];
        assert_eq!(edits[0].new_text, "Fixed\nContent");
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.start.character, 0);
    }

    #[test]
    fn test_code_description_url_generation() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD013"),
            message: "Line too long".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert!(diagnostic.code_description.is_some());

        let url = diagnostic.code_description.unwrap().href;
        assert_eq!(url.as_str(), "https://github.com/rvben/rumdl/blob/main/docs/md013.md");
    }

    #[test]
    fn test_lsp_config_partial_deserialization() {
        // Test that partial JSON can be deserialized with defaults
        let json = r#"{"enable_linting": false}"#;
        let config: RumdlLspConfig = serde_json::from_str(json).unwrap();

        assert!(!config.enable_linting);
        assert_eq!(config.config_path, None); // Should use default
        assert!(!config.enable_auto_fix); // Should use default
        assert!(config.disable_rules.is_empty()); // Should use default
    }
}
