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
    /// ::: module.path.ClassName or ::: handler:module.path
    /// Lenient: accepts any non-whitespace after ::: to detect potentially dangerous patterns
    /// Security validation should happen at a different layer (e.g., a specific rule)
    static ref AUTODOC_MARKER: Regex = Regex::new(
        r"^(\s*):::\s+\S+.*$"  // Just need non-whitespace after :::
    ).unwrap();

    /// Pattern to match cross-reference links in various forms
    /// [module.Class][], [text][module.Class], [module.Class]
    static ref CROSSREF_PATTERN: Regex = Regex::new(
        r"\[(?:[^\]]*)\]\[[a-zA-Z_][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*\]|\[[a-zA-Z_][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*\]\[\]"
    ).unwrap();

    /// Pattern to match handler options in YAML format (indented under :::)
    static ref HANDLER_OPTIONS: Regex = Regex::new(
        r"^(\s{4,})\w+:"
    ).unwrap();

    /// Pattern to validate module/class names
    static ref VALID_MODULE_PATH: Regex = Regex::new(
        r"^[a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*$"
    ).unwrap();
}

/// Check if a line is an auto-doc insertion marker
pub fn is_autodoc_marker(line: &str) -> bool {
    // First check with regex
    if !AUTODOC_MARKER.is_match(line) {
        return false;
    }

    // Additional validation: reject obviously malformed paths
    // like consecutive dots (module..Class) which Python/JS would reject
    let trimmed = line.trim();
    if let Some(start) = trimmed.find(":::") {
        let after_marker = &trimmed[start + 3..].trim();
        // Get the module path (first non-whitespace token)
        if let Some(module_path) = after_marker.split_whitespace().next() {
            // Reject paths with consecutive dots/colons or starting/ending with separator
            if module_path.starts_with('.') || module_path.starts_with(':') {
                return false; // Can't start with separator
            }
            if module_path.ends_with('.') || module_path.ends_with(':') {
                return false; // Can't end with separator
            }
            if module_path.contains("..")
                || module_path.contains("::")
                || module_path.contains(".:")
                || module_path.contains(":.")
            {
                return false; // No consecutive separators
            }
        }
    }

    // For a linter, we want to be lenient and detect most autodoc-like syntax
    // even if it contains dangerous or potentially invalid module paths
    // A separate rule can validate and warn about dangerous patterns
    true
}

/// Check if a line contains cross-reference links
pub fn contains_crossref(line: &str) -> bool {
    CROSSREF_PATTERN.is_match(line)
}

/// Get the indentation level of an autodoc marker
pub fn get_autodoc_indent(line: &str) -> Option<usize> {
    if AUTODOC_MARKER.is_match(line) {
        // Use consistent indentation calculation (tabs = 4 spaces)
        return Some(super::mkdocs_common::get_line_indent(line));
    }
    None
}

/// Check if a line is part of autodoc options (YAML format)
pub fn is_autodoc_options(line: &str, base_indent: usize) -> bool {
    // Options must be indented at least 4 spaces more than the ::: marker
    let line_indent = super::mkdocs_common::get_line_indent(line);

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
