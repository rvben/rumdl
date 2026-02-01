/// MkDocs attr_list extension support
///
/// This module provides support for the Python-Markdown attr_list extension,
/// which allows adding custom attributes to Markdown elements including:
/// - Custom IDs: `{#custom-id}`
/// - Classes: `{.my-class}`
/// - Key-value pairs: `{key="value"}`
///
/// ## Syntax
///
/// ### Headings with custom anchors
/// ```markdown
/// # Heading {#custom-anchor}
/// # Heading {.class-name}
/// # Heading {#id .class key=value}
/// ```
///
/// ### Block attributes (on separate line)
/// ```markdown
/// Paragraph text here.
/// {: #id .class }
/// ```
///
/// ### Inline attributes
/// ```markdown
/// [link text](url){: .external target="_blank" }
/// *emphasis*{: .special }
/// ```
///
/// ## References
///
/// - [Python-Markdown attr_list](https://python-markdown.github.io/extensions/attr_list/)
/// - [MkDocs Material - Anchor Links](https://squidfunk.github.io/mkdocs-material/reference/annotations/#anchor-links)
use regex::Regex;
use std::sync::LazyLock;

/// Pattern to match attr_list syntax: `{: #id .class key="value" }`
/// The `:` prefix is optional (kramdown style uses it, but attr_list accepts both)
/// Requirements for valid attr_list:
/// - Must start with `{` and optional `:` with optional whitespace
/// - Must contain at least one of: #id, .class, or key="value"
/// - Must end with `}`
static ATTR_LIST_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Pattern requires at least one attribute (id, class, or key=value)
    // to avoid matching plain text in braces like {word}
    Regex::new(r#"\{:?\s*(?:(?:[#.][a-zA-Z_][a-zA-Z0-9_-]*|[a-zA-Z_][a-zA-Z0-9_-]*=["'][^"']*["'])\s*)+\}"#).unwrap()
});

/// Pattern to extract custom ID from attr_list: `#id`
static CUSTOM_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap());

/// Pattern to extract classes from attr_list: `.class`
static CLASS_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap());

/// Pattern to extract key-value pairs: `key="value"` or `key='value'`
static KEY_VALUE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([a-zA-Z_][a-zA-Z0-9_-]*)=["']([^"']*)["']"#).unwrap());

/// Parsed attribute list containing IDs, classes, and key-value pairs
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttrList {
    /// Custom ID (e.g., `custom-id` from `{#custom-id}`)
    pub id: Option<String>,
    /// CSS classes (e.g., `["class1", "class2"]` from `{.class1 .class2}`)
    pub classes: Vec<String>,
    /// Key-value attributes (e.g., `[("target", "_blank")]`)
    pub attributes: Vec<(String, String)>,
    /// Start position in the line (0-indexed)
    pub start: usize,
    /// End position in the line (0-indexed, exclusive)
    pub end: usize,
}

impl AttrList {
    /// Create a new empty AttrList
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this attr_list has a custom ID
    #[inline]
    pub fn has_id(&self) -> bool {
        self.id.is_some()
    }

    /// Check if this attr_list has any classes
    #[inline]
    pub fn has_classes(&self) -> bool {
        !self.classes.is_empty()
    }

    /// Check if this attr_list has any attributes
    #[inline]
    pub fn has_attributes(&self) -> bool {
        !self.attributes.is_empty()
    }

    /// Check if this attr_list is empty (no id, classes, or attributes)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.attributes.is_empty()
    }
}

/// Check if a line contains attr_list syntax
#[inline]
pub fn contains_attr_list(line: &str) -> bool {
    // Fast path: check for opening brace first
    if !line.contains('{') {
        return false;
    }
    ATTR_LIST_PATTERN.is_match(line)
}

/// Check if a line is a standalone block attr_list (on its own line)
/// This is used for block-level attributes like:
/// ```markdown
/// Paragraph text.
/// { .class-name }
/// ```
/// or with colon:
/// ```markdown
/// Paragraph text.
/// {: .class-name }
/// ```
#[inline]
pub fn is_standalone_attr_list(line: &str) -> bool {
    let trimmed = line.trim();
    // Must start with { and end with }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return false;
    }
    // Must be a valid attr_list (not just random braces)
    ATTR_LIST_PATTERN.is_match(trimmed)
}

