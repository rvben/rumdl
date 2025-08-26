/// Common utilities and constants for MkDocs pattern detection
///
/// This module provides shared functionality used across all MkDocs feature
/// detection modules to reduce code duplication and improve maintainability.
use crate::config::MarkdownFlavor;

/// Standard indentation size for MkDocs content blocks
/// Most MkDocs features require content to be indented by 4 spaces
pub const MKDOCS_CONTENT_INDENT: usize = 4;

/// Maximum reasonable length for references and identifiers
pub const MAX_REFERENCE_LENGTH: usize = 200;

/// Maximum reasonable length for individual path components
pub const MAX_COMPONENT_LENGTH: usize = 50;

/// Trait for MkDocs pattern detection implementations
/// All MkDocs features should implement this trait for consistency
pub trait MkDocsPattern: Send + Sync {
    /// Check if a line matches the pattern's start marker
    fn is_marker(&self, line: &str) -> bool;

    /// Get the base indentation level of a marker line
    fn get_indent(&self, line: &str) -> Option<usize>;

    /// Check if a line is part of the pattern's content area
    fn is_content(&self, line: &str, base_indent: usize) -> bool;

    /// Check if a byte position is within this pattern's context
    fn is_within_context(&self, content: &str, position: usize) -> bool;

    /// Get a descriptive name for this pattern (for debugging)
    fn name(&self) -> &'static str;
}

/// Utility for tracking byte positions through document lines
/// Reduces duplication of line-by-line byte position tracking logic
pub struct BytePositionTracker<'a> {
    pub content: &'a str,
    pub lines: Vec<&'a str>,
}

impl<'a> BytePositionTracker<'a> {
    /// Create a new byte position tracker for the given content
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            lines: content.lines().collect(),
        }
    }

    /// Iterate through lines with byte position tracking
    /// Returns an iterator of (line_index, line_content, byte_start, byte_end)
    pub fn iter_with_positions(&self) -> impl Iterator<Item = (usize, &'a str, usize, usize)> + '_ {
        let mut byte_pos = 0;
        self.lines.iter().enumerate().map(move |(idx, line)| {
            let start = byte_pos;
            let end = byte_pos + line.len();
            byte_pos = end + 1; // Account for newline
            (idx, *line, start, end)
        })
    }

    /// Check if a position falls within any line matching the given predicate
    pub fn is_position_in_matching_lines<F>(&self, position: usize, predicate: F) -> bool
    where
        F: Fn(usize, &str) -> bool,
    {
        for (idx, line, start, end) in self.iter_with_positions() {
            if start <= position && position <= end && predicate(idx, line) {
                return true;
            }
        }
        false
    }
}

/// Check if we should process MkDocs patterns for the given flavor
#[inline]
pub fn should_check_mkdocs(flavor: MarkdownFlavor) -> bool {
    matches!(flavor, MarkdownFlavor::MkDocs)
}

/// Extract indentation from a line (counts spaces and tabs)
pub fn get_line_indent(line: &str) -> usize {
    line.chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 }) // Treat tabs as 4 spaces
        .sum()
}

/// Check if a line is indented enough to be content
pub fn is_indented_content(line: &str, base_indent: usize, required_indent: usize) -> bool {
    // Empty lines are handled separately by callers
    if line.trim().is_empty() {
        return false;
    }

    get_line_indent(line) >= base_indent + required_indent
}

/// State machine for tracking nested context boundaries
pub struct ContextStateMachine {
    in_context: bool,
    context_indent: usize,
    context_type: Option<String>,
}

impl ContextStateMachine {
    pub fn new() -> Self {
        Self {
            in_context: false,
            context_indent: 0,
            context_type: None,
        }
    }

    /// Enter a new context with the given indentation and type
    pub fn enter_context(&mut self, indent: usize, context_type: String) {
        self.in_context = true;
        self.context_indent = indent;
        self.context_type = Some(context_type);
    }

    /// Exit the current context
    pub fn exit_context(&mut self) {
        self.in_context = false;
        self.context_indent = 0;
        self.context_type = None;
    }

    /// Check if currently in a context
    pub fn is_in_context(&self) -> bool {
        self.in_context
    }

    /// Get the current context indentation
    pub fn context_indent(&self) -> usize {
        self.context_indent
    }

    /// Get the current context type
    pub fn context_type(&self) -> Option<&str> {
        self.context_type.as_deref()
    }
}

impl Default for ContextStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_line_indent() {
        assert_eq!(get_line_indent("no indent"), 0);
        assert_eq!(get_line_indent("  two spaces"), 2);
        assert_eq!(get_line_indent("    four spaces"), 4);
        assert_eq!(get_line_indent("\tone tab"), 4);
        assert_eq!(get_line_indent("\t\ttwo tabs"), 8);
        assert_eq!(get_line_indent("  \tmixed"), 6); // 2 spaces + 1 tab
    }

    #[test]
    fn test_is_indented_content() {
        assert!(is_indented_content("    content", 0, 4));
        assert!(!is_indented_content("  content", 0, 4));
        assert!(is_indented_content("      content", 2, 4));
        assert!(!is_indented_content("", 0, 4)); // Empty line
        assert!(!is_indented_content("   ", 0, 4)); // Only whitespace
    }

    #[test]
    fn test_byte_position_tracker() {
        let content = "line1\nline2\nline3";
        let tracker = BytePositionTracker::new(content);

        let positions: Vec<_> = tracker.iter_with_positions().collect();
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], (0, "line1", 0, 5));
        assert_eq!(positions[1], (1, "line2", 6, 11));
        assert_eq!(positions[2], (2, "line3", 12, 17));
    }

    #[test]
    fn test_position_in_matching_lines() {
        let content = "normal\nspecial\nnormal";
        let tracker = BytePositionTracker::new(content);

        // Position 8 is in "special"
        assert!(tracker.is_position_in_matching_lines(8, |_, line| line == "special"));
        // Position 2 is in "normal"
        assert!(!tracker.is_position_in_matching_lines(2, |_, line| line == "special"));
    }

    #[test]
    fn test_context_state_machine() {
        let mut sm = ContextStateMachine::new();
        assert!(!sm.is_in_context());

        sm.enter_context(4, "admonition".to_string());
        assert!(sm.is_in_context());
        assert_eq!(sm.context_indent(), 4);
        assert_eq!(sm.context_type(), Some("admonition"));

        sm.exit_context();
        assert!(!sm.is_in_context());
        assert_eq!(sm.context_type(), None);
    }
}
