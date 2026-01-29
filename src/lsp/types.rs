//! LSP type definitions and utilities for rumdl
//!
//! This module contains LSP-specific types and utilities for rumdl,
//! following the Language Server Protocol specification.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tower_lsp::lsp_types::*;

/// State of the workspace index
#[derive(Debug, Clone, PartialEq)]
pub enum IndexState {
    /// Index is being built
    Building {
        /// Progress percentage (0-100)
        progress: f32,
        /// Number of files indexed so far
        files_indexed: usize,
        /// Total number of files to index
        total_files: usize,
    },
    /// Index is ready for use
    Ready,
    /// Index encountered an error
    Error(String),
}

impl Default for IndexState {
    fn default() -> Self {
        Self::Building {
            progress: 0.0,
            files_indexed: 0,
            total_files: 0,
        }
    }
}

/// Messages sent to the background index worker
#[derive(Debug)]
pub enum IndexUpdate {
    /// A file was changed (content included for debouncing)
    FileChanged { path: PathBuf, content: String },
    /// A file was deleted
    FileDeleted { path: PathBuf },
    /// Request a full workspace rescan
    FullRescan,
    /// Shutdown the worker
    Shutdown,
}

/// Controls the order in which configuration sources are merged
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigurationPreference {
    /// Editor settings take priority over config files (default)
    #[default]
    EditorFirst,
    /// Config files take priority over editor settings
    FilesystemFirst,
    /// Ignore config files, use only editor settings
    EditorOnly,
}

/// Per-rule settings that can be passed via LSP initialization options
///
/// This struct mirrors the rule-specific settings from Config, allowing
/// editors to configure rules without needing a config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LspRuleSettings {
    /// Global line length for rules that use it
    pub line_length: Option<usize>,
    /// Rules to disable
    pub disable: Option<Vec<String>>,
    /// Rules to enable
    pub enable: Option<Vec<String>>,
    /// Per-rule configuration (e.g., "MD013": { "lineLength": 120 })
    #[serde(flatten)]
    pub rules: std::collections::HashMap<String, serde_json::Value>,
}

/// Configuration for the rumdl LSP server (from initialization options)
///
/// Uses camelCase for all fields per LSP specification.
/// Follows Ruff's LSP configuration pattern for consistency.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RumdlLspConfig {
    /// Path to rumdl configuration file
    pub config_path: Option<String>,
    /// Enable/disable real-time linting
    pub enable_linting: bool,
    /// Enable/disable auto-fixing on save
    pub enable_auto_fix: bool,
    /// Rules to enable (overrides config file)
    /// If specified, only these rules will be active
    pub enable_rules: Option<Vec<String>>,
    /// Rules to disable (overrides config file)
    pub disable_rules: Option<Vec<String>>,
    /// Controls priority between editor settings and config files
    pub configuration_preference: ConfigurationPreference,
    /// Rule-specific settings passed from the editor
    /// This allows configuring rules like MD013.lineLength directly from editor settings
    pub settings: Option<LspRuleSettings>,
}

