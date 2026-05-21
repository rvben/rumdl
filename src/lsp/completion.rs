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
                .or_else(|| default_alias(canonical).map(std::string::ToString::to_string))
                .unwrap_or_else(|| (*canonical).to_string());

            // Add the preferred alias as primary completion
            language_entries.push(((*canonical).to_string(), preferred.clone(), true));

            // Add other aliases as secondary completions
            for &alias in aliases {
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

    /// Get file path completion items for a markdown link target.
    ///
    /// Two modes:
    ///
    /// - **Absolute** (`partial_path` starts with `/`): walks the filesystem one
    ///   directory level at a time under the configured content roots, offering
    ///   both directories and files of any type (so `/img/01.webp` completes).
    /// - **Relative** (otherwise): enumerates the markdown files in the workspace
    ///   index, computes each path relative to the current document's directory,
    ///   ranks nearer files (fewer `../` hops) first, and caps the result set.
    ///
    /// Returns a [`CompletionList`] whose `is_incomplete` flag tells the editor to
    /// re-query as the user types, so a capped relative result set still surfaces
    /// nearby files once the prefix narrows.
    pub(super) async fn get_file_completions(
        &self,
        uri: &Url,
        partial_path: &str,
        start_col: u32,
        position: Position,
    ) -> CompletionList {
        // Absolute-style links resolve against content roots, not the current file.
        if partial_path.starts_with('/') {
            return self
                .get_absolute_path_completions(partial_path, start_col, position)
                .await;
        }

        let Ok(current_file) = uri.to_file_path() else {
            return CompletionList::default();
        };
        let Some(current_dir) = current_file.parent().map(std::path::Path::to_path_buf) else {
            return CompletionList::default();
        };

        let index = self.workspace_index.read().await;
        let partial_lower = partial_path.to_lowercase();

        // Collect (distance, relative path) pairs so we can rank before truncating.
        let mut matches: Vec<(usize, String)> = Vec::new();
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

            matches.push((path_distance(&rel), rel_str));
        }

        // Nearest files first (fewest `../` hops), then alphabetical within a distance.
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        const MAX_ITEMS: usize = 50;
        let is_incomplete = matches.len() > MAX_ITEMS;
        matches.truncate(MAX_ITEMS);

        let items = matches
            .into_iter()
            .map(|(distance, rel_str)| CompletionItem {
                label: rel_str.clone(),
                kind: Some(CompletionItemKind::FILE),
                detail: Some("Markdown file".to_string()),
                // Encode distance in the sort key so the editor keeps nearer files
                // on top even when its own ordering would otherwise be lexical.
                sort_text: Some(format!("{distance:04}{rel_str}")),
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
                    new_text: rel_str.clone(),
                })),
                ..Default::default()
            })
            .collect();

        CompletionList { is_incomplete, items }
    }

    /// Complete absolute-style link targets (e.g. `/img/01.webp`) one directory
    /// level at a time, resolved against the configured content roots.
    ///
    /// Directories and files of any type are offered (not just markdown), and
    /// `.gitignore`d entries are skipped. Accepting a directory re-triggers
    /// completion so the user can drill in.
    async fn get_absolute_path_completions(
        &self,
        partial_path: &str,
        start_col: u32,
        position: Position,
    ) -> CompletionList {
        let content_roots = self.resolve_content_roots().await;
        if content_roots.is_empty() {
            return CompletionList::default();
        }

        // Split into the committed directory portion (kept) and the filename
        // prefix being typed. `/img/ic` -> dir "/img/", prefix "ic".
        let last_slash = partial_path.rfind('/').unwrap_or(0);
        let dir_part = &partial_path[..=last_slash];
        let file_prefix = &partial_path[last_slash + 1..];
        let rel_dir = dir_part.trim_start_matches('/');
        let prefix_lower = file_prefix.to_lowercase();

        // Absolute links resolve against the content roots; `..` segments could
        // escape those roots and surface unrelated files, so refuse to complete.
        if Path::new(rel_dir)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return CompletionList::default();
        }

        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();

        for root in &content_roots {
            let base = if rel_dir.is_empty() {
                root.clone()
            } else {
                normalize_path(&root.join(rel_dir))
            };

            // List only the immediate children of `base`, honoring .gitignore.
            let walker = ignore::WalkBuilder::new(&base)
                .max_depth(Some(1))
                .hidden(true)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .parents(true)
                .require_git(false)
                .build();

            for entry in walker.flatten() {
                if entry.depth() == 0 {
                    continue; // the base directory itself
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !file_prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
                    continue;
                }

                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                let new_text = if is_dir {
                    format!("{dir_part}{name}/")
                } else {
                    format!("{dir_part}{name}")
                };
                if !seen.insert(new_text.clone()) {
                    continue; // same path from another content root
                }

                let label = if is_dir { format!("{name}/") } else { name.clone() };
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: Some(if is_dir {
                        CompletionItemKind::FOLDER
                    } else {
                        CompletionItemKind::FILE
                    }),
                    detail: Some(if is_dir { "Directory" } else { "File" }.to_string()),
                    // Directories first, then alphabetical.
                    sort_text: Some(format!("{}{}", if is_dir { '0' } else { '1' }, label)),
                    // Filter against the full replacement text (e.g. `/img/icons/`)
                    // since the edit replaces the whole typed path, not just the
                    // child name; otherwise clients filter out valid items.
                    filter_text: Some(new_text.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: Range {
                            start: Position {
                                line: position.line,
                                character: start_col,
                            },
                            end: position,
                        },
                        new_text,
                    })),
                    // Re-open completion after a directory so the user keeps drilling.
                    command: is_dir.then(|| Command {
                        title: "Trigger Suggest".to_string(),
                        command: "editor.action.triggerSuggest".to_string(),
                        arguments: None,
                    }),
                    ..Default::default()
                });
            }
        }

        // Always incomplete: each new path segment needs a fresh directory listing.
        CompletionList {
            is_incomplete: true,
            items,
        }
    }

    /// Resolve the content roots used for absolute-style link completion.
    ///
    /// Uses the explicitly configured `link_completion_content_roots` when set
    /// (absolute paths as-is, relative paths joined to each workspace root),
    /// otherwise falls back to the workspace root folders.
    pub(super) async fn resolve_content_roots(&self) -> Vec<PathBuf> {
        let configured = self.config.read().await.link_completion_content_roots.clone();
        let roots = self.workspace_roots.read().await;

        if configured.is_empty() {
            return roots.clone();
        }

        let mut out = Vec::new();
        for entry in &configured {
            let path = PathBuf::from(entry);
            if path.is_absolute() {
                out.push(path);
            } else {
                for root in roots.iter() {
                    out.push(normalize_path(&root.join(&path)));
                }
            }
        }
        out
    }

    /// Resolve a markdown link's `file_path` to a target path on disk.
    ///
    /// Empty `file_path` refers to `current_file` itself. Root-relative paths
    /// (leading `/`) resolve against the content roots, mirroring the absolute
    /// link completion: an already-indexed candidate wins, otherwise the first
    /// candidate that exists on disk. `..` segments are refused so a link cannot
    /// escape a content root. Other paths resolve against the current document's
    /// directory. Shared by completion and navigation so an accepted completion
    /// always resolves the same way hover and go-to-definition resolve it.
    pub(super) async fn resolve_link_path(&self, current_file: &Path, file_path: &str) -> Option<PathBuf> {
        if file_path.is_empty() {
            return Some(current_file.to_path_buf());
        }

        if let Some(rel) = file_path.strip_prefix('/') {
            if Path::new(rel)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return None;
            }
            let content_roots = self.resolve_content_roots().await;
            let candidates: Vec<PathBuf> = content_roots
                .iter()
                .map(|root| normalize_path(&root.join(rel)))
                .collect();
            let indexed = {
                let index = self.workspace_index.read().await;
                candidates.iter().find(|c| index.get_file(c).is_some()).cloned()
            };
            return indexed.or_else(|| candidates.into_iter().find(|c| c.is_file()));
        }

        let current_dir = current_file.parent()?;
        Some(normalize_path(&current_dir.join(file_path)))
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
        let Ok(current_file) = uri.to_file_path() else {
            return Vec::new();
        };

        // Resolve the target the same way navigation does, so anchors are offered
        // for exactly the files an accepted completion would later navigate to.
        let Some(target) = self.resolve_link_path(&current_file, file_path).await else {
            return Vec::new();
        };
        // Absolute targets may live outside the indexed workspace, so allow their
        // headings to be parsed from disk; in-workspace targets must be indexed.
        let allow_disk_fallback = file_path.starts_with('/');

        // Headings come from the index when the target is indexed; otherwise an
        // absolute target is parsed from disk so anchor completion stays coherent
        // with the file-path completion that suggested it.
        let indexed_headings = {
            let index = self.workspace_index.read().await;
            index.get_file(&target).map(|fi| fi.headings.clone())
        };
        let headings = match indexed_headings {
            Some(headings) => headings,
            None if allow_disk_fallback => match tokio::fs::read_to_string(&target).await {
                Ok(content) => crate::lsp::index_worker::IndexWorker::build_file_index(&content).headings,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        };

        let partial_lower = partial_anchor.to_lowercase();
        let mut items = Vec::new();

        for heading in &headings {
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

/// Distance of a relative path from its base directory, measured as the number
/// of leading `..` components. Files in the same directory or a subdirectory
/// have distance 0; each `../` hop up the tree adds one.
fn path_distance(rel: &Path) -> usize {
    rel.components()
        .take_while(|c| matches!(c, std::path::Component::ParentDir))
        .count()
}

/// Resolve `..` and `.` components in a path without touching the filesystem.
pub(super) fn normalize_path(path: &std::path::Path) -> PathBuf {
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
pub(super) fn utf16_to_byte_offset(s: &str, utf16_offset: usize) -> Option<usize> {
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
pub(super) fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
}
