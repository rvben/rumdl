use regex::Regex;
use std::sync::LazyLock;

/// Format a Unicode codepoint as a string in the format "U+XXXX" or "U+XXXXX" or "U+XXXXXX",
/// depending on the value of the codepoint. The output is always uppercase.
pub fn format_codepoint(c: char) -> String {
    let cp = c as u32;
    if cp <= 0xFFFF {
        format!("U+{cp:04X}")
    } else if cp <= 0xFFFFF {
        format!("U+{cp:05X}")
    } else if cp <= 0x10FFFF {
        format!("U+{cp:06X}")
    } else {
        panic!("Invalid Unicode codepoint: {cp}");
    }
}

/// Parse a single character from a string, returning `Some(char)` if the string contains exactly one character,
/// or `None` if the string is empty or contains more than one character.
pub fn parse_single_char(input: &str) -> Option<char> {
    let mut chars = input.trim().chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if first.len_utf8() != input.len() {
        return None;
    }
    Some(first)
}

/// Check a Unicode codepoint token in the format "U+XXXX" or "u+XXXX",
/// and return a normalized version of it in the format "U+XXXX".
/// with uppercase letters and no leading/trailing whitespace.
pub fn normalize_codepoint(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let Some(hex) = trimmed.strip_prefix("U+").or_else(|| trimmed.strip_prefix("u+")) else {
        return Err(format!("Invalid codepoint '{trimmed}': expected format U+XXXX"));
    };

    if !(4..=6).contains(&hex.len()) {
        return Err(format!("Invalid codepoint '{trimmed}': expected 4 to 6 hex digits"));
    }

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid codepoint '{trimmed}': contains non-hex characters"));
    }

    let value = u32::from_str_radix(hex, 16).map_err(|_| format!("Invalid codepoint '{trimmed}': parse failed"))?;

    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return Err(format!("Invalid codepoint '{trimmed}': out of Unicode range"));
    }

    Ok(format_codepoint(char::from_u32(value).unwrap()))
}

/// Parse a codepoint token in the format "U+XXXX" or "u+XXXX" and returns the corresponding character.
pub fn parse_codepoint(token: &str) -> Option<char> {
    let normalized = normalize_codepoint(token).ok()?;
    let hex = normalized
        .strip_prefix("U+")
        .or_else(|| normalized.strip_prefix("u+"))?;
    if hex.len() < 4 || hex.len() > 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let value = u32::from_str_radix(hex, 16).ok()?;
    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return None;
    }
    std::char::from_u32(value)
}

/// Check if a Unicode character is considered invisible according to the Unicode standard and common usage.
/// This includes control characters, formatting characters,
/// and other non-printing characters that do not produce a visible mark in text.
pub fn is_invisible_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x0000..=0x0008
            | 0x000A..=0x001F // C0 Control characters, excluding TAB (0x0009)
            | 0x007F..=0x009F // DEL + C1 control characters
            | 0x00AD // SOFT HYPHEN
            | 0x034F // COMBINING GRAPHEME JOINER
            | 0x061C // ARABIC LETTER MARK
            | 0x115F // HANGUL CHOSEONG FILLER
            | 0x1160 // HANGUL JUNGSEONG FILLER
            | 0x17B4 // KHMER VOWEL INHERENT AQ
            | 0x17B5 // KHMER VOWEL INHERENT AA
            | 0x180B..=0x180E // Mongolian variation selectors + MONGOLIAN VOWEL SEPARATOR
            | 0x200B..=0x200F // ZWSP, ZWNJ, ZWJ, LRM, RLM
            | 0x202A..=0x202E // Bidi embedding/override controls
            | 0x2060..=0x206F // WORD JOINER, invisibles, and bidi isolate controls
            | 0x3164 // HANGUL FILLER
            | 0xFE00..=0xFE0F // Variation Selectors (VS1..VS16)
            | 0xFEFF // ZERO WIDTH NO-BREAK SPACE (BOM)
            | 0xFFA0 // HALFWIDTH HANGUL FILLER
            | 0xFFF0..=0xFFF8 // Reserved non-rendering specials
            | 0x1BCA0..=0x1BCA3 // Shorthand format controls
            | 0x1D173..=0x1D17A // Musical symbol format controls
            | 0xE0000..=0xE0FFF // Tags block + Variation Selectors Supplement
    )
}

