use regex::Regex;
use std::sync::LazyLock;

// Better detection of inline code with support for multiple backticks
static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap());

// Inline math pattern - matches both $...$ and $$...$$ syntax
// The pattern allows zero or more characters between delimiters to handle empty math spans
static INLINE_MATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$[^$]*\$\$|\$[^$\n]*\$").unwrap());

// Documentation style patterns
static DOC_METADATA_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\*?\s*\*\*(?:[^*\s][^*]*[^*\s]|[^*\s])\*\*\s*:").unwrap());

// Bold text pattern (for preserving bold text in documentation) - only match valid bold without spaces
static BOLD_TEXT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*[^*\s][^*]*[^*\s]\*\*|\*\*[^*\s]\*\*").unwrap());

// Pre-compiled patterns for quick checks
static QUICK_DOC_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\*\s+\*").unwrap());
static QUICK_BOLD_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*[^*\s]").unwrap());

// Template/shortcode syntax pattern - {* ... *} used by documentation systems like FastAPI/MkDocs
// These are not emphasis markers but template directives for code inclusion/highlighting
static TEMPLATE_SHORTCODE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\*.*\*\}").unwrap());

/// Represents an emphasis marker found in text
#[derive(Debug, Clone, PartialEq)]
pub struct EmphasisMarker {
    pub marker_type: u8,  // b'*' or b'_' for faster comparison
    pub count: u8,        // 1 for single, 2 for double
    pub start_pos: usize, // Position in the line
}

impl EmphasisMarker {
    #[inline]
    pub fn end_pos(&self) -> usize {
        self.start_pos + self.count as usize
    }

    #[inline]
    pub fn as_char(&self) -> char {
        self.marker_type as char
    }
}

/// Represents a complete emphasis span
#[derive(Debug, Clone)]
pub struct EmphasisSpan {
    pub opening: EmphasisMarker,
    pub closing: EmphasisMarker,
    pub content: String,
    pub has_leading_space: bool,
    pub has_trailing_space: bool,
}

/// Enhanced inline code replacement with optimized performance
/// Replaces inline code with 'X' characters to prevent false positives in emphasis detection
#[inline]
pub fn replace_inline_code(line: &str) -> String {
    // Quick check: if no backticks, return original
    if !line.contains('`') {
        return line.to_string();
    }

    let mut result = line.to_string();
    let mut offset = 0;

    for cap in INLINE_CODE.captures_iter(line) {
        if let (Some(full_match), Some(_opening), Some(_content), Some(_closing)) =
            (cap.get(0), cap.get(1), cap.get(2), cap.get(3))
        {
            let match_start = full_match.start();
            let match_end = full_match.end();
            // Use 'X' instead of spaces to avoid false positives for "spaces in emphasis"
            let placeholder = "X".repeat(match_end - match_start);

            result.replace_range(match_start + offset..match_end + offset, &placeholder);
            offset += placeholder.len() - (match_end - match_start);
        }
    }

    result
}

/// Replace inline math ($...$ and $$...$$) with placeholder characters
/// This prevents math content from being mistaken for emphasis markers
pub fn replace_inline_math(line: &str) -> String {
    // Quick check: if no dollar signs, return original
    if !line.contains('$') {
        return line.to_string();
    }

    let mut result = line.to_string();
    let mut offset: isize = 0;

    for m in INLINE_MATH.find_iter(line) {
        let match_start = m.start();
        let match_end = m.end();
        // Use 'M' instead of spaces or asterisks to avoid affecting emphasis detection
        let placeholder = "M".repeat(match_end - match_start);

        let adjusted_start = (match_start as isize + offset) as usize;
        let adjusted_end = (match_end as isize + offset) as usize;
        result.replace_range(adjusted_start..adjusted_end, &placeholder);
        offset += placeholder.len() as isize - (match_end - match_start) as isize;
    }

    result
}

/// Optimized emphasis marker parsing using byte iteration
#[inline]
pub fn find_emphasis_markers(line: &str) -> Vec<EmphasisMarker> {
    // Early return for lines without emphasis markers
    if !line.contains('*') && !line.contains('_') {
        return Vec::new();
    }

    let mut markers = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'*' || byte == b'_' {
            let start_pos = i;
            let mut count = 1u8;

            // Count consecutive markers (limit to avoid overflow)
            while i + (count as usize) < bytes.len() && bytes[i + (count as usize)] == byte && count < 3 {
                count += 1;
            }

            // Only consider single (*) and double (**) markers
            if count == 1 || count == 2 {
                markers.push(EmphasisMarker {
                    marker_type: byte,
                    count,
                    start_pos,
                });
            }

            i += count as usize;
        } else {
            i += 1;
        }
    }

    markers
}

