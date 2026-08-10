//! Pandoc Markdown syntax detection.
//!
//! This module provides detection for Pandoc Markdown constructs that affect
//! rumdl rule output: fenced divs (`:::`), attribute lists (`{#id .class}`),
//! citations (`[@key]`), bracketed spans (`[text]{.class}`), and other
//! Pandoc-specific syntax.
//!
//! Pandoc is the foundation; the Quarto flavor extends it with Quarto-only
//! syntax (executable code blocks, shortcodes, cell options) elsewhere in
//! the codebase. Anything that's pure Pandoc lives here.
//!
//! Common patterns this module handles:
//! - `::: {.callout-note}` — fenced div with class
//! - `::: {#myid .class}` — generic div with id and class
//! - `:::` — closing marker
//! - `{#id .class key="value"}` — Pandoc attribute lists
//! - `@key`, `[@key]`, `[-@key]`, `[@a; @b]` — citations

use regex::Regex;
use std::sync::LazyLock;

use crate::utils::skip_context::ByteRange;

/// Pattern to match div opening markers
/// Matches: ::: {.class}, ::: {#id .class}, ::: classname, etc.
/// Does NOT match a closing ::: on its own
static DIV_OPEN_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*):::\s*(?:\{[^}]+\}|\S+)").unwrap());

/// Pattern to match div closing markers
/// Matches: ::: (with optional whitespace before and after)
static DIV_CLOSE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*):::\s*$").unwrap());

/// Pattern to match callout blocks specifically
/// Callout types: note, warning, tip, important, caution
static CALLOUT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*):::\s*\{[^}]*\.callout-(?:note|warning|tip|important|caution)[^}]*\}").unwrap()
});

/// Pattern to match Pandoc-style attributes on any element
/// Matches: {#id}, {.class}, {#id .class key="value"}, etc.
/// Note: We match the entire attribute block including contents
static PANDOC_ATTR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[^}]+\}").unwrap());

/// Check if a line is a div opening marker
pub fn is_div_open(line: &str) -> bool {
    DIV_OPEN_PATTERN.is_match(line)
}

/// Check if a line is a div closing marker (just `:::`)
pub fn is_div_close(line: &str) -> bool {
    DIV_CLOSE_PATTERN.is_match(line)
}

/// Check if a line is a callout block opening
pub fn is_callout_open(line: &str) -> bool {
    CALLOUT_PATTERN.is_match(line)
}

/// Check if a line contains Pandoc-style attributes
pub fn has_pandoc_attributes(line: &str) -> bool {
    PANDOC_ATTR_PATTERN.is_match(line)
}

/// Return true if `lang` is a Pandoc raw-format declaration: `{=html}`,
/// `{=latex}`, etc. The format name must be non-empty and consist only of
/// ASCII alphanumeric characters, underscores, or hyphens.
pub fn is_pandoc_raw_block_lang(lang: &str) -> bool {
    let l = lang.trim();
    l.starts_with("{=") && l.ends_with('}') && {
        let inner = &l[2..l.len() - 1];
        !inner.trim().is_empty() && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }
}

/// Return the language a Pandoc code-attribute list declares, if any: the first
/// `.class` inside a brace-delimited attribute block, without its leading dot.
/// `{.python}` yields `python`, `{#snippet .haskell startFrom="10"}` yields
/// `haskell`.
///
/// Pandoc treats the first `.class` inside the attribute block as the language
/// for syntax highlighting. Tokens are space-separated; a `.class` token is one
/// that starts with `.` followed by a non-empty identifier.
pub fn pandoc_code_class_lang(lang: &str) -> Option<&str> {
    let l = lang.trim();
    if !l.starts_with('{') || !l.ends_with('}') || l.len() < 2 {
        return None;
    }
    let inner = &l[1..l.len() - 1];
    inner
        .split_whitespace()
        .filter_map(|tok| tok.strip_prefix('.'))
        .find(|class| !class.is_empty() && class.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

/// Return true if `lang` is a Pandoc code-attribute language declaration: a
/// brace-delimited attribute list containing at least one `.class`, e.g.
/// `{.python}`, `{.haskell .numberLines}`, `{#snippet .python startFrom="10"}`.
pub fn is_pandoc_code_class_attr(lang: &str) -> bool {
    pandoc_code_class_lang(lang).is_some()
}

/// Get the indentation level of a div marker
pub fn get_div_indent(line: &str) -> usize {
    let mut indent = 0;
    for c in line.chars() {
        match c {
            ' ' => indent += 1,
            '\t' => indent += 4, // Tabs expand to 4 spaces (CommonMark)
            _ => break,
        }
    }
    indent
}

/// Track div nesting state for a document
#[derive(Debug, Clone, Default)]
pub struct DivTracker {
    /// Stack of div indentation levels for nesting tracking
    indent_stack: Vec<usize>,
}

impl DivTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a line and return whether we're inside a div after processing
    pub fn process_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();

        if trimmed.starts_with(":::") {
            let indent = get_div_indent(line);

            if is_div_close(line) {
                // Closing marker - pop the matching div from stack
                // Pop the top div if its indent is >= the closing marker's indent
                if let Some(&top_indent) = self.indent_stack.last()
                    && top_indent >= indent
                {
                    self.indent_stack.pop();
                }
            } else if is_div_open(line) {
                // Opening marker - push to stack
                self.indent_stack.push(indent);
            }
        }

        !self.indent_stack.is_empty()
    }

    /// Check if we're currently inside a div
    pub fn is_inside_div(&self) -> bool {
        !self.indent_stack.is_empty()
    }
}

/// Detect fenced div block ranges in content.
/// Returns a vector of byte ranges (start, end) for each div block.
pub fn detect_div_block_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut tracker = DivTracker::new();
    let mut div_start: Option<usize> = None;
    let mut byte_offset = 0;

    for line in content.lines() {
        let line_len = line.len();
        let was_inside = tracker.is_inside_div();
        let is_inside = tracker.process_line(line);

        // Started a new div block
        if !was_inside && is_inside {
            div_start = Some(byte_offset);
        }
        // Exited a div block
        else if was_inside
            && !is_inside
            && let Some(start) = div_start.take()
        {
            // End at the start of the closing line
            ranges.push(ByteRange {
                start,
                end: byte_offset + line_len,
            });
        }

        // Account for newline
        byte_offset += line_len + 1;
    }

    // Handle unclosed divs at end of document
    if let Some(start) = div_start {
        ranges.push(ByteRange {
            start,
            end: content.len(),
        });
    }

    ranges
}

/// Check if a byte position is within a div block
pub fn is_within_div_block_ranges(ranges: &[ByteRange], position: usize) -> bool {
    ranges.iter().any(|r| position >= r.start && position < r.end)
}

// ============================================================================
// Citation Support
// ============================================================================
//
// Pandoc citation syntax:
// - Inline citation: @smith2020
// - Parenthetical citation: [@smith2020]
// - Suppress author: [-@smith2020]
// - With locator: [@smith2020, p. 10]
// - Multiple citations: [@smith2020; @jones2021]
// - With prefix: [see @smith2020]
//
// Citation keys must start with a letter, digit, or underscore, and may contain
// alphanumerics, underscores, hyphens, periods, and colons.

/// Pattern to match bracketed citations: [@key], [-@key], [see @key], [@a; @b]
///
/// The `@` must sit at a citation boundary: immediately after `[`, or after a
/// non-word character such as whitespace, `-`, `;`, or `,`. This excludes
/// word-embedded `@` (e.g. emails or handles in link text like
/// `[contact user@example.com](url)`), which are not citations.
static BRACKETED_CITATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(?:[^\]@]*[^A-Za-z0-9_])?@[a-zA-Z0-9_][a-zA-Z0-9_:.#$%&\-+?<>~/]*[^\]]*\]").unwrap()
});

