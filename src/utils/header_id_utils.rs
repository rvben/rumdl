//! Utilities for extracting custom header IDs from various Markdown flavors
//!
//! This module supports multiple syntax formats for custom header IDs:
//!
//! ## Kramdown Format
//! - `{#custom-id}` - Simple ID without colon
//! - Example: `# Header {#my-id}`
//!
//! ## Python-markdown attr-list Format
//! - `{:#custom-id}` - ID with colon, no spaces
//! - `{: #custom-id}` - ID with colon and spaces
//! - `{: #custom-id .class}` - ID with classes
//! - `{: #custom-id .class data="value"}` - ID with full attributes
//! - Example: `# Header {: #my-id .highlight}`
//!
//! ## Position Support
//! - Inline: `# Header {#id}` (all formats)
//! - Next-line: Jekyll/kramdown style where attr-list appears on the line after the header
//!   ```markdown
//!   # Header
//!   {#next-line-id}
//!   ```
//!
//! ## HTML anchors
//! - `<a id="custom-id"></a>` or `<a name="custom-id"></a>` beside the heading text
//! - Example: `## <a name="my-id"></a>Header`
//!
//! An empty anchor element is stripped from the heading text but, unlike an
//! attr-list ID, it does not replace the slug generated from the text: the
//! rendered heading answers to both. Tags are read in source order as a browser
//! tokenizes them, so an `<a>` written inside another tag's attribute value is
//! part of that value. Anchor markup inside a code span or an HTML comment, or
//! whose `<` is backslash-escaped, is heading text and defines nothing.
//!
//! The module provides functions to detect and extract IDs from both inline
//! and standalone (next-line) attr-list syntax.

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

/// The name of an HTML tag: a letter, then letters, digits and hyphens.
pub const HTML_TAG_NAME_PATTERN: &str = "[A-Za-z][A-Za-z0-9-]*";

/// Attribute list of an inline HTML open tag as CommonMark defines it.
///
/// A quoted value may contain `>` without ending the tag, which is why a tag
/// cannot be delimited by searching for the next `>`.
pub const HTML_TAG_ATTRIBUTES_PATTERN: &str = r#"(?:\s+[^\s"'>/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*"#;

/// An inline HTML open tag, self-closing or not, with its name captured.
pub static HTML_OPEN_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"<({HTML_TAG_NAME_PATTERN}){HTML_TAG_ATTRIBUTES_PATTERN}\s*/?>"
    ))
    .unwrap()
});

/// The name of a tag as a browser reads it inside raw HTML: a letter, then
/// anything up to whitespace, `/` or `>`.
pub const HTML_BLOCK_TAG_NAME_PATTERN: &str = r"[A-Za-z][^\s/>]*";

/// An open tag inside an HTML block, where the browser's tokenizer rather than
/// CommonMark's inline grammar decides what a tag is. Only the tag name is read
/// the browser's way; attributes still follow the CommonMark grammar.
pub static HTML_BLOCK_OPEN_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"<({HTML_BLOCK_TAG_NAME_PATTERN}){HTML_TAG_ATTRIBUTES_PATTERN}\s*/?>"
    ))
    .unwrap()
});

/// The closing tag of an `<a>` element at the start of the text, with any
/// whitespace before it.
static HTML_ANCHOR_CLOSING_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s*</a\s*>").unwrap());

/// Pattern for custom header IDs supporting both kramdown and python-markdown attr-list formats
/// Supports: {#id}, { #id }, {:#id}, {: #id } and full attr-list with classes/attributes
/// Must contain #id but can have other attributes: {: #id .class data="value" }
/// More conservative: only matches when there's actually a hash followed by valid ID characters
static HEADER_ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\{\s*:?\s*([^}]*?#[^}]*?)\s*\}\s*$").unwrap());

/// Pattern to validate that an ID contains only valid characters
static ID_VALIDATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_\-:]+$").unwrap());

