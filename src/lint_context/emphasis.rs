//! Source-backed lexical emphasis shared by the spacing and style rules.
//!
//! Pairing and block eligibility remain rule-specific. Delimiters and the rare
//! masked text live in document-level arenas rather than one allocation per line.

use std::borrow::Cow;
use std::ops::Range;

use super::LintContext;
use crate::filtered_lines::FilteredLinesExt;
use crate::utils::emphasis_utils::{EmphasisMarker, find_emphasis_markers_into, replace_inline_code_cow};

struct LineRecord {
    line_num: usize,
    source: Range<usize>,
    mdx_masked: Option<Range<usize>>,
    code_masked: Option<Range<usize>>,
    markers: Range<usize>,
}

#[derive(Default)]
pub(super) struct LexicalEmphasis {
    lines: Vec<LineRecord>,
    markers: Vec<EmphasisMarker>,
    masked: String,
}

#[derive(Clone, Copy)]
pub(crate) struct EmphasisLine<'a> {
    pub(crate) line_num: usize,
    source: &'a str,
    without_code: &'a str,
    pub(crate) markers: &'a [EmphasisMarker],
}

impl<'a> EmphasisLine<'a> {
    pub(crate) fn source(self) -> &'a str {
        self.source
    }
    pub(crate) fn without_code(self) -> &'a str {
        self.without_code
    }
}

impl LexicalEmphasis {
    pub(super) fn lines<'a>(&'a self, content: &'a str) -> impl Iterator<Item = EmphasisLine<'a>> {
        self.lines.iter().map(|line| {
            let source = line
                .mdx_masked
                .as_ref()
                .map_or_else(|| &content[line.source.clone()], |range| &self.masked[range.clone()]);
            EmphasisLine {
                line_num: line.line_num,
                source,
                without_code: line
                    .code_masked
                    .as_ref()
                    .map_or(source, |range| &self.masked[range.clone()]),
                markers: &self.markers[line.markers.clone()],
            }
        })
    }

    fn store_mask(&mut self, text: Cow<'_, str>) -> Option<Range<usize>> {
        match text {
            Cow::Borrowed(_) => None,
            Cow::Owned(text) => {
                let start = self.masked.len();
                self.masked.push_str(&text);
                Some(start..self.masked.len())
            }
        }
    }
}

pub(super) fn collect(ctx: &LintContext<'_>) -> LexicalEmphasis {
    let mut result = LexicalEmphasis::default();
    for line in ctx
        .filtered_lines()
        .skip_front_matter()
        .skip_code_blocks()
        .skip_math_blocks()
        .skip_html_blocks()
        .skip_esm_blocks()
        .skip_obsidian_comments()
        .skip_mkdocstrings()
        .candidates(super::LineCandidateKind::Emphasis)
        .filter(|line| !line.line_info.is_myst_comment)
    {
        let info = line.line_info;
        let source_range = info.byte_offset..info.byte_offset + info.byte_len;
        let source =
            crate::utils::skip_context::mask_mdx_inline_code(ctx, &ctx.content[source_range.clone()], info.byte_offset);
        let code = replace_inline_code_cow(&source);
        let markers_start = result.markers.len();
        find_emphasis_markers_into(&code, &mut result.markers);
        let code_masked = result.store_mask(code);
        let mdx_masked = result.store_mask(source);
        result.lines.push(LineRecord {
            line_num: line.line_num,
            source: source_range,
            mdx_masked,
            code_masked,
            markers: markers_start..result.markers.len(),
        });
    }
    result
}
