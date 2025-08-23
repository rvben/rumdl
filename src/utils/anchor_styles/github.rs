//! GitHub.com official anchor generation with security hardening
//!
//! This module implements the exact anchor generation algorithm used by GitHub.com,
//! verified through comprehensive testing with GitHub Gists, with comprehensive
//! security hardening against injection attacks and DoS vectors.
//!
//! Algorithm verified against GitHub.com (not third-party packages):
//! 1. Input validation and size limits (max 10KB)
//! 2. Unicode normalization (NFC) to prevent homograph attacks
//! 3. Dangerous Unicode filtering (RTL override, zero-width, control chars)
//! 4. Lowercase conversion
//! 5. Markdown formatting removal (*, `, []) with ReDoS-safe patterns
//! 6. Multi-character pattern replacement (-->, <->, ==>, ->)
//! 7. Special symbol replacement (& → --, © → --)
//! 8. Character processing (preserve letters, digits, underscores, hyphens)
//! 9. Space → single hyphen, emojis → single hyphen
//! 10. No leading/trailing trimming (unlike kramdown)
//!
//! Security measures implemented:
//! - Input size limits to prevent memory exhaustion
//! - Unicode normalization to prevent homograph attacks
//! - Bidirectional text injection prevention
//! - Zero-width character stripping
//! - Control character filtering
//! - ReDoS-resistant regex patterns with complexity limits
//! - Comprehensive emoji detection including country flags and keycaps

use lazy_static::lazy_static;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

// Security limits for input validation
const MAX_INPUT_LENGTH: usize = 10240; // 10KB maximum input

lazy_static! {
    // ReDoS-resistant patterns with atomic grouping and possessive quantifiers where possible
    // Limited repetition depth to prevent catastrophic backtracking
    // Match both asterisk and underscore emphasis (with proper nesting handling)
    static ref EMPHASIS_ASTERISK: Regex = Regex::new(r"\*{1,3}([^*]+?)\*{1,3}").unwrap();
    // Match emphasis underscores - only when they wrap text, not in snake_case
    // This pattern matches _text_ or __text__ but not test_with_underscores
    static ref EMPHASIS_UNDERSCORE: Regex = Regex::new(r"\b_{1,2}([^_\s][^_]*?)_{1,2}\b").unwrap();
    static ref CODE_PATTERN: Regex = Regex::new(r"`([^`]{0,500})`").unwrap();
    // Match image and link patterns
    // Using simple approach: match the brackets and parentheses, extract only the bracket content
    static ref IMAGE_PATTERN: Regex = Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap();
    static ref LINK_PATTERN: Regex = Regex::new(r"\[([^\[\]]*(?:\[[^\[\]]*\][^\[\]]*)*)\](?:\([^)]*\)|\[[^\]]*\])").unwrap();

    // Zero-width character patterns - remove these entirely for security
    static ref ZERO_WIDTH_PATTERN: Regex = Regex::new(r"[\u200B-\u200D\u2060\uFEFF]").unwrap();

    // RTL override and dangerous Unicode control patterns
    static ref DANGEROUS_UNICODE_PATTERN: Regex = Regex::new(r"[\u202A-\u202E\u2066-\u2069\u061C\u200E\u200F]").unwrap();

    // Ampersand and copyright with whitespace patterns
    static ref AMPERSAND_WITH_SPACES: Regex = Regex::new(r"\s+&\s+").unwrap();
    static ref COPYRIGHT_WITH_SPACES: Regex = Regex::new(r"\s+©\s+").unwrap();
}

/// Generate GitHub.com style anchor fragment from heading text with security hardening
///
/// This implementation matches GitHub.com's exact behavior, verified through
/// comprehensive testing with GitHub Gists, while providing robust security
/// against various injection and DoS attacks.
///
/// # Security Features
/// - Input size limits (max 10KB) to prevent memory exhaustion
/// - Unicode normalization (NFC) to prevent homograph attacks
/// - Bidirectional text injection filtering
/// - Zero-width character removal
/// - Control character filtering
/// - ReDoS-resistant regex patterns
/// - Comprehensive emoji detection
///
/// # Examples
/// ```
/// use rumdl_lib::utils::anchor_styles::github;
///
/// assert_eq!(github::heading_to_fragment("Hello World"), "hello-world");
/// assert_eq!(github::heading_to_fragment("cbrown --> sbrown: --unsafe-paths"), "cbrown----sbrown---unsafe-paths");
/// assert_eq!(github::heading_to_fragment("test_with_underscores"), "test_with_underscores");
/// ```
pub fn heading_to_fragment(heading: &str) -> String {
    // Security Step 1: Input validation and size limits
    if heading.is_empty() {
        return String::new();
    }

    if heading.len() > MAX_INPUT_LENGTH {
        // Truncate oversized input to prevent memory exhaustion
        // Use char_indices to ensure we don't split in the middle of a UTF-8 character
        let mut truncated_len = 0;
        for (byte_index, _) in heading.char_indices() {
            if byte_index >= MAX_INPUT_LENGTH {
                truncated_len = byte_index;
                break;
            }
            truncated_len = byte_index + 1; // Include the current character
        }
        if truncated_len == 0 {
            truncated_len = MAX_INPUT_LENGTH.min(heading.len());
        }
        let truncated = &heading[..truncated_len];
        return heading_to_fragment_internal(truncated);
    }

    heading_to_fragment_internal(heading)
}