/// Pattern for standalone attr-list lines (Jekyll/kramdown style on line after heading)
/// Matches lines that are just attr-list syntax: {#id}, {: #id .class }, etc.
static STANDALONE_ATTR_LIST_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\{\s*:?\s*([^}]*#[a-zA-Z0-9_\-:]+[^}]*)\s*\}\s*$").unwrap());

/// Extract custom header ID from a line if present, returning clean text and ID
///
/// Supports multiple formats:
/// - Kramdown: `{#id}`
/// - Python-markdown: `{:#id}`, `{: #id}`, `{: #id .class}`
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::extract_header_id;
///
/// // Kramdown format
/// let (text, id) = extract_header_id("# Header {#custom-id}");
/// assert_eq!(text, "# Header");
/// assert_eq!(id, Some("custom-id".to_string()));
///
/// // Python-markdown attr-list format
/// let (text, id) = extract_header_id("# Header {: #my-id .highlight}");
/// assert_eq!(text, "# Header");
/// assert_eq!(id, Some("my-id".to_string()));
/// ```
pub fn extract_header_id(line: &str) -> (String, Option<String>) {
    let heading = extract_heading_text(line);
    (heading.text, heading.custom_id)
}

/// The content of a heading split into what a reader sees, what a renderer
/// slugs, and the custom ID its attribute list declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingText {
    /// The text as a reader sees it: anchor elements and the attribute list
    /// removed, one of the two spaces an element sat between gone with it, and
    /// both ends trimmed.
    pub text: String,
    /// The text a slug is generated from: the same as `text`, except that every
    /// space an anchor element leaves behind stays. A browser shows
    /// `## Alpha <a id="x"></a>` as "Alpha" and `## Foo <a id="x"></a> Bar` as
    /// "Foo Bar", but GitHub and kramdown slug them to `alpha-` and `foo--bar`;
    /// each anchor style decides for itself whether to trim and collapse.
    pub slug_text: String,
    /// The ID from a `{#id}` or `{: #id}` attribute list, if any.
    pub custom_id: Option<String>,
}

/// Split heading content into display text, slug text and custom ID.
///
/// See [`HeadingText`] for what each part holds. The attribute-list formats are
/// those of [`extract_header_id`], which returns the display text and ID only.
pub fn extract_heading_text(line: &str) -> HeadingText {
    // An empty anchor element beside the text (`## <a name="foo"></a>Heading`) is
    // markup, not heading text.
    let (line, seams) = strip_html_anchor_elements(line);
    let line = line.as_ref();

    let (slug_text, custom_id) = match custom_id_at_end(line) {
        Some((attr_list_start, id)) => (line[..attr_list_start].trim_end(), Some(id)),
        None => (line, None),
    };
    HeadingText {
        text: display_text(slug_text, &seams),
        slug_text: slug_text.to_string(),
        custom_id,
    }
}

/// The ID declared by an attribute list that ends `line`, with the byte offset
/// where the attribute list starts. An attribute list whose ID fails validation
/// declares nothing and stays heading text.
fn custom_id_at_end(line: &str) -> Option<(usize, String)> {
    let captures = HEADER_ID_PATTERN.captures(line)?;
    let attr_list_start = captures.get(0)?.start();
    let attr_str = captures.get(1)?.as_str().trim();
    let hash_pos = attr_str.find('#')?;
    let after_hash = &attr_str[hash_pos + 1..];

    // In the simple kramdown form `{#id}` the ID runs to the end. In the full
    // attr-list form `{: #id .class key="value"}` it ends at the next attribute
    // (whitespace), class (dot) or value (equals sign).
    let is_simple_format = !attr_str.contains(' ') && !attr_str.contains('=') && attr_str.starts_with('#');
    let potential_id = if is_simple_format {
        after_hash
    } else {
        match after_hash.find(|c: char| c.is_whitespace() || c == '.' || c == '=') {
            Some(delimiter_pos) => &after_hash[..delimiter_pos],
            None => after_hash,
        }
    };

    (!potential_id.is_empty() && ID_VALIDATE_PATTERN.is_match(potential_id))
        .then(|| (attr_list_start, potential_id.to_string()))
}

