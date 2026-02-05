/// MkDocs PyMdown extensions support
///
/// This module provides support for various PyMdown Markdown extensions
/// commonly used with MkDocs Material:
///
/// - **InlineHilite**: Inline code highlighting `` `#!python code` ``
/// - **Keys**: Keyboard key notation `++ctrl+alt+delete++`
/// - **Caret**: Superscript and insert `^superscript^` and `^^insert^^`
/// - **Tilde**: Subscript and strikethrough `~subscript~` and `~~strike~~`
/// - **Mark**: Highlight text `==highlighted==`
/// - **SmartSymbols**: Auto-replace symbols `(c)` → `©`
///
/// ## Architecture
///
/// All markup detection follows a consistent span-based pattern:
/// 1. `find_*_spans(line) -> Vec<(usize, usize)>` - find byte ranges
/// 2. `contains_*(line) -> bool` - check if markup exists
/// 3. `is_in_*(line, position) -> bool` - check if position is inside markup
///
/// For double-takes-precedence patterns (caret: ^^/^, tilde: ~~/~):
/// - Double-delimiter spans are found first
/// - Single-delimiter spans exclude positions inside double spans
///
/// ## References
///
/// - [PyMdown Extensions](https://facelessuser.github.io/pymdown-extensions/)
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

/// Merge overlapping or adjacent spans. Input must be sorted by start.
fn merge_spans(spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if spans.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::with_capacity(spans.len());
    let mut current = spans[0];

    for &(start, end) in &spans[1..] {
        if start <= current.1 {
            current.1 = current.1.max(end);
        } else {
            merged.push(current);
            current = (start, end);
        }
    }
    merged.push(current);
    merged
}

// ============================================================================
// InlineHilite: `#!lang code` syntax for inline code with syntax highlighting
// ============================================================================

/// Pattern to match InlineHilite syntax: `#!language code`
static INLINE_HILITE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`#!([a-zA-Z][a-zA-Z0-9_+-]*)\s+[^`]+`").unwrap());

/// Pattern to match inline hilite shebang at the start of backtick content
static INLINE_HILITE_SHEBANG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#!([a-zA-Z][a-zA-Z0-9_+-]*)").unwrap());

/// Check if a line contains InlineHilite syntax
#[inline]
pub fn contains_inline_hilite(line: &str) -> bool {
    line.contains('`') && line.contains("#!") && INLINE_HILITE_PATTERN.is_match(line)
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
];

/// Parsed keyboard shortcut
#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardShortcut {
    pub full_text: String,
    pub keys: Vec<String>,
    pub start: usize,
    pub end: usize,
}

/// Find all keyboard shortcut spans
pub fn find_keys_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("++") {
        return Vec::new();
    }
    find_regex_spans(line, &KEYS_PATTERN)
}

/// Check if a line contains keyboard key notation
#[inline]
pub fn contains_keys(line: &str) -> bool {
    line.contains("++") && KEYS_PATTERN.is_match(line)
}

/// Find all keyboard shortcuts in a line
pub fn find_keyboard_shortcuts(line: &str) -> Vec<KeyboardShortcut> {
    if !line.contains("++") {
        return Vec::new();
    }

    KEYS_PATTERN
        .find_iter(line)
        .map(|m| {
            let full_text = m.as_str().to_string();
            let inner = &full_text[2..full_text.len() - 2];
            let keys = inner.split('+').map(String::from).collect();
            KeyboardShortcut {
                full_text,
                keys,
                start: m.start(),
                end: m.end(),
            }
        })
        .collect()
}

/// Check if a position in a line is within a keyboard shortcut
pub fn is_in_keys(line: &str, position: usize) -> bool {
    position_in_spans(position, &find_keys_spans(line))
}

// ============================================================================
// Caret: ^superscript^ and ^^insert^^ syntax
// ============================================================================

/// Pattern to match insert: `^^text^^` (double caret)
/// Handles content with single carets inside (e.g., `^^a^b^^`)
static INSERT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^\^[^\^]+(?:\^[^\^]+)*\^\^").unwrap());

/// Find all insert (^^text^^) spans
pub fn find_insert_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("^^") {
        return Vec::new();
    }
    find_regex_spans(line, &INSERT_PATTERN)
}

/// Find all superscript (^text^) spans, excluding those inside insert spans
pub fn find_superscript_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains('^') {
        return Vec::new();
    }
    let insert_spans = find_insert_spans(line);
    find_single_delim_spans(line, '^', &insert_spans)
}

