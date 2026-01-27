//! Sentence detection utilities
//!
//! This module provides shared functionality for detecting sentence boundaries
//! in markdown text. Used by both text reflow (MD013) and the multiple spaces
//! rule (MD064).
//!
//! Features:
//! - Common abbreviation detection (Mr., Dr., Prof., etc.)
//! - CJK punctuation support (。, ！, ？)
//! - Closing quote detection (straight and curly)
//! - Both forward-looking (reflow) and backward-looking (MD064) sentence detection

use std::collections::HashSet;

/// Default abbreviations that should NOT be treated as sentence endings.
///
/// Only includes abbreviations that:
/// 1. Conventionally ALWAYS have a period in standard writing
/// 2. Are followed by something (name, example), not sentence-final
///
/// Does NOT include:
/// - Words that don't typically take periods (vs, etc)
/// - Abbreviations that can end sentences (Inc., Ph.D., U.S.)
pub const DEFAULT_ABBREVIATIONS: &[&str] = &[
    // Titles - always have period, always followed by a name
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr",
    // Latin - always written with periods, introduce examples/references
    "i.e", "e.g",
];

/// Get the effective abbreviations set based on custom additions
/// All abbreviations are normalized to lowercase for case-insensitive matching
/// Custom abbreviations are always merged with built-in defaults
pub fn get_abbreviations(custom: &Option<Vec<String>>) -> HashSet<String> {
    let mut abbreviations: HashSet<String> = DEFAULT_ABBREVIATIONS.iter().map(|s| s.to_lowercase()).collect();

    // Always extend defaults with custom abbreviations
    // Strip any trailing periods and normalize to lowercase for consistent matching
    if let Some(custom_list) = custom {
        for abbr in custom_list {
            let normalized = abbr.trim_end_matches('.').to_lowercase();
            if !normalized.is_empty() {
                abbreviations.insert(normalized);
            }
        }
    }

    abbreviations
}

/// Check if text ends with a common abbreviation followed by a period
///
/// Abbreviations only count when followed by a period, not ! or ?.
/// This prevents false positives where words ending in abbreviation-like
/// letter sequences (e.g., "paradigms" ending in "ms") are incorrectly
/// detected as abbreviations.
///
/// Examples:
///   - "Dr." -> true (abbreviation)
///   - "Dr?" -> false (question, not abbreviation)
///   - "paradigms." -> false (not in abbreviation list)
///   - "paradigms?" -> false (question mark, not abbreviation)
pub fn text_ends_with_abbreviation(text: &str, abbreviations: &HashSet<String>) -> bool {
    // Only check if text ends with a period (abbreviations require periods)
    if !text.ends_with('.') {
        return false;
    }

    // Remove the trailing period
    let without_period = text.trim_end_matches('.');

    // Get the last word by splitting on whitespace
    let last_word = without_period.split_whitespace().last().unwrap_or("");

    if last_word.is_empty() {
        return false;
    }

    // O(1) HashSet lookup (abbreviations are already lowercase)
    abbreviations.contains(&last_word.to_lowercase())
}

/// Check if a character is CJK sentence-ending punctuation
/// These include: 。(ideographic full stop), ！(fullwidth exclamation), ？(fullwidth question)
pub fn is_cjk_sentence_ending(c: char) -> bool {
    matches!(c, '。' | '！' | '？')
}

/// Check if a character is a closing quote mark
/// Includes straight quotes and curly/smart quotes
pub fn is_closing_quote(c: char) -> bool {
    // " (straight double), ' (straight single), " (U+201D right double), ' (U+2019 right single)
    // » (right guillemet), › (single right guillemet)
    matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | '»' | '›')
}

/// Check if a character is an opening quote mark
/// Includes straight quotes and curly/smart quotes
pub fn is_opening_quote(c: char) -> bool {
    // " (straight double), ' (straight single), " (U+201C left double), ' (U+2018 left single)
    // « (left guillemet), ‹ (single left guillemet)
    matches!(c, '"' | '\'' | '\u{201C}' | '\u{2018}' | '«' | '‹')
}

/// Check if a character is a CJK character (Chinese, Japanese, Korean)
pub fn is_cjk_char(c: char) -> bool {
    // CJK Unified Ideographs and common extensions
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Unified Ideographs Extension A
        '\u{3040}'..='\u{309F}' |   // Hiragana
        '\u{30A0}'..='\u{30FF}' |   // Katakana
        '\u{AC00}'..='\u{D7AF}'     // Hangul Syllables
    )
}

/// Check if a character is sentence-ending punctuation (ASCII or CJK)
pub fn is_sentence_ending_punctuation(c: char) -> bool {
    matches!(c, '.' | '!' | '?') || is_cjk_sentence_ending(c)
}

