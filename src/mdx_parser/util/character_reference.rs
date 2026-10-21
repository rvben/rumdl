//! Helpers for character references.

use crate::mdx_parser::util::constant::{
    CHARACTER_REFERENCE_DECIMAL_SIZE_MAX, CHARACTER_REFERENCE_HEXADECIMAL_SIZE_MAX, CHARACTER_REFERENCE_NAMED_SIZE_MAX,
    CHARACTER_REFERENCES, CHARACTER_REFERENCES_HTML_4,
};
use alloc::string::String;
use core::str;

/// Decode named character references.
///
/// Turn the name coming from a named character reference (without the `&` or
/// `;`) into a string.
/// This looks the given string up at `0` in the tuples of
/// `CHARACTER_REFERENCES` (or `CHARACTER_REFERENCES_HTML_4`)
/// and then takes the corresponding value from `1`.
///
/// The `html5` boolean is used for named character references, and specifier
/// whether the 2125 names from HTML 5 or the 252 names from HTML 4 are
/// supported.
///
/// The result is `String` instead of `char` because named character references
/// can expand into multiple characters.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::decode_character_reference::decode_named;
///
/// assert_eq!(decode_named("amp", true), "&");
/// assert_eq!(decode_named("AElig", true), "Æ");
/// assert_eq!(decode_named("aelig", true), "æ");
/// ```
///
/// ## References
///
/// * [`wooorm/decode-named-character-reference`](https://github.com/wooorm/decode-named-character-reference)
/// * [*§ 2.5 Entity and numeric character references* in `CommonMark`](https://spec.commonmark.org/0.31/#entity-and-numeric-character-references)
pub fn decode_named(value: &str, html5: bool) -> Option<String> {
    let mut iter = if html5 {
        CHARACTER_REFERENCES.iter()
    } else {
        CHARACTER_REFERENCES_HTML_4.iter()
    };
    iter.find(|d| d.0 == value).map(|d| d.1.into())
}

/// Decode numeric character references.
///
/// Turn the number (in string form as either hexadecimal or decimal) coming
/// from a numeric character reference into a string.
/// The base of the string form must be passed as the `radix` parameter, as
/// `10` (decimal) or `16` (hexadecimal).
///
/// This returns a `String` form of the associated character or a replacement
/// character for C0 control characters (except for ASCII whitespace), C1
/// control characters, lone surrogates, noncharacters, and out of range
/// characters.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::decode_character_reference::decode_numeric;
///
/// assert_eq!(decode_numeric("123", 10), "{");
/// assert_eq!(decode_numeric("9", 16), "\t");
/// assert_eq!(decode_numeric("0", 10), "�"); // Not allowed.
/// ```
///
/// ## Panics
///
/// This function panics if a invalid string or an out of bounds valid string
/// is given.
/// It is expected that figuring out whether a number is allowed is handled in
/// the parser.
/// When `markdown-rs` is used, this function never panics.
///
/// ## References
///
/// * [`micromark-util-decode-numeric-character-reference` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-decode-numeric-character-reference)
/// * [*§ 2.5 Entity and numeric character references* in `CommonMark`](https://spec.commonmark.org/0.31/#entity-and-numeric-character-references)
pub fn decode_numeric(value: &str, radix: u32) -> String {
    if let Some(char) = char::from_u32(u32::from_str_radix(value, radix).unwrap()) {
        if !matches!(char,
            // C0 except for HT, LF, FF, CR, space.
            '\0'..='\u{08}' | '\u{0B}' | '\u{0E}'..='\u{1F}' |
            // Control character (DEL) of C0, and C1 controls.
            '\u{7F}'..='\u{9F}'
            // Lone surrogates, noncharacters, and out of range are handled by
            // Rust.
        ) {
            return char.into();
        }
    }

    char::REPLACEMENT_CHARACTER.into()
}

/// Decode a character reference.
///
/// This turns the number (in string form as either hexadecimal or decimal) or
/// name from a character reference into a string.
///
/// The marker specifies the format: `#` for hexadecimal, `x` for decimal, and
/// `&` for named.
///
/// The `html5` boolean is used for named character references, and specifier
/// whether the 2125 names from HTML 5 or the 252 names from HTML 4 are
/// supported.
///
/// ## Panics
///
/// Panics if `marker` is not `b'&'`, `b'x'`, or `b'#'`.
pub fn decode(value: &str, marker: u8, html5: bool) -> Option<String> {
    match marker {
        b'#' => Some(decode_numeric(value, 10)),
        b'x' => Some(decode_numeric(value, 16)),
        b'&' => decode_named(value, html5),
        _ => unreachable!("Unexpected marker `{}`", marker),
    }
}

/// Get the maximum size of a value for different kinds of references.
///
/// The value is the stuff after the markers, before the `;`.
///
/// ## Panics
///
/// Panics if `marker` is not `b'&'`, `b'x'`, or `b'#'`.
pub fn value_max(marker: u8) -> usize {
    match marker {
        b'&' => CHARACTER_REFERENCE_NAMED_SIZE_MAX,
        b'x' => CHARACTER_REFERENCE_HEXADECIMAL_SIZE_MAX,
        b'#' => CHARACTER_REFERENCE_DECIMAL_SIZE_MAX,
        _ => unreachable!("Unexpected marker `{}`", marker),
    }
}

/// Get a test to check if a byte is allowed as a value for different kinds of
/// references.
///
/// The value is the stuff after the markers, before the `;`.
///
/// ## Panics
///
/// Panics if `marker` is not `b'&'`, `b'x'`, or `b'#'`.
pub fn value_test(marker: u8) -> fn(&u8) -> bool {
    match marker {
        b'&' => u8::is_ascii_alphanumeric,
        b'x' => u8::is_ascii_hexdigit,
        b'#' => u8::is_ascii_digit,
        _ => unreachable!("Unexpected marker `{}`", marker),
    }
}

/// Decode character references in a string.
///
/// > 👉 **Note**: this currently only supports the 252 named character
/// > references from HTML 4, as it’s only used for JSX.
/// >
/// > If it’s ever needed to support HTML 5 (which is what normal markdown
/// > uses), a boolean parameter can be added here.
pub fn parse(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut index = 0;
    let len = bytes.len();
    // Grows a bit smaller with each character reference.
    let mut result = String::with_capacity(value.len());
    let mut start = 0;

    while index < len {
        if bytes[index] == b'&' {
            let (marker, value_start) = if index + 1 < len && bytes[index + 1] == b'#' {
                if index + 2 < len && matches!(bytes[index + 2], b'x' | b'X') {
                    (b'x', index + 3)
                } else {
                    (b'#', index + 2)
                }
            } else {
                (b'&', index + 1)
            };

            let max = value_max(marker);
            let test = value_test(marker);
            let mut value_index = 0;
            while value_index < max && (value_start + value_index) < len {
                if !test(&bytes[value_start + value_index]) {
                    break;
                }
                value_index += 1;
            }

            let value_end = value_start + value_index;

            // Non empty and terminated.
            if value_index > 0 && value_end < len && bytes[value_end] == b';' {
                if let Some(decoded) = decode(str::from_utf8(&bytes[value_start..value_end]).unwrap(), marker, false) {
                    result.push_str(&value[start..index]);
                    result.push_str(&decoded);
                    start = value_end + 1;
                    index = start;
                    continue;
                }
            }
        }

        index += 1;
    }

    result.push_str(&value[start..]);

    result
}
