/// Check if a line is a thematic break (horizontal rule).
///
/// Matches two forms:
/// 1. Compact: 3+ of the same marker (-, *, _) with optional trailing whitespace
///    Examples: `---`, `****`, `_____`
/// 2. Spaced: 3+ single markers separated by whitespace
///    Examples: `- - -`, `* * *`, `_  _  _`
///
/// Per CommonMark, up to 3 spaces of leading indentation are allowed; 4 or
/// more spaces mark an indented code block, so such lines are not thematic
/// breaks.
///
/// Does NOT match multi-char groups with spaces (e.g., `---- ------`)
/// even though CommonMark treats them as valid thematic breaks. This avoids
/// false positives on table separators in unformatted output.
pub fn is_thematic_break(line: &str) -> bool {
    // Strip CRLF but preserve leading indent so the indent check is meaningful.
    let line = line.strip_suffix('\r').unwrap_or(line);
    let leading_spaces = line.bytes().take_while(|b| *b == b' ').count();
    if leading_spaces >= 4 {
        return false;
    }
    let line = line[leading_spaces..].trim_end();
    if line.len() < 3 {
        return false;
    }

    let first = line.as_bytes()[0];
    if first != b'-' && first != b'*' && first != b'_' {
        return false;
    }

    // Check if all non-whitespace chars are the same marker
    let non_ws_count = line.bytes().filter(|&b| !b.is_ascii_whitespace()).count();
    let marker_only = line.bytes().all(|b| b == first || b.is_ascii_whitespace());

    if !marker_only || non_ws_count < 3 {
        return false;
    }

    // Check for spaces between markers (not just trailing)
    let has_internal_space = line.trim_end().as_bytes().iter().any(|&b| b.is_ascii_whitespace());

    if !has_internal_space {
        // Compact form: markers only (with optional trailing whitespace)
        true
    } else {
        // Spaced form: single markers separated by whitespace
        // Each non-whitespace char must be the marker, and markers must not be adjacent
        let mut marker_count = 0;
        let mut prev_was_marker = false;

        for c in line.chars() {
            if c as u32 == first as u32 {
                if prev_was_marker {
                    return false; // Adjacent markers in spaced form
                }
                marker_count += 1;
                prev_was_marker = true;
            } else if c.is_whitespace() {
                prev_was_marker = false;
            } else {
                return false; // Wrong character
            }
        }

        marker_count >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_thematic_breaks() {
        assert!(is_thematic_break("---"));
        assert!(is_thematic_break("***"));
        assert!(is_thematic_break("___"));
    }

    #[test]
    fn test_extended_length() {
        assert!(is_thematic_break("----"));
        assert!(is_thematic_break("****"));
        assert!(is_thematic_break("____"));
        assert!(is_thematic_break("----------"));
        assert!(is_thematic_break(&"-".repeat(200)));
    }

    #[test]
    fn test_spaced_variants() {
        assert!(is_thematic_break("- - -"));
        assert!(is_thematic_break("* * *"));
        assert!(is_thematic_break("_ _ _"));
        assert!(is_thematic_break("- - - -"));
        assert!(is_thematic_break("* * * * *"));
    }

    #[test]
    fn test_multiple_spaces_between() {
        assert!(is_thematic_break("*   *   *"));
        assert!(is_thematic_break("-    -    -"));
        assert!(is_thematic_break("_  _  _"));
    }

    #[test]
    fn test_tabs_as_whitespace() {
        assert!(is_thematic_break("*\t*\t*"));
        assert!(is_thematic_break("-\t-\t-"));
    }

    #[test]
    fn test_trailing_whitespace() {
        assert!(is_thematic_break("--- "));
        assert!(is_thematic_break("*** "));
        assert!(is_thematic_break("___ "));
        assert!(is_thematic_break("- - - "));
        assert!(is_thematic_break("  ---  "));
    }

    #[test]
    fn test_too_few_markers() {
        assert!(!is_thematic_break("--"));
        assert!(!is_thematic_break("**"));
        assert!(!is_thematic_break("__"));
        assert!(!is_thematic_break("- -"));
        assert!(!is_thematic_break("* *"));
        assert!(!is_thematic_break("_ _"));
        assert!(!is_thematic_break("-"));
        assert!(!is_thematic_break("*"));
        assert!(!is_thematic_break("_"));
    }

    #[test]
    fn test_mixed_markers() {
        assert!(!is_thematic_break("-*-"));
        assert!(!is_thematic_break("- * -"));
        assert!(!is_thematic_break("_-_"));
        assert!(!is_thematic_break("*_*"));
    }

    #[test]
    fn test_non_marker_content() {
        assert!(!is_thematic_break("---text"));
        assert!(!is_thematic_break("***text"));
        assert!(!is_thematic_break("- - - text"));
        assert!(!is_thematic_break("-a-b-"));
        assert!(!is_thematic_break("*x*x*"));
        assert!(!is_thematic_break("text"));
    }

    #[test]
    fn test_empty_and_whitespace() {
        assert!(!is_thematic_break(""));
        assert!(!is_thematic_break("   "));
        assert!(!is_thematic_break("\t"));
    }

    #[test]
    fn test_indent_rule_commonmark() {
        // Up to 3 leading spaces are allowed.
        assert!(is_thematic_break("---"));
        assert!(is_thematic_break(" ---"));
        assert!(is_thematic_break("  ---"));
        assert!(is_thematic_break("   ---"));
        assert!(is_thematic_break("   - - -"));

        // 4+ leading spaces are not thematic breaks (indented code block).
        assert!(!is_thematic_break("    ---"));
        assert!(!is_thematic_break("     ---"));
        assert!(!is_thematic_break("    - - -"));
        assert!(!is_thematic_break("        ***"));
    }

    #[test]
    fn test_multi_char_groups_not_matched() {
        // Groups of multiple markers with spaces are NOT treated as spaced HRs
        // to avoid false positives on table separators like kubectl output
        assert!(!is_thematic_break("---- ----"));
        assert!(!is_thematic_break("----  ------  ----"));
        assert!(!is_thematic_break(
            "---- ------            ----  ----              -------"
        ));
        assert!(!is_thematic_break("** **"));
        assert!(!is_thematic_break("__ __"));
    }
}
