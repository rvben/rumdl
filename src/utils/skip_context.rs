//! Utilities for determining if a position in markdown should be skipped from processing
//!
//! This module provides centralized context detection for various markdown constructs
//! that should typically be skipped when processing rules.

use crate::config::MarkdownFlavor;
use crate::lint_context::{HtmlTag, LintContext};
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_critic;
use crate::utils::mkdocs_extensions;
use crate::utils::mkdocs_footnotes;
use crate::utils::mkdocs_icons;
use crate::utils::mkdocs_snippets;
use crate::utils::mkdocs_tabs;
use regex::Regex;
use std::sync::LazyLock;

/// Enhanced inline math pattern that handles both single $ and double $$ delimiters.
/// Matches:
/// - Display math: $$...$$ (zero or more non-$ characters)
/// - Inline math: $...$ (zero or more non-$ non-newline characters)
///
/// The display math pattern is tried first to correctly handle $$content$$.
/// Critically, both patterns allow ZERO characters between delimiters,
/// so empty math like $$ or $ $ is consumed and won't pair with other $ signs.
static INLINE_MATH_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$[^$]*\$\$|\$[^$\n]*\$").unwrap());

/// Range representing a span of bytes (start inclusive, end exclusive)
#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Pre-compute all HTML comment ranges in the content.
/// Returns a sorted vector of byte ranges for efficient lookup.
pub fn compute_html_comment_ranges(content: &str) -> Vec<ByteRange> {
    compute_html_comment_ranges_filtered(content, &[], &[])
}

/// Pre-compute HTML comment ranges, treating `<!--`/`-->` inside inline code
/// spans or fenced/indented code blocks as literal text rather than comment
/// delimiters.
///
/// Inside code (a backtick code span or a code block), `<!--` and `-->` are
/// literal. A naive `<!--[\s\S]*?-->` scan would pair a `<!--` in one code
/// region with a `-->` in a later region, spuriously marking every line between
/// them as "inside an HTML comment". That made content invisible to rules that
/// skip comment lines: MD032's blank-line check produced false positives and a
/// non-converging `--fix` loop, and MD034 silently dropped a real bare URL that
/// fell between a code-block `<!--` and a later `-->`.
///
/// To stay correct even when a literal delimiter precedes a genuine comment
/// (`` `<!--` <!-- real --> ``), this scans for the next `<!--` opener that is
/// not in code, then the next `-->` closer that is not in code - it does not
/// filter completed regex matches. `code_span_ranges` and `code_block_ranges`
/// are the parser's half-open `[start, end)` byte ranges (`ParseResult`). With
/// no code ranges this is equivalent to the lazy regex: an opener pairs with the
/// first following closer, and an opener with no closer yields no range.
pub fn compute_html_comment_ranges_filtered(
    content: &str,
    code_span_ranges: &[(usize, usize)],
    code_block_ranges: &[(usize, usize)],
) -> Vec<ByteRange> {
    let in_code = |pos: usize| {
        code_span_ranges.iter().any(|&(start, end)| pos >= start && pos < end)
            || code_block_ranges.iter().any(|&(start, end)| pos >= start && pos < end)
    };

    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = content[search_from..].find("<!--") {
        let open = search_from + rel;
        if in_code(open) {
            // Literal `<!--` inside code: not a comment opener.
            search_from = open + "<!--".len();
            continue;
        }
        // Find the next `-->` that is not itself inside code.
        let mut close_from = open + "<!--".len();
        let end = loop {
            let Some(crel) = content[close_from..].find("-->") else {
                break None;
            };
            let close = close_from + crel;
            if in_code(close) {
                close_from = close + "-->".len();
                continue;
            }
            break Some(close + "-->".len());
        };
        match end {
            Some(end) => {
                ranges.push(ByteRange { start: open, end });
                search_from = end;
            }
            // Unterminated comment (no closer anywhere): the regex would not
            // match either, so emit nothing and stop.
            None => break,
        }
    }
    ranges
}

