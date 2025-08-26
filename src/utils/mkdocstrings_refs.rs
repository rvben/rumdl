/// MkDocstrings cross-references detection utilities
///
/// MkDocstrings provides automatic cross-references to documented code objects
/// using special syntax patterns for Python, JavaScript, and other languages.
///
/// Common patterns:
/// - `::: module.Class` - Auto-doc insertion
/// - `[module.Class][]` - Cross-reference link
/// - `[text][module.Class]` - Cross-reference with custom text
/// - `::: module.Class` with options block (YAML indented)
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern to match auto-doc insertion markers
    /// ::: module.path.ClassName
    static ref AUTODOC_MARKER: Regex = Regex::new(
        r"^(\s*):::\s+[\w.]+(?::[\w.]+)?\s*$"
    ).unwrap();

    /// Pattern to match cross-reference links in various forms
    /// [module.Class][], [text][module.Class], [module.Class]
    static ref CROSSREF_PATTERN: Regex = Regex::new(
        r"\[(?:[^\]]*)\]\[[\w.:]+\]|\[[\w.:]+\]\[\]"
    ).unwrap();

    /// Pattern to match handler options in YAML format (indented under :::)
    static ref HANDLER_OPTIONS: Regex = Regex::new(
        r"^(\s{4,})\w+:"
    ).unwrap();
}

/// Check if a line is an auto-doc insertion marker
pub fn is_autodoc_marker(line: &str) -> bool {
    AUTODOC_MARKER.is_match(line)
}

/// Check if a line contains cross-reference links
pub fn contains_crossref(line: &str) -> bool {
    CROSSREF_PATTERN.is_match(line)
}

/// Get the indentation level of an autodoc marker
pub fn get_autodoc_indent(line: &str) -> Option<usize> {
    if let Some(caps) = AUTODOC_MARKER.captures(line)
        && let Some(indent) = caps.get(1)
    {
        return Some(indent.as_str().len());
    }
    None
}

/// Check if a line is part of autodoc options (YAML format)
pub fn is_autodoc_options(line: &str, base_indent: usize) -> bool {
    // Options must be indented at least 4 spaces more than the ::: marker
    let line_indent = line.chars().take_while(|&c| c == ' ' || c == '\t').count();

    // Empty lines within options are allowed
    if line.trim().is_empty() {
        return true;
    }

    // Check if it looks like YAML options (key: value format)
    if line_indent >= base_indent + 4 && line.contains(':') {
        return true;
    }

    false
}

/// Check if content at a byte position is within an autodoc block
pub fn is_within_autodoc_block(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_autodoc = false;
    let mut autodoc_indent = 0;

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if we're starting an autodoc block
        if is_autodoc_marker(line) {
            in_autodoc = true;
            autodoc_indent = get_autodoc_indent(line).unwrap_or(0);
        } else if in_autodoc {
            // Check if we're still in autodoc options
            if !is_autodoc_options(line, autodoc_indent) && !line.trim().is_empty() {
                // Non-option line that's not empty ends the autodoc block
                in_autodoc = false;
                autodoc_indent = 0;
            }
        }

        // Check if the position is within this line and we're in an autodoc block
        if byte_pos <= position && position <= line_end && in_autodoc {
            return true;
        }

        // Account for newline character
        byte_pos = line_end + 1;
    }

    false
}

/// Check if a reference should be treated as a cross-reference (not a broken link)
pub fn is_valid_crossref(ref_text: &str) -> bool {
    // Cross-references typically follow module.Class or module:function patterns
    // They often contain dots or colons
    ref_text.contains('.') || ref_text.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autodoc_marker_detection() {
        assert!(is_autodoc_marker("::: mymodule.MyClass"));
        assert!(is_autodoc_marker("::: package.module.Class"));
        assert!(is_autodoc_marker("  ::: indented.Class"));
        assert!(is_autodoc_marker("::: module:function"));
        assert!(!is_autodoc_marker(":: Wrong number"));
        assert!(!is_autodoc_marker("Regular text"));
    }

    #[test]
    fn test_crossref_detection() {
        assert!(contains_crossref("See [module.Class][]"));
        assert!(contains_crossref("The [text][module.Class] here"));
        assert!(contains_crossref("[package.module.Class][]"));
        assert!(contains_crossref("[custom text][module:function]"));
        assert!(!contains_crossref("Regular [link](url)"));
        assert!(!contains_crossref("No references here"));
    }

    #[test]
    fn test_autodoc_options() {
        assert!(is_autodoc_options("    handler: python", 0));
        assert!(is_autodoc_options("    options:", 0));
        assert!(is_autodoc_options("      show_source: true", 0));
        assert!(is_autodoc_options("", 0)); // Empty lines allowed
        assert!(!is_autodoc_options("Not indented", 0));
        assert!(!is_autodoc_options("  Only 2 spaces", 0));
    }

    #[test]
    fn test_within_autodoc_block() {
        let content = r#"# API Documentation

::: mymodule.MyClass
    handler: python
    options:
      show_source: true
      show_root_heading: true

Regular text here.

::: another.Class

More text."#;

        let handler_pos = content.find("handler:").unwrap();
        let options_pos = content.find("show_source:").unwrap();
        let regular_pos = content.find("Regular text").unwrap();
        let more_pos = content.find("More text").unwrap();

        assert!(is_within_autodoc_block(content, handler_pos));
        assert!(is_within_autodoc_block(content, options_pos));
        assert!(!is_within_autodoc_block(content, regular_pos));
        assert!(!is_within_autodoc_block(content, more_pos));
    }

    #[test]
    fn test_valid_crossref() {
        assert!(is_valid_crossref("module.Class"));
        assert!(is_valid_crossref("package.module.Class"));
        assert!(is_valid_crossref("module:function"));
        assert!(is_valid_crossref("numpy.ndarray"));
        assert!(!is_valid_crossref("simple_word"));
        assert!(!is_valid_crossref("no-dots-here"));
    }
}
