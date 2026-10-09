//! Facts about HTML elements shared by the rules that read inline HTML.

/// The void elements of the HTML standard, plus `param`, which browsers still
/// parse as one. Each is complete in its start tag: it holds no content, and no
/// later closing tag belongs to it.
///
/// Sorted, so a lowercase tag name can be binary searched.
pub const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

/// Whether the lowercase tag `name` is a void element.
pub fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.binary_search(&name).is_ok()
}

/// HTML's ASCII whitespace excludes vertical tab, unlike Rust's ASCII helper.
pub fn is_html_whitespace(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// Find the end of a start tag without treating a quoted `>` as its end.
/// `source` starts at the opening `<`; the returned offset includes `>`.
pub fn start_tag_end(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = 1;
    while i < bytes.len() && !is_html_whitespace(&bytes[i]) && !matches!(bytes[i], b'>' | b'/') {
        i += 1;
    }
    while i < bytes.len() {
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        match bytes.get(i)? {
            b'>' => return Some(i + 1),
            b'/' => {
                i += 1;
                continue;
            }
            _ => {}
        }
        i += usize::from(bytes[i] == b'=');
        while i < bytes.len() && !is_html_whitespace(&bytes[i]) && !matches!(bytes[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|b| matches!(b, b'\'' | b'"'));
        i += usize::from(quote.is_some());
        while i < bytes.len()
            && match quote {
                Some(quote) => bytes[i] != quote,
                None => !is_html_whitespace(&bytes[i]) && bytes[i] != b'>',
            }
        {
            i += 1;
        }
        i += usize::from(quote.is_some() && i < bytes.len());
    }
    None
}

/// Read an HTML attribute with the exact source range of its value.
/// Slashes belong to unquoted values, including a slash immediately before
/// `>`; they mark a self-closing tag only outside an attribute value.
pub fn extract_attribute_with_range(tag: &str, attr_name: &str) -> Option<(String, std::ops::Range<usize>)> {
    let bytes = tag.as_bytes();
    let mut i = usize::from(bytes.first() == Some(&b'<'));
    while i < bytes.len() && !is_html_whitespace(&bytes[i]) && !matches!(bytes[i], b'>' | b'/') {
        i += 1;
    }
    while i < bytes.len() {
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        if bytes.get(i).is_none_or(|b| *b == b'>') {
            break;
        }
        if bytes[i] == b'/' {
            if bytes.get(i + 1) == Some(&b'>') {
                break;
            }
            // HTML reconsumes after an unexpected solidus rather than
            // dropping the following attributes.
            i += 1;
            continue;
        }
        let name_start = i;
        // A leading '=' is an HTML parse error but belongs to the name.
        i += usize::from(bytes[i] == b'=');
        while i < bytes.len() && !is_html_whitespace(&bytes[i]) && !matches!(bytes[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        if name_start == i {
            break;
        }
        let matches = tag[name_start..i].eq_ignore_ascii_case(attr_name);
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            // The first duplicate wins, even when it has no explicit value.
            if matches {
                return Some((String::new(), i..i));
            }
            continue;
        }
        i += 1;
        while bytes.get(i).is_some_and(is_html_whitespace) {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|b| matches!(b, b'\'' | b'"'));
        i += usize::from(quote.is_some());
        let start = i;
        while i < bytes.len()
            && match quote {
                Some(quote) => bytes[i] != quote,
                None => !is_html_whitespace(&bytes[i]) && bytes[i] != b'>',
            }
        {
            i += 1;
        }
        if matches {
            return Some((tag[start..i].to_string(), start..i));
        }
        i += usize::from(quote.is_some() && i < bytes.len());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_tab_belongs_to_an_attribute_name() {
        let tag = "<a href\u{000B} href=\"https://еxample.com\">";
        let (value, range) = extract_attribute_with_range(tag, "href").unwrap();
        assert_eq!(value, "https://еxample.com");
        assert_eq!(&tag[range], value);
        assert!(!is_html_whitespace(&b'\x0b'));
        for byte in [b' ', b'\t', b'\r', b'\n', b'\x0c'] {
            assert!(is_html_whitespace(&byte));
        }
    }

    #[test]
    fn void_elements_are_sorted_for_binary_search() {
        assert!(VOID_ELEMENTS.is_sorted(), "{VOID_ELEMENTS:?}");
    }

    #[test]
    fn param_is_void_as_browsers_parse_it() {
        assert!(is_void_element("param"));
        assert!(is_void_element("br"));
        assert!(!is_void_element("span"));
    }

    #[test]
    fn attribute_range_selects_value_after_unicode_and_duplicate_text() {
        for tag in [
            "<a title=\"https://еxample.com\" href=\"https://еxample.com\">",
            "<a title='é' href=https://еxample.com>",
            "<a HREF = 'https://еxample.com' />",
        ] {
            let (value, range) = extract_attribute_with_range(tag, "href").unwrap();
            assert_eq!(value, "https://еxample.com");
            assert_eq!(&tag[range.clone()], value);
            assert_eq!(range.start, tag.rfind("https://").unwrap());
        }
    }

    #[test]
    fn unquoted_attribute_preserves_slashes_including_before_tag_end() {
        for tag in [
            "<a href=https://example.com/docs>",
            "<a href=https://example.com/>",
            "<a href=https://example.com/ >",
        ] {
            let (value, range) = extract_attribute_with_range(tag, "href").unwrap();
            assert_eq!(&tag[range], value);
            assert!(value.starts_with("https://"));
            if !tag.contains("docs") {
                assert!(value.ends_with('/'));
            }
        }
    }

    #[test]
    fn first_duplicate_attribute_wins_including_boolean_attribute() {
        assert_eq!(
            extract_attribute_with_range("<a href=safe href=unsafe>", "href")
                .unwrap()
                .0,
            "safe"
        );
        assert_eq!(
            extract_attribute_with_range("<a href href=unsafe>", "href").unwrap().0,
            ""
        );
    }

    #[test]
    fn only_html_ascii_whitespace_separates_attributes() {
        assert!(extract_attribute_with_range("<a title='é'\u{00a0}href='unsafe'>", "href").is_none());
    }
    #[test]
    fn malformed_but_rendered_attributes_do_not_hide_href() {
        for tag in [
            "<a / href=\"https://еxample.com\">",
            "<a title=\"x\"/href=\"https://еxample.com\">",
            "<a =x href=\"https://еxample.com\">",
            "<a title=\"x>y\" / href=\"https://еxample.com\">",
        ] {
            let (value, range) = extract_attribute_with_range(tag, "href").unwrap();
            assert_eq!(value, "https://еxample.com");
            assert_eq!(&tag[range], value);
        }
    }
}
