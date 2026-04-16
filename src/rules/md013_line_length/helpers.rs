use regex::Regex;
use std::sync::LazyLock;

/// Regex for standalone inline link/image: `[text](url)` or `![alt](url)`
/// Handles escaped brackets in link text and one level of balanced parentheses
/// in URLs (e.g., Wikipedia links like `https://en.wikipedia.org/wiki/Foo_(bar)`).
static INLINE_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^!?\[(?:[^\]\\]|\\.)*\]\((?:[^()]*\([^()]*\))*[^()]*\)$").unwrap());

/// Regex for standalone reference-style link/image: `[text][ref]` or `![alt][ref]`
static REF_LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^!?\[(?:[^\]\\]|\\.)*\]\[[^\]]*\]$").unwrap());

/// Check if a line ends with a hard break (either two spaces or backslash)
///
/// CommonMark supports two formats for hard line breaks:
/// 1. Two or more trailing spaces
/// 2. A backslash at the end of the line
pub(crate) fn has_hard_break(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.ends_with("  ") || line.ends_with('\\')
}

/// Extract list marker and content from a list item
/// Trim trailing whitespace while preserving hard breaks (two trailing spaces or backslash)
///
/// Hard breaks in Markdown can be indicated by:
/// 1. Two trailing spaces before a newline (traditional)
/// 2. A backslash at the end of the line (mdformat style)
pub(crate) fn trim_preserving_hard_break(s: &str) -> String {
    // Strip trailing \r from CRLF line endings first to handle Windows files
    let s = s.strip_suffix('\r').unwrap_or(s);

    // Check for backslash hard break (mdformat style)
    if s.ends_with('\\') {
        // Preserve the backslash exactly as-is
        return s.to_string();
    }

    // Check if there are at least 2 trailing spaces (traditional hard break)
    if s.ends_with("  ") {
        // Find the position where non-space content ends
        let content_end = s.trim_end().len();
        if content_end == 0 {
            // String is all whitespace
            return String::new();
        }
        // Preserve exactly 2 trailing spaces for hard break
        format!("{}  ", &s[..content_end])
    } else {
        // No hard break, just trim all trailing whitespace
        s.trim_end().to_string()
    }
}

/// Split paragraph lines into segments at hard break boundaries.
/// Each segment is a group of lines that can be reflowed together.
/// Lines with hard breaks (ending with 2+ spaces or backslash) form segment boundaries.
///
/// Example:
///   Input:  ["Line 1", "Line 2  ", "Line 3", "Line 4"]
///   Output: [["Line 1", "Line 2  "], ["Line 3", "Line 4"]]
///
/// The first segment includes "Line 2  " which has a hard break at the end.
/// The second segment starts after the hard break.
pub(crate) fn split_into_segments(para_lines: &[String]) -> Vec<Vec<String>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current_segment: Vec<String> = Vec::new();

    for line in para_lines {
        current_segment.push(line.clone());

        // If this line has a hard break, end the current segment
        if has_hard_break(line) {
            segments.push(current_segment.clone());
            current_segment.clear();
        }
    }

    // Add any remaining lines as the final segment
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    segments
}

pub(crate) fn extract_list_marker_and_content(line: &str) -> (String, String) {
    // First, find the leading indentation
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let trimmed = &line[indent_len..];

    // Handle bullet lists
    // Trim trailing whitespace while preserving hard breaks
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let marker_prefix = &bullet[..bullet.len() - 1]; // "-", "*", or "+"
            // Include GFM task list checkboxes in the non-wrappable marker prefix
            for checkbox in ["[ ] ", "[x] ", "[X] "] {
                if let Some(content) = rest.strip_prefix(checkbox) {
                    return (
                        format!("{indent}{marker_prefix} {checkbox}"),
                        trim_preserving_hard_break(content),
                    );
                }
            }
            return (format!("{indent}{bullet}"), trim_preserving_hard_break(rest));
        }
    }

    // Handle numbered lists on trimmed content
    let mut chars = trimmed.chars();
    let mut marker_content = String::new();

    while let Some(c) = chars.next() {
        marker_content.push(c);
        if c == '.' {
            // Check if next char is a space
            if let Some(next) = chars.next()
                && next == ' '
            {
                marker_content.push(next);
                let rest = chars.as_str();
                // Check for GFM task list checkboxes
                for checkbox in ["[ ] ", "[x] ", "[X] "] {
                    if let Some(content) = rest.strip_prefix(checkbox) {
                        return (
                            format!("{indent}{marker_content}{checkbox}"),
                            trim_preserving_hard_break(content),
                        );
                    }
                }
                let content = trim_preserving_hard_break(rest);
                return (format!("{indent}{marker_content}"), content);
            }
            break;
        }
    }

    // Fallback - shouldn't happen if is_list_item was correct
    (String::new(), line.to_string())
}

