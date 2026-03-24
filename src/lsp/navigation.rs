//! Go-to-definition, find-references, and rename for markdown links
//!
//! Provides navigation features for the LSP server:
//!
//! - **Go to definition** -- jump from a `[text](file.md#heading)` link to the
//!   target file and heading.
//!
//! - **Find references** -- from a heading, find all links pointing to it across
//!   the workspace.
//!
//! - **Rename** -- rename a heading and update all links that reference it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::*;

use super::completion::{byte_to_utf16_offset, normalize_path, utf16_to_byte_offset};
use super::server::RumdlLanguageServer;
use crate::utils::anchor_styles::AnchorStyle;
use crate::workspace_index::PROTOCOL_DOMAIN_REGEX;

/// Full link target extracted from a markdown link `[text](file_path#anchor)`.
///
/// Unlike `LinkTargetInfo` (used for completion, which returns content up to the
/// cursor), this struct contains the complete file path and anchor regardless of
/// where the cursor sits within the link target.
struct FullLinkTarget {
    /// The file path portion (before `#`), may be empty for same-file anchors
    file_path: String,
    /// The anchor/fragment portion (after `#`), empty when absent
    anchor: String,
}

/// Strip a CommonMark link title from a link target.
///
/// Link titles start after whitespace followed by `"`, `'`, or `(`.
/// E.g., `guide.md "My Title"` -> `guide.md`
fn strip_link_title(target: &str) -> &str {
    for (i, _) in target.match_indices(' ') {
        let after = &target[i + 1..];
        if after.starts_with('"') || after.starts_with('\'') || after.starts_with('(') {
            return target[..i].trim_end();
        }
    }
    target
}

/// Check whether a link target is an external URL (has a protocol or `www.` prefix).
///
/// External URLs like `https://example.com` or `mailto:user@host` have no
/// corresponding local file, so go-to-definition and find-references should
/// return `None` for them.
fn is_external_url(target: &str) -> bool {
    PROTOCOL_DOMAIN_REGEX.is_match(target)
}

/// Find the position of the closing `)` that balances with the opening `(`.
///
/// CommonMark allows balanced parentheses in link destinations, e.g.
/// `[text](file(1).md)`. This helper tracks nesting depth to find the
/// correct closing paren.
fn find_balanced_close_paren(s: &str) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Detect the full link target when the cursor is anywhere inside `](...)`.
///
/// Scans backward from the cursor to find `](` and forward to find the closing
/// `)`, then extracts the complete file path and optional anchor.
fn detect_full_link_target(text: &str, position: Position) -> Option<FullLinkTarget> {
    let line_num = position.line as usize;
    let utf16_cursor = position.character as usize;

    let lines: Vec<&str> = text.lines().collect();
    if line_num >= lines.len() {
        return None;
    }
    let line = lines[line_num];

    // Convert UTF-16 cursor to byte offset
    let byte_cursor = utf16_to_byte_offset(line, utf16_cursor)?;

    let before_cursor = &line[..byte_cursor];

    // Find the last `](` before the cursor
    let link_open = before_cursor.rfind("](")?;
    let content_start = link_open + 2;

    // Find the balanced closing `)` after the content start
    let after_open = &line[content_start..];
    let close_paren = find_balanced_close_paren(after_open)?;

    let raw_content = &after_open[..close_paren];

    // Verify the cursor is within the link target (between `](` and `)`)
    let content_end = content_start + close_paren;
    if byte_cursor < content_start || byte_cursor > content_end {
        return None;
    }

    // Heuristic: odd number of backticks before `](` suggests code span
    let backtick_count = before_cursor[..link_open].chars().filter(|&c| c == '`').count();
    if backtick_count % 2 != 0 {
        return None;
    }

    // Strip angle brackets: [text](<path.md>) -> path.md
    let content = raw_content.trim();
    let content = if content.starts_with('<') && content.ends_with('>') {
        &content[1..content.len() - 1]
    } else {
        content
    };

    // Strip link title: guide.md "Title" -> guide.md
    let content = strip_link_title(content);

    // Split on first `#` to separate file path from anchor
    if let Some(hash_pos) = content.find('#') {
        Some(FullLinkTarget {
            file_path: content[..hash_pos].to_string(),
            anchor: content[hash_pos + 1..].to_string(),
        })
    } else {
        Some(FullLinkTarget {
            file_path: content.to_string(),
            anchor: String::new(),
        })
    }
}

/// Find same-file fragment-only links (e.g., `[text](#anchor)`) in the given content.
///
/// Uses pulldown-cmark to parse the document, which natively skips code blocks
/// and code spans, eliminating false positives from literal `](#...)` text.
fn find_same_file_fragment_links(content: &str, uri: &Url, anchor: &str) -> Vec<Location> {
    use pulldown_cmark::{Event, Parser, Tag};

    let options = crate::utils::rumdl_parser_options();

    let parser = Parser::new_ext(content, options).into_offset_iter();

    let mut locations = Vec::new();

    for (event, range) in parser {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            // Fragment-only: destination is exactly `#something`.
            // This catches all link types (inline, reference, collapsed, shortcut)
            // since pulldown-cmark resolves reference destinations for us.
            if let Some(frag) = dest_url.strip_prefix('#')
                && frag.eq_ignore_ascii_case(anchor)
            {
                // Convert byte offset to line/column (UTF-16 for LSP)
                let byte_start = range.start;
                let line_idx = content[..byte_start].matches('\n').count();
                let line_start = content[..byte_start].rfind('\n').map_or(0, |p| p + 1);
                let line_text = content[line_start..].split('\n').next().unwrap_or("");
                let character = byte_to_utf16_offset(line_text, byte_start - line_start);

                locations.push(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line: line_idx as u32,
                            character,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character,
                        },
                    },
                });
            }
        }
    }
    locations
}

/// Detect a reference-style link at the cursor and resolve it to a `FullLinkTarget`.
///
/// Handles three CommonMark reference link forms:
/// - Full reference: `[text][ref-id]`
/// - Collapsed reference: `[text][]`
/// - Shortcut reference: `[text]`
///
/// Also handles cursor on a reference definition line: `[ref-id]: target.md`
///
/// Returns `None` if the cursor is not on a reference link or the reference
/// definition cannot be found in the document.
fn detect_ref_link_target(text: &str, position: Position) -> Option<FullLinkTarget> {
    let line_num = position.line as usize;
    let utf16_cursor = position.character as usize;

    let lines: Vec<&str> = text.lines().collect();
    if line_num >= lines.len() {
        return None;
    }
    let line = lines[line_num];
    let byte_cursor = utf16_to_byte_offset(line, utf16_cursor)?;

    // First, check if cursor is on a reference definition line: `[ref-id]: target`
    if let Some(target) = detect_ref_definition(line) {
        return Some(target);
    }

    // Detect reference link usage and resolve it
    let ref_id = detect_ref_link_usage(line, byte_cursor)?;
    resolve_reference_to_target(text, &ref_id)
}