/// Find all emphasis spans in a line, excluding only single emphasis (not strong)
pub fn find_single_emphasis_spans(line: &str, markers: &[EmphasisMarker]) -> Vec<EmphasisSpan> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    // CommonMark left/right-flanking (whitespace clause): an emphasis opener
    // must not be immediately followed by whitespace, and a closer must not be
    // immediately preceded by it. A marker that can do neither is a literal
    // `*`/`_` (e.g. a list-marker `*`, or a `*` flanked by spaces) and is
    // transparent to delimiter matching. `find_emphasis_spans` (MD037)
    // deliberately keeps such runs so MD037 can flag the spaces inside them;
    // this single-emphasis finder, used only by MD049, must not.
    let bytes = line.as_bytes();
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let can_open = |m: &EmphasisMarker| {
        let after = m.end_pos();
        after < bytes.len() && !is_ws(bytes[after])
    };
    let can_close = |m: &EmphasisMarker| m.start_pos > 0 && !is_ws(bytes[m.start_pos - 1]);

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] || markers[i].count != 1 || !can_open(&markers[i]) {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check - only single emphasis that can close
            if closing.marker_type == opening.marker_type && closing.count == 1 && can_close(closing) {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // A pairing is only blocked by an intervening *viable*
                        // delimiter (one that can itself open or close); a
                        // transparent literal marker does not interfere, which
                        // lets `*foo * bar*` pair its outer asterisks the way
                        // CommonMark does.
                        let crosses_markers = markers[i + 1..j].iter().any(|marker| {
                            marker.marker_type == opening.marker_type
                                && marker.count == 1
                                && (can_open(marker) || can_close(marker))
                        });

                        if !crosses_markers {
                            // Flanking guarantees the content is not whitespace-
                            // padded, but keep the fields honest for callers.
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(EmphasisSpan {
                                opening: opening.clone(),
                                closing: closing.clone(),
                                content: content.to_string(),
                                has_leading_space,
                                has_trailing_space,
                            });

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    spans
}

/// Optimized emphasis span finding with reduced complexity (includes both single and strong)
pub fn find_emphasis_spans(line: &str, markers: &[EmphasisMarker]) -> Vec<EmphasisSpan> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check
            if closing.marker_type == opening.marker_type && closing.count == opening.count {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // Quick check for crossing markers
                        let crosses_markers = markers[i + 1..j]
                            .iter()
                            .any(|marker| marker.marker_type == opening.marker_type);

                        if !crosses_markers {
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(EmphasisSpan {
                                opening: opening.clone(),
                                closing: closing.clone(),
                                content: content.to_string(),
                                has_leading_space,
                                has_trailing_space,
                            });

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    spans
}

/// Byte ranges `[start, end)` of *valid* CommonMark emphasis on a single line,
/// covering both single (`*`/`_`) and double (`**`/`__`) emphasis.
///
/// Unlike [`find_emphasis_spans`] (which greedily pairs markers and keeps
/// whitespace-padded runs so MD037 can flag them), this applies the CommonMark
/// left/right-flanking whitespace rule: an opener must not be immediately
/// followed by whitespace, a closer must not be immediately preceded by it, and
/// a marker that can do neither is a transparent literal that does not block an
/// outer pairing. MD037 uses these ranges to avoid flagging text that is in
/// fact valid emphasis containing an interior literal marker, e.g.
/// `*foo * bar*` -> `<em>foo * bar</em>`, where the inner `* ` is literal.
pub fn find_valid_emphasis_ranges(line: &str, markers: &[EmphasisMarker]) -> Vec<(usize, usize)> {
    if markers.len() < 2 {
        return Vec::new();
    }

    let bytes = line.as_bytes();
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let can_open = |m: &EmphasisMarker| {
        let after = m.end_pos();
        after < bytes.len() && !is_ws(bytes[after])
    };
    let can_close = |m: &EmphasisMarker| m.start_pos > 0 && !is_ws(bytes[m.start_pos - 1]);

    let mut ranges = Vec::new();
    let mut used = vec![false; markers.len()];

    for i in 0..markers.len() {
        if used[i] || !can_open(&markers[i]) {
            continue;
        }

        let opening = &markers[i];

        for j in (i + 1)..markers.len() {
            if used[j] {
                continue;
            }

            let closing = &markers[j];

            // Same marker run (type and strength) that can validly close.
            if closing.marker_type == opening.marker_type && closing.count == opening.count && can_close(closing) {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // Only an intervening *viable* delimiter of the same
                        // type blocks the pairing; transparent literals do not.
                        let crosses = markers[i + 1..j]
                            .iter()
                            .any(|m| m.marker_type == opening.marker_type && (can_open(m) || can_close(m)));

                        if !crosses {
                            ranges.push((opening.start_pos, closing.end_pos()));
                            used[i] = true;
                            used[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    ranges
}

/// Fast validation of emphasis span context
#[inline]
fn is_valid_emphasis_span_fast(line: &str, opening: &EmphasisMarker, closing: &EmphasisMarker) -> bool {
    let content_start = opening.end_pos();
    let content_end = closing.start_pos;

    // Content must exist and not be just whitespace
    if content_end <= content_start {
        return false;
    }

    let content = &line[content_start..content_end];
    if content.trim().is_empty() {
        return false;
    }

    // Quick boundary checks using byte indexing
    let bytes = line.as_bytes();

    // Opening should be at start or after valid character
    let valid_opening = opening.start_pos == 0
        || matches!(
            bytes.get(opening.start_pos.saturating_sub(1)),
            Some(&b' ')
                | Some(&b'\t')
                | Some(&b'(')
                | Some(&b'[')
                | Some(&b'{')
                | Some(&b'"')
                | Some(&b'\'')
                | Some(&b'>')
        );

    // Closing should be at end or before valid character
    let valid_closing = closing.end_pos() >= bytes.len()
        || matches!(
            bytes.get(closing.end_pos()),
            Some(&b' ')
                | Some(&b'\t')
                | Some(&b')')
                | Some(&b']')
                | Some(&b'}')
                | Some(&b'"')
                | Some(&b'\'')
                | Some(&b'.')
                | Some(&b',')
                | Some(&b'!')
                | Some(&b'?')
                | Some(&b';')
                | Some(&b':')
                | Some(&b'<')
        );

    valid_opening && valid_closing && !content.contains('\n')
}

/// Fast validation of emphasis content
#[inline]
fn is_valid_emphasis_content_fast(content: &str) -> bool {
    !content.trim().is_empty()
}

/// Check if line has documentation patterns that should be preserved
pub fn has_doc_patterns(line: &str) -> bool {
    // Check for template/shortcode syntax like {* ... *} used by FastAPI/MkDocs
    // These contain asterisks that are not emphasis markers
    if line.contains("{*") && TEMPLATE_SHORTCODE_PATTERN.is_match(line) {
        return true;
    }

    (QUICK_DOC_CHECK.is_match(line) || QUICK_BOLD_CHECK.is_match(line))
        && (DOC_METADATA_PATTERN.is_match(line) || BOLD_TEXT_PATTERN.is_match(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emphasis_marker_parsing() {
        let markers = find_emphasis_markers("This has *single* and **double** emphasis");
        assert_eq!(markers.len(), 4); // *, *, **, **

        let markers = find_emphasis_markers("*start* and *end*");
        assert_eq!(markers.len(), 4); // *, *, *, *
    }

    #[test]
    fn test_single_emphasis_span_detection() {
        let markers = find_emphasis_markers("This has *valid* emphasis and **strong** too");
        let spans = find_single_emphasis_spans("This has *valid* emphasis and **strong** too", &markers);
        assert_eq!(spans.len(), 1); // Only the single emphasis
        assert_eq!(spans[0].content, "valid");
        assert!(!spans[0].has_leading_space);
        assert!(!spans[0].has_trailing_space);
    }

    #[test]
    fn test_emphasis_with_spaces() {
        let markers = find_emphasis_markers("This has * invalid * emphasis");
        let spans = find_emphasis_spans("This has * invalid * emphasis", &markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, " invalid ");
        assert!(spans[0].has_leading_space);
        assert!(spans[0].has_trailing_space);
    }

    #[test]
    fn test_single_emphasis_rejects_whitespace_flanked_runs() {
        // `find_single_emphasis_spans` powers MD049, which must only see real
        // emphasis. A whitespace-flanked `* ... *` run is not emphasis per
        // CommonMark, so no span is produced.
        let line = "foo * bar * baz";
        let markers = find_emphasis_markers(line);
        let spans = find_single_emphasis_spans(line, &markers);
        assert!(
            spans.is_empty(),
            "whitespace-flanked run must not be a single-emphasis span: {spans:?}"
        );

        // The sibling `find_emphasis_spans` (used by MD037) intentionally keeps
        // the run so MD037 can flag the spaces. Locking this in prevents a
        // future "fix" landing in the shared helper and breaking MD037.
        let md037_spans = find_emphasis_spans(line, &markers);
        assert_eq!(
            md037_spans.len(),
            1,
            "MD037's span finder must still detect the spaced run: {md037_spans:?}"
        );
        assert_eq!(md037_spans[0].content, " bar ");
    }

    #[test]
    fn test_valid_emphasis_ranges() {
        let ranges = |line: &str| {
            let markers = find_emphasis_markers(line);
            find_valid_emphasis_ranges(line, &markers)
        };

        // Plain single and double emphasis yield their full marker-to-marker range.
        assert_eq!(ranges("a *foo* b"), vec![(2, 7)]);
        assert_eq!(ranges("a **foo** b"), vec![(2, 9)]);

        // Valid emphasis spanning an interior whitespace-flanked literal marker.
        assert_eq!(ranges("*foo * bar*"), vec![(0, 11)]);
        assert_eq!(ranges("**foo ** bar**"), vec![(0, 14)]);

        // Whitespace-flanked runs are not valid emphasis - no range.
        assert!(ranges("foo * bar * baz").is_empty());
        assert!(ranges("** spaced **").is_empty());
        // A leading list marker `*` cannot open emphasis.
        assert!(ranges("* item only").is_empty());
    }

    #[test]
    fn test_single_emphasis_spans_literal_marker_inside_emphasis() {
        // CommonMark parses `*foo * bar*` as <em>foo * bar</em>: the inner `*`
        // is whitespace-flanked (a transparent literal), so the outer pair is
        // still emphasis. A naive "skip on inner marker" would miss this.
        let line = "*foo * bar*";
        let markers = find_emphasis_markers(line);
        let spans = find_single_emphasis_spans(line, &markers);
        assert_eq!(spans.len(), 1, "outer emphasis must be detected: {spans:?}");
        assert_eq!(spans[0].content, "foo * bar");

        // `*a *b*` is `*a <em>b</em>`: the inner `*b*` is emphasis, the leading
        // `*` stays literal. Only the inner span is a single-emphasis span.
        let line = "*a *b*";
        let markers = find_emphasis_markers(line);
        let spans = find_single_emphasis_spans(line, &markers);
        assert_eq!(spans.len(), 1, "only inner emphasis: {spans:?}");
        assert_eq!(spans[0].content, "b");
    }

    #[test]
    fn test_mixed_markers() {
        let markers = find_emphasis_markers("This has *asterisk* and _underscore_ emphasis");
        let spans = find_single_emphasis_spans("This has *asterisk* and _underscore_ emphasis", &markers);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].opening.as_char(), '*');
        assert_eq!(spans[1].opening.as_char(), '_');
    }

    #[test]
    fn test_template_shortcode_detection() {
        // FastAPI/MkDocs style template syntax should be detected as doc pattern
        assert!(has_doc_patterns(
            "{* ../../docs_src/cookie_param_models/tutorial001.py hl[9:12,16] *}"
        ));
        assert!(has_doc_patterns(
            "{* ../../docs_src/conditional_openapi/tutorial001.py hl[6,11] *}"
        ));
        // Simple shortcode
        assert!(has_doc_patterns("{* file.py *}"));
        // With path and options
        assert!(has_doc_patterns("{* ../path/to/file.py ln[1-10] *}"));

        // Regular emphasis should NOT match
        assert!(!has_doc_patterns("This has *emphasis* text"));
        assert!(!has_doc_patterns("This has * spaces * in emphasis"));
        // Only opening brace without closing should not match
        assert!(!has_doc_patterns("{* incomplete"));
    }

    #[test]
    fn test_doc_pattern_rejects_spaced_bold_metadata() {
        // Valid bold metadata — should be treated as doc pattern (skip MD037)
        assert!(has_doc_patterns("**Key**: value"));
        assert!(has_doc_patterns("**Name**: another value"));
        assert!(has_doc_patterns("**X**: single char"));
        assert!(has_doc_patterns("* **Key**: list item with bold key"));

        // Broken bold with internal spaces — should NOT be treated as doc pattern
        // so MD037 can flag the spacing issue
        assert!(!has_doc_patterns("** Key**: value"));
        assert!(!has_doc_patterns("**Key **: value"));
        assert!(!has_doc_patterns("** Key **: value"));
        assert!(!has_doc_patterns(
            "** Explicit Import**: Convert markdownlint configs to rumdl format:"
        ));
    }
}
