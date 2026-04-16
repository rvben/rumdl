//! MkDocs PyMdown extensions support
//!
//! This module provides support for various PyMdown Markdown extensions
//! commonly used with MkDocs Material:
//!
//! - **InlineHilite**: Inline code highlighting `` `#!python code` ``
//! - **Keys**: Keyboard key notation `++ctrl+alt+delete++`
//! - **Caret**: Superscript and insert `^superscript^` and `^^insert^^`
//! - **Tilde**: Subscript and strikethrough `~subscript~` and `~~strike~~`
//! - **Mark**: Highlight text `==highlighted==`
//! - **SmartSymbols**: Auto-replace symbols `(c)` → `©`
//!
//! ## Architecture
//!
//! All markup detection follows a consistent span-based pattern:
//! 1. `find_*_spans(line) -> Vec<(usize, usize)>` - find byte ranges
//! 2. `is_in_*(line, position) -> bool` - check if position is inside markup
//!
//! For double-takes-precedence patterns (caret: ^^/^, tilde: ~~/~):
//! - Double-delimiter spans are found first
//! - Single-delimiter spans exclude positions inside double spans
//!
//! ## References
//!
//! - [PyMdown Extensions](https://facelessuser.github.io/pymdown-extensions/)

use regex::Regex;
use std::sync::LazyLock;

// ============================================================================
// Core span utilities
// ============================================================================

/// Check if a byte position falls within any span.
/// Assumes spans are sorted by start position for early-exit optimization.
#[inline]
fn position_in_spans(position: usize, spans: &[(usize, usize)]) -> bool {
    for &(start, end) in spans {
        if position < start {
            return false;
        }
        if position < end {
            return true;
        }
    }
    false
}

/// Find all regex matches as (start, end) byte spans.
#[inline]
fn find_regex_spans(line: &str, pattern: &Regex) -> Vec<(usize, usize)> {
    pattern.find_iter(line).map(|m| (m.start(), m.end())).collect()
}

/// Find single-delimiter spans (like `~sub~` or `^super^`) that are NOT inside
/// double-delimiter spans (like `~~strike~~` or `^^insert^^`).
///
/// Rules for single-delimiter content:
/// - Must have at least one character between delimiters
/// - Cannot contain whitespace (per PyMdown spec)
/// - Cannot be inside a double-delimiter span
fn find_single_delim_spans(line: &str, delim: char, double_spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut chars = line.char_indices().peekable();
    let delim_len = delim.len_utf8();

    while let Some((start_byte, ch)) = chars.next() {
        // Skip if inside a double-delimiter span
        if position_in_spans(start_byte, double_spans) {
            continue;
        }

        if ch != delim {
            continue;
        }

        // Check if this is a double delimiter (skip it entirely)
        if chars.peek().is_some_and(|(_, c)| *c == delim) {
            chars.next();
            continue;
        }

        // Look for closing single delimiter
        let mut found_content = false;
        let mut has_whitespace = false;

        for (byte_pos, inner_ch) in chars.by_ref() {
            // If we enter a double-delimiter span, stop looking
            if position_in_spans(byte_pos, double_spans) {
                break;
            }

            if inner_ch == delim {
                // Check it's not the start of a double delimiter
                let is_double = chars.peek().is_some_and(|(_, c)| *c == delim);
                if !is_double && found_content && !has_whitespace {
                    spans.push((start_byte, byte_pos + delim_len));
                }
                break;
            }

            found_content = true;
            if inner_ch.is_whitespace() {
                has_whitespace = true;
            }
        }
    }

    spans
}

// ============================================================================
// InlineHilite: `#!lang code` syntax for inline code with syntax highlighting
// ============================================================================

/// Pattern to match inline hilite shebang at the start of backtick content
static INLINE_HILITE_SHEBANG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#!([a-zA-Z][a-zA-Z0-9_+-]*)").unwrap());

/// Check if code span content starts with InlineHilite shebang
#[inline]
pub fn is_inline_hilite_content(content: &str) -> bool {
    INLINE_HILITE_SHEBANG.is_match(content)
}

// ============================================================================
// Keys: ++key++ syntax for keyboard keys
// ============================================================================

/// Pattern to match keyboard key notation: `++key++` or `++key1+key2++`
static KEYS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+\+([a-zA-Z0-9_-]+(?:\+[a-zA-Z0-9_-]+)*)\+\+").unwrap());

/// Find all keyboard shortcut spans
fn find_keys_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("++") {
        return Vec::new();
    }
    find_regex_spans(line, &KEYS_PATTERN)
}

