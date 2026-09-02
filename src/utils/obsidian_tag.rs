//! The Obsidian tag token (`#tag`).
//!
//! Rules that meet a `#` have to tell a tag from an ATX heading marker or an
//! issue reference, and they have to agree on the answer, so the definition
//! lives here once.

use regex::Regex;
use std::sync::LazyLock;

/// A tag must contain at least one non-numerical character, wherever it sits:
/// `#1984` is not a tag, `#y1984` and `#3d_printing` are. A leading run of
/// digits is therefore allowed as long as a non-numerical tag character follows
/// it. That character may be a letter, a combining mark, an emoji or one of the
/// three punctuation characters Obsidian lists (`_`, `-`, `/`), but not
/// punctuation in general: `#37.` and `#42,` are issue references, not tags.
/// The token runs to the next whitespace.
pub const TAG_PATTERN_STR: &str = r"^#(?:[^\d\s#]|\d+[\p{L}\p{M}\p{So}_/-])[^\s#]*(?:\s|$)";

/// [`TAG_PATTERN_STR`] compiled. The pattern is anchored, so the text handed to
/// it starts at the `#`.
pub static TAG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(TAG_PATTERN_STR).expect("tag pattern is a valid regex"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_holds_a_non_numerical_character() {
        for text in [
            "#tagname",
            "#tagname followed by prose",
            "#project/active",
            "#my-tag_2023",
            "#3d_printing",
            "#y1984",
            "#tag中文",
        ] {
            assert!(TAG_PATTERN.is_match(text), "{text:?} is a tag");
        }
        // Digits alone, and digits followed by punctuation Obsidian does not
        // list, are issue references. A heading marker takes a space, and a
        // second `#` ends the token before it can qualify.
        for text in ["#1984", "#123", "#37.", "#42,", "# heading", "#", "##sub"] {
            assert!(!TAG_PATTERN.is_match(text), "{text:?} is not a tag");
        }
    }
}
