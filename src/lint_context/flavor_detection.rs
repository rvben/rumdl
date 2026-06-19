use crate::config::MarkdownFlavor;
use crate::utils::mkdocs_html_markdown::MarkdownHtmlTracker;

use super::ByteRanges;
use super::types::*;

/// Tracks whether we're inside a fenced code block within a MkDocs container.
///
/// MkDocs admonitions, content tabs, and markdown HTML blocks use 4-space indentation
/// which pulldown-cmark misclassifies as indented code blocks. We clear `in_code_block`
/// for container content, but must preserve it for actual fenced code blocks (``` or ~~~)
/// within those containers.
struct FencedCodeTracker {
    in_fenced_code: bool,
    fence_marker: Option<String>,
}

impl FencedCodeTracker {
    fn new() -> Self {
        Self {
            in_fenced_code: false,
            fence_marker: None,
        }
    }

    /// Process a trimmed line and update fenced code state.
    /// Returns true if currently inside a fenced code block.
    fn process_line(&mut self, trimmed: &str) -> bool {
        if !self.in_fenced_code {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = trimmed.chars().next().unwrap();
                let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
                if fence_len >= 3 {
                    self.in_fenced_code = true;
                    self.fence_marker = Some(fence_char.to_string().repeat(fence_len));
                }
            }
            self.in_fenced_code
        } else if let Some(ref marker) = self.fence_marker {
            let fence_char = marker.chars().next().unwrap();
            if trimmed.starts_with(marker.as_str())
                && trimmed
                    .chars()
                    .skip(marker.len())
                    .all(|c| c == fence_char || c.is_whitespace())
            {
                // The closing fence is still part of the code block for the
                // current line, so return true. Subsequent lines will see
                // in_fenced_code = false.
                self.in_fenced_code = false;
                self.fence_marker = None;
                return true;
            }
            true
        } else {
            self.in_fenced_code
        }
    }

    /// Reset state when exiting a container.
    fn reset(&mut self) {
        self.in_fenced_code = false;
        self.fence_marker = None;
    }
}

/// Detect ESM import/export blocks anywhere in MDX files
/// MDX 2.0+ allows imports/exports anywhere in the document, not just at the top
pub(super) fn detect_esm_blocks(content: &str, lines: &mut [LineInfo], flavor: MarkdownFlavor) {
    // Only process MDX files
    if !flavor.supports_esm_blocks() {
        return;
    }

    let mut in_multiline_import = false;

    for line in lines.iter_mut() {
        // Skip code blocks, front matter, and HTML comments
        if line.in_code_block || line.in_front_matter || line.in_html_comment {
            in_multiline_import = false;
            continue;
        }

        let line_content = line.content(content);
        let trimmed = line_content.trim();

        // Handle continuation of multi-line import/export
        if in_multiline_import {
            line.in_esm_block = true;
            // Check if this line completes the statement
            // Multi-line import ends when we see the closing quote + optional semicolon
            if trimmed.ends_with('\'')
                || trimmed.ends_with('"')
                || trimmed.ends_with("';")
                || trimmed.ends_with("\";")
                || line_content.contains(';')
            {
                in_multiline_import = false;
            }
            continue;
        }

        // Skip blank lines
        if line.is_blank {
            continue;
        }

        // Check if line starts with import or export
        if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
            line.in_esm_block = true;

            // Determine if this is a complete single-line statement or starts a multi-line one
            let is_import = trimmed.starts_with("import ");

            // Check for simple complete statements
            let is_complete =
                // Ends with semicolon
                trimmed.ends_with(';')
                // import/export with from clause that ends with quote
                || (trimmed.contains(" from ") && (trimmed.ends_with('\'') || trimmed.ends_with('"')))
                // Simple export (export const/let/var/function/class without from)
                || (!is_import && !trimmed.contains(" from ") && (
                    trimmed.starts_with("export const ")
                    || trimmed.starts_with("export let ")
                    || trimmed.starts_with("export var ")
                    || trimmed.starts_with("export function ")
                    || trimmed.starts_with("export class ")
                    || trimmed.starts_with("export default ")
                ));

            if !is_complete && is_import {
                // Only imports can span multiple lines in the typical case
                if trimmed.contains('{') && !trimmed.contains('}') {
                    in_multiline_import = true;
                }
            }
        }
    }
}

/// Detect JSX component blocks in MDX files.
///
/// JSX components use uppercase-first naming (React convention) to distinguish from HTML.
/// Lines between matched opening and closing JSX component tags are marked with `in_jsx_block`.
/// Also clears false `in_code_block` flags for indented content inside JSX blocks
/// (pulldown-cmark misclassifies 4-space indented content as indented code blocks).
pub(super) fn detect_jsx_blocks(content: &str, lines: &mut [LineInfo], flavor: MarkdownFlavor) {
    if !flavor.supports_jsx() {
        return;
    }

    let mut tag_stack: Vec<(String, usize)> = Vec::new();

    for i in 0..lines.len() {
        if lines[i].in_front_matter || lines[i].in_html_comment {
            continue;
        }

        let line_content = lines[i].content(content);
        let trimmed = line_content.trim();

        // Skip lines in code blocks that don't contain '<' — they can't have JSX tags
        if lines[i].in_code_block && !trimmed.contains('<') {
            continue;
        }

        for tag in scan_jsx_tags(trimmed) {
            if tag.is_self_closing {
                lines[i].in_jsx_block = true;
                continue;
            }

            if tag.is_closing {
                // Find the matching opening tag (innermost match)
                if let Some(pos) = tag_stack.iter().rposition(|(name, _)| name == tag.name) {
                    let (_tag_name, start_idx) = tag_stack.remove(pos);
                    for line in &mut lines[start_idx..=i] {
                        line.in_jsx_block = true;
                    }
                }
            } else {
                // Check if the closing tag is on the same line (after the opening tag)
                let after_tag = &trimmed[tag.end_offset..];
                if has_closing_tag(after_tag, tag.name) {
                    lines[i].in_jsx_block = true;
                } else {
                    tag_stack.push((tag.name.to_owned(), i));
                }
            }
        }
    }

    // Reconcile `in_code_block` for content inside JSX blocks. pulldown-cmark
    // classifies the whole component as one HTML block, so it neither marks a
    // nested fenced code block as code (a false negative that let MD034 rewrite
    // URLs inside ```bash``` fences - issue #678) nor is reliable about 4-space
    // indented content (a false positive). Re-derive the flag from the fence
    // markers: lines inside a fence are code, everything else is not.
    let mut fenced_code = FencedCodeTracker::new();
    for line in lines.iter_mut() {
        if line.in_jsx_block {
            let trimmed = line.content(content).trim();
            line.in_code_block = fenced_code.process_line(trimmed);
        } else {
            fenced_code.reset();
        }
    }
}