impl Default for RumdlLspConfig {
    fn default() -> Self {
        Self {
            config_path: None,
            enable_linting: true,
            enable_auto_fix: false,
            enable_rules: None,
            disable_rules: None,
            configuration_preference: ConfigurationPreference::default(),
            settings: None,
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
        crate::rule::Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    // Create clickable link to rule documentation
    let code_description = warning.rule_name.as_ref().and_then(|rule_name| {
        // Create a link to the rule documentation on rumdl.dev
        Url::parse(&format!("https://rumdl.dev/{}/", rule_name.to_lowercase()))
            .ok()
            .map(|href| CodeDescription { href })
    });

    Diagnostic {
        range: Range {
            start: start_position,
            end: end_position,
        },
        severity: Some(severity),
        code: warning.rule_name.as_ref().map(|s| NumberOrString::String(s.clone())),
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

    // Handle positions at or beyond EOF
    // This is crucial for fixes that delete trailing content (like MD012 EOF blanks)
    if start_pos.is_none() && byte_pos >= byte_range.start {
        start_pos = Some(Position { line, character });
    }
    if end_pos.is_none() && byte_pos >= byte_range.end {
        end_pos = Some(Position { line, character });
    }

    match (start_pos, end_pos) {
        (Some(start), Some(end)) => Some(Range { start, end }),
        _ => {
            // If we still don't have valid positions, log for debugging
            // This shouldn't happen with proper fix ranges
            log::warn!(
                "Failed to convert byte range {:?} to LSP range for text of length {}",
                byte_range,
                text.len()
            );
            None
        }
    }
}

/// Create code actions from a rumdl warning
/// Returns a vector of available actions: fix action (if available) and ignore actions
pub fn warning_to_code_actions(warning: &crate::rule::LintWarning, uri: &Url, document_text: &str) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // Add fix action if available (marked as preferred)
    if let Some(fix_action) = create_fix_action(warning, uri, document_text) {
        actions.push(fix_action);
    }

    // Add manual reflow action for MD013 when no fix is available
    // This allows users to manually reflow paragraphs without enabling reflow globally
    if warning.rule_name.as_deref() == Some("MD013")
        && warning.fix.is_none()
        && let Some(reflow_action) = create_reflow_action(warning, uri, document_text)
    {
        actions.push(reflow_action);
    }

    // Add convert-to-markdown-link action for MD034 (bare URLs)
    // This provides an alternative to the default angle bracket fix
    if warning.rule_name.as_deref() == Some("MD034")
        && let Some(convert_action) = create_convert_to_link_action(warning, uri, document_text)
    {
        actions.push(convert_action);
    }

    // Add ignore-line action
    if let Some(ignore_line_action) = create_ignore_line_action(warning, uri, document_text) {
        actions.push(ignore_line_action);
    }

    actions
}

/// Create a fix code action from a rumdl warning with fix
fn create_fix_action(warning: &crate::rule::LintWarning, uri: &Url, document_text: &str) -> Option<CodeAction> {
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

/// Create a manual reflow code action for MD013 line length warnings
/// This allows users to manually reflow paragraphs even when reflow is disabled in config
fn create_reflow_action(warning: &crate::rule::LintWarning, uri: &Url, document_text: &str) -> Option<CodeAction> {
    // Extract line length limit from message (format: "Line length X exceeds Y characters")
    let line_length = extract_line_length_from_message(&warning.message).unwrap_or(80);

    // Use the reflow helper to find and reflow the paragraph
    let reflow_result = crate::utils::text_reflow::reflow_paragraph_at_line(document_text, warning.line, line_length)?;

    // Convert byte offsets to LSP range
    let range = byte_range_to_lsp_range(document_text, reflow_result.start_byte..reflow_result.end_byte)?;

    let edit = TextEdit {
        range,
        new_text: reflow_result.reflowed_text,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };

    Some(CodeAction {
        title: "Reflow paragraph".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![warning_to_diagnostic(warning)]),
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(false), // Not preferred - manual action only
        disabled: None,
        data: None,
    })
}

/// Extract line length limit from MD013 warning message
/// Message format: "Line length X exceeds Y characters"
fn extract_line_length_from_message(message: &str) -> Option<usize> {
    // Find "exceeds" in the message
    let exceeds_idx = message.find("exceeds")?;
    let after_exceeds = &message[exceeds_idx + 7..]; // Skip "exceeds"

    // Find the number after "exceeds"
    let num_str = after_exceeds.split_whitespace().next()?;

    num_str.parse::<usize>().ok()
}

/// Create a "convert to markdown link" action for MD034 bare URL warnings
/// This provides an alternative to the default angle bracket fix, allowing users
/// to create proper markdown links with descriptive text
fn create_convert_to_link_action(
    warning: &crate::rule::LintWarning,
    uri: &Url,
    document_text: &str,
) -> Option<CodeAction> {
    // Get the fix from the warning
    let fix = warning.fix.as_ref()?;

    // Extract the URL from the fix replacement (format: "<https://example.com>" or "<user@example.com>")
    // The MD034 fix wraps URLs in angle brackets
    let url = extract_url_from_fix_replacement(&fix.replacement)?;

    // Convert byte offsets to LSP range
    let range = byte_range_to_lsp_range(document_text, fix.range.clone())?;

    // Create markdown link with the domain as link text
    // The user can then edit the link text manually
    // Note: LSP WorkspaceEdit doesn't support snippet placeholders like ${1:text}
    // so we just use the domain as default text that user can select and replace
    let link_text = extract_domain_for_placeholder(url);
    let new_text = format!("[{link_text}]({url})");

    let edit = TextEdit { range, new_text };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };

    Some(CodeAction {
        title: "Convert to markdown link".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![warning_to_diagnostic(warning)]),
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(false), // Not preferred - user explicitly chooses this
        disabled: None,
        data: None,
    })
}