/// Check if a byte position is within any of the pre-computed HTML comment ranges
/// Uses binary search for O(log n) complexity
pub fn is_in_html_comment_ranges(ranges: &[ByteRange], byte_pos: usize) -> bool {
    // Binary search to find a range that might contain byte_pos
    ranges
        .binary_search_by(|range| {
            if byte_pos < range.start {
                std::cmp::Ordering::Greater
            } else if byte_pos >= range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Check if a line's content is ENTIRELY within a single HTML comment.
///
/// Callers must pass the byte range of the line's *content* (leading and
/// trailing whitespace trimmed off), not the raw line bounds: an indented
/// comment begins at the `<!--` after the indent, so passing the column-0
/// line start would place `content_start` before the comment range and the
/// line would never be recognised as being inside the comment.
///
/// Returns true only if both `content_start` AND `content_end` fall within the
/// same comment range.
pub fn is_line_entirely_in_html_comment(ranges: &[ByteRange], content_start: usize, content_end: usize) -> bool {
    for range in ranges {
        // If the content start is within this range, check if the content end is also within it
        if content_start >= range.start && content_start < range.end {
            return content_end <= range.end;
        }
    }
    false
}

/// Check if a byte position is within a JSX expression (MDX: {expression})
#[inline]
pub fn is_in_jsx_expression(ctx: &LintContext, byte_pos: usize) -> bool {
    ctx.flavor == MarkdownFlavor::MDX && ctx.is_in_jsx_expression(byte_pos)
}

/// Check if a byte position is within an MDX comment ({/* ... */})
#[inline]
pub fn is_in_mdx_comment(ctx: &LintContext, byte_pos: usize) -> bool {
    ctx.flavor == MarkdownFlavor::MDX && ctx.is_in_mdx_comment(byte_pos)
}

/// Check if a line should be skipped due to MkDocs snippet syntax
pub fn is_mkdocs_snippet_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_snippets::is_snippet_marker(line)
}

/// Check if a line is a MkDocs admonition marker
pub fn is_mkdocs_admonition_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_admonitions::is_admonition_marker(line)
}

/// Check if a line is a MkDocs footnote definition
pub fn is_mkdocs_footnote_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_footnotes::is_footnote_definition(line)
}

/// Check if a line is a MkDocs tab marker
pub fn is_mkdocs_tab_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_tabs::is_tab_marker(line)
}

/// Check if a line contains MkDocs Critic Markup
pub fn is_mkdocs_critic_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_critic::contains_critic_markup(line)
}

/// Check if a byte position is within an HTML tag
pub fn is_in_html_tag(ctx: &LintContext, byte_pos: usize) -> bool {
    for html_tag in ctx.html_tags().iter() {
        if html_tag.byte_offset <= byte_pos && byte_pos < html_tag.byte_end {
            return true;
        }
    }
    false
}

/// Check if a byte position is within a math context.
///
/// `$$...$$` display math is recognized only when it begins its line, via
/// [`math_block_ranges`]; a mid-line or stray-prose `$$...$$` is a literal,
/// not math. Single-`$` inline spans are recognized anywhere. This keeps
/// every math-aware rule agreeing on what is math.
pub fn is_in_math_context(ctx: &LintContext, byte_pos: usize) -> bool {
    // Use the cached ranges on the context; recomputing math_byte_ranges(content)
    // on every call made callers that invoke this per element O(elements * content).
    ctx.math_byte_ranges()
        .iter()
        .any(|&(start, end)| byte_pos >= start && byte_pos < end)
}

/// Paired `$$ ... $$` display-math byte ranges, half-open `[start, end)`.
///
/// A block only *opens* on a `$$` that begins its line, ignoring leading
/// whitespace and blockquote markers (`>`); a stray `$$` mid-prose is a
/// literal, not a block opener. This keeps the byte-level result consistent
/// with the line-level [`compute_math_block_line_map`] guard. Once open, the
/// block *closes* on the next `$$` anywhere - even when that closing `$$`
/// shares its line with LaTeX content (`\end{cases}$$`) or trailing Markdown
/// prose. An opener with no matching closer is dropped, not treated as an
/// unterminated block that swallows the rest of the document.
pub(crate) fn math_block_ranges(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();
    let mut open: Option<usize> = None;
    let mut line_start = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line_start = i + 1;
                i += 1;
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'$' => {
                match open {
                    None => {
                        // Open only when this `$$` is the first non-blank,
                        // non-blockquote content on its line.
                        let starts_line = bytes[line_start..i]
                            .iter()
                            .all(|&b| b == b' ' || b == b'\t' || b == b'>');
                        if starts_line {
                            open = Some(i);
                        }
                    }
                    Some(start) => {
                        ranges.push((start, i + 2));
                        open = None;
                    }
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    ranges
}

/// Check if a byte position is within a `$$ ... $$` display-math block.
///
/// A block opens only on a `$$` that begins its line (see [`math_block_ranges`])
/// and closes on the next `$$` anywhere, so the closing fence ends the block
/// even when it shares its line with LaTeX content (e.g. `\end{cases}$$`) or
/// trailing Markdown prose; bytes after the closing `$$` are not math.
pub fn is_in_math_block(content: &str, byte_pos: usize) -> bool {
    math_block_ranges(content)
        .iter()
        .any(|&(start, end)| byte_pos >= start && byte_pos < end)
}

/// Check if a byte position is within inline math (`$...$`).
///
/// Only single-`$` spans count here. A `$$...$$` token is display-math
/// syntax, and whether it is actually math depends solely on whether it
/// begins its line - that decision belongs to [`math_block_ranges`]. The
/// regex still consumes `$$...$$` tokens so a single-`$` span cannot straddle
/// them, but a mid-line `$$...$$` is a literal here, not inline math, keeping
/// this function consistent with the line-start-gated block model.
pub fn is_in_inline_math(content: &str, byte_pos: usize) -> bool {
    for m in INLINE_MATH_REGEX.find_iter(content) {
        if content[m.start()..m.end()].starts_with("$$") {
            continue;
        }
        if m.start() <= byte_pos && byte_pos < m.end() {
            return true;
        }
    }
    false
}

/// All math byte ranges in `content`: line-start `$$...$$` display blocks
/// plus single-`$` inline spans. Ranges are half-open `[start, end)` and may
/// be unordered relative to each other; membership is by `any`-containment.
///
/// Precompute this once when classifying many positions in one document
/// (e.g. every emphasis span). [`is_in_math_context`] is the single-shot
/// equivalent and is defined in terms of the same two sources.
pub fn math_byte_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = math_block_ranges(content);
    for m in INLINE_MATH_REGEX.find_iter(content) {
        if content[m.start()..m.end()].starts_with("$$") {
            continue;
        }
        ranges.push((m.start(), m.end()));
    }
    ranges
}

/// Check if a position is within a table cell
pub fn is_in_table_cell(ctx: &LintContext, line_num: usize, _col: usize) -> bool {
    // Check if this line is part of a table
    for table_row in ctx.table_rows().iter() {
        if table_row.line == line_num {
            // This line is part of a table
            // For now, we'll skip the entire table row
            // Future enhancement: check specific column boundaries
            return true;
        }
    }
    false
}

/// Check if a line contains table syntax
pub fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Check for table separator line
    if trimmed
        .chars()
        .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
        && trimmed.contains('|')
        && trimmed.contains('-')
    {
        return true;
    }

    // Check for table content line (starts and/or ends with |)
    if (trimmed.starts_with('|') || trimmed.ends_with('|')) && trimmed.matches('|').count() >= 2 {
        return true;
    }

    false
}