/// A JSX tag found during line scanning.
struct JsxTag<'a> {
    name: &'a str,
    is_closing: bool,
    is_self_closing: bool,
    /// Byte offset in the line where the tag ends (after `>`)
    end_offset: usize,
}

/// Scan a line for all JSX component tags (uppercase-first names).
/// Handles multiple tags per line and skips quoted attribute strings.
fn scan_jsx_tags(line: &str) -> Vec<JsxTag<'_>> {
    let mut tags = Vec::new();
    let bytes = line.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] != b'<' {
            pos += 1;
            continue;
        }

        let rest = &line[pos..];
        let after_bracket = &rest[1..];
        let is_closing = after_bracket.starts_with('/');
        let tag_start_str = if is_closing { &after_bracket[1..] } else { after_bracket };

        // JSX components must start with an uppercase ASCII letter
        match tag_start_str.as_bytes().first() {
            Some(&c) if c.is_ascii_uppercase() => {}
            _ => {
                pos += 1;
                continue;
            }
        }

        // Read the component name (alphanumeric, dot, underscore)
        let name_len = tag_start_str
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'.' || *c == b'_')
            .count();
        if name_len == 0 {
            pos += 1;
            continue;
        }
        let name = &tag_start_str[..name_len];

        // Scan forward to find '>', skipping quoted strings
        let scan_start = pos + 1 + usize::from(is_closing) + name_len;
        let mut j = scan_start;
        let mut in_string = false;
        let mut string_char = b'"';
        let mut found_end = false;
        let mut is_self_closing = false;

        while j < bytes.len() {
            let c = bytes[j];
            if in_string {
                if c == string_char && (j == 0 || bytes[j - 1] != b'\\') {
                    in_string = false;
                }
            } else if c == b'"' || c == b'\'' {
                in_string = true;
                string_char = c;
            } else if c == b'>' {
                is_self_closing = !is_closing && j > 0 && bytes[j - 1] == b'/';
                found_end = true;
                j += 1;
                break;
            }
            j += 1;
        }

        if !found_end {
            // Tag extends beyond the line (multi-line attributes)
            tags.push(JsxTag {
                name,
                is_closing,
                is_self_closing: false,
                end_offset: line.len(),
            });
            break;
        }

        tags.push(JsxTag {
            name,
            is_closing,
            is_self_closing,
            end_offset: j,
        });
        pos = j;
    }

    tags
}

