//! The native source and list preparation, before derived collections finish.
//!
//! This is a restricted view, not an incomplete `LintContext`. In particular,
//! table/list membership flags are settled by completion and are not inputs
//! of the native continuation algorithm.

use super::{CodeBlockDetail, InlineConfig, LineInfo, LintContext, ListBlock, MarkdownFlavor, OnceLock};

pub(crate) struct NativePreparedStructure<'a> {
    pub(super) content: &'a str,
    pub(super) flavor: MarkdownFlavor,
    pub(super) line_offsets: Vec<usize>,
    pub(super) code_blocks: Vec<(usize, usize)>,
    pub(super) code_block_details: Vec<CodeBlockDetail>,
    pub(super) lines: Vec<LineInfo>,
    pub(super) list_blocks: Vec<ListBlock>,
    pub(super) html_block_lines: Vec<std::ops::Range<usize>>,
    pub(super) inline_code_ranges: Vec<(usize, usize)>,
    pub(super) code_span_ranges: Vec<(usize, usize)>,
    pub(super) inline_config: InlineConfig,
    pub(super) front_matter_end: usize,
    pub(super) code_spans_without_math_cache: OnceLock<Vec<(usize, usize)>>,

    pub(super) parser_has_math: bool,
}

/// Inputs of the native continuation algorithm. Borrowing these facts keeps
/// their source revision and cannot initialize links, references or tables.
pub(crate) struct NativeListSource<'a, 's> {
    pub(crate) content: &'s str,
    pub(crate) flavor: MarkdownFlavor,
    pub(crate) lines: &'a [LineInfo],
    pub(crate) list_blocks: &'a [ListBlock],
    pub(crate) code_block_details: &'a [CodeBlockDetail],
    html_block_lines: &'a [std::ops::Range<usize>],
    offsets: &'a [usize],
    inline_code_ranges: &'a [(usize, usize)],
    code_span_ranges: &'a [(usize, usize)],
    inline_config: &'a InlineConfig,
    front_matter_end: usize,
    has_dollar: bool,
    without_math: &'a OnceLock<Vec<(usize, usize)>>,

    parser_has_math: bool,
}

impl NativePreparedStructure<'_> {
    #[cfg(test)]
    pub(crate) fn list_source(&self) -> NativeListSource<'_, '_> {
        NativeListSource {
            content: self.content,
            flavor: self.flavor,
            lines: &self.lines,
            list_blocks: &self.list_blocks,
            code_block_details: &self.code_block_details,
            html_block_lines: &self.html_block_lines,
            offsets: &self.line_offsets,
            inline_code_ranges: &self.inline_code_ranges,
            code_span_ranges: &self.code_span_ranges,
            inline_config: &self.inline_config,
            front_matter_end: self.front_matter_end,
            has_dollar: self.content.contains('$'),
            without_math: &self.code_spans_without_math_cache,

            parser_has_math: self.parser_has_math,
        }
    }
}

impl LintContext<'_> {
    pub(crate) fn native_list_source(&self) -> NativeListSource<'_, '_> {
        NativeListSource {
            content: self.content,
            flavor: self.flavor,
            lines: &self.lines,
            list_blocks: &self.list_blocks,
            code_block_details: &self.code_block_details,
            html_block_lines: &self.html_block_lines,
            offsets: &self.line_offsets,
            inline_code_ranges: &self.inline_code_ranges,
            code_span_ranges: &self.code_span_byte_ranges,
            inline_config: &self.inline_config,
            front_matter_end: self.front_matter_end,
            has_dollar: self.has_char('$'),
            without_math: &self.code_spans_without_math_cache,

            parser_has_math: self.parser_has_math,
        }
    }
}

pub(super) fn offset_to_line_col(content: &str, offsets: &[usize], offset: usize) -> (usize, usize) {
    match offsets.binary_search(&offset) {
        Ok(line) => (line + 1, 1),
        Err(line) => {
            let line_start = offsets.get(line.wrapping_sub(1)).copied().unwrap_or(0);
            let column = super::byte_to_char_count(&content[line_start..], offset.saturating_sub(line_start));
            (line, column)
        }
    }
}

impl NativeListSource<'_, '_> {
    pub(crate) fn line_holds_html_block(&self, line_idx: usize) -> bool {
        let run = self.html_block_lines.partition_point(|run| run.end <= line_idx);
        self.html_block_lines
            .get(run)
            .is_some_and(|run| run.contains(&line_idx))
    }

