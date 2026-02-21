//! Code completion for the LSP server
//!
//! Provides two categories of completion:
//!
//! - **Code fence language** — triggered by `` ` `` after a fenced code block opening,
//!   using GitHub Linguist data and respecting MD040 configuration.
//!
//! - **Link target** — triggered by `(` or `#` inside a markdown link `[text](…)`,
//!   offering relative file paths (from the workspace index) and heading anchors.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::*;

use crate::linguist_data::{CANONICAL_TO_ALIASES, default_alias};
use crate::rule_config_serde::load_rule_config;
use crate::rules::md040_fenced_code_language::md040_config::MD040Config;

use super::server::RumdlLanguageServer;

/// Position detected for link target completion
///
/// Returned by [`RumdlLanguageServer::detect_link_target_position`] when
/// the cursor is inside a markdown link target `[text](…)`.
pub(crate) struct LinkTargetInfo {
    /// Content between `](` and the cursor (the file path portion, before any `#`)
    pub(crate) file_path: String,
    /// LSP column (UTF-16) immediately after `](`; used as the start of text edits
    pub(crate) path_start_col: u32,
    /// When the cursor is past a `#`: `(partial_anchor_text, column_after_hash)`
    pub(crate) anchor: Option<(String, u32)>,
}

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
        let utf16_cursor = position.character as usize;

        // Get the line content
        let lines: Vec<&str> = text.lines().collect();
        if line_num >= lines.len() {
            return None;
        }
        let line = lines[line_num];
        let trimmed = line.trim_start();

        // `indent` and `fence_len` are counts of ASCII characters, so byte
        // offset == UTF-8 byte offset == UTF-16 code unit offset for this prefix.
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

        // fence_end is a byte offset here; because indent and fence_len are
        // both counts of ASCII characters, it equals the UTF-16 column too.
        let fence_end_byte = indent + fence_len;

        // The cursor (UTF-16) must be at or past the fence end (also UTF-16/ASCII).
        if utf16_cursor < fence_end_byte {
            return None;
        }

        // Check if this is an opening or closing fence by scanning previous lines
        let is_closing_fence = Self::is_closing_fence(&lines[..line_num], fence_char, fence_len);
        if is_closing_fence {
            return None;
        }

        // Convert the UTF-16 cursor to a byte offset for slicing the language text.
        let byte_cursor = utf16_to_byte_offset(line, utf16_cursor).unwrap_or(line.len());

        // Extract the current language text (from fence end to cursor position)
        let current_text = &line[fence_end_byte..byte_cursor.min(line.len())];

        // Don't complete if there's a space (info string contains more than just language)
        if current_text.contains(' ') {
            return None;
        }

        // Return fence_end as a UTF-16 column. Since the fence is all ASCII,
        // byte offset == UTF-16 offset.
        Some((fence_end_byte as u32, current_text.to_string()))
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

    /// Detect if the cursor is inside a markdown link target `[text](…)`
    ///
    /// Scans backward from the cursor on the current line to find a `](` opening.
    /// Returns `Some(LinkTargetInfo)` with the partial path / anchor text and the
    /// LSP column position to use as the start of the text edit, or `None` when
    /// the cursor is not in a link target context.
    ///
    /// All column positions in the returned `LinkTargetInfo` are UTF-16 code unit
    /// offsets, as required by the LSP specification.
    pub(super) fn detect_link_target_position(text: &str, position: Position) -> Option<LinkTargetInfo> {
        let line_num = position.line as usize;
        let utf16_cursor = position.character as usize;

        let lines: Vec<&str> = text.lines().collect();
        if line_num >= lines.len() {
            return None;
        }
        let line = lines[line_num];

        // Convert the UTF-16 cursor offset to a byte offset for string slicing.
        let byte_cursor = utf16_to_byte_offset(line, utf16_cursor)?;

        let before_cursor = &line[..byte_cursor];

        // Find the last `](` on this line before the cursor
        let link_open = before_cursor.rfind("](")?;
        let content_start = link_open + 2; // first byte after `](`
        let content = &before_cursor[content_start..];

        // Link is already closed — no completion inside a finished `](…)`
        if content.contains(')') {
            return None;
        }

        // Heuristic: odd number of backticks before `](` suggests we're inside a
        // code span; skip completion in that context.
        let backtick_count = before_cursor[..link_open].chars().filter(|&c| c == '`').count();
        if backtick_count % 2 != 0 {
            return None;
        }

        // Convert byte positions back to UTF-16 offsets for LSP TextEdit ranges.
        let path_start_col = byte_to_utf16_offset(line, content_start);

        if let Some(hash_pos) = content.find('#') {
            let file_path = content[..hash_pos].to_string();
            let partial_anchor = content[hash_pos + 1..].to_string();
            let anchor_start_col = byte_to_utf16_offset(line, content_start + hash_pos + 1);
            Some(LinkTargetInfo {
                file_path,
                path_start_col,
                anchor: Some((partial_anchor, anchor_start_col)),
            })
        } else {
            Some(LinkTargetInfo {
                file_path: content.to_string(),
                path_start_col,
                anchor: None,
            })
        }
    }

    /// Get relative file path completion items for a markdown link target
    ///
    /// Enumerates all markdown files in the workspace index, computes their path
    /// relative to the current document's directory, and returns those whose
    /// prefix matches `partial_path`.
    pub(super) async fn get_file_completions(
        &self,
        uri: &Url,
        partial_path: &str,
        start_col: u32,
        position: Position,
    ) -> Vec<CompletionItem> {
        let current_file = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        let current_dir = match current_file.parent() {
            Some(d) => d.to_path_buf(),
            None => return Vec::new(),
        };

        let index = self.workspace_index.read().await;
        let mut items = Vec::new();
        let partial_lower = partial_path.to_lowercase();

        for (file_path, _) in index.files() {
            // Exclude the document being edited
            if file_path == current_file.as_path() {
                continue;
            }

            let rel = make_relative_path(&current_dir, file_path);
            // Normalise path separators: markdown links always use forward slashes
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if !partial_path.is_empty() && !rel_str.to_lowercase().starts_with(&partial_lower) {
                continue;
            }

            let item = CompletionItem {
                label: rel_str.clone(),
                kind: Some(CompletionItemKind::FILE),
                detail: Some("Markdown file".to_string()),
                sort_text: Some(rel_str.clone()),
                filter_text: Some(rel_str.clone()),
                insert_text: Some(rel_str.clone()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: Range {
                        start: Position {
                            line: position.line,
                            character: start_col,
                        },
                        end: position,
                    },
                    new_text: rel_str.to_string(),
                })),
                ..Default::default()
            };
            items.push(item);
        }

        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.truncate(50);
        items
    }

    /// Get heading anchor completion items for a markdown link target
    ///
    /// Resolves `file_path` relative to the current document, looks up its
    /// `FileIndex` in the workspace index, and returns one `CompletionItem` per
    /// heading whose anchor starts with `partial_anchor`.
    pub(super) async fn get_anchor_completions(
        &self,
        uri: &Url,
        file_path: &str,
        partial_anchor: &str,
        start_col: u32,
        position: Position,
    ) -> Vec<CompletionItem> {
        let current_file = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        // Resolve the target file: empty path means the current file itself
        let target = if file_path.is_empty() {
            current_file.clone()
        } else {
            let current_dir = match current_file.parent() {
                Some(d) => d.to_path_buf(),
                None => return Vec::new(),
            };
            normalize_path(current_dir.join(file_path))
        };

        let index = self.workspace_index.read().await;
        let file_index = match index.get_file(&target) {
            Some(fi) => fi,
            None => return Vec::new(),
        };

        let partial_lower = partial_anchor.to_lowercase();
        let mut items = Vec::new();

        for heading in &file_index.headings {
            let anchor = heading.custom_anchor.as_deref().unwrap_or(&heading.auto_anchor);

            if !partial_anchor.is_empty() && !anchor.to_lowercase().starts_with(&partial_lower) {
                continue;
            }

            let item = CompletionItem {
                label: heading.text.clone(),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(format!("#{anchor}")),
                // Sort by line number to preserve document order
                sort_text: Some(format!("{:06}", heading.line)),
                filter_text: Some(anchor.to_string()),
                insert_text: Some(anchor.to_string()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: Range {
                        start: Position {
                            line: position.line,
                            character: start_col,
                        },
                        end: position,
                    },
                    new_text: anchor.to_string(),
                })),
                ..Default::default()
            };
            items.push(item);
        }

        items.truncate(50);
        items
    }
}