/// Detect whether the cursor is on a reference link usage and return the ref ID.
///
/// Recognises:
/// - `[text][ref-id]` — full reference (returns `ref-id`)
/// - `[text][]`       — collapsed reference (returns `text`)
/// - `[text]`         — shortcut reference (returns `text`, only when no `(` follows)
fn detect_ref_link_usage(line: &str, byte_cursor: usize) -> Option<String> {
    // Heuristic: odd number of backticks before cursor suggests code span
    let backtick_count = line[..byte_cursor].chars().filter(|&c| c == '`').count();
    if backtick_count % 2 != 0 {
        return None;
    }

    // Skip reference definitions (lines like `[ref]: target`)
    let trimmed = line.trim_start();
    if trimmed.starts_with('[')
        && let Some(colon_pos) = trimmed.find("]:")
        && colon_pos == trimmed.find(']').unwrap_or(0)
    {
        return None;
    }

    let before = &line[..byte_cursor];

    // Find the nearest `[` before cursor
    let open = before.rfind('[')?;

    // Find its matching `]`
    let from_open = &line[open..];
    let rel_close = from_open.find(']')?;
    let close = open + rel_close;

    // Cursor must be within this bracket pair
    if byte_cursor > close {
        return None;
    }

    let bracket_content = &line[open + 1..close];

    // Check if this bracket pair is the second part of `[text][ref-id]`
    // i.e. a `]` immediately precedes our `[`
    if open > 0 && line.as_bytes()[open - 1] == b']' {
        if bracket_content.is_empty() {
            // Collapsed: `[text][]` — ref ID is the text from the first bracket
            let text_open = line[..open - 1].rfind('[')?;
            let text_content = &line[text_open + 1..open - 1];
            return Some(text_content.to_lowercase());
        }
        // Full reference: `[text][ref-id]` — cursor is in [ref-id]
        return Some(bracket_content.to_lowercase());
    }

    // Cursor is in the first (or only) bracket pair
    let after_close = &line[close + 1..];

    // Full reference `[text][ref-id]`: cursor is on [text], extract ref-id
    if after_close.starts_with('[')
        && let Some(ref_close) = after_close[1..].find(']')
    {
        let ref_id = &after_close[1..1 + ref_close];
        if !ref_id.is_empty() {
            return Some(ref_id.to_lowercase());
        }
    }

    // Collapsed reference `[text][]`: cursor is on [text]
    if after_close.starts_with("[]") {
        return Some(bracket_content.to_lowercase());
    }

    // Shortcut reference `[text]`: no `(` or `[` follows
    if !after_close.starts_with('(') && !after_close.starts_with('[') && !bracket_content.is_empty() {
        return Some(bracket_content.to_lowercase());
    }

    None
}

/// Detect a reference definition on the current line: `[ref-id]: target.md#anchor`
///
/// Returns a `FullLinkTarget` if the line is a reference definition.
fn detect_ref_definition(line: &str) -> Option<FullLinkTarget> {
    use regex::Regex;
    use std::sync::LazyLock;

    static REF_DEF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^[ ]{0,3}\[([^\]]+)\]:\s+<?([^>\s]+)>?"#).unwrap());

    let caps = REF_DEF_RE.captures(line)?;
    let target = caps.get(2)?.as_str();

    // Split on first `#` to separate file path from anchor
    if let Some(hash_pos) = target.find('#') {
        Some(FullLinkTarget {
            file_path: target[..hash_pos].to_string(),
            anchor: target[hash_pos + 1..].to_string(),
        })
    } else {
        Some(FullLinkTarget {
            file_path: target.to_string(),
            anchor: String::new(),
        })
    }
}

/// Compute byte ranges of fenced code blocks and code spans using pulldown-cmark.
///
/// Reference definitions inside these ranges should be ignored because
/// CommonMark does not recognise them as definitions.
fn code_byte_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let parser = Parser::new_ext(text, Options::empty()).into_offset_iter();
    let mut ranges = Vec::new();
    let mut code_block_start: Option<usize> = None;

    for (event, range) in parser {
        match &event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_start = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            Event::Code(_) => {
                ranges.push(range);
            }
            _ => {}
        }
    }
    ranges
}

/// Scan the document for a reference definition `[ref_id]: target` and
/// return the parsed target.
///
/// Definitions inside fenced code blocks and code spans are skipped,
/// matching CommonMark semantics.
fn resolve_reference_to_target(text: &str, ref_id: &str) -> Option<FullLinkTarget> {
    use regex::Regex;
    use std::sync::LazyLock;

    static REF_DEF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?m)^[ ]{0,3}\[([^\]]+)\]:\s+<?([^>\s]+)>?"#).unwrap());

    let code_ranges = code_byte_ranges(text);

    for caps in REF_DEF_RE.captures_iter(text) {
        let Some(id) = caps.get(1) else { continue };

        // Skip definitions that fall inside code blocks or code spans
        let match_start = caps.get(0).map_or(0, |m| m.start());
        if code_ranges.iter().any(|r| r.contains(&match_start)) {
            continue;
        }

        if !id.as_str().eq_ignore_ascii_case(ref_id) {
            continue;
        }

        let Some(target) = caps.get(2) else { continue };
        let target = target.as_str();
        return if let Some(hash_pos) = target.find('#') {
            Some(FullLinkTarget {
                file_path: target[..hash_pos].to_string(),
                anchor: target[hash_pos + 1..].to_string(),
            })
        } else {
            Some(FullLinkTarget {
                file_path: target.to_string(),
                anchor: String::new(),
            })
        };
    }

    None
}

impl RumdlLanguageServer {
    /// Handle `textDocument/definition` requests.
    ///
    /// When the cursor is on a markdown link `[text](target.md#anchor)`, resolves
    /// the target file path and optional heading anchor, then returns a `Location`
    /// pointing to the target.
    pub(super) async fn handle_goto_definition(&self, uri: &Url, position: Position) -> Option<GotoDefinitionResponse> {
        let text = self.get_document_content(uri).await?;

        let link = detect_full_link_target(&text, position).or_else(|| detect_ref_link_target(&text, position))?;

        // External URLs have no local file to navigate to
        if is_external_url(&link.file_path) {
            return None;
        }

        self.resolve_link_target(uri, &link).await
    }

