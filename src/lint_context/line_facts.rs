//! Compact, source-ordered views shared by line-oriented rule families.
//!
//! Build all views in one traversal of the completed semantic line model. The
//! records borrow source information by index; they never reclassify Markdown
//! or discard malformed constructs from the lexical candidate index.

use super::{HeadingInfo, LineInfo};
use crate::filtered_lines::LineFilterConfig;

/// Context membership fits in one word, so visiting a line does not have to
/// load its large semantic record and repeat a chain of boolean tests.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineContextMask(u32);

macro_rules! contexts {
    ($(($bit:literal, $line:ident, $config:ident)),* $(,)?) => {
        impl LineContextMask {
            pub(crate) fn from_line(line: &LineInfo) -> Self {
                Self((line.in_kramdown_extension_block as u32) | $( (line.$line as u32) << $bit )|*)
            }

            pub(crate) fn for_config(config: &LineFilterConfig) -> Self {
                // A new filter must be represented here: no `..` lets it drift.
                let LineFilterConfig { $($config),* } = config;
                // Kramdown literal extension blocks are always excluded.
                Self(1 | $( (*$config as u32) << $bit )|*)
            }
        }

        #[cfg(test)]
        #[test]
        fn context_masks_match_individual_native_filters() {
            let ctx = crate::lint_context::LintContext::new(
                "plain\n", crate::config::MarkdownFlavor::Standard, None,
            );
            let setters: &[fn(&mut LineInfo)] = &[
                |line| line.in_kramdown_extension_block = true,
                $(|line| line.$line = true),*
            ];
            let configs = [
                LineFilterConfig::default(),
                $(LineFilterConfig { $config: true, ..LineFilterConfig::default() }),*
            ];
            for setter in setters {
                let mut line = ctx.lines[0].clone();
                setter(&mut line);
                for config in &configs {
                    let expected = line.in_kramdown_extension_block
                        $(|| (config.$config && line.$line))*;
                    assert_eq!(LineContextMask::for_config(config).excludes(
                        LineContextMask::from_line(&line)), expected);
                }
            }
        }
    };
}

contexts! {
    (1, in_front_matter, skip_front_matter),
    (2, in_code_block, skip_code_blocks),
    (3, in_html_block, skip_html_blocks),
    (4, in_html_comment, skip_html_comments),
    (5, in_mkdocstrings, skip_mkdocstrings),
    (6, in_esm_block, skip_esm_blocks),
    (7, in_math_block, skip_math_blocks),
    (8, in_pandoc_div, skip_quarto_divs),
    (9, in_jsx_expression, skip_jsx_expressions),
    (10, in_mdx_comment, skip_mdx_comments),
    (11, in_admonition, skip_admonitions),
    (12, in_content_tab, skip_content_tabs),
    (13, in_mkdocs_html_markdown, skip_mkdocs_html_markdown),
    (14, in_definition_list, skip_definition_lists),
    (15, in_obsidian_comment, skip_obsidian_comments),
    (16, in_pymdown_block, skip_pymdown_blocks),
    (17, is_div_marker, skip_div_markers),
    (18, in_jsx_block, skip_jsx_blocks),
}

impl LineContextMask {
    #[inline]
    pub(crate) fn excludes(self, context: Self) -> bool {
        self.0 & context.0 != 0
    }
}

pub(super) struct DocumentLines {
    pub(super) contexts: Vec<LineContextMask>,
    pub(super) headings: Vec<usize>,
    pub(super) parsed_headings: Vec<usize>,
    pub(super) list_items: Vec<usize>,
    pub(super) has_unordered_items: bool,
}

impl DocumentLines {
    pub(super) fn new(lines: &[LineInfo], blockquote_headings: &[Option<Box<HeadingInfo>>]) -> Self {
        let mut facts = Self {
            contexts: Vec::with_capacity(lines.len()),
            headings: Vec::new(),
            parsed_headings: Vec::new(),
            list_items: Vec::new(),
            has_unordered_items: false,
        };
        for (index, line) in lines.iter().enumerate() {
            facts.contexts.push(LineContextMask::from_line(line));
            if line.heading.is_some() {
                facts.headings.push(index);
                facts.parsed_headings.push(index);
            } else if blockquote_headings[index].is_some() {
                facts.parsed_headings.push(index);
            }
            if let Some(item) = &line.list_item {
                facts.list_items.push(index);
                facts.has_unordered_items |= !item.is_ordered;
            }
        }
        facts
    }
}

