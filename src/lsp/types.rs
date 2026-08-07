//! LSP type definitions and utilities for rumdl
//!
//! This module contains LSP-specific types and utilities for rumdl,
//! following the Language Server Protocol specification.

use super::position::{byte_range_to_lsp_range, char_column_to_utf16, utf16_len};
use crate::rules::md013_line_length::MD013Config;
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

/// A request from the background index worker asking the server to publish a
/// document's diagnostics again.
///
/// Cross-file diagnostics are computed from the workspace index, so their
/// answers can change with no editor event to recompute them: the file that
/// changed is a different one, or the change is the initial scan finishing
/// after a document was already opened and linted without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelintRequest {
    /// Re-lint this file, if the editor has it open.
    File(PathBuf),
    /// Re-lint every open document, for a change to the index as a whole that
    /// no per-file request describes.
    AllOpen,
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
    /// Enable file path and heading anchor completions inside markdown link targets
    /// When true, typing `](` triggers file path suggestions and `#` triggers anchor suggestions
    pub enable_link_completions: bool,
    /// Enable hover preview, go-to-definition, find-references, and rename for markdown links
    /// When false, rumdl will not respond to these requests, avoiding conflicts with other LSPs
    /// that provide the same features (e.g., PKM-focused LSPs)
    pub enable_link_navigation: bool,
    /// Enable the document and workspace symbol providers (the heading outline and
    /// cross-file heading search). When false, rumdl advertises neither symbol
    /// capability and answers neither request, avoiding duplicate heading entries
    /// when another Markdown LSP (e.g. marksman, markdown-oxide) already provides
    /// the outline. Takes effect when the server (re)starts, like the other
    /// capability flags.
    pub enable_symbols: bool,
    /// Content roots for absolute-style link completion (e.g. `/img/01.webp`).
    /// Each entry is an absolute path, or a path relative to the workspace root.
    /// When empty, the workspace root folders are used.
    pub link_completion_content_roots: Vec<String>,
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
            enable_link_completions: true,
            enable_link_navigation: true,
            enable_symbols: true,
            link_completion_content_roots: Vec::new(),
        }
    }
}

/// Convert rumdl warnings to LSP diagnostics.
///
/// The document text is needed to place the columns: a warning column counts
/// characters and an LSP position counts UTF-16 code units.
pub fn warnings_to_diagnostics(warnings: &[crate::rule::LintWarning], document_text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = document_text.lines().collect();
    warnings.iter().map(|warning| diagnostic_in(warning, &lines)).collect()
}

/// Convert a single rumdl warning to an LSP diagnostic.
///
/// [`warnings_to_diagnostics`] splits the document once for a whole batch;
/// prefer it when converting more than one warning.
pub fn warning_to_diagnostic(warning: &crate::rule::LintWarning, document_text: &str) -> Diagnostic {
    let lines: Vec<&str> = document_text.lines().collect();
    diagnostic_in(warning, &lines)
}