/// Check if a line is a MkDocs anchor line (empty link with attr_list)
///
/// MkDocs anchor lines are used to create invisible anchor points in documentation.
/// They consist of an empty link `[]()` followed by an attr_list containing an ID
/// or class. These are rendered as `<a id="anchor"></a>` in the HTML output.
///
/// # Syntax
///
/// ```markdown
/// [](){ #anchor-id }              <!-- Basic anchor -->
/// [](){#anchor-id}                <!-- No spaces -->
/// [](){ #id .class }              <!-- Anchor with class -->
/// [](){: #id }                    <!-- Kramdown-style with colon -->
/// [](){ .highlight }              <!-- Class-only (styling hook) -->
/// ```
///
/// # Use Cases
///
/// 1. **Deep linking**: Create anchor points for linking to specific paragraphs
/// 2. **Cross-references**: Target for mkdocs-autorefs links
/// 3. **Styling hooks**: Apply CSS classes to following content
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::mkdocs_attr_list::is_mkdocs_anchor_line;
///
/// // Valid anchor lines
/// assert!(is_mkdocs_anchor_line("[](){ #example }"));
/// assert!(is_mkdocs_anchor_line("[](){#example}"));
/// assert!(is_mkdocs_anchor_line("[](){ #id .class }"));
/// assert!(is_mkdocs_anchor_line("[](){: #anchor }"));
///
/// // NOT anchor lines
/// assert!(!is_mkdocs_anchor_line("[link](url)"));           // Has URL
/// assert!(!is_mkdocs_anchor_line("[](){ #id } text"));      // Has trailing content
/// assert!(!is_mkdocs_anchor_line("[]()"));                  // No attr_list
/// assert!(!is_mkdocs_anchor_line("[](){ }"));               // Empty attr_list
/// ```
///
/// # References
///
/// - [Python-Markdown attr_list](https://python-markdown.github.io/extensions/attr_list/)
/// - [MkDocs Material - Anchor Links](https://squidfunk.github.io/mkdocs-material/reference/annotations/#anchor-links)
/// - [MkDocs discussions on paragraph anchors](https://github.com/mkdocs/mkdocs/discussions/3754)
#[inline]
pub fn is_mkdocs_anchor_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Fast path: must contain the empty link pattern
    if !trimmed.starts_with("[]()") {
        return false;
    }

    // Extract the part after []()
    let after_link = &trimmed[4..];

    // Fast path: must contain opening brace for attr_list
    if !after_link.contains('{') {
        return false;
    }

    // Skip optional whitespace between []() and {
    let attr_start = after_link.trim_start();

    // Must start with { or {:
    if !attr_start.starts_with('{') {
        return false;
    }

    // Find the closing brace
    let Some(close_idx) = attr_start.find('}') else {
        return false;
    };

    // Nothing meaningful should follow the closing brace
    if !attr_start[close_idx + 1..].trim().is_empty() {
        return false;
    }

    // Extract and validate the attr_list content
    let attr_content = &attr_start[..=close_idx];

    // Use the existing attr_list validation - must be a valid attr_list
    if !ATTR_LIST_PATTERN.is_match(attr_content) {
        return false;
    }

    // Parse the attr_list to ensure it has meaningful content (ID or class)
    let attrs = find_attr_lists(attr_content);
    attrs.iter().any(|a| a.has_id() || a.has_classes())
}

/// Extract all attr_lists from a line
pub fn find_attr_lists(line: &str) -> Vec<AttrList> {
    if !line.contains('{') {
        return Vec::new();
    }

    let mut results = Vec::new();

    for m in ATTR_LIST_PATTERN.find_iter(line) {
        let attr_text = m.as_str();
        let mut attr_list = AttrList {
            start: m.start(),
            end: m.end(),
            ..Default::default()
        };

        // Extract custom ID (first one wins per HTML spec)
        if let Some(caps) = CUSTOM_ID_PATTERN.captures(attr_text)
            && let Some(id_match) = caps.get(1)
        {
            attr_list.id = Some(id_match.as_str().to_string());
        }

        // Extract all classes
        for caps in CLASS_PATTERN.captures_iter(attr_text) {
            if let Some(class_match) = caps.get(1) {
                attr_list.classes.push(class_match.as_str().to_string());
            }
        }

        // Extract key-value pairs
        for caps in KEY_VALUE_PATTERN.captures_iter(attr_text) {
            if let Some(key) = caps.get(1)
                && let Some(value) = caps.get(2)
            {
                attr_list
                    .attributes
                    .push((key.as_str().to_string(), value.as_str().to_string()));
            }
        }

        if !attr_list.is_empty() {
            results.push(attr_list);
        }
    }

    results
}

