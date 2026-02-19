//! Blockquote-related utilities for rumdl.
//!
//! Provides functions for working with blockquote-prefixed lines, including
//! calculating effective indentation within blockquote context.

/// Parsed blockquote prefix components.
///
/// This parser supports both compact (`>> text`) and spaced (`> > text`) nesting.
/// It preserves at most one optional space/tab after the final marker in `prefix`;
/// additional whitespace is left in `content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedBlockquotePrefix<'a> {
    /// Leading indentation before the first marker.
    pub indent: &'a str,
    /// Prefix up to (and including) the optional single space/tab after final `>`.
    pub prefix: &'a str,
    /// Content after `prefix` (may begin with additional spaces/tabs).
    pub content: &'a str,
    /// Number of blockquote markers at the start of the line.
    pub nesting_level: usize,
    /// All whitespace after the final `>` (including the optional first space/tab).
    pub spaces_after_marker: &'a str,
}

#[inline]
fn is_space_or_tab(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// Parse a blockquote prefix from a line.
///
/// Returns `None` when the line does not start (after leading indentation) with
/// a blockquote marker.
#[inline]
pub fn parse_blockquote_prefix(line: &str) -> Option<ParsedBlockquotePrefix<'_>> {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Leading indentation before the first marker.
    while pos < bytes.len() && is_space_or_tab(bytes[pos]) {
        pos += 1;
    }
    let indent_end = pos;

    if pos >= bytes.len() || bytes[pos] != b'>' {
        return None;
    }

    let mut nesting_level = 0;
    let mut prefix_end = pos;
    let mut spaces_after_marker_start = pos;
    let mut spaces_after_marker_end = pos;

    loop {
        if pos >= bytes.len() || bytes[pos] != b'>' {
            break;
        }

        nesting_level += 1;
        pos += 1; // past '>'
        let marker_end = pos;

        // Include at most one optional space/tab in the preserved prefix.
        if pos < bytes.len() && is_space_or_tab(bytes[pos]) {
            pos += 1;
        }
        let content_start_candidate = pos;

        // Consume any additional whitespace.
        while pos < bytes.len() && is_space_or_tab(bytes[pos]) {
            pos += 1;
        }

        // If another marker follows, all consumed whitespace belongs to nesting.
        if pos < bytes.len() && bytes[pos] == b'>' {
            continue;
        }

        // Final marker.
        prefix_end = content_start_candidate;
        spaces_after_marker_start = marker_end;
        spaces_after_marker_end = pos;
        break;
    }

    Some(ParsedBlockquotePrefix {
        indent: &line[..indent_end],
        prefix: &line[..prefix_end],
        content: &line[prefix_end..],
        nesting_level,
        spaces_after_marker: &line[spaces_after_marker_start..spaces_after_marker_end],
    })
}

/// Calculate the effective indentation of a line within a blockquote context.
///
/// For lines inside blockquotes, the "raw" leading whitespace (before `>`) is always 0,
/// but the semantically meaningful indent is the whitespace *after* the blockquote markers.
///
/// # Arguments
///
/// * `line_content` - The full line content including any blockquote markers
/// * `expected_bq_level` - The blockquote nesting level to match (0 for no blockquote)
/// * `fallback_indent` - The indent to return if blockquote levels don't match or if
///   `expected_bq_level` is 0
///
/// # Returns
///
/// The effective indentation:
/// - If `expected_bq_level` is 0: returns `fallback_indent`
/// - If line's blockquote level matches `expected_bq_level`: returns indent after stripping markers
/// - If blockquote levels don't match: returns `fallback_indent`
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::effective_indent_in_blockquote;
///
/// // Regular line (no blockquote context)
/// assert_eq!(effective_indent_in_blockquote("   text", 0, 3), 3);
///
/// // Blockquote line with 2 spaces after marker
/// assert_eq!(effective_indent_in_blockquote(">  text", 1, 0), 2);
///
/// // Nested blockquote with 3 spaces after markers
/// assert_eq!(effective_indent_in_blockquote("> >   text", 2, 0), 3);
///
/// // Mismatched blockquote level - returns fallback
/// assert_eq!(effective_indent_in_blockquote("> text", 2, 5), 5);
/// ```
pub fn effective_indent_in_blockquote(line_content: &str, expected_bq_level: usize, fallback_indent: usize) -> usize {
    if expected_bq_level == 0 {
        return fallback_indent;
    }

    // Count blockquote markers at the start of the line
    // Markers can be separated by whitespace: "> > text" or ">> text"
    let line_bq_level = line_content
        .chars()
        .take_while(|c| *c == '>' || c.is_whitespace())
        .filter(|&c| c == '>')
        .count();

    if line_bq_level != expected_bq_level {
        return fallback_indent;
    }

    // Strip blockquote markers and compute indent within the blockquote context
    let mut pos = 0;
    let mut found_markers = 0;
    for c in line_content.chars() {
        pos += c.len_utf8();
        if c == '>' {
            found_markers += 1;
            if found_markers == line_bq_level {
                // Skip optional space after final >
                if line_content.get(pos..pos + 1) == Some(" ") {
                    pos += 1;
                }
                break;
            }
        }
    }

    let after_bq = &line_content[pos..];
    after_bq.len() - after_bq.trim_start().len()
}