/// Pattern to match inline citations: @key (not inside brackets)
/// Citation key: starts with letter/digit/underscore, contains alphanumerics and some punctuation
/// The @ must be preceded by whitespace, start of line, or punctuation (not alphanumeric)
static INLINE_CITATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Match @ at start of string, after whitespace, or after non-alphanumeric (except @[)
    Regex::new(r"(?:^|[\s\(\[\{,;:])(@[a-zA-Z0-9_][a-zA-Z0-9_:.#$%&\-+?<>~/]*)").unwrap()
});

/// Pattern to match the bracketed text portion of a Markdown link.
///
/// Matches `[...]` that is *immediately* followed by `(` (inline link) or
/// `[` (reference link). Capture group 1 is the bracket span, including the
/// surrounding `[` and `]`. Used by citation detection to exclude `@key`
/// occurrences appearing inside link labels.
static LINK_LABEL_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\[[^\]]*\])(?:\(|\[)").unwrap());

/// Quick check if text might contain citations
#[inline]
pub fn has_citations(text: &str) -> bool {
    text.contains('@')
}

// ============================================================================
// Inline Footnote Support
// ============================================================================
//
// Pandoc inline footnote syntax: ^[footnote text]
//
// The `^` must not be preceded by `!` (image) or by a word character
// (superscript syntax: `2^10^`). The footnote body extends to the first
// unescaped `]`; nested brackets are not supported in this detector.

/// Pattern for Pandoc inline footnotes: `^[note text]`.
/// The `^` must not be preceded by `!` (which would be an image) or by
/// alphanumeric (which would be a superscript: `2^10^`).
static INLINE_FOOTNOTE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[^\w!])(\^\[[^\]]*\])").unwrap());

/// Compute the Pandoc-style slug for a heading text.
///
/// Pandoc's `auto_identifiers` extension:
/// 1. Remove all formatting, links, etc.
/// 2. Remove all footnotes.
/// 3. Remove all non-alphanumeric characters except `_`, `-`, `.`.
/// 4. Replace all spaces with `-`.
/// 5. Lowercase letters.
/// 6. If nothing remains, use `section`.
pub fn pandoc_header_slug(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
            for lc in c.to_lowercase() {
                s.push(lc);
            }
        } else if c.is_whitespace() {
            // Collapse runs of whitespace to a single `-`.
            if !s.ends_with('-') {
                s.push('-');
            }
        }
        // Drop other punctuation entirely.
    }
    let trimmed = s.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed
    }
}

/// Find headings in the document and return a set of their Pandoc slugs.
///
/// Scans ATX-style headings (lines beginning with one or more `#`) and computes
/// a slug for each using [`pandoc_header_slug`]. The resulting set is used by
/// the `implicit_header_references` extension detector in [`LintContext`].
///
/// Pandoc's `auto_identifiers` extension disambiguates duplicate headings by
/// appending `-1`, `-2`, etc. to the second, third, … occurrence of the same
/// base slug. Both the base slug and its suffixed forms are inserted so that
/// links such as `#section` and `#section-1` both resolve.
///
/// Lines inside fenced code blocks (delimited by ` ``` ` or `~~~`, >= 3 chars)
/// are skipped so that bash comments and shebang lines are not mistaken for
/// headings.
pub fn collect_pandoc_header_slugs(content: &str) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet};
    let mut slugs = HashSet::new();
    let mut base_counts: HashMap<String, usize> = HashMap::new();
    let mut in_fence = false;
    let mut fence_marker: Option<char> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        // Detect fenced code block open/close. Pandoc fences are >= 3 backticks
        // or >= 3 tildes at the start of a line (after optional indentation).
        // A closing fence must use the same marker character as the opening one.
        if let Some(c) = trimmed.chars().next()
            && (c == '`' || c == '~')
        {
            let count = trimmed.chars().take_while(|&ch| ch == c).count();
            if count >= 3 {
                match fence_marker {
                    None => {
                        in_fence = true;
                        fence_marker = Some(c);
                    }
                    Some(m) if m == c => {
                        in_fence = false;
                        fence_marker = None;
                    }
                    _ => {}
                }
                continue;
            }
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let mut text = rest.trim_start_matches('#').trim();
            // Strip trailing `{#id .class}` attribute block only when the `{...}`
            // extends to the end of the text (possibly followed by whitespace).
            // This prevents `{` appearing inside heading body text (e.g.
            // `# Some {curly} word`) from being mistaken for an attribute block.
            if let Some(idx) = text.rfind(" {")
                && let Some(close_rel) = text[idx + 2..].find('}')
                && text[idx + 2 + close_rel + 1..].trim().is_empty()
            {
                text = &text[..idx];
            }
            let base = pandoc_header_slug(text);
            let count = base_counts.entry(base.clone()).or_insert(0);
            let slug = if *count == 0 {
                base.clone()
            } else {
                format!("{base}-{count}")
            };
            *count += 1;
            slugs.insert(slug);
        }
    }
    slugs
}

// ============================================================================
// Subscript and Superscript Support
// ============================================================================
//
// Pandoc `subscript` extension: `~x~` where x contains no whitespace or `~`.
// Pandoc `superscript` extension: `^x^` where x contains no whitespace or `^`.
//
// These are distinct from GFM strikethrough (`~~text~~`) and Pandoc inline
// footnotes (`^[...]`). The disambiguation rule for subscript is: reject any
// match where the opening or closing `~` is immediately adjacent to another `~`
// (which would make it GFM strikethrough). For superscript, reject matches
// where a `^` neighbour would form `^^`.

/// Pattern for Pandoc subscript: `~x~` where x is non-whitespace, non-`~`.
static SUBSCRIPT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~[^\s~]+~").unwrap());

/// Pattern for Pandoc superscript: `^x^` where x is non-whitespace, non-`^`.
static SUPERSCRIPT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^[^\s^]+\^").unwrap());

/// Detect Pandoc subscript (`~x~`) and superscript (`^x^`) ranges.
///
/// Returns byte ranges covering the full delimited span (including the
/// delimiter characters). Excludes `~~strikethrough~~` and superscript-like
/// runs of `^^`. The returned ranges are sorted by `start`.
///
/// Note: a `^[…]^` construct will also match `detect_inline_footnote_ranges`.
/// Rules that distinguish footnotes from superscripts must check both accessors.
pub fn detect_subscript_superscript_ranges(content: &str) -> Vec<ByteRange> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();

    for m in SUBSCRIPT_PATTERN.find_iter(content) {
        // Reject if preceded or followed by `~` (would be strikethrough).
        let prev = m.start().checked_sub(1).map_or(0, |i| bytes[i]);
        let next = bytes.get(m.end()).copied().unwrap_or(0);
        if prev != b'~' && next != b'~' {
            ranges.push(ByteRange {
                start: m.start(),
                end: m.end(),
            });
        }
    }
    for m in SUPERSCRIPT_PATTERN.find_iter(content) {
        // Reject if preceded or followed by `^` (would be a `^^` run).
        let prev = m.start().checked_sub(1).map_or(0, |i| bytes[i]);
        let next = bytes.get(m.end()).copied().unwrap_or(0);
        if prev != b'^' && next != b'^' {
            ranges.push(ByteRange {
                start: m.start(),
                end: m.end(),
            });
        }
    }
    // Sort because the two regex passes are merged and their results may interleave.
    ranges.sort_by_key(|r| r.start);
    ranges
}

// ============================================================================
// Inline Code Attribute Support
// ============================================================================
//
// Pandoc `inline_code_attributes` extension: `` `code`{.lang} ``
//
// The attribute block must immediately follow the closing backtick of the
// inline code span. Only the `{...}` part is captured; the backtick span
// itself is already handled by the standard code-span detector.

/// Pattern for inline code attribute: a backtick-quoted span immediately
/// followed by `{...}`. We capture only the trailing attribute block.
static INLINE_CODE_ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`]*`(\{[^}]+\})").unwrap());

