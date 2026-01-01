use regex::Regex;
use std::sync::LazyLock;

// Better detection of inline code with support for multiple backticks
static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap());

// Inline math pattern - matches both $...$ and $$...$$ syntax
// The pattern allows zero or more characters between delimiters to handle empty math spans
static INLINE_MATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$[^$]*\$\$|\$[^$\n]*\$").unwrap());

// List markers pattern - used to avoid confusion with emphasis
static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[*+-]\s+").unwrap());

// Documentation style patterns
static DOC_METADATA_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\*?\s*\*\*[^*]+\*\*\s*:").unwrap());

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
pub fn find_single_emphasis_spans(line: &str, markers: Vec<EmphasisMarker>) -> Vec<EmphasisSpan> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] || markers[i].count != 1 {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check - only single emphasis
            if closing.marker_type == opening.marker_type && closing.count == 1 {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // Quick check for crossing markers
                        let crosses_markers = markers[i + 1..j]
                            .iter()
                            .any(|marker| marker.marker_type == opening.marker_type && marker.count == 1);

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

/// Optimized emphasis span finding with reduced complexity (includes both single and strong)
pub fn find_emphasis_spans(line: &str, markers: Vec<EmphasisMarker>) -> Vec<EmphasisSpan> {
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

/// Fast validation of emphasis span context
#[inline]
pub fn is_valid_emphasis_span_fast(line: &str, opening: &EmphasisMarker, closing: &EmphasisMarker) -> bool {
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
pub fn is_valid_emphasis_content_fast(content: &str) -> bool {
    !content.trim().is_empty()
}

/// Check if a line should be treated as a list item vs emphasis
pub fn is_likely_list_line(line: &str) -> bool {
    LIST_MARKER.is_match(line)
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
        let spans = find_single_emphasis_spans("This has *valid* emphasis and **strong** too", markers);
        assert_eq!(spans.len(), 1); // Only the single emphasis
        assert_eq!(spans[0].content, "valid");
        assert!(!spans[0].has_leading_space);
        assert!(!spans[0].has_trailing_space);
    }

    #[test]
    fn test_emphasis_with_spaces() {
        let markers = find_emphasis_markers("This has * invalid * emphasis");
        let spans = find_emphasis_spans("This has * invalid * emphasis", markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, " invalid ");
        assert!(spans[0].has_leading_space);
        assert!(spans[0].has_trailing_space);
    }

    #[test]
    fn test_mixed_markers() {
        let markers = find_emphasis_markers("This has *asterisk* and _underscore_ emphasis");
        let spans = find_single_emphasis_spans("This has *asterisk* and _underscore_ emphasis", markers);
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
}