/// Check if a position in a line is within a keyboard shortcut
fn is_in_keys(line: &str, position: usize) -> bool {
    position_in_spans(position, &find_keys_spans(line))
}

// ============================================================================
// Caret: ^superscript^ and ^^insert^^ syntax
// ============================================================================

/// Pattern to match insert: `^^text^^` (double caret)
/// Handles content with single carets inside (e.g., `^^a^b^^`)
static INSERT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^\^[^\^]+(?:\^[^\^]+)*\^\^").unwrap());

/// Find all insert (^^text^^) spans
fn find_insert_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("^^") {
        return Vec::new();
    }
    find_regex_spans(line, &INSERT_PATTERN)
}

/// Check if a position is within superscript or insert markup
fn is_in_caret_markup(line: &str, position: usize) -> bool {
    if !line.contains('^') {
        return false;
    }
    let insert_spans = find_insert_spans(line);
    if position_in_spans(position, &insert_spans) {
        return true;
    }
    let super_spans = find_single_delim_spans(line, '^', &insert_spans);
    position_in_spans(position, &super_spans)
}

// ============================================================================
// Tilde: ~subscript~ and ~~strikethrough~~ syntax
// ============================================================================

/// Pattern to match strikethrough: `~~text~~` (double tilde)
/// Handles content with single tildes inside (e.g., `~~a~b~~`)
static STRIKETHROUGH_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~[^~]+(?:~[^~]+)*~~").unwrap());

/// Find all strikethrough (~~text~~) spans
fn find_strikethrough_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("~~") {
        return Vec::new();
    }
    find_regex_spans(line, &STRIKETHROUGH_PATTERN)
}

/// Check if a position is within subscript or strikethrough markup
fn is_in_tilde_markup(line: &str, position: usize) -> bool {
    if !line.contains('~') {
        return false;
    }
    let strike_spans = find_strikethrough_spans(line);
    if position_in_spans(position, &strike_spans) {
        return true;
    }
    let sub_spans = find_single_delim_spans(line, '~', &strike_spans);
    position_in_spans(position, &sub_spans)
}

// ============================================================================
// Mark: ==highlighted== syntax
// ============================================================================

/// Pattern to match highlight/mark: `==text==`
static MARK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"==([^=]+)==").unwrap());

/// Find all mark (==text==) spans
fn find_mark_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("==") {
        return Vec::new();
    }
    find_regex_spans(line, &MARK_PATTERN)
}

/// Check if a position is within mark markup
pub fn is_in_mark(line: &str, position: usize) -> bool {
    position_in_spans(position, &find_mark_spans(line))
}

// ============================================================================
// SmartSymbols: (c), (tm), (r), -->, <--, etc.
// ============================================================================

/// Pattern to match any SmartSymbol that might be replaced
static SMART_SYMBOL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\(c\)|\(C\)|\(r\)|\(R\)|\(tm\)|\(TM\)|\(p\)|\.\.\.|-{2,3}|<->|<-|->|<=>|<=|=>|1/4|1/2|3/4|\+-|!=)")
        .unwrap()
});

/// Find all SmartSymbol spans
fn find_smart_symbol_spans(line: &str) -> Vec<(usize, usize)> {
    // Quick rejection checks
    if !line.contains('(')
        && !line.contains("...")
        && !line.contains("--")
        && !line.contains("->")
        && !line.contains("<-")
        && !line.contains("=>")
        && !line.contains("<=")
        && !line.contains("1/")
        && !line.contains("3/")
        && !line.contains("+-")
        && !line.contains("!=")
    {
        return Vec::new();
    }
    find_regex_spans(line, &SMART_SYMBOL_PATTERN)
}

/// Check if a position is at a SmartSymbol
fn is_in_smart_symbol(line: &str, position: usize) -> bool {
    position_in_spans(position, &find_smart_symbol_spans(line))
}

// ============================================================================
// Combined utilities
// ============================================================================

