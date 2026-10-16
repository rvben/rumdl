//! The UTF-8 byte order mark at the start of a document.
//!
//! A leading U+FEFF (bytes `EF BB BF`) is an encoding marker, not document text:
//! CommonMark implementations and editors skip it, so a heading that follows it is
//! still the first line's heading. Analysis therefore reads the text after the mark
//! and reports lines and columns relative to that text. Public fix ranges address
//! the supplied input, including its mark. Rewrites preserve that encoding marker.

/// The byte order mark as a character.
pub const BOM: char = '\u{feff}';

/// Length of the byte order mark in UTF-8.
pub const BOM_LEN: usize = BOM.len_utf8();

/// The text without one leading byte order mark. Exactly one is removed: a second
/// U+FEFF is ordinary content of the document.
///
/// This is applied once per analysis, at the point a document enters it. Text that
/// already went through it must not be passed again.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix(BOM).unwrap_or(text)
}

/// Whether the text starts with a byte order mark.
pub fn has_bom(text: &str) -> bool {
    text.starts_with(BOM)
}

/// Length in bytes of the byte order mark the text starts with, `0` if none.
pub fn bom_len(text: &str) -> usize {
    if has_bom(text) { BOM_LEN } else { 0 }
}

/// Width of a document's leading encoding marker in UTF-16 code units.
pub fn utf16_len(text: &str) -> u32 {
    u32::from(has_bom(text))
}

/// Translate internal analysis fixes to byte ranges in the supplied document.
/// Columns continue to describe the visible Markdown after the encoding mark.
pub(crate) fn restore_fix_offsets(warnings: &mut [crate::rule::LintWarning], shift: usize) {
    fn shift_fix(fix: &mut crate::rule::Fix, shift: usize) {
        fix.range.start += shift;
        fix.range.end += shift;
        for edit in &mut fix.additional_edits {
            shift_fix(edit, shift);
        }
    }
    if shift != 0 {
        for warning in warnings {
            if let Some(fix) = &mut warning.fix {
                shift_fix(fix, shift);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_bom_removes_exactly_one_mark() {
        assert_eq!(strip_bom("\u{feff}# Title"), "# Title");
        assert_eq!(strip_bom("\u{feff}\u{feff}# Title"), "\u{feff}# Title");
        assert_eq!(strip_bom("# Title"), "# Title");
        assert_eq!(strip_bom(""), "");
        assert_eq!(strip_bom("\u{feff}"), "");
    }

    #[test]
    fn a_mark_that_is_not_first_is_not_a_byte_order_mark() {
        assert_eq!(strip_bom("a\u{feff}"), "a\u{feff}");
        assert!(!has_bom("a\u{feff}"));
        assert_eq!(bom_len("a\u{feff}"), 0);
    }

    #[test]
    fn bom_len_is_the_utf8_length_of_the_mark() {
        assert_eq!(bom_len("\u{feff}x"), 3);
        assert_eq!(BOM_LEN, 3);
        assert_eq!(bom_len("x"), 0);
    }
}