/// Check if a line contains superscript syntax (^text^ not inside ^^insert^^)
#[inline]
pub fn contains_superscript(line: &str) -> bool {
    !find_superscript_spans(line).is_empty()
}

/// Check if a line contains insert syntax (^^text^^)
#[inline]
pub fn contains_insert(line: &str) -> bool {
    line.contains("^^") && INSERT_PATTERN.is_match(line)
}

/// Check if a position is within superscript or insert markup
pub fn is_in_caret_markup(line: &str, position: usize) -> bool {
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
pub fn find_strikethrough_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("~~") {
        return Vec::new();
    }
    find_regex_spans(line, &STRIKETHROUGH_PATTERN)
}

/// Find all subscript (~text~) spans, excluding those inside strikethrough spans
pub fn find_subscript_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains('~') {
        return Vec::new();
    }
    let strike_spans = find_strikethrough_spans(line);
    find_single_delim_spans(line, '~', &strike_spans)
}

/// Check if a line contains subscript syntax (~text~ not inside ~~strike~~)
#[inline]
pub fn contains_subscript(line: &str) -> bool {
    !find_subscript_spans(line).is_empty()
}

/// Check if a line contains strikethrough syntax (~~text~~)
#[inline]
pub fn contains_strikethrough(line: &str) -> bool {
    line.contains("~~") && STRIKETHROUGH_PATTERN.is_match(line)
}

