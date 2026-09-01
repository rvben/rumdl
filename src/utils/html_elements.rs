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
}
