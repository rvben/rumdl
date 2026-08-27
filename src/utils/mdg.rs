//! Line-level tokens the Markdown with Gherkin flavor inherits from Gherkin.
//!
//! Gherkin is line-oriented: its parser classifies Markdown content one line at
//! a time. MDG embeds three of its recognized shapes in Markdown, where each
//! one collides with a Markdown construct that would otherwise rewrite or
//! delete it:
//!
//! - a Data Table or Examples row, matched as `/^\s\s\s?\s?\s?\|/`, whose
//!   indentation overlaps the 4-column indented-code threshold;
//! - a tag line, spelled in MDG as backtick-wrapped `@tags` so Gherkin's bare
//!   `@tag` survives as Markdown, which binds to the structure on the very next
//!   line;
//! - a structure heading, `Keyword: name`, whose keyword is a dialect term that
//!   names a structure only when spelled exactly.
//!
//! Each rule decides for itself what to do about a token; this module only says
//! what Gherkin sees.

use regex::Regex;
use std::sync::LazyLock;

/// The JavaScript reference matcher scans globally rather than requiring the
/// whole line to consist of tags.
static TAG_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`(@[^`]+)`").unwrap());

/// The narrowest indentation Gherkin accepts for a Data Table or Examples
/// table, and therefore the canonical width such a table is normalized to.
pub const MIN_TABLE_INDENT: usize = 2;

/// The widest indentation Gherkin still reads as such a table.
pub const MAX_TABLE_INDENT: usize = 5;

/// Whether `line` is a row of a Gherkin Data Table or Examples table.
///
/// Gherkin matches such a row as 2-5 whitespace characters followed by a pipe,
/// so a tab counts like a space. Anything else in that position, a blockquote
/// marker or a list marker included, stays ordinary Markdown.
pub fn is_table_row(line: &str) -> bool {
    let indent = line.bytes().take_while(|&byte| byte == b' ' || byte == b'\t').count();
    (MIN_TABLE_INDENT..=MAX_TABLE_INDENT).contains(&indent) && line.as_bytes().get(indent) == Some(&b'|')
}

/// Whether Gherkin finds at least one backtick-wrapped tag on a line.
///
/// This deliberately mirrors the reference `/`(@[^`]+)`/g` scan: surrounding
/// prose and trailing comments do not disqualify a tag, and the tag body may
/// contain whitespace or `#` as long as it reaches a closing backtick.
pub fn is_tag_line(line: &str) -> bool {
    TAG_TOKEN.is_match(line)
}

/// Split a structure heading into its keyword, colon included, and the name
/// that follows, or `None` when the text names no structure.
///
/// The colon alone marks the structure: keywords are localized, so no list of
/// them can name them all. A backtick before the colon does disqualify it
/// though: dialect keywords are one or two plain words, so such a colon is
/// inside a code span rather than behind a keyword.
pub fn keyword_split(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    (!text[..colon].contains('`')).then(|| text.split_at(colon + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_row_accepts_gherkins_whole_indent_range() {
        for indent in ["  ", "   ", "    ", "     ", "\t\t", " \t", "\t   \t"] {
            assert!(
                is_table_row(&format!("{indent}| a | b |")),
                "{indent:?} indents a table"
            );
        }
    }

    #[test]
    fn table_row_rejects_an_indent_outside_the_range() {
        for indent in ["", " ", "      ", "\t\t\t\t\t\t"] {
            assert!(
                !is_table_row(&format!("{indent}| a | b |")),
                "{indent:?} is outside Gherkin's range"
            );
        }
    }

    #[test]
    fn table_row_needs_a_pipe_behind_its_indent() {
        for line in ["  > | a | b |", "  - | a | b |", "  text | a |", "   ", "  "] {
            assert!(!is_table_row(line), "{line:?} is not a table row");
        }
    }

    #[test]
    fn tag_line_matches_gherkin_reference_scan() {
        for line in [
            "`@browser`",
            "`@checkout` `@smoke`",
            "  `@a`\t`@b`  ",
            "`@a``@b`",
            "`@comment_tag1` #a comment",
            "prose `@a` after",
            "`@a b`",
            "`@comment_tag#2` #a comment",
        ] {
            assert!(is_tag_line(line), "{line:?} is a tag line");
        }
    }

    #[test]
    fn tag_line_requires_at_least_one_complete_wrapped_tag() {
        for line in ["", "   ", "plain prose", "@browser", "`browser`", "`@`", "`@a"] {
            assert!(!is_tag_line(line), "{line:?} is not a tag line");
        }
    }

    #[test]
    fn keyword_split_keeps_the_colon_with_the_keyword() {
        assert_eq!(keyword_split("Feature: Checkout"), Some(("Feature:", " Checkout")));
        assert_eq!(keyword_split("Scenario:name"), Some(("Scenario:", "name")));
        assert_eq!(keyword_split("Examples:"), Some(("Examples:", "")));
    }

    #[test]
    fn keyword_split_takes_the_first_colon() {
        // A later colon belongs to the name, which stays prose.
        assert_eq!(keyword_split("Scenario: a: b"), Some(("Scenario:", " a: b")));
    }

    #[test]
    fn keyword_split_declines_a_colon_behind_a_backtick() {
        assert_eq!(keyword_split("A `b: c` d"), None);
        assert_eq!(keyword_split("`a: b"), None);
    }

    #[test]
    fn keyword_split_declines_text_without_a_colon() {
        assert_eq!(keyword_split("Notes"), None);
        assert_eq!(keyword_split(""), None);
    }

    #[test]
    fn keyword_split_takes_a_keyword_colon_that_precedes_a_backtick() {
        assert_eq!(keyword_split("Scenario: a `b` c"), Some(("Scenario:", " a `b` c")));
    }
}
