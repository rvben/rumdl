//! Make urls safe.

use crate::mdx_parser::util::encode::encode;
use alloc::{format, string::String, vec::Vec};

/// Make a value safe for injection as a URL.
///
/// This encodes unsafe characters with percent-encoding and skips already
/// encoded sequences (see `normalize` below).
/// Further unsafe characters are encoded as character references (see
/// `encode`).
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::sanitize_uri::sanitize;
///
/// assert_eq!(sanitize("javascript:alert(1)"), "javascript:alert(1)");
/// assert_eq!(sanitize("https://a👍b.c/%20/%"), "https://a%F0%9F%91%8Db.c/%20/%25");
/// ```
///
/// ## References
///
/// * [`micromark-util-sanitize-uri` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-sanitize-uri)
#[must_use]
pub fn sanitize(value: &str) -> String {
    encode(&normalize(value), true)
}

/// Make a value safe for injection as a URL, and check protocols.
///
/// This first uses [`sanitize`][].
/// Then, a vec of (lowercase) allowed protocols can be given, in which case
/// the URL is ignored or kept.
///
/// For example, `&["http", "https", "irc", "ircs", "mailto", "xmpp"]`
/// can be used for `a[href]`, or `&["http", "https"]` for `img[src]`.
/// If the URL includes an unknown protocol (one not matched by `protocol`, such
/// as a dangerous example, `javascript:`), the value is ignored.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::sanitize_uri::sanitize_with_protocols;
///
/// assert_eq!(sanitize_with_protocols("javascript:alert(1)", &["http", "https"]), "");
/// assert_eq!(sanitize_with_protocols("https://example.com", &["http", "https"]), "https://example.com");
/// assert_eq!(sanitize_with_protocols("https://a👍b.c/%20/%", &["http", "https"]), "https://a%F0%9F%91%8Db.c/%20/%25");
/// ```
///
/// ## References
///
/// * [`micromark-util-sanitize-uri` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-sanitize-uri)
pub fn sanitize_with_protocols(value: &str, protocols: &[&str]) -> String {
    let value = sanitize(value);

    let end = value.find(|c| matches!(c, '?' | '#' | '/'));
    let mut colon = value.find(':');

    // If the first colon is after `?`, `#`, or `/`, it’s not a protocol.
    if let Some(end) = end {
        if let Some(index) = colon {
            if index > end {
                colon = None;
            }
        }
    }

    // If there is no protocol, it’s relative, and fine.
    if let Some(colon) = colon {
        // If it is a protocol, it should be allowed.
        let protocol = value[0..colon].to_lowercase();
        if !protocols.contains(&protocol.as_str()) {
            return String::new();
        }
    }

    value
}

/// Normalize a URL (such as used in [definitions][definition],
/// [references][label_end]).
///
/// It encodes unsafe characters with percent-encoding, skipping already encoded
/// sequences.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::sanitize_uri::normalize;
///
/// assert_eq!(sanitize_uri("https://example.com"), "https://example.com");
/// assert_eq!(sanitize_uri("https://a👍b.c/%20/%"), "https://a%F0%9F%91%8Db.c/%20/%25");
/// ```
///
/// ## References
///
/// * [`micromark-util-sanitize-uri` in `micromark`](https://github.com/micromark/micromark/tree/main/packages/micromark-util-sanitize-uri)
///
/// [definition]: crate::mdx_parser::construct::definition
/// [label_end]: crate::mdx_parser::construct::label_end
fn normalize(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    // Note: it’ll grow bigger for each non-ascii or non-safe character.
    let mut result = String::with_capacity(value.len());
    let mut index = 0;
    let mut start = 0;
    let mut buff = [0; 4];

    while index < chars.len() {
        let char = chars[index];

        // A correct percent encoded value.
        if char == '%'
            && index + 2 < chars.len()
            && chars[index + 1].is_ascii_alphanumeric()
            && chars[index + 2].is_ascii_alphanumeric()
        {
            index += 3;
            continue;
        }

        // Note: Rust already takes care of lone surrogates.
        // Non-ascii or not allowed ascii.
        if char >= '\u{0080}' || !matches!(char, '!' | '#' | '$' | '&'..=';' | '=' | '?'..='Z' | '_' | 'a'..='z' | '~')
        {
            result.push_str(&chars[start..index].iter().collect::<String>());
            char.encode_utf8(&mut buff);

            #[allow(clippy::format_collect)]
            result.push_str(
                &buff[0..char.len_utf8()]
                    .iter()
                    .map(|&byte| format!("%{:>02X}", byte))
                    .collect::<String>(),
            );

            start = index + 1;
        }

        index += 1;
    }

    result.push_str(&chars[start..].iter().collect::<String>());

    result
}
