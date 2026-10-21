//! Character columns for parser byte offsets, including out-of-order queries.

use super::LineInfo;
use crate::utils::range_utils::byte_to_char_count;
use std::collections::HashMap;

const CHECKPOINT_BYTES: usize = 256;

/// Character counts at byte multiples of CHECKPOINT_BYTES, rounded down to a
/// UTF-8 boundary. Grow only as far as requested, so an early link does not
/// require indexing the unused tail of a long line.
struct LineColumns(Vec<usize>);

impl LineColumns {
    fn new() -> Self {
        Self(vec![0])
    }

    fn column(&mut self, text: &str, byte: usize) -> usize {
        let byte = char_boundary(text, byte);
        let checkpoint = byte / CHECKPOINT_BYTES;
        while self.0.len() <= checkpoint {
            let next = self.0.len();
            let start = char_boundary(text, (next - 1) * CHECKPOINT_BYTES);
            let end = char_boundary(text, next * CHECKPOINT_BYTES);
            self.0.push(self.0[next - 1] + text[start..end].chars().count());
        }
        let start = char_boundary(text, checkpoint * CHECKPOINT_BYTES);
        self.0[checkpoint] + text[start..byte].chars().count()
    }
}

fn char_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

pub(super) fn line_index(lines: &[LineInfo], byte_offset: usize) -> usize {
    match lines.binary_search_by(|line| {
        if byte_offset < line.byte_offset {
            std::cmp::Ordering::Greater
        } else if byte_offset > line.byte_offset + line.byte_len {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

/// Index only lines queried beyond the first checkpoint. Ordinary short lines
/// need no allocation; repeated queries on long lines do not rescan a growing
/// prefix. Nesting and fallback parses can visit offsets in any order.
pub(crate) struct SourcePositions<'a> {
    content: &'a str,
    lines: &'a [LineInfo],
    columns: HashMap<usize, LineColumns>,
}

impl<'a> SourcePositions<'a> {
    pub(crate) fn new(content: &'a str, lines: &'a [LineInfo]) -> Self {
        Self {
            content,
            lines,
            columns: HashMap::new(),
        }
    }

    /// Return (0-based line index, 1-based line number, 0-based character column).
    pub(crate) fn position(&mut self, byte_offset: usize) -> (usize, usize, usize) {
        let idx = line_index(self.lines, byte_offset);
        (idx, idx + 1, self.column_on_line(idx, byte_offset))
    }

    /// Character column clamped to a known line. Multiline HTML tags retain
    /// end columns on their starting line, so their end offset can lie beyond it.
    pub(crate) fn column_on_line(&mut self, line_idx: usize, byte_offset: usize) -> usize {
        let line = &self.lines[line_idx];
        let text = line.content(self.content);
        let byte_col = byte_offset.saturating_sub(line.byte_offset).min(text.len());
        if byte_col < CHECKPOINT_BYTES {
            byte_to_char_count(text, byte_col) - 1
        } else {
            self.columns
                .entry(line_idx)
                .or_insert_with(LineColumns::new)
                .column(text, byte_col)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    #[test]
    fn line_index_matches_legacy_lookup_at_all_byte_boundaries() {
        for content in ["", "\n", "\r\n", "é日本🙂", "é日本🙂\n\n", "é日本🙂\r\n\r\nlast\r"] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            for offset in (0..=content.len() + 3).chain([usize::MAX]) {
                assert_eq!(
                    line_index(&ctx.lines, offset) + 1,
                    if ctx.lines.is_empty() {
                        1
                    } else {
                        LintContext::find_line_for_offset(&ctx.lines, content, offset).1
                    },
                    "{content:?}, offset={offset}",
                );
            }
        }
    }

    #[test]
    fn line_only_lookup_matches_full_position_at_all_byte_boundaries() {
        for content in ["", "\n", "\r\n", "é日本🙂", "é日本🙂\n\n", "é日本🙂\r\n\r\nlast\r"] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            for offset in (0..=content.len() + 3).chain([usize::MAX]) {
                assert_eq!(
                    ctx.line_number_at_offset(offset),
                    ctx.offset_to_line_col(offset).0,
                    "{content:?}, offset={offset}",
                );
            }
        }
    }

    fn verify_columns(text: &str) {
        let mut columns = LineColumns::new();
        // Reverse order exercises arbitrary access, including interior bytes of
        // multibyte characters and offsets beyond the end of the source line.
        for byte in (0..=text.len() + 3).rev() {
            assert_eq!(columns.column(text, byte), byte_to_char_count(text, byte) - 1);
        }
    }

    #[test]
    fn checkpoints_preserve_character_columns() {
        for prefix in 0..4 {
            for unit in ["a", "é", "日本", "🙂", "e\u{301}", "aé日🙂\t"] {
                verify_columns(&format!("{}{}", "x".repeat(prefix), unit.repeat(400)));
            }
        }
        verify_columns("");
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]
        #[test]
        fn indexed_columns_match_scalar_count(chars in proptest::collection::vec(proptest::char::any(), 0..700)) {
            verify_columns(&chars.into_iter().collect::<String>());
        }
    }

    #[test]
    fn source_positions_preserve_line_boundaries_and_clamping() {
        for newline in ["\n", "\r\n"] {
            let content = format!("{}{}{}{}end", "é🙂".repeat(150), newline, "a".repeat(700), newline);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let mut positions = SourcePositions::new(&content, &ctx.lines);
            for byte in (0..=content.len() + 3).rev() {
                // Independent linear oracle, including bytes inside CRLF and
                // beyond EOF. Newline bytes belong to the preceding line.
                let idx = ctx.lines.iter().rposition(|line| line.byte_offset <= byte).unwrap();
                let line = &ctx.lines[idx];
                let col = byte_to_char_count(line.content(&content), byte - line.byte_offset) - 1;
                assert_eq!(positions.position(byte), (idx, idx + 1, col), "at byte {byte}");
                assert_eq!(
                    positions.position(byte),
                    LintContext::find_line_for_offset(&ctx.lines, &content, byte),
                    "legacy at byte {byte}"
                );
                // Alternate between two distant lines, then grow the first
                // line's index again, rather than visiting each line once.
                let opposite = content.len().saturating_sub(byte);
                assert_eq!(
                    positions.position(opposite),
                    LintContext::find_line_for_offset(&ctx.lines, &content, opposite),
                    "legacy at opposite byte {opposite}"
                );
            }
        }
    }

    #[test]
    fn short_lines_do_not_allocate_column_indexes() {
        let ctx = LintContext::new("é link\nnext\n", MarkdownFlavor::Standard, None);
        let mut positions = SourcePositions::new(ctx.content, &ctx.lines);
        for byte in 0..=ctx.content.len() {
            positions.position(byte);
        }
        assert!(positions.columns.is_empty());
    }
    #[test]
    fn checkpoints_grow_only_to_requested_offsets() {
        let text = "é🙂".repeat(1000);
        let mut columns = LineColumns::new();
        for byte in 0..600 {
            assert_eq!(columns.column(&text, byte), byte_to_char_count(&text, byte) - 1);
        }
        assert_eq!(columns.0.len(), 3);
        assert_eq!(columns.column(&text, 12), 4);
        assert_eq!(columns.0.len(), 3);
    }

    #[test]
    fn parsed_link_and_image_columns_match_source_across_flavors() {
        for flavor in crate::format_proof::ALL_FLAVORS {
            for newline in ["\n", "\r\n"] {
                let prefix = "é日本🙂\t".repeat(80);
                let content = format!(
                    "{prefix}[![图](image.png)](target) [resolved][r] [unresolved][missing] ![alt][missing]\n\n\
                     {prefix}[multiline\n{prefix}label](target) ![multi\n{prefix}line](image.png)\n\n\
                     [r]: target\n"
                )
                .replace('\n', newline);
                let ctx = LintContext::new(&content, flavor, None);
                assert!(ctx.links().len() >= 4, "{flavor:?}");
                assert!(ctx.images().len() >= 3, "{flavor:?}");
                let spans = ctx
                    .links()
                    .iter()
                    .map(|link| {
                        (
                            link.byte_offset,
                            link.byte_end,
                            link.line,
                            link.start_col,
                            link.end_line,
                            link.end_col,
                        )
                    })
                    .chain(ctx.images().iter().map(|image| {
                        (
                            image.byte_offset,
                            image.byte_end,
                            image.line,
                            image.start_col,
                            image.end_line,
                            image.end_col,
                        )
                    }));
                for (start, end, line, col, end_line, end_col) in spans {
                    for (byte, expected_line, expected_col) in [(start, line, col), (end, end_line, end_col)] {
                        let preceding = &content[..byte];
                        let actual_line = preceding.bytes().filter(|&b| b == b'\n').count() + 1;
                        let actual_col = preceding
                            .rsplit('\n')
                            .next()
                            .unwrap()
                            .trim_end_matches('\r')
                            .chars()
                            .count();
                        assert_eq!(
                            (expected_line, expected_col),
                            (actual_line, actual_col),
                            "{flavor:?} at {byte}"
                        );
                    }
                }
            }
        }
    }
}