/// Check if a Unicode character carries the `Deprecated` property
/// or is otherwise discouraged from use, but still renders in most environments.
/// These characters are discouraged from use but they still render, so they are reported without a
/// removal fix: only the author knows what the text should say instead.
pub fn is_deprecated_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x0149 // LATIN SMALL LETTER N PRECEDED BY APOSTROPHE
            | 0x0673 // ARABIC LETTER ALEF WITH WAVY HAMZA ABOVE
            | 0x0F77 // TIBETAN VOWEL SIGN VOCALIC LL
            | 0x0F79 // TIBETAN VOWEL SIGN VOCALIC LR
            | 0x17A3..=0x17A4 // KHMER INHERENT VOWEL SIGN AA..KHMER INHERENT VOWEL SIGN AE
            | 0x206A..=0x206F // INHIBIT SYMMETRIC SWAPPING..NOMINAL DIGIT SHAPES
            | 0x2329 // LEFT-POINTING ANGLE BRACKET
            | 0x232A // RIGHT-POINTING ANGLE BRACKET
            | 0xE0001 // LANGUAGE TAG
    )
}

/// The rows of UTR#20 table 3.1 that neither of the sets above already covers:
/// visible or structural code points a markup document is meant to express with
/// markup instead. They are not default-ignorable, so removing one would drop
/// content or leave a paired construct half-open, and only the two tone marks
/// have a replacement that preserves the text exactly.
pub fn is_unsuitable_for_markup_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x0340 // COMBINING GRAVE TONE MARK
            | 0x0341 // COMBINING ACUTE TONE MARK
            | 0xFFF9..=0xFFFC // Interlinear annotation delimiters + OBJECT REPLACEMENT CHARACTER
    )
}

/// Whether `c` is a letter of a Chinese, Japanese or Korean script.
///
/// Letters only: Han ideographs (`中`, the iteration mark `々`, `〇`), kana
/// including half-width forms and the prolonged sound mark (`ー`), and Hangul
/// syllables and jamo. CJK punctuation (`。`, `「`, `・`), full-width Latin
/// letters and digits (`Ｔ`, `１`) and bopomofo are not CJK letters. Neither are
/// combining marks such as the kana voiced sound mark (`\u{3099}`): a mark
/// renders as part of the letter it follows, so it is that letter's business.
pub fn is_cjk_letter(c: char) -> bool {
    let cp = c as u32;
    // Below U+1100 only U+0305 COMBINING OVERLINE and U+0323 COMBINING DOT BELOW match
    // the class, carrying Katakana script extensions. These combining marks attach to
    // whatever precedes them, so returning false here is deliberate to avoid splitting
    // grapheme clusters.
    if cp < 0x1100 {
        return false;
    }
    // The three dense blocks contain nothing but letters; answer without the regex.
    if matches!(cp, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xAC00..=0xD7A3) {
        return true;
    }
    // Script extensions catch characters shared across CJK scripts (`々`, `ー`);
    // subtracting punctuation, symbols and combining marks leaves letters and
    // numerals.
    static CJK_LETTER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[\p{scx=Han}\p{scx=Hiragana}\p{scx=Katakana}\p{scx=Hangul}--\p{P}--\p{S}--\p{Mn}--\p{Me}]$")
            .expect("CJK letter class is a valid regex")
    });
    let mut buf = [0u8; 4];
    CJK_LETTER.is_match(c.encode_utf8(&mut buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_letters_cover_han_kana_and_hangul() {
        // Han: ideographs, the iteration mark, the ideographic zero, Extension B.
        for c in ['中', '々', '〇', '\u{20000}'] {
            assert!(is_cjk_letter(c), "{c:?} (U+{:04X}) is a CJK letter", c as u32);
        }
        // Kana: hiragana, katakana, the prolonged sound mark, half-width katakana.
        for c in ['あ', 'カ', 'ー', 'ﾊ'] {
            assert!(is_cjk_letter(c), "{c:?} (U+{:04X}) is a CJK letter", c as u32);
        }
        // Hangul: a syllable and a conjoining jamo.
        for c in ['한', '\u{1100}'] {
            assert!(is_cjk_letter(c), "{c:?} (U+{:04X}) is a CJK letter", c as u32);
        }
    }

    #[test]
    fn cjk_punctuation_symbols_and_other_scripts_are_not_letters() {
        // CJK punctuation and symbols share the scripts but are not letters. U+2E80 is a
        // symbol rather than a letter, so the --\p{S} term in the class excludes it.
        for c in ['・', '。', '「', '〜', '\u{2E80}', '\u{3000}'] {
            assert!(!is_cjk_letter(c), "{c:?} (U+{:04X}) is not a CJK letter", c as u32);
        }
        // Full-width Latin and digits, ASCII, bopomofo.
        for c in ['１', 'Ｔ', 'a', '1', 'ㄅ', ' ', 'é'] {
            assert!(!is_cjk_letter(c), "{c:?} (U+{:04X}) is not a CJK letter", c as u32);
        }
        // Combining marks are excluded whatever their script, so a base character
        // and its mark stay one unit. U+3099 and U+309A voice decomposed kana,
        // U+0305 and U+0323 carry Katakana script extensions.
        for c in ['\u{3099}', '\u{309A}', '\u{0305}', '\u{0323}'] {
            assert!(!is_cjk_letter(c), "{c:?} (U+{:04X}) is not a CJK letter", c as u32);
        }
    }
}