/// Check if a line is a horizontal rule (thematic break).
///
/// Expects the raw, untrimmed line so the CommonMark indentation rule can be
/// enforced: up to 3 spaces of leading indentation are allowed; 4 or more
/// spaces mark an indented code block.
pub(crate) fn is_horizontal_rule(line: &str) -> bool {
    crate::utils::thematic_break::is_thematic_break(line)
}

pub(crate) fn is_numbered_list_item(line: &str) -> bool {
    let mut chars = line.chars();
    // Must start with a digit
    if !chars.next().is_some_and(char::is_numeric) {
        return false;
    }
    // Can have more digits
    while let Some(c) = chars.next() {
        if c == '.' {
            // After period, must have a space (consistent with extract_list_marker_and_content)
            // "2019." alone is NOT treated as a list item to avoid false positives
            return chars.next() == Some(' ');
        }
        if !c.is_numeric() {
            return false;
        }
    }
    false
}

pub(crate) fn is_list_item(line: &str) -> bool {
    // Bullet lists
    if (line.starts_with('-') || line.starts_with('*') || line.starts_with('+'))
        && line.len() > 1
        && line.chars().nth(1) == Some(' ')
    {
        return true;
    }
    // Numbered lists
    is_numbered_list_item(line)
}

/// Returns true if the content looks like a GitHub Flavored Markdown alert marker.
///
/// GFM alert markers take the form `[!TYPE]` where TYPE is uppercase ASCII letters,
/// optionally followed by content on the same line. They appear as the first line of
/// a blockquote alert block and must not be merged with subsequent content lines.
///
/// Standard types: NOTE, TIP, IMPORTANT, WARNING, CAUTION.
pub(crate) fn is_github_alert_marker(trimmed: &str) -> bool {
    if !trimmed.starts_with("[!") {
        return false;
    }
    let rest = &trimmed[2..];
    let end = rest.find(|c: char| !c.is_ascii_uppercase()).unwrap_or(rest.len());
    end > 0 && rest[end..].starts_with(']')
}

/// Check if a line contains only a link or image (after stripping structural
/// prefixes like blockquote markers, list markers, and emphasis wrappers).
///
/// Lines matching this pattern are exempt from MD013 in non-strict mode because
/// there is no way to shorten them without breaking the markdown structure.
///
/// Exempt patterns include:
/// - `[text](url)` or `![alt](url)` (inline)
/// - `[text][ref]` or `![alt][ref]` (reference-style)
/// - `- [text](url)` (in list items)
/// - `> [text](url)` (in blockquotes)
/// - `**[text](url)**` (with emphasis)
/// - Combinations of the above
pub(crate) fn is_standalone_link_or_image_line(line: &str) -> bool {
    let mut s = line.trim_start();

    // Strip blockquote markers: repeated `> ` or `>` prefixes
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }

    // Strip list marker and task checkbox via shared utility
    if is_list_item(s) {
        let (_, content) = extract_list_marker_and_content(s);
        return is_link_with_optional_emphasis(&content);
    }

    is_link_with_optional_emphasis(s)
}

