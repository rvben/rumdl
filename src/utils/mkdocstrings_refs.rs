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

use super::mkdocs_common::get_line_indent;
use crate::config::MarkdownFlavor;
use crate::utils::skip_context::ByteRange;

/// Pre-filter regex for auto-doc insertion markers.
/// Matches any `:::` followed by non-whitespace. Which identifiers count as
/// auto-doc depends on the flavor and the following lines; see
/// `opens_autodoc_block()`.
static AUTODOC_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\s*):::\s+\S+.*$", // Pre-filter: any non-whitespace after :::
    )
    .unwrap()
});

/// mkdocstrings configuration keys that can open the YAML block after a marker.
/// mkdocstrings itself recognises these two when the block follows a blank line.
const OPTIONS_BLOCK_KEYS: [&str; 2] = ["handler:", "options:"];

/// The identifier after an auto-doc marker, read the way mkdocstrings reads it:
/// everything after `::: `. Pandoc attribute syntax (`::: {.note}`) is never one.
fn marker_identifier(line: &str) -> Option<&str> {
    if !AUTODOC_MARKER.is_match(line) {
        return None;
    }
    let identifier = line.trim_start().strip_prefix(":::")?.trim();
    (!identifier.starts_with('{')).then_some(identifier)
}

/// Check if a line is an auto-doc marker on its own, without context
///
/// Matches mkdocstrings syntax `::: module.Class` but NOT Pandoc fenced divs
/// like `::: warning` or `::: {.note}`. The key distinction is that autodoc
/// paths contain at least one `.` or `:` separator (e.g., `package.module`,
/// `handler:path`), while Pandoc divs use plain words or `{}`-wrapped classes.
/// A marker this rejects can still open an auto-doc block, depending on the
/// flavor and the lines after it; see `detect_autodoc_block_ranges()`.
pub fn is_autodoc_marker(line: &str) -> bool {
    let Some(identifier) = marker_identifier(line) else {
        return false;
    };
    // The module path is the first token; the regex guarantees there is one
    let module_path = identifier.split_whitespace().next().unwrap_or_default();

    // Require at least one `.` or `:` separator to distinguish module paths
    // (package.module.Class, handler:module) from Pandoc fenced div names
    // (warning, note, danger)
    if !module_path.contains(['.', ':']) {
        return false;
    }

    // Reject malformed paths: a separator at either end, or two in a row
    // (module..Class, handler::path)
    if module_path.starts_with(['.', ':']) || module_path.ends_with(['.', ':']) {
        return false;
    }
    !["..", "::", ".:", ":."].iter().any(|pair| module_path.contains(pair))
}

/// Whether `lines[idx]` opens an mkdocstrings auto-doc block under `flavor`
///
/// mkdocstrings accepts any identifier after `::: `, including a top-level
/// package with no separator (`::: mypackage`). Outside MkDocs a single word
/// also reads as a Pandoc fenced div (`::: warning`), so a marker is accepted
/// when its path is self-evidently a module path, when the flavor is MkDocs
/// (which has no fenced divs), or when the options block mkdocstrings reads
/// follows it. The Pandoc flavors accept only the path form, since there a
/// single word always names a div.
fn opens_autodoc_block(lines: &[&str], idx: usize, flavor: MarkdownFlavor) -> bool {
    let line = lines[idx];
    if is_autodoc_marker(line) {
        return true;
    }
    if marker_identifier(line).is_none() || flavor.is_pandoc_compatible() {
        return false;
    }
    flavor == MarkdownFlavor::MkDocs || starts_options_block(&lines[idx + 1..], get_line_indent(line))
}

/// Whether `following` begins with the YAML block mkdocstrings reads as a
/// marker's configuration: a `handler:` or `options:` key indented at least
/// four columns past the marker, on the next line or after one blank line.
fn starts_options_block(following: &[&str], marker_indent: usize) -> bool {
    let mut rest = following.iter();
    let first = match rest.next() {
        Some(line) if line.trim().is_empty() => rest.next(),
        other => other,
    };
    first.is_some_and(|line| {
        let key = line.trim_start();
        get_line_indent(line) >= marker_indent + 4 && OPTIONS_BLOCK_KEYS.iter().any(|k| key.starts_with(k))
    })
}