/// Check if a byte position is within an MkDocs icon shortcode
/// Icon shortcodes use format like `:material-check:`, `:octicons-mark-github-16:`
pub fn is_in_icon_shortcode(line: &str, position: usize, _flavor: MarkdownFlavor) -> bool {
    // Only skip for MkDocs flavor, but check pattern for all flavors
    // since emoji shortcodes are universal
    mkdocs_icons::is_in_any_shortcode(line, position)
}

/// Check if a byte position is within PyMdown extension markup
/// Includes: Keys (++ctrl+alt++), Caret (^text^), Insert (^^text^^), Mark (==text==)
///
/// For MkDocs flavor: supports all PyMdown extensions
/// For Obsidian flavor: only supports Mark (==highlight==) syntax
pub fn is_in_pymdown_markup(line: &str, position: usize, flavor: MarkdownFlavor) -> bool {
    match flavor {
        MarkdownFlavor::MkDocs => mkdocs_extensions::is_in_pymdown_markup(line, position),
        MarkdownFlavor::Obsidian => {
            // Obsidian supports ==highlight== syntax (same as PyMdown Mark)
            mkdocs_extensions::is_in_mark(line, position)
        }
        _ => false,
    }
}

/// Check whether a position on a line falls inside an inline HTML code-like element.
///
/// Handles `<code>`, `<pre>`, `<samp>`, `<kbd>`, and `<var>` tags (case-insensitive).
/// These are inline elements whose content should not be interpreted as markdown emphasis.
pub fn is_in_inline_html_code(line: &str, position: usize) -> bool {
    // Tags whose content should not be parsed as markdown
    const TAGS: &[&str] = &["code", "pre", "samp", "kbd", "var"];

    let bytes = line.as_bytes();

    for tag in TAGS {
        let open_bytes = format!("<{tag}").into_bytes();
        let close_pattern = format!("</{tag}>").into_bytes();

        let mut search_from = 0;
        while search_from + open_bytes.len() <= bytes.len() {
            // Find opening tag (case-insensitive byte search)
            let Some(open_abs) = find_case_insensitive(bytes, &open_bytes, search_from) else {
                break;
            };

            let after_tag = open_abs + open_bytes.len();

            // Verify the character after the tag name is '>' or whitespace (not a longer tag name)
            if after_tag < bytes.len() {
                let next = bytes[after_tag];
                if next != b'>' && next != b' ' && next != b'\t' {
                    search_from = after_tag;
                    continue;
                }
            }

            // Find the end of the opening tag
            let Some(tag_close) = bytes[after_tag..].iter().position(|&b| b == b'>') else {
                break;
            };
            let content_start = after_tag + tag_close + 1;

            // Find the closing tag (case-insensitive)
            let Some(close_start) = find_case_insensitive(bytes, &close_pattern, content_start) else {
                break;
            };
            let content_end = close_start;

            if position >= content_start && position < content_end {
                return true;
            }

            search_from = close_start + close_pattern.len();
        }
    }
    false
}

/// Case-insensitive byte search within a slice, starting at `from`.
fn find_case_insensitive(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from + needle.len() > haystack.len() {
        return None;
    }
    for i in from..=haystack.len() - needle.len() {
        if haystack[i..i + needle.len()]
            .iter()
            .zip(needle.iter())
            .all(|(h, n)| h.eq_ignore_ascii_case(n))
        {
            return Some(i);
        }
    }
    None
}

