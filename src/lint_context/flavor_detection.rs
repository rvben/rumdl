use crate::config::MarkdownFlavor;
use crate::utils::mkdocs_html_markdown::MarkdownHtmlTracker;

use super::ByteRanges;
use super::types::*;

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

/// Detect MkDocs-specific constructs (admonitions, tabs, definition lists)
/// and populate the corresponding fields in LineInfo
pub(super) fn detect_mkdocs_line_info(content: &str, lines: &mut [LineInfo], flavor: MarkdownFlavor) {
    if flavor != MarkdownFlavor::MkDocs {
        return;
    }

    use crate::utils::mkdocs_admonitions;
    use crate::utils::mkdocs_definition_lists;
    use crate::utils::mkdocs_tabs;

    let content_lines: Vec<&str> = content.lines().collect();

    // Track admonition context
    let mut in_admonition = false;
    let mut admonition_indent = 0;

    // Track tab context
    let mut in_tab = false;
    let mut tab_indent = 0;

    // Track fenced code blocks within MkDocs containers (separate from pulldown-cmark detection)
    let mut in_mkdocs_fenced_code = false;
    let mut mkdocs_fence_marker: Option<String> = None;

    // Track definition list context
    let mut in_definition = false;

    // Track markdown-enabled HTML block context (grid cards, etc.)
    let mut markdown_html_tracker = MarkdownHtmlTracker::new();

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
        } else if in_admonition {
            // Check if still in admonition content
            if line.trim().is_empty() {
                // Blank lines are part of admonitions
                lines[i].in_admonition = true;
                // Override code block detection for blank lines inside admonitions
                lines[i].in_code_block = false;
            } else if mkdocs_admonitions::is_admonition_content(line, admonition_indent) {
                lines[i].in_admonition = true;
                // Override code block detection - this is admonition content, not code
                lines[i].in_code_block = false;
            } else {
                // End of admonition
                in_admonition = false;
                // Check if this line starts a new admonition
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
            // Reset fenced code tracking when entering new tab
            in_mkdocs_fenced_code = false;
            mkdocs_fence_marker = None;
        } else if in_tab {
            let trimmed = line.trim();

            // Track fenced code blocks within tabs
            if !in_mkdocs_fenced_code {
                // Check for fence start (``` or ~~~)
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    let fence_char = trimmed.chars().next().unwrap();
                    let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
                    if fence_len >= 3 {
                        in_mkdocs_fenced_code = true;
                        mkdocs_fence_marker = Some(fence_char.to_string().repeat(fence_len));
                    }
                }
            } else if let Some(ref marker) = mkdocs_fence_marker {
                // Check for fence end (same or more chars)
                let fence_char = marker.chars().next().unwrap();
                if trimmed.starts_with(marker.as_str())
                    && trimmed
                        .chars()
                        .skip(marker.len())
                        .all(|c| c == fence_char || c.is_whitespace())
                {
                    in_mkdocs_fenced_code = false;
                    mkdocs_fence_marker = None;
                }
            }

            // Check if still in tab content
            if line.trim().is_empty() {
                // Blank lines are part of tabs
                lines[i].in_content_tab = true;
                // Only override code block if not in a fenced code block
                if !in_mkdocs_fenced_code {
                    lines[i].in_code_block = false;
                }
            } else if mkdocs_tabs::is_tab_content(line, tab_indent) {
                lines[i].in_content_tab = true;
                // Override INDENTED code block detection - this is tab content, not code
                // But preserve fenced code block detection (```...```)
                if !in_mkdocs_fenced_code {
                    lines[i].in_code_block = false;
                }
            } else {
                // End of tab content
                in_tab = false;
                in_mkdocs_fenced_code = false;
                mkdocs_fence_marker = None;
                // Check if this line starts a new tab
                if mkdocs_tabs::is_tab_marker(line) {
                    in_tab = true;
                    tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
                    lines[i].in_content_tab = true;
                }
            }
        }

        // Check for markdown-enabled HTML blocks (grid cards, etc.)
        // Supports div, section, article, aside, details, figure, footer, header, main, nav
        // with markdown, markdown="1", or markdown="block" attributes
        lines[i].in_mkdocs_html_markdown = markdown_html_tracker.process_line(line);

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