/// Extract custom ID from a heading line with attr_list syntax
///
/// Returns the custom ID if found, or None if no custom ID is present.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::mkdocs_attr_list::extract_heading_custom_id;
///
/// assert_eq!(extract_heading_custom_id("# Heading {#my-id}"), Some("my-id".to_string()));
/// assert_eq!(extract_heading_custom_id("## Title {#custom .class}"), Some("custom".to_string()));
/// assert_eq!(extract_heading_custom_id("# No ID here"), None);
/// ```
pub fn extract_heading_custom_id(line: &str) -> Option<String> {
    let attrs = find_attr_lists(line);
    attrs.into_iter().find_map(|a| a.id)
}

/// Strip attr_list syntax from a heading text
///
/// Returns the heading text without the trailing attr_list.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::mkdocs_attr_list::strip_attr_list_from_heading;
///
/// assert_eq!(strip_attr_list_from_heading("Heading {#my-id}"), "Heading");
/// assert_eq!(strip_attr_list_from_heading("Title {#id .class}"), "Title");
/// assert_eq!(strip_attr_list_from_heading("No attributes"), "No attributes");
/// ```
pub fn strip_attr_list_from_heading(text: &str) -> String {
    if let Some(m) = ATTR_LIST_PATTERN.find(text) {
        // Only strip if at the end of the text (with optional whitespace)
        let after = &text[m.end()..];
        if after.trim().is_empty() {
            return text[..m.start()].trim_end().to_string();
        }
    }
    text.to_string()
}

/// Check if a position in a line is within an attr_list
pub fn is_in_attr_list(line: &str, position: usize) -> bool {
    for m in ATTR_LIST_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }
    false
}