/// Count the number of blockquote markers (`>`) at the start of a line.
///
/// Handles both compact (`>>text`) and spaced (`> > text`) blockquote syntax.
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::count_blockquote_level;
///
/// assert_eq!(count_blockquote_level("regular text"), 0);
/// assert_eq!(count_blockquote_level("> quoted"), 1);
/// assert_eq!(count_blockquote_level(">> nested"), 2);
/// assert_eq!(count_blockquote_level("> > spaced nested"), 2);
/// ```
pub fn count_blockquote_level(line_content: &str) -> usize {
    line_content
        .chars()
        .take_while(|c| *c == '>' || c.is_whitespace())
        .filter(|&c| c == '>')
        .count()
}

/// Extract the content after blockquote markers.
///
/// Returns the portion of the line after all blockquote markers and the
/// optional space following the last marker.
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::content_after_blockquote;
///
/// assert_eq!(content_after_blockquote("> text", 1), "text");
/// assert_eq!(content_after_blockquote(">  indented", 1), " indented");
/// assert_eq!(content_after_blockquote("> > nested", 2), "nested");
/// assert_eq!(content_after_blockquote("no quote", 0), "no quote");
/// ```
pub fn content_after_blockquote(line_content: &str, expected_bq_level: usize) -> &str {
    if expected_bq_level == 0 {
        return line_content;
    }

    // First, verify the line has the expected blockquote level
    let actual_level = count_blockquote_level(line_content);
    if actual_level != expected_bq_level {
        return line_content;
    }

    let mut pos = 0;
    let mut found_markers = 0;
    for c in line_content.chars() {
        pos += c.len_utf8();
        if c == '>' {
            found_markers += 1;
            if found_markers == expected_bq_level {
                // Skip optional space after final >
                if line_content.get(pos..pos + 1) == Some(" ") {
                    pos += 1;
                }
                break;
            }
        }
    }

    &line_content[pos..]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // parse_blockquote_prefix tests
    // ==========================================================================

    #[test]
    fn test_parse_blockquote_prefix_compact_nested() {
        let parsed = parse_blockquote_prefix(">> text").expect("should parse compact nested blockquote");
        assert_eq!(parsed.indent, "");
        assert_eq!(parsed.prefix, ">> ");
        assert_eq!(parsed.content, "text");
        assert_eq!(parsed.nesting_level, 2);
        assert_eq!(parsed.spaces_after_marker, " ");
    }

    #[test]
    fn test_parse_blockquote_prefix_spaced_nested() {
        let parsed = parse_blockquote_prefix("> >  text").expect("should parse spaced nested blockquote");
        assert_eq!(parsed.indent, "");
        assert_eq!(parsed.prefix, "> > ");
        assert_eq!(parsed.content, " text");
        assert_eq!(parsed.nesting_level, 2);
        assert_eq!(parsed.spaces_after_marker, "  ");
    }

    #[test]
    fn test_parse_blockquote_prefix_with_indent() {
        let parsed = parse_blockquote_prefix("   > quote").expect("should parse indented blockquote");
        assert_eq!(parsed.indent, "   ");
        assert_eq!(parsed.prefix, "   > ");
        assert_eq!(parsed.content, "quote");
        assert_eq!(parsed.nesting_level, 1);
        assert_eq!(parsed.spaces_after_marker, " ");
    }

    #[test]
    fn test_parse_blockquote_prefix_non_blockquote() {
        assert!(parse_blockquote_prefix("plain text").is_none());
        assert!(parse_blockquote_prefix("  plain text").is_none());
    }

    // ==========================================================================
    // effective_indent_in_blockquote tests
    // ==========================================================================

    #[test]
    fn test_effective_indent_no_blockquote_context() {
        // When expected_bq_level is 0, always return fallback
        assert_eq!(effective_indent_in_blockquote("text", 0, 0), 0);
        assert_eq!(effective_indent_in_blockquote("   text", 0, 3), 3);
        assert_eq!(effective_indent_in_blockquote("> text", 0, 5), 5);
    }

    #[test]
    fn test_effective_indent_single_level_blockquote() {
        // Single > with various indents after
        assert_eq!(effective_indent_in_blockquote("> text", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">  text", 1, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">   text", 1, 99), 2);
        assert_eq!(effective_indent_in_blockquote(">    text", 1, 99), 3);
    }

    #[test]
    fn test_effective_indent_no_space_after_marker() {
        // >text (no space after >) - should have 0 effective indent
        assert_eq!(effective_indent_in_blockquote(">text", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>text", 2, 99), 0);
    }

    #[test]
    fn test_effective_indent_nested_blockquote_compact() {
        // Compact nested: >>text, >> text, >>  text
        assert_eq!(effective_indent_in_blockquote(">> text", 2, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>  text", 2, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">>   text", 2, 99), 2);
    }

    #[test]
    fn test_effective_indent_nested_blockquote_spaced() {
        // Spaced nested: > > text, > >  text
        assert_eq!(effective_indent_in_blockquote("> > text", 2, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> >  text", 2, 99), 1);
        assert_eq!(effective_indent_in_blockquote("> >   text", 2, 99), 2);
    }

    #[test]
    fn test_effective_indent_mismatched_level() {
        // Line has different blockquote level than expected - return fallback
        assert_eq!(effective_indent_in_blockquote("> text", 2, 42), 42);
        assert_eq!(effective_indent_in_blockquote(">> text", 1, 42), 42);
        assert_eq!(effective_indent_in_blockquote("text", 1, 42), 42);
    }

    #[test]
    fn test_effective_indent_empty_blockquote() {
        // Empty blockquote lines
        assert_eq!(effective_indent_in_blockquote(">", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> ", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">  ", 1, 99), 1);
    }

    #[test]
    fn test_effective_indent_issue_268_case() {
        // The exact pattern from issue #268:
        // ">   text" where we expect 2 spaces of indent (list continuation)
        assert_eq!(effective_indent_in_blockquote(">   Opening the app", 1, 0), 2);
        assert_eq!(
            effective_indent_in_blockquote(">   [**See preview here!**](https://example.com)", 1, 0),
            2
        );
    }

    #[test]
    fn test_effective_indent_triple_nested() {
        // Triple nested blockquotes
        assert_eq!(effective_indent_in_blockquote("> > > text", 3, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> > >  text", 3, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">>> text", 3, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>>  text", 3, 99), 1);
    }

    // ==========================================================================
    // count_blockquote_level tests
    // ==========================================================================

    #[test]
    fn test_count_blockquote_level_none() {
        assert_eq!(count_blockquote_level("regular text"), 0);
        assert_eq!(count_blockquote_level("   indented text"), 0);
        assert_eq!(count_blockquote_level(""), 0);
    }

    #[test]
    fn test_count_blockquote_level_single() {
        assert_eq!(count_blockquote_level("> text"), 1);
        assert_eq!(count_blockquote_level(">text"), 1);
        assert_eq!(count_blockquote_level(">"), 1);
    }

    #[test]
    fn test_count_blockquote_level_nested() {
        assert_eq!(count_blockquote_level(">> text"), 2);
        assert_eq!(count_blockquote_level("> > text"), 2);
        assert_eq!(count_blockquote_level(">>> text"), 3);
        assert_eq!(count_blockquote_level("> > > text"), 3);
    }

    // ==========================================================================
    // content_after_blockquote tests
    // ==========================================================================

    #[test]
    fn test_content_after_blockquote_no_quote() {
        assert_eq!(content_after_blockquote("text", 0), "text");
        assert_eq!(content_after_blockquote("   indented", 0), "   indented");
    }

    #[test]
    fn test_content_after_blockquote_single() {
        assert_eq!(content_after_blockquote("> text", 1), "text");
        assert_eq!(content_after_blockquote(">text", 1), "text");
        assert_eq!(content_after_blockquote(">  indented", 1), " indented");
    }

    #[test]
    fn test_content_after_blockquote_nested() {
        assert_eq!(content_after_blockquote(">> text", 2), "text");
        assert_eq!(content_after_blockquote("> > text", 2), "text");
        assert_eq!(content_after_blockquote("> >  indented", 2), " indented");
    }

    #[test]
    fn test_content_after_blockquote_mismatched_level() {
        // If level doesn't match, return original
        assert_eq!(content_after_blockquote("> text", 2), "> text");
        assert_eq!(content_after_blockquote(">> text", 1), ">> text");
    }
}
