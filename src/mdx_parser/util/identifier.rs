//! Info on JavaScript identifiers.

use unicode_id::UnicodeID;

/// Check if a character can start a JS identifier.
#[must_use]
pub fn id_start(char: char) -> bool {
    UnicodeID::is_id_start(char) || matches!(char, '$' | '_')
}

/// Check if a character can continue a JS (or JSX) identifier.
#[must_use]
pub fn id_cont(char: char, jsx: bool) -> bool {
    UnicodeID::is_id_continue(char) || matches!(char, '\u{200c}' | '\u{200d}') || (jsx && char == '-')
}