/// Check if a byte position is within flavor-specific markup
/// For MkDocs: icon shortcodes and PyMdown extensions
/// For Obsidian: highlight syntax (==text==)
pub fn is_in_mkdocs_markup(line: &str, position: usize, flavor: MarkdownFlavor) -> bool {
    if is_in_icon_shortcode(line, position, flavor) {
        return true;
    }
    if is_in_pymdown_markup(line, position, flavor) {
        return true;
    }
    false
}

/// Check if a byte position within a line is inside a backtick-delimited code span.
///
/// This is a line-level fallback for cases where pulldown-cmark's code span detection
/// misses spans due to table parsing interference (e.g., pipes inside code spans
/// in table rows cause pulldown-cmark to misidentify cell boundaries).
fn is_in_inline_code_on_line(line: &str, byte_pos: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            let open_start = i;
            let mut backtick_count = 0;
            while i < bytes.len() && bytes[i] == b'`' {
                backtick_count += 1;
                i += 1;
            }

            // Search for matching closing backticks
            let mut j = i;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let mut close_count = 0;
                    while j < bytes.len() && bytes[j] == b'`' {
                        close_count += 1;
                        j += 1;
                    }
                    if close_count == backtick_count {
                        // Found matching pair: code span covers open_start..j
                        if byte_pos >= open_start && byte_pos < j {
                            return true;
                        }
                        i = j;
                        break;
                    }
                } else {
                    j += 1;
                }
            }

            if j >= bytes.len() {
                // No matching close found, remaining text is not a code span
                break;
            }
        } else {
            i += 1;
        }
    }

    false
}

/// Check if a byte position is within an HTML tag. O(log n) via binary search.
fn is_byte_in_html_tag(html_tags: &[HtmlTag], byte_pos: usize) -> bool {
    let idx = html_tags.partition_point(|tag| tag.byte_offset <= byte_pos);
    idx > 0 && byte_pos < html_tags[idx - 1].byte_end
}

/// Check if a byte position is within HTML code content (`<code>...</code>`).
/// Uses pre-computed code ranges for O(log n) lookup via binary search.
fn is_byte_in_html_code_content(code_ranges: &[(usize, usize)], byte_pos: usize) -> bool {
    let idx = code_ranges.partition_point(|&(start, _)| start <= byte_pos);
    idx > 0 && byte_pos < code_ranges[idx - 1].1
}

/// Pre-compute ranges covered by `<code>...</code>` HTML tags.
/// Returns sorted Vec of (start, end) byte ranges.
pub(crate) fn compute_html_code_ranges(html_tags: &[HtmlTag]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open_code_end: Option<usize> = None;

    for tag in html_tags {
        if tag.tag_name == "code" {
            if tag.is_self_closing {
                continue;
            } else if !tag.is_closing {
                open_code_end = Some(tag.byte_end);
            } else if tag.is_closing {
                if let Some(start) = open_code_end {
                    ranges.push((start, tag.byte_offset));
                }
                open_code_end = None;
            }
        }
    }
    // Handle unclosed <code> tag
    if let Some(start) = open_code_end {
        ranges.push((start, usize::MAX));
    }
    ranges
}