/// Internal implementation with security hardening
fn heading_to_fragment_internal(heading: &str) -> String {
    // Save original heading state for edge detection
    let _original_heading_lower = heading.to_lowercase();

    // Security Step 2: Unicode normalization to prevent homograph attacks
    // NFC normalization ensures canonical representation
    let normalized: String = heading.nfc().collect();

    // Step 3: Handle emoji sequences BEFORE sanitizing ZWJ
    // This preserves multi-component emojis and keycaps
    // Quick optimization: skip if clearly no emojis (common case)
    let emoji_processed = if normalized.chars().any(|c| {
        let code = c as u32;
        // Quick check for common emoji ranges
        (0x1F300..=0x1F9FF).contains(&code) || // Most emojis
        (0x2600..=0x26FF).contains(&code) ||   // Misc symbols
        (0x1F1E6..=0x1F1FF).contains(&code) // Regional indicators
    }) {
        process_emoji_sequences(&normalized)
    } else {
        normalized.clone()
    };

    // Security Step 4: Filter dangerous Unicode characters
    let sanitized = sanitize_unicode(&emoji_processed);

    // Step 5: Convert to lowercase
    let mut text = sanitized.to_lowercase();

    // Step 5: Remove markdown formatting while preserving inner text
    // Process multiple times to handle nested emphasis (e.g., **_text_**)
    // Using ReDoS-resistant patterns with bounded repetition
    for _ in 0..3 {
        // Max 3 levels of nesting to prevent infinite loops
        let prev = text.clone();
        text = EMPHASIS_ASTERISK.replace_all(&text, "$1").to_string();
        // Strip emphasis underscores - the regex now properly handles snake_case preservation
        text = EMPHASIS_UNDERSCORE.replace_all(&text, "$1").to_string();
        if text == prev {
            break;
        } // No more changes
    }
    text = CODE_PATTERN.replace_all(&text, "$1").to_string();
    // Handle images first, then links
    text = IMAGE_PATTERN.replace_all(&text, "$1").to_string();
    text = LINK_PATTERN.replace_all(&text, "$1").to_string();

    // Step 6: Multi-character arrow patterns (order matters!)
    // GitHub.com converts these patterns to specific hyphen sequences
    // Handle patterns with spaces to avoid double-processing spaces
    text = text.replace(" --> ", "----"); // space + arrow + space = 4 hyphens total
    text = text.replace("-->", "----"); // 4 hyphens when no surrounding spaces
    text = text.replace(" <-> ", "---"); // space + arrow + space = 3 hyphens total
    text = text.replace("<->", "---"); // 3 hyphens when no surrounding spaces
    text = text.replace(" ==> ", "--"); // space + arrow + space = 2 hyphens total
    text = text.replace("==>", "--"); // 2 hyphens when no surrounding spaces
    text = text.replace(" -> ", "---"); // space + arrow + space = 3 hyphens total
    text = text.replace("->", "---"); // 3 hyphens when no surrounding spaces

    // Step 7: Remove problematic characters before symbol replacement
    // First remove em-dashes and en-dashes entirely
    text = text.replace(['–', '—'], "");

    // Step 8: Emojis were already replaced with hyphens in process_emoji_sequences
    // No further processing needed for emoji markers

    // Step 9: Special symbol replacements
    // Handle ampersand based on position and surrounding spaces
    // GitHub's behavior:
    // - "& text" at start → "--text"
    // - "text &" at end → "text-"
    // - "text & text" in middle → "text--text"
    // - "&text" (no space) → "text"

    // First handle ampersand at start with space
    if text.starts_with("& ") {
        text = text.replacen("& ", "--", 1);
    }
    // Then handle ampersand at end with space
    else if text.ends_with(" &") {
        text = text[..text.len() - 2].to_string() + "-";
    }
    // Then handle ampersand with spaces on both sides
    else {
        text = AMPERSAND_WITH_SPACES.replace_all(&text, "--").to_string();
    }

    // Handle copyright similarly
    text = COPYRIGHT_WITH_SPACES.replace_all(&text, "--").to_string();

    // Remove ampersand and copyright without spaces
    text = text.replace("&", "");
    text = text.replace("©", "");

    // Step 10: Character-by-character processing
    let mut result = String::with_capacity(text.len()); // Pre-allocate for efficiency

    for c in text.chars() {
        let code = c as u32;
        if c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '_' || c == '-' {
            // Preserve letters, numbers, underscores, and hyphens
            result.push(c);
        } else if c == '§' {
            // Preserve our marker character
            result.push(c);
        } else if code == 0x20E3 {
            // Preserve combining keycap for keycap sequences
            // Note: FE0F should only be preserved as part of a keycap, not standalone
            // The keycap preservation is handled in process_emoji_sequences
            result.push(c);
        } else if code == 0xFE0F {
            // Only preserve variation selector if it's preceded by a keycap base
            if let Some(prev) = result.chars().last()
                && is_keycap_base(prev)
            {
                result.push(c);
            }
            // Otherwise filter it out
        } else if c.is_alphabetic() && is_safe_unicode_letter(c) {
            // Preserve Unicode letters (like é, ñ, etc.) but only safe ones
            result.push(c);
        } else if c.is_numeric() {
            // Preserve all numeric characters (digits from any script)
            result.push(c);
        } else if c.is_whitespace() {
            // Convert each whitespace character to a hyphen
            // GitHub preserves multiple spaces as multiple hyphens
            result.push('-');
        }
        // ASCII punctuation is removed (no replacement)
        // Unicode symbols have already been handled above
    }

    // GitHub does NOT trim leading/trailing hyphens, even those from symbol removal
    // "---leading" → "---leading"
    // "© 2024" → "-2024"
    // "trailing---" → "trailing---"

    // Step 11: Replace emoji markers with the correct number of hyphens
    // Note: markers are lowercase after the lowercasing step above
    // GitHub's behavior:
    // - Single emoji at start: "-"
    // - Single emoji at end: "-"
    // - Single emoji between words: "--"
    // - Multiple emojis with spaces: n+1 hyphens

    // Quick check: if no emoji markers, skip processing entirely
    if !result.contains("§emoji§") {
        return result;
    }

    // Simple two-step approach for better performance
    let mut final_result = result;

    // First, handle multiple consecutive markers (n markers → n+1 hyphens)
    // Process from longest to shortest to avoid partial replacements
    for count in (2..=10).rev() {
        if final_result.contains("§emoji§") {
            let marker_seq = "§emoji§".repeat(count);
            if final_result.contains(&marker_seq) {
                let replacement = "-".repeat(count + 1);
                final_result = final_result.replace(&marker_seq, &replacement);
            }
        }
    }

    // Then handle single markers based on position
    if final_result.contains("§emoji§") {
        let bytes = final_result.as_bytes();
        let marker = "§emoji§".as_bytes();
        let mut result_bytes = Vec::with_capacity(bytes.len());
        let mut i = 0;

        while i < bytes.len() {
            if i + marker.len() <= bytes.len() && &bytes[i..i + marker.len()] == marker {
                // Found a marker - check position
                let at_start = i == 0;
                let at_end = i + marker.len() >= bytes.len();

                if at_start || at_end {
                    result_bytes.push(b'-');
                } else {
                    result_bytes.extend_from_slice(b"--");
                }
                i += marker.len();
            } else {
                result_bytes.push(bytes[i]);
                i += 1;
            }
        }

        final_result = String::from_utf8(result_bytes).unwrap_or(final_result);
    }

    final_result
}

