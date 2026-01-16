/// MkDocs PyMdown extensions support
///
/// This module provides support for various PyMdown Markdown extensions
/// commonly used with MkDocs Material:
///
/// - **InlineHilite**: Inline code highlighting `` `#!python code` ``
/// - **Keys**: Keyboard key notation `++ctrl+alt+delete++`
/// - **Caret**: Superscript and insert `^superscript^` and `^^insert^^`
/// - **Mark**: Highlight text `==highlighted==`
/// - **SmartSymbols**: Auto-replace symbols `(c)` → `©`
///
/// ## References
///
/// - [PyMdown Extensions](https://facelessuser.github.io/pymdown-extensions/)
/// - [InlineHilite](https://facelessuser.github.io/pymdown-extensions/extensions/inlinehilite/)
/// - [Keys](https://facelessuser.github.io/pymdown-extensions/extensions/keys/)
/// - [Caret](https://facelessuser.github.io/pymdown-extensions/extensions/caret/)
/// - [Mark](https://facelessuser.github.io/pymdown-extensions/extensions/mark/)
/// - [SmartSymbols](https://facelessuser.github.io/pymdown-extensions/extensions/smartsymbols/)
use regex::Regex;
use std::sync::LazyLock;

// ============================================================================
// InlineHilite: `#!lang code` syntax for inline code with syntax highlighting
// ============================================================================

/// Pattern to match InlineHilite syntax: `#!language code`
/// Examples: `#!python print("hello")`, `#!js alert('hi')`
static INLINE_HILITE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`#!([a-zA-Z][a-zA-Z0-9_+-]*)\s+[^`]+`").unwrap());

/// Pattern to match inline hilite shebang at the start of backtick content
static INLINE_HILITE_SHEBANG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#!([a-zA-Z][a-zA-Z0-9_+-]*)").unwrap());

/// Check if a line contains InlineHilite syntax
#[inline]
pub fn contains_inline_hilite(line: &str) -> bool {
    if !line.contains('`') || !line.contains("#!") {
        return false;
    }
    INLINE_HILITE_PATTERN.is_match(line)
}

/// Check if code span content starts with InlineHilite shebang
#[inline]
pub fn is_inline_hilite_content(content: &str) -> bool {
    INLINE_HILITE_SHEBANG.is_match(content)
}

// ============================================================================
// Keys: ++key++ syntax for keyboard keys
// ============================================================================

/// Pattern to match keyboard key notation: `++key++` or `++key1+key2++`
/// Examples: ++ctrl++, ++ctrl+alt+delete++, ++cmd+shift+p++
static KEYS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+\+([a-zA-Z0-9_-]+(?:\+[a-zA-Z0-9_-]+)*)\+\+").unwrap());

/// Common keyboard key names for validation
pub const COMMON_KEYS: &[&str] = &[
    "ctrl",
    "alt",
    "shift",
    "cmd",
    "meta",
    "win",
    "windows",
    "option",
    "enter",
    "return",
    "tab",
    "space",
    "backspace",
    "delete",
    "del",
    "insert",
    "ins",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
    "escape",
    "esc",
    "capslock",
    "numlock",
    "scrolllock",
    "printscreen",
    "pause",
    "break",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    // Letters and numbers are also valid
];

/// Parsed keyboard shortcut
#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardShortcut {
    /// The full shortcut text including ++ markers
    pub full_text: String,
    /// Individual keys in the shortcut
    pub keys: Vec<String>,
    /// Start position in the line (0-indexed)
    pub start: usize,
    /// End position in the line (0-indexed, exclusive)
    pub end: usize,
}

/// Check if a line contains keyboard key notation
#[inline]
pub fn contains_keys(line: &str) -> bool {
    if !line.contains("++") {
        return false;
    }
    KEYS_PATTERN.is_match(line)
}

/// Find all keyboard shortcuts in a line
pub fn find_keyboard_shortcuts(line: &str) -> Vec<KeyboardShortcut> {
    if !line.contains("++") {
        return Vec::new();
    }

    let mut results = Vec::new();

    for m in KEYS_PATTERN.find_iter(line) {
        let full_text = m.as_str().to_string();
        // Remove the surrounding ++ and split by +
        let inner = &full_text[2..full_text.len() - 2];
        let keys: Vec<String> = inner.split('+').map(|s| s.to_string()).collect();

        results.push(KeyboardShortcut {
            full_text,
            keys,
            start: m.start(),
            end: m.end(),
        });
    }

    results
}

