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

/// Parse HTML attributes into lowercase names and optional values.
pub fn parse_attributes(tag: &str) -> Vec<(String, Option<String>)> {
    let mut attrs = Vec::new();
    let tag_content = tag.trim_start_matches('<').trim_end_matches('>').trim_end_matches('/');
    let attr_start = tag_content
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(tag_content.len(), |(i, c)| i + c.len_utf8());

    if attr_start >= tag_content.len() {
        return attrs;
    }

    let attr_str = &tag_content[attr_start..];
    let mut chars = attr_str.chars().peekable();

    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        let mut attr_name = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
                break;
            }
            attr_name.push(c);
            chars.next();
        }
        if attr_name.is_empty() {
            break;
        }

        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        if chars.peek() == Some(&'=') {
            chars.next();
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }

            let mut value = String::new();
            if let Some(&quote) = chars.peek() {
                if quote == '"' || quote == '\'' {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == quote {
                            break;
                        }
                        value.push(c);
                    }
                } else {
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() || c == '>' {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
            attrs.push((attr_name.to_ascii_lowercase(), Some(value)));
        } else {
            attrs.push((attr_name.to_ascii_lowercase(), None));
        }
    }

    attrs
}

/// Extract an HTML attribute value from a tag string.
pub fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let attr_lower = attr_name.to_ascii_lowercase();
    parse_attributes(tag)
        .into_iter()
        .find(|(name, _)| name == &attr_lower)
        .and_then(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn preserves_slashes_in_unquoted_attribute_values() {
        let tag = "<a href=https://example.com/docs>";

        assert_eq!(
            extract_attribute(tag, "href"),
            Some("https://example.com/docs".to_string())
        );
        assert_eq!(
            parse_attributes(tag),
            vec![("href".to_string(), Some("https://example.com/docs".to_string()))]
        );
    }
}