/// Check if a position is within subscript or strikethrough markup
pub fn is_in_tilde_markup(line: &str, position: usize) -> bool {
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
pub fn find_mark_spans(line: &str) -> Vec<(usize, usize)> {
    if !line.contains("==") {
        return Vec::new();
    }
    find_regex_spans(line, &MARK_PATTERN)
}

/// Check if a line contains mark/highlight syntax
#[inline]
pub fn contains_mark(line: &str) -> bool {
    line.contains("==") && MARK_PATTERN.is_match(line)
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
pub fn find_smart_symbol_spans(line: &str) -> Vec<(usize, usize)> {
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

/// Check if a line contains potential SmartSymbol patterns
#[inline]
pub fn contains_smart_symbols(line: &str) -> bool {
    !find_smart_symbol_spans(line).is_empty()
}

/// Check if a position is at a SmartSymbol
pub fn is_in_smart_symbol(line: &str, position: usize) -> bool {
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

/// Mask all PyMdown extension markup with spaces (single-pass)
///
/// This function collects all markup spans and replaces them with spaces
/// in a single pass, preserving string length for position-based operations.
pub fn mask_pymdown_markup(line: &str) -> String {
    // Collect all spans to mask
    let mut all_spans: Vec<(usize, usize)> = Vec::new();

    // Keys
    all_spans.extend(find_keys_spans(line));

    // Caret: insert and superscript
    if line.contains('^') {
        let insert_spans = find_insert_spans(line);
        let super_spans = find_single_delim_spans(line, '^', &insert_spans);
        all_spans.extend(insert_spans);
        all_spans.extend(super_spans);
    }

    // Tilde: strikethrough and subscript
    if line.contains('~') {
        let strike_spans = find_strikethrough_spans(line);
        let sub_spans = find_single_delim_spans(line, '~', &strike_spans);
        all_spans.extend(strike_spans);
        all_spans.extend(sub_spans);
    }

    // Mark
    all_spans.extend(find_mark_spans(line));

    // Early return if nothing to mask
    if all_spans.is_empty() {
        return line.to_string();
    }

    // Sort by start position and merge overlapping spans
    all_spans.sort_unstable_by_key(|&(start, _)| start);
    let merged = merge_spans(&all_spans);

    // Build result in single pass
    let mut result = String::with_capacity(line.len());
    let mut last_end = 0;

    for (start, end) in merged {
        result.push_str(&line[last_end..start]);
        // Use spaces to preserve length
        for _ in 0..(end - start) {
            result.push(' ');
        }
        last_end = end;
    }
    result.push_str(&line[last_end..]);

    result
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

    #[test]
    fn test_merge_spans_empty() {
        assert!(merge_spans(&[]).is_empty());
    }

    #[test]
    fn test_merge_spans_no_overlap() {
        let spans = [(0, 5), (10, 15), (20, 25)];
        let merged = merge_spans(&spans);
        assert_eq!(merged, vec![(0, 5), (10, 15), (20, 25)]);
    }

    #[test]
    fn test_merge_spans_overlapping() {
        let spans = [(0, 10), (5, 15), (20, 25)];
        let merged = merge_spans(&spans);
        assert_eq!(merged, vec![(0, 15), (20, 25)]);
    }

    #[test]
    fn test_merge_spans_adjacent() {
        let spans = [(0, 10), (10, 20)];
        let merged = merge_spans(&spans);
        assert_eq!(merged, vec![(0, 20)]);
    }

    // =========================================================================
    // InlineHilite tests
    // =========================================================================

    #[test]
    fn test_contains_inline_hilite() {
        assert!(contains_inline_hilite("`#!python print('hello')`"));
        assert!(contains_inline_hilite("Use `#!js alert('hi')` for alerts"));
        assert!(contains_inline_hilite("`#!c++ cout << x;`"));

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

    // =========================================================================
    // Keys tests
    // =========================================================================

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

    // =========================================================================
    // Caret tests
    // =========================================================================

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
    fn test_find_superscript_spans() {
        let spans = find_superscript_spans("E=mc^2^");
        assert_eq!(spans.len(), 1);
        assert_eq!(&"E=mc^2^"[spans[0].0..spans[0].1], "^2^");
    }

    #[test]
    fn test_superscript_not_inside_insert() {
        // ^x^ inside ^^text^^ should not be detected as superscript
        let line = "^^some^x^text^^";
        let spans = find_superscript_spans(line);
        assert!(spans.is_empty(), "Superscript inside insert should not be detected");
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

    // =========================================================================
    // Tilde tests
    // =========================================================================

    #[test]
    fn test_contains_subscript() {
        assert!(contains_subscript("H~2~O"));
        assert!(contains_subscript("x~n~ power"));

        assert!(!contains_subscript("no tilde here"));
        assert!(!contains_subscript("~~strikethrough~~"));
    }

    #[test]
    fn test_contains_strikethrough() {
        assert!(contains_strikethrough("~~deleted text~~"));
        assert!(contains_strikethrough("Some ~~old~~ text"));
        assert!(contains_strikethrough("~~a~b~~")); // single tilde inside is OK

        assert!(!contains_strikethrough("~subscript~"));
        assert!(!contains_strikethrough("no markup"));
    }

    #[test]
    fn test_find_subscript_spans() {
        let spans = find_subscript_spans("H~2~O");
        assert_eq!(spans.len(), 1);
        assert_eq!(&"H~2~O"[spans[0].0..spans[0].1], "~2~");
    }

    #[test]
    fn test_subscript_not_inside_strikethrough() {
        let line = "~~some~x~text~~";
        let spans = find_subscript_spans(line);
        assert!(
            spans.is_empty(),
            "Subscript inside strikethrough should not be detected"
        );
    }

    #[test]
    fn test_multiple_subscripts() {
        let line = "~a~ and ~b~";
        let spans = find_subscript_spans(line);
        assert_eq!(spans.len(), 2);
        assert_eq!(&line[spans[0].0..spans[0].1], "~a~");
        assert_eq!(&line[spans[1].0..spans[1].1], "~b~");
    }

    #[test]
    fn test_subscript_no_whitespace() {
        let line = "~no spaces allowed~";
        let spans = find_subscript_spans(line);
        assert!(spans.is_empty(), "Subscript with whitespace should not match");
    }

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

    #[test]
    fn test_subscript_vs_strikethrough_coexist() {
        let line = "H~2~O is ~~not~~ water";
        assert!(contains_subscript(line));
        assert!(contains_strikethrough(line));
    }

    #[test]
    fn test_strikethrough_with_internal_tilde() {
        // ~~a~b~~ should match as one strikethrough, not as strikethrough + subscript
        let line = "~~a~b~~";
        assert!(contains_strikethrough(line));

        let strike_spans = find_strikethrough_spans(line);
        assert_eq!(strike_spans.len(), 1);
        assert_eq!(&line[strike_spans[0].0..strike_spans[0].1], "~~a~b~~");

        // No subscript should be found
        assert!(!contains_subscript(line));
    }

    // =========================================================================
    // Mark tests
    // =========================================================================

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

    // =========================================================================
    // SmartSymbols tests
    // =========================================================================

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
    fn test_mask_pymdown_markup() {
        let line = "Press ++ctrl++ and ^super^ with ==mark==";
        let masked = mask_pymdown_markup(line);
        assert!(!masked.contains("++"));
        assert!(!masked.contains("^super^"));
        assert!(!masked.contains("==mark=="));
        assert!(masked.contains("Press"));
        assert!(masked.contains("and"));
        assert!(masked.contains("with"));
        assert_eq!(masked.len(), line.len());
    }

    #[test]
    fn test_mask_pymdown_markup_with_tilde() {
        let line = "H~2~O is ~~deleted~~ water";
        let masked = mask_pymdown_markup(line);
        assert!(!masked.contains("~2~"));
        assert!(!masked.contains("~~deleted~~"));
        assert!(masked.contains("H"));
        assert!(masked.contains("O is"));
        assert!(masked.contains("water"));
        assert_eq!(masked.len(), line.len());
    }

    #[test]
    fn test_mask_preserves_unmasked_text() {
        let line = "plain text without markup";
        let masked = mask_pymdown_markup(line);
        assert_eq!(masked, line);
    }

    #[test]
    fn test_mask_complex_mixed_markup() {
        let line = "++ctrl++ ^2^ ~x~ ~~old~~ ==new==";
        let masked = mask_pymdown_markup(line);
        // All markup should be masked
        assert!(!masked.contains("++"));
        assert!(!masked.contains("^2^"));
        assert!(!masked.contains("~x~"));
        assert!(!masked.contains("~~old~~"));
        assert!(!masked.contains("==new=="));
        // Length preserved
        assert_eq!(masked.len(), line.len());
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_empty_line() {
        assert!(!contains_keys(""));
        assert!(!contains_superscript(""));
        assert!(!contains_subscript(""));
        assert!(!contains_mark(""));
        assert_eq!(mask_pymdown_markup(""), "");
    }

    #[test]
    fn test_unclosed_delimiters() {
        assert!(!contains_superscript("^unclosed"));
        assert!(!contains_subscript("~unclosed"));
        assert!(!contains_mark("==unclosed"));
        assert!(!contains_keys("++unclosed"));
    }

    #[test]
    fn test_adjacent_markup() {
        let line = "^a^^b^";
        // This is: ^a^ followed by ^b^ (two superscripts)
        // Wait, that's not right. Let me trace:
        // Position 0: ^, check if double -> pos 1 is ^, YES -> skip both, i=2
        // Position 2: ^, check if double -> pos 3 is b, NO -> start superscript
        // Inner: b at pos 3, found_content=true
        // Inner: ^ at pos 4, check if double -> pos 5 is nothing, NO -> valid!
        // So we get one superscript: ^b^
        let spans = find_superscript_spans(line);
        // Actually wait, let me re-trace more carefully.
        // ^a^^b^
        // 012345
        // i=0: ch='^', is_double? chars[1]=a, NO. Start looking for close.
        //   j=1: 'a', found_content=true
        //   j=2: '^', is_double? chars[3]='^', YES. Break without adding span.
        // i=1: ch='a', not '^', continue
        // i=2: ch='^', is_double? chars[3]='^', YES. Skip, i becomes 3 after next()
        // Actually wait, after break from inner loop at j=2, outer while does next() which gives i=3
        // i=3: ch='^', is_double? chars[4]='b', NO. Start looking for close.
        //   j=4: 'b', found_content=true
        //   j=5: '^', is_double? no more chars, NO. Valid! Add span.
        // So we get ^b^ at positions 3-6
        assert_eq!(spans.len(), 1);
        assert_eq!(&line[spans[0].0..spans[0].1], "^b^");
    }

    #[test]
    fn test_triple_tilde() {
        // ~~~a~~~ should match ~~a~~ (strikethrough) with extra tildes as text
        let line = "~~~a~~~";
        let strike_spans = find_strikethrough_spans(line);
        // The regex ~~[^~]+(?:~[^~]+)*~~ on "~~~a~~~":
        // Try at pos 0: ~~ matches, then [^~]+ needs non-tilde at pos 2, which is ~. Fail.
        // Try at pos 1: ~~ matches (pos 1-2), [^~]+ matches 'a' at pos 3.
        // Then (?:~[^~]+)* tries at pos 4: ~ at 4, then [^~]+ needs non-tilde at 5, which is ~. Zero matches.
        // Then ~~ matches at pos 4-5.
        // So we get ~~a~~ at positions 1-6.
        assert_eq!(strike_spans.len(), 1);
        assert_eq!(&line[strike_spans[0].0..strike_spans[0].1], "~~a~~");
    }
}