/// Check if a character is closing punctuation that can follow sentence-ending punctuation
/// This includes closing quotes, parentheses, and brackets
pub fn is_trailing_close_punctuation(c: char) -> bool {
    is_closing_quote(c) || matches!(c, ')' | ']' | '}')
}

/// Check if multiple spaces occur immediately after sentence-ending punctuation.
/// This is a backward-looking check used by MD064.
///
/// Returns true if the character(s) immediately before `match_start` constitute
/// a sentence ending, supporting the traditional two-space-after-sentence convention.
///
/// Recognized sentence-ending patterns:
/// - Direct punctuation: `.`, `!`, `?`, `。`, `！`, `？`
/// - With closing quotes: `."`, `!"`, `?"`, `.'`, `!'`, `?'`, `."`, `?"`, `!"`
/// - With closing parenthesis: `.)`, `!)`, `?)`
/// - With closing bracket: `.]`, `!]`, `?]`
/// - Ellipsis: `...`
/// - Combinations: `.")`  (quote then paren), `?')`
///
/// Does NOT treat as sentence ending:
/// - Abbreviations: `Dr.`, `Mr.`, `Prof.`, etc. (when detectable)
/// - Single letters followed by period: `A.` (likely initials or list markers)
pub fn is_after_sentence_ending(text: &str, match_start: usize) -> bool {
    is_after_sentence_ending_with_abbreviations(text, match_start, &get_abbreviations(&None))
}