/// Check if a closing tag `</name>` exists in haystack, using byte-level comparison.
fn has_closing_tag(haystack: &str, tag_name: &str) -> bool {
    let bytes = haystack.as_bytes();
    let pattern_len = 2 + tag_name.len() + 1; // </name>
    if bytes.len() < pattern_len {
        return false;
    }
    let mut i = 0;
    while i + pattern_len <= bytes.len() {
        if bytes[i] == b'<'
            && bytes[i + 1] == b'/'
            && haystack[i + 2..].starts_with(tag_name)
            && bytes[i + 2 + tag_name.len()] == b'>'
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Detect JSX expressions {expression} and MDX comments {/* comment */} in MDX files
/// Returns (jsx_expression_ranges, mdx_comment_ranges)
pub(super) fn detect_jsx_and_mdx_comments(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
    code_blocks: &[(usize, usize)],
) -> (ByteRanges, ByteRanges) {
    // Only process MDX files
    if !flavor.supports_jsx() {
        return (Vec::new(), Vec::new());
    }

    let mut jsx_expression_ranges: Vec<(usize, usize)> = Vec::new();
    let mut mdx_comment_ranges: Vec<(usize, usize)> = Vec::new();

    // Quick check - if no braces, no JSX expressions or MDX comments
    if !content.contains('{') {
        return (jsx_expression_ranges, mdx_comment_ranges);
    }

    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Check if we're in a code block
            if code_blocks.iter().any(|(start, end)| i >= *start && i < *end) {
                i += 1;
                continue;
            }

            let start = i;

            // Check if it's an MDX comment: {/* ... */}
            if i + 2 < bytes.len() && &bytes[i + 1..i + 3] == b"/*" {
                // Find the closing */}
                let mut j = i + 3;
                while j + 2 < bytes.len() {
                    if &bytes[j..j + 2] == b"*/" && j + 2 < bytes.len() && bytes[j + 2] == b'}' {
                        let end = j + 3;
                        mdx_comment_ranges.push((start, end));

                        // Mark lines as in MDX comment
                        mark_lines_in_range(lines, content, start, end, |line| {
                            line.in_mdx_comment = true;
                        });

                        i = end;
                        break;
                    }
                    j += 1;
                }
                if j + 2 >= bytes.len() {
                    // Unclosed MDX comment - mark rest as comment
                    mdx_comment_ranges.push((start, bytes.len()));
                    mark_lines_in_range(lines, content, start, bytes.len(), |line| {
                        line.in_mdx_comment = true;
                    });
                    break;
                }
            } else {
                // Regular JSX expression: { ... }
                // Need to handle nested braces
                let mut brace_depth = 1;
                let mut j = i + 1;
                let mut in_string = false;
                let mut string_char = b'"';

                while j < bytes.len() && brace_depth > 0 {
                    let c = bytes[j];

                    // Handle strings to avoid counting braces inside them
                    if !in_string && (c == b'"' || c == b'\'' || c == b'`') {
                        in_string = true;
                        string_char = c;
                    } else if in_string && c == string_char && (j == 0 || bytes[j - 1] != b'\\') {
                        in_string = false;
                    } else if !in_string {
                        if c == b'{' {
                            brace_depth += 1;
                        } else if c == b'}' {
                            brace_depth -= 1;
                        }
                    }
                    j += 1;
                }

                if brace_depth == 0 {
                    let end = j;
                    jsx_expression_ranges.push((start, end));

                    // Mark lines as in JSX expression
                    mark_lines_in_range(lines, content, start, end, |line| {
                        line.in_jsx_expression = true;
                    });

                    i = end;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    (jsx_expression_ranges, mdx_comment_ranges)
}

/// Detect `<div markdown>`-style HTML blocks and populate `in_mkdocs_html_markdown`.
///
/// The `markdown` attribute on a block-level HTML element is Python-Markdown's
/// `md_in_html` opt-in and is also used by MkDocs Material for constructs like
/// grid cards. Because the attribute is an unambiguous author-supplied signal,
/// we recognize these blocks regardless of the configured flavor — otherwise
/// `rumdl fmt` can silently mangle a page (e.g. rewriting 4-space-indented
/// continuation content as indented code blocks) when the flavor isn't set.
///
/// Also clears `in_code_block` for content inside such blocks (outside fenced
/// code), mirroring the admonition/tab handling: pulldown-cmark otherwise
/// treats the 4-space-indented continuation content as indented code.
pub(super) fn detect_markdown_html_blocks(content_lines: &[&str], lines: &mut [LineInfo]) {
    let mut markdown_html_tracker = MarkdownHtmlTracker::new();
    let mut html_markdown_fence = FencedCodeTracker::new();

    for (i, line) in content_lines.iter().enumerate() {
        if i >= lines.len() {
            break;
        }

        lines[i].in_mkdocs_html_markdown = markdown_html_tracker.process_line(line);

        if lines[i].in_mkdocs_html_markdown {
            let in_fenced = html_markdown_fence.process_line(line.trim());
            if !in_fenced {
                lines[i].in_code_block = false;
            }
        } else {
            html_markdown_fence.reset();
        }
    }
}

/// Detect MkDocs-specific constructs (admonitions, tabs, definition lists)
/// and populate the corresponding fields in LineInfo
pub(super) fn detect_mkdocs_line_info(content_lines: &[&str], lines: &mut [LineInfo], flavor: MarkdownFlavor) {
    if flavor != MarkdownFlavor::MkDocs {
        return;
    }

    use crate::utils::mkdocs_admonitions;
    use crate::utils::mkdocs_definition_lists;
    use crate::utils::mkdocs_tabs;

    // Track admonition context
    let mut in_admonition = false;
    let mut admonition_indent = 0;
    let mut admonition_fence = FencedCodeTracker::new();

    // Track tab context
    let mut in_tab = false;
    let mut tab_indent = 0;
    let mut tab_fence = FencedCodeTracker::new();

    // Track definition list context
    let mut in_definition = false;

    for (i, line) in content_lines.iter().enumerate() {
        if i >= lines.len() {
            break;
        }

        // Check for admonition markers first - even on lines marked as code blocks
        // Pulldown-cmark marks 4-space indented content as indented code blocks,
        // but in MkDocs this is admonition/tab content, not code.
        if mkdocs_admonitions::is_admonition_start(line) {
            in_admonition = true;
            admonition_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);
            lines[i].in_admonition = true;
            // Nested admonition start lines (indented 4+ spaces) are misclassified as
            // indented code blocks by pulldown-cmark. Clear that flag.
            lines[i].in_code_block = false;
            admonition_fence.reset();
        } else if in_admonition {
            let in_fenced = admonition_fence.process_line(line.trim());

            // Check if still in admonition content
            if line.trim().is_empty() || mkdocs_admonitions::is_admonition_content(line, admonition_indent) {
                lines[i].in_admonition = true;
                if !in_fenced {
                    lines[i].in_code_block = false;
                }
            } else {
                in_admonition = false;
                admonition_fence.reset();
                if mkdocs_admonitions::is_admonition_start(line) {
                    in_admonition = true;
                    admonition_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);
                    lines[i].in_admonition = true;
                }
            }
        }

        // Check for tab markers - also before the code block skip
        // Tab content also uses 4-space indentation which pulldown-cmark treats as code
        if mkdocs_tabs::is_tab_marker(line) {
            in_tab = true;
            tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
            lines[i].in_content_tab = true;
            tab_fence.reset();
        } else if in_tab {
            let in_fenced = tab_fence.process_line(line.trim());

            if line.trim().is_empty() || mkdocs_tabs::is_tab_content(line, tab_indent) {
                lines[i].in_content_tab = true;
                if !in_fenced {
                    lines[i].in_code_block = false;
                }
            } else {
                in_tab = false;
                tab_fence.reset();
                if mkdocs_tabs::is_tab_marker(line) {
                    in_tab = true;
                    tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
                    lines[i].in_content_tab = true;
                }
            }
        }

        // Skip remaining detection for lines in actual code blocks
        if lines[i].in_code_block {
            continue;
        }

        // Check for definition list items
        if mkdocs_definition_lists::is_definition_line(line) {
            in_definition = true;
            lines[i].in_definition_list = true;
        } else if in_definition {
            // Check if continuation
            if mkdocs_definition_lists::is_definition_continuation(line) {
                lines[i].in_definition_list = true;
            } else if line.trim().is_empty() {
                // Blank line might continue definition
                lines[i].in_definition_list = true;
            } else if mkdocs_definition_lists::could_be_term_line(line) {
                // This could be a new term - check if followed by definition
                if i + 1 < content_lines.len() && mkdocs_definition_lists::is_definition_line(content_lines[i + 1]) {
                    lines[i].in_definition_list = true;
                } else {
                    in_definition = false;
                }
            } else {
                in_definition = false;
            }
        } else if mkdocs_definition_lists::could_be_term_line(line) {
            // Check if this is a term followed by a definition
            if i + 1 < content_lines.len() && mkdocs_definition_lists::is_definition_line(content_lines[i + 1]) {
                lines[i].in_definition_list = true;
                in_definition = true;
            }
        }
    }
}

/// Detect Obsidian comment blocks (%%...%%) in Obsidian flavor
///
/// Obsidian comments use `%%` as delimiters:
/// - Inline: `text %%hidden%% text`
/// - Block: `%%\nmulti-line\n%%`
///
/// Comments do NOT nest - the first `%%` after an opening `%%` closes the comment.
/// Comments are NOT detected inside code blocks or HTML comments.
///
/// Returns the computed comment ranges for use by rules that need position-level checking.
pub(super) fn detect_obsidian_comments(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
    code_span_ranges: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    // Only process Obsidian files
    if flavor != MarkdownFlavor::Obsidian {
        return Vec::new();
    }

    // Compute Obsidian comment ranges (byte ranges)
    let comment_ranges = compute_obsidian_comment_ranges(content, lines, code_span_ranges);

    // Mark lines that fall within comment ranges
    for range in &comment_ranges {
        for line in lines.iter_mut() {
            // Skip lines in code blocks or HTML comments - they take precedence
            if line.in_code_block || line.in_html_comment {
                continue;
            }

            let line_start = line.byte_offset;
            let line_end = line.byte_offset + line.byte_len;

            // Check if this line is entirely within a comment
            if line_start >= range.0 && line_end <= range.1 {
                line.in_obsidian_comment = true;
            } else if line_start < range.1 && line_end > range.0 {
                // Line partially overlaps with comment
                let line_content_start = line_start;
                let line_content_end = line_end;

                if line_content_start >= range.0 && line_content_end <= range.1 {
                    line.in_obsidian_comment = true;
                }
            }
        }
    }

    comment_ranges
}

/// Compute byte ranges for all Obsidian comments in the content
///
/// Returns a vector of (start, end) byte offset pairs for each comment.
/// Comments do not nest - first `%%` after an opening `%%` closes it.
pub(super) fn compute_obsidian_comment_ranges(
    content: &str,
    lines: &[LineInfo],
    code_span_ranges: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    // Quick check - if no %% at all, no comments
    if !content.contains("%%") {
        return ranges;
    }

    // Build skip ranges for code blocks, HTML comments, and inline code spans
    // to avoid detecting %% inside those regions.
    let mut skip_ranges: Vec<(usize, usize)> = Vec::new();
    for line in lines {
        if line.in_code_block || line.in_html_comment {
            skip_ranges.push((line.byte_offset, line.byte_offset + line.byte_len));
        }
    }
    skip_ranges.extend(code_span_ranges.iter().copied());

    if !skip_ranges.is_empty() {
        // Sort and merge overlapping ranges for efficient scanning
        skip_ranges.sort_by_key(|(start, _)| *start);
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(skip_ranges.len());
        for (start, end) in skip_ranges {
            if let Some((_, last_end)) = merged.last_mut()
                && start <= *last_end
            {
                *last_end = (*last_end).max(end);
                continue;
            }
            merged.push((start, end));
        }
        skip_ranges = merged;
    }

    let content_bytes = content.as_bytes();
    let len = content.len();
    let mut i = 0;
    let mut in_comment = false;
    let mut comment_start = 0;
    let mut skip_idx = 0;

    while i < len.saturating_sub(1) {
        // Fast-skip any ranges we should ignore (code blocks, HTML comments, code spans)
        if skip_idx < skip_ranges.len() {
            let (skip_start, skip_end) = skip_ranges[skip_idx];
            if i >= skip_end {
                skip_idx += 1;
                continue;
            }
            if i >= skip_start {
                i = skip_end;
                continue;
            }
        }

        // Check for %%
        if content_bytes[i] == b'%' && content_bytes[i + 1] == b'%' {
            if !in_comment {
                // Opening %%
                in_comment = true;
                comment_start = i;
                i += 2;
            } else {
                // Closing %%
                let comment_end = i + 2;
                ranges.push((comment_start, comment_end));
                in_comment = false;
                i += 2;
            }
        } else {
            i += 1;
        }
    }

    // Handle unclosed comment - extends to end of document
    if in_comment {
        ranges.push((comment_start, len));
    }

    ranges
}

/// Detect kramdown-specific constructs (extension blocks, IALs, ALDs)
/// and populate the corresponding fields in LineInfo
pub(super) fn detect_kramdown_line_info(content: &str, lines: &mut [LineInfo], flavor: MarkdownFlavor) {
    if !flavor.supports_kramdown_syntax() {
        return;
    }

    use crate::utils::kramdown_utils;

    let mut in_extension_block = false;

    for line in lines.iter_mut() {
        let line_content = line.content(content);
        let trimmed = line_content.trim();

        // Extension block tracking takes priority over base parser flags.
        // The base parser doesn't know about kramdown extensions, so it may
        // mark lines inside {::nomarkdown} or {::comment} as code blocks
        // or HTML blocks. We need to keep tracking the extension block
        // through these regions.
        if in_extension_block {
            line.in_kramdown_extension_block = true;
            if kramdown_utils::is_kramdown_extension_close(trimmed) {
                in_extension_block = false;
            }
            continue;
        }

        // Outside extension blocks, skip code blocks, front matter, and HTML comments
        if line.in_code_block || line.in_front_matter || line.in_html_comment {
            continue;
        }

        // Check for self-closing extension blocks first ({::options ... /}, {::comment /})
        if kramdown_utils::is_kramdown_extension_self_closing(trimmed) {
            line.in_kramdown_extension_block = true;
            continue;
        }

        // Check for multi-line extension block opening
        if kramdown_utils::is_kramdown_extension_open(trimmed) {
            line.in_kramdown_extension_block = true;
            in_extension_block = true;
            continue;
        }

        // Check for block IAL or ALD (standalone lines with {: ...} syntax)
        if kramdown_utils::is_kramdown_block_attribute(trimmed) {
            line.is_kramdown_block_ial = true;
        }
    }
}

/// Helper to mark lines within a byte range
pub(super) fn mark_lines_in_range<F>(lines: &mut [LineInfo], content: &str, start: usize, end: usize, mut f: F)
where
    F: FnMut(&mut LineInfo),
{
    // Find lines that overlap with the range
    for line in lines.iter_mut() {
        let line_start = line.byte_offset;
        let line_end = line.byte_offset + line.byte_len;

        // Check if this line overlaps with the range
        if line_start < end && line_end > start {
            f(line);
        }
    }

    // Silence unused warning for content (needed for signature consistency)
    let _ = content;
}

/// Count leading ASCII space characters (tabs do not count).
fn count_leading_spaces(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b' ').count()
}

/// A colon fence opener is 0–3 leading spaces, then `:::`, then at least one
/// non-whitespace character. Tabs before `:::` disqualify the line.
fn is_colon_fence_opener(line: &str) -> bool {
    let spaces = count_leading_spaces(line);
    if spaces > 3 {
        return false;
    }
    let rest = &line[spaces..];
    if rest.starts_with('\t') {
        return false;
    }
    rest.starts_with(":::") && !rest[3..].trim().is_empty()
}

/// A colon fence closer is 0–3 leading spaces, then `:::`, then only whitespace.
fn is_colon_fence_closer(line: &str) -> bool {
    let spaces = count_leading_spaces(line);
    if spaces > 3 {
        return false;
    }
    let rest = &line[spaces..];
    if rest.starts_with('\t') {
        return false;
    }
    rest.starts_with(":::") && rest[3..].trim().is_empty()
}

/// Detect Azure DevOps colon code fences (`:::lang … :::`) and mark their
/// lines as `in_code_block`. Returns byte ranges for each detected fence so
/// the caller can extend `code_blocks` for byte-range consumers.
///
/// Only runs when `flavor.supports_colon_code_fences()`. Skips lines already
/// in front matter or HTML comments. Nesting is not supported — the first bare
/// `:::` after an opener closes the block.
pub(super) fn detect_azure_colon_fences(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
) -> Vec<(usize, usize)> {
    if !flavor.supports_colon_code_fences() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut fence_byte_start: Option<usize> = None;

    for line in lines.iter_mut() {
        if line.in_front_matter || line.in_html_comment {
            continue;
        }

        let line_content = line.content(content);

        if fence_byte_start.is_none() {
            if is_colon_fence_opener(line_content) {
                fence_byte_start = Some(line.byte_offset);
                line.in_code_block = true;
            }
        } else {
            // Inside an open fence — mark everything as code.
            line.in_code_block = true;

            if is_colon_fence_closer(line_content) {
                let start = fence_byte_start.take().unwrap();
                // End is exclusive: byte after the last byte of the closer line
                // (including its newline if present).
                let end = (line.byte_offset + line.byte_len + 1).min(content.len());
                ranges.push((start, end));
            }
        }
    }

    // Unclosed fence — extend to end of document.
    if let Some(start) = fence_byte_start {
        ranges.push((start, content.len()));
    }

    ranges
}

#[cfg(test)]
mod colon_fence_tests {
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn azure_ctx(content: &str) -> LintContext<'_> {
        LintContext::new(content, MarkdownFlavor::AzureDevOps, None)
    }

    fn standard_ctx(content: &str) -> LintContext<'_> {
        LintContext::new(content, MarkdownFlavor::Standard, None)
    }

    #[test]
    fn test_colon_fence_basic_marks_content_as_code_block() {
        let content = "::: mermaid\nflowchart LR\n    A --> B\n:::\n";
        let ctx = azure_ctx(content);
        assert!(ctx.lines[0].in_code_block, "opener should be in_code_block");
        assert!(ctx.lines[1].in_code_block, "content should be in_code_block");
        assert!(ctx.lines[2].in_code_block, "content should be in_code_block");
        assert!(ctx.lines[3].in_code_block, "closer should be in_code_block");
    }

    #[test]
    fn test_colon_fence_no_space_variant() {
        let content = ":::mermaid\ndata\n:::\n";
        let ctx = azure_ctx(content);
        assert!(ctx.lines[0].in_code_block);
        assert!(ctx.lines[1].in_code_block);
        assert!(ctx.lines[2].in_code_block);
    }

    #[test]
    fn test_colon_fence_space_variant() {
        let content = "::: mermaid\ndata\n:::\n";
        let ctx = azure_ctx(content);
        assert!(ctx.lines[0].in_code_block);
        assert!(ctx.lines[1].in_code_block);
        assert!(ctx.lines[2].in_code_block);
    }

    #[test]
    fn test_bare_colon_without_opener_is_not_a_block() {
        let content = "Some text\n:::\nMore text\n";
        let ctx = azure_ctx(content);
        assert!(!ctx.lines[0].in_code_block);
        assert!(
            !ctx.lines[1].in_code_block,
            "bare ::: without opener should not be code block"
        );
        assert!(!ctx.lines[2].in_code_block);
    }

    #[test]
    fn test_four_leading_spaces_is_not_opener() {
        let content = "    ::: mermaid\ndata\n:::\n";
        let ctx = azure_ctx(content);
        // 4 spaces = indented code, not a colon opener
        assert!(!ctx.lines[1].in_code_block, "content should not be in_code_block");
    }

    #[test]
    fn test_three_leading_spaces_is_opener() {
        let content = "   ::: mermaid\ndata\n   :::\n";
        let ctx = azure_ctx(content);
        assert!(ctx.lines[0].in_code_block);
        assert!(ctx.lines[1].in_code_block);
        assert!(ctx.lines[2].in_code_block);
    }

    #[test]
    fn test_colon_fence_inside_front_matter_ignored() {
        let content = "---\ntitle: test\n---\n::: mermaid\ndata\n:::\n";
        let ctx = azure_ctx(content);
        // Front matter lines 0-2 are in_front_matter; colon block starts at line 3
        assert!(ctx.lines[3].in_code_block);
        assert!(ctx.lines[4].in_code_block);
        assert!(ctx.lines[5].in_code_block);
    }

    #[test]
    fn test_standard_flavor_does_not_treat_colon_as_code_block() {
        let content = "::: mermaid\nflowchart LR\n    A --> B\n:::\n";
        let ctx = standard_ctx(content);
        for line in &ctx.lines {
            assert!(
                !line.in_code_block,
                "standard flavor should not mark colon blocks as code"
            );
        }
    }

    #[test]
    fn test_colon_fence_byte_ranges_in_code_blocks() {
        let content = "text\n::: mermaid\ndiagram\n:::\nafter\n";
        let ctx = azure_ctx(content);
        let diagram_line_start = ctx.lines[2].byte_offset;
        let in_block = ctx
            .code_blocks
            .iter()
            .any(|&(s, e)| diagram_line_start >= s && diagram_line_start < e);
        assert!(in_block, "diagram line should be in code_blocks byte ranges");
    }

    #[test]
    fn test_colon_fence_content_not_flagged_by_md013() {
        use crate::rule::Rule;
        use crate::rules::md013_line_length::MD013LineLength;
        let long_line = "A".repeat(200);
        let content = format!("::: mermaid\n{long_line}\n:::\n");
        let ctx = azure_ctx(&content);
        let rule = MD013LineLength::default();
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD013 should not fire inside colon fence: {warnings:?}"
        );
    }
}