/// Check if a position in a line is within a keyboard shortcut
pub fn is_in_keys(line: &str, position: usize) -> bool {
    for shortcut in find_keyboard_shortcuts(line) {
        if shortcut.start <= position && position < shortcut.end {
            return true;
        }
    }
    false
}

// ============================================================================
// Caret: ^superscript^ and ^^insert^^ syntax
// ============================================================================

/// Pattern to match insert: `^^text^^` (double caret)
/// Must be checked before superscript since ^^ is more specific
static INSERT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^\^([^\^]+)\^\^").unwrap());

/// Check if a line contains superscript syntax
/// Returns true if there's a single caret pattern that's NOT part of ^^insert^^
#[inline]
pub fn contains_superscript(line: &str) -> bool {
    if !line.contains('^') {
        return false;
    }

    // Mask out insert patterns (^^text^^) first
    let masked = mask_insert_patterns(line);

    // Now check for single caret superscript in the remaining text
    // We need a simple pattern: ^text^ where text doesn't contain ^
    let bytes = masked.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' {
            // Check if this is start of superscript (not masked)
            // Find the closing ^
            if let Some(end) = masked[i + 1..].find('^') {
                let end_pos = i + 1 + end;
                // Check that content between carets is not empty and doesn't contain ^
                let content = &masked[i + 1..end_pos];
                if !content.is_empty() && !content.contains('^') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Mask insert patterns (^^text^^) with spaces to help detect superscript
fn mask_insert_patterns(line: &str) -> String {
    if !line.contains("^^") {
        return line.to_string();
    }

    let mut result = line.to_string();
    for m in INSERT_PATTERN.find_iter(line) {
        let replacement = " ".repeat(m.end() - m.start());
        result.replace_range(m.start()..m.end(), &replacement);
    }
    result
}

/// Check if a line contains insert syntax
#[inline]
pub fn contains_insert(line: &str) -> bool {
    if !line.contains("^^") {
        return false;
    }
    INSERT_PATTERN.is_match(line)
}

/// Check if a position is within superscript or insert markup
pub fn is_in_caret_markup(line: &str, position: usize) -> bool {
    if !line.contains('^') {
        return false;
    }

    // Check insert first (double caret takes precedence)
    for m in INSERT_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }

    // Check superscript - find ^text^ patterns that aren't part of ^^insert^^
    let masked = mask_insert_patterns(line);
    let bytes = masked.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' {
            // Find the closing ^
            if let Some(end) = masked[i + 1..].find('^') {
                let end_pos = i + 1 + end;
                // Check that content between carets is not empty
                let content = &masked[i + 1..end_pos];
                if !content.is_empty() && !content.contains('^') && position >= i && position <= end_pos + 1 {
                    return true;
                }
                // Skip past this pattern
                i = end_pos + 1;
                continue;
            }
        }
        i += 1;
    }

    false
}

// ============================================================================
// Mark: ==highlighted== syntax
// ============================================================================

/// Pattern to match highlight/mark: `==text==`
static MARK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"==([^=]+)==").unwrap());

/// Check if a line contains mark/highlight syntax
#[inline]
pub fn contains_mark(line: &str) -> bool {
    if !line.contains("==") {
        return false;
    }
    MARK_PATTERN.is_match(line)
}

/// Check if a position is within mark markup
pub fn is_in_mark(line: &str, position: usize) -> bool {
    if !line.contains("==") {
        return false;
    }

    for m in MARK_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }

    false
}

// ============================================================================
// SmartSymbols: (c), (tm), (r), -->, <--, etc.
// ============================================================================

/// SmartSymbol patterns and their Unicode replacements
/// Note: This constant is kept for documentation reference but not currently
/// used since we only need to detect patterns, not replace them.
/// Patterns: (c)→©, (r)→®, (tm)→™, ...→…, --→–, ---→—, ->→→, <-→←, etc.
#[allow(dead_code)]
const SMART_SYMBOLS_DOC: &str =
    "(c)©, (r)®, (tm)™, ...→…, --→–, ---→—, <->↔, =>⇒, <=⇐, <=>⇔, 1/4¼, 1/2½, 3/4¾, +-±, !=≠";