/// Extract all custom anchor IDs from a document
///
/// This function finds all custom IDs defined using attr_list syntax throughout
/// the document. These IDs can be used as fragment link targets.
///
/// # Arguments
/// * `content` - The full document content
///
/// # Returns
/// A vector of (custom_id, line_number) tuples, where line_number is 1-indexed
pub fn extract_all_custom_anchors(content: &str) -> Vec<(String, usize)> {
    let mut anchors = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;

        for attr_list in find_attr_lists(line) {
            if let Some(id) = attr_list.id {
                anchors.push((id, line_num));
            }
        }
    }

    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_attr_list() {
        // Valid attr_list syntax
        assert!(contains_attr_list("# Heading {#custom-id}"));
        assert!(contains_attr_list("# Heading {.my-class}"));
        assert!(contains_attr_list("# Heading {#id .class}"));
        assert!(contains_attr_list("Text {: #id}"));
        assert!(contains_attr_list("Link {target=\"_blank\"}"));

        // Not attr_list
        assert!(!contains_attr_list("# Regular heading"));
        assert!(!contains_attr_list("Code with {braces}"));
        assert!(!contains_attr_list("Empty {}"));
        assert!(!contains_attr_list("Just text"));
    }

    #[test]
    fn test_find_attr_lists_basic() {
        let attrs = find_attr_lists("# Heading {#custom-id}");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].id, Some("custom-id".to_string()));
        assert!(attrs[0].classes.is_empty());
    }

    #[test]
    fn test_find_attr_lists_with_class() {
        let attrs = find_attr_lists("# Heading {.highlight}");
        assert_eq!(attrs.len(), 1);
        assert!(attrs[0].id.is_none());
        assert_eq!(attrs[0].classes, vec!["highlight"]);
    }

    #[test]
    fn test_find_attr_lists_complex() {
        let attrs = find_attr_lists("# Heading {#my-id .class1 .class2 data-value=\"test\"}");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].id, Some("my-id".to_string()));
        assert_eq!(attrs[0].classes, vec!["class1", "class2"]);
        assert_eq!(
            attrs[0].attributes,
            vec![("data-value".to_string(), "test".to_string())]
        );
    }

    #[test]
    fn test_find_attr_lists_kramdown_style() {
        // With colon prefix (kramdown style)
        let attrs = find_attr_lists("Paragraph {: #para-id .special }");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].id, Some("para-id".to_string()));
        assert_eq!(attrs[0].classes, vec!["special"]);
    }

    #[test]
    fn test_extract_heading_custom_id() {
        assert_eq!(
            extract_heading_custom_id("# Heading {#my-anchor}"),
            Some("my-anchor".to_string())
        );
        assert_eq!(
            extract_heading_custom_id("## Title {#title .class}"),
            Some("title".to_string())
        );
        assert_eq!(extract_heading_custom_id("# No ID {.class-only}"), None);
        assert_eq!(extract_heading_custom_id("# Plain heading"), None);
    }

    #[test]
    fn test_strip_attr_list_from_heading() {
        assert_eq!(strip_attr_list_from_heading("Heading {#my-id}"), "Heading");
        assert_eq!(strip_attr_list_from_heading("Title {#id .class}"), "Title");
        assert_eq!(
            strip_attr_list_from_heading("Multi Word Title {#anchor}"),
            "Multi Word Title"
        );
        assert_eq!(strip_attr_list_from_heading("No attributes"), "No attributes");
        // Attr list in middle should not be stripped
        assert_eq!(strip_attr_list_from_heading("Before {#id} after"), "Before {#id} after");
    }

    #[test]
    fn test_is_in_attr_list() {
        let line = "Some text {#my-id} more text";
        assert!(!is_in_attr_list(line, 0)); // "S"
        assert!(!is_in_attr_list(line, 8)); // " "
        assert!(is_in_attr_list(line, 10)); // "{"
        assert!(is_in_attr_list(line, 15)); // "i"
        assert!(!is_in_attr_list(line, 19)); // " "
    }

    #[test]
    fn test_extract_all_custom_anchors() {
        let content = r#"# First Heading {#first}

Some paragraph {: #para-id}

## Second {#second .class}

No ID here.

### Third {.class-only}

{#standalone-id}
"#;
        let anchors = extract_all_custom_anchors(content);

        assert_eq!(anchors.len(), 4);
        assert_eq!(anchors[0], ("first".to_string(), 1));
        assert_eq!(anchors[1], ("para-id".to_string(), 3));
        assert_eq!(anchors[2], ("second".to_string(), 5));
        assert_eq!(anchors[3], ("standalone-id".to_string(), 11));
    }

    #[test]
    fn test_multiple_attr_lists_same_line() {
        let attrs = find_attr_lists("[link]{#link-id} and [other]{#other-id}");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].id, Some("link-id".to_string()));
        assert_eq!(attrs[1].id, Some("other-id".to_string()));
    }

    #[test]
    fn test_attr_list_positions() {
        let line = "Text {#my-id} more";
        let attrs = find_attr_lists(line);
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].start, 5);
        assert_eq!(attrs[0].end, 13);
        assert_eq!(&line[attrs[0].start..attrs[0].end], "{#my-id}");
    }

    #[test]
    fn test_underscore_in_identifiers() {
        let attrs = find_attr_lists("# Heading {#my_custom_id .my_class}");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].id, Some("my_custom_id".to_string()));
        assert_eq!(attrs[0].classes, vec!["my_class"]);
    }

    /// Test for issue #337: Standalone attr_lists should be detected
    /// These should be treated as paragraph boundaries in reflow
    #[test]
    fn test_is_standalone_attr_list() {
        // Valid standalone attr_lists (on their own line)
        assert!(is_standalone_attr_list("{ .class-name }"));
        assert!(is_standalone_attr_list("{: .class-name }"));
        assert!(is_standalone_attr_list("{#custom-id}"));
        assert!(is_standalone_attr_list("{: #custom-id .class }"));
        assert!(is_standalone_attr_list("  { .indented }  ")); // With whitespace

        // Not standalone (part of other content)
        assert!(!is_standalone_attr_list("Some text {#id}"));
        assert!(!is_standalone_attr_list("{#id} more text"));
        assert!(!is_standalone_attr_list("# Heading {#id}"));

        // Not valid attr_lists (just braces)
        assert!(!is_standalone_attr_list("{ }"));
        assert!(!is_standalone_attr_list("{}"));
        assert!(!is_standalone_attr_list("{ random text }"));

        // Empty line
        assert!(!is_standalone_attr_list(""));
        assert!(!is_standalone_attr_list("   "));
    }

    /// Test for issue #365: MkDocs anchor lines should be detected
    /// Pattern: `[](){ #anchor }` creates invisible anchor points
    #[test]
    fn test_is_mkdocs_anchor_line_basic() {
        // Valid anchor lines with ID
        assert!(is_mkdocs_anchor_line("[](){ #example }"));
        assert!(is_mkdocs_anchor_line("[](){#example}"));
        assert!(is_mkdocs_anchor_line("[](){ #my-anchor }"));
        assert!(is_mkdocs_anchor_line("[](){ #anchor_with_underscore }"));

        // Valid anchor lines with class
        assert!(is_mkdocs_anchor_line("[](){ .highlight }"));
        assert!(is_mkdocs_anchor_line("[](){.my-class}"));

        // Valid anchor lines with both ID and class
        assert!(is_mkdocs_anchor_line("[](){ #anchor .class }"));
        assert!(is_mkdocs_anchor_line("[](){ .class #anchor }"));
        assert!(is_mkdocs_anchor_line("[](){ #id .class1 .class2 }"));
    }

    #[test]
    fn test_is_mkdocs_anchor_line_kramdown_style() {
        // Kramdown-style with colon prefix
        assert!(is_mkdocs_anchor_line("[](){: #anchor }"));
        assert!(is_mkdocs_anchor_line("[](){:#anchor}"));
        assert!(is_mkdocs_anchor_line("[](){: .class }"));
        assert!(is_mkdocs_anchor_line("[](){: #id .class }"));
    }

    #[test]
    fn test_is_mkdocs_anchor_line_whitespace_variations() {
        // Leading/trailing whitespace on line
        assert!(is_mkdocs_anchor_line("  [](){ #example }"));
        assert!(is_mkdocs_anchor_line("[](){ #example }  "));
        assert!(is_mkdocs_anchor_line("  [](){ #example }  "));
        assert!(is_mkdocs_anchor_line("\t[](){ #example }\t"));

        // Whitespace between []() and {
        assert!(is_mkdocs_anchor_line("[]()  { #example }"));
        assert!(is_mkdocs_anchor_line("[]()\t{ #example }"));

        // No whitespace (compact form)
        assert!(is_mkdocs_anchor_line("[](){#example}"));
    }

    #[test]
    fn test_is_mkdocs_anchor_line_not_anchor_lines() {
        // Empty link without attr_list
        assert!(!is_mkdocs_anchor_line("[]()"));

        // Empty attr_list (no ID or class)
        assert!(!is_mkdocs_anchor_line("[](){ }"));
        assert!(!is_mkdocs_anchor_line("[](){}"));

        // Regular link with URL
        assert!(!is_mkdocs_anchor_line("[](url)"));
        assert!(!is_mkdocs_anchor_line("[text](url)"));
        assert!(!is_mkdocs_anchor_line("[text](url){ #id }"));

        // Trailing content after attr_list
        assert!(!is_mkdocs_anchor_line("[](){ #anchor } extra text"));
        assert!(!is_mkdocs_anchor_line("[](){ #anchor } <!-- comment -->"));

        // Leading content before link
        assert!(!is_mkdocs_anchor_line("text [](){ #anchor }"));
        assert!(!is_mkdocs_anchor_line("# Heading [](){ #anchor }"));

        // Not a link at all
        assert!(!is_mkdocs_anchor_line("# Heading"));
        assert!(!is_mkdocs_anchor_line("Some paragraph text"));
        assert!(!is_mkdocs_anchor_line("{ #standalone-attr }"));

        // Malformed patterns
        assert!(!is_mkdocs_anchor_line("[]{#anchor}"));   // Missing ()
        assert!(!is_mkdocs_anchor_line("[](#anchor)"));   // ID in URL position
        assert!(!is_mkdocs_anchor_line("[](){ #anchor")); // Unclosed brace
    }

    #[test]
    fn test_is_mkdocs_anchor_line_edge_cases() {
        // Empty line
        assert!(!is_mkdocs_anchor_line(""));
        assert!(!is_mkdocs_anchor_line("   "));
        assert!(!is_mkdocs_anchor_line("\t"));

        // Only braces
        assert!(!is_mkdocs_anchor_line("{}"));
        assert!(!is_mkdocs_anchor_line("{ }"));

        // Key-value attributes (valid in MkDocs but unusual for anchors)
        assert!(is_mkdocs_anchor_line("[](){ #id data-value=\"test\" }"));

        // Multiple IDs (first one wins per HTML spec, but pattern is valid)
        assert!(is_mkdocs_anchor_line("[](){ #first #second }"));

        // Unicode in ID (should work per attr_list spec)
        // Note: depends on regex pattern supporting unicode identifiers
    }

    #[test]
    fn test_is_mkdocs_anchor_line_real_world_examples() {
        // Examples from MkDocs Material documentation
        assert!(is_mkdocs_anchor_line("[](){ #installation }"));
        assert!(is_mkdocs_anchor_line("[](){ #getting-started }"));
        assert!(is_mkdocs_anchor_line("[](){ #api-reference }"));

        // Examples with styling classes
        assert!(is_mkdocs_anchor_line("[](){ .annotate }"));
        assert!(is_mkdocs_anchor_line("[](){ #note .warning }"));
    }
}