    pub(crate) fn line_info(&self, line: usize) -> Option<&LineInfo> {
        line.checked_sub(1).and_then(|index| self.lines.get(index))
    }

    pub(crate) fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        offset_to_line_col(self.content, self.offsets, offset)
    }

    pub(crate) fn front_matter_end_line(&self) -> usize {
        self.front_matter_end
    }

    pub(crate) fn is_rule_disabled(&self, name: &str, line: usize) -> bool {
        self.inline_config.is_rule_disabled(name, line)
    }

    pub(crate) fn is_byte_offset_in_code_span(&self, pos: usize) -> bool {
        LintContext::binary_search_ranges(self.inline_code_ranges, pos)
    }

    pub(crate) fn is_in_code_span_byte_with_or_without_math(&self, pos: usize) -> bool {
        let in_primary = LintContext::binary_search_ranges(self.code_span_ranges, pos);
        // This is the native alternate reading, shared by the full context and
        // the early list view. MDX can supply different primary code spans.

        if !self.parser_has_math && self.flavor != MarkdownFlavor::MDX {
            return in_primary;
        }
        let without_math = self.without_math.get_or_init(|| {
            if self.has_dollar {
                crate::utils::parser_options::code_span_ranges_without_math(self.content)
            } else {
                Vec::new()
            }
        });
        in_primary || LintContext::binary_search_ranges(without_math, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Rule;
    use crate::rules::{ContinuationStyle, MD077ListContinuationIndent};

    fn compare(source: &str, flavor: MarkdownFlavor) {
        let (prepared, finish) = LintContext::prepare_native_structure(source, flavor, None, None, None);
        let rules = [
            MD077ListContinuationIndent::new(ContinuationStyle::Any),
            MD077ListContinuationIndent::new(ContinuationStyle::Aligned),
        ];
        let early: Vec<_> = rules
            .iter()
            .map(|rule| rule.check_native_source(&prepared.list_source()))
            .collect();
        let full = finish(prepared);
        for (rule, early) in rules.iter().zip(early) {
            assert_eq!(early, rule.check(&full).unwrap(), "{flavor:?}: {source:?}");
            let fixed =
                crate::utils::fix_utils::filter_warnings_by_inline_config(early, full.inline_config(), rule.name());
            // The native fixer retains its ordered edit handling, including
            // paired fence edits and overlapping continuation ranges.
            let expected = rule.fix(&full).unwrap();
            let fixes: Vec<_> = fixed.into_iter().filter_map(|warning| warning.fix).collect();
            assert_eq!(
                MD077ListContinuationIndent::apply_native_fixes(source, fixes),
                expected,
                "{flavor:?}: {source:?}"
            );
        }
    }

    #[test]
    fn native_list_source_matches_completed_context() {
        let fragments = [
            "- item\n    continuation\n",
            "- item\n\nescaped prose\n",
            "- - nested\n    paragraph\n",
            "- [x] task\n      continuation\n",
            "- item\n  ```sh\n  echo ok\n  ```\n",
            "> - quoted\n>   continuation\n",
            "- <div>\n  html\n\n- next\n    continuation\n",
            "[^id]: footnote\n    body\n",
            "label\n: definition\n    continuation\n",
            "name | value\n--- | ---\n| a | b |\n",
            "`unclosed | other\n--- | ---\n--- | ---\n",
            "<!-- comment\n- literal\n-->\n",
            "!!! note\n    - item\n      continuation\n",
            "<div markdown=\"1\">\n- item\n    continuation\n</div>\n",
            "$math ` span$\n- item\n    `tail\n",
            "<!-- rumdl-disable MD077 -->\n- item\n    continuation\n",
            "---\ntitle: hi\n---\n- item\n    continuation\n",
            "# Heading\n- item\n    continuation\n",
            "- élève\n\tcontinuación\n",
        ];
        for flavor in crate::format_proof::ALL_FLAVORS {
            for left in fragments {
                for right in fragments {
                    compare(&format!("{left}\n{right}"), flavor);
                }
            }
        }
    }

    proptest::proptest! {
        #[test]
        fn generated_native_list_source_matches_completed_context(
            parts in proptest::collection::vec(
                proptest::sample::select(vec!["- a", "1. b", "    continuation", "", "---", "a | b", "`", "```", "[^x]: x", "> text", "# heading", "::: x", "!!! note", "[a]: /url"]),
                0..24,
            ),
            mkdocs in proptest::bool::ANY,
        ) {
            compare(&parts.join("\n"), if mkdocs { MarkdownFlavor::MkDocs } else { MarkdownFlavor::Standard });
        }
    }
}