/// Detect Pandoc inline code attribute ranges.
///
/// Inline code attributes are written as `` `code`{.lang} ``. Returns the
/// byte ranges of the trailing `{...}` attribute block only (not the
/// backticked code itself).
pub fn detect_inline_code_attr_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    for caps in INLINE_CODE_ATTR.captures_iter(content) {
        let m = caps.get(1).unwrap();
        ranges.push(ByteRange {
            start: m.start(),
            end: m.end(),
        });
    }
    ranges
}

// ============================================================================
// Example List Support
// ============================================================================
//
// Pandoc `example_lists` extension:
// - Line-start marker: `(@)` or `(@label)` followed by whitespace
// - Inline reference: `(@label)` appearing mid-paragraph (not at line start)
//
// Example keys contain letters, digits, underscores, and hyphens.
// The anonymous form `(@)` is valid as a marker but cannot appear as a reference
// (references require a label to be named).

/// Pattern for an example-list marker at line start: `(@)` or `(@label)` followed
/// by whitespace. Captures the `(@...)` portion.
static EXAMPLE_LIST_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(\(@[A-Za-z0-9_-]*\))[ \t]+").unwrap());

/// Pattern for an example reference: `(@label)` anywhere in text. Used together
/// with the marker pre-pass to filter out line-start markers.
static EXAMPLE_REFERENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\(@[A-Za-z0-9_-]+\))").unwrap());

/// Detect Pandoc example-list marker ranges (`(@)` / `(@label)` at line start).
///
/// Returns byte ranges covering the `(@...)` portion of each marker. Used by
/// rules that process list markers to skip Pandoc example markers.
pub fn detect_example_list_marker_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    for caps in EXAMPLE_LIST_MARKER.captures_iter(content) {
        let m = caps.get(1).unwrap();
        ranges.push(ByteRange {
            start: m.start(),
            end: m.end(),
        });
    }
    ranges
}

/// Detect Pandoc example reference ranges (`(@label)` not at line start).
///
/// Excludes positions whose start byte appears in `marker_ranges` (those are
/// line-start markers, not references). The caller must pass the already-computed
/// result of [`detect_example_list_marker_ranges`] so the marker regex is not
/// executed a second time.
pub fn detect_example_reference_ranges(content: &str, marker_ranges: &[ByteRange]) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let marker_starts: std::collections::HashSet<usize> = marker_ranges.iter().map(|r| r.start).collect();
    for caps in EXAMPLE_REFERENCE.captures_iter(content) {
        let m = caps.get(1).unwrap();
        if !marker_starts.contains(&m.start()) {
            ranges.push(ByteRange {
                start: m.start(),
                end: m.end(),
            });
        }
    }
    ranges
}

// ============================================================================
// Bracketed Span Support
// ============================================================================
//
// Pandoc `bracketed_spans` extension: `[text]{attrs}` where attrs is a
// non-empty Pandoc attribute block.
//
// Distinguished from `[text](url)` (link) and `[text][ref]` (reference link)
// by requiring `]{` immediately adjacent — the `{` must directly follow `]`
// with no intervening characters.

/// Pattern for Pandoc bracketed span: `[text]{attrs}` where attrs is a
/// non-empty Pandoc attribute block. The regex requires `]{` immediately
/// adjacent (no characters between `]` and `{`), which excludes `[text](url)`
/// links and `[text][ref]` reference links.
static BRACKETED_SPAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]+\]\{[^}]+\}").unwrap());

/// Detect Pandoc bracketed span ranges (`[text]{attrs}`).
///
/// Returns byte ranges covering the full `[...]` + `{...}` span. The detector
/// is structural only — it does not validate `attrs` content.
pub fn detect_bracketed_span_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    for m in BRACKETED_SPAN.find_iter(content) {
        ranges.push(ByteRange {
            start: m.start(),
            end: m.end(),
        });
    }
    ranges
}

// ============================================================================
// Line Block Support
// ============================================================================
//
// Pandoc `line_blocks` extension: a contiguous run of lines starting with `| `
// (pipe space). Each line in a line block is rendered as a separate line of
// verse or address. Continuation lines — indented, non-empty, not starting
// with `|` — extend the immediately preceding block line.
//
// Distinguished from pipe tables: a line whose trimmed form ends with `|`
// (i.e. `| col1 | col2 |`) is a table row, not a line block entry.

/// Detect Pandoc line blocks (consecutive lines starting with `| `).
///
/// A line block is a contiguous run of lines where each line either:
/// - Starts with `| ` (a single pipe followed by space) and does NOT
///   end with `|` (which would be a pipe-table row), or
/// - Is a continuation line (whitespace-indented, non-empty, not starting
///   with `|`) appearing within an active line-block run.
///
/// A blank line ends the run.
pub fn detect_line_block_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut in_block = false;
    let mut block_start = 0usize;
    let mut block_end = 0usize;
    let mut byte_offset = 0usize;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let is_line_block_line = trimmed.starts_with("| ") && !trimmed.trim_end().ends_with('|');
        let is_continuation = in_block
            && !trimmed.is_empty()
            && trimmed.starts_with(|c: char| c.is_whitespace())
            && !trimmed.trim_start().starts_with('|');

        if is_line_block_line || is_continuation {
            if !in_block {
                block_start = byte_offset;
                in_block = true;
            }
            block_end = byte_offset + line.len();
        } else if in_block {
            ranges.push(ByteRange {
                start: block_start,
                end: block_end,
            });
            in_block = false;
        }
        byte_offset += line.len();
    }
    if in_block {
        ranges.push(ByteRange {
            start: block_start,
            end: block_end,
        });
    }
    ranges
}

// ============================================================================
// Pipe-Table Caption Support
// ============================================================================
//
// Pandoc `table_captions` extension: a `: caption text` line that appears
// adjacent to a pipe table, separated by exactly one blank line (either
// above or below). Without the blank-line adjacency to a pipe table, a
// `: text` line is a definition-list value and must NOT be matched here.
//
// Matching rule:
//   caption_below: caption at line i, blank at i+1, pipe-table row at i+2
//   caption_above: pipe-table row at i-2, blank at i-1, caption at i

/// Detect Pandoc pipe-table caption lines (`: caption`) adjacent (above or
/// below, separated by exactly one blank line) to a pipe table. A `: text`
/// line not adjacent to a table is treated as a definition-list value and
/// is not matched here.
///
/// Iterates with `split_inclusive('\n')` so byte ranges remain accurate for
/// content without a trailing newline and for CRLF line endings.
pub fn detect_pipe_table_caption_ranges(content: &str) -> Vec<ByteRange> {
    let mut lines: Vec<&str> = Vec::new();
    let mut line_offsets: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        line_offsets.push(offset);
        lines.push(line);
        offset += line.len();
    }
    line_offsets.push(offset);

    fn line_body(line: &str) -> &str {
        line.trim_end_matches('\n').trim_end_matches('\r')
    }
    fn is_pipe_table_row(line: &str) -> bool {
        let t = line_body(line).trim();
        t.starts_with('|') && t.ends_with('|') && t.len() >= 3
    }
    fn is_caption_line(line: &str) -> bool {
        line_body(line).trim_start().starts_with(": ")
    }
    fn is_blank(line: &str) -> bool {
        line_body(line).trim().is_empty()
    }

    let mut ranges = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !is_caption_line(line) {
            continue;
        }
        let table_below = i + 2 < lines.len() && is_blank(lines[i + 1]) && is_pipe_table_row(lines[i + 2]);
        let table_above = i >= 2 && is_blank(lines[i - 1]) && is_pipe_table_row(lines[i - 2]);
        if table_below || table_above {
            ranges.push(ByteRange {
                start: line_offsets[i],
                end: line_offsets[i + 1],
            });
        }
    }
    ranges
}

