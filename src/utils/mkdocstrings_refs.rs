use regex::Regex;
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
use std::sync::LazyLock;

/// Pre-filter regex for auto-doc insertion markers.
/// Matches any `:::` followed by non-whitespace. The actual validation
/// (requiring `.` or `:` separators, rejecting Pandoc syntax) happens
/// in `is_autodoc_marker()`.
static AUTODOC_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\s*):::\s+\S+.*$", // Pre-filter: any non-whitespace after :::
    )
    .unwrap()
});

/// Pattern to match cross-reference links in various forms
/// [module.Class][], [text][module.Class], [module.Class]
static CROSSREF_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[(?:[^\]]*)\]\[[a-zA-Z_][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*\]|\[[a-zA-Z_][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*\]\[\]"
    ).unwrap()
});

/// Check if a line is an auto-doc insertion marker
///
/// Matches mkdocstrings syntax `::: module.Class` but NOT Pandoc fenced divs
/// like `::: warning` or `::: {.note}`. The key distinction is that autodoc
/// paths contain at least one `.` or `:` separator (e.g., `package.module`,
/// `handler:path`), while Pandoc divs use plain words or `{}`-wrapped classes.
pub fn is_autodoc_marker(line: &str) -> bool {
    // First check with regex
    if !AUTODOC_MARKER.is_match(line) {
        return false;
    }

    let trimmed = line.trim();
    if let Some(start) = trimmed.find(":::") {
        let after_marker = &trimmed[start + 3..].trim();
        // Get the module path (first non-whitespace token)
        if let Some(module_path) = after_marker.split_whitespace().next() {
            // Reject Pandoc attribute syntax: ::: {.note}, ::: {#id .class}
            if module_path.starts_with('{') {
                return false;
            }

            // Require at least one `.` or `:` separator to distinguish module
            // paths (package.module.Class, handler:module) from Pandoc fenced
            // div names (warning, note, danger)
            if !module_path.contains('.') && !module_path.contains(':') {
                return false;
            }

            // Reject malformed paths: can't start/end with separator
            if module_path.starts_with('.') || module_path.starts_with(':') {
                return false;
            }
            if module_path.ends_with('.') || module_path.ends_with(':') {
                return false;
            }
            // Reject consecutive separators (module..Class, handler::path)
            if module_path.contains("..")
                || module_path.contains("::")
                || module_path.contains(".:")
                || module_path.contains(":.")
            {
                return false;
            }
        }
    }

    true
}

/// Check if a line contains cross-reference links
pub fn contains_crossref(line: &str) -> bool {
    CROSSREF_PATTERN.is_match(line)
}

/// Get the indentation level of an autodoc marker
pub fn get_autodoc_indent(line: &str) -> Option<usize> {
    if is_autodoc_marker(line) {
        return Some(super::mkdocs_common::get_line_indent(line));
    }
    None
}

/// Check if a line is part of autodoc options (YAML format)
pub fn is_autodoc_options(line: &str, base_indent: usize) -> bool {
    // Options must be indented at least 4 spaces more than the ::: marker
    let line_indent = super::mkdocs_common::get_line_indent(line);

    // Check if properly indented (at least 4 spaces from base)
    if line_indent >= base_indent + 4 {
        // Empty lines that are properly indented are considered part of options
        if line.trim().is_empty() {
            return true;
        }

        // YAML key-value pairs
        if line.contains(':') {
            return true;
        }
        // YAML list items
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            return true;
        }
    }

    false
}

/// Pre-compute all autodoc block ranges in the content
/// Returns a sorted vector of byte ranges for efficient lookup
pub fn detect_autodoc_block_ranges(content: &str) -> Vec<crate::utils::skip_context::ByteRange> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_autodoc = false;
    let mut autodoc_indent = 0;
    let mut block_start = 0;

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if we're starting an autodoc block
        if is_autodoc_marker(line) {
            in_autodoc = true;
            autodoc_indent = get_autodoc_indent(line).unwrap_or(0);
            block_start = byte_pos;
        } else if in_autodoc {
            // Check if we're still in autodoc options
            if is_autodoc_options(line, autodoc_indent) {
                // Continue in autodoc block
            } else {
                // Not part of options - check if this ends the block
                // Completely empty lines (no indentation) don't end the block
                if line.is_empty() {
                    // Continue in autodoc
                } else {
                    // Non-option, non-empty line ends the autodoc block
                    // Save the range up to the previous line
                    ranges.push(crate::utils::skip_context::ByteRange {
                        start: block_start,
                        end: byte_pos.saturating_sub(1), // Don't include the newline before this line
                    });
                    in_autodoc = false;
                    autodoc_indent = 0;
                }
            }
        }

        // Account for newline character
        byte_pos = line_end + 1;
    }

    // If we ended while still in an autodoc block, save it
    if in_autodoc {
        ranges.push(crate::utils::skip_context::ByteRange {
            start: block_start,
            end: byte_pos.saturating_sub(1),
        });
    }

    ranges
}

/// Check if a position is within any of the pre-computed autodoc block ranges
pub fn is_within_autodoc_block_ranges(ranges: &[crate::utils::skip_context::ByteRange], position: usize) -> bool {
    crate::utils::skip_context::is_in_html_comment_ranges(ranges, position)
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
        // Valid mkdocstrings autodoc markers (dotted or colon-separated paths)
        assert!(is_autodoc_marker("::: mymodule.MyClass"));
        assert!(is_autodoc_marker("::: package.module.Class"));
        assert!(is_autodoc_marker("  ::: indented.Class"));
        assert!(is_autodoc_marker("::: module:function"));
        assert!(is_autodoc_marker("::: handler:package.module"));
        assert!(is_autodoc_marker("::: a.b"));

        // Not autodoc: wrong syntax
        assert!(!is_autodoc_marker(":: Wrong number"));
        assert!(!is_autodoc_marker("Regular text"));
        assert!(!is_autodoc_marker(":::"));
        assert!(!is_autodoc_marker(":::    "));

        // Not autodoc: Pandoc fenced divs (plain words, no separator)
        assert!(!is_autodoc_marker("::: warning"));
        assert!(!is_autodoc_marker("::: note"));
        assert!(!is_autodoc_marker("::: danger"));
        assert!(!is_autodoc_marker("::: sidebar"));
        assert!(!is_autodoc_marker("  ::: callout"));

        // Not autodoc: Pandoc attribute syntax
        assert!(!is_autodoc_marker("::: {.note}"));
        assert!(!is_autodoc_marker("::: {#myid .warning}"));
        assert!(!is_autodoc_marker("::: {.note .important}"));

        // Not autodoc: malformed paths
        assert!(!is_autodoc_marker("::: .starts.with.dot"));
        assert!(!is_autodoc_marker("::: ends.with.dot."));
        assert!(!is_autodoc_marker("::: has..consecutive.dots"));
        assert!(!is_autodoc_marker("::: :starts.with.colon"));
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
        assert!(!is_autodoc_options("", 0)); // Empty lines are neutral
        assert!(!is_autodoc_options("Not indented", 0));
        assert!(!is_autodoc_options("  Only 2 spaces", 0));
        // Test YAML list items
        assert!(is_autodoc_options("            - window", 0));
        assert!(is_autodoc_options("            - app", 0));
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
