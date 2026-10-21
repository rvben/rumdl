//! Encode HTML.

use alloc::string::String;

/// Encode dangerous html characters.
///
/// This ensures that certain characters which have special meaning in HTML are
/// dealt with.
/// Technically, we can skip `>` and `"` in many cases, but `CommonMark`
/// includes them.
///
/// This behavior is not explained in prose in `CommonMark` but can be inferred
/// from the input/output test cases.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::encode;
///
/// assert_eq!(encode("I <3 🦀"), "I &lt;3 🦀");
/// ```
///
/// ## References
///
/// * [`micromark-util-encode` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-encode)
pub fn encode(value: &str, encode_html: bool) -> String {
    // It’ll grow a bit bigger for each dangerous character.
    let mut result = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut start = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if matches!(byte, b'\0') || (encode_html && matches!(byte, b'&' | b'"' | b'<' | b'>')) {
            result.push_str(&value[start..index]);
            result.push_str(match byte {
                b'\0' => "�",
                b'&' => "&amp;",
                b'"' => "&quot;",
                b'<' => "&lt;",
                // `b'>'`
                _ => "&gt;",
            });

            start = index + 1;
        }

        index += 1;
    }

    result.push_str(&value[start..]);

    result
}