// ============================================================================
// YAML Metadata Block Support
// ============================================================================
//
// Pandoc `yaml_metadata_block` extension: one or more `---`-delimited YAML
// blocks anywhere in the document. Unlike standard frontmatter (single block
// at file start), Pandoc allows:
//   - Multiple blocks per document
//   - `---` opener
//   - Either `---` or `...` as the closer
//   - Opener must be at start-of-file OR immediately after a blank line
//   - Unterminated openers are skipped

/// Detect Pandoc YAML metadata blocks (`---...---` or `---...`).
/// Unlike standard frontmatter, these can appear anywhere in the document
/// and there can be multiple per file.
pub fn detect_yaml_metadata_block_ranges(content: &str) -> Vec<ByteRange> {
    let mut lines: Vec<&str> = Vec::new();
    let mut line_offsets: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        line_offsets.push(offset);
        lines.push(line);
        offset += line.len();
    }
    line_offsets.push(offset);

    fn line_body(line: &str) -> &str {
        line.trim_end_matches('\n').trim_end_matches('\r')
    }
    fn is_blank(line: &str) -> bool {
        line_body(line).trim().is_empty()
    }
    fn is_opener(line: &str) -> bool {
        line_body(line).trim_end() == "---"
    }
    fn is_closer(line: &str) -> bool {
        let t = line_body(line).trim_end();
        t == "---" || t == "..."
    }

    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let preceded_by_blank = i == 0 || is_blank(lines[i - 1]);
        if preceded_by_blank && is_opener(lines[i]) {
            let mut j = i + 1;
            let mut found_closer = false;
            while j < lines.len() {
                if is_closer(lines[j]) {
                    ranges.push(ByteRange {
                        start: line_offsets[i],
                        end: line_offsets[j + 1],
                    });
                    i = j + 1;
                    found_closer = true;
                    break;
                }
                j += 1;
            }
            if !found_closer {
                // Unterminated opener — skip and continue scanning.
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    ranges
}

// ============================================================================
// Grid Table Support
// ============================================================================
//
// Pandoc `grid_tables` extension: a contiguous block of lines where the
// first line is a `+---+---+` border row (`+` corners, `-` or `=` between),
// followed by alternating content rows (`| ... | ... |`) and border rows
// (`+---+---+` or `+===+===+`), ending with a closing border row.
// At least one content row is required for a valid grid table.

/// Pattern for a grid-table border row: `+---+---+` or `+===+===+`.
static GRID_BORDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\+(?:[-=]+\+)+\s*$").unwrap());

/// Pattern for a grid-table content row: `| ... | ... |`.
static GRID_CONTENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\|.*\|\s*$").unwrap());

/// Detect Pandoc grid tables. A grid table is a contiguous run of lines
/// where the first line is a `+---+---+` border, followed by alternating
/// content rows `|...|` and border rows, and ending in a border row.
/// At least one content row is required.
///
/// Iterates with `split_inclusive('\n')` so byte ranges remain accurate for
/// content without a trailing newline and for CRLF line endings.
pub fn detect_grid_table_ranges(content: &str) -> Vec<ByteRange> {
    let mut lines: Vec<&str> = Vec::new();
    let mut line_offsets: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        line_offsets.push(offset);
        lines.push(line);
        offset += line.len();
    }
    line_offsets.push(offset);

    fn line_body(line: &str) -> &str {
        line.trim_end_matches('\n').trim_end_matches('\r')
    }
    fn is_border(line: &str) -> bool {
        GRID_BORDER.is_match(line_body(line))
    }
    fn is_content(line: &str) -> bool {
        GRID_CONTENT.is_match(line_body(line))
    }

    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if is_border(lines[i]) {
            let start_line = i;
            let mut j = i + 1;
            let mut last_border = i;
            let mut saw_content = false;
            while j < lines.len() {
                if is_border(lines[j]) {
                    last_border = j;
                    j += 1;
                } else if is_content(lines[j]) {
                    saw_content = true;
                    j += 1;
                } else {
                    break;
                }
            }
            // A valid grid table needs at least one content row and a
            // closing border (last_border > start_line).
            if saw_content && last_border > start_line {
                ranges.push(ByteRange {
                    start: line_offsets[start_line],
                    end: line_offsets[last_border + 1],
                });
                i = last_border + 1;
                continue;
            }
        }
        i += 1;
    }
    ranges
}

// ============================================================================
// Multi-line Table Support
// ============================================================================
//
// Pandoc `multiline_tables` extension: a block whose column widths are declared
// by an underline row of dashes-separated-by-spaces (MULTI_LINE_UNDERLINE), with
// an optional top-border and a mandatory closing solid-dash row (MULTI_LINE_BORDER).
// The header line immediately precedes the underline row.

/// Pattern for a multi-line table column-width underline row.
/// Matches two or more runs of dashes separated by spaces, e.g.:
/// `----------- ------- --------------- -------------------------`
static MULTI_LINE_UNDERLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-{2,}(?:\s+-{2,})+\s*$").unwrap());

/// Pattern for a multi-line table solid border row (≥10 dashes).
/// Used as both an optional top border and the mandatory closing border.
static MULTI_LINE_BORDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-{10,}\s*$").unwrap());

/// Detect Pandoc multi-line table ranges.
///
/// A multi-line table is recognised by an underline row (dashes separated by
/// spaces, ≥2 columns) immediately following a non-empty header line. The table
/// extends to the next solid-dash border row (≥10 dashes). An optional solid
/// border may appear before the header as well.
///
/// Iterates with `split_inclusive('\n')` so byte ranges remain accurate for
/// content without a trailing newline and for CRLF line endings.
pub fn detect_multi_line_table_ranges(content: &str) -> Vec<ByteRange> {
    let mut lines: Vec<&str> = Vec::new();
    let mut line_offsets: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        line_offsets.push(offset);
        lines.push(line);
        offset += line.len();
    }
    line_offsets.push(offset);

    fn line_body(line: &str) -> &str {
        line.trim_end_matches('\n').trim_end_matches('\r')
    }
    fn is_underline(line: &str) -> bool {
        MULTI_LINE_UNDERLINE.is_match(line_body(line))
    }
    fn is_border(line: &str) -> bool {
        MULTI_LINE_BORDER.is_match(line_body(line))
    }

    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        // Look for an underline row whose previous line is a non-empty header.
        if i >= 1 && is_underline(lines[i]) && !line_body(lines[i - 1]).is_empty() {
            // Walk backward from i-1 to find the first line of the header block.
            // The header may span multiple lines; keep going back while lines are
            // non-empty and not themselves borders or underlines.
            let mut header_start = i - 1;
            while header_start > 0
                && !line_body(lines[header_start - 1]).is_empty()
                && !is_border(lines[header_start - 1])
                && !is_underline(lines[header_start - 1])
            {
                header_start -= 1;
            }

            // Optionally include a solid border that precedes the header block.
            let start_line = if header_start > 0 && is_border(lines[header_start - 1]) {
                header_start - 1
            } else {
                header_start
            };

            // Walk forward from the line after the underline to find the closing border.
            let mut j = i + 1;
            let mut end_line: Option<usize> = None;
            while j < lines.len() {
                if is_border(lines[j]) {
                    // Closing solid-dash border found.
                    end_line = Some(j);
                    break;
                } else if j > i + 1 && is_underline(lines[j]) {
                    // Another column-width underline (second header section?):
                    // the previous line is the last body line.
                    end_line = Some(j - 1);
                    break;
                }
                j += 1;
            }

            if let Some(end) = end_line {
                ranges.push(ByteRange {
                    start: line_offsets[start_line],
                    end: line_offsets[end + 1],
                });
                i = end + 1;
                continue;
            }
            // No closing border found — skip this candidate and keep walking.
        }
        i += 1;
    }
    ranges
}

