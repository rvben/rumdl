//! Normalize identifiers.

use alloc::string::String;

/// Normalize an identifier, as found in [references][label_end] and
/// [definitions][definition], so it can be compared when matching.
///
/// This collapsed whitespace found in markdown (`\t`, `\r`, `\n`, and ` `)
/// into one space, trims it (as in, dropping the first and last space), and
/// then performs unicode case folding twice: first by lowercasing uppercase
/// characters, and then uppercasing lowercase characters.
///
/// Some characters are considered “uppercase”, such as U+03F4 (`ϴ`), but if
/// their lowercase counterpart (U+03B8 (`θ`)) is uppercased will result in a
/// different uppercase character (U+0398 (`Θ`)).
/// Hence, to get that form, we perform both lower- and uppercase.
///
/// Performing these steps in that order works, but the inverse does not work.
/// To illustrate, say the source markdown containes two identifiers
/// `SS` (U+0053 U+0053) and `ẞ` (U+1E9E), which would be lowercased to
/// `ss` (U+0073 U+0073) and `ß` (U+00DF), and those in turn would both
/// uppercase to `SS` (U+0053 U+0053).
/// If we’d inverse the steps, for `ẞ`, we’d first uppercase without a
/// change, and then lowercase to `ß`, which would not match `ss`.
///
/// ## Examples
///
/// ```rust ignore
/// markdown::util::normalize_identifier::normalize_identifier;
///
/// assert_eq!(normalize_identifier(" a "), "A");
/// assert_eq!(normalize_identifier("a\t\r\nb"), "A B");
/// assert_eq!(normalize_identifier("ПРИВЕТ"), "ПРИВЕТ");
/// assert_eq!(normalize_identifier("Привет"), "ПРИВЕТ");
/// assert_eq!(normalize_identifier("привет"), "ПРИВЕТ");
/// ```
///
/// ## References
///
/// * [`micromark-util-normalize-identifier` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-normalize-identifier)
///
/// [definition]: crate::mdx_parser::construct::definition
/// [label_end]: crate::mdx_parser::construct::label_end
pub fn normalize_identifier(value: &str) -> String {
    // Note: it’ll grow a bit smaller for consecutive whitespace.
    let mut result = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut in_whitespace = true;
    let mut index = 0;
    let mut start = 0;

    while index < bytes.len() {
        if matches!(bytes[index], b'\t' | b'\n' | b'\r' | b' ') {
            // First whitespace we see after non-whitespace.
            if !in_whitespace {
                result.push_str(&value[start..index]);
                in_whitespace = true;
            }
        }
        // First non-whitespace we see after whitespace.
        else if in_whitespace {
            if !result.is_empty() {
                result.push(' ');
            }

            start = index;
            in_whitespace = false;
        }

        index += 1;
    }

    if !in_whitespace {
        result.push_str(&value[start..]);
    }

    result.to_lowercase().to_uppercase()
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    #[test]
    fn test_normalize_identifier_keeps_brackets() {
        // `label_end` relies on this: a label with an unescaped bracket never
        // normalizes to the identifier of a definition, which has none.
        for code in 0..=0x10_FFFF_u32 {
            if let Some(char) = char::from_u32(code) {
                if matches!(char, '[' | ']' | '\\') {
                    continue;
                }
                let folded = char.to_string().to_lowercase().to_uppercase();
                assert!(
                    !folded.contains(['[', ']', '\\']),
                    "should not fold {:?} into a bracket or backslash",
                    char
                );
            }
        }
    }

    use super::*;

    #[test]
    fn test_normalize_identifier() {
        assert_eq!(normalize_identifier("a b"), "A B", "should keep a space between words");
        assert_eq!(
            normalize_identifier(" a  b "),
            "A B",
            "should collapse and trim whitespace"
        );
        assert_eq!(
            normalize_identifier("a\t\r\nb"),
            "A B",
            "should collapse markdown whitespace"
        );
        assert_eq!(normalize_identifier("ẞ"), "SS", "should fold case");
    }
}