// ============================================================================
// MyST Markdown Detection
// ============================================================================

/// Check if a line is a MyST colon directive opener.
/// Pattern: 0-3 leading spaces, 3+ colons, immediately followed by `{name}`.
/// Returns the colon count if it's an opener, None otherwise.
pub(super) fn myst_colon_directive_opener(line: &str) -> Option<usize> {
    let spaces = count_leading_spaces(line);
    if spaces > 3 {
        return None;
    }
    let rest = &line[spaces..];
    if rest.starts_with('\t') {
        return None;
    }
    let colon_count = rest.bytes().take_while(|&b| b == b':').count();
    if colon_count < 3 {
        return None;
    }
    let after_colons = &rest[colon_count..];
    if after_colons.starts_with('{') && after_colons.contains('}') {
        let name = after_colons.trim_start_matches('{').split('}').next().unwrap_or("");
        if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            return Some(colon_count);
        }
    }
    None
}

/// Detect MyST colon fence directives (`:::{name} ... :::`) and mark their
/// lines as `in_myst_directive`. Returns byte ranges for each detected directive.
///
/// Supports nesting: outer directives use more colons than inner ones.
pub(super) fn detect_myst_colon_directives(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
) -> Vec<(usize, usize)> {
    if !flavor.supports_myst_directives() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    // Stack of (colon_count, byte_start) for nested directives
    let mut stack: Vec<(usize, usize)> = Vec::new();
    // True while the lines immediately following an opener are still the directive's
    // option block (`:key: value`), before any prose body. Mirrors the backtick path:
    // option lines are structural metadata, not prose, so they are flagged
    // `in_code_block` to keep reflow (MD013) from joining them.
    let mut in_option_region = false;

    for line in lines.iter_mut() {
        if line.in_front_matter || line.in_html_comment || line.in_code_block {
            continue;
        }

        let line_content = line.content(content);

        if let Some(colon_count) = myst_colon_directive_opener(line_content) {
            stack.push((colon_count, line.byte_offset));
            line.in_myst_directive = true;
            in_option_region = true;
        } else if !stack.is_empty() {
            // Check if this is a closer for any level in the stack
            // Closers match the innermost directive with <= colon count
            let spaces = count_leading_spaces(line_content);
            let rest = if spaces <= 3 { &line_content[spaces..] } else { "" };
            let colon_count = rest.bytes().take_while(|&b| b == b':').count();
            let is_bare_colons = colon_count >= 3 && rest[colon_count..].trim().is_empty();

            if is_bare_colons {
                in_option_region = false;
                // Find the innermost directive this closer matches
                // A closer with N colons closes the innermost directive with <= N colons
                if let Some(pos) = stack.iter().rposition(|&(c, _)| c <= colon_count) {
                    let (_, start) = stack.remove(pos);
                    // Also remove any inner directives that were opened after this one
                    stack.truncate(pos);
                    let end = (line.byte_offset + line.byte_len + 1).min(content.len());
                    ranges.push((start, end));
                    line.in_myst_directive = true;
                } else {
                    // Not a valid closer, mark as directive content if inside one
                    line.in_myst_directive = true;
                }
            } else {
                // Regular content inside a directive.
                line.in_myst_directive = true;
                let trimmed = line_content.trim();
                let is_option_line = trimmed.starts_with(':') && trimmed.len() > 1 && trimmed[1..].contains(':');
                if in_option_region && is_option_line {
                    // Structural option line: keep verbatim, do not reflow.
                    line.in_code_block = true;
                } else {
                    // First prose/blank line ends the option block; the body reflows.
                    in_option_region = false;
                }
            }
        }
    }

    // Unclosed directives extend to end of document
    for (_, start) in stack {
        ranges.push((start, content.len()));
    }

    ranges.sort_by_key(|&(s, _)| s);
    ranges
}