    /// Handle `textDocument/hover` requests.
    ///
    /// When the cursor is on a markdown link, shows a preview of the target:
    /// - External URLs: shows the URL
    /// - Local files without anchor: shows the file path and first lines
    /// - Local files with anchor: shows the heading and content below it
    pub(super) async fn handle_hover(&self, uri: &Url, position: Position) -> Option<Hover> {
        let text = self.get_document_content(uri).await?;

        let link = detect_full_link_target(&text, position).or_else(|| detect_ref_link_target(&text, position))?;

        // External URLs: show the URL itself
        if is_external_url(&link.file_path) {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**External link**\n\n{}", link.file_path),
                }),
                range: None,
            });
        }

        let current_file = uri.to_file_path().ok()?;
        let current_dir = current_file.parent()?.to_path_buf();

        let target_path = if link.file_path.is_empty() {
            current_file.clone()
        } else {
            normalize_path(current_dir.join(&link.file_path))
        };

        // Read target file content
        let target_uri = Url::from_file_path(&target_path).ok()?;
        let target_content = self.get_document_content(&target_uri).await?;

        let preview = if !link.anchor.is_empty() {
            self.build_anchor_preview(&target_path, &link.anchor, &target_content)
                .await
        } else {
            self.build_file_preview(&target_path, &target_content)
        };

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: preview,
            }),
            range: None,
        })
    }

    /// Build a hover preview for a link targeting a specific heading anchor.
    ///
    /// Finds the heading line in the file and extracts up to 15 lines of content
    /// below it (stopping at the next heading of equal or higher level).
    async fn build_anchor_preview(&self, file_path: &Path, anchor: &str, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();

        // Look up the heading line from the workspace index
        let heading_line = self.resolve_heading_line(file_path, anchor).await;

        let Some(heading_line_0indexed) = heading_line else {
            let display_path = file_path.file_name().unwrap_or(file_path.as_os_str());
            return format!("{}#{}\n\n*Heading not found*", display_path.to_string_lossy(), anchor);
        };

        let start = heading_line_0indexed as usize;
        if start >= lines.len() {
            let display_path = file_path.file_name().unwrap_or(file_path.as_os_str());
            return format!("{}#{}", display_path.to_string_lossy(), anchor);
        }

        // Determine the heading level of the target heading
        let heading_level = lines[start].chars().take_while(|&c| c == '#').count();

        // Collect lines: the heading + up to 15 lines of content below it,
        // stopping at the next heading of equal or higher level.
        // Track whether we stopped due to reaching the line limit vs section end.
        let max_lines = 15;
        let mut preview_lines: Vec<&str> = vec![lines[start]];
        let mut in_fenced_code_block = false;
        let mut hit_line_limit = false;

        for (i, line) in lines.iter().skip(start + 1).enumerate() {
            if i >= max_lines {
                hit_line_limit = true;
                break;
            }

            // Track fenced code blocks to avoid false heading detection
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fenced_code_block = !in_fenced_code_block;
            }

            if !in_fenced_code_block {
                let line_level = trimmed.chars().take_while(|&c| c == '#').count();
                // ATX heading requires a space after the `#` characters (or be empty)
                let after_hashes = &trimmed[trimmed.len().min(line_level)..];
                let is_atx_heading = line_level > 0 && (after_hashes.is_empty() || after_hashes.starts_with(' '));
                if is_atx_heading && line_level <= heading_level {
                    break;
                }
            }

            preview_lines.push(line);
        }

        let mut preview = preview_lines.join("\n");
        if hit_line_limit {
            preview.push_str("\n\n...");
        }
        preview
    }

    /// Build a hover preview for a link targeting a whole file (no anchor).
    ///
    /// Shows the file name and the first 15 lines of content.
    fn build_file_preview(&self, file_path: &Path, content: &str) -> String {
        let display_path = file_path.file_name().unwrap_or(file_path.as_os_str());
        let lines: Vec<&str> = content.lines().collect();
        let max_lines = 15;
        let preview_lines: Vec<&str> = lines.iter().take(max_lines).copied().collect();

        let mut preview = format!("**{}**\n\n{}", display_path.to_string_lossy(), preview_lines.join("\n"));
        if lines.len() > max_lines {
            preview.push_str("\n\n...");
        }
        preview
    }

    /// Handle `textDocument/references` requests.
    ///
    /// When the cursor is on a heading, finds all links across the workspace that
    /// reference this heading. When the cursor is on a link, finds all other links
    /// that point to the same target.
    pub(super) async fn handle_references(&self, uri: &Url, position: Position) -> Option<Vec<Location>> {
        let text = self.get_document_content(uri).await?;
        let current_file = uri.to_file_path().ok()?;

        // Check if cursor is on a heading by consulting the workspace index.
        // This avoids false positives from `#` lines inside code blocks.
        let heading_line_1indexed = (position.line as usize) + 1;
        let heading_anchor = {
            let index = self.workspace_index.read().await;
            index.get_file(&current_file).and_then(|file_index| {
                file_index
                    .headings
                    .iter()
                    .find(|h| h.line == heading_line_1indexed)
                    .map(|h| h.custom_anchor.clone().unwrap_or_else(|| h.auto_anchor.clone()))
            })
        };

        if let Some(anchor) = heading_anchor {
            // Find cross-file references
            let mut locations = self
                .find_references_to_target(&current_file, &anchor)
                .await
                .unwrap_or_default();

            // Also find same-file fragment-only links (e.g., [text](#anchor))
            let same_file = find_same_file_fragment_links(&text, uri, &anchor);
            locations.extend(same_file);

            return if locations.is_empty() { None } else { Some(locations) };
        }

        // Check if cursor is on a link (inline or reference-style)
        if let Some(link) = detect_full_link_target(&text, position).or_else(|| detect_ref_link_target(&text, position))
        {
            // External URLs have no local file to find references for
            if is_external_url(&link.file_path) {
                return None;
            }

            let current_dir = current_file.parent()?.to_path_buf();
            let target_path = if link.file_path.is_empty() {
                current_file.clone()
            } else {
                normalize_path(current_dir.join(&link.file_path))
            };

            return self.find_references_to_target(&target_path, &link.anchor).await;
        }

        // Fallback: find all cross-file references pointing to the current file,
        // regardless of fragment. This handles the common case where a user opens
        // a target file and invokes find-references to discover what links to it.
        self.find_all_references_to_file(&current_file).await
    }

    /// Find all links across the workspace that point to `target_file`,
    /// regardless of the fragment/anchor.
    ///
    /// This is the fallback for find-references when the cursor is not on a
    /// heading or a link: it returns every cross-file link whose resolved path
    /// matches `target_file`.
    async fn find_all_references_to_file(&self, target_file: &Path) -> Option<Vec<Location>> {
        let index = self.workspace_index.read().await;
        let mut locations = Vec::new();

        for (source_path, file_index) in index.files() {
            let source_dir = source_path.parent().unwrap_or(Path::new(""));

            let matching_links: Vec<_> = file_index
                .cross_file_links
                .iter()
                .filter(|link| {
                    let resolved_target = normalize_path(source_dir.join(&link.target_path));
                    resolved_target == *target_file
                })
                .collect();

            if matching_links.is_empty() {
                continue;
            }

            let source_uri = match Url::from_file_path(source_path) {
                Ok(uri) => uri,
                Err(_) => continue,
            };

            let source_content = tokio::fs::read_to_string(source_path).await.ok();
            let source_lines: Vec<&str> = source_content
                .as_deref()
                .map(|c| c.lines().collect())
                .unwrap_or_default();

            for link in matching_links {
                let line = (link.line.saturating_sub(1)) as u32;
                let byte_col_0indexed = link.column.saturating_sub(1);

                let character = source_lines
                    .get(line as usize)
                    .map(|line_text| {
                        let clamped = byte_col_0indexed.min(line_text.len());
                        byte_to_utf16_offset(line_text, clamped)
                    })
                    .unwrap_or(byte_col_0indexed as u32);

                locations.push(Location {
                    uri: source_uri.clone(),
                    range: Range {
                        start: Position { line, character },
                        end: Position { line, character },
                    },
                });
            }
        }

        if locations.is_empty() { None } else { Some(locations) }
    }

    /// Resolve a `FullLinkTarget` to a `GotoDefinitionResponse`.
    ///
    /// Shared between inline links and reference-style links to avoid
    /// duplicating the path resolution and heading lookup logic.
    async fn resolve_link_target(&self, uri: &Url, link: &FullLinkTarget) -> Option<GotoDefinitionResponse> {
        let current_file = uri.to_file_path().ok()?;
        let current_dir = current_file.parent()?.to_path_buf();

        let target_path = if link.file_path.is_empty() {
            current_file.clone()
        } else {
            normalize_path(current_dir.join(&link.file_path))
        };

        let target_uri = Url::from_file_path(&target_path).ok()?;

        let target_line = if link.anchor.is_empty() {
            0
        } else {
            self.resolve_heading_line(&target_path, &link.anchor).await.unwrap_or(0)
        };

        let target_position = Position {
            line: target_line,
            character: 0,
        };
        let range = Range {
            start: target_position,
            end: target_position,
        };

        Some(GotoDefinitionResponse::Scalar(Location { uri: target_uri, range }))
    }

    /// Look up a heading's line number (0-indexed for LSP) in the workspace index.
    async fn resolve_heading_line(&self, file_path: &Path, anchor: &str) -> Option<u32> {
        let index = self.workspace_index.read().await;
        let file_index = index.get_file(file_path)?;
        let heading = file_index.get_heading_by_anchor(anchor)?;
        // HeadingIndex.line is 1-indexed; LSP is 0-indexed
        Some((heading.line.saturating_sub(1)) as u32)
    }

    /// Find all links across the workspace that point to `target_path` with
    /// the given `fragment` (anchor).
    ///
    /// An empty fragment matches links that target the file without an anchor.
    async fn find_references_to_target(&self, target_path: &Path, fragment: &str) -> Option<Vec<Location>> {
        let index = self.workspace_index.read().await;
        let mut locations = Vec::new();

        for (source_path, file_index) in index.files() {
            let source_dir = source_path.parent().unwrap_or(Path::new(""));

            // Collect matching links for this file before loading content
            let matching_links: Vec<_> = file_index
                .cross_file_links
                .iter()
                .filter(|link| {
                    let resolved_target = normalize_path(source_dir.join(&link.target_path));
                    resolved_target == *target_path && link.fragment.eq_ignore_ascii_case(fragment)
                })
                .collect();

            if matching_links.is_empty() {
                continue;
            }

            let source_uri = match Url::from_file_path(source_path) {
                Ok(uri) => uri,
                Err(_) => continue,
            };

            // Load source content for byte→UTF-16 column conversion.
            // We read the file directly instead of using get_document_content
            // to avoid acquiring additional locks while holding the workspace
            // index read lock.
            let source_content = tokio::fs::read_to_string(source_path).await.ok();
            let source_lines: Vec<&str> = source_content
                .as_deref()
                .map(|c| c.lines().collect())
                .unwrap_or_default();

            for link in matching_links {
                // CrossFileLinkIndex uses 1-indexed line/column; LSP uses 0-indexed
                let line = (link.line.saturating_sub(1)) as u32;
                let byte_col_0indexed = link.column.saturating_sub(1);

                // Convert byte column to UTF-16 code units using the actual line text
                let character = source_lines
                    .get(line as usize)
                    .map(|line_text| {
                        let clamped = byte_col_0indexed.min(line_text.len());
                        byte_to_utf16_offset(line_text, clamped)
                    })
                    .unwrap_or(byte_col_0indexed as u32);

                locations.push(Location {
                    uri: source_uri.clone(),
                    range: Range {
                        start: Position { line, character },
                        end: Position { line, character },
                    },
                });
            }
        }

        if locations.is_empty() { None } else { Some(locations) }
    }

    // =========================================================================
    // Rename support
    // =========================================================================

    /// Handle `textDocument/prepareRename` requests.
    ///
    /// Validates that the cursor is on a heading and returns the renameable
    /// text range (excluding `#` markers, leading whitespace, and custom anchors).
    pub(super) async fn handle_prepare_rename(&self, uri: &Url, position: Position) -> Option<PrepareRenameResponse> {
        let text = self.get_document_content(uri).await?;
        let current_file = uri.to_file_path().ok()?;

        let heading_line_1indexed = (position.line as usize) + 1;

        let heading_info = {
            let index = self.workspace_index.read().await;
            index.get_file(&current_file).and_then(|file_index| {
                file_index
                    .headings
                    .iter()
                    .find(|h| h.line == heading_line_1indexed)
                    .cloned()
            })
        }?;

        let lines: Vec<&str> = text.lines().collect();
        let line_text = lines.get(position.line as usize)?;

        let (text_start, text_end) = if heading_info.is_setext {
            // Setext heading: the entire line is the heading text
            let trimmed_start = line_text.len() - line_text.trim_start().len();
            let trimmed_end = line_text.trim_end().len();
            (trimmed_start, trimmed_end)
        } else {
            let start = find_heading_text_start(line_text)?;
            let end = find_heading_text_end(line_text, start);
            (start, end)
        };

        if text_start >= text_end {
            return None;
        }

        let start_char = byte_to_utf16_offset(line_text, text_start);
        let end_char = byte_to_utf16_offset(line_text, text_end);

        Some(PrepareRenameResponse::Range(Range {
            start: Position {
                line: position.line,
                character: start_char,
            },
            end: Position {
                line: position.line,
                character: end_char,
            },
        }))
    }

    /// Handle `textDocument/rename` requests.
    ///
    /// When a heading is renamed, generates a `WorkspaceEdit` that:
    /// 1. Replaces the heading text in the source file
    /// 2. Updates all cross-file links referencing the old anchor
    /// 3. Updates all same-file fragment links referencing the old anchor
    ///
    /// If the heading has a custom anchor (`{#id}`), only the heading text is
    /// changed — links reference the custom ID which remains unchanged.
    pub(super) async fn handle_rename(&self, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit> {
        // Reject empty or whitespace-only names
        if new_name.trim().is_empty() {
            return None;
        }

        let text = self.get_document_content(uri).await?;
        let current_file = uri.to_file_path().ok()?;

        let heading_line_1indexed = (position.line as usize) + 1;

        let heading_info = {
            let index = self.workspace_index.read().await;
            index.get_file(&current_file).and_then(|file_index| {
                file_index
                    .headings
                    .iter()
                    .find(|h| h.line == heading_line_1indexed)
                    .cloned()
            })
        }?;

        let has_custom_anchor = heading_info.custom_anchor.is_some();
        let old_anchor = heading_info
            .custom_anchor
            .as_deref()
            .unwrap_or(&heading_info.auto_anchor);

        let lines: Vec<&str> = text.lines().collect();
        let line_text = lines.get(position.line as usize)?;

        let (text_start, text_end) = if heading_info.is_setext {
            let trimmed_start = line_text.len() - line_text.trim_start().len();
            let trimmed_end = line_text.trim_end().len();
            (trimmed_start, trimmed_end)
        } else {
            let start = find_heading_text_start(line_text)?;
            let end = find_heading_text_end(line_text, start);
            (start, end)
        };

        let start_char = byte_to_utf16_offset(line_text, text_start);
        let end_char = byte_to_utf16_offset(line_text, text_end);

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        // Edit 1: Replace heading text
        changes.entry(uri.clone()).or_default().push(TextEdit {
            range: Range {
                start: Position {
                    line: position.line,
                    character: start_char,
                },
                end: Position {
                    line: position.line,
                    character: end_char,
                },
            },
            new_text: new_name.to_string(),
        });

        // If heading has a custom anchor, links reference the custom ID.
        // Renaming the heading text doesn't change the anchor, so no link updates needed.
        if has_custom_anchor {
            return Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            });
        }

        let new_anchor = AnchorStyle::GitHub.generate_fragment(new_name);

        // Check for anchor collision with existing headings in the same file
        {
            let index = self.workspace_index.read().await;
            if let Some(file_index) = index.get_file(&current_file) {
                let collision = file_index.headings.iter().any(|h| {
                    h.line != heading_line_1indexed
                        && (h.auto_anchor.eq_ignore_ascii_case(&new_anchor)
                            || h.custom_anchor
                                .as_deref()
                                .is_some_and(|ca| ca.eq_ignore_ascii_case(&new_anchor)))
                });
                if collision {
                    log::warn!("Rename refused: anchor '{new_anchor}' would collide with existing heading");
                    return None;
                }
            }
        }

        // Collect cross-file link data while holding the lock, then release it
        let cross_file_matches = self.collect_cross_file_link_matches(&current_file, old_anchor).await;

        // Process cross-file matches (file I/O without holding the lock)
        for (source_path, matching_links) in &cross_file_matches {
            // Skip the current file — same-file links are handled separately
            if *source_path == current_file {
                continue;
            }

            let source_uri = match Url::from_file_path(source_path) {
                Ok(uri) => uri,
                Err(_) => continue,
            };

            // Try editor buffer first, fall back to disk
            let source_content = if let Some(content) = self.get_document_content(&source_uri).await {
                content
            } else {
                match tokio::fs::read_to_string(source_path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                }
            };

            let source_lines: Vec<&str> = source_content.lines().collect();

            // Group links by line to avoid duplicate edits when multiple links
            // on the same line reference the same anchor
            let mut seen_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for link in matching_links {
                let line_idx = link.line.saturating_sub(1);
                if !seen_lines.insert(line_idx) {
                    continue; // already processed this line
                }
                let Some(line_content) = source_lines.get(line_idx) else {
                    continue;
                };

                let edits = find_all_anchor_edits_in_link(line_content, line_idx as u32, old_anchor, &new_anchor);
                changes.entry(source_uri.clone()).or_default().extend(edits);
            }
        }

        // Same-file fragment links (#anchor)
        collect_same_file_anchor_edits(uri, &text, old_anchor, &new_anchor, &mut changes);

        // Same-file reference definitions ([ref]: #anchor)
        collect_same_file_ref_def_edits(uri, &text, old_anchor, &new_anchor, &mut changes);

        Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
    }

    /// Collect cross-file link matches from the workspace index.
    ///
    /// Returns the data needed to compute edits without holding the lock
    /// during file I/O.
    async fn collect_cross_file_link_matches(
        &self,
        target_path: &Path,
        old_anchor: &str,
    ) -> Vec<(PathBuf, Vec<crate::workspace_index::CrossFileLinkIndex>)> {
        let index = self.workspace_index.read().await;
        let mut result = Vec::new();

        for (source_path, file_index) in index.files() {
            let source_dir = source_path.parent().unwrap_or(Path::new(""));

            let matching: Vec<_> = file_index
                .cross_file_links
                .iter()
                .filter(|link| {
                    let resolved = normalize_path(source_dir.join(&link.target_path));
                    resolved == *target_path && link.fragment.eq_ignore_ascii_case(old_anchor)
                })
                .cloned()
                .collect();

            if !matching.is_empty() {
                result.push((source_path.to_path_buf(), matching));
            }
        }

        result
    }
}