/// Determine whether an emphasis or strong span starting at `span_start` should be
/// skipped because it falls inside a non-prose context: code blocks/spans, inline
/// code, links, HTML tags or `<code>` content, MkDocs/PyMdown markup, math, JSX
/// expressions, MDX comments, front matter, or mkdocstrings blocks.
///
/// `html_tags` and `html_code_ranges` are passed in so callers iterating many spans
/// can compute them once via [`compute_html_code_ranges`].
pub(crate) fn should_skip_emphasis_span(
    ctx: &LintContext,
    html_tags: &[HtmlTag],
    html_code_ranges: &[(usize, usize)],
    span_start: usize,
) -> bool {
    let lines = ctx.raw_lines();
    let (line_num, col) = ctx.offset_to_line_col(span_start);

    // Skip matches in front matter or mkdocstrings blocks
    if ctx
        .line_info(line_num)
        .is_some_and(|info| info.in_front_matter || info.in_mkdocstrings)
    {
        return true;
    }

    // Check MkDocs markup
    let in_mkdocs_markup = lines
        .get(line_num.saturating_sub(1))
        .is_some_and(|line| is_in_mkdocs_markup(line, col.saturating_sub(1), ctx.flavor));

    // Line-level inline code fallback for cases pulldown-cmark misses
    let in_inline_code = lines
        .get(line_num.saturating_sub(1))
        .is_some_and(|line| is_in_inline_code_on_line(line, col.saturating_sub(1)));

    ctx.is_in_code_block_or_span(span_start)
        || in_inline_code
        || ctx.is_in_link(span_start)
        || is_byte_in_html_tag(html_tags, span_start)
        || is_byte_in_html_code_content(html_code_ranges, span_start)
        || in_mkdocs_markup
        || is_in_math_context(ctx, span_start)
        || is_in_jsx_expression(ctx, span_start)
        || is_in_mdx_comment(ctx, span_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_comment_detection() {
        let content = "Text <!-- comment --> more text";
        let ranges = compute_html_comment_ranges(content);
        assert!(is_in_html_comment_ranges(&ranges, 10)); // Inside comment
        assert!(!is_in_html_comment_ranges(&ranges, 0)); // Before comment
        assert!(!is_in_html_comment_ranges(&ranges, 25)); // After comment
    }

    #[test]
    fn test_compute_html_comment_ranges_ignores_code_span_delimiters() {
        // `<!--` and `-->` inside inline code spans on different lines must not
        // pair into a multi-line HTML comment (issue #679).
        let content = "a `<!--` b\n\nc `-->` d";
        let open = content.find("<!--").unwrap();
        let close = content.find("-->").unwrap();
        // Code spans covering the two backtick-delimited tokens.
        let code_spans = [
            (content.find('`').unwrap(), open + "<!--".len() + 1),
            (content.rfind("` d").unwrap() - "-->".len(), close + "-->".len() + 1),
        ];

        // Without code-span awareness the pattern spans open..close (the bug).
        assert!(
            !compute_html_comment_ranges(content).is_empty(),
            "sanity: raw pattern matches across the code spans"
        );
        // With code-span awareness the spurious match is dropped.
        assert!(
            compute_html_comment_ranges_filtered(content, &code_spans, &[]).is_empty(),
            "a `<!--`/`-->` pair inside code spans must not be treated as a comment"
        );
    }

    #[test]
    fn test_compute_html_comment_ranges_ignores_code_block_delimiters() {
        // A `<!--` inside a code block must not pair with a later `-->` outside it
        // (the code-block counterpart of the code-span case).
        let content = "```\n<!-- literal\n```\n\nhttps://example.com\n\n-->\n";
        let block_end = content.find("```\n\n").unwrap() + "```".len();
        let code_blocks = [(0usize, block_end)];
        assert!(
            compute_html_comment_ranges_filtered(content, &[], &code_blocks).is_empty(),
            "a `<!--` inside a code block must not open a comment that spans to a later `-->`"
        );
        // A real comment whose opener is outside the block is still detected.
        let real = "```\n<!-- literal\n```\n\n<!-- real --> tail";
        let real_block_end = real.find("```\n\n").unwrap() + "```".len();
        let ranges = compute_html_comment_ranges_filtered(real, &[], &[(0usize, real_block_end)]);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, real.find("<!-- real").unwrap());
    }

    #[test]
    fn test_compute_html_comment_ranges_keeps_real_comments() {
        // A genuine comment whose `<!--` is not inside a code span is still
        // detected, even when an unrelated code span exists elsewhere.
        let content = "text `code` <!-- real comment --> more";
        let code_spans = [(content.find('`').unwrap(), content.find("` ").unwrap() + 1)];
        let ranges = compute_html_comment_ranges_filtered(content, &code_spans, &[]);
        assert_eq!(ranges.len(), 1, "the real comment must still be detected");
        let comment_start = content.find("<!--").unwrap();
        assert_eq!(ranges[0].start, comment_start);
    }

    #[test]
    fn test_compute_html_comment_ranges_real_comment_after_code_span_opener() {
        // A code span containing `<!--` must not consume a real comment that
        // follows it: skipping the literal opener, the scan must still discover
        // the genuine `<!-- ... -->` and mark its content as a comment.
        let content = "a `<!--` then <!-- real --> end";
        let code_spans = [(content.find('`').unwrap(), content.find("` then").unwrap() + 1)];
        let ranges = compute_html_comment_ranges_filtered(content, &code_spans, &[]);
        assert_eq!(
            ranges.len(),
            1,
            "the real comment after a code-span opener must be detected"
        );
        let real_open = content.find("<!-- real").unwrap();
        assert_eq!(
            ranges[0].start, real_open,
            "range must start at the real comment, not the code-span opener"
        );
        assert_eq!(ranges[0].end, content.find("--> end").unwrap() + "-->".len());
    }

    #[test]
    fn test_compute_html_comment_ranges_closer_inside_code_span_is_not_a_closer() {
        // A real comment's closing `-->` that lands inside a code span is literal;
        // the scan must continue to the next real `-->`.
        let content = "<!-- open `-->` still open --> done";
        let first_close = content.find("`-->`").unwrap() + 1;
        let code_spans = [(content.find('`').unwrap(), content.find("` still").unwrap() + 1)];
        let ranges = compute_html_comment_ranges_filtered(content, &code_spans, &[]);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        let real_close_end = content.find("--> done").unwrap() + "-->".len();
        assert_eq!(
            ranges[0].end, real_close_end,
            "must close at the real --> ({real_close_end}), not the one in the code span ({first_close})"
        );
    }

    #[test]
    fn test_is_line_entirely_in_html_comment() {
        // Test 1: Multi-line comment with content after closing
        let content = "<!--\ncomment\n--> Content after comment";
        let ranges = compute_html_comment_ranges(content);
        // Line 0: "<!--" (bytes 0-4) - entirely in comment
        assert!(is_line_entirely_in_html_comment(&ranges, 0, 4));
        // Line 1: "comment" (bytes 5-12) - entirely in comment
        assert!(is_line_entirely_in_html_comment(&ranges, 5, 12));
        // Line 2: "--> Content after comment" (bytes 13-38) - NOT entirely in comment
        assert!(!is_line_entirely_in_html_comment(&ranges, 13, 38));

        // Test 2: Single-line comment with content after
        let content2 = "<!-- comment --> Not a comment";
        let ranges2 = compute_html_comment_ranges(content2);
        // The entire line is NOT entirely in the comment
        assert!(!is_line_entirely_in_html_comment(&ranges2, 0, 30));

        // Test 3: Single-line comment alone
        let content3 = "<!-- comment -->";
        let ranges3 = compute_html_comment_ranges(content3);
        // The entire line IS entirely in the comment
        assert!(is_line_entirely_in_html_comment(&ranges3, 0, 16));

        // Test 4: Content before comment
        let content4 = "Text before <!-- comment -->";
        let ranges4 = compute_html_comment_ranges(content4);
        // Line start is NOT in the comment range
        assert!(!is_line_entirely_in_html_comment(&ranges4, 0, 28));
    }

    #[test]
    fn test_is_line_entirely_in_html_comment_indented() {
        // An indented single-line comment: callers pass the trimmed content bounds
        // (start at the `<!--`, end after the `-->`), so it is recognised as being
        // entirely inside the comment even though the line starts with whitespace.
        let content = "    <!-- comment -->";
        let ranges = compute_html_comment_ranges(content);
        let content_start = content.find("<!--").unwrap();
        let content_end = content.trim_end().len();
        assert!(is_line_entirely_in_html_comment(&ranges, content_start, content_end));
        // Passing the raw column-0 line start would miss it (regression guard for #755).
        assert!(!is_line_entirely_in_html_comment(&ranges, 0, content.len()));
    }

    #[test]
    fn test_is_line_entirely_in_html_comment_trailing_whitespace() {
        // Trailing whitespace after the closer must not push content_end past the range.
        let content = "<!-- comment -->   ";
        let ranges = compute_html_comment_ranges(content);
        let content_end = content.trim_end().len();
        assert!(is_line_entirely_in_html_comment(&ranges, 0, content_end));
        // With the raw line length (incl. trailing spaces) it would fall outside the range.
        assert!(!is_line_entirely_in_html_comment(&ranges, 0, content.len()));
    }

    #[test]
    fn test_math_block_detection() {
        let content = "Text\n$$\nmath content\n$$\nmore text";
        assert!(is_in_math_block(content, 8)); // On opening $$
        assert!(is_in_math_block(content, 15)); // Inside math block
        assert!(!is_in_math_block(content, 0)); // Before math block
        assert!(!is_in_math_block(content, 30)); // After math block
    }

    #[test]
    fn test_stray_double_dollar_in_prose_is_not_math() {
        // Two `$$` tokens inside a prose line must NOT pair into a math block:
        // a multi-line block only opens on a `$$` that begins its line. This
        // keeps the byte-level result consistent with the line-level map.
        let content = "Note: $$ is used for display math and $$ closes it";
        let between = content.find("is used").unwrap();
        assert!(
            !is_in_math_block(content, between),
            "stray paired `$$` in prose must not be treated as a math block"
        );
        assert!(math_block_ranges(content).is_empty());
    }

    #[test]
    fn test_blockquoted_double_dollar_opens_block() {
        // A `$$` opener is still recognized behind a blockquote prefix.
        let content = "> $$\n> x = y\n> $$\n";
        let inside = content.find("x = y").unwrap();
        assert!(is_in_math_block(content, inside), "blockquoted math interior");
    }

    #[test]
    fn test_self_contained_single_line_block_leaves_trailing_prose() {
        // `$$ a $$` at line start is math; prose after the closing `$$` is not.
        let content = "$$ a $$ and __not math__\n";
        let in_math = content.find('a').unwrap();
        assert!(is_in_math_block(content, in_math), "single-line math interior");
        let after = content.find("not math").unwrap();
        assert!(!is_in_math_block(content, after), "trailing prose is lintable");
    }

    #[test]
    fn test_math_block_closes_with_content_before_fence() {
        // A display-math block whose closing `$$` shares its line with
        // content (e.g. `\end{aligned}$$`) must still close the block.
        // Content after the block is prose, not math.
        let content = "$$\nx = y\n\\end{x}$$\nafter __text__ here";

        let inside = content.find("x = y").unwrap();
        assert!(is_in_math_block(content, inside), "interior must be math");

        let after = content.find("after").unwrap();
        assert!(
            !is_in_math_block(content, after),
            "content after a content-sharing closing fence must NOT be math"
        );
    }

    #[test]
    fn test_inline_math_detection() {
        let content = "Text $x + y$ and $$a^2 + b^2$$ here";
        assert!(is_in_inline_math(content, 7), "inside the single-`$` inline span");
        // The mid-line `$$a^2 + b^2$$` is display syntax, not a line-start
        // block, so it is a literal under the shared math model - neither the
        // inline path nor `math_block_ranges` treats it as math.
        assert!(!is_in_inline_math(content, 20), "mid-line $$...$$ is not inline math");
        assert!(
            !is_in_math_block(content, 20),
            "mid-line $$...$$ is not a line-start display block"
        );
        assert!(!is_in_inline_math(content, 0), "before any math");
        assert!(!is_in_inline_math(content, 35), "after the spans");
    }

    #[test]
    fn test_table_line_detection() {
        assert!(is_table_line("| Header | Column |"));
        assert!(is_table_line("|--------|--------|"));
        assert!(is_table_line("| Cell 1 | Cell 2 |"));
        assert!(!is_table_line("Regular text"));
        assert!(!is_table_line("Just a pipe | here"));
    }

    #[test]
    fn test_is_in_icon_shortcode() {
        let line = "Click :material-check: to confirm";
        // Position 0-5 is "Click"
        assert!(!is_in_icon_shortcode(line, 0, MarkdownFlavor::MkDocs));
        // Position 6-22 is ":material-check:"
        assert!(is_in_icon_shortcode(line, 6, MarkdownFlavor::MkDocs));
        assert!(is_in_icon_shortcode(line, 15, MarkdownFlavor::MkDocs));
        assert!(is_in_icon_shortcode(line, 21, MarkdownFlavor::MkDocs));
        // Position 22+ is " to confirm"
        assert!(!is_in_icon_shortcode(line, 22, MarkdownFlavor::MkDocs));
    }

    #[test]
    fn test_is_in_pymdown_markup() {
        // Test Keys notation
        let line = "Press ++ctrl+c++ to copy";
        assert!(!is_in_pymdown_markup(line, 0, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line, 6, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line, 10, MarkdownFlavor::MkDocs));
        assert!(!is_in_pymdown_markup(line, 17, MarkdownFlavor::MkDocs));

        // Test Mark notation
        let line2 = "This is ==highlighted== text";
        assert!(!is_in_pymdown_markup(line2, 0, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line2, 8, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line2, 15, MarkdownFlavor::MkDocs));
        assert!(!is_in_pymdown_markup(line2, 23, MarkdownFlavor::MkDocs));

        // Should not match for Standard flavor
        assert!(!is_in_pymdown_markup(line, 10, MarkdownFlavor::Standard));
    }

    #[test]
    fn test_is_in_mkdocs_markup() {
        // Should combine both icon and pymdown
        let line = ":material-check: and ++ctrl++";
        assert!(is_in_mkdocs_markup(line, 5, MarkdownFlavor::MkDocs)); // In icon
        assert!(is_in_mkdocs_markup(line, 23, MarkdownFlavor::MkDocs)); // In keys
        assert!(!is_in_mkdocs_markup(line, 17, MarkdownFlavor::MkDocs)); // In " and "
    }

    // ==================== Obsidian highlight tests ====================

    #[test]
    fn test_obsidian_highlight_basic() {
        // Obsidian flavor should recognize ==highlight== syntax
        let line = "This is ==highlighted== text";
        assert!(!is_in_pymdown_markup(line, 0, MarkdownFlavor::Obsidian)); // "T"
        assert!(is_in_pymdown_markup(line, 8, MarkdownFlavor::Obsidian)); // First "="
        assert!(is_in_pymdown_markup(line, 10, MarkdownFlavor::Obsidian)); // "h"
        assert!(is_in_pymdown_markup(line, 15, MarkdownFlavor::Obsidian)); // "g"
        assert!(is_in_pymdown_markup(line, 22, MarkdownFlavor::Obsidian)); // Last "="
        assert!(!is_in_pymdown_markup(line, 23, MarkdownFlavor::Obsidian)); // " "
    }

    #[test]
    fn test_obsidian_highlight_multiple() {
        // Multiple highlights on one line
        let line = "Both ==one== and ==two== here";
        assert!(is_in_pymdown_markup(line, 5, MarkdownFlavor::Obsidian)); // In first
        assert!(is_in_pymdown_markup(line, 8, MarkdownFlavor::Obsidian)); // "o"
        assert!(!is_in_pymdown_markup(line, 12, MarkdownFlavor::Obsidian)); // Space after
        assert!(is_in_pymdown_markup(line, 17, MarkdownFlavor::Obsidian)); // In second
    }

    #[test]
    fn test_obsidian_highlight_not_standard_flavor() {
        // Standard flavor should NOT recognize ==highlight== as special
        let line = "This is ==highlighted== text";
        assert!(!is_in_pymdown_markup(line, 8, MarkdownFlavor::Standard));
        assert!(!is_in_pymdown_markup(line, 15, MarkdownFlavor::Standard));
    }

    #[test]
    fn test_obsidian_highlight_with_spaces_inside() {
        // Highlights can have spaces inside the content
        let line = "This is ==text with spaces== here";
        assert!(is_in_pymdown_markup(line, 10, MarkdownFlavor::Obsidian)); // "t"
        assert!(is_in_pymdown_markup(line, 15, MarkdownFlavor::Obsidian)); // "w"
        assert!(is_in_pymdown_markup(line, 27, MarkdownFlavor::Obsidian)); // "="
    }

    #[test]
    fn test_obsidian_does_not_support_keys_notation() {
        // Obsidian flavor should NOT recognize ++keys++ syntax (that's MkDocs-specific)
        let line = "Press ++ctrl+c++ to copy";
        assert!(!is_in_pymdown_markup(line, 6, MarkdownFlavor::Obsidian));
        assert!(!is_in_pymdown_markup(line, 10, MarkdownFlavor::Obsidian));
    }

    #[test]
    fn test_obsidian_mkdocs_markup_function() {
        // is_in_mkdocs_markup should also work for Obsidian highlights
        let line = "This is ==highlighted== text";
        assert!(is_in_mkdocs_markup(line, 10, MarkdownFlavor::Obsidian)); // In highlight
        assert!(!is_in_mkdocs_markup(line, 0, MarkdownFlavor::Obsidian)); // Not in highlight
    }

    #[test]
    fn test_obsidian_highlight_edge_cases() {
        // Empty highlight (====) should not match
        let line = "Test ==== here";
        assert!(!is_in_pymdown_markup(line, 5, MarkdownFlavor::Obsidian)); // Position at first =
        assert!(!is_in_pymdown_markup(line, 6, MarkdownFlavor::Obsidian));

        // Single character highlight
        let line2 = "Test ==a== here";
        assert!(is_in_pymdown_markup(line2, 5, MarkdownFlavor::Obsidian));
        assert!(is_in_pymdown_markup(line2, 7, MarkdownFlavor::Obsidian)); // "a"
        assert!(is_in_pymdown_markup(line2, 9, MarkdownFlavor::Obsidian)); // last =

        // Triple equals (===) should not create highlight
        let line3 = "a === b";
        assert!(!is_in_pymdown_markup(line3, 3, MarkdownFlavor::Obsidian));
    }

    #[test]
    fn test_obsidian_highlight_unclosed() {
        // Unclosed highlight should not match
        let line = "This ==starts but never ends";
        assert!(!is_in_pymdown_markup(line, 5, MarkdownFlavor::Obsidian));
        assert!(!is_in_pymdown_markup(line, 10, MarkdownFlavor::Obsidian));
    }

    #[test]
    fn test_inline_html_code_basic() {
        let line = "The formula is <code>a * b * c</code> in math.";
        // Position inside <code> content
        assert!(is_in_inline_html_code(line, 21)); // 'a'
        assert!(is_in_inline_html_code(line, 25)); // '*'
        // Position outside <code> content
        assert!(!is_in_inline_html_code(line, 0)); // 'T'
        assert!(!is_in_inline_html_code(line, 40)); // after </code>
    }

    #[test]
    fn test_inline_html_code_multiple_tags() {
        let line = "<kbd>Ctrl</kbd> + <samp>output</samp>";
        assert!(is_in_inline_html_code(line, 5)); // 'C' in Ctrl
        assert!(is_in_inline_html_code(line, 24)); // 'o' in output
        assert!(!is_in_inline_html_code(line, 16)); // '+'
    }

    #[test]
    fn test_inline_html_code_with_attributes() {
        let line = r#"<code class="lang">x * y</code>"#;
        assert!(is_in_inline_html_code(line, 19)); // 'x'
        assert!(is_in_inline_html_code(line, 23)); // '*'
        assert!(!is_in_inline_html_code(line, 0)); // before tag
    }

    #[test]
    fn test_inline_html_code_case_insensitive() {
        let line = "<CODE>a * b</CODE>";
        assert!(is_in_inline_html_code(line, 6)); // 'a'
        assert!(is_in_inline_html_code(line, 8)); // '*'
    }

    #[test]
    fn test_inline_html_code_var_and_pre() {
        let line = "<var>x * y</var> and <pre>a * b</pre>";
        assert!(is_in_inline_html_code(line, 5)); // 'x' in var
        assert!(is_in_inline_html_code(line, 26)); // 'a' in pre
        assert!(!is_in_inline_html_code(line, 17)); // 'and'
    }

    #[test]
    fn test_inline_html_code_unclosed() {
        // Unclosed tag should not match
        let line = "<code>a * b without closing";
        assert!(!is_in_inline_html_code(line, 6));
    }

    #[test]
    fn test_inline_html_code_no_substring_match() {
        // <variable> should NOT be treated as <var>
        let line = "<variable>a * b</variable>";
        assert!(!is_in_inline_html_code(line, 11));

        // <keyboard> should NOT be treated as <kbd>
        let line2 = "<keyboard>x * y</keyboard>";
        assert!(!is_in_inline_html_code(line2, 11));
    }
}