/// Check if a line consists entirely of HTML structure that cannot be
/// meaningfully shortened. Used to exempt HTML-only lines from MD013 in
/// non-strict mode.
///
/// After stripping blockquote and list markers, a line is exempt if either:
///
/// 1. All non-whitespace content is inside `<...>` tags (e.g., badges,
///    self-closing images, nested tags with no text between them).
/// 2. The line starts with `<` and ends with `>` AND contains URL-bearing
///    attributes (`href=`, `src=`, `srcset=`, `poster=`). This handles
///    `<a href="url">text</a>` — functionally identical to `[text](url)`
///    which is already exempt as a standalone link.
///
/// Handles quoted attribute values that may contain `>` characters.
///
/// Examples that return true:
/// - `<a href="..."><img alt="badge" src="..."/></a>` (all content in tags)
/// - `<img src="..." alt="..." width="..." height="..."/>` (self-closing)
/// - `<a href="...">link text</a>` (HTML link, consistent with markdown link exemption)
/// - `<video src="..." poster="..." controls></video>` (media with URL attrs)
///
/// Examples that return false:
/// - `Some text <a href="...">link</a>` (text before tags)
/// - `<b>very long bold text</b>` (formatting tag without URL attributes)
/// - `Plain text without any HTML`
pub(crate) fn is_html_only_line(line: &str) -> bool {
    let mut s = line.trim_start();

    // Strip blockquote markers
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }

    // Strip list markers
    if is_list_item(s) {
        let (_, content) = extract_list_marker_and_content(s);
        return is_html_only_content(&content);
    }

    is_html_only_content(s)
}

/// Combined check for HTML-only content.
fn is_html_only_content(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || !s.starts_with('<') {
        return false;
    }

    // Check 1: All non-whitespace content is inside HTML tags.
    // Covers badges, self-closing images, nested tags with no text between them.
    if is_content_all_html_tags(s) {
        return true;
    }

    // Check 2: Line is entirely wrapped in HTML (starts with <, ends with >)
    // and contains URL-bearing attributes. This makes <a href="url">text</a>
    // consistent with the existing [text](url) standalone link exemption.
    if s.ends_with('>') && (s.contains("href=") || s.contains("src=") || s.contains("srcset=") || s.contains("poster="))
    {
        return true;
    }

    false
}

/// Returns true if all non-whitespace content is inside `<...>` delimiters.
fn is_content_all_html_tags(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || !s.starts_with('<') {
        return false;
    }

    let mut in_tag = false;
    let mut quote_char: Option<char> = None;
    let mut found_complete_tag = false;

    for c in s.chars() {
        if let Some(q) = quote_char {
            if c == q {
                quote_char = None;
            }
        } else if in_tag {
            match c {
                '"' | '\'' => quote_char = Some(c),
                '>' => {
                    in_tag = false;
                    found_complete_tag = true;
                }
                _ => {}
            }
        } else if c == '<' {
            in_tag = true;
        } else if !c.is_whitespace() {
            return false;
        }
    }

    found_complete_tag
}

