//! Make dangerous HTML a tiny bit safer.

use crate::mdx_parser::util::constant::{GFM_HTML_TAGFILTER_NAMES, GFM_HTML_TAGFILTER_SIZE_MAX};
use alloc::string::String;
use core::str;

/// Make dangerous HTML a tiny bit safer.
///
/// The tagfilter is kinda weird and kinda useless.
/// The tag filter is a naïve attempt at XSS protection.
/// You should use a proper HTML sanitizing algorithm.
///
/// ## Examples
///
/// ```rust ignore
/// use markdown::util::gfm_tagfilter::gfm_tagfilter;
///
/// assert_eq!(gfm_tagfilter("<iframe>"), "&lt;iframe>");
/// ```
///
/// ## References
///
/// * [*§ 6.1 Disallowed Raw HTML (extension)* in GFM](https://github.github.com/gfm/#disallowed-raw-html-extension-)
/// * [`cmark-gfm#extensions/tagfilter.c`](https://github.com/github/cmark-gfm/blob/master/extensions/tagfilter.c)
pub fn gfm_tagfilter(value: &str) -> String {
    let bytes = value.as_bytes();
    // It’ll grow a bit bigger for each encoded `<`.
    let mut result = String::with_capacity(bytes.len());
    let mut index = 0;
    let mut start = 0;
    let len = bytes.len();

    while index < len {
        if bytes[index] == b'<' {
            let mut name_start = index + 1;

            // Optional `/`.
            if name_start < len && bytes[name_start] == b'/' {
                name_start += 1;
            }

            // Tag name.
            let mut name_end = name_start;

            while name_end < len
                && name_end - name_start < GFM_HTML_TAGFILTER_SIZE_MAX
                && bytes[name_end].is_ascii_alphabetic()
            {
                name_end += 1;
            }

            // Non-empty.
            if (name_end == len || (name_end != name_start &&
                // HTML whitespace, closing slash, or closing angle bracket.
                matches!(bytes[name_end], b'\t' | b'\n' | 12 /* `\f` */ | b'\r' | b' ' | b'/' | b'>'))) &&
                // Known name.
                GFM_HTML_TAGFILTER_NAMES.contains(&str::from_utf8(&bytes[name_start..name_end])
                .unwrap()
                .to_ascii_lowercase().as_str())
            {
                result.push_str(&value[start..index]);
                result.push_str("&lt;");
                start = index + 1;
            }

            // There was no `<` before `name_end`, so move to that next.
            index = name_end;
            continue;
        }

        index += 1;
    }

    result.push_str(&value[start..]);

    result
}