/// Remove the empty `<a>` elements that give a heading its anchors.
///
/// Exactly the element's bytes go, so the whitespace beside it stays part of
/// the text: `Foo<a id="x"></a> Bar` reads "Foo Bar", and whitespace left at
/// either end stays for the slug, since GitHub and kramdown slug
/// `Alpha <a id="x"></a>` as `#alpha-`. The offsets returned beside the text
/// are where each element sat, for [`display_text`].
fn strip_html_anchor_elements(text: &str) -> (Cow<'_, str>, Vec<usize>) {
    let anchors = empty_anchor_elements(text);
    if anchors.is_empty() {
        return (Cow::Borrowed(text), Vec::new());
    }

    let mut stripped = String::with_capacity(text.len());
    let mut seams = Vec::with_capacity(anchors.len());
    let mut copied_up_to = 0;
    for (range, _) in anchors {
        stripped.push_str(&text[copied_up_to..range.start]);
        seams.push(stripped.len());
        copied_up_to = range.end;
    }
    stripped.push_str(&text[copied_up_to..]);
    (Cow::Owned(stripped), seams)
}

/// The text a reader sees once the elements that sat at `seams` are gone.
///
/// An element written between two spaces (`Foo <a id="x"></a> Bar`) leaves
/// them adjacent, and a browser shows adjacent spaces as one, so one of each
/// such pair goes with the element. Spaces the author wrote in a run stay, and
/// whitespace at either end is trimmed as CommonMark trims a heading.
fn display_text(slug_text: &str, seams: &[usize]) -> String {
    let mut text = slug_text.to_string();
    for &seam in seams.iter().rev() {
        if seam == 0 || seam >= text.len() {
            continue;
        }
        let is_blank = |byte: u8| matches!(byte, b' ' | b'\t');
        if is_blank(text.as_bytes()[seam - 1]) && is_blank(text.as_bytes()[seam]) {
            text.remove(seam);
        }
    }
    text.trim().to_string()
}

/// The targets of the empty `<a>` elements in `text`, in source order.
///
/// Each element contributes its `id`, or its legacy `name` when it has no `id`,
/// so `<a name="old"></a><a id="new"></a>Heading` yields both. An element with
/// neither attribute set is not a target.
pub fn extract_html_anchor_ids(text: &str) -> Vec<String> {
    empty_anchor_elements(text)
        .into_iter()
        .filter_map(|(_, open_tag)| html_tag_attribute(open_tag, "id").or_else(|| html_tag_attribute(open_tag, "name")))
        .map(str::to_string)
        .collect()
}

/// The empty `<a>` elements of `text` in source order, each as the byte range
/// of the whole element and its open tag.
///
/// Tags are read the way a browser tokenizes them, one after another, so an
/// `<a>` written inside another tag's attribute value is part of that value
/// and no element. A tag inside a code span or an HTML comment, or whose `<`
/// is backslash-escaped, is heading text; the scan resumes just past its `<`,
/// since the text it was taken for may hold a real tag of its own.
fn empty_anchor_elements(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    if !text.contains('<') {
        return Vec::new();
    }

    let opaque = opaque_ranges(text);
    let mut anchors = Vec::new();
    let mut pos = 0;
    while let Some(tag) = HTML_OPEN_TAG.captures_at(text, pos) {
        let open_tag = tag.get(0).unwrap();
        if is_within(&opaque, open_tag.start()) || is_backslash_escaped(text, open_tag.start()) {
            pos = open_tag.start() + 1;
            continue;
        }
        pos = open_tag.end();

        if !tag[1].eq_ignore_ascii_case("a") {
            continue;
        }
        if let Some(closing_tag) = HTML_ANCHOR_CLOSING_TAG.find(&text[open_tag.end()..]) {
            pos = open_tag.end() + closing_tag.end();
            anchors.push((open_tag.start()..pos, open_tag.as_str()));
        }
    }

    anchors
}

/// Byte ranges of `text` that render as literal text: code spans and HTML comments.
///
/// A code span opens with a run of backticks and closes with a run of exactly
/// the same length; an opener that never closes is literal backticks. A comment
/// runs from `<!--` to the next `-->`, or to the end of the text; `<!-->` and
/// `<!--->` are complete comments, so the search for the closer starts right
/// after `<!`.
fn opaque_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] == b'`' {
            let run_end = pos + bytes[pos..].iter().take_while(|&&b| b == b'`').count();
            let run_len = run_end - pos;
            match closing_backtick_run(bytes, run_end, run_len) {
                Some(close_start) => {
                    ranges.push((pos, close_start + run_len));
                    pos = close_start + run_len;
                }
                None => pos = run_end,
            }
        } else if bytes[pos..].starts_with(b"<!--") {
            let end = text[pos + 2..]
                .find("-->")
                .map_or(bytes.len(), |offset| pos + 2 + offset + 3);
            ranges.push((pos, end));
            pos = end;
        } else {
            pos += 1;
        }
    }

    ranges
}

