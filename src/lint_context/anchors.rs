//! Source anchor targets shared by local fragment checks and workspace indexing.

use super::{LineInfo, LintContext};
use crate::config::MarkdownFlavor;
use crate::utils::anchor_styles::AnchorStyle;
use crate::utils::header_id_utils::{HTML_BLOCK_OPEN_TAG, HTML_OPEN_TAG, html_tag_attribute, is_backslash_escaped};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{LazyLock, OnceLock};

static ATTR_ANCHOR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{\s*#([a-zA-Z0-9_][a-zA-Z0-9_-]*)[^}]*\}"#).unwrap());
static MD_SETTING_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--\s*md:setting\s+([^\s]+)\s*-->").unwrap());

/// Case-preserving source targets. Configuration is applied by each consumer.
#[derive(Default)]
pub(crate) struct DocumentAnchors {
    pub html: Vec<Range<usize>>,
    pub attributes: Vec<Range<usize>>,
    /// Material setting anchors are indexed, but are not local HTML targets.
    pub settings: Vec<Range<usize>>,
    heading_lines: Vec<usize>,
    styles: [OnceLock<Vec<HeadingAnchor>>; 4],
}

pub(crate) struct HeadingAnchor {
    pub line: usize,
    /// Empty slugs are omitted except under Python-Markdown's `_N` convention.
    pub primary: Option<String>,
    pub alias: Option<String>,
}

impl DocumentAnchors {
    pub(super) fn new(ctx: &LintContext<'_>) -> Self {
        let mut targets = Self::default();
        for (line_idx, info) in ctx.lines.iter().enumerate() {
            if info.in_front_matter || info.in_code_block {
                continue;
            }
            let content = info.content(ctx.content);
            for_each_html_anchor_target(ctx, info, |id| targets.html.push(id));
            let heading = ctx.heading_on_line(line_idx + 1);
            if heading.is_some() {
                targets.heading_lines.push(line_idx + 1);
                if ctx.flavor == MarkdownFlavor::MkDocs
                    && content.contains("<!--")
                    && let Some(caps) = MD_SETTING_PATTERN.captures(content)
                    && let Some(name) = caps.get(1)
                {
                    targets
                        .settings
                        .push(info.byte_offset + name.start()..info.byte_offset + name.end());
                }
            } else if !info.is_setext_heading_text && content.contains('{') && content.contains('#') {
                for caps in ATTR_ANCHOR_PATTERN.captures_iter(content) {
                    if let Some(id) = caps.get(1) {
                        targets
                            .attributes
                            .push(info.byte_offset + id.start()..info.byte_offset + id.end());
                    }
                }
            }
        }
        targets
    }

    pub(crate) fn headings(&self, ctx: &LintContext<'_>, style: AnchorStyle) -> &[HeadingAnchor] {
        let slot = match style {
            AnchorStyle::GitHub => 0,
            AnchorStyle::KramdownGfm => 1,
            AnchorStyle::Kramdown => 2,
            AnchorStyle::PythonMarkdown => 3,
        };
        self.styles[slot].get_or_init(|| {
            let python = style == AnchorStyle::PythonMarkdown;
            let mut counts: HashMap<String, usize> = HashMap::new();
            self.heading_lines
                .iter()
                .map(|&line| {
                    let heading = ctx.heading_on_line(line).expect("recorded heading").heading;
                    let fragment = style.generate_fragment(&heading.slug_text);
                    let (primary, alias) = if fragment.is_empty() {
                        if python {
                            let count = counts.entry(fragment).or_insert(0);
                            *count += 1;
                            (Some(format!("_{count}")), None)
                        } else {
                            (None, None)
                        }
                    } else if let Some(count) = counts.get_mut(&fragment) {
                        let suffix = *count;
                        *count += 1;
                        if python {
                            (
                                Some(format!("{fragment}_{suffix}")),
                                Some(format!("{fragment}-{suffix}")),
                            )
                        } else {
                            (Some(format!("{fragment}-{suffix}")), None)
                        }
                    } else {
                        counts.insert(fragment.clone(), 1);
                        (Some(fragment), None)
                    };
                    HeadingAnchor { line, primary, alias }
                })
                .collect()
        })
    }
}

/// The renderer's HTML targets, excluding literal markup in comments, code,
/// image descriptions and backslash escapes. HTML blocks do not apply escapes.
fn for_each_html_anchor_target(ctx: &LintContext<'_>, info: &LineInfo, mut record: impl FnMut(Range<usize>)) {
    let content = info.content(ctx.content);
    if !content.contains('<') {
        return;
    }
    let escapes_apply = !info.in_html_block;
    let open_tags: &Regex = if info.in_html_block {
        &HTML_BLOCK_OPEN_TAG
    } else {
        &HTML_OPEN_TAG
    };
    let mut pos = 0;
    while let Some(tag) = open_tags.captures_at(content, pos) {
        let whole = tag.get(0).unwrap();
        let byte_pos = info.byte_offset + whole.start();
        if ctx.is_in_code_span_byte(byte_pos)
            || ctx.is_in_html_comment(byte_pos)
            || ctx.image_containing(byte_pos).is_some()
            || (escapes_apply && is_backslash_escaped(content, whole.start()))
        {
            pos = whole.start() + 1;
            continue;
        }
        pos = whole.end();
        if let Some(id) = html_tag_attribute(whole.as_str(), "id") {
            // The attribute borrows an unchanged slice of the original source.
            let start = id.as_ptr() as usize - ctx.content.as_ptr() as usize;
            record(start..start + id.len());
        }
        if tag[1].eq_ignore_ascii_case("a")
            && let Some(name) = html_tag_attribute(whole.as_str(), "name")
        {
            let start = name.as_ptr() as usize - ctx.content.as_ptr() as usize;
            record(start..start + name.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Rule;
    use crate::rules::MD051LinkFragments;
    use crate::rules::md051_link_fragments::MD051Config;
    use crate::workspace_index::FileIndex;

    #[test]
    fn renderer_styles_remain_independent_on_one_context() {
        let content = "# Repeat\n\n# Repeat\n\n# 日本語\n\n# 日本語\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        // Alternate styles to exercise an already populated document cache.
        for style in [
            AnchorStyle::PythonMarkdown,
            AnchorStyle::GitHub,
            AnchorStyle::Kramdown,
            AnchorStyle::KramdownGfm,
            AnchorStyle::PythonMarkdown,
            AnchorStyle::GitHub,
        ] {
            let rule = MD051LinkFragments::with_anchor_style(style);
            let mut index = FileIndex::new();
            rule.contribute_to_index(&ctx, &mut index);
            assert!(index.has_anchor_with_case("repeat", false));
            assert!(index.has_anchor_with_case("repeat-1", false));
            if style == AnchorStyle::PythonMarkdown {
                assert!(index.has_anchor_with_case("repeat_1", false));
                assert!(index.has_anchor_with_case("_1", false));
                assert!(index.has_anchor_with_case("_2", false));
            } else {
                assert!(!index.has_anchor_with_case("repeat_1", false));
                assert!(!index.has_anchor_with_case("_1", false));
            }
        }
    }

    #[test]
    fn case_sensitive_checks_and_indexing_share_case_preserving_targets() {
        let content = "# Title {#CustomCase}\n\n<a id=\"HtmlCase\" name=\"NamedCase\"></a>\n\nText {#AttrCase}\n\n\
            [custom](#CustomCase) [custom lower](#customcase)\n\
            [html](#HtmlCase) [html lower](#htmlcase)\n\
            [name](#NamedCase) [name lower](#namedcase)\n\
            [attr](#AttrCase) [attr lower](#attrcase)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);
        for ignore_case in [true, false, true, false] {
            let rule = MD051LinkFragments::from_config_struct(MD051Config {
                ignore_case,
                ..Default::default()
            });
            assert_eq!(rule.check(&ctx).unwrap().len(), if ignore_case { 0 } else { 4 });
            let mut index = FileIndex::new();
            rule.contribute_to_index(&ctx, &mut index);
            for target in ["CustomCase", "HtmlCase", "NamedCase", "AttrCase"] {
                assert!(index.has_anchor_with_case(target, ignore_case));
                assert_eq!(
                    index.has_anchor_with_case(&target.to_lowercase(), ignore_case),
                    ignore_case
                );
            }
            assert_eq!(rule.check(&ctx).unwrap().len(), if ignore_case { 0 } else { 4 });
        }
    }

    #[test]
    fn non_heading_attributes_and_material_settings_keep_their_scope() {
        let content = "Paragraph {#Paragraph}\n\ncontinued {#NotAnAttribute}\nheading\n---\n\n\
            ## Setting <!-- md:setting option.path -->\n\n\
            [setting](#option.path)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let rule = MD051LinkFragments::new();
        let mut index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut index);
        assert!(index.has_anchor_with_case("Paragraph", false));
        assert!(!index.has_anchor_with_case("NotAnAttribute", false));
        assert!(index.has_anchor_with_case("option.path", false));
        // Material setting comments are indexed for other documents, while
        // local checks retain the existing runtime-anchor exemption policy.
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    }
}