/// Extract URL/email from MD034 fix replacement
/// MD034 fix format: "<https://example.com>" or "<user@example.com>"
fn extract_url_from_fix_replacement(replacement: &str) -> Option<&str> {
    // Remove angle brackets that MD034's fix adds
    let trimmed = replacement.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        Some(&trimmed[1..trimmed.len() - 1])
    } else {
        None
    }
}

/// Extract a smart placeholder from a URL for the link text
/// For "https://example.com/path" returns "example.com"
/// For "user@example.com" returns "user@example.com"
fn extract_domain_for_placeholder(url: &str) -> &str {
    // For email addresses, use the whole email
    if url.contains('@') && !url.contains("://") {
        return url;
    }

    // For URLs, extract the domain
    url.split("://").nth(1).and_then(|s| s.split('/').next()).unwrap_or(url)
}

/// Create an ignore-line code action that adds a rumdl-disable-line comment
fn create_ignore_line_action(warning: &crate::rule::LintWarning, uri: &Url, document_text: &str) -> Option<CodeAction> {
    let rule_id = warning.rule_name.as_ref()?;
    let warning_line = warning.line.saturating_sub(1);

    // Find the end of the line where the warning occurs
    let lines: Vec<&str> = document_text.lines().collect();
    let line_content = lines.get(warning_line)?;

    // Check if this line already has a rumdl-disable-line comment
    if line_content.contains("rumdl-disable-line") || line_content.contains("markdownlint-disable-line") {
        // Don't offer the action if the line already has a disable comment
        return None;
    }

    // Calculate position at end of line
    let line_end = Position {
        line: warning_line as u32,
        character: line_content.len() as u32,
    };

    // Use rumdl-disable-line syntax
    let comment = format!(" <!-- rumdl-disable-line {rule_id} -->");

    let edit = TextEdit {
        range: Range {
            start: line_end,
            end: line_end,
        },
        new_text: comment,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeAction {
        title: format!("Ignore {rule_id} for this line"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![warning_to_diagnostic(warning)]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false), // Fix action is preferred
        disabled: None,
        data: None,
    })
}