/// Start of the first backtick run of exactly `run_len` after `from`, if any.
fn closing_backtick_run(bytes: &[u8], from: usize, run_len: usize) -> Option<usize> {
    let mut pos = from;
    while pos < bytes.len() {
        if bytes[pos] != b'`' {
            pos += 1;
            continue;
        }
        let run_end = pos + bytes[pos..].iter().take_while(|&&b| b == b'`').count();
        if run_end - pos == run_len {
            return Some(pos);
        }
        pos = run_end;
    }
    None
}

fn is_within(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|&(start, end)| start <= pos && pos < end)
}

/// Whether the character at byte `pos` of `text` is backslash-escaped.
///
/// CommonMark reads `\<` as a literal `<` and `\\<` as a literal backslash
/// followed by a tag, so the character is escaped when an odd number of
/// backslashes precede it.
pub fn is_backslash_escaped(text: &str, pos: usize) -> bool {
    text.as_bytes()[..pos].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
}

/// The value of attribute `name` in the HTML open tag `tag`.
///
/// Attribute names are compared whole and without regard to case, so `data-id`
/// never stands in for `id`. As in HTML, the first occurrence decides. An
/// attribute that is absent, has no value or has an empty value yields `None`.
pub fn html_tag_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let bytes = tag.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }

    let ends_name = |b: u8| b.is_ascii_whitespace() || matches!(b, b'=' | b'>' | b'/');
    let mut pos = 1 + bytes[1..].iter().take_while(|&&b| !ends_name(b)).count();

    loop {
        pos += bytes[pos..].iter().take_while(|b| b.is_ascii_whitespace()).count();
        if pos >= bytes.len() || matches!(bytes[pos], b'>' | b'/') {
            return None;
        }

        let name_start = pos;
        pos += bytes[pos..].iter().take_while(|&&b| !ends_name(b)).count();
        let attribute = &tag[name_start..pos];

        let after_name = pos + bytes[pos..].iter().take_while(|b| b.is_ascii_whitespace()).count();
        let value = if bytes.get(after_name) == Some(&b'=') {
            let value_start = after_name
                + 1
                + bytes[after_name + 1..]
                    .iter()
                    .take_while(|b| b.is_ascii_whitespace())
                    .count();
            match bytes.get(value_start) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let value_end = value_start + 1 + tag[value_start + 1..].find(quote as char)?;
                    pos = value_end + 1;
                    &tag[value_start + 1..value_end]
                }
                _ => {
                    let value_end = value_start
                        + bytes[value_start..]
                            .iter()
                            .take_while(|&&b| !b.is_ascii_whitespace() && b != b'>')
                            .count();
                    pos = value_end;
                    &tag[value_start..value_end]
                }
            }
        } else {
            ""
        };

        if attribute.eq_ignore_ascii_case(name) {
            return (!value.is_empty()).then_some(value);
        }
    }
}

/// Check if a line is a standalone attr-list (Jekyll/kramdown style)
///
/// This detects attr-list syntax that appears on its own line, typically
/// the line after a header to provide additional attributes.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::is_standalone_attr_list;
///
/// assert!(is_standalone_attr_list("{#custom-id}"));
/// assert!(is_standalone_attr_list("{: #spaced .class }"));
/// assert!(!is_standalone_attr_list("Some text {#not-standalone}"));
/// assert!(!is_standalone_attr_list(""));
/// ```
pub fn is_standalone_attr_list(line: &str) -> bool {
    STANDALONE_ATTR_LIST_PATTERN.is_match(line)
}