fn diagnostic_in(warning: &crate::rule::LintWarning, lines: &[&str]) -> Diagnostic {
    let start_line = warning.line.saturating_sub(1);
    let end_line = warning.end_line.saturating_sub(1);

    let start_position = Position {
        line: start_line as u32,
        character: char_column_to_utf16(lines.get(start_line).copied(), warning.column),
    };

    // Use proper range from warning
    let end_position = Position {
        line: end_line as u32,
        character: char_column_to_utf16(lines.get(end_line).copied(), warning.end_column),
    };

    let severity = match warning.severity {
        crate::rule::Severity::Error => DiagnosticSeverity::ERROR,
        crate::rule::Severity::Warning => DiagnosticSeverity::WARNING,
        crate::rule::Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    // Only generate documentation URLs for rumdl rule names (MD001, MD007, etc.),
    // not for external tool names (jq, tombi, shellcheck, etc.)
    let code_description = warning.rule_name.as_ref().and_then(|rule_name| {
        let is_rumdl_rule = rule_name.len() > 2
            && rule_name[..2].eq_ignore_ascii_case("MD")
            && rule_name[2..].chars().all(|c| c.is_ascii_digit());
        if is_rumdl_rule {
            Url::parse(&format!("https://rumdl.dev/{}/", rule_name.to_lowercase()))
                .ok()
                .map(|href| CodeDescription { href })
        } else {
            None
        }
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

/// Create code actions from a rumdl warning
/// Returns a vector of available actions: fix action (if available) and ignore actions
pub fn warning_to_code_actions(warning: &crate::rule::LintWarning, uri: &Url, document_text: &str) -> Vec<CodeAction> {
    warning_to_code_actions_with_md013_config(warning, uri, document_text, None)
}

/// Like [`warning_to_code_actions`] but uses the provided MD013 configuration when
/// generating the "Reflow paragraph" action, so the LSP action respects user-configured
/// reflow mode, abbreviations, and length mode rather than using defaults.
pub(crate) fn warning_to_code_actions_with_md013_config(
    warning: &crate::rule::LintWarning,
    uri: &Url,
    document_text: &str,
    md013_config: Option<&MD013Config>,
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // Add fix action if available (marked as preferred)
    if let Some(fix_action) = create_fix_action(warning, uri, document_text) {
        actions.push(fix_action);
    }

    // Add manual reflow action for MD013 when no fix is available
    // This allows users to manually reflow paragraphs without enabling reflow globally
    if warning.rule_name.as_deref() == Some("MD013")
        && warning.fix.is_none()
        && let Some(reflow_action) = create_reflow_action(warning, uri, document_text, md013_config)
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
        // Build the primary edit plus any additional edits this fix carries.
        // A logical fix is atomic — either every edit applies or none should.
        // If any sub-edit's range can't be mapped to LSP positions, abort the
        // whole code action so we don't emit a partial/inconsistent fix.
        let primary = TextEdit {
            range: byte_range_to_lsp_range(document_text, fix.range.clone())?,
            new_text: fix.replacement.clone(),
        };

        let mut edits = Vec::with_capacity(1 + fix.additional_edits.len());
        edits.push(primary);
        for extra in &fix.additional_edits {
            edits.push(TextEdit {
                range: byte_range_to_lsp_range(document_text, extra.range.clone())?,
                new_text: extra.replacement.clone(),
            });
        }

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), edits);

        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        };

        Some(CodeAction {
            title: format!("Fix: {}", warning.message),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![warning_to_diagnostic(warning, document_text)]),
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
fn create_reflow_action(
    warning: &crate::rule::LintWarning,
    uri: &Url,
    document_text: &str,
    md013_config: Option<&MD013Config>,
) -> Option<CodeAction> {
    // Build reflow options from config when available, falling back to extracting
    // the line length from the warning message and using defaults for other fields.
    let options = if let Some(config) = md013_config {
        config.to_reflow_options()
    } else {
        let line_length = extract_line_length_from_message(&warning.message).unwrap_or(80);
        crate::utils::text_reflow::ReflowOptions {
            line_length,
            ..Default::default()
        }
    };

    // Use the reflow helper to find and reflow the paragraph
    let reflow_result =
        crate::utils::text_reflow::reflow_paragraph_at_line_with_options(document_text, warning.line, &options)?;

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
        diagnostics: Some(vec![warning_to_diagnostic(warning, document_text)]),
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
        diagnostics: Some(vec![warning_to_diagnostic(warning, document_text)]),
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
        character: utf16_len(line_content),
    };

    // A readable name says what the rule checks, so the comment left behind
    // explains itself without a lookup. Both spellings are accepted wherever a
    // rule is named, and the ID stands in for a rule the registry has no name for.
    let rule_label = crate::config::primary_alias(rule_id).unwrap_or(rule_id.as_str());
    let comment = format!(" <!-- rumdl-disable-line {rule_label} -->");

    let edit = TextEdit {
        range: Range {
            start: line_end,
            end: line_end,
        },
        new_text: comment,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    let title = if rule_label == rule_id {
        format!("Ignore {rule_id} for this line")
    } else {
        format!("Ignore {rule_label} ({rule_id}) for this line")
    };

    Some(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![warning_to_diagnostic(warning, document_text)]),
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
            enable_link_completions: true,
            enable_link_navigation: true,
            enable_symbols: true,
            link_completion_content_roots: Vec::new(),
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

        let diagnostic = warning_to_diagnostic(&warning, "one\ntwo\nthree\nfour\nfive: a longer line\n");

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

        let diagnostic = warning_to_diagnostic(&warning, "a line of text\n");
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

        let diagnostic = warning_to_diagnostic(&warning, "a line of text\n");
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

        let diagnostic = warning_to_diagnostic(&warning, "a line of text\n");
        assert_eq!(diagnostic.range.start.line, 0);
        assert_eq!(diagnostic.range.start.character, 0);
    }

    #[test]
    fn a_diagnostic_column_after_a_non_bmp_codepoint_counts_both_code_units() {
        // U+1F389 PARTY POPPER is one character to the linter and two UTF-16
        // code units to the client, so the word behind it sits one position
        // further right than its column.
        let text = "🎉 badword here\n";
        let warning = LintWarning {
            line: 1,
            column: 3,
            end_line: 1,
            end_column: 10,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostic = warning_to_diagnostic(&warning, text);
        assert_eq!(diagnostic.range.start.character, 3);
        assert_eq!(diagnostic.range.end.character, 10);
    }

    #[test]
    fn a_batch_of_diagnostics_places_each_column_on_its_own_line() {
        let text = "🎉 first\nplain second\n";
        let warning_of = |line: usize, column: usize| LintWarning {
            line,
            column,
            end_line: line,
            end_column: column + 1,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let diagnostics = warnings_to_diagnostics(&[warning_of(1, 3), warning_of(2, 3)], text);
        assert_eq!(diagnostics[0].range.start.character, 3);
        assert_eq!(diagnostics[1].range.start.character, 2);
    }

    #[test]
    fn an_ignore_line_action_appends_after_the_last_code_unit_of_the_line() {
        let text = "🎉 needs an ignore\n";
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 2,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let action = warning_to_code_actions(&warning, &uri, text)
            .into_iter()
            .find(|action| action.title.contains("Ignore"))
            .expect("an ignore-line action");
        let edits = action.edit.unwrap().changes.unwrap().remove(&uri).unwrap();
        // 17 characters, of which the emoji contributes two code units and
        // four bytes.
        assert_eq!(edits[0].range.start, Position { line: 0, character: 18 });
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
            fix: Some(Fix::new(0..5, "Fixed".to_string())),
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
    fn test_warning_to_code_actions_md013_blockquote_reflow_action() {
        let warning = LintWarning {
            line: 2,
            column: 1,
            end_line: 2,
            end_column: 100,
            rule_name: Some("MD013".to_string()),
            message: "Line length 95 exceeds 40 characters".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let document_text = "> This quoted paragraph starts explicitly and is intentionally long enough for reflow.\nlazy continuation line should also be included when reflow is triggered from this warning.\n";

        let actions = warning_to_code_actions(&warning, &uri, document_text);
        let reflow_action = actions
            .iter()
            .find(|action| action.title == "Reflow paragraph")
            .expect("Expected manual reflow action for MD013");

        let changes = reflow_action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .expect("Expected edits for reflow action");
        let file_edits = changes.get(&uri).expect("Expected edits for URI");
        assert_eq!(file_edits.len(), 1);
        assert!(
            file_edits[0]
                .new_text
                .lines()
                .next()
                .is_some_and(|line| line.starts_with("> ")),
            "Expected blockquote prefix in reflow output"
        );
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
            fix: Some(Fix::new(6..16, "Fixed\nContent".to_string())),
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
    fn test_warning_to_code_action_atomic_with_additional_edits() {
        // Models MD054 ref-emit: the warning's fix carries a primary edit
        // (inline-link rewrite) plus an additional_edit (append ref-def at EOF).
        // The LSP code action must surface BOTH edits as a single WorkspaceEdit
        // so the client applies them atomically — applying only the primary
        // would leave a dangling reference.
        let document_text = "See [docs](https://example.com) for details.\n";
        let primary_start = document_text.find("[docs](https://example.com)").unwrap();
        let primary_end = document_text.find(" for details").unwrap();
        let appended = "\n[docs]: https://example.com\n".to_string();

        let warning = LintWarning {
            line: 1,
            column: primary_start + 1,
            end_line: 1,
            end_column: primary_end + 1,
            rule_name: Some("MD054".to_string()),
            message: "Inconsistent link style".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix::with_additional_edits(
                primary_start..primary_end,
                "[docs]".to_string(),
                vec![Fix::new(document_text.len()..document_text.len(), appended.clone())],
            )),
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let actions = warning_to_code_actions(&warning, &uri, document_text);

        let fix_action = actions
            .iter()
            .find(|a| a.is_preferred == Some(true))
            .expect("expected a preferred fix code action for MD054 ref-emit warning");
        assert_eq!(fix_action.kind, Some(CodeActionKind::QUICKFIX));

        let edits = fix_action
            .edit
            .as_ref()
            .and_then(|w| w.changes.as_ref())
            .and_then(|c| c.get(&uri))
            .expect("WorkspaceEdit should carry edits keyed by the document URI");

        assert_eq!(
            edits.len(),
            2,
            "atomic fix must surface primary + 1 additional edit as TWO TextEdits, got {edits:?}"
        );
        assert_eq!(edits[0].new_text, "[docs]");
        assert_eq!(edits[1].new_text, appended);

        // The additional EOF-insert edit is a zero-width range at end-of-document.
        assert_eq!(edits[1].range.start, edits[1].range.end);
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

        let diagnostic = warning_to_diagnostic(&warning, "Line too long\n");
        assert!(diagnostic.code_description.is_some());

        let url = diagnostic.code_description.unwrap().href;
        assert_eq!(url.as_str(), "https://rumdl.dev/md013/");
    }

    #[test]
    fn test_no_url_for_code_block_tool_warnings() {
        // Warnings from code-block-tools use the tool name (e.g., "jq") as rule_name.
        // These should NOT produce documentation URLs since they aren't rumdl rules.
        for tool_name in &["jq", "tombi", "shellcheck", "prettier", "code-block-tools"] {
            let warning = LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 10,
                rule_name: Some(tool_name.to_string()),
                message: "some tool warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            };

            let diagnostic = warning_to_diagnostic(&warning, "some tool output\n");
            assert!(
                diagnostic.code_description.is_none(),
                "Expected no URL for tool name '{tool_name}', but got one",
            );
        }
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

        assert_eq!(action.title, "Ignore line-length (MD013) for this line");
        assert_eq!(action.is_preferred, Some(false));
        assert!(action.edit.is_some());

        // Verify the edit adds the rumdl-disable-line comment
        let edit = action.edit.unwrap();
        let changes = edit.changes.unwrap();
        let file_edits = changes.get(&uri).unwrap();

        assert_eq!(file_edits.len(), 1);
        assert_eq!(file_edits[0].new_text, " <!-- rumdl-disable-line line-length -->");
        assert!(!file_edits[0].new_text.contains("markdownlint"));

        // Verify position is at end of line
        assert_eq!(file_edits[0].range.start.line, 4); // 0-indexed line 5
        assert_eq!(file_edits[0].range.start.character, 47); // End of "This is a very long line that exceeds the limit"
    }

    /// Apply a code action's single edit to `document`.
    fn apply_ignore_line_edit(action: &CodeAction, uri: &Url, document: &str) -> String {
        let edits = action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()
            .get(uri)
            .unwrap();
        assert_eq!(edits.len(), 1);
        let edit = &edits[0];
        let mut lines: Vec<String> = document.lines().map(str::to_string).collect();
        let line = &mut lines[edit.range.start.line as usize];
        line.push_str(&edit.new_text);
        lines.join("\n")
    }

    #[test]
    fn an_ignore_line_comment_names_the_rule_in_a_form_the_linter_accepts() {
        let long_line = "word ".repeat(40);
        let document = format!("# Title\n\n{long_line}text\n");
        let uri = Url::parse("file:///test.md").unwrap();
        let rules = crate::rules::all_rules(&crate::config::Config::default());

        let before = crate::lint(
            &document,
            &rules,
            false,
            crate::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();
        let warning = before
            .iter()
            .find(|w| w.rule_name.as_deref() == Some("MD013"))
            .expect("control: the long line must be reported before the comment is added");

        let action = create_ignore_line_action(warning, &uri, &document).unwrap();
        let disabled = apply_ignore_line_edit(&action, &uri, &document);
        assert!(
            disabled.contains("<!-- rumdl-disable-line line-length -->"),
            "the comment names the rule readably, got: {disabled}"
        );

        let after = crate::lint(
            &disabled,
            &rules,
            false,
            crate::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();
        assert!(
            !after.iter().any(|w| w.rule_name.as_deref() == Some("MD013")),
            "the readable name must suppress the rule it names, got: {after:?}"
        );
    }

    #[test]
    fn an_ignore_line_comment_falls_back_to_the_id_for_a_rule_with_no_readable_name() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 2,
            rule_name: Some("MD999".to_string()),
            message: "From a rule the registry does not know".to_string(),
            severity: Severity::Warning,
            fix: None,
        };
        let uri = Url::parse("file:///test.md").unwrap();

        let action = create_ignore_line_action(&warning, &uri, "text").unwrap();
        assert_eq!(action.title, "Ignore MD999 for this line");
        let edit = action.edit.unwrap();
        let file_edits = edit.changes.unwrap();
        assert_eq!(
            file_edits.get(&uri).unwrap()[0].new_text,
            " <!-- rumdl-disable-line MD999 -->"
        );
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
            fix: Some(Fix::new(0..5, "Fixed".to_string())),
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
        assert_eq!(actions[1].title, "Ignore no-trailing-spaces (MD009) for this line");
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
        assert_eq!(actions[0].title, "Ignore no-inline-html (MD033) for this line");
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
            fix: Some(Fix::new(0..5, "Fixed".to_string())),
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
            fix: Some(Fix::new(0..20, "<https://example.com>".to_string())),
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
        assert_eq!(actions[2].title, "Ignore no-bare-urls (MD034) for this line");
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
            fix: Some(Fix::new(0..16, "<user@example.com>".to_string())),
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
}