/// Check if content (after stripping list/blockquote markers) is a standalone link,
/// optionally wrapped in emphasis.
fn is_link_with_optional_emphasis(s: &str) -> bool {
    let mut s = s.trim();
    if s.is_empty() {
        return false;
    }

    // Strip emphasis wrappers (up to 3 chars: *, **, ***, _, __, ___)
    let emphasis_chars: &[char] = &['*', '_'];
    let leading_emphasis = s.chars().take_while(|c| emphasis_chars.contains(c)).count();
    if leading_emphasis > 0 && leading_emphasis <= 3 {
        let trimmed_end = s.trim_end();
        let trailing_emphasis = trimmed_end
            .chars()
            .rev()
            .take_while(|c| emphasis_chars.contains(c))
            .count();
        if trailing_emphasis == leading_emphasis {
            s = &s[leading_emphasis..trimmed_end.len() - trailing_emphasis];
        }
    }

    let s = s.trim();
    if s.is_empty() {
        return false;
    }

    INLINE_LINK_RE.is_match(s) || REF_LINK_RE.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test for issue #336: "2019." alone should NOT be treated as a list item
    /// This prevents convergence failures when a year appears at the end of a sentence
    #[test]
    fn test_numbered_list_item_requires_space_after_period() {
        // Valid list items (have space after period)
        assert!(is_numbered_list_item("1. Item"));
        assert!(is_numbered_list_item("10. Item"));
        assert!(is_numbered_list_item("99. Long number"));
        assert!(is_numbered_list_item("123. Triple digits"));

        // Invalid: number+period without space (like years at end of sentences)
        // These should NOT be treated as list items to avoid reflow issues
        assert!(!is_numbered_list_item("2019."));
        assert!(!is_numbered_list_item("1999."));
        assert!(!is_numbered_list_item("2023."));
        assert!(!is_numbered_list_item("1.")); // Even single digit without space

        // Invalid: not starting with digit
        assert!(!is_numbered_list_item("a. Item"));
        assert!(!is_numbered_list_item(". Item"));
        assert!(!is_numbered_list_item("Item"));

        // Invalid: no period
        assert!(!is_numbered_list_item("1 Item"));
        assert!(!is_numbered_list_item("123"));
    }

    #[test]
    fn test_extract_list_marker_task_checkboxes() {
        // Unchecked task item: checkbox becomes part of the marker prefix
        assert_eq!(
            extract_list_marker_and_content("- [ ] some content"),
            ("- [ ] ".to_string(), "some content".to_string())
        );
        // Checked task item (lowercase x)
        assert_eq!(
            extract_list_marker_and_content("- [x] done item"),
            ("- [x] ".to_string(), "done item".to_string())
        );
        // Checked task item (uppercase X)
        assert_eq!(
            extract_list_marker_and_content("- [X] also done"),
            ("- [X] ".to_string(), "also done".to_string())
        );
        // Other bullet markers preserve checkbox
        assert_eq!(
            extract_list_marker_and_content("* [ ] star task"),
            ("* [ ] ".to_string(), "star task".to_string())
        );
        assert_eq!(
            extract_list_marker_and_content("+ [ ] plus task"),
            ("+ [ ] ".to_string(), "plus task".to_string())
        );
        // Indented task item
        assert_eq!(
            extract_list_marker_and_content("  - [ ] indented task"),
            ("  - [ ] ".to_string(), "indented task".to_string())
        );
        // Regular bullet (no checkbox) is unchanged
        assert_eq!(
            extract_list_marker_and_content("- regular item"),
            ("- ".to_string(), "regular item".to_string())
        );
        // Ordered list with task checkboxes
        assert_eq!(
            extract_list_marker_and_content("1. [ ] unchecked ordered"),
            ("1. [ ] ".to_string(), "unchecked ordered".to_string())
        );
        assert_eq!(
            extract_list_marker_and_content("1. [x] checked ordered"),
            ("1. [x] ".to_string(), "checked ordered".to_string())
        );
        assert_eq!(
            extract_list_marker_and_content("1. [X] checked upper ordered"),
            ("1. [X] ".to_string(), "checked upper ordered".to_string())
        );
        assert_eq!(
            extract_list_marker_and_content("99. [x] multi-digit ordered"),
            ("99. [x] ".to_string(), "multi-digit ordered".to_string())
        );
    }

    #[test]
    fn test_is_horizontal_rule_commonmark_indent() {
        // Up to 3 spaces of leading indent is allowed (CommonMark thematic break).
        assert!(is_horizontal_rule("---"));
        assert!(is_horizontal_rule(" ---"));
        assert!(is_horizontal_rule("  ---"));
        assert!(is_horizontal_rule("   ---"));
        assert!(is_horizontal_rule("   ***"));
        assert!(is_horizontal_rule("   - - -"));

        // 4+ spaces of leading indent is an indented code block, not a thematic break.
        assert!(!is_horizontal_rule("    ---"));
        assert!(!is_horizontal_rule("     ---"));
        assert!(!is_horizontal_rule("        ***"));
        assert!(!is_horizontal_rule("    - - -"));

        // Trailing whitespace is still allowed.
        assert!(is_horizontal_rule("---   "));
        assert!(is_horizontal_rule("  ---  "));

        // Basic shapes still match.
        assert!(is_horizontal_rule("----"));
        assert!(is_horizontal_rule("***"));
        assert!(is_horizontal_rule("___"));
        assert!(is_horizontal_rule("- - -"));
        assert!(!is_horizontal_rule("--"));
        assert!(!is_horizontal_rule("text"));
        assert!(!is_horizontal_rule(""));
    }

    #[test]
    fn test_is_list_item_bullet_and_numbered() {
        // Bullet list items
        assert!(is_list_item("- Item"));
        assert!(is_list_item("* Item"));
        assert!(is_list_item("+ Item"));

        // Bullet without space = not a list item
        assert!(!is_list_item("-Item"));
        assert!(!is_list_item("*Item"));

        // Numbered list items
        assert!(is_list_item("1. Item"));
        assert!(is_list_item("99. Item"));

        // Year at end of sentence = not a list item
        assert!(!is_list_item("2019."));
    }

    #[test]
    fn test_is_github_alert_marker() {
        // Standard GFM alert types
        assert!(is_github_alert_marker("[!NOTE]"));
        assert!(is_github_alert_marker("[!TIP]"));
        assert!(is_github_alert_marker("[!WARNING]"));
        assert!(is_github_alert_marker("[!CAUTION]"));
        assert!(is_github_alert_marker("[!IMPORTANT]"));

        // Alert with trailing content on the same line
        assert!(is_github_alert_marker("[!NOTE] Some inline content here"));
        assert!(is_github_alert_marker("[!WARNING] Do not do this"));

        // Custom uppercase type (not standard but structurally valid)
        assert!(is_github_alert_marker("[!CUSTOM]"));

        // Not an alert marker
        assert!(!is_github_alert_marker("[!note]")); // lowercase
        assert!(!is_github_alert_marker("[Note]")); // missing !
        assert!(!is_github_alert_marker("[!]")); // empty type
        assert!(!is_github_alert_marker("[!NOTE")); // missing closing bracket
        assert!(!is_github_alert_marker("NOTE")); // no brackets
        assert!(!is_github_alert_marker("[link]: url")); // link definition
        assert!(!is_github_alert_marker("Some text [!NOTE]")); // not at start
    }

    #[test]
    fn test_standalone_link_bare() {
        // Bare inline link
        assert!(is_standalone_link_or_image_line("[text](https://example.com)"));
        assert!(is_standalone_link_or_image_line(
            "[long title here](https://example.com/path)"
        ));
        // With leading whitespace
        assert!(is_standalone_link_or_image_line("  [text](https://example.com)"));
        // URL with balanced parentheses (Wikipedia-style)
        assert!(is_standalone_link_or_image_line(
            "[Rust](https://en.wikipedia.org/wiki/Rust_(programming_language))"
        ));
        assert!(is_standalone_link_or_image_line("[A](https://example.com/A_(B)_C)"));
    }

    #[test]
    fn test_standalone_image() {
        assert!(is_standalone_link_or_image_line(
            "![alt text](https://example.com/img.png)"
        ));
        assert!(is_standalone_link_or_image_line("  ![alt](url)"));
    }

    #[test]
    fn test_standalone_link_in_list() {
        // Bullet list items
        assert!(is_standalone_link_or_image_line("- [text](url)"));
        assert!(is_standalone_link_or_image_line("* [text](url)"));
        assert!(is_standalone_link_or_image_line("+ [text](url)"));
        // Ordered list
        assert!(is_standalone_link_or_image_line("1. [text](url)"));
        assert!(is_standalone_link_or_image_line("99. [text](url)"));
        // Indented list item
        assert!(is_standalone_link_or_image_line("  - [text](url)"));
        // Task list with link (bullet and ordered)
        assert!(is_standalone_link_or_image_line("- [ ] [text](url)"));
        assert!(is_standalone_link_or_image_line("- [x] [text](url)"));
        assert!(is_standalone_link_or_image_line("1. [x] [text](url)"));
        assert!(is_standalone_link_or_image_line("1. [ ] [text](url)"));
    }

    #[test]
    fn test_standalone_link_in_blockquote() {
        assert!(is_standalone_link_or_image_line("> [text](url)"));
        assert!(is_standalone_link_or_image_line(">> [text](url)"));
        assert!(is_standalone_link_or_image_line("> > [text](url)"));
    }

    #[test]
    fn test_standalone_link_with_emphasis() {
        assert!(is_standalone_link_or_image_line("**[text](url)**"));
        assert!(is_standalone_link_or_image_line("*[text](url)*"));
        assert!(is_standalone_link_or_image_line("__[text](url)__"));
        assert!(is_standalone_link_or_image_line("_[text](url)_"));
        assert!(is_standalone_link_or_image_line("***[text](url)***"));
        // List + emphasis
        assert!(is_standalone_link_or_image_line("- **[text](url)**"));
    }

    #[test]
    fn test_standalone_link_reference_style() {
        assert!(is_standalone_link_or_image_line("[text][ref]"));
        assert!(is_standalone_link_or_image_line("![alt][ref]"));
        assert!(is_standalone_link_or_image_line("- [text][ref]"));
        assert!(is_standalone_link_or_image_line("> [text][ref]"));
        // Collapsed reference link
        assert!(is_standalone_link_or_image_line("[text][]"));
        assert!(is_standalone_link_or_image_line("- [text][]"));
    }

    #[test]
    fn test_not_standalone_link() {
        // Has text before the link
        assert!(!is_standalone_link_or_image_line("Some text [link](url)"));
        assert!(!is_standalone_link_or_image_line("See [link](url) for details"));
        // Plain text (no link)
        assert!(!is_standalone_link_or_image_line("Just some long text"));
        // Empty
        assert!(!is_standalone_link_or_image_line(""));
        assert!(!is_standalone_link_or_image_line("   "));
        // Multiple links
        assert!(!is_standalone_link_or_image_line("[link1](url1) [link2](url2)"));
        // Link followed by text
        assert!(!is_standalone_link_or_image_line("[link](url) extra text"));
    }

    // --- is_html_only_line tests ---

    #[test]
    fn test_html_only_badge_line() {
        // The reported case: badge with nested <a> and <img>
        assert!(is_html_only_line(
            r#"<a href="https://dotfyle.com/plugins/chrisgrieser/nvim-rulebook"><img alt="badge" src="https://dotfyle.com/plugins/chrisgrieser/nvim-rulebook/shield"/></a>"#
        ));
    }

    #[test]
    fn test_html_only_self_closing_tags() {
        assert!(is_html_only_line(
            r#"<img src="https://example.com/image.png" alt="screenshot" width="800" height="600"/>"#
        ));
        assert!(is_html_only_line(r#"<br/>"#));
        assert!(is_html_only_line(r#"<hr />"#));
    }

    #[test]
    fn test_html_only_multiple_tags() {
        // Multiple adjacent tags with no text between them
        assert!(is_html_only_line(r#"<img src="a.png"/><img src="b.png"/>"#));
        assert!(is_html_only_line(r#"<br/><br/><br/>"#));
    }

    #[test]
    fn test_html_only_empty_element() {
        // Tags with no content between opening and closing
        assert!(is_html_only_line(r#"<video src="long-url.mp4" controls></video>"#));
        assert!(is_html_only_line(r#"<div></div>"#));
    }

    #[test]
    fn test_html_only_with_whitespace_between_tags() {
        assert!(is_html_only_line(r#"<img src="a.png"/> <img src="b.png"/>"#));
    }

    #[test]
    fn test_html_only_quoted_angle_brackets() {
        // Attribute value containing > should not break parsing
        assert!(is_html_only_line(r#"<img alt="a > b" src="test.png"/>"#));
        assert!(is_html_only_line(r#"<img alt='a > b' src="test.png"/>"#));
    }

    #[test]
    fn test_html_only_in_blockquote() {
        assert!(is_html_only_line(r#"> <img src="long-url.png" alt="screenshot"/>"#));
        assert!(is_html_only_line(r#">> <a href="url"><img src="img"/></a>"#));
    }

    #[test]
    fn test_html_only_in_list() {
        assert!(is_html_only_line(r#"- <img src="long-url.png" alt="screenshot"/>"#));
        assert!(is_html_only_line(r#"1. <a href="url"><img src="img"/></a>"#));
        assert!(is_html_only_line(r#"  - <img src="long-url.png"/>"#));
    }

    #[test]
    fn test_html_only_link_with_text_and_url() {
        // <a href="url">text</a> is functionally identical to [text](url)
        // which is already exempt — so this should also be exempt
        assert!(is_html_only_line(
            r#"<a href="https://example.com/very-long-path">Click here for details</a>"#
        ));
        // With target attribute (reason to use HTML over markdown)
        assert!(is_html_only_line(
            r#"<a href="https://example.com/very-long-path" target="_blank">Click here for details</a>"#
        ));
        // Multiple URL attributes
        assert!(is_html_only_line(
            r#"<a href="https://example.com/path"><img src="https://example.com/badge.svg" alt="status"/></a>"#
        ));
    }

    #[test]
    fn test_not_html_only_text_before_tags() {
        assert!(!is_html_only_line(r#"Click here: <a href="url">link</a>"#));
        assert!(!is_html_only_line(r#"See <img src="url"/> for details"#));
    }

    #[test]
    fn test_not_html_only_text_after_tags() {
        assert!(!is_html_only_line(r#"<a href="url">link</a> - click above"#));
        assert!(!is_html_only_line(r#"<img src="url"/> is an image"#));
    }

    #[test]
    fn test_not_html_only_formatting_tags_without_urls() {
        // Formatting tags without URL attributes should NOT be exempt —
        // the line is long because of text content, not URLs
        assert!(!is_html_only_line(
            r#"<b>This is very long bold text that exceeds the line length limit</b>"#
        ));
        assert!(!is_html_only_line(
            r#"<p>This is a very long paragraph written in HTML tags for some reason</p>"#
        ));
        assert!(!is_html_only_line(
            r#"<span style="color:red">Some styled text that is quite long</span>"#
        ));
        assert!(!is_html_only_line(
            r#"<em>Emphasized text that goes on and on and on</em>"#
        ));
        // Multiple formatting tags with text between them
        assert!(!is_html_only_line(r#"<b>bold</b> and <i>italic</i>"#));
    }

    #[test]
    fn test_not_html_only_plain_text() {
        assert!(!is_html_only_line("Just some long text without any HTML"));
        assert!(!is_html_only_line(""));
        assert!(!is_html_only_line("   "));
    }

    #[test]
    fn test_not_html_only_incomplete_tag() {
        // Unclosed tag with no complete tag
        assert!(!is_html_only_line("<unclosed"));
        // Doesn't end with > (unclosed outer element)
        assert!(!is_html_only_line(r#"<a href="url">text"#));
    }

    #[test]
    fn test_html_only_comment() {
        // Simple HTML comments (no > inside) are detected as all-inside-tags
        assert!(is_html_only_line(
            "<!-- this is a long HTML comment that spans many characters -->"
        ));
    }

    #[test]
    fn test_html_only_media_elements() {
        assert!(is_html_only_line(
            r#"<video src="https://example.com/very-long-path/video.mp4" poster="https://example.com/thumb.jpg" controls></video>"#
        ));
        assert!(is_html_only_line(
            r#"<audio src="https://example.com/very-long-path/audio.mp3" controls></audio>"#
        ));
        assert!(is_html_only_line(
            r#"<source srcset="https://example.com/image-large.webp" media="(min-width: 800px)"/>"#
        ));
        assert!(is_html_only_line(
            r#"<picture><source srcset="large.webp"/><img src="fallback.png"/></picture>"#
        ));
    }

    #[test]
    fn test_html_only_in_list_with_url_text() {
        // List item containing an HTML link with text — should be exempt
        assert!(is_html_only_line(
            r#"- <a href="https://example.com/very-long-path">documentation link</a>"#
        ));
    }

    #[test]
    fn test_html_only_in_blockquote_with_url_text() {
        assert!(is_html_only_line(
            r#"> <a href="https://example.com/very-long-path">documentation link</a>"#
        ));
    }
}