/// Detect Pandoc inline footnote ranges (`^[note text]`).
///
/// Returns byte ranges covering the entire `^[...]` span. Intended for rules that
/// process bracket-like syntax to skip Pandoc inline footnotes.
pub fn detect_inline_footnote_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    for caps in INLINE_FOOTNOTE_PATTERN.captures_iter(content) {
        let m = caps.get(1).unwrap();
        ranges.push(ByteRange {
            start: m.start(),
            end: m.end(),
        });
    }
    ranges
}

/// Find all citation ranges in content (byte ranges)
/// Returns ranges for both bracketed `[@key]` and inline `@key` citations.
///
/// Markdown link labels are excluded: when `[text]` is immediately followed
/// by `(` (inline link) or `[` (reference link), Pandoc prefers the link
/// parse over the citation parse, so any `@key` mentioned inside `text`
/// is not a citation. The link-label scan covers both bracketed-form
/// matches and free-floating inline `@key` matches.
pub fn find_citation_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();

    // Pre-compute Markdown link-label byte ranges (the `[text]` portion of
    // `[text](url)` or `[text][ref]`).
    let link_label_ranges: Vec<(usize, usize)> = LINK_LABEL_PATTERN
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| (m.start(), m.end())))
        .collect();

    let in_link_label = |pos: usize| -> bool { link_label_ranges.iter().any(|&(s, e)| pos >= s && pos < e) };

    // Find bracketed citations first (higher priority)
    for mat in BRACKETED_CITATION_PATTERN.find_iter(content) {
        if in_link_label(mat.start()) {
            continue;
        }
        ranges.push(ByteRange {
            start: mat.start(),
            end: mat.end(),
        });
    }

    // Find inline citations (but not inside already-found brackets or link labels)
    for cap in INLINE_CITATION_PATTERN.captures_iter(content) {
        if let Some(mat) = cap.get(1) {
            let start = mat.start();
            if in_link_label(start) {
                continue;
            }
            // Skip if this is inside a bracketed citation
            if !ranges.iter().any(|r| start >= r.start && start < r.end) {
                ranges.push(ByteRange { start, end: mat.end() });
            }
        }
    }

    // Sort by start position
    ranges.sort_by_key(|r| r.start);
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_div_open_detection() {
        // Valid div openings
        assert!(is_div_open("::: {.callout-note}"));
        assert!(is_div_open("::: {.callout-warning}"));
        assert!(is_div_open("::: {#myid .class}"));
        assert!(is_div_open("::: bordered"));
        assert!(is_div_open("  ::: {.note}")); // Indented
        assert!(is_div_open("::: {.callout-tip title=\"My Title\"}"));

        // Invalid patterns
        assert!(!is_div_open(":::")); // Just closing marker
        assert!(!is_div_open(":::  ")); // Just closing with trailing space
        assert!(!is_div_open("Regular text"));
        assert!(!is_div_open("# Heading"));
        assert!(!is_div_open("```python")); // Code fence
    }

    #[test]
    fn test_div_close_detection() {
        assert!(is_div_close(":::"));
        assert!(is_div_close(":::  "));
        assert!(is_div_close("  :::"));
        assert!(is_div_close("    :::  "));

        assert!(!is_div_close("::: {.note}"));
        assert!(!is_div_close("::: class"));
        assert!(!is_div_close(":::note"));
    }

    #[test]
    fn test_callout_detection() {
        assert!(is_callout_open("::: {.callout-note}"));
        assert!(is_callout_open("::: {.callout-warning}"));
        assert!(is_callout_open("::: {.callout-tip}"));
        assert!(is_callout_open("::: {.callout-important}"));
        assert!(is_callout_open("::: {.callout-caution}"));
        assert!(is_callout_open("::: {#myid .callout-note}"));
        assert!(is_callout_open("::: {.callout-note title=\"Title\"}"));

        assert!(!is_callout_open("::: {.note}")); // Not a callout
        assert!(!is_callout_open("::: {.bordered}")); // Not a callout
        assert!(!is_callout_open("::: callout-note")); // Missing braces
    }

    #[test]
    fn test_div_tracker() {
        let mut tracker = DivTracker::new();

        // Enter a div
        assert!(tracker.process_line("::: {.callout-note}"));
        assert!(tracker.is_inside_div());

        // Inside content
        assert!(tracker.process_line("This is content."));
        assert!(tracker.is_inside_div());

        // Exit the div
        assert!(!tracker.process_line(":::"));
        assert!(!tracker.is_inside_div());
    }

    #[test]
    fn test_nested_divs() {
        let mut tracker = DivTracker::new();

        // Outer div
        assert!(tracker.process_line("::: {.outer}"));
        assert!(tracker.is_inside_div());

        // Inner div
        assert!(tracker.process_line("  ::: {.inner}"));
        assert!(tracker.is_inside_div());

        // Content
        assert!(tracker.process_line("    Content"));
        assert!(tracker.is_inside_div());

        // Close inner
        assert!(tracker.process_line("  :::"));
        assert!(tracker.is_inside_div());

        // Close outer
        assert!(!tracker.process_line(":::"));
        assert!(!tracker.is_inside_div());
    }

    #[test]
    fn test_detect_div_block_ranges() {
        let content = r#"# Heading

::: {.callout-note}
This is a note.
:::

Regular text.

::: {.bordered}
Content here.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 2);

        // First div
        let first_div_content = &content[ranges[0].start..ranges[0].end];
        assert!(first_div_content.contains("callout-note"));
        assert!(first_div_content.contains("This is a note"));

        // Second div
        let second_div_content = &content[ranges[1].start..ranges[1].end];
        assert!(second_div_content.contains("bordered"));
        assert!(second_div_content.contains("Content here"));
    }

    #[test]
    fn test_pandoc_attributes() {
        assert!(has_pandoc_attributes("# Heading {#custom-id}"));
        assert!(has_pandoc_attributes("# Heading {.unnumbered}"));
        assert!(has_pandoc_attributes("![Image](path.png){#fig-1 width=\"50%\"}"));
        assert!(has_pandoc_attributes("{#id .class key=\"value\"}"));

        assert!(!has_pandoc_attributes("# Heading"));
        assert!(!has_pandoc_attributes("Regular text"));
        assert!(!has_pandoc_attributes("{}"));
    }

    #[test]
    fn test_div_with_title_attribute() {
        let content = r#"::: {.callout-note title="Important Note"}
This is the content of the note.
It can span multiple lines.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert!(is_callout_open("::: {.callout-note title=\"Important Note\"}"));
    }

    #[test]
    fn test_unclosed_div() {
        let content = r#"::: {.callout-note}
This note is never closed.
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        // Should include all content to end of document
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_heading_inside_callout() {
        let content = r#"::: {.callout-warning}
## Warning Title

Warning content here.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);

        let div_content = &content[ranges[0].start..ranges[0].end];
        assert!(div_content.contains("## Warning Title"));
    }

    // Citation tests
    #[test]
    fn test_has_citations() {
        assert!(has_citations("See @smith2020 for details."));
        assert!(has_citations("[@smith2020]"));
        assert!(has_citations("Multiple [@a; @b] citations"));
        assert!(!has_citations("No citations here"));
        // has_citations is just a quick @ check - emails will pass (intended behavior)
        assert!(has_citations("Email: user@example.com"));
    }

    #[test]
    fn test_bracketed_citation_detection() {
        let content = "See [@smith2020] for more info.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[@smith2020]");
    }

    #[test]
    fn test_inline_citation_detection() {
        let content = "As @smith2020 argues, this is true.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "@smith2020");
    }

    #[test]
    fn test_multiple_citations_in_brackets() {
        let content = "See [@smith2020; @jones2021] for details.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[@smith2020; @jones2021]");
    }

    #[test]
    fn test_citation_with_prefix() {
        let content = "[see @smith2020, p. 10]";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[see @smith2020, p. 10]");
    }

    #[test]
    fn test_suppress_author_citation() {
        let content = "The theory [-@smith2020] states that...";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[-@smith2020]");
    }

    #[test]
    fn test_mixed_citations() {
        let content = "@smith2020 argues that [@jones2021] is wrong.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 2);
        // Inline citation
        assert_eq!(&content[ranges[0].start..ranges[0].end], "@smith2020");
        // Bracketed citation
        assert_eq!(&content[ranges[1].start..ranges[1].end], "[@jones2021]");
    }

    #[test]
    fn test_email_not_confused_with_citation() {
        // Email addresses should not match as inline citations when properly filtered
        // The has_citations() is just a quick check, but find_citation_ranges uses more strict patterns
        let content = "Contact user@example.com for help.";
        let ranges = find_citation_ranges(content);
        // Email should not be detected as citation (@ is preceded by alphanumeric)
        assert!(
            ranges.is_empty()
                || !ranges.iter().any(|r| {
                    let s = &content[r.start..r.end];
                    s.contains("example.com")
                })
        );
    }

    /// Bracketed link text containing an email (`@` embedded in a word) must
    /// NOT be classified as a Pandoc citation. A citation `@key` requires the
    /// `@` to sit at a citation boundary — start of bracket, after `-`, after
    /// whitespace, or after `;` — never in the middle of a word like an email.
    #[test]
    fn test_bracketed_link_text_with_email_not_citation() {
        let content = "[contact user@example.com](#missing)";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.is_empty(),
            "Bracketed link text with embedded email must not be detected as a Pandoc citation: {ranges:?}"
        );
    }

    /// Same bracketed text with an empty link target — also a link, not a citation.
    #[test]
    fn test_bracketed_link_text_with_email_empty_href_not_citation() {
        let content = "[contact user@example.com]()";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.is_empty(),
            "Bracketed link text with embedded email and empty href must not be a Pandoc citation: {ranges:?}"
        );
    }

    /// A bracketed label whose text mentions a citation key but is *immediately*
    /// followed by a link target `(...)` is a Markdown link, not a citation.
    /// Pandoc itself prefers the link interpretation for `[text](url)` over a
    /// citation parse, even when `text` contains `@key`.
    #[test]
    fn test_bracketed_text_followed_by_inline_link_not_citation() {
        let content = "[see @smith2020](#missing)";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.is_empty(),
            "Bracketed text followed by `(...)` is a link, not a citation: {ranges:?}"
        );
    }

    /// Same disambiguation when the link target is empty: still a link.
    #[test]
    fn test_bracketed_text_followed_by_empty_inline_link_not_citation() {
        let content = "[see @smith2020]()";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.is_empty(),
            "Bracketed text followed by `()` is a link with empty href, not a citation: {ranges:?}"
        );
    }

    /// Reference-style links `[text][ref]` are also links — the bracketed
    /// label `[text]` must not be classified as a citation just because it
    /// contains `@key`.
    #[test]
    fn test_bracketed_text_followed_by_reference_link_not_citation() {
        let content = "[see @smith2020][ref]";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.is_empty(),
            "Bracketed text followed by `[ref]` is a reference link, not a citation: {ranges:?}"
        );
    }

    /// Standalone bracketed citations remain citations: nothing immediately
    /// follows the closing `]`, so the link disambiguation does not apply.
    #[test]
    fn test_standalone_bracketed_citation_still_detected() {
        let content = "See [see @smith2020] for details.";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.iter().any(|r| &content[r.start..r.end] == "[see @smith2020]"),
            "Standalone bracketed citation must still be detected: {ranges:?}"
        );
    }

    /// Citation followed by sentence punctuation remains a citation.
    #[test]
    fn test_bracketed_citation_followed_by_punctuation_still_detected() {
        let content = "Note [@smith2020].";
        let ranges = find_citation_ranges(content);
        assert!(
            ranges.iter().any(|r| &content[r.start..r.end] == "[@smith2020]"),
            "Bracketed citation followed by `.` must still be detected: {ranges:?}"
        );
    }

    #[test]
    fn test_detect_inline_footnotes() {
        let content = "See ^[a quick note] here.\nAnd ^[another one] too.\n";
        let ranges = detect_inline_footnote_ranges(content);
        assert_eq!(ranges.len(), 2);
        // First footnote
        let first_start = content.find("^[").unwrap();
        let first_end = content[first_start..].find(']').unwrap() + first_start + 1;
        assert_eq!(ranges[0].start, first_start);
        assert_eq!(ranges[0].end, first_end);
        // Second footnote
        let second_start = content[first_end..].find("^[").unwrap() + first_end;
        let second_end = content[second_start..].find(']').unwrap() + second_start + 1;
        assert_eq!(ranges[1].start, second_start);
        assert_eq!(ranges[1].end, second_end);
    }

    #[test]
    fn test_inline_footnote_with_brackets_inside() {
        // Inline footnotes do not nest; a `]` inside terminates the footnote.
        // This documents the chosen behavior. Pandoc itself supports nesting via
        // backslash-escapes; rumdl currently treats the first unescaped `]` as
        // the terminator.
        let content = "Note ^[ref to [other] thing] here.\n";
        let ranges = detect_inline_footnote_ranges(content);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn test_inline_footnote_does_not_match_image_or_link() {
        // `![alt]` is an image, not a footnote.
        let content = "An image ![alt](url) and a link [txt](url).\n";
        let ranges = detect_inline_footnote_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_implicit_header_reference_slug() {
        // Pandoc lowercases, replaces internal whitespace with `-`, and strips
        // punctuation other than `_`, `-`, `.`.
        assert_eq!(pandoc_header_slug("My Section"), "my-section");
        assert_eq!(pandoc_header_slug("API: v2!"), "api-v2");
        assert_eq!(pandoc_header_slug("  Trim Me  "), "trim-me");
        assert_eq!(pandoc_header_slug("Multiple   Spaces"), "multiple-spaces");
    }

    #[test]
    fn test_collect_pandoc_header_slugs() {
        let content = "# My Section\n\n## Sub-section\n\nbody\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("my-section"));
        assert!(slugs.contains("sub-section"));
    }

    #[test]
    fn test_collect_pandoc_header_slugs_strips_attribute_block() {
        let content = "# My Section {#custom-id .red}\n## Plain Section\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("my-section"));
        assert!(slugs.contains("plain-section"));
        // Slug must not include the attribute block contents.
        assert!(!slugs.iter().any(|s| s.contains("custom-id")));
    }

    #[test]
    fn test_collect_pandoc_header_slugs_preserves_body_braces() {
        // `{` in heading body must NOT be mistaken for an attribute block.
        let content = "# Some {curly} word in title\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("some-curly-word-in-title"));
    }

    #[test]
    fn test_collect_pandoc_header_slugs_disambiguates_duplicates() {
        // Pandoc's auto_identifiers extension assigns the second heading with the
        // same slug `<base>-1`, the third `<base>-2`, etc. Both base and suffixed
        // forms must be reachable as link targets.
        let content = "# A.\n\nbody\n\n# A.\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("a."), "first occurrence should expose base slug `a.`");
        assert!(
            slugs.contains("a.-1"),
            "second occurrence should expose `a.-1`: got {slugs:?}"
        );
    }

    #[test]
    fn test_collect_pandoc_header_slugs_three_duplicates_get_two_suffixes() {
        let content = "# Intro\n\n# Intro\n\n# Intro\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("intro"));
        assert!(slugs.contains("intro-1"));
        assert!(slugs.contains("intro-2"));
        assert!(
            !slugs.contains("intro-3"),
            "three occurrences must produce only -1 and -2 suffixes, not -3: got {slugs:?}"
        );
    }

    #[test]
    fn test_collect_pandoc_header_slugs_unique_headings_get_no_suffix() {
        let content = "# Foo\n\n# Bar\n\n# Baz\n";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("foo"));
        assert!(slugs.contains("bar"));
        assert!(slugs.contains("baz"));
        // Unique headings must not gain a `-1` suffix.
        assert!(!slugs.contains("foo-1"));
        assert!(!slugs.contains("bar-1"));
        assert!(!slugs.contains("baz-1"));
    }

    #[test]
    fn test_detect_example_list_markers() {
        let content = "(@)  First item.\n(@good) Second item.\n(@) Third item.\n";
        let ranges = detect_example_list_marker_ranges(content);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "(@)");
        let second_start = content.find("(@good)").unwrap();
        assert_eq!(ranges[1].start, second_start);
        assert_eq!(&content[ranges[1].start..ranges[1].end], "(@good)");
    }

    #[test]
    fn test_detect_example_references() {
        // `(@label)` mid-paragraph is a reference, not a list marker.
        let content = "As shown in (@good), this works.\n";
        let marker_ranges = detect_example_list_marker_ranges(content);
        let ranges = detect_example_reference_ranges(content, &marker_ranges);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn test_example_marker_must_be_at_line_start() {
        let content = "Inline (@) is not a marker.\n";
        let ranges = detect_example_list_marker_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_detect_subscript() {
        let content = "H~2~O is water.\n";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "~2~");
    }

    #[test]
    fn test_detect_superscript() {
        let content = "2^10^ is 1024.\n";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "^10^");
    }

    #[test]
    fn test_subscript_does_not_match_strikethrough() {
        // `~~text~~` is GFM strikethrough, not subscript.
        let content = "This is ~~struck~~.\n";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_superscript_with_internal_space_is_not_matched() {
        // Pandoc requires no whitespace inside `^...^`.
        let content = "x^a b^ y\n";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_subscript_at_start_of_input() {
        // Position 0: previous-byte path uses checked_sub(1).unwrap_or(0).
        let content = "~x~ rest of line\n";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "~x~");
    }

    #[test]
    fn test_superscript_at_end_of_input_no_newline() {
        // EOF: next-byte path uses bytes.get(end).unwrap_or(0).
        let content = "text ^x^";
        let ranges = detect_subscript_superscript_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "^x^");
    }

    #[test]
    fn test_detect_inline_code_attribute() {
        // `code`{.python} — the {.python} is a Pandoc attribute on inline code.
        let content = "Use `print()`{.python} for output.\n";
        let ranges = detect_inline_code_attr_ranges(content);
        assert_eq!(ranges.len(), 1);
        let r = &ranges[0];
        assert_eq!(&content[r.start..r.end], "{.python}");
    }

    #[test]
    fn test_inline_code_attribute_only_after_backtick() {
        // A bare `{...}` in prose is not an inline code attribute.
        let content = "Use {.example} for the class.\n";
        let ranges = detect_inline_code_attr_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_inline_code_attribute_multiple_on_one_line() {
        let content = "Use `a`{.x} and `b`{.y} here.\n";
        let ranges = detect_inline_code_attr_ranges(content);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "{.x}");
        assert_eq!(&content[ranges[1].start..ranges[1].end], "{.y}");
    }

    #[test]
    fn test_inline_code_attribute_compound_attributes() {
        // Pandoc supports compound attribute blocks: classes, IDs, and key=value pairs.
        let content = "Use `code`{.lang #id key=value} here.\n";
        let ranges = detect_inline_code_attr_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "{.lang #id key=value}");
    }

    #[test]
    fn test_detect_bracketed_span() {
        let content = "This is [some text]{.smallcaps} here.\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 1);
        let r = &ranges[0];
        assert_eq!(&content[r.start..r.end], "[some text]{.smallcaps}");
    }

    #[test]
    fn test_bracketed_span_does_not_match_link() {
        // `[text](url)` is a link, not a bracketed span.
        let content = "A [link](http://example.com) here.\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_bracketed_span_does_not_match_reference_link() {
        // `[text][ref]` is a reference link.
        let content = "A [ref][def] here.\n[def]: http://example.com\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_bracketed_span_multiple_on_one_line() {
        let content = "[one]{.a} and [two]{.b} together.\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[one]{.a}");
        assert_eq!(&content[ranges[1].start..ranges[1].end], "[two]{.b}");
    }

    #[test]
    fn test_bracketed_span_rejects_empty_content() {
        // Both bracket and brace bodies require at least one character.
        let content = "[]{.x} and [x]{} here.\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_bracketed_span_at_start_of_line() {
        let content = "[head]{.intro} starts the line.\n";
        let ranges = detect_bracketed_span_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[head]{.intro}");
    }

    #[test]
    fn test_detect_line_block_single() {
        let content = "| The Lord of the Rings\n| by J.R.R. Tolkien\n";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_line_block_no_trailing_newline() {
        // Single-line block with no terminating newline must be flushed.
        let content = "| Only line";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_line_block_indented_pipe_is_not_continuation() {
        // An indented line whose non-whitespace content begins with `|` is
        // not a plain-text continuation; it ends the active block.
        let content = "| First\n  | indented\n";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end, "| First\n".len());
    }

    #[test]
    fn test_line_block_continuation_with_indent() {
        // A line starting with whitespace (and NOT `|`) inside a line block is
        // a continuation of the previous line.
        let content = "| First line\n  continuation\n| Second\n";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn test_line_block_separated_by_blank() {
        let content = "| Block A\n\n| Block B\n";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_line_block_does_not_match_pipe_table() {
        // A `| col |...| row` line ending with `|` is a pipe-table row, not a line block.
        let content = "| col1 | col2 |\n|------|------|\n";
        let ranges = detect_line_block_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_detect_pipe_table_caption_below() {
        let content = "\
| col1 | col2 |
|------|------|
| a    | b    |

: My caption
";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 1);
        let cap = &content[ranges[0].start..ranges[0].end];
        assert!(cap.starts_with(": My caption"));
    }

    #[test]
    fn test_detect_pipe_table_caption_above() {
        let content = "\
: Caption first

| col1 | col2 |
|------|------|
| a    | b    |
";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn test_colon_line_without_adjacent_table_is_definition_term() {
        // A `: text` line not adjacent to a table is part of a definition list.
        let content = "Term\n: definition\n";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_pipe_table_caption_two_blank_lines_does_not_match() {
        // Pandoc requires exactly one blank line between table and caption.
        let content = "\
| a | b |
|---|---|
| 1 | 2 |


: Caption
";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_pipe_table_caption_no_blank_line_does_not_match() {
        // Adjacent without a blank line is not a caption either.
        let content = "\
| a | b |
|---|---|
| 1 | 2 |
: Caption
";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_pipe_table_caption_no_trailing_newline() {
        // Caption is the final line of the document with no newline; the
        // computed end must equal the content length, not overshoot.
        let content = "\
| a | b |
|---|---|
| 1 | 2 |

: Trailing caption";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end, content.len());
        assert_eq!(&content[ranges[0].start..ranges[0].end], ": Trailing caption");
    }

    #[test]
    fn test_pipe_table_caption_handles_crlf() {
        // CRLF line endings must produce correct byte offsets too.
        let content = "| a | b |\r\n|---|---|\r\n| 1 | 2 |\r\n\r\n: CRLF caption\r\n";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 1);
        let cap = &content[ranges[0].start..ranges[0].end];
        assert!(cap.starts_with(": CRLF caption"));
    }

    #[test]
    fn test_pipe_table_caption_lone_colon_does_not_match() {
        // Pandoc requires `: ` (colon-space) for a caption; bare `:` is not.
        let content = "\
| a | b |
|---|---|
| 1 | 2 |

:
";
        let ranges = detect_pipe_table_caption_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_detect_metadata_block_at_start() {
        // Standard frontmatter case — should be returned as a metadata range.
        let content = "---\ntitle: Doc\n---\n\nBody.\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
    }

    #[test]
    fn test_detect_metadata_block_mid_document() {
        // Pandoc allows multiple `---...---` metadata blocks anywhere.
        let content = "---\ntitle: Doc\n---\n\n# Heading\n\n---\nauthor: X\n---\n\nBody.\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_metadata_block_uses_dot_terminator() {
        // Pandoc accepts `...` as an alternative terminator.
        let content = "---\ntitle: Doc\n...\n\nBody.\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn test_metadata_block_unterminated_opener_skipped() {
        // An opener with no closer reaching EOF must NOT produce a range.
        let content = "---\ntitle: Doc\nbody continues forever\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_metadata_block_dashes_after_text_are_not_opener() {
        // A `---` line not preceded by a blank is a horizontal rule,
        // not a metadata opener.
        let content = "Some prose paragraph.\n---\nbody: not-metadata\n---\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_metadata_block_no_trailing_newline() {
        // Block at end of file with no trailing newline; end must equal
        // content length, not overshoot.
        let content = "---\ntitle: Doc\n---";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_metadata_block_handles_crlf() {
        // CRLF endings must produce correct byte offsets.
        let content = "---\r\ntitle: Doc\r\n---\r\n\r\nBody.\r\n";
        let ranges = detect_yaml_metadata_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        let block = &content[ranges[0].start..ranges[0].end];
        assert!(block.starts_with("---\r\n"));
        assert!(block.ends_with("---\r\n"));
    }

    #[test]
    fn test_collect_pandoc_header_slugs_skips_code_blocks() {
        let content = "\
# Real Heading

```bash
# This is a bash comment
#!/usr/bin/env bash
```

# Another Heading
";
        let slugs = collect_pandoc_header_slugs(content);
        assert!(slugs.contains("real-heading"));
        assert!(slugs.contains("another-heading"));
        assert!(!slugs.contains("this-is-a-bash-comment"));
        assert!(!slugs.iter().any(|s| s.contains("usr-bin")));
    }

    #[test]
    fn test_detect_simple_grid_table() {
        let content = "\
+---------+---------+
| col1    | col2    |
+=========+=========+
| a       | b       |
+---------+---------+
";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_grid_table_with_surrounding_text() {
        let content = "\
Before.

+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+

After.
";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        let region = &content[ranges[0].start..ranges[0].end];
        assert!(region.contains("+---+---+"));
        assert!(!region.contains("Before"));
        assert!(!region.contains("After"));
    }

    #[test]
    fn test_lone_plus_dash_line_is_not_a_table() {
        let content = "Just a +---+ in prose.\n";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_grid_table_no_trailing_newline() {
        // Block at end of file with no trailing newline; end must equal
        // content length, not overshoot.
        let content = "+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_grid_table_crlf() {
        // CRLF endings must produce correct byte offsets.
        let content = "+---+---+\r\n| a | b |\r\n+---+---+\r\n| 1 | 2 |\r\n+---+---+\r\n";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_grid_table_borders_only_no_content_row_rejected() {
        // Two border lines with no content row must not form a valid table.
        let content = "+---+\n+---+\n";
        let ranges = detect_grid_table_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Multi-line table tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_multi_line_table() {
        let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example of a row that
                                    spans multiple lines.

  Second    row                 5.0 Here's another one. Note
                                    the blank line between
                                    rows.
-------------------------------------------------------------
";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_simple_dash_header_underline_only_does_not_match() {
        // The dash line has length 8 < 10 so it is not a MULTI_LINE_BORDER,
        // and it is not a MULTI_LINE_UNDERLINE (only one dash run — no spaces).
        let content = "Some text\n--------\nMore text\n";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_multi_line_table_no_trailing_newline() {
        // The last line has no trailing newline; end must equal content.len().
        let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example of a row that
                                    spans multiple lines.

  Second    row                 5.0 Here's another one. Note
                                    the blank line between
                                    rows.
-------------------------------------------------------------";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_multi_line_table_crlf() {
        // CRLF line endings must produce correct byte offsets.
        let content = "\
-------------------------------------------------------------\r\n\
 Centered   Default           Right Left\r\n\
  Header    Aligned         Aligned Aligned\r\n\
----------- ------- --------------- -------------------------\r\n\
   First    row                12.0 Example of a row that\r\n\
                                    spans multiple lines.\r\n\
\r\n\
  Second    row                 5.0 Here's another one. Note\r\n\
                                    the blank line between\r\n\
                                    rows.\r\n\
-------------------------------------------------------------\r\n";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_multi_line_table_unterminated_skipped() {
        // Header + underline but no closing border — must return 0 ranges.
        let content = "\
 Centered   Default
  Header    Aligned
----------- -------
   First    row
   Second   row
";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_multi_line_table_no_top_border() {
        // Valid table with no top border: header line immediately followed by
        // the column underline, then body rows, then closing border.
        let content = "\
  Centered   Default           Right Left
----------- ------- --------------- -------------------------
   First    row                12.0 Example
  Second    row                 5.0 Another
-------------------------------------------------------------
";
        let ranges = detect_multi_line_table_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_is_pandoc_raw_block_lang() {
        assert!(is_pandoc_raw_block_lang("{=html}"));
        assert!(is_pandoc_raw_block_lang("{=latex}"));
        assert!(is_pandoc_raw_block_lang("{=docx}"));
        assert!(is_pandoc_raw_block_lang("{=rst}"));
        // Hyphens and underscores are part of the allowed character set.
        assert!(is_pandoc_raw_block_lang("{=open-document}"));
        assert!(is_pandoc_raw_block_lang("{=my_format}"));
        // Uppercase is accepted (Pandoc itself is case-sensitive but the
        // grammar permits any ASCII alphanumeric).
        assert!(is_pandoc_raw_block_lang("{=HTML}"));
        // Reject Quarto exec blocks.
        assert!(!is_pandoc_raw_block_lang("{r}"));
        assert!(!is_pandoc_raw_block_lang("{python}"));
        // Reject malformed.
        assert!(!is_pandoc_raw_block_lang("{=}"));
        assert!(!is_pandoc_raw_block_lang("{=  }"));
        assert!(!is_pandoc_raw_block_lang("=html"));
        // Reject inner whitespace and special characters.
        assert!(!is_pandoc_raw_block_lang("{=html }"));
        assert!(!is_pandoc_raw_block_lang("{=ht ml}"));
    }

    #[test]
    fn test_is_pandoc_code_class_attr() {
        // Single class declares the language.
        assert!(is_pandoc_code_class_attr("{.python}"));
        assert!(is_pandoc_code_class_attr("{.haskell}"));
        assert!(is_pandoc_code_class_attr("{.rust}"));
        // Multiple classes — first class is the language, rest are decoration.
        assert!(is_pandoc_code_class_attr("{.haskell .numberLines}"));
        // Class plus id.
        assert!(is_pandoc_code_class_attr("{#myid .python}"));
        // Class plus key=value attributes.
        assert!(is_pandoc_code_class_attr("{.python startFrom=\"10\"}"));
        // Class anywhere in the attribute list.
        assert!(is_pandoc_code_class_attr("{#snippet .python startFrom=\"10\"}"));
        // Identifiers with hyphens and underscores are valid.
        assert!(is_pandoc_code_class_attr("{.objective-c}"));
        assert!(is_pandoc_code_class_attr("{.my_lang}"));

        // Reject — no class anywhere.
        assert!(!is_pandoc_code_class_attr("{}"));
        assert!(!is_pandoc_code_class_attr("{#myid}"));
        assert!(!is_pandoc_code_class_attr("{startFrom=\"10\"}"));
        // Reject — Pandoc raw block (handled by separate predicate).
        assert!(!is_pandoc_code_class_attr("{=html}"));
        // Reject — Quarto exec syntax (no leading dot).
        assert!(!is_pandoc_code_class_attr("{r}"));
        assert!(!is_pandoc_code_class_attr("{python}"));
        // Reject — bare dot with no identifier.
        assert!(!is_pandoc_code_class_attr("{.}"));
        // Reject — missing braces.
        assert!(!is_pandoc_code_class_attr(".python"));
        assert!(!is_pandoc_code_class_attr("python"));
    }
}