/// Detect MyST `%` comments and mark their lines.
/// Pattern: 0-3 leading spaces followed by `%` then space or end of line.
pub(super) fn detect_myst_comments(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
) -> Vec<(usize, usize)> {
    if !flavor.supports_myst_comments() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for line in lines.iter_mut() {
        if line.in_code_block || line.in_front_matter || line.in_html_comment || line.in_myst_directive {
            continue;
        }

        let line_content = line.content(content);
        let spaces = count_leading_spaces(line_content);
        if spaces > 3 {
            continue;
        }
        let rest = &line_content[spaces..];
        if rest.starts_with('%') && (rest.len() == 1 || rest.as_bytes().get(1) == Some(&b' ')) {
            line.is_myst_comment = true;
            let end = (line.byte_offset + line.byte_len + 1).min(content.len());
            ranges.push((line.byte_offset, end));
        }
    }

    ranges
}

/// Known MyST content-bearing directives whose body should be linted as markdown.
const MYST_CONTENT_DIRECTIVES: &[&str] = &[
    "note",
    "warning",
    "tip",
    "hint",
    "important",
    "caution",
    "danger",
    "admonition",
    "attention",
    "error",
    "seealso",
    "topic",
    "sidebar",
    "margin",
    "exercise",
    "solution",
    "dropdown",
    "tab-item",
    "grid",
    "card",
    "tab-set",
    "toggle",
    "proof",
    "prf:proof",
    "prf:theorem",
    "prf:lemma",
    "prf:definition",
    "prf:criterion",
    "prf:remark",
    "prf:conjecture",
    "prf:corollary",
    "prf:algorithm",
    "prf:example",
    "prf:property",
    "prf:observation",
    "prf:proposition",
    "prf:assumption",
    "figure",
    "table",
    "list-table",
    "csv-table",
];

