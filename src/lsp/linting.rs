//! Document linting, diagnostics, code actions, and auto-fix
//!
//! Handles the core linting workflow: running rules against documents,
//! converting warnings to LSP diagnostics, generating code actions,
//! and applying automatic fixes.

use anyhow::Result;
use tower_lsp::lsp_types::*;

use crate::lint;
use crate::rule::FixCapability;
use crate::rules;

use super::server::RumdlLanguageServer;
use super::types::{IndexState, warning_to_code_actions_with_md013_config, warning_to_diagnostic};
use crate::rules::md013_line_length::MD013Config;

impl RumdlLanguageServer {
    /// Check if a file URI should be excluded based on exclude patterns
    pub(super) async fn should_exclude_uri(&self, uri: &Url) -> bool {
        // Try to convert URI to file path
        let file_path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => return false, // If we can't get a path, don't exclude
        };

        // Resolve configuration for this specific file to get its exclude patterns
        let rumdl_config = self.resolve_config_for_file(&file_path).await;
        let exclude_patterns = &rumdl_config.global.exclude;

        // If no exclude patterns, don't exclude
        if exclude_patterns.is_empty() {
            return false;
        }

        // Convert path to relative path for pattern matching
        // This matches the CLI behavior in find_markdown_files
        let path_to_check = if file_path.is_absolute() {
            // Try to make it relative to the current directory
            if let Ok(cwd) = std::env::current_dir() {
                // Canonicalize both paths to handle symlinks
                if let (Ok(canonical_cwd), Ok(canonical_path)) = (cwd.canonicalize(), file_path.canonicalize()) {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_cwd) {
                        relative.to_string_lossy().to_string()
                    } else {
                        // Path is absolute but not under cwd
                        file_path.to_string_lossy().to_string()
                    }
                } else {
                    // Canonicalization failed
                    file_path.to_string_lossy().to_string()
                }
            } else {
                file_path.to_string_lossy().to_string()
            }
        } else {
            // Already relative
            file_path.to_string_lossy().to_string()
        };

        // Check if path matches any exclude pattern
        for pattern in exclude_patterns {
            if let Ok(glob) = globset::Glob::new(pattern) {
                let matcher = glob.compile_matcher();
                if matcher.is_match(&path_to_check) {
                    log::debug!("Excluding file from LSP linting: {path_to_check}");
                    return true;
                }
            }
        }

        false
    }

    /// Lint a document and return diagnostics
    pub(crate) async fn lint_document(&self, uri: &Url, text: &str) -> Result<Vec<Diagnostic>> {
        let config_guard = self.config.read().await;

        // Skip linting if disabled
        if !config_guard.enable_linting {
            return Ok(Vec::new());
        }

        let lsp_config = config_guard.clone();
        drop(config_guard); // Release config lock early

        // Check if file should be excluded based on exclude patterns
        if self.should_exclude_uri(uri).await {
            return Ok(Vec::new());
        }

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // Run rumdl linting with the configured flavor
        let mut all_warnings = match crate::lint(text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => warnings,
            Err(e) => {
                log::error!("Failed to lint document {uri}: {e}");
                return Ok(Vec::new());
            }
        };

        // Run cross-file checks if workspace index is ready
        if let Some(ref path) = file_path {
            let index_state = self.index_state.read().await.clone();
            if matches!(index_state, IndexState::Ready) {
                let workspace_index = self.workspace_index.read().await;
                if let Some(file_index) = workspace_index.get_file(path) {
                    match crate::run_cross_file_checks(
                        path,
                        file_index,
                        &filtered_rules,
                        &workspace_index,
                        Some(&rumdl_config),
                    ) {
                        Ok(cross_file_warnings) => {
                            all_warnings.extend(cross_file_warnings);
                        }
                        Err(e) => {
                            log::warn!("Failed to run cross-file checks for {uri}: {e}");
                        }
                    }
                }
            }
        }

        let diagnostics = all_warnings.iter().map(warning_to_diagnostic).collect();
        Ok(diagnostics)
    }

    /// Update diagnostics for a document
    ///
    /// This method pushes diagnostics to the client via publishDiagnostics.
    /// When the client supports pull diagnostics (textDocument/diagnostic),
    /// we skip pushing to avoid duplicate diagnostics.
    pub(super) async fn update_diagnostics(&self, uri: Url, text: String) {
        // Skip pushing if client supports pull diagnostics to avoid duplicates
        if *self.client_supports_pull_diagnostics.read().await {
            log::debug!("Skipping push diagnostics for {uri} - client supports pull model");
            return;
        }

        // Get the document version if available
        let version = {
            let docs = self.documents.read().await;
            docs.get(&uri).and_then(|entry| entry.version)
        };

        match self.lint_document(&uri, &text).await {
            Ok(diagnostics) => {
                self.client.publish_diagnostics(uri, diagnostics, version).await;
            }
            Err(e) => {
                log::error!("Failed to update diagnostics: {e}");
            }
        }
    }

    /// Apply all available fixes to a document
    pub(super) async fn apply_all_fixes(&self, uri: &Url, text: &str) -> Result<Option<String>> {
        // Check if file should be excluded based on exclude patterns
        if self.should_exclude_uri(uri).await {
            return Ok(None);
        }

        let config_guard = self.config.read().await;
        let lsp_config = config_guard.clone();
        drop(config_guard);

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // First, run lint to get active warnings (respecting ignore comments)
        // This tells us which rules actually have unfixed issues
        let mut rules_with_warnings = std::collections::HashSet::new();
        let mut fixed_text = text.to_string();

        match lint(&fixed_text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => {
                for warning in warnings {
                    if let Some(rule_name) = &warning.rule_name {
                        rules_with_warnings.insert(rule_name.clone());
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to lint document for auto-fix: {e}");
                return Ok(None);
            }
        }

        // Early return if no warnings to fix
        if rules_with_warnings.is_empty() {
            return Ok(None);
        }

        // Only apply fixes for rules that have active warnings
        let mut any_changes = false;

        for rule in &filtered_rules {
            // Skip rules that don't have any active warnings
            if !rules_with_warnings.contains(rule.name()) {
                continue;
            }

            let ctx = crate::lint_context::LintContext::new(&fixed_text, flavor, None);
            match rule.fix(&ctx) {
                Ok(new_text) => {
                    if new_text != fixed_text {
                        fixed_text = new_text;
                        any_changes = true;
                    }
                }
                Err(e) => {
                    // Only log if it's an actual error, not just "rule doesn't support auto-fix"
                    let msg = e.to_string();
                    if !msg.contains("does not support automatic fixing") {
                        log::warn!("Failed to apply fix for rule {}: {}", rule.name(), e);
                    }
                }
            }
        }

        if any_changes { Ok(Some(fixed_text)) } else { Ok(None) }
    }

    /// Get the end position of a document
    pub(super) fn get_end_position(&self, text: &str) -> Position {
        let mut line = 0u32;
        let mut character = 0u32;

        for ch in text.chars() {
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }

        Position { line, character }
    }

    /// Apply LSP FormattingOptions to content
    ///
    /// This implements the standard LSP formatting options that editors send:
    /// - `trim_trailing_whitespace`: Remove trailing whitespace from each line
    /// - `insert_final_newline`: Ensure file ends with a newline
    /// - `trim_final_newlines`: Remove extra blank lines at end of file
    ///
    /// This is applied AFTER lint fixes to ensure we respect editor preferences
    /// even when the editor's buffer content differs from the file on disk
    /// (e.g., nvim may strip trailing newlines from its buffer representation).
    pub(super) fn apply_formatting_options(content: String, options: &FormattingOptions) -> String {
        // If the original content is empty, keep it empty regardless of options
        // This prevents marking empty documents as needing formatting
        if content.is_empty() {
            return content;
        }

        let mut result = content.clone();
        let original_ended_with_newline = content.ends_with('\n');

        // 1. Trim trailing whitespace from each line (if requested)
        if options.trim_trailing_whitespace.unwrap_or(false) {
            result = result
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            // Preserve final newline status for next steps
            if original_ended_with_newline && !result.ends_with('\n') {
                result.push('\n');
            }
        }

        // 2. Trim final newlines (remove extra blank lines at EOF)
        // This runs BEFORE insert_final_newline to handle the case where
        // we have multiple trailing newlines and want exactly one
        if options.trim_final_newlines.unwrap_or(false) {
            // Remove all trailing newlines
            while result.ends_with('\n') {
                result.pop();
            }
            // We'll add back exactly one in the next step if insert_final_newline is true
        }

        // 3. Insert final newline (ensure file ends with exactly one newline)
        if options.insert_final_newline.unwrap_or(false) && !result.ends_with('\n') {
            result.push('\n');
        }

        result
    }

    /// Get code actions for diagnostics at a position
    pub(super) async fn get_code_actions(&self, uri: &Url, text: &str, range: Range) -> Result<Vec<CodeAction>> {
        let config_guard = self.config.read().await;
        let lsp_config = config_guard.clone();
        drop(config_guard);

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // Extract MD013 config once so the "Reflow paragraph" action respects user settings.
        let mut md013_config = crate::rule_config_serde::load_rule_config::<MD013Config>(&rumdl_config);
        if md013_config.line_length.get() == 80 {
            md013_config.line_length = rumdl_config.global.line_length;
        }

        match crate::lint(text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => {
                let mut actions = Vec::new();
                let mut fixable_count = 0;

                for warning in &warnings {
                    // Check if warning is within the requested range
                    let warning_line = (warning.line.saturating_sub(1)) as u32;
                    if warning_line >= range.start.line && warning_line <= range.end.line {
                        // Get all code actions for this warning (fix + ignore actions)
                        let mut warning_actions =
                            warning_to_code_actions_with_md013_config(warning, uri, text, Some(&md013_config));
                        actions.append(&mut warning_actions);

                        if warning.fix.is_some() {
                            fixable_count += 1;
                        }
                    }
                }

                // Add "Fix all" action if there are multiple fixable issues in range
                if fixable_count > 1 {
                    // Only apply fixes from fixable rules during "Fix all"
                    // Unfixable rules provide warning-level fixes for individual Quick Fix actions
                    let fixable_warnings: Vec<_> = warnings
                        .iter()
                        .filter(|w| {
                            if let Some(rule_name) = &w.rule_name {
                                filtered_rules
                                    .iter()
                                    .find(|r| r.name() == rule_name)
                                    .map(|r| r.fix_capability() != FixCapability::Unfixable)
                                    .unwrap_or(false)
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect();

                    // Count total fixable issues (excluding Unfixable rules)
                    let total_fixable = fixable_warnings.len();

                    if let Ok(fixed_content) = crate::utils::fix_utils::apply_warning_fixes(text, &fixable_warnings)
                        && fixed_content != text
                    {
                        // Calculate proper end position
                        let mut line = 0u32;
                        let mut character = 0u32;
                        for ch in text.chars() {
                            if ch == '\n' {
                                line += 1;
                                character = 0;
                            } else {
                                character += 1;
                            }
                        }

                        let fix_all_action = CodeAction {
                            title: format!("Fix all rumdl issues ({total_fixable} fixable)"),
                            kind: Some(CodeActionKind::new("source.fixAll.rumdl")),
                            diagnostics: Some(Vec::new()),
                            edit: Some(WorkspaceEdit {
                                changes: Some(
                                    [(
                                        uri.clone(),
                                        vec![TextEdit {
                                            range: Range {
                                                start: Position { line: 0, character: 0 },
                                                end: Position { line, character },
                                            },
                                            new_text: fixed_content,
                                        }],
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                                ..Default::default()
                            }),
                            command: None,
                            is_preferred: Some(true),
                            disabled: None,
                            data: None,
                        };

                        // Insert at the beginning to make it prominent
                        actions.insert(0, fix_all_action);
                    }
                }

                Ok(actions)
            }
            Err(e) => {
                log::error!("Failed to get code actions: {e}");
                Ok(Vec::new())
            }
        }
    }
}