// =============================================================================
// Path helpers (free functions, not methods)
// =============================================================================

/// Compute the relative path from `from_dir` to `to_file`.
///
/// Both arguments should be absolute paths. Traverses up with `..` components
/// from the common ancestor to the target.
fn make_relative_path(from_dir: &Path, to_file: &Path) -> PathBuf {
    let from_comps: Vec<_> = from_dir.components().collect();
    let to_comps: Vec<_> = to_file.components().collect();

    let common_len = from_comps
        .iter()
        .zip(to_comps.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut rel = PathBuf::new();
    for _ in &from_comps[common_len..] {
        rel.push("..");
    }
    for comp in &to_comps[common_len..] {
        rel.push(comp);
    }
    rel
}

/// Resolve `..` and `.` components in a path without touching the filesystem.
fn normalize_path(path: PathBuf) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            c => result.push(c),
        }
    }
    result
}

// =============================================================================
// UTF-16 / UTF-8 offset helpers
// =============================================================================

/// Convert a UTF-16 code unit offset to the corresponding byte offset in a UTF-8 string.
///
/// Returns `None` if `utf16_offset` is beyond the end of the string.
fn utf16_to_byte_offset(s: &str, utf16_offset: usize) -> Option<usize> {
    let mut byte_pos = 0;
    let mut utf16_pos = 0;
    for ch in s.chars() {
        if utf16_pos >= utf16_offset {
            return Some(byte_pos);
        }
        byte_pos += ch.len_utf8();
        utf16_pos += ch.len_utf16();
    }
    // Cursor at the very end of the string is valid.
    if utf16_pos >= utf16_offset {
        Some(byte_pos)
    } else {
        None
    }
}

/// Convert a byte offset to the corresponding UTF-16 code unit offset in a UTF-8 string.
///
/// Panics if `byte_offset` is not on a character boundary.
fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
}
