//! LSP type definitions and utilities for rumdl
//!
//! This module contains LSP-specific types and utilities for rumdl,
//! following the Language Server Protocol specification.

use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::*;

/// Configuration for the rumdl LSP server
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    // For now, assume single character range - we can improve this later
    let end_position = Position {
        line: start_position.line,
        character: start_position.character + 1,
    };

    let severity = match warning.severity {
        crate::rule::Severity::Error => DiagnosticSeverity::ERROR,
        crate::rule::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    Diagnostic {
        range: Range {
            start: start_position,
            end: end_position,
        },
        severity: Some(severity),
        code: warning.rule_name.clone().map(|s| NumberOrString::String(s.to_string())),
        source: Some("rumdl".to_string()),
        message: warning.message.clone(),
        related_information: None,
        tags: None,
        code_description: None,
        data: None,
    }
}

/// Create a code action from a rumdl warning with fix
pub fn warning_to_code_action(
    warning: &crate::rule::LintWarning,
    uri: &Url,
) -> Option<CodeAction> {
    if let Some(fix) = &warning.fix {
        let range = Range {
            start: Position {
                line: (warning.line.saturating_sub(1)) as u32,
                character: (warning.column.saturating_sub(1)) as u32,
            },
            end: Position {
                line: (warning.line.saturating_sub(1)) as u32,
                character: (warning.column.saturating_sub(1)) as u32 + fix.replacement.len() as u32,
            },
        };

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