/// Extract ID from a standalone attr-list line
///
/// Returns the ID if the line is a valid standalone attr-list with an ID.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::extract_standalone_attr_list_id;
///
/// assert_eq!(extract_standalone_attr_list_id("{#custom-id}"), Some("custom-id".to_string()));
/// assert_eq!(extract_standalone_attr_list_id("{: #spaced .class }"), Some("spaced".to_string()));
/// assert_eq!(extract_standalone_attr_list_id("not an attr-list"), None);
/// ```
pub fn extract_standalone_attr_list_id(line: &str) -> Option<String> {
    if let Some(captures) = STANDALONE_ATTR_LIST_PATTERN.captures(line)
        && let Some(attr_content) = captures.get(1)
    {
        let attr_str = attr_content.as_str().trim();

        // Use the same logic as extract_header_id for consistency
        if let Some(hash_pos) = attr_str.find('#') {
            let after_hash = &attr_str[hash_pos + 1..];

            // Check if this looks like a simple kramdown ID: {#id} with no spaces or attributes
            let is_simple_format = !attr_str.contains(' ') && !attr_str.contains('=') && attr_str.starts_with('#');

            if is_simple_format {
                // Simple format: entire content after # should be the ID
                let potential_id = after_hash;
                if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                    return Some(potential_id.to_string());
                }
            } else {
                // Complex format: find proper delimiters (space for next attribute, dot for class)
                if let Some(delimiter_pos) = after_hash.find(|c: char| c.is_whitespace() || c == '.' || c == '=') {
                    let potential_id = &after_hash[..delimiter_pos];
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        return Some(potential_id.to_string());
                    }
                } else {
                    // No delimiter found in complex format, ID goes to end
                    let potential_id = after_hash;
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        return Some(potential_id.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kramdown_format_extraction() {
        // Simple kramdown format
        let (text, id) = extract_header_id("# Header {#simple}");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("simple".to_string()));

        let (text, id) = extract_header_id("## Section {#section-id}");
        assert_eq!(text, "## Section");
        assert_eq!(id, Some("section-id".to_string()));
    }

    #[test]
    fn test_python_markdown_attr_list_extraction() {
        // Python-markdown formats
        let (text, id) = extract_header_id("# Header {:#colon-id}");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("colon-id".to_string()));

        let (text, id) = extract_header_id("# Header {: #spaced-id }");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("spaced-id".to_string()));
    }

    #[test]
    fn test_extended_attr_list_extraction() {
        // ID with single class
        let (text, id) = extract_header_id("# Header {: #with-class .highlight }");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("with-class".to_string()));

        // ID with multiple classes
        let (text, id) = extract_header_id("## Section {: #multi .class1 .class2 }");
        assert_eq!(text, "## Section");
        assert_eq!(id, Some("multi".to_string()));

        // ID with key-value attributes
        let (text, id) = extract_header_id("### Subsection {: #with-attrs data-test=\"value\" style=\"color: red\" }");
        assert_eq!(text, "### Subsection");
        assert_eq!(id, Some("with-attrs".to_string()));

        // Complex combination
        let (text, id) = extract_header_id("#### Complex {: #complex .highlight data-role=\"button\" title=\"Test\" }");
        assert_eq!(text, "#### Complex");
        assert_eq!(id, Some("complex".to_string()));

        // ID with quotes in attributes
        let (text, id) = extract_header_id("##### Quotes {: #quotes title=\"Has \\\"nested\\\" quotes\" }");
        assert_eq!(text, "##### Quotes");
        assert_eq!(id, Some("quotes".to_string()));
    }

    #[test]
    fn test_attr_list_detection_edge_cases() {
        // Attr-list without ID should not match
        let (text, id) = extract_header_id("# Header {: .class-only }");
        assert_eq!(text, "# Header {: .class-only }");
        assert_eq!(id, None);

        // Malformed attr-list should not match
        let (text, id) = extract_header_id("# Header { no-hash }");
        assert_eq!(text, "# Header { no-hash }");
        assert_eq!(id, None);

        // Empty ID should not match
        let (text, id) = extract_header_id("# Header {: # }");
        assert_eq!(text, "# Header {: # }");
        assert_eq!(id, None);

        // ID in middle (not at end) should not match
        let (text, id) = extract_header_id("# Header {: #middle } with more text");
        assert_eq!(text, "# Header {: #middle } with more text");
        assert_eq!(id, None);
    }

    #[test]
    fn test_standalone_attr_list_detection() {
        // Simple ID formats
        assert!(is_standalone_attr_list("{#custom-id}"));
        assert!(is_standalone_attr_list("{ #spaced-id }"));
        assert!(is_standalone_attr_list("{:#colon-id}"));
        assert!(is_standalone_attr_list("{: #full-format }"));

        // With classes and attributes
        assert!(is_standalone_attr_list("{: #with-class .highlight }"));
        assert!(is_standalone_attr_list("{: #multi .class1 .class2 }"));
        assert!(is_standalone_attr_list("{: #complex .highlight data-test=\"value\" }"));

        // Should not match
        assert!(!is_standalone_attr_list("Some text {#not-standalone}"));
        assert!(!is_standalone_attr_list("Text before {#id}"));
        assert!(!is_standalone_attr_list("{#id} text after"));
        assert!(!is_standalone_attr_list(""));
        assert!(!is_standalone_attr_list("   ")); // just spaces
        assert!(!is_standalone_attr_list("{: .class-only }")); // no ID
    }

    #[test]
    fn test_standalone_attr_list_id_extraction() {
        // Basic formats
        assert_eq!(extract_standalone_attr_list_id("{#simple}"), Some("simple".to_string()));
        assert_eq!(
            extract_standalone_attr_list_id("{ #spaced }"),
            Some("spaced".to_string())
        );
        assert_eq!(extract_standalone_attr_list_id("{:#colon}"), Some("colon".to_string()));
        assert_eq!(extract_standalone_attr_list_id("{: #full }"), Some("full".to_string()));

        // With additional attributes
        assert_eq!(
            extract_standalone_attr_list_id("{: #with-class .highlight }"),
            Some("with-class".to_string())
        );
        assert_eq!(
            extract_standalone_attr_list_id("{: #complex .class1 .class2 data=\"value\" }"),
            Some("complex".to_string())
        );

        // Should return None
        assert_eq!(extract_standalone_attr_list_id("Not an attr-list"), None);
        assert_eq!(extract_standalone_attr_list_id("Text {#not-standalone}"), None);
        assert_eq!(extract_standalone_attr_list_id("{: .class-only }"), None);
        assert_eq!(extract_standalone_attr_list_id(""), None);
    }

    #[test]
    fn test_backward_compatibility() {
        // Ensure all original kramdown formats still work
        let test_cases = vec![
            ("# Header {#a}", "# Header", Some("a".to_string())),
            ("# Header {#simple-id}", "# Header", Some("simple-id".to_string())),
            ("## Heading {#heading-2}", "## Heading", Some("heading-2".to_string())),
            (
                "### With-Hyphens {#with-hyphens}",
                "### With-Hyphens",
                Some("with-hyphens".to_string()),
            ),
        ];

        for (input, expected_text, expected_id) in test_cases {
            let (text, id) = extract_header_id(input);
            assert_eq!(text, expected_text, "Text mismatch for input: {input}");
            assert_eq!(id, expected_id, "ID mismatch for input: {input}");
        }
    }

    #[test]
    fn test_invalid_id_with_dots() {
        // IDs with dots should not be extracted (dots are not valid ID characters)
        let (text, id) = extract_header_id("## Another. {#id.with.dots}");
        assert_eq!(text, "## Another. {#id.with.dots}"); // Should not strip invalid ID
        assert_eq!(id, None); // Should not extract invalid ID

        // Test that only the part before the dot would be extracted if it was valid standalone
        // But since it's in an invalid format, the whole thing should be rejected
        let (text, id) = extract_header_id("## Another. {#id.more.dots}");
        assert_eq!(text, "## Another. {#id.more.dots}");
        assert_eq!(id, None);
    }

    #[test]
    fn test_html_anchor_stripping() {
        // HTML anchor elements should be stripped from heading text
        // This is used by some authors for custom anchors

        // Basic <a name="..."></a> pattern
        let (text, id) = extract_header_id("<a name=\"cheatsheets\"></a>Cheat Sheets");
        assert_eq!(text, "Cheat Sheets");
        assert_eq!(id, None);

        // <a id="..."></a> pattern
        let (text, id) = extract_header_id("<a id=\"tools\"></a>Tools and session management");
        assert_eq!(text, "Tools and session management");
        assert_eq!(id, None);

        // With spaces around the anchor
        let (text, id) = extract_header_id("<a name=\"foo\"></a> Heading with space");
        assert_eq!(text, "Heading with space");
        assert_eq!(id, None);

        // Combined with kramdown custom ID
        let (text, id) = extract_header_id("<a name=\"old\"></a>My Section {#my-custom-id}");
        assert_eq!(text, "My Section");
        assert_eq!(id, Some("my-custom-id".to_string()));
    }

    #[test]
    fn test_html_anchor_ids_are_read_from_empty_anchor_elements_in_order() {
        assert_eq!(extract_html_anchor_ids(r#"Heading<a id="target"></a>"#), ["target"]);
        assert_eq!(
            extract_html_anchor_ids(r#"<A class="legacy" NAME='fallback' ID='preferred'></A>Heading"#),
            ["preferred"]
        );
        assert_eq!(
            extract_html_anchor_ids(r#"<a name='legacy'></a><a id="newer"></a>Heading"#),
            ["legacy", "newer"]
        );
        assert_eq!(extract_html_anchor_ids("<a id=plain></a>Heading"), ["plain"]);
        assert!(extract_html_anchor_ids(r##"<a href="#target"></a>Heading"##).is_empty());
        assert!(extract_html_anchor_ids(r#"<span id="target"></span>Heading"#).is_empty());
        assert!(extract_html_anchor_ids(r#"<a id="target">text</a>Heading"#).is_empty());
    }

    #[test]
    fn test_an_attribute_merely_ending_in_id_or_name_is_not_an_anchor() {
        assert!(extract_html_anchor_ids(r#"Foo<a data-id="tracking" data-name="pixel"></a>"#).is_empty());
        assert!(extract_html_anchor_ids(r#"Foo<a id=""></a>"#).is_empty());
    }

    #[test]
    fn test_a_quoted_attribute_value_may_contain_a_closing_angle_bracket() {
        let raw = r#"<a title="a > b" id="target"></a>Heading"#;
        assert_eq!(extract_html_anchor_ids(raw), ["target"]);
        assert_eq!(extract_header_id(raw), ("Heading".to_string(), None));
    }

    #[test]
    fn test_anchor_markup_inside_a_code_span_is_heading_text() {
        let raw = "Showing `<a id=\"literal\"></a>` syntax";
        assert!(extract_html_anchor_ids(raw).is_empty());
        assert_eq!(extract_header_id(raw), (raw.to_string(), None));

        // A real anchor beside the code span is still found and stripped.
        let raw = "Showing `<a id=\"literal\"></a>` syntax<a id=\"real\"></a>";
        assert_eq!(extract_html_anchor_ids(raw), ["real"]);
        assert_eq!(extract_header_id(raw).0, "Showing `<a id=\"literal\"></a>` syntax");
    }

    #[test]
    fn test_anchor_markup_inside_an_html_comment_is_not_a_target() {
        let raw = "Foo <!-- <a id=\"hidden\"></a> -->";
        assert!(extract_html_anchor_ids(raw).is_empty());
        assert_eq!(extract_header_id(raw), (raw.to_string(), None));
    }

    #[test]
    fn test_html_tag_attribute_matches_whole_names_case_insensitively() {
        assert_eq!(html_tag_attribute(r#"<div data-id="x" ID="y">"#, "id"), Some("y"));
        assert_eq!(html_tag_attribute("<a name = 'legacy' >", "name"), Some("legacy"));
        assert_eq!(html_tag_attribute("<a id=plain>", "id"), Some("plain"));
        assert_eq!(html_tag_attribute(r#"<a id="first" id="second">"#, "id"), Some("first"));
        assert_eq!(html_tag_attribute(r#"<a id="">"#, "id"), None);
        assert_eq!(html_tag_attribute(r#"<a hidden id="x">"#, "hidden"), None);
        assert_eq!(html_tag_attribute(r#"<a hidden id="x">"#, "id"), Some("x"));
        assert_eq!(html_tag_attribute(r#"<a title="a > b" id="x">"#, "id"), Some("x"));
        assert_eq!(html_tag_attribute(r#"<video title="id=fake">"#, "id"), None);
        assert_eq!(html_tag_attribute("<br/>", "id"), None);
    }

    #[test]
    fn test_html_anchor_stripping_handles_attribute_variations() {
        let (text, id) = extract_header_id(r#"<A class="legacy" ID='target'></A>Heading"#);
        assert_eq!(text, "Heading");
        assert_eq!(id, None);
    }

    #[test]
    fn test_a_backslash_escaped_anchor_element_is_heading_text() {
        // `\<` is a literal `<`, so the markup renders as text and defines no
        // target. An escaped backslash before the `<` leaves it a tag.
        let raw = r#"Show \<a id="example"></a> syntax"#;
        assert!(extract_html_anchor_ids(raw).is_empty());
        assert_eq!(extract_header_id(raw), (raw.to_string(), None));

        let raw = r#"Show \\<a id="example"></a> syntax"#;
        assert_eq!(extract_html_anchor_ids(raw), ["example"]);
        assert_eq!(extract_header_id(raw).0, r"Show \\ syntax");
    }

    #[test]
    fn test_stripping_an_anchor_element_keeps_the_whitespace_beside_it() {
        // The element renders nothing, so removing exactly its bytes leaves the
        // text the browser shows: the space between the words stays. The display
        // text is trimmed at either end, as CommonMark trims a heading, and an
        // element between two spaces takes one of them with it, as a browser
        // shows adjacent spaces as one. The slug text keeps every space, since
        // GitHub and kramdown turn each into a hyphen.
        let cases = [
            (r#"Foo<a id="alias"></a> Bar"#, "Foo Bar", "Foo Bar"),
            (r#"Foo <a id="alias"></a>Bar"#, "Foo Bar", "Foo Bar"),
            (r#"Foo <a id="alias"></a> Bar"#, "Foo Bar", "Foo  Bar"),
            (r#"Foo <a id="a"></a> <a id="b"></a> Bar"#, "Foo Bar", "Foo   Bar"),
            (r#"Foo  <a id="alias"></a>Bar"#, "Foo  Bar", "Foo  Bar"),
            (r#"<a id="alias"></a> Foo"#, "Foo", " Foo"),
            (r#"Foo <a id="alias"></a>"#, "Foo", "Foo "),
            (r#"<a id="a"></a> Foo <a id="b"></a>"#, "Foo", " Foo "),
        ];
        for (raw, text, slug_text) in cases {
            let heading = extract_heading_text(raw);
            assert_eq!(heading.text, text, "display text of {raw:?}");
            assert_eq!(heading.slug_text, slug_text, "slug text of {raw:?}");
            assert_eq!(heading.custom_id, None, "custom ID of {raw:?}");
        }
        assert_eq!(extract_header_id(r#"Foo <a id="alias"></a>"#).0, "Foo");
    }

    #[test]
    fn test_an_anchor_inside_another_tags_attribute_value_is_not_an_element() {
        // The `<a>` is part of the span's `title` value, so the browser creates
        // no element from it and the heading keeps its bytes.
        let raw = r#"<span title='<a id="fake"></a>'>Foo</span>"#;
        assert!(extract_html_anchor_ids(raw).is_empty());
        assert_eq!(extract_header_id(raw), (raw.to_string(), None));

        let raw = r#"<span title='x'><a id="real"></a>Foo</span>"#;
        assert_eq!(extract_html_anchor_ids(raw), ["real"]);
        assert_eq!(extract_header_id(raw).0, "<span title='x'>Foo</span>");
    }

    #[test]
    fn test_an_escaped_tag_does_not_hide_the_element_written_inside_it() {
        // `\<span` is text, so the `<a>` where its attribute value would be is a
        // real element.
        let raw = r#"\<span title='<a id="real"></a>'>Foo"#;
        assert_eq!(extract_html_anchor_ids(raw), ["real"]);
        assert_eq!(extract_header_id(raw).0, r#"\<span title=''>Foo"#);
    }

    #[test]
    fn test_a_degenerate_comment_ends_at_its_own_closer() {
        // `<!-->` and `<!--->` are complete comments, so the anchor after them is
        // an element and the later `-->` is text.
        assert_eq!(extract_html_anchor_ids(r#"<!--> <a id="x"></a> --> Foo"#), ["x"]);
        assert_eq!(extract_html_anchor_ids(r#"<!---> <a id="y"></a> --> Foo"#), ["y"]);
        assert!(extract_html_anchor_ids(r#"<!-- <a id="z"></a> --> Foo"#).is_empty());
    }

    #[test]
    fn test_is_backslash_escaped_counts_the_run_of_backslashes() {
        assert!(!is_backslash_escaped("<a>", 0));
        assert!(is_backslash_escaped(r"\<a>", 1));
        assert!(!is_backslash_escaped(r"\\<a>", 2));
        assert!(is_backslash_escaped(r"\\\<a>", 3));
        assert!(!is_backslash_escaped(r"x<a>", 1));
    }
}