/// Find the byte offset where ATX heading text starts (after `#` markers and space).
fn find_heading_text_start(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let leading_ws = line.len() - trimmed.len();
    let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hash_count > 6 {
        return None;
    }
    let after_hashes = leading_ws + hash_count;
    // ATX headings require a space after #
    if line.as_bytes().get(after_hashes) == Some(&b' ') {
        Some(after_hashes + 1)
    } else if after_hashes == line.len() {
        // Empty heading: just "##"
        Some(after_hashes)
    } else {
        None
    }
}

/// Find the byte offset where ATX heading text ends.
///
/// Handles:
/// - Custom anchor syntax `{#id}` at end of heading
/// - Closing ATX markers (`## Heading ##`)
fn find_heading_text_end(line: &str, text_start: usize) -> usize {
    let text_portion = &line[text_start..];

    // Check for custom anchor syntax {#id} at end
    if let Some(brace_pos) = text_portion.rfind("{#") {
        // Verify the brace pair is at the end (after trimming)
        if let Some(close_pos) = text_portion[brace_pos..].find('}') {
            let after_brace = text_portion[brace_pos + close_pos + 1..].trim();
            if after_brace.is_empty() {
                let before_brace = text_portion[..brace_pos].trim_end();
                return text_start + before_brace.len();
            }
        }
    }

    // Check for closing ATX markers (trailing ###)
    let trimmed_end = text_portion.trim_end();
    if let Some(last_non_hash_pos) = trimmed_end.rfind(|c: char| c != '#') {
        let after_last = &trimmed_end[last_non_hash_pos..];
        // The character at last_non_hash_pos must be a space for this to be a closing sequence
        let last_char = trimmed_end[last_non_hash_pos..].chars().next().unwrap();
        if last_char == ' ' && after_last.len() > 1 {
            // Closing ATX: trim the space and trailing hashes
            return text_start + last_non_hash_pos;
        }
    }

    text_start + trimmed_end.len()
}