/// Legacy function for backwards compatibility
/// Use `warning_to_code_actions` instead
#[deprecated(since = "0.0.167", note = "Use warning_to_code_actions instead")]
pub fn warning_to_code_action(
    warning: &crate::rule::LintWarning,
    uri: &Url,
    document_text: &str,
) -> Option<CodeAction> {
    warning_to_code_actions(warning, uri, document_text)
        .into_iter()
        .find(|action| action.is_preferred == Some(true))
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
    }

    #[test]
    fn test_rumdl_lsp_config_serialization() {
        let config = RumdlLspConfig {
            config_path: Some("/path/to/config.toml".to_string()),
            enable_linting: false,
            enable_auto_fix: true,
            enable_rules: None,
            disable_rules: None,
            configuration_preference: ConfigurationPreference::EditorFirst,
            settings: None,
        };

        // Test serialization (uses camelCase)
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"configPath\":\"/path/to/config.toml\""));
        assert!(json.contains("\"enableLinting\":false"));
        assert!(json.contains("\"enableAutoFix\":true"));

        // Test deserialization
        let deserialized: RumdlLspConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.config_path, config.config_path);
        assert_eq!(deserialized.enable_linting, config.enable_linting);
        assert_eq!(deserialized.enable_auto_fix, config.enable_auto_fix);
    }

    #[test]
    fn test_warning_to_diagnostic_basic() {
        let warning = LintWarning {
            line: 5,
            column: 10,
            end_line: 5,
            end_column: 15,
            rule_name: Some("MD001".to_string()),
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
            rule_name: Some("MD002".to_string()),
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
            rule_name: Some("MD001".to_string()),
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
    fn test_byte_range_to_lsp_range_insertion_at_eof() {
        // Test insertion point at EOF (like MD047 adds trailing newline)
        let text = "Hello\nWorld";
        let text_len = text.len(); // 11 bytes
        let range = byte_range_to_lsp_range(text, text_len..text_len).unwrap();

        // Should create a zero-width range at EOF position
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 5); // After "World"
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_byte_range_to_lsp_range_insertion_at_eof_with_trailing_newline() {
        // Test when file already ends with newline
        let text = "Hello\nWorld\n";
        let text_len = text.len(); // 12 bytes
        let range = byte_range_to_lsp_range(text, text_len..text_len).unwrap();

        // Should create a zero-width range at EOF (after the newline)
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.character, 0); // Beginning of line after newline
        assert_eq!(range.end.line, 2);
        assert_eq!(range.end.character, 0);
    }

    #[test]
    fn test_warning_to_code_action_with_fix() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Missing space".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "Fixed".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        let actions = warning_to_code_actions(&warning, &uri, document_text);
        assert!(!actions.is_empty());
        let action = &actions[0]; // First action is the fix

        assert_eq!(action.title, "Fix: Missing space");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.is_preferred, Some(true));

        let changes = action.edit.as_ref().unwrap().changes.as_ref().unwrap();
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
            rule_name: Some("MD001".to_string()),
            message: "No fix available".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        let actions = warning_to_code_actions(&warning, &uri, document_text);
        // Should have ignore actions but no fix action (fix actions have is_preferred = true)
        assert!(actions.iter().all(|a| a.is_preferred != Some(true)));
    }

    #[test]
    fn test_warning_to_code_action_multiline_fix() {
        let warning = LintWarning {
            line: 2,
            column: 1,
            end_line: 3,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Multiline fix".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 6..16, // "World\nTest"
                replacement: "Fixed\nContent".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello\nWorld\nTest Line";

        let actions = warning_to_code_actions(&warning, &uri, document_text);
        assert!(!actions.is_empty());
        let action = &actions[0]; // First action is the fix

        let changes = action.edit.as_ref().unwrap().changes.as_ref().unwrap();
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
            rule_name: Some("MD013".to_string()),
            message: "Line too long".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert!(diagnostic.code_description.is_some());

        let url = diagnostic.code_description.unwrap().href;
        assert_eq!(url.as_str(), "https://rumdl.dev/md013/");
    }

    #[test]
    fn test_lsp_config_partial_deserialization() {
        // Test that partial JSON can be deserialized with defaults (uses camelCase per LSP spec)
        let json = r#"{"enableLinting": false}"#;
        let config: RumdlLspConfig = serde_json::from_str(json).unwrap();

        assert!(!config.enable_linting);
        assert_eq!(config.config_path, None); // Should use default
        assert!(!config.enable_auto_fix); // Should use default
    }

    #[test]
    fn test_configuration_preference_serialization() {
        // Test EditorFirst (default)
        let pref = ConfigurationPreference::EditorFirst;
        let json = serde_json::to_string(&pref).unwrap();
        assert_eq!(json, "\"editorFirst\"");

        // Test FilesystemFirst
        let pref = ConfigurationPreference::FilesystemFirst;
        let json = serde_json::to_string(&pref).unwrap();
        assert_eq!(json, "\"filesystemFirst\"");

        // Test EditorOnly
        let pref = ConfigurationPreference::EditorOnly;
        let json = serde_json::to_string(&pref).unwrap();
        assert_eq!(json, "\"editorOnly\"");

        // Test deserialization
        let pref: ConfigurationPreference = serde_json::from_str("\"filesystemFirst\"").unwrap();
        assert_eq!(pref, ConfigurationPreference::FilesystemFirst);
    }

    #[test]
    fn test_lsp_rule_settings_deserialization() {
        // Test basic settings
        let json = r#"{
            "lineLength": 120,
            "disable": ["MD001", "MD002"],
            "enable": ["MD013"]
        }"#;
        let settings: LspRuleSettings = serde_json::from_str(json).unwrap();

        assert_eq!(settings.line_length, Some(120));
        assert_eq!(settings.disable, Some(vec!["MD001".to_string(), "MD002".to_string()]));
        assert_eq!(settings.enable, Some(vec!["MD013".to_string()]));
    }

    #[test]
    fn test_lsp_rule_settings_with_per_rule_config() {
        // Test per-rule configuration via flattened HashMap
        let json = r#"{
            "lineLength": 80,
            "MD013": {
                "lineLength": 120,
                "codeBlocks": false
            },
            "MD024": {
                "siblingsOnly": true
            }
        }"#;
        let settings: LspRuleSettings = serde_json::from_str(json).unwrap();

        assert_eq!(settings.line_length, Some(80));

        // Check MD013 config
        let md013 = settings.rules.get("MD013").unwrap();
        assert_eq!(md013.get("lineLength").unwrap().as_u64(), Some(120));
        assert_eq!(md013.get("codeBlocks").unwrap().as_bool(), Some(false));

        // Check MD024 config
        let md024 = settings.rules.get("MD024").unwrap();
        assert_eq!(md024.get("siblingsOnly").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_full_lsp_config_with_settings() {
        // Test complete LSP config with all new fields (camelCase per LSP spec)
        let json = r#"{
            "configPath": "/path/to/config",
            "enableLinting": true,
            "enableAutoFix": false,
            "configurationPreference": "editorFirst",
            "settings": {
                "lineLength": 100,
                "disable": ["MD033"],
                "MD013": {
                    "lineLength": 120,
                    "tables": false
                }
            }
        }"#;
        let config: RumdlLspConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.config_path, Some("/path/to/config".to_string()));
        assert!(config.enable_linting);
        assert!(!config.enable_auto_fix);
        assert_eq!(config.configuration_preference, ConfigurationPreference::EditorFirst);

        let settings = config.settings.unwrap();
        assert_eq!(settings.line_length, Some(100));
        assert_eq!(settings.disable, Some(vec!["MD033".to_string()]));

        let md013 = settings.rules.get("MD013").unwrap();
        assert_eq!(md013.get("lineLength").unwrap().as_u64(), Some(120));
        assert_eq!(md013.get("tables").unwrap().as_bool(), Some(false));
    }

    #[test]
    fn test_create_ignore_line_action_uses_rumdl_syntax() {
        let warning = LintWarning {
            line: 5,
            column: 1,
            end_line: 5,
            end_column: 50,
            rule_name: Some("MD013".to_string()),
            message: "Line too long".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let document = "Line 1\nLine 2\nLine 3\nLine 4\nThis is a very long line that exceeds the limit\nLine 6";
        let uri = Url::parse("file:///test.md").unwrap();

        let action = create_ignore_line_action(&warning, &uri, document).unwrap();

        assert_eq!(action.title, "Ignore MD013 for this line");
        assert_eq!(action.is_preferred, Some(false));
        assert!(action.edit.is_some());

        // Verify the edit adds the rumdl-disable-line comment
        let edit = action.edit.unwrap();
        let changes = edit.changes.unwrap();
        let file_edits = changes.get(&uri).unwrap();

        assert_eq!(file_edits.len(), 1);
        assert!(file_edits[0].new_text.contains("rumdl-disable-line MD013"));
        assert!(!file_edits[0].new_text.contains("markdownlint"));

        // Verify position is at end of line
        assert_eq!(file_edits[0].range.start.line, 4); // 0-indexed line 5
        assert_eq!(file_edits[0].range.start.character, 47); // End of "This is a very long line that exceeds the limit"
    }

    #[test]
    fn test_create_ignore_line_action_no_duplicate() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 50,
            rule_name: Some("MD013".to_string()),
            message: "Line too long".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        // Line already has a disable comment
        let document = "This is a line <!-- rumdl-disable-line MD013 -->";
        let uri = Url::parse("file:///test.md").unwrap();

        let action = create_ignore_line_action(&warning, &uri, document);

        // Should not offer the action if comment already exists
        assert!(action.is_none());
    }

    #[test]
    fn test_create_ignore_line_action_detects_markdownlint_syntax() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 50,
            rule_name: Some("MD013".to_string()),
            message: "Line too long".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        // Line has markdownlint-disable-line comment
        let document = "This is a line <!-- markdownlint-disable-line MD013 -->";
        let uri = Url::parse("file:///test.md").unwrap();

        let action = create_ignore_line_action(&warning, &uri, document);

        // Should not offer the action if markdownlint comment exists
        assert!(action.is_none());
    }

    #[test]
    fn test_warning_to_code_actions_with_fix() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD009".to_string()),
            message: "Trailing spaces".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "Fixed".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello   \nWorld";

        let actions = warning_to_code_actions(&warning, &uri, document_text);

        // Should have 2 actions: fix and ignore-line
        assert_eq!(actions.len(), 2);

        // First action should be fix (preferred)
        assert_eq!(actions[0].title, "Fix: Trailing spaces");
        assert_eq!(actions[0].is_preferred, Some(true));

        // Second action should be ignore-line
        assert_eq!(actions[1].title, "Ignore MD009 for this line");
        assert_eq!(actions[1].is_preferred, Some(false));
    }

    #[test]
    fn test_warning_to_code_actions_no_fix() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            rule_name: Some("MD033".to_string()),
            message: "Inline HTML".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "<div>HTML</div>";

        let actions = warning_to_code_actions(&warning, &uri, document_text);

        // Should have 1 action: ignore-line only (no fix available)
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Ignore MD033 for this line");
        assert_eq!(actions[0].is_preferred, Some(false));
    }

    #[test]
    fn test_warning_to_code_actions_no_rule_name() {
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

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        let actions = warning_to_code_actions(&warning, &uri, document_text);

        // Should have no actions (no rule name means can't create ignore comment)
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_legacy_warning_to_code_action_compatibility() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "Fixed".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "Hello World";

        #[allow(deprecated)]
        let action = warning_to_code_action(&warning, &uri, document_text);

        // Should return the preferred (fix) action
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.title, "Fix: Test");
        assert_eq!(action.is_preferred, Some(true));
    }

    #[test]
    fn test_md034_convert_to_link_action() {
        // Test the "convert to markdown link" action for MD034 bare URLs
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 25,
            rule_name: Some("MD034".to_string()),
            message: "URL without angle brackets or link formatting: 'https://example.com'".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..20, // "https://example.com"
                replacement: "<https://example.com>".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "https://example.com is a test URL";

        let actions = warning_to_code_actions(&warning, &uri, document_text);

        // Should have 3 actions: fix (angle brackets), convert to link, and ignore
        assert_eq!(actions.len(), 3);

        // First action should be the fix (angle brackets) - preferred
        assert_eq!(
            actions[0].title,
            "Fix: URL without angle brackets or link formatting: 'https://example.com'"
        );
        assert_eq!(actions[0].is_preferred, Some(true));

        // Second action should be convert to link - not preferred
        assert_eq!(actions[1].title, "Convert to markdown link");
        assert_eq!(actions[1].is_preferred, Some(false));

        // Check that the convert action creates a proper markdown link
        let edit = actions[1].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let file_edits = changes.get(&uri).unwrap();
        assert_eq!(file_edits.len(), 1);

        // The replacement should be: [example.com](https://example.com)
        assert_eq!(file_edits[0].new_text, "[example.com](https://example.com)");

        // Third action should be ignore
        assert_eq!(actions[2].title, "Ignore MD034 for this line");
    }

    #[test]
    fn test_md034_convert_to_link_action_email() {
        // Test the "convert to markdown link" action for MD034 bare emails
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 20,
            rule_name: Some("MD034".to_string()),
            message: "Email address without angle brackets or link formatting: 'user@example.com'".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..16, // "user@example.com"
                replacement: "<user@example.com>".to_string(),
            }),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "user@example.com is my email";

        let actions = warning_to_code_actions(&warning, &uri, document_text);

        // Should have 3 actions
        assert_eq!(actions.len(), 3);

        // Check convert to link action
        assert_eq!(actions[1].title, "Convert to markdown link");

        let edit = actions[1].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let file_edits = changes.get(&uri).unwrap();

        // For emails, use the whole email as link text
        assert_eq!(file_edits[0].new_text, "[user@example.com](user@example.com)");
    }

    #[test]
    fn test_extract_url_from_fix_replacement() {
        assert_eq!(
            extract_url_from_fix_replacement("<https://example.com>"),
            Some("https://example.com")
        );
        assert_eq!(
            extract_url_from_fix_replacement("<user@example.com>"),
            Some("user@example.com")
        );
        assert_eq!(extract_url_from_fix_replacement("https://example.com"), None);
        assert_eq!(extract_url_from_fix_replacement("<>"), Some(""));
    }

    #[test]
    fn test_extract_domain_for_placeholder() {
        assert_eq!(extract_domain_for_placeholder("https://example.com"), "example.com");
        assert_eq!(
            extract_domain_for_placeholder("https://example.com/path/to/page"),
            "example.com"
        );
        assert_eq!(
            extract_domain_for_placeholder("http://sub.example.com:8080/"),
            "sub.example.com:8080"
        );
        assert_eq!(extract_domain_for_placeholder("user@example.com"), "user@example.com");
        assert_eq!(
            extract_domain_for_placeholder("ftp://files.example.com"),
            "files.example.com"
        );
    }

    #[test]
    fn test_byte_range_to_lsp_range_trailing_newlines() {
        // Test converting byte ranges for MD012 trailing blank line fixes
        let text = "line1\nline2\n\n"; // 13 bytes: "line1\n" (6) + "line2\n" (6) + "\n" (1)

        // Remove the last blank line (byte 12..13)
        let range = byte_range_to_lsp_range(text, 12..13);
        assert!(range.is_some());
        let range = range.unwrap();

        // Should be on line 2 (0-indexed), at position 0 for start
        // End should be on line 3 (after the newline at byte 12)
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 3);
        assert_eq!(range.end.character, 0);
    }

    #[test]
    fn test_byte_range_to_lsp_range_at_eof() {
        // Test a range that starts at EOF (empty range)
        let text = "test\n"; // 5 bytes

        // Try to convert a range starting at EOF (should handle gracefully)
        let range = byte_range_to_lsp_range(text, 5..5);
        assert!(range.is_some());
        let range = range.unwrap();

        // Should be at line 1 (after newline), position 0
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
    }
}
