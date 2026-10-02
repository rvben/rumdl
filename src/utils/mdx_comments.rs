//! Classification of already parsed MDX expressions.

/// Whether a complete braced expression contains comments and whitespace only.
/// A comment followed by expression code can render visible content.
pub(crate) fn is_comment_only_expression(source: &str) -> bool {
    let Some(mut rest) = source.strip_prefix('{').and_then(|inner| inner.strip_suffix('}')) else {
        return false;
    };
    let mut has_comment = false;
    loop {
        rest = rest.trim_start_matches(|ch: char| ch.is_whitespace() || ch == '\u{feff}');
        if rest.is_empty() {
            return has_comment;
        }
        if let Some(comment) = rest.strip_prefix("/*") {
            let Some(end) = comment.find("*/") else {
                return false;
            };
            rest = &comment[end + 2..];
        } else if rest.starts_with("//") {
            let end = rest.find(['\n', '\r', '\u{2028}', '\u{2029}']).unwrap_or(rest.len());
            rest = &rest[end..];
        } else {
            return false;
        }
        has_comment = true;
    }
}