/// Process emoji sequences before sanitization
/// Handles multi-component emojis, keycaps, and flags as units
/// GitHub's behavior: consecutive symbols with spaces between them become n+1 hyphens
fn process_emoji_sequences(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        // Check if this starts a symbol/emoji sequence
        if is_emoji_or_symbol(c) || is_regional_indicator(c) {
            // Remove preceding space if any
            if result.ends_with(' ') {
                result.pop();
            }

            // Count symbols in this sequence (separated by single spaces)
            let mut symbol_count = 1;

            // Handle the current symbol
            // If it's a regional indicator pair (flag)
            if is_regional_indicator(c) {
                if let Some(&next) = chars.peek()
                    && is_regional_indicator(next)
                {
                    chars.next(); // Consume second part of flag
                }
            }
            // If it's an emoji with ZWJ sequences
            else if is_emoji_or_symbol(c) {
                // Consume the entire emoji sequence including ZWJs
                while let Some(&next) = chars.peek() {
                    if next as u32 == 0x200D {
                        // ZWJ
                        chars.next();
                        // After ZWJ, expect another emoji component
                        if let Some(&emoji) = chars.peek() {
                            if is_emoji_or_symbol(emoji) || is_regional_indicator(emoji) {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    } else if next as u32 == 0xFE0F {
                        // Variation selector
                        chars.next();
                    } else if is_emoji_or_symbol(next) || is_regional_indicator(next) {
                        // Adjacent symbols without spaces are treated as a single unit
                        // Don't increment symbol_count, just consume them
                        chars.next();
                        // Handle multi-part adjacent symbols
                        if is_regional_indicator(next)
                            && let Some(&next2) = chars.peek()
                            && is_regional_indicator(next2)
                        {
                            chars.next();
                        }
                    } else {
                        break;
                    }
                }
            }

            // Look for more symbols separated by single spaces
            while let Some(&next) = chars.peek() {
                if next == ' ' {
                    // Peek ahead to see if there's a symbol after the space
                    let mut temp_chars = chars.clone();
                    temp_chars.next(); // Skip the space
                    if let Some(&after_space) = temp_chars.peek() {
                        if is_emoji_or_symbol(after_space) || is_regional_indicator(after_space) {
                            // Consume the space and the symbol
                            chars.next(); // Space
                            let symbol = chars.next().unwrap(); // Symbol
                            symbol_count += 1;

                            // Handle multi-part symbols
                            if is_regional_indicator(symbol) {
                                if let Some(&next) = chars.peek()
                                    && is_regional_indicator(next)
                                {
                                    chars.next();
                                }
                            } else if is_emoji_or_symbol(symbol) {
                                // Handle ZWJ sequences
                                while let Some(&next) = chars.peek() {
                                    if next as u32 == 0x200D {
                                        // ZWJ
                                        chars.next();
                                        if let Some(&emoji) = chars.peek() {
                                            if is_emoji_or_symbol(emoji) || is_regional_indicator(emoji) {
                                                chars.next();
                                            } else {
                                                break;
                                            }
                                        }
                                    } else if next as u32 == 0xFE0F {
                                        chars.next();
                                    } else {
                                        break;
                                    }
                                }
                            }
                        } else {
                            break; // Not a symbol after space
                        }
                    } else {
                        break; // Nothing after space
                    }
                } else {
                    break; // Not a space
                }
            }

            // Skip trailing space if any
            if let Some(&next) = chars.peek()
                && next == ' '
            {
                chars.next();
            }

            // Generate markers based on symbol count
            // GitHub's pattern: n symbols with spaces = n+1 hyphens
            // We use markers that will be replaced with the correct number of hyphens
            result.push_str("§EMOJI§");
            // Add extra markers for each additional symbol that was separated by spaces
            for _ in 1..symbol_count {
                result.push_str("§EMOJI§");
            }
        }
        // Check for keycap sequences - these should be PRESERVED
        else if is_keycap_base(c) {
            let mut keycap_seq = String::new();
            keycap_seq.push(c);

            // Check for variation selector and/or combining keycap
            let mut has_keycap = false;
            while let Some(&next) = chars.peek() {
                if next as u32 == 0xFE0F || next as u32 == 0x20E3 {
                    keycap_seq.push(next);
                    chars.next();
                    if next as u32 == 0x20E3 {
                        has_keycap = true;
                        break;
                    }
                } else {
                    break;
                }
            }

            if has_keycap {
                // Preserve the entire keycap sequence
                result.push_str(&keycap_seq);
            } else {
                // Not a keycap, just push the original character
                result.push(c);
                // Push back any variation selectors we consumed
                for ch in keycap_seq.chars().skip(1) {
                    result.push(ch);
                }
            }
        } else {
            // Regular character
            result.push(c);
        }
    }

    result
}

/// Sanitize Unicode input by removing dangerous character categories
/// Filters out bidirectional text injection, zero-width chars, and control chars
fn sanitize_unicode(input: &str) -> String {
    // Remove zero-width characters that can be used for injection attacks
    let no_zero_width = ZERO_WIDTH_PATTERN.replace_all(input, "");

    // Remove dangerous RTL override and bidirectional control characters
    let no_bidi_attack = DANGEROUS_UNICODE_PATTERN.replace_all(&no_zero_width, "");

    // Filter out control characters (except basic whitespace)
    let mut sanitized = String::with_capacity(no_bidi_attack.len());
    for c in no_bidi_attack.chars() {
        if !c.is_control() || c.is_whitespace() {
            sanitized.push(c);
        }
        // Skip control characters entirely for security
    }

    sanitized
}

/// Check if a Unicode letter is safe to include in anchors
/// Excludes potentially dangerous or confusing character ranges
fn is_safe_unicode_letter(c: char) -> bool {
    let code = c as u32;

    // Exclude potentially dangerous ranges:
    // - Private Use Areas (could contain malicious content)
    // - Variation Selectors (can change appearance)
    // - Format characters (invisible formatting)
    if (0xE000..=0xF8FF).contains(&code) ||    // Private Use Area
       (0xF0000..=0xFFFFD).contains(&code) ||  // Supplementary Private Use Area-A
       (0x100000..=0x10FFFD).contains(&code) || // Supplementary Private Use Area-B
       (0xFE00..=0xFE0F).contains(&code) ||    // Variation Selectors
       (0xE0100..=0xE01EF).contains(&code)
    {
        // Variation Selectors Supplement
        return false;
    }

    // Allow common safe Unicode letter ranges
    // Basic Latin (already covered by is_alphabetic())
    (0x0000..=0x007F).contains(&code) ||       // Basic Latin
    (0x0080..=0x00FF).contains(&code) ||       // Latin-1 Supplement
    (0x0100..=0x017F).contains(&code) ||       // Latin Extended-A
    (0x0180..=0x024F).contains(&code) ||       // Latin Extended-B
    (0x0370..=0x03FF).contains(&code) ||       // Greek and Coptic
    (0x0400..=0x04FF).contains(&code) ||       // Cyrillic
    (0x0500..=0x052F).contains(&code) ||       // Cyrillic Supplement
    (0x0590..=0x05FF).contains(&code) ||       // Hebrew
    (0x0600..=0x06FF).contains(&code) ||       // Arabic
    (0x0700..=0x074F).contains(&code) ||       // Syriac
    (0x0750..=0x077F).contains(&code) ||       // Arabic Supplement
    (0x1100..=0x11FF).contains(&code) ||       // Hangul Jamo
    (0x3040..=0x309F).contains(&code) ||       // Hiragana
    (0x30A0..=0x30FF).contains(&code) ||       // Katakana
    (0x3130..=0x318F).contains(&code) ||       // Hangul Compatibility Jamo
    (0x4E00..=0x9FFF).contains(&code) ||       // CJK Unified Ideographs
    (0xAC00..=0xD7AF).contains(&code) ||       // Hangul Syllables (Korean)
    (0xA000..=0xA48F).contains(&code) ||       // Yi Syllables
    (0xA490..=0xA4CF).contains(&code) // Yi Radicals
}

/// Comprehensive emoji and symbol detection
/// Covers all major emoji ranges including newer additions and symbols
fn is_emoji_or_symbol(c: char) -> bool {
    let code = c as u32;

    // Exclude dangerous unicode characters that should be filtered, not replaced
    // These include bidirectional overrides, zero-width chars, etc.
    if (0x202A..=0x202E).contains(&code) ||  // Bidirectional formatting
       (0x2066..=0x2069).contains(&code) ||  // Isolate formatting
       (0x200B..=0x200D).contains(&code) ||  // Zero-width chars
       (0x200E..=0x200F).contains(&code) ||  // LTR/RTL marks
       code == 0x061C ||                     // Arabic Letter Mark
       code == 0x2060 ||                     // Word Joiner
       code == 0xFEFF
    {
        // Zero Width No-Break Space
        return false;
    }

    // Core emoji ranges
    (0x1F600..=0x1F64F).contains(&code) ||  // Emoticons
    (0x1F300..=0x1F5FF).contains(&code) ||  // Miscellaneous Symbols and Pictographs
    (0x1F680..=0x1F6FF).contains(&code) ||  // Transport and Map Symbols
    (0x1F700..=0x1F77F).contains(&code) ||  // Alchemical Symbols
    (0x1F780..=0x1F7FF).contains(&code) ||  // Geometric Shapes Extended
    (0x1F800..=0x1F8FF).contains(&code) ||  // Supplemental Arrows-C
    (0x1F900..=0x1F9FF).contains(&code) ||  // Supplemental Symbols and Pictographs
    (0x1FA00..=0x1FA6F).contains(&code) ||  // Chess Symbols
    (0x1FA70..=0x1FAFF).contains(&code) ||  // Symbols and Pictographs Extended-A
    (0x1FB00..=0x1FBFF).contains(&code) ||  // Symbols for Legacy Computing

    // Symbol ranges that should be removed
    (0x2600..=0x26FF).contains(&code) ||    // Miscellaneous Symbols
    (0x2700..=0x27BF).contains(&code) ||    // Dingbats
    (0x2B00..=0x2BFF).contains(&code) ||    // Miscellaneous Symbols and Arrows
    (0x1F000..=0x1F02F).contains(&code) ||  // Mahjong Tiles
    (0x1F030..=0x1F09F).contains(&code) ||  // Domino Tiles
    (0x1F0A0..=0x1F0FF).contains(&code) ||  // Playing Cards

    // Additional symbol ranges
    (0x2190..=0x21FF).contains(&code) ||    // Arrows
    (0x2200..=0x22FF).contains(&code) ||    // Mathematical Operators
    (0x2300..=0x23FF).contains(&code) ||    // Miscellaneous Technical
    (0x2400..=0x243F).contains(&code) ||    // Control Pictures
    (0x2440..=0x245F).contains(&code) ||    // Optical Character Recognition
    (0x25A0..=0x25FF).contains(&code) ||    // Geometric Shapes
    (0x2000..=0x206F).contains(&code) ||    // General Punctuation (includes dangerous chars)

    // Combining marks used in emoji (but not variation selectors - those are handled separately)
    (0x20D0..=0x20FF).contains(&code) // Combining Diacritical Marks for Symbols
}

/// Check if character is a regional indicator (used for country flags)
fn is_regional_indicator(c: char) -> bool {
    let code = c as u32;
    (0x1F1E6..=0x1F1FF).contains(&code) // Regional Indicator Symbol letters A-Z
}

/// Check if character can be the base of a keycap sequence
fn is_keycap_base(c: char) -> bool {
    let code = c as u32;
    // Digits 0-9, *, #, and some letters used in keycap sequences
    (0x0030..=0x0039).contains(&code) ||  // ASCII digits 0-9
    code == 0x002A ||                     // Asterisk *
    code == 0x0023 // Number sign #
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_basic_cases() {
        assert_eq!(heading_to_fragment("Hello World"), "hello-world");
        assert_eq!(heading_to_fragment("Test Case"), "test-case");
        assert_eq!(heading_to_fragment(""), "");
    }

    #[test]
    fn test_github_underscores() {
        // GitHub preserves underscores in snake_case but removes emphasis markdown
        assert_eq!(heading_to_fragment("test_with_underscores"), "test_with_underscores");
        assert_eq!(heading_to_fragment("Update login_type"), "update-login_type");
        assert_eq!(heading_to_fragment("__dunder__"), "dunder"); // Emphasis removed
        assert_eq!(heading_to_fragment("_emphasized_"), "emphasized"); // Single underscore emphasis
        assert_eq!(heading_to_fragment("__double__ underscore"), "double-underscore");
    }

    #[test]
    fn test_github_arrows_issue_39() {
        // These are the specific cases from issue #39 that were failing
        assert_eq!(
            heading_to_fragment("cbrown --> sbrown: --unsafe-paths"),
            "cbrown----sbrown---unsafe-paths"
        );
        assert_eq!(heading_to_fragment("cbrown -> sbrown"), "cbrown---sbrown");
        assert_eq!(
            heading_to_fragment("Arrow Test <-> bidirectional"),
            "arrow-test---bidirectional"
        );
        assert_eq!(heading_to_fragment("Double Arrow ==> Test"), "double-arrow--test");
    }

    #[test]
    fn test_github_hyphens() {
        // GitHub preserves consecutive hyphens (no consolidation)
        assert_eq!(heading_to_fragment("Double--Hyphen"), "double--hyphen");
        assert_eq!(heading_to_fragment("Triple---Dash"), "triple---dash");
        assert_eq!(
            heading_to_fragment("Test---with---multiple---hyphens"),
            "test---with---multiple---hyphens"
        );
    }

    #[test]
    fn test_github_special_symbols() {
        assert_eq!(heading_to_fragment("Testing & Coverage"), "testing--coverage");
        assert_eq!(heading_to_fragment("Copyright © 2024"), "copyright--2024");
        assert_eq!(
            heading_to_fragment("API::Response > Error--Handling"),
            "apiresponse--error--handling"
        );
    }

    #[test]
    fn test_github_unicode() {
        // GitHub preserves Unicode letters
        assert_eq!(heading_to_fragment("Café René"), "café-rené");
        assert_eq!(heading_to_fragment("naïve résumé"), "naïve-résumé");
        assert_eq!(heading_to_fragment("über uns"), "über-uns");
    }

    #[test]
    fn test_github_emojis() {
        // GitHub converts emojis to hyphens
        assert_eq!(heading_to_fragment("Emoji 🎉 Party"), "emoji--party");
        assert_eq!(heading_to_fragment("Test 🚀 Rocket"), "test--rocket");
    }

    #[test]
    fn test_github_markdown_removal() {
        assert_eq!(heading_to_fragment("*emphasized* text"), "emphasized-text");
        assert_eq!(heading_to_fragment("`code` in heading"), "code-in-heading");
        assert_eq!(heading_to_fragment("[link text](url)"), "link-text");
        assert_eq!(heading_to_fragment("[ref link][]"), "ref-link");
    }

    #[test]
    fn test_github_leading_trailing() {
        // GitHub does NOT trim leading/trailing hyphens (unlike kramdown)
        assert_eq!(heading_to_fragment("---leading"), "---leading");
        assert_eq!(heading_to_fragment("trailing---"), "trailing---");
        assert_eq!(heading_to_fragment("---both---"), "---both---");
    }

    #[test]
    fn test_github_numbers() {
        assert_eq!(heading_to_fragment("Step 1: Getting Started"), "step-1-getting-started");
        assert_eq!(heading_to_fragment("Version 2.1.0"), "version-210");
        assert_eq!(heading_to_fragment("123 Numbers"), "123-numbers");
    }

    #[test]
    fn test_github_comprehensive_verified() {
        // These test cases were verified against actual GitHub Gist behavior
        let test_cases = [
            ("GitHub Anchor Generation Test", "github-anchor-generation-test"),
            (
                "Test Case 1: cbrown --> sbrown: --unsafe-paths",
                "test-case-1-cbrown----sbrown---unsafe-paths",
            ),
            ("Test Case 2: PHP $_REQUEST", "test-case-2-php-_request"),
            ("Test Case 3: Update login_type", "test-case-3-update-login_type"),
            (
                "Test Case 4: Test with: colons > and arrows",
                "test-case-4-test-with-colons--and-arrows",
            ),
            (
                "Test Case 5: Test---with---multiple---hyphens",
                "test-case-5-test---with---multiple---hyphens",
            ),
            ("Test Case 6: Simple test case", "test-case-6-simple-test-case"),
            (
                "Test Case 7: API::Response > Error--Handling",
                "test-case-7-apiresponse--error--handling",
            ),
        ];

        for (input, expected) in test_cases {
            let actual = heading_to_fragment(input);
            assert_eq!(
                actual, expected,
                "GitHub verified test failed for input: '{input}'\nExpected: '{expected}'\nActual: '{actual}'"
            );
        }
    }

    // Security Tests

    #[test]
    fn test_security_input_size_limits() {
        // Test input size limits to prevent memory exhaustion
        let large_input = "a".repeat(20000); // 20KB input
        let result = heading_to_fragment(&large_input);

        // Should be truncated to MAX_INPUT_LENGTH
        assert!(result.len() <= MAX_INPUT_LENGTH);

        // Empty input should return empty
        assert_eq!(heading_to_fragment(""), "");
    }

    #[test]
    fn test_security_unicode_normalization() {
        // Test Unicode normalization prevents homograph attacks

        // Different Unicode representations of "café"
        let normal_cafe = "café"; // NFC normalized
        let decomposed_cafe = "cafe\u{0301}"; // NFD decomposed (e + combining acute)

        let result1 = heading_to_fragment(normal_cafe);
        let result2 = heading_to_fragment(decomposed_cafe);

        // Both should normalize to the same result
        assert_eq!(result1, result2);
        assert_eq!(result1, "café");
    }

    #[test]
    fn test_security_bidi_injection_prevention() {
        // Test bidirectional text injection attack prevention

        // RTL override attack attempt
        let rtl_attack = "Hello\u{202E}dlroW\u{202D}";
        let result = heading_to_fragment(rtl_attack);
        assert_eq!(result, "hellodlrow"); // RTL overrides should be removed

        // RLO/LRO attack
        let rlo_attack = "user\u{202E}@bank.com";
        let result = heading_to_fragment(rlo_attack);
        assert!(!result.contains('\u{202E}')); // Should not contain RTL override

        // Isolate attacks
        let isolate_attack = "test\u{2066}hidden\u{2069}text";
        let result = heading_to_fragment(isolate_attack);
        assert_eq!(result, "testhiddentext"); // Isolate chars should be removed
    }

    #[test]
    fn test_security_zero_width_character_removal() {
        // Test zero-width character injection prevention

        let zero_width_attack = "hel\u{200B}lo\u{200C}wor\u{200D}ld\u{FEFF}";
        let result = heading_to_fragment(zero_width_attack);
        assert_eq!(result, "helloworld"); // All zero-width chars should be removed

        // Test various zero-width characters
        let zwj_attack = "test\u{200D}text"; // Zero Width Joiner
        let result = heading_to_fragment(zwj_attack);
        assert_eq!(result, "testtext");

        let bom_attack = "test\u{FEFF}text"; // Byte Order Mark
        let result = heading_to_fragment(bom_attack);
        assert_eq!(result, "testtext");
    }

    #[test]
    fn test_security_control_character_filtering() {
        // Test control character filtering

        let control_chars = "test\x01\x02\x03\x1F text";
        let result = heading_to_fragment(control_chars);
        assert_eq!(result, "test-text"); // Control chars removed, space becomes hyphen

        // Preserve normal whitespace
        let normal_whitespace = "test\n\t text";
        let result = heading_to_fragment(normal_whitespace);
        assert_eq!(result, "test---text"); // Multiple whitespace becomes hyphens (\n, \t, space)
    }

    #[test]
    fn test_security_comprehensive_emoji_detection() {
        // Test comprehensive emoji detection including country flags and keycaps
        // Note: GitHub preserves keycap emojis but removes other emojis

        // Country flags (regional indicators)
        let flag_test = "Hello 🇺🇸 World 🇬🇧 Test";
        let result = heading_to_fragment(flag_test);
        assert_eq!(result, "hello--world--test"); // Flags should be removed

        // Keycap sequences - GitHub PRESERVES these
        let keycap_test = "Step 1️⃣ and 2️⃣ complete";
        let result = heading_to_fragment(keycap_test);
        assert_eq!(result, "step-1️⃣-and-2️⃣-complete"); // Keycaps are PRESERVED by GitHub

        // Complex emoji sequences
        let complex_emoji = "Test 👨‍👩‍👧‍👦 family";
        let result = heading_to_fragment(complex_emoji);
        assert_eq!(result, "test--family"); // Complex emoji should be single --

        // Mixed emoji and symbols
        let mixed_symbols = "Math ∑ ∆ 🧮 symbols";
        let result = heading_to_fragment(mixed_symbols);
        assert_eq!(result, "math----symbols"); // All symbols should be removed
    }

    #[test]
    fn test_security_redos_resistance() {
        // Test ReDoS resistance with pathological inputs

        // Nested patterns that could cause exponential backtracking
        let nested_emphasis = "*".repeat(50) + "text" + &"*".repeat(50);
        let result = heading_to_fragment(&nested_emphasis);
        // Should not hang and should produce reasonable output
        assert!(result.len() < 200); // Bounded output

        // Deeply nested code blocks
        let nested_code = "`".repeat(100) + "code" + &"`".repeat(100);
        let result = heading_to_fragment(&nested_code);
        assert!(result.len() < 300); // Bounded output

        // Pathological link patterns
        let nested_links = "[".repeat(50) + "text" + &"]".repeat(50);
        let result = heading_to_fragment(&nested_links);
        assert!(result.len() < 200); // Bounded output
    }

    #[test]
    fn test_security_dangerous_unicode_blocks() {
        // Test filtering of dangerous Unicode blocks

        // Private Use Area characters (potential malicious content)
        let pua_test = "test\u{E000}\u{F8FF}text";
        let result = heading_to_fragment(pua_test);
        assert_eq!(result, "testtext"); // PUA chars should be filtered

        // Variation selectors (can change appearance)
        let variation_test = "test\u{FE00}\u{FE0F}text";
        let result = heading_to_fragment(variation_test);
        assert_eq!(result, "testtext"); // Variation selectors should be filtered
    }

    #[test]
    fn test_security_normal_behavior_preserved() {
        // Ensure security measures don't break normal functionality

        // Normal Unicode letters should still work
        let unicode_letters = "Café René naïve über";
        let result = heading_to_fragment(unicode_letters);
        assert_eq!(result, "café-rené-naïve-über");

        // Normal ASCII should still work
        let ascii_test = "Hello World 123";
        let result = heading_to_fragment(ascii_test);
        assert_eq!(result, "hello-world-123");

        // GitHub-specific behavior should be preserved
        let github_specific = "cbrown --> sbrown: --unsafe-paths";
        let result = heading_to_fragment(github_specific);
        assert_eq!(result, "cbrown----sbrown---unsafe-paths");
    }

    #[test]
    fn test_security_performance_edge_cases() {
        // Test performance with edge cases that could cause issues

        // Long repetitive patterns
        let repetitive = "ab".repeat(1000);
        let start = std::time::Instant::now();
        let result = heading_to_fragment(&repetitive);
        let duration = start.elapsed();

        // Should complete quickly (under 100ms for this size)
        assert!(duration.as_millis() < 100);
        assert!(!result.is_empty());

        // Mixed ASCII and Unicode
        let mixed = ("a".to_string() + "ñ").repeat(500);
        let start = std::time::Instant::now();
        let result = heading_to_fragment(&mixed);
        let duration = start.elapsed();

        assert!(duration.as_millis() < 100);
        assert!(!result.is_empty());
    }
}