/// Pattern to match any SmartSymbol that might be replaced
static SMART_SYMBOL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\(c\)|\(C\)|\(r\)|\(R\)|\(tm\)|\(TM\)|\(p\)|\.\.\.|-{2,3}|<->|<-|->|<=>|<=|=>|1/4|1/2|3/4|\+-|!=)")
        .unwrap()
});

/// Check if a line contains potential SmartSymbol patterns
#[inline]
pub fn contains_smart_symbols(line: &str) -> bool {
    // Quick checks for common patterns
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
        return false;
    }
    SMART_SYMBOL_PATTERN.is_match(line)
}

/// Check if a position is at a SmartSymbol that will be replaced
pub fn is_in_smart_symbol(line: &str, position: usize) -> bool {
    for m in SMART_SYMBOL_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }
    false
}

// ============================================================================
// Combined utilities
// ============================================================================

/// Check if a position is within any PyMdown extension markup
///
/// This includes Keys, Caret (superscript/insert), Mark, and SmartSymbols.
/// InlineHilite is excluded as it uses standard backtick syntax.
pub fn is_in_pymdown_markup(line: &str, position: usize) -> bool {
    is_in_keys(line, position)
        || is_in_caret_markup(line, position)
        || is_in_mark(line, position)
        || is_in_smart_symbol(line, position)
}