/// Find ALL `#anchor` occurrences inside links on a given line and create TextEdits.
///
/// Searches specifically within the URL portion of links (after `](` or after `]: `)
/// to avoid matching `#` in link display text. Returns all matches, not just the first.
fn find_all_anchor_edits_in_link(line: &str, line_num: u32, old_anchor: &str, new_anchor: &str) -> Vec<TextEdit> {
    let target = format!("#{old_anchor}");
    let target_lower = target.to_lowercase();
    let mut edits = Vec::new();

    // Search for the anchor in the URL portion of links
    // Look for ](url#anchor) patterns
    let mut search_start = 0;
    while search_start < line.len() {
        // Find the next `](` which starts a link URL
        if let Some(paren_start) = line[search_start..].find("](") {
            let url_start = search_start + paren_start + 2;
            // Find the closing `)`
            if let Some(paren_end) = line[url_start..].find(')') {
                let url_portion = &line[url_start..url_start + paren_end];
                if let Some(hash_pos) = url_portion.to_lowercase().find(&target_lower) {
                    let anchor_start = url_start + hash_pos + 1; // after #
                    let anchor_end = anchor_start + old_anchor.len();

                    let start_char = byte_to_utf16_offset(line, anchor_start);
                    let end_char = byte_to_utf16_offset(line, anchor_end);

                    edits.push(TextEdit {
                        range: Range {
                            start: Position {
                                line: line_num,
                                character: start_char,
                            },
                            end: Position {
                                line: line_num,
                                character: end_char,
                            },
                        },
                        new_text: new_anchor.to_string(),
                    });
                }
                search_start = url_start + paren_end + 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Also check reference definitions: [ref]: file.md#anchor
    if let Some(colon_pos) = line.find("]: ") {
        let def_target = &line[colon_pos + 3..];
        if let Some(hash_pos) = def_target.to_lowercase().find(&target_lower) {
            let anchor_start = colon_pos + 3 + hash_pos + 1; // after #
            let anchor_end = anchor_start + old_anchor.len();
            if anchor_end <= line.len() {
                let start_char = byte_to_utf16_offset(line, anchor_start);
                let end_char = byte_to_utf16_offset(line, anchor_end);

                edits.push(TextEdit {
                    range: Range {
                        start: Position {
                            line: line_num,
                            character: start_char,
                        },
                        end: Position {
                            line: line_num,
                            character: end_char,
                        },
                    },
                    new_text: new_anchor.to_string(),
                });
            }
        }
    }

    edits
}

/// Collect text edits for same-file fragment-only links (`[text](#anchor)`).
///
/// Uses pulldown-cmark to parse links, then searches within the URL portion
/// for the fragment to replace.
fn collect_same_file_anchor_edits(
    uri: &Url,
    content: &str,
    old_anchor: &str,
    new_anchor: &str,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    use pulldown_cmark::{Event, Parser, Tag};

    let options = crate::utils::rumdl_parser_options();

    let parser = Parser::new_ext(content, options).into_offset_iter();

    for (event, range) in parser {
        if let Event::Start(Tag::Link { dest_url, .. }) = event
            && let Some(frag) = dest_url.strip_prefix('#')
            && frag.eq_ignore_ascii_case(old_anchor)
        {
            // Find the `#fragment` in the raw source within the link range
            let link_source = &content[range.start..range.end];
            if let Some(anchor_byte_in_source) = find_fragment_in_link_source(link_source, old_anchor) {
                let anchor_start_byte = range.start + anchor_byte_in_source;
                let anchor_end_byte = anchor_start_byte + old_anchor.len();

                let line_idx = content[..anchor_start_byte].matches('\n').count();
                let line_start = content[..anchor_start_byte].rfind('\n').map_or(0, |p| p + 1);
                let line_end = content[line_start..]
                    .find('\n')
                    .map_or(content.len(), |p| line_start + p);
                let line_text = &content[line_start..line_end];

                let start_char = byte_to_utf16_offset(line_text, anchor_start_byte - line_start);
                let end_char = byte_to_utf16_offset(line_text, anchor_end_byte - line_start);

                changes.entry(uri.clone()).or_default().push(TextEdit {
                    range: Range {
                        start: Position {
                            line: line_idx as u32,
                            character: start_char,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: end_char,
                        },
                    },
                    new_text: new_anchor.to_string(),
                });
            }
        }
    }
}

/// Find the anchor text position within a link's raw source.
///
/// For inline links like `[text](#anchor)`, searches after `(#`.
/// For reference definitions resolved by pulldown-cmark, the raw source
/// includes the `[text]` portion, so we search for `#anchor` within `(...)`.
/// Falls back to finding `#anchor` after any `(` or `]: ` pattern.
fn find_fragment_in_link_source(source: &str, anchor: &str) -> Option<usize> {
    let target = format!("#{anchor}");
    let target_lower = target.to_lowercase();

    // Try inline link: find `(#anchor` within `(...)`
    if let Some(paren_pos) = source.find('(') {
        let after_paren = &source[paren_pos + 1..];
        if let Some(hash_pos) = after_paren.to_lowercase().find(&target_lower) {
            // Return position of anchor text (after #)
            return Some(paren_pos + 1 + hash_pos + 1);
        }
    }

    // Fallback: search anywhere in the source (for edge cases)
    if let Some(pos) = source.to_lowercase().find(&target_lower) {
        return Some(pos + 1); // after #
    }

    None
}

/// Collect text edits for same-file reference definitions that point to fragments.
///
/// Handles lines like `[ref]: #anchor` where the anchor needs updating.
fn collect_same_file_ref_def_edits(
    uri: &Url,
    content: &str,
    old_anchor: &str,
    new_anchor: &str,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    let target = format!("#{old_anchor}");
    let target_lower = target.to_lowercase();

    for (line_idx, line) in content.lines().enumerate() {
        // Match reference definition pattern: [ref]: #anchor
        let trimmed = line.trim_start();
        if !trimmed.starts_with('[') {
            continue;
        }
        if let Some(colon_pos) = trimmed.find("]: ") {
            let def_target = trimmed[colon_pos + 3..].trim();
            // Only fragment-only definitions
            if let Some(frag) = def_target.strip_prefix('#') {
                // Strip any trailing title
                let frag = frag.split_whitespace().next().unwrap_or(frag);
                if frag.eq_ignore_ascii_case(old_anchor) {
                    // Find the position in the original line
                    if let Some(hash_pos) = line.to_lowercase().find(&target_lower) {
                        let anchor_start = hash_pos + 1;
                        let anchor_end = anchor_start + old_anchor.len();

                        let start_char = byte_to_utf16_offset(line, anchor_start);
                        let end_char = byte_to_utf16_offset(line, anchor_end);

                        changes.entry(uri.clone()).or_default().push(TextEdit {
                            range: Range {
                                start: Position {
                                    line: line_idx as u32,
                                    character: start_char,
                                },
                                end: Position {
                                    line: line_idx as u32,
                                    character: end_char,
                                },
                            },
                            new_text: new_anchor.to_string(),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_full_link_target_file_only() {
        let text = "See [link](guide.md) here.\n";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "");
    }

    #[test]
    fn test_detect_full_link_target_file_with_anchor() {
        let text = "See [link](guide.md#install) here.\n";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_detect_full_link_target_same_file_anchor() {
        let text = "See [below](#configuration) here.\n";
        let position = Position { line: 0, character: 15 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "");
        assert_eq!(link.anchor, "configuration");
    }

    #[test]
    fn test_detect_full_link_target_cursor_outside_link() {
        let text = "Just some text here.\n";
        let position = Position { line: 0, character: 5 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_full_link_target_cursor_at_start_of_target() {
        let text = "See [link](guide.md) here.\n";
        // Cursor right after `](`
        let position = Position { line: 0, character: 11 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_full_link_target_cursor_at_end_of_target() {
        let text = "See [link](guide.md) here.\n";
        // Cursor right before `)`
        let position = Position { line: 0, character: 19 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_full_link_target_in_code_span() {
        let text = "See `[link](guide.md)` here.\n";
        let position = Position { line: 0, character: 15 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_none(), "Should not detect links inside code spans");
    }

    #[test]
    fn test_detect_full_link_target_with_title() {
        let text = r#"See [link](guide.md "Title") here."#;
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "");
    }

    #[test]
    fn test_detect_full_link_target_with_single_quote_title() {
        let text = "See [link](guide.md 'Title') here.";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_full_link_target_with_paren_title() {
        let text = "See [link](guide.md (Title)) here.";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_full_link_target_with_title_and_anchor() {
        let text = r#"See [link](guide.md#install "Install Guide") here."#;
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_detect_full_link_target_angle_brackets() {
        let text = "See [link](<guide.md>) here.";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "");
    }

    #[test]
    fn test_detect_full_link_target_angle_brackets_with_anchor() {
        let text = "See [link](<guide.md#install>) here.";
        let position = Position { line: 0, character: 14 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_strip_link_title_double_quotes() {
        assert_eq!(strip_link_title(r#"file.md "Title""#), "file.md");
    }

    #[test]
    fn test_strip_link_title_single_quotes() {
        assert_eq!(strip_link_title("file.md 'Title'"), "file.md");
    }

    #[test]
    fn test_strip_link_title_parens() {
        assert_eq!(strip_link_title("file.md (Title)"), "file.md");
    }

    #[test]
    fn test_strip_link_title_no_title() {
        assert_eq!(strip_link_title("file.md"), "file.md");
    }

    #[test]
    fn test_strip_link_title_with_spaces_in_path() {
        // Space not followed by title delimiter should not strip
        assert_eq!(strip_link_title("my file.md"), "my file.md");
    }

    #[test]
    fn test_find_same_file_fragment_links_basic() {
        let content = "# Heading\n\nSee [below](#heading) for details.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 2);
    }

    #[test]
    fn test_find_same_file_fragment_links_case_insensitive() {
        let content = "See [link](#HEADING) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 1);
    }

    #[test]
    fn test_find_same_file_fragment_links_multiple() {
        let content = "See [a](#heading) and [b](#heading) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_find_same_file_fragment_links_no_match() {
        let content = "See [link](#other) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 0);
    }

    // =========================================================================
    // Balanced parentheses
    // =========================================================================

    #[test]
    fn test_find_balanced_close_paren_simple() {
        assert_eq!(find_balanced_close_paren("file.md)"), Some(7));
    }

    #[test]
    fn test_find_balanced_close_paren_nested() {
        // file(1).md)  — the inner `(1)` should not terminate the search
        assert_eq!(find_balanced_close_paren("file(1).md)"), Some(10));
    }

    #[test]
    fn test_find_balanced_close_paren_double_nested() {
        assert_eq!(find_balanced_close_paren("a(b(c)).md)"), Some(10));
    }

    #[test]
    fn test_find_balanced_close_paren_no_close() {
        assert_eq!(find_balanced_close_paren("file.md"), None);
    }

    #[test]
    fn test_detect_full_link_target_nested_parens() {
        let text = "See [manpage](file(1).md) here.\n";
        // Cursor on "file(1).md"
        let position = Position { line: 0, character: 18 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some(), "Should handle balanced parens in link target");
        let link = result.unwrap();
        assert_eq!(link.file_path, "file(1).md");
        assert_eq!(link.anchor, "");
    }

    #[test]
    fn test_detect_full_link_target_double_nested_parens() {
        let text = "See [x](a(b(c)).md) here.\n";
        let position = Position { line: 0, character: 12 };
        let result = detect_full_link_target(text, position);
        assert!(result.is_some(), "Should handle double-nested parens");
        let link = result.unwrap();
        assert_eq!(link.file_path, "a(b(c)).md");
    }

    // =========================================================================
    // URL vs file-path distinction
    // =========================================================================

    #[test]
    fn test_is_external_url_https() {
        assert!(is_external_url("https://example.com/page"));
    }

    #[test]
    fn test_is_external_url_http() {
        assert!(is_external_url("http://example.com/page"));
    }

    #[test]
    fn test_is_external_url_mailto() {
        assert!(is_external_url("mailto:user@example.com"));
    }

    #[test]
    fn test_is_external_url_www() {
        assert!(is_external_url("www.example.com"));
    }

    #[test]
    fn test_is_external_url_relative_path() {
        assert!(!is_external_url("guide.md"));
    }

    #[test]
    fn test_is_external_url_empty() {
        assert!(!is_external_url(""));
    }

    #[test]
    fn test_is_external_url_fragment_only() {
        assert!(!is_external_url("#heading"));
    }

    // =========================================================================
    // Code block / code span filtering in find_same_file_fragment_links
    // =========================================================================

    #[test]
    fn test_find_same_file_fragment_links_skips_code_blocks() {
        let content = "# Heading\n\n```\nSee [link](#heading) in code.\n```\n\nReal [link](#heading) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 1, "Should only find the link outside the code block");
        assert_eq!(
            locations[0].range.start.line, 6,
            "Should be on the line after the code block"
        );
    }

    #[test]
    fn test_find_same_file_fragment_links_skips_code_spans() {
        let content = "See `[link](#heading)` and [real](#heading) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 1, "Should only find the link outside the code span");
    }

    #[test]
    fn test_find_same_file_fragment_links_includes_reference_links() {
        let content = "See [text][ref] and [inline](#heading) here.\n\n[ref]: #heading\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(
            locations.len(),
            2,
            "Should find both inline and reference-style fragment links"
        );
    }

    #[test]
    fn test_find_same_file_fragment_links_utf16_position() {
        // Emoji before the link: "🎉 " is 4 bytes / 2 UTF-16 code units + 1 space
        let content = "🎉 [link](#heading) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let locations = find_same_file_fragment_links(content, &uri, "heading");
        assert_eq!(locations.len(), 1);
        // '🎉' is 2 UTF-16 code units + ' ' is 1 = character 3
        assert_eq!(
            locations[0].range.start.character, 3,
            "Character position should be in UTF-16 code units, not bytes"
        );
    }

    // =========================================================================
    // Reference-style link detection
    // =========================================================================

    #[test]
    fn test_detect_ref_definition_basic() {
        let result = detect_ref_definition("[guide]: guide.md");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "");
    }

    #[test]
    fn test_detect_ref_definition_with_anchor() {
        let result = detect_ref_definition("[guide]: guide.md#install");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_detect_ref_definition_indented() {
        let result = detect_ref_definition("   [guide]: guide.md");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_ref_definition_not_a_definition() {
        let result = detect_ref_definition("Some [text] here");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_reference_to_target_basic() {
        let text = "See [guide] for info.\n\n[guide]: guide.md\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_resolve_reference_to_target_case_insensitive() {
        let text = "See [Guide] here.\n\n[guide]: guide.md\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_reference_to_target_with_anchor() {
        let text = "[ref]: guide.md#install\n";
        let result = resolve_reference_to_target(text, "ref");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_resolve_reference_to_target_not_found() {
        let text = "No definitions here.\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_reference_to_target_skips_code_block() {
        let text = "See [guide] here.\n\n```\n[guide]: wrong.md\n```\n\n[guide]: correct.md\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(link.file_path, "correct.md", "Should skip definition inside code block");
    }

    #[test]
    fn test_resolve_reference_to_target_only_in_code_block() {
        let text = "See [guide] here.\n\n```\n[guide]: guide.md\n```\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_none(), "Should not find definition inside code block");
    }

    #[test]
    fn test_resolve_reference_to_target_skips_indented_code_block() {
        let text = "See [guide] here.\n\n    [guide]: wrong.md\n\n[guide]: correct.md\n";
        let result = resolve_reference_to_target(text, "guide");
        assert!(result.is_some());
        let link = result.unwrap();
        assert_eq!(
            link.file_path, "correct.md",
            "Should skip definition inside indented code block"
        );
    }

    #[test]
    fn test_detect_ref_link_usage_full_reference() {
        // [text][ref-id]  — cursor on "text"
        let line = "See [guide text][guide] here.";
        let result = detect_ref_link_usage(line, 8);
        assert_eq!(result.as_deref(), Some("guide"));
    }

    #[test]
    fn test_detect_ref_link_usage_collapsed_reference() {
        // [text][]  — cursor on "text"
        let line = "See [guide][] here.";
        let result = detect_ref_link_usage(line, 7);
        assert_eq!(result.as_deref(), Some("guide"));
    }

    #[test]
    fn test_detect_ref_link_usage_shortcut_reference() {
        // [text]  — cursor on "text"
        let line = "See [guide] here.";
        let result = detect_ref_link_usage(line, 7);
        assert_eq!(result.as_deref(), Some("guide"));
    }

    #[test]
    fn test_detect_ref_link_usage_cursor_in_second_bracket() {
        // [text][ref-id]  — cursor on "ref-id" (second bracket pair)
        let line = "See [text][guide] here.";
        // Cursor at 'g' in "guide" (index 11)
        let result = detect_ref_link_usage(line, 11);
        assert_eq!(result.as_deref(), Some("guide"));
    }

    #[test]
    fn test_detect_ref_link_usage_cursor_in_empty_second_bracket() {
        // [text][]  — cursor in the empty second bracket
        let line = "See [guide][] here.";
        // Cursor at index 12 (between the empty [])
        let result = detect_ref_link_usage(line, 12);
        assert_eq!(result.as_deref(), Some("guide"));
    }

    #[test]
    fn test_detect_ref_link_usage_not_inline_link() {
        // Should not match `[text](url)` — that's an inline link, not a reference
        let line = "See [link](guide.md) here.";
        let result = detect_ref_link_usage(line, 7);
        assert!(result.is_none(), "Should not match inline links");
    }

    #[test]
    fn test_detect_ref_link_usage_in_code_span() {
        let line = "See `[guide]` here.";
        let result = detect_ref_link_usage(line, 8);
        assert!(result.is_none(), "Should not match inside code spans");
    }

    #[test]
    fn test_detect_ref_link_target_full_reference() {
        let text = "See [click here][guide] for info.\n\n[guide]: guide.md#install\n";
        // Cursor on "click here"
        let position = Position { line: 0, character: 8 };
        let result = detect_ref_link_target(text, position);
        assert!(result.is_some(), "Should resolve full reference link");
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_detect_ref_link_target_collapsed_reference() {
        let text = "See [guide][] for info.\n\n[guide]: guide.md\n";
        let position = Position { line: 0, character: 7 };
        let result = detect_ref_link_target(text, position);
        assert!(result.is_some(), "Should resolve collapsed reference link");
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
    }

    #[test]
    fn test_detect_ref_link_target_definition_line() {
        let text = "[guide]: guide.md#install\n";
        // Cursor on the definition line
        let position = Position { line: 0, character: 5 };
        let result = detect_ref_link_target(text, position);
        assert!(result.is_some(), "Should resolve reference definition line");
        let link = result.unwrap();
        assert_eq!(link.file_path, "guide.md");
        assert_eq!(link.anchor, "install");
    }

    #[test]
    fn test_detect_ref_link_target_external_url() {
        let text = "See [example] for info.\n\n[example]: https://example.com\n";
        let position = Position { line: 0, character: 7 };
        let result = detect_ref_link_target(text, position);
        assert!(result.is_some());
        let link = result.unwrap();
        // The target is an external URL — is_external_url should catch this at the handler level
        assert!(is_external_url(&link.file_path));
    }

    // =========================================================================
    // Heading text range helpers (for rename)
    // =========================================================================

    #[test]
    fn test_find_heading_text_start_atx() {
        assert_eq!(find_heading_text_start("# Heading"), Some(2));
        assert_eq!(find_heading_text_start("## Heading"), Some(3));
        assert_eq!(find_heading_text_start("### Heading"), Some(4));
    }

    #[test]
    fn test_find_heading_text_start_no_space() {
        // Invalid ATX heading (no space after #)
        assert_eq!(find_heading_text_start("#NoSpace"), None);
    }

    #[test]
    fn test_find_heading_text_start_not_heading() {
        assert_eq!(find_heading_text_start("Just text"), None);
        assert_eq!(find_heading_text_start(""), None);
    }

    #[test]
    fn test_find_heading_text_start_leading_whitespace() {
        assert_eq!(find_heading_text_start("  ## Heading"), Some(5));
    }

    #[test]
    fn test_find_heading_text_start_empty_heading() {
        assert_eq!(find_heading_text_start("##"), Some(2));
    }

    #[test]
    fn test_find_heading_text_end_simple() {
        let line = "## Hello World";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "Hello World");
    }

    #[test]
    fn test_find_heading_text_end_with_custom_anchor() {
        let line = "## Guide {#install}";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "Guide");
    }

    #[test]
    fn test_find_heading_text_end_with_closing_atx() {
        let line = "## Hello ##";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "Hello");
    }

    #[test]
    fn test_find_heading_text_end_with_trailing_spaces() {
        let line = "## Hello   ";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "Hello");
    }

    #[test]
    fn test_find_heading_text_end_unicode() {
        let line = "## 日本語";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "日本語");
    }

    #[test]
    fn test_find_heading_text_end_unicode_with_closing() {
        let line = "## 日本語 ##";
        let start = find_heading_text_start(line).unwrap();
        let end = find_heading_text_end(line, start);
        assert_eq!(&line[start..end], "日本語");
    }

    // =========================================================================
    // Anchor edit in link helpers (for rename)
    // =========================================================================

    #[test]
    fn test_find_all_anchor_edits_in_link_inline() {
        let line = "See [link](guide.md#getting-started) here.";
        let edits = find_all_anchor_edits_in_link(line, 0, "getting-started", "quick-start");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "quick-start");
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        assert_eq!(&line[start..end], "getting-started");
    }

    #[test]
    fn test_find_all_anchor_edits_in_link_ignores_display_text() {
        // The `#getting-started` appears in the display text too
        let line = "See [#getting-started info](guide.md#getting-started) here.";
        let edits = find_all_anchor_edits_in_link(line, 0, "getting-started", "quick-start");
        assert_eq!(edits.len(), 1);
        // Should match the URL portion, not the display text
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        assert_eq!(&line[start..end], "getting-started");
        assert!(start > 30, "Should match in the URL, not the display text");
    }

    #[test]
    fn test_find_all_anchor_edits_in_link_ref_definition() {
        let line = "[guide]: guide.md#getting-started";
        let edits = find_all_anchor_edits_in_link(line, 5, "getting-started", "quick-start");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "quick-start");
        assert_eq!(edits[0].range.start.line, 5);
    }

    #[test]
    fn test_find_all_anchor_edits_in_link_no_match() {
        let line = "See [link](guide.md#other-section) here.";
        let edits = find_all_anchor_edits_in_link(line, 0, "getting-started", "quick-start");
        assert!(edits.is_empty());
    }

    #[test]
    fn test_find_all_anchor_edits_in_link_multiple_on_same_line() {
        let line = "See [a](guide.md#heading) and [b](guide.md#heading) here.";
        let edits = find_all_anchor_edits_in_link(line, 0, "heading", "new-heading");
        assert_eq!(edits.len(), 2, "Should find both anchors on the same line");
        // Edits should target different positions
        assert_ne!(edits[0].range.start.character, edits[1].range.start.character,);
    }

    // =========================================================================
    // Same-file anchor edits (for rename)
    // =========================================================================

    #[test]
    fn test_collect_same_file_anchor_edits_inline() {
        let content = "# Getting Started\n\nSee [below](#getting-started) for details.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_anchor_edits(&uri, content, "getting-started", "quick-start", &mut changes);

        let edits = changes.get(&uri).expect("Should have edits for the URI");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "quick-start");
    }

    #[test]
    fn test_collect_same_file_anchor_edits_multiple() {
        let content = "# Heading\n\n[a](#heading) and [b](#heading).\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_anchor_edits(&uri, content, "heading", "new-heading", &mut changes);

        let edits = changes.get(&uri).expect("Should have edits");
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn test_collect_same_file_anchor_edits_no_match() {
        let content = "# Heading\n\n[a](#other) here.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_anchor_edits(&uri, content, "heading", "new-heading", &mut changes);

        assert!(changes.is_empty());
    }

    // =========================================================================
    // Same-file reference definition edits (for rename)
    // =========================================================================

    #[test]
    fn test_collect_same_file_ref_def_edits_basic() {
        let content = "# Heading\n\n[ref]: #heading\n\nSee [ref] for info.\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_ref_def_edits(&uri, content, "heading", "new-heading", &mut changes);

        let edits = changes.get(&uri).expect("Should have edits");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "new-heading");
    }

    #[test]
    fn test_collect_same_file_ref_def_edits_no_match() {
        let content = "[ref]: #other\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_ref_def_edits(&uri, content, "heading", "new-heading", &mut changes);

        assert!(changes.is_empty());
    }

    #[test]
    fn test_collect_same_file_ref_def_edits_not_fragment_only() {
        // Reference definitions pointing to files shouldn't be caught here
        let content = "[ref]: guide.md#heading\n";
        let uri = Url::parse("file:///test.md").unwrap();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        collect_same_file_ref_def_edits(&uri, content, "heading", "new-heading", &mut changes);

        // This targets same-file fragment-only defs, so file.md#heading is not matched
        assert!(changes.is_empty());
    }

    // =========================================================================
    // find_fragment_in_link_source (for rename)
    // =========================================================================

    #[test]
    fn test_find_fragment_in_link_source_inline() {
        let source = "[text](#getting-started)";
        let pos = find_fragment_in_link_source(source, "getting-started");
        assert!(pos.is_some());
        let pos = pos.unwrap();
        assert_eq!(&source[pos..pos + "getting-started".len()], "getting-started");
    }

    #[test]
    fn test_find_fragment_in_link_source_with_hash_in_text() {
        // Display text contains #, but anchor is in URL
        let source = "[C# Guide](#c-guide)";
        let pos = find_fragment_in_link_source(source, "c-guide");
        assert!(pos.is_some());
        let pos = pos.unwrap();
        assert_eq!(&source[pos..pos + "c-guide".len()], "c-guide");
    }
}