/// Check if a MyST directive name is content-bearing (body is markdown, not code).
fn is_myst_content_directive(name: &str) -> bool {
    MYST_CONTENT_DIRECTIVES.contains(&name)
}

/// Detect MyST backtick directives (` ```{name} `) and clear `in_code_block` for
/// content-bearing directives so their body is linted as markdown.
///
/// Code-bearing directives ({code-cell}, {code-block}, {raw}, etc.) keep `in_code_block = true`.
pub(super) fn detect_myst_backtick_directives(
    content: &str,
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
    code_block_details: &[crate::utils::code_block_utils::CodeBlockDetail],
    line_offsets: &[usize],
) {
    if !flavor.supports_myst_directives() {
        return;
    }

    for block in code_block_details {
        if !block.is_fenced {
            continue;
        }

        // Check if info string matches MyST directive pattern: {name} or {name} args
        let info = block.info_string.trim();
        if !info.starts_with('{') || !info.contains('}') {
            continue;
        }

        let name = info.trim_start_matches('{').split('}').next().unwrap_or("");
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            continue;
        }

        // Find the line index for this block's start byte offset
        let start_line_idx = line_offsets
            .partition_point(|&offset| offset <= block.start)
            .saturating_sub(1);
        let end_line_idx = line_offsets
            .partition_point(|&offset| offset < block.end)
            .min(lines.len());

        // Mark the opener line as a directive
        if let Some(line) = lines.get_mut(start_line_idx) {
            line.in_myst_directive = true;
        }

        // For content-bearing directives, clear in_code_block for body lines
        if is_myst_content_directive(name) {
            let mut past_options = false;
            let mut fence_tracker = FencedCodeTracker::new();

            for i in (start_line_idx + 1)..end_line_idx {
                if i >= lines.len() {
                    break;
                }

                let line_content = lines[i].content(content);
                let trimmed = line_content.trim();

                // Check if this is the closing fence line. The fence is structural,
                // not prose: keep its `in_code_block` flag (set by the base fenced-block
                // detection) so reflow rules like MD013 leave it verbatim and never join
                // it into the directive body.
                let is_closer =
                    trimmed.starts_with("```") && trimmed.chars().skip(3).all(|c| c == '`' || c.is_whitespace());
                if is_closer {
                    lines[i].in_myst_directive = true;
                    break;
                }

                // Option lines (`:key: value`) and the YAML options delimiter (`---`)
                // are structural directive metadata, not markdown prose. Keep their
                // `in_code_block` flag so reflow leaves them on their own lines; only
                // the directive's content body (a `{figure}` caption, a `{note}` body)
                // is cleared below so it reflows as markdown.
                if !past_options {
                    let is_option_line = trimmed.starts_with(':') && trimmed.len() > 1 && trimmed[1..].contains(':');
                    let is_yaml_delimiter = trimmed.starts_with("---");
                    if is_option_line || is_yaml_delimiter {
                        lines[i].in_myst_directive = true;
                        continue;
                    }
                    past_options = true;
                }

                // Track nested fenced code blocks within the directive body
                let in_nested_fence = fence_tracker.process_line(trimmed);
                lines[i].in_myst_directive = true;
                if !in_nested_fence {
                    lines[i].in_code_block = false;
                }
            }
        } else {
            // Code-bearing directive (e.g. `{eval-rst}`, `{code-block}`): the body is
            // opaque code, so keep `in_code_block` set on the body lines. Mark every
            // line of the directive (opener, body, closer) as `in_myst_directive` so
            // rules that skip directive structure (MD046, MD048) treat the whole block
            // as one directive. Marking only the fences left the indented body lines
            // looking like a standalone indented code block to those rules.
            let body_end = end_line_idx.min(lines.len());
            for line in &mut lines[start_line_idx..body_end] {
                line.in_myst_directive = true;
            }
        }
    }
}