/// Check if a position is within any PyMdown extension markup
pub fn is_in_pymdown_markup(line: &str, position: usize) -> bool {
    is_in_keys(line, position)
        || is_in_caret_markup(line, position)
        || is_in_tilde_markup(line, position)
        || is_in_mark(line, position)
        || is_in_smart_symbol(line, position)
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Core utility tests
    // =========================================================================

    #[test]
    fn test_position_in_spans_empty() {
        assert!(!position_in_spans(0, &[]));
        assert!(!position_in_spans(100, &[]));
    }

    #[test]
    fn test_position_in_spans_early_exit() {
        let spans = [(10, 20), (30, 40)];
        assert!(!position_in_spans(5, &spans)); // Before all spans
        assert!(!position_in_spans(25, &spans)); // Between spans
        assert!(!position_in_spans(50, &spans)); // After all spans
    }

    #[test]
    fn test_position_in_spans_inside() {
        let spans = [(10, 20), (30, 40)];
        assert!(position_in_spans(10, &spans)); // Start of first span
        assert!(position_in_spans(15, &spans)); // Middle of first span
        assert!(position_in_spans(19, &spans)); // End-1 of first span
        assert!(!position_in_spans(20, &spans)); // End of first span (exclusive)
        assert!(position_in_spans(30, &spans)); // Start of second span
    }

    // =========================================================================
    // InlineHilite tests
    // =========================================================================

    #[test]
    fn test_is_inline_hilite_content() {
        assert!(is_inline_hilite_content("#!python print()"));
        assert!(is_inline_hilite_content("#!js code"));

        assert!(!is_inline_hilite_content("regular code"));
        assert!(!is_inline_hilite_content(" #!python with space"));
    }

    // =========================================================================
    // Keys tests
    // =========================================================================

    #[test]
    fn test_is_in_keys() {
        let line = "Press ++ctrl++ here";
        assert!(!is_in_keys(line, 0)); // "P"
        assert!(!is_in_keys(line, 5)); // " "
        assert!(is_in_keys(line, 6)); // first +
        assert!(is_in_keys(line, 10)); // "r"
        assert!(is_in_keys(line, 13)); // last +
        assert!(!is_in_keys(line, 14)); // " "
    }

    // =========================================================================
    // Caret tests
    // =========================================================================

    #[test]
    fn test_is_in_caret_markup() {
        let line = "Text ^super^ here";
        assert!(!is_in_caret_markup(line, 0));
        assert!(is_in_caret_markup(line, 5)); // "^"
        assert!(is_in_caret_markup(line, 8)); // "p"
        assert!(!is_in_caret_markup(line, 13)); // " "

        let line2 = "Text ^^insert^^ here";
        assert!(is_in_caret_markup(line2, 5)); // first ^
        assert!(is_in_caret_markup(line2, 10)); // "e"
    }

    // =========================================================================
    // Tilde tests
    // =========================================================================

    #[test]
    fn test_is_in_tilde_markup() {
        let line = "Text ~sub~ here";
        assert!(!is_in_tilde_markup(line, 0));
        assert!(is_in_tilde_markup(line, 5)); // "~"
        assert!(is_in_tilde_markup(line, 7)); // "u"
        assert!(!is_in_tilde_markup(line, 12)); // " "

        let line2 = "Text ~~strike~~ here";
        assert!(is_in_tilde_markup(line2, 5)); // first ~
        assert!(is_in_tilde_markup(line2, 10)); // "i"
    }

    // =========================================================================
    // Mark tests
    // =========================================================================

    #[test]
    fn test_is_in_mark() {
        let line = "Text ==highlight== more";
        assert!(!is_in_mark(line, 0));
        assert!(is_in_mark(line, 5)); // first =
        assert!(is_in_mark(line, 10)); // "h"
        assert!(!is_in_mark(line, 19)); // " "
    }

    // =========================================================================
    // SmartSymbols tests
    // =========================================================================

    #[test]
    fn test_is_in_smart_symbol() {
        let line = "Copyright (c) text";
        assert!(!is_in_smart_symbol(line, 0));
        assert!(is_in_smart_symbol(line, 10)); // "("
        assert!(is_in_smart_symbol(line, 11)); // "c"
        assert!(is_in_smart_symbol(line, 12)); // ")"
        assert!(!is_in_smart_symbol(line, 14)); // " "
    }

    // =========================================================================
    // Combined tests
    // =========================================================================

    #[test]
    fn test_is_in_pymdown_markup() {
        assert!(is_in_pymdown_markup("++ctrl++", 2));
        assert!(is_in_pymdown_markup("^super^", 1));
        assert!(is_in_pymdown_markup("~sub~", 1));
        assert!(is_in_pymdown_markup("~~strike~~", 2));
        assert!(is_in_pymdown_markup("==mark==", 2));
        assert!(is_in_pymdown_markup("(c)", 1));

        assert!(!is_in_pymdown_markup("plain text", 5));
    }

    #[test]
    fn test_empty_line() {
        assert!(!is_in_pymdown_markup("", 0));
        assert!(!is_in_mark("", 0));
        assert!(!is_inline_hilite_content(""));
    }
}