/// Check if a line is part of autodoc options (YAML format)
pub fn is_autodoc_options(line: &str, base_indent: usize) -> bool {
    // Options must be indented at least 4 spaces more than the ::: marker
    let line_indent = get_line_indent(line);

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

/// Pre-compute all autodoc block ranges in the content for `flavor`
/// Returns a sorted vector of byte ranges for efficient lookup
pub fn detect_autodoc_block_ranges(content: &str, flavor: MarkdownFlavor) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    // Start of the open block, and the indentation of its marker
    let mut open_block: Option<(usize, usize)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_start = byte_pos;
        // Account for the newline character
        byte_pos += line.len() + 1;

        if opens_autodoc_block(&lines, idx, flavor) {
            open_block = Some((line_start, get_line_indent(line)));
        } else if let Some((start, marker_indent)) = open_block {
            // Option lines and blank lines, at any indentation, continue the
            // block; any other line ends it before that line's newline
            if !is_autodoc_options(line, marker_indent) && !line.trim().is_empty() {
                ranges.push(ByteRange {
                    start,
                    end: line_start.saturating_sub(1),
                });
                open_block = None;
            }
        }
    }

    // If we ended while still in an autodoc block, save it
    if let Some((start, _)) = open_block {
        ranges.push(ByteRange {
            start,
            end: byte_pos.saturating_sub(1),
        });
    }

    ranges
}

/// Check if a position is within any of the pre-computed autodoc block ranges
pub fn is_within_autodoc_block_ranges(ranges: &[ByteRange], position: usize) -> bool {
    crate::utils::skip_context::is_in_html_comment_ranges(ranges, position)
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

    /// 1-indexed lines of `content` that fall inside a detected auto-doc block
    fn autodoc_lines(content: &str, flavor: MarkdownFlavor) -> Vec<usize> {
        let ranges = detect_autodoc_block_ranges(content, flavor);
        let mut offset = 0;
        let mut lines = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if is_within_autodoc_block_ranges(&ranges, offset) {
                lines.push(idx + 1);
            }
            offset += line.len() + 1;
        }
        lines
    }

    #[test]
    fn test_single_word_marker_with_options_block() {
        let content = "::: mypackage\n    options:\n      show_source: false\nText\n";
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::Obsidian,
        ] {
            assert_eq!(autodoc_lines(content, flavor), vec![1, 2, 3], "{flavor:?}");
        }
        // A single word after `:::` names a fenced div in the Pandoc flavors
        for flavor in [MarkdownFlavor::Pandoc, MarkdownFlavor::Quarto] {
            assert!(autodoc_lines(content, flavor).is_empty(), "{flavor:?}");
        }
    }

    #[test]
    fn test_options_block_after_one_blank_line() {
        let content = "::: mypackage\n\n    handler: python\nText\n";
        assert_eq!(autodoc_lines(content, MarkdownFlavor::Standard), vec![1, 2, 3]);
        // Two blank lines separate the options from the marker in mkdocstrings too
        let content = "::: mypackage\n\n\n    handler: python\n";
        assert!(autodoc_lines(content, MarkdownFlavor::Standard).is_empty());
    }

    #[test]
    fn test_whitespace_only_blank_line_keeps_block_open() {
        // A blank line holding only spaces separates the options like an empty one
        let content = "::: mypackage\n  \n    handler: python\nText\n";
        assert_eq!(autodoc_lines(content, MarkdownFlavor::Standard), vec![1, 2, 3]);
        let content = "::: pkg.mod\n  \n    options:\n      x: 1\nText\n";
        assert_eq!(autodoc_lines(content, MarkdownFlavor::Standard), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_pandoc_flavors_recognize_only_path_markers() {
        // A dotted path is never a div class, so the Pandoc flavors keep it
        let content = "::: module.Class\n    options:\n      x: 1\nText\n";
        for flavor in [MarkdownFlavor::Pandoc, MarkdownFlavor::Quarto] {
            assert_eq!(autodoc_lines(content, flavor), vec![1, 2, 3], "{flavor:?}");
        }
    }

    #[test]
    fn test_single_word_marker_needs_options_key_outside_mkdocs() {
        // No options block: a plain Pandoc-style div name
        assert!(autodoc_lines("::: warning\nText\n", MarkdownFlavor::Standard).is_empty());
        // An indented key mkdocstrings does not read as configuration
        let content = "::: sidebar\n    Important: note\n";
        assert!(autodoc_lines(content, MarkdownFlavor::Standard).is_empty());
        // The key must be indented four columns past the marker
        let content = "  ::: mypackage\n    options:\n";
        assert!(autodoc_lines(content, MarkdownFlavor::Standard).is_empty());
        // Attribute syntax is never an identifier, even with an options key after it
        let content = "::: {.note}\n    options:\n";
        assert!(autodoc_lines(content, MarkdownFlavor::Standard).is_empty());
        assert!(autodoc_lines(content, MarkdownFlavor::MkDocs).is_empty());
    }

    #[test]
    fn test_mkdocs_flavor_accepts_any_identifier() {
        assert_eq!(autodoc_lines("::: mypackage\nText\n", MarkdownFlavor::MkDocs), vec![1]);
        let content = "::: handler: python\n    options:\n      show_source: false\n";
        assert_eq!(autodoc_lines(content, MarkdownFlavor::MkDocs), vec![1, 2, 3]);
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
}