/// Detect MyST role syntax (`{rolename}`content``) and return byte ranges.
/// Roles look like `{name}`content`` where name is alphanumeric with hyphens/underscores.
pub(super) fn detect_myst_role_ranges(
    content: &str,
    lines: &[LineInfo],
    flavor: MarkdownFlavor,
    code_blocks: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    if !flavor.supports_myst_roles() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let bytes = content.as_bytes();

    for line in lines {
        if line.in_code_block || line.in_front_matter || line.in_html_comment {
            continue;
        }

        let line_start = line.byte_offset;
        let line_end = line.byte_offset + line.byte_len;
        let line_bytes = &bytes[line_start..line_end];

        let mut i = 0;
        while i < line_bytes.len() {
            // Look for `{` that starts a role
            if line_bytes[i] != b'{' {
                i += 1;
                continue;
            }

            let role_start = line_start + i;

            // Check if inside a code block (byte-range check)
            if code_blocks.iter().any(|&(s, e)| role_start >= s && role_start < e) {
                i += 1;
                continue;
            }

            // Parse role name: {name}
            let mut j = i + 1;
            if j >= line_bytes.len() || !(line_bytes[j].is_ascii_alphabetic() || line_bytes[j] == b'_') {
                i += 1;
                continue;
            }
            while j < line_bytes.len()
                && (line_bytes[j].is_ascii_alphanumeric()
                    || line_bytes[j] == b'-'
                    || line_bytes[j] == b'_'
                    || line_bytes[j] == b':'
                    || line_bytes[j] == b'.')
            {
                j += 1;
            }
            if j >= line_bytes.len() || line_bytes[j] != b'}' {
                i += 1;
                continue;
            }
            j += 1; // past '}'

            // Must be immediately followed by backtick(s)
            if j >= line_bytes.len() || line_bytes[j] != b'`' {
                i += 1;
                continue;
            }

            // Count opening backticks
            let backtick_start = j;
            while j < line_bytes.len() && line_bytes[j] == b'`' {
                j += 1;
            }
            let backtick_count = j - backtick_start;

            // Find matching closing backticks
            let mut found_close = false;
            while j + backtick_count <= line_bytes.len() {
                if line_bytes[j] == b'`' {
                    let close_count = line_bytes[j..].iter().take_while(|&&b| b == b'`').count();
                    if close_count == backtick_count {
                        j += close_count;
                        found_close = true;
                        break;
                    }
                    j += close_count;
                } else {
                    j += 1;
                }
            }

            if found_close {
                let role_end = line_start + j;
                ranges.push((role_start, role_end));
                i = j;
            } else {
                i += 1;
            }
        }
    }

    ranges.sort_by_key(|&(s, _)| s);
    ranges
}