/// Check if multiple spaces occur immediately after sentence-ending punctuation,
/// with a custom abbreviations set.
///
/// Note: `match_start` is a byte position (from regex). This function handles
/// multi-byte UTF-8 characters correctly by working with character iterators.
pub fn is_after_sentence_ending_with_abbreviations(
    text: &str,
    match_start: usize,
    abbreviations: &HashSet<String>,
) -> bool {
    if match_start == 0 || match_start > text.len() {
        return false;
    }

    // Safely get the portion of the text before the spaces
    // match_start is a byte position, so we need to ensure it's a valid char boundary
    let before = match text.get(..match_start) {
        Some(s) => s,
        None => return false, // Invalid byte position
    };

    // Collect chars for iteration (we need random access for some checks)
    let chars: Vec<char> = before.chars().collect();
    if chars.is_empty() {
        return false;
    }

    let mut idx = chars.len() - 1;

    // Skip through any trailing closing punctuation (quotes, parens, brackets)
    // These can appear after the sentence-ending punctuation
    // e.g., `sentence."  Next` or `sentence.)  Next` or `sentence.")`
    while idx > 0 && is_trailing_close_punctuation(chars[idx]) {
        idx -= 1;
    }

    // Now check if we're at sentence-ending punctuation
    let current = chars[idx];

    // Check for CJK sentence-ending punctuation
    if is_cjk_sentence_ending(current) {
        return true;
    }

    // Direct sentence-ending punctuation (! and ?)
    if current == '!' || current == '?' {
        return true;
    }

    // Period - need more careful handling
    if current == '.' {
        // Check for ellipsis (...) - always a valid sentence ending
        if idx >= 2 && chars[idx - 1] == '.' && chars[idx - 2] == '.' {
            return true;
        }

        // Build the text before the period by collecting chars up to idx
        // (not including the period itself)
        let text_before_period: String = chars[..idx].iter().collect();

        // Check if this is an abbreviation
        if text_ends_with_abbreviation(&format!("{text_before_period}."), abbreviations) {
            return false;
        }

        // Check what comes before the period
        if idx > 0 {
            let prev = chars[idx - 1];

            // Single letter before period - likely initial or list marker, not sentence
            // e.g., "A." "B." but allow "a." at end of sentence
            if prev.is_ascii_uppercase() {
                // Check if it's preceded by whitespace or start of text (isolated initial)
                if idx >= 2 {
                    if chars[idx - 2].is_whitespace() {
                        // "word A." - isolated initial, not sentence ending
                        return false;
                    }
                } else {
                    // "A." at start - not a sentence ending
                    return false;
                }
            }

            // If previous char is alphanumeric, closing quote/paren, or markdown inline delimiters, treat as sentence end
            // Markdown inline elements that can end before punctuation:
            // - `)` `]` - links, images, footnote refs
            // - `` ` `` - inline code
            // - `*` `_` - emphasis/bold
            // - `~` - strikethrough
            // - `=` - highlight (extended markdown)
            // - `^` - superscript (extended markdown)
            if prev.is_alphanumeric()
                || is_closing_quote(prev)
                || matches!(prev, ')' | ']' | '`' | '*' | '_' | '~' | '=' | '^')
                || is_cjk_char(prev)
            {
                return true;
            }
        }

        // Period at start or after non-word char - not a sentence ending
        return false;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Abbreviation tests ===

    #[test]
    fn test_get_abbreviations_default() {
        let abbrevs = get_abbreviations(&None);
        assert!(abbrevs.contains("dr"));
        assert!(abbrevs.contains("mr"));
        assert!(abbrevs.contains("prof"));
        assert!(abbrevs.contains("i.e"));
        assert!(abbrevs.contains("e.g"));
    }

    #[test]
    fn test_get_abbreviations_custom() {
        let custom = Some(vec!["Corp".to_string(), "Ltd.".to_string()]);
        let abbrevs = get_abbreviations(&custom);
        // Should include defaults
        assert!(abbrevs.contains("dr"));
        // Should include custom (normalized)
        assert!(abbrevs.contains("corp"));
        assert!(abbrevs.contains("ltd"));
    }

    #[test]
    fn test_text_ends_with_abbreviation() {
        let abbrevs = get_abbreviations(&None);
        assert!(text_ends_with_abbreviation("Dr.", &abbrevs));
        assert!(text_ends_with_abbreviation("Hello Dr.", &abbrevs));
        assert!(text_ends_with_abbreviation("Prof.", &abbrevs));
        assert!(!text_ends_with_abbreviation("Doctor.", &abbrevs));
        assert!(!text_ends_with_abbreviation("Dr?", &abbrevs)); // Not a period
        assert!(!text_ends_with_abbreviation("paradigms.", &abbrevs));
    }

    // === Punctuation helper tests ===

    #[test]
    fn test_is_closing_quote() {
        assert!(is_closing_quote('"'));
        assert!(is_closing_quote('\''));
        assert!(is_closing_quote('\u{201D}')); // "
        assert!(is_closing_quote('\u{2019}')); // '
        assert!(is_closing_quote('»'));
        assert!(is_closing_quote('›'));
        assert!(!is_closing_quote('a'));
        assert!(!is_closing_quote('.'));
    }

    #[test]
    fn test_is_cjk_sentence_ending() {
        assert!(is_cjk_sentence_ending('。'));
        assert!(is_cjk_sentence_ending('！'));
        assert!(is_cjk_sentence_ending('？'));
        assert!(!is_cjk_sentence_ending('.'));
        assert!(!is_cjk_sentence_ending('!'));
    }

    #[test]
    fn test_is_cjk_char() {
        assert!(is_cjk_char('中'));
        assert!(is_cjk_char('あ')); // Hiragana
        assert!(is_cjk_char('ア')); // Katakana
        assert!(is_cjk_char('한')); // Hangul
        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char('A'));
    }

    // === is_after_sentence_ending tests ===

    #[test]
    fn test_after_period() {
        assert!(is_after_sentence_ending("Hello.  ", 6));
        assert!(is_after_sentence_ending("End of sentence.  Next", 16));
    }

    #[test]
    fn test_after_exclamation() {
        assert!(is_after_sentence_ending("Wow!  ", 4));
        assert!(is_after_sentence_ending("Great!  Next", 6));
    }

    #[test]
    fn test_after_question() {
        assert!(is_after_sentence_ending("Really?  ", 7));
        assert!(is_after_sentence_ending("What?  Next", 5));
    }

    #[test]
    fn test_after_closing_quote() {
        assert!(is_after_sentence_ending("He said \"Hello.\"  Next", 16));
        assert!(is_after_sentence_ending("She said 'Hi.'  Next", 14));
    }

    #[test]
    fn test_after_curly_quotes() {
        let content = format!("He said {}Hello.{}  Next", '\u{201C}', '\u{201D}');
        // Find the position after the closing quote
        let pos = content.find("  ").unwrap();
        assert!(is_after_sentence_ending(&content, pos));
    }

    #[test]
    fn test_after_closing_paren() {
        assert!(is_after_sentence_ending("(See note.)  Next", 11));
        assert!(is_after_sentence_ending("(Really!)  Next", 9));
    }

    #[test]
    fn test_after_closing_bracket() {
        assert!(is_after_sentence_ending("[Citation.]  Next", 11));
    }

    #[test]
    fn test_after_ellipsis() {
        assert!(is_after_sentence_ending("And so...  Next", 9));
        assert!(is_after_sentence_ending("Hmm...  Let me think", 6));
    }

    #[test]
    fn test_not_after_abbreviation() {
        // Dr. should NOT be treated as sentence ending
        assert!(!is_after_sentence_ending("Dr.  Smith", 3));
        assert!(!is_after_sentence_ending("Mr.  Jones", 3));
        assert!(!is_after_sentence_ending("Prof.  Williams", 5));
    }

    #[test]
    fn test_not_after_single_initial() {
        // Single capital letter + period is likely an initial, not sentence end
        assert!(!is_after_sentence_ending("John A.  Smith", 7));
        // But lowercase should work (end of sentence)
        assert!(is_after_sentence_ending("letter a.  Next", 9));
    }

    #[test]
    fn test_mid_sentence_not_detected() {
        // Spaces not after sentence punctuation
        assert!(!is_after_sentence_ending("word  word", 4));
        assert!(!is_after_sentence_ending("multiple  spaces", 8));
    }

    #[test]
    fn test_cjk_sentence_ending() {
        // CJK chars are 3 bytes each in UTF-8
        // 日(3)+本(3)+語(3)+。(3) = 12 bytes before the spaces
        assert!(is_after_sentence_ending("日本語。  Next", 12)); // After 。
        // 中(3)+文(3)+！(3) = 9 bytes before the spaces
        assert!(is_after_sentence_ending("中文！  Next", 9)); // After ！
        // 한(3)+국(3)+어(3)+？(3) = 12 bytes before the spaces
        assert!(is_after_sentence_ending("한국어？  Next", 12)); // After ？
    }

    #[test]
    fn test_complex_endings() {
        // Multiple closing punctuation
        assert!(is_after_sentence_ending("(He said \"Yes.\")  Next", 16));
        // Quote then paren
        assert!(is_after_sentence_ending("\"End.\")  Next", 7));
    }

    #[test]
    fn test_guillemets() {
        assert!(is_after_sentence_ending("Il dit «Oui.»  Next", 13));
    }

    #[test]
    fn test_empty_and_edge_cases() {
        assert!(!is_after_sentence_ending("", 0));
        assert!(!is_after_sentence_ending(".", 0));
        assert!(!is_after_sentence_ending("a", 0));
    }

    #[test]
    fn test_latin_abbreviations() {
        // i.e. and e.g. should not be sentence endings
        assert!(!is_after_sentence_ending("i.e.  example", 4));
        assert!(!is_after_sentence_ending("e.g.  example", 4));
    }

    #[test]
    fn test_after_inline_code() {
        // Issue #345: Sentence ending with inline code should be recognized
        // "Hello from `backticks`.  How's it going?"
        // Position 23 is after the period following the closing backtick
        assert!(is_after_sentence_ending("Hello from `backticks`.  Next", 23));

        // Simple case: just code and period
        assert!(is_after_sentence_ending("`code`.  Next", 7));

        // Multiple inline code spans
        assert!(is_after_sentence_ending("Use `foo` and `bar`.  Next", 20));

        // With exclamation mark
        assert!(is_after_sentence_ending("`important`!  Next", 12));

        // With question mark
        assert!(is_after_sentence_ending("Is it `true`?  Next", 13));

        // Inline code in the middle shouldn't affect sentence detection
        assert!(is_after_sentence_ending("The `code` works.  Next", 17));
    }

    #[test]
    fn test_after_inline_code_with_quotes() {
        // Inline code before closing quote before period
        assert!(is_after_sentence_ending("He said \"use `code`\".  Next", 21));

        // Inline code in parentheses
        assert!(is_after_sentence_ending("(see `example`).  Next", 16));
    }

    #[test]
    fn test_after_emphasis() {
        // Asterisk emphasis
        assert!(is_after_sentence_ending("The word is *important*.  Next", 24));

        // Underscore emphasis
        assert!(is_after_sentence_ending("The word is _important_.  Next", 24));

        // With exclamation
        assert!(is_after_sentence_ending("This is *urgent*!  Next", 17));

        // With question
        assert!(is_after_sentence_ending("Is it _true_?  Next", 13));
    }

    #[test]
    fn test_after_bold() {
        // Asterisk bold
        assert!(is_after_sentence_ending("The word is **critical**.  Next", 25));

        // Underscore bold
        assert!(is_after_sentence_ending("The word is __critical__.  Next", 25));
    }

    #[test]
    fn test_after_strikethrough() {
        // GFM strikethrough
        assert!(is_after_sentence_ending("This is ~~wrong~~.  Next", 18));

        // With exclamation
        assert!(is_after_sentence_ending("That was ~~bad~~!  Next", 17));
    }

    #[test]
    fn test_after_extended_markdown() {
        // Highlight syntax (some flavors)
        assert!(is_after_sentence_ending("This is ==highlighted==.  Next", 24));

        // Superscript syntax (some flavors)
        assert!(is_after_sentence_ending("E equals mc^2^.  Next", 15));
    }
}