/// Mask all PyMdown extension markup with spaces
///
/// Useful for rules that need to process text without being confused
/// by extension syntax.
pub fn mask_pymdown_markup(line: &str) -> String {
    let mut result = line.to_string();

    // Process in specific order to handle overlapping patterns correctly

    // Keys: ++key++
    if line.contains("++") {
        for m in KEYS_PATTERN.find_iter(line) {
            let replacement = " ".repeat(m.end() - m.start());
            // We need to replace at the right position considering previous replacements
            // Since we're replacing with same-length strings, positions stay the same
            result.replace_range(m.start()..m.end(), &replacement);
        }
    }

    // Insert: ^^text^^ (must come before superscript)
    if result.contains("^^") {
        let temp = result.clone();
        for m in INSERT_PATTERN.find_iter(&temp) {
            let replacement = " ".repeat(m.end() - m.start());
            result.replace_range(m.start()..m.end(), &replacement);
        }
    }

    // Superscript: ^text^ - use manual parsing since regex crate doesn't support lookaround
    if result.contains('^') {
        let mut new_result = result.clone();
        let bytes = result.as_bytes();
        let mut superscript_ranges = Vec::new();

        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'^' {
                // Find closing ^
                if let Some(end) = result[i + 1..].find('^') {
                    let end_pos = i + 1 + end;
                    let content = &result[i + 1..end_pos];
                    // Valid superscript if content is not empty and doesn't contain ^
                    if !content.is_empty() && !content.contains('^') {
                        superscript_ranges.push((i, end_pos + 1));
                        i = end_pos + 1;
                        continue;
                    }
                }
            }
            i += 1;
        }

        // Apply masking in reverse order to preserve indices
        for (start, end) in superscript_ranges.into_iter().rev() {
            let replacement = " ".repeat(end - start);
            new_result.replace_range(start..end, &replacement);
        }
        result = new_result;
    }

    // Mark: ==text==
    if result.contains("==") {
        let temp = result.clone();
        for m in MARK_PATTERN.find_iter(&temp) {
            let replacement = " ".repeat(m.end() - m.start());
            result.replace_range(m.start()..m.end(), &replacement);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // InlineHilite tests
    #[test]
    fn test_contains_inline_hilite() {
        assert!(contains_inline_hilite("`#!python print('hello')`"));
        assert!(contains_inline_hilite("Use `#!js alert('hi')` for alerts"));
        assert!(contains_inline_hilite("`#!c++ cout << x;`"));

        // Not InlineHilite
        assert!(!contains_inline_hilite("`regular code`"));
        assert!(!contains_inline_hilite("#! not in backticks"));
        assert!(!contains_inline_hilite("`#!` empty"));
    }

    #[test]
    fn test_is_inline_hilite_content() {
        assert!(is_inline_hilite_content("#!python print()"));
        assert!(is_inline_hilite_content("#!js code"));

        assert!(!is_inline_hilite_content("regular code"));
        assert!(!is_inline_hilite_content(" #!python with space"));
    }

    // Keys tests
    #[test]
    fn test_contains_keys() {
        assert!(contains_keys("Press ++ctrl++ to continue"));
        assert!(contains_keys("++ctrl+alt+delete++"));
        assert!(contains_keys("Use ++cmd+shift+p++ for command palette"));

        assert!(!contains_keys("Use + for addition"));
        assert!(!contains_keys("a++ increment"));
        assert!(!contains_keys("++incomplete"));
    }

    #[test]
    fn test_find_keyboard_shortcuts() {
        let shortcuts = find_keyboard_shortcuts("Press ++ctrl+c++ then ++ctrl+v++");
        assert_eq!(shortcuts.len(), 2);
        assert_eq!(shortcuts[0].keys, vec!["ctrl", "c"]);
        assert_eq!(shortcuts[1].keys, vec!["ctrl", "v"]);

        let shortcuts = find_keyboard_shortcuts("++ctrl+alt+delete++");
        assert_eq!(shortcuts.len(), 1);
        assert_eq!(shortcuts[0].keys, vec!["ctrl", "alt", "delete"]);
    }

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

    // Caret tests
    #[test]
    fn test_contains_superscript() {
        assert!(contains_superscript("E=mc^2^"));
        assert!(contains_superscript("x^n^ power"));

        assert!(!contains_superscript("no caret here"));
        assert!(!contains_superscript("^^insert^^")); // double caret is insert
    }

    #[test]
    fn test_contains_insert() {
        assert!(contains_insert("^^inserted text^^"));
        assert!(contains_insert("Some ^^new^^ text"));

        assert!(!contains_insert("^superscript^"));
        assert!(!contains_insert("no markup"));
    }

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

    // Mark tests
    #[test]
    fn test_contains_mark() {
        assert!(contains_mark("This is ==highlighted== text"));
        assert!(contains_mark("==important=="));

        assert!(!contains_mark("no highlight"));
        assert!(!contains_mark("a == b comparison")); // spaces
    }

    #[test]
    fn test_is_in_mark() {
        let line = "Text ==highlight== more";
        assert!(!is_in_mark(line, 0));
        assert!(is_in_mark(line, 5)); // first =
        assert!(is_in_mark(line, 10)); // "h"
        assert!(!is_in_mark(line, 19)); // " "
    }

    // SmartSymbols tests
    #[test]
    fn test_contains_smart_symbols() {
        assert!(contains_smart_symbols("Copyright (c) 2024"));
        assert!(contains_smart_symbols("This is (tm) trademarked"));
        assert!(contains_smart_symbols("Left arrow <- here"));
        assert!(contains_smart_symbols("Right arrow -> there"));
        assert!(contains_smart_symbols("Em dash --- here"));
        assert!(contains_smart_symbols("Fraction 1/2"));

        assert!(!contains_smart_symbols("No symbols here"));
        assert!(!contains_smart_symbols("(other) parentheses"));
    }

    #[test]
    fn test_is_in_smart_symbol() {
        let line = "Copyright (c) text";
        assert!(!is_in_smart_symbol(line, 0));
        assert!(is_in_smart_symbol(line, 10)); // "("
        assert!(is_in_smart_symbol(line, 11)); // "c"
        assert!(is_in_smart_symbol(line, 12)); // ")"
        assert!(!is_in_smart_symbol(line, 14)); // " "
    }

    // Combined tests
    #[test]
    fn test_is_in_pymdown_markup() {
        assert!(is_in_pymdown_markup("++ctrl++", 2));
        assert!(is_in_pymdown_markup("^super^", 1));
        assert!(is_in_pymdown_markup("==mark==", 2));
        assert!(is_in_pymdown_markup("(c)", 1));

        assert!(!is_in_pymdown_markup("plain text", 5));
    }

    #[test]
    fn test_mask_pymdown_markup() {
        let line = "Press ++ctrl++ and ^super^ with ==mark==";
        let masked = mask_pymdown_markup(line);
        assert!(!masked.contains("++"));
        assert!(!masked.contains("^super^"));
        assert!(!masked.contains("==mark=="));
        assert!(masked.contains("Press"));
        assert!(masked.contains("and"));
        assert!(masked.contains("with"));
        // Length should be preserved
        assert_eq!(masked.len(), line.len());
    }
}