#[cfg(test)]
mod myst_tests {
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn myst_ctx(content: &str) -> LintContext<'_> {
        LintContext::new(content, MarkdownFlavor::MyST, None)
    }

    #[test]
    fn test_myst_colon_directive_basic() {
        let content = ":::{note}\nThis is a note.\n:::\n";
        let ctx = myst_ctx(content);
        assert!(ctx.lines[0].in_myst_directive);
        assert!(ctx.lines[1].in_myst_directive);
        assert!(ctx.lines[2].in_myst_directive);
        assert!(!ctx.lines[0].in_code_block);
        assert!(!ctx.lines[1].in_code_block);
    }

    #[test]
    fn test_myst_colon_directive_nested() {
        let content = "::::{note}\n:::{warning}\nInner content\n:::\nOuter content\n::::\n";
        let ctx = myst_ctx(content);
        for i in 0..6 {
            assert!(ctx.lines[i].in_myst_directive, "line {i} should be in_myst_directive");
        }
    }

    #[test]
    fn test_myst_comment() {
        let content = "% This is a comment\nRegular text\n";
        let ctx = myst_ctx(content);
        assert!(ctx.lines[0].is_myst_comment);
        assert!(!ctx.lines[1].is_myst_comment);
    }

    #[test]
    fn test_myst_comment_not_in_standard_flavor() {
        let content = "% This is a comment\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(!ctx.lines[0].is_myst_comment);
    }

    #[test]
    fn test_myst_backtick_content_directive() {
        let content = "```{note}\nThis is **markdown** content.\n```\n";
        let ctx = myst_ctx(content);
        assert!(ctx.lines[0].in_myst_directive);
        assert!(ctx.lines[1].in_myst_directive);
        assert!(
            !ctx.lines[1].in_code_block,
            "content directive body should not be in_code_block"
        );
    }

    #[test]
    fn test_myst_backtick_code_directive() {
        let content = "```{code-cell} python\nprint('hello')\n```\n";
        let ctx = myst_ctx(content);
        assert!(ctx.lines[0].in_myst_directive);
        assert!(
            ctx.lines[1].in_code_block,
            "code directive body should remain in_code_block"
        );
    }

    #[test]
    fn test_myst_role_detection() {
        let content = "See {ref}`my-label` for details.\n";
        let ctx = myst_ctx(content);
        // The role should be detected
        assert!(ctx.is_in_myst_role(4)); // byte position of `{ref}`
    }

    #[test]
    fn test_myst_role_not_in_standard_flavor() {
        let content = "See {ref}`my-label` for details.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(!ctx.is_in_myst_role(4));
    }
}