#[cfg(test)]
mod tests {
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;
    use proptest::prelude::*;

    fn assert_source_views(source: &str, flavor: MarkdownFlavor) {
        let ctx = LintContext::new(source, flavor, None);
        let expected_headings: Vec<_> = ctx
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.heading.as_ref().map(|h| (index + 1, h.text.as_str())))
            .collect();
        let actual_headings: Vec<_> = ctx
            .valid_headings()
            .map(|h| (h.line_num, h.heading.text.as_str()))
            .collect();
        assert_eq!(actual_headings, expected_headings);
        assert_eq!(ctx.has_valid_headings(), !expected_headings.is_empty());

        let expected_parsed: Vec<_> = ctx
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                line.heading
                    .as_deref()
                    .or(ctx.blockquote_headings[index].as_deref())
                    .map(|h| (index + 1, h.text.as_str()))
            })
            .collect();
        let actual_parsed: Vec<_> = ctx.headings().map(|h| (h.line_num, h.heading.text.as_str())).collect();
        assert_eq!(actual_parsed, expected_parsed);

        let expected_items: Vec<_> = ctx
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.list_item.as_ref().map(|_| index + 1))
            .collect();
        let actual_items: Vec<_> = ctx
            .list_items()
            .map(super::super::types::ParsedListItem::line_num)
            .collect();
        assert_eq!(actual_items, expected_items);
        assert_eq!(ctx.has_list_items(), !expected_items.is_empty());
        assert_eq!(
            ctx.has_unordered_list_items(),
            ctx.lines
                .iter()
                .any(|line| line.list_item.as_ref().is_some_and(|item| !item.is_ordered))
        );
    }

    proptest! {
        #[test]
        fn family_views_preserve_the_semantic_model(source in "(?s).{0,1024}") {
            assert_source_views(&source, MarkdownFlavor::Standard);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn family_views_preserve_multiline_dialects(
            parts in proptest::collection::vec(proptest::sample::select(vec![
                "# Heading", "#Malformed", "Title\n===", "> # Quoted", "- item",
                "  continuation", "1. - nested", "\tindented", "", "```", "`span",
                "tail`", "!!! note", "=== \"Tab\"", "{::comment}", "{:/comment}",
                "é🦀", "<!-- comment -->", "[id]: url", "$$math$$",
            ]), 0..12)
        ) {
            let source = parts.join("\n");
            for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX,
                MarkdownFlavor::MkDocs, MarkdownFlavor::Pandoc, MarkdownFlavor::Quarto,
                MarkdownFlavor::Obsidian, MarkdownFlavor::Kramdown, MarkdownFlavor::AzureDevOps,
                MarkdownFlavor::MyST, MarkdownFlavor::Hugo, MarkdownFlavor::MDG, MarkdownFlavor::GhAw] {
                for ending in ["\n", "\r\n", "\r"] {
                    let content = source.replace('\n', ending);
                    assert_source_views(&content, flavor);
                    let ctx = LintContext::new(&content, flavor, None);
                    assert_eq!(ctx.raw_lines().len(), ctx.lines.len());
                    for (raw, line) in ctx.raw_lines().iter().zip(&ctx.lines) {
                        assert_eq!(*raw, line.content(&content));
                    }
                }
            }
        }
    }

    #[test]
    fn family_views_preserve_containers_and_multiline_headings() {
        let source = "---\ntitle: Example\n---\n\n#Heading\n\n# Heading\n\nMultiline\nheading\n===\n\n> # Quote\n>\n> setext\n> ---\n\n- item\n  - nested\n- 1. inline\n\n```md\n# code\n- code\n```\n\n::: note\n## Inside\n:::\n\n{::comment}\n# Hidden\n{:/comment}\n";
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MDX,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
            MarkdownFlavor::Kramdown,
            MarkdownFlavor::AzureDevOps,
            MarkdownFlavor::MyST,
            MarkdownFlavor::Hugo,
            MarkdownFlavor::MDG,
            MarkdownFlavor::GhAw,
        ] {
            assert_source_views(source, flavor);
            assert_source_views(&source.replace('\n', "\r\n"), flavor);
        }
    }
}
