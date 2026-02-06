//! Code fence language completion for the LSP server
//!
//! Provides completion items for fenced code block language identifiers,
//! using GitHub Linguist data and respecting MD040 configuration.

use tower_lsp::lsp_types::*;

use crate::linguist_data::{CANONICAL_TO_ALIASES, default_alias};
use crate::rule_config_serde::load_rule_config;
use crate::rules::md040_fenced_code_language::md040_config::MD040Config;

use super::server::RumdlLanguageServer;

impl RumdlLanguageServer {
    /// Detect if the cursor is at a fenced code block language position
    ///
    /// Returns Some((start_column, current_text)) if the cursor is after ``` or ~~~
    /// where language completion should be provided.
    ///
    /// Handles:
    /// - Standard fences (``` and ~~~)
    /// - Extended fences (4+ backticks/tildes for nested code blocks)
    /// - Indented fences
    /// - Distinguishes opening vs closing fences
    pub(super) fn detect_code_fence_language_position(text: &str, position: Position) -> Option<(u32, String)> {
        let line_num = position.line as usize;
        let char_pos = position.character as usize;

        // Get the line content
        let lines: Vec<&str> = text.lines().collect();
        if line_num >= lines.len() {
            return None;
        }
        let line = lines[line_num];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Detect fence character and count consecutive fence chars
        let (fence_char, fence_len) = if trimmed.starts_with('`') {
            let count = trimmed.chars().take_while(|&c| c == '`').count();
            if count >= 3 {
                ('`', count)
            } else {
                return None;
            }
        } else if trimmed.starts_with('~') {
            let count = trimmed.chars().take_while(|&c| c == '~').count();
            if count >= 3 {
                ('~', count)
            } else {
                return None;
            }
        } else {
            return None;
        };

        let fence_start = indent;
        let fence_end = fence_start + fence_len;

        // The cursor must be after the fence
        if char_pos < fence_end {
            return None;
        }

        // Check if this is an opening or closing fence by scanning previous lines
        // A closing fence has no content after it and matches an unclosed opening fence
        let is_closing_fence = Self::is_closing_fence(&lines[..line_num], fence_char, fence_len);
        if is_closing_fence {
            return None;
        }

        // Extract the current language text (from fence end to cursor position)
        let current_text = if char_pos <= line.len() {
            &line[fence_end..char_pos]
        } else {
            &line[fence_end..]
        };

        // Don't complete if there's a space (info string contains more than just language)
        if current_text.contains(' ') {
            return None;
        }

        Some((fence_end as u32, current_text.to_string()))
    }

    /// Check if we're inside an unclosed code block (meaning current fence is closing)
    pub(super) fn is_closing_fence(previous_lines: &[&str], fence_char: char, fence_len: usize) -> bool {
        let mut open_fences: Vec<(char, usize)> = Vec::new();

        for line in previous_lines {
            let trimmed = line.trim_start();

            // Check for fence
            let (line_fence_char, line_fence_len) = if trimmed.starts_with('`') {
                let count = trimmed.chars().take_while(|&c| c == '`').count();
                if count >= 3 {
                    ('`', count)
                } else {
                    continue;
                }
            } else if trimmed.starts_with('~') {
                let count = trimmed.chars().take_while(|&c| c == '~').count();
                if count >= 3 {
                    ('~', count)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            // Check if this closes an existing fence
            if let Some(pos) = open_fences
                .iter()
                .rposition(|(c, len)| *c == line_fence_char && line_fence_len >= *len)
            {
                // Check if this is a closing fence (no content after fence chars)
                let after_fence = &trimmed[line_fence_len..].trim();
                if after_fence.is_empty() {
                    open_fences.truncate(pos);
                    continue;
                }
            }

            // This is an opening fence
            open_fences.push((line_fence_char, line_fence_len));
        }

        // Check if current fence would close any open fence
        open_fences.iter().any(|(c, len)| *c == fence_char && fence_len >= *len)
    }

    /// Get language completion items for fenced code blocks
    ///
    /// Uses GitHub Linguist data and respects MD040 config for filtering
    pub(super) async fn get_language_completions(
        &self,
        uri: &Url,
        current_text: &str,
        start_col: u32,
        position: Position,
    ) -> Vec<CompletionItem> {
        // Resolve config for this file to get MD040 settings
        let file_path = uri.to_file_path().ok();
        let config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            self.rumdl_config.read().await.clone()
        };

        // Load MD040 config
        let md040_config: MD040Config = load_rule_config(&config);

        let mut items = Vec::new();
        let current_lower = current_text.to_lowercase();

        // Collect all canonical languages and their aliases
        let mut language_entries: Vec<(String, String, bool)> = Vec::new(); // (canonical, alias, is_default)

        for (canonical, aliases) in CANONICAL_TO_ALIASES.iter() {
            // Check if language is allowed
            if !md040_config.allowed_languages.is_empty()
                && !md040_config
                    .allowed_languages
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(canonical))
            {
                continue;
            }

            // Check if language is disallowed
            if md040_config
                .disallowed_languages
                .iter()
                .any(|d| d.eq_ignore_ascii_case(canonical))
            {
                continue;
            }

            // Get preferred alias from config, or use default
            let preferred = md040_config
                .preferred_aliases
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(canonical))
                .map(|(_, v)| v.clone())
                .or_else(|| default_alias(canonical).map(|s| s.to_string()))
                .unwrap_or_else(|| (*canonical).to_string());

            // Add the preferred alias as primary completion
            language_entries.push(((*canonical).to_string(), preferred.clone(), true));

            // Add other aliases as secondary completions
            for &alias in aliases.iter() {
                if alias != preferred {
                    language_entries.push(((*canonical).to_string(), alias.to_string(), false));
                }
            }
        }

        // Filter by current text prefix
        for (canonical, alias, is_default) in language_entries {
            if !current_text.is_empty() && !alias.to_lowercase().starts_with(&current_lower) {
                continue;
            }

            let sort_priority = if is_default { "0" } else { "1" };

            let item = CompletionItem {
                label: alias.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some(format!("{canonical} (GitHub Linguist)")),
                documentation: None,
                sort_text: Some(format!("{sort_priority}{alias}")),
                filter_text: Some(alias.clone()),
                insert_text: Some(alias.clone()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: Range {
                        start: Position {
                            line: position.line,
                            character: start_col,
                        },
                        end: position,
                    },
                    new_text: alias,
                })),
                ..Default::default()
            };
            items.push(item);
        }

        // Limit results to prevent overwhelming the editor
        items.truncate(100);
        items
    }
}
