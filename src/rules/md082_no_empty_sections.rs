//! Rule MD082: Flag headings with no content before the next heading.
//!
//! A heading immediately followed by another heading, with no rendered body in
//! between, is an empty section. It usually signals a document that needs
//! restructuring: a parent heading with nothing under it before its first
//! child, or sibling headings with no body. This rule (opt-in) flags the
//! heading whose section is empty.
//!
//! Detection only: a fix would have to invent placeholder prose, so there is no
//! auto-fix. The `level` knob sets the minimum heading level that must have a
//! body. With the default `level = 1` every heading is checked, including
//! `# Title` straight into `## Section`. Set `level = 2` to exempt H1 while
//! still requiring content under H2 and deeper.
//!
//! What does not count as a section body: blank lines, HTML comments,
//! reference-link definitions (`[x]: url`), and lone thematic breaks (`---`).
//! Everything else that renders counts: paragraphs, lists, code blocks, tables,
//! blockquotes, and raw HTML. A `{#id}` attribute list on the line immediately
//! after a heading is its anchor (the parser folds it into the heading), so it
//! is treated as part of the heading rather than as the section body; an
//! attribute list anywhere else renders as ordinary text and counts as content.

use crate::lint_context::{HeadingStyle, LintContext};
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

fn default_level() -> u8 {
    1
}

/// Configuration for MD082 (No empty sections).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD082Config {
    /// Minimum heading level (1-6) that must be followed by content. A heading
    /// whose level is at least this value is flagged when it is immediately
    /// followed by another heading with no body in between. Default 1 checks
    /// every heading; set to 2 to exempt H1 (so `# Title` straight into
    /// `## Section` is allowed) while still requiring content under H2 and below.
    #[serde(default = "default_level")]
    pub level: u8,
}

impl Default for MD082Config {
    fn default() -> Self {
        Self { level: default_level() }
    }
}

impl RuleConfig for MD082Config {
    const RULE_NAME: &'static str = "MD082";
}

/// Position of a heading in the document, captured for adjacency analysis.
struct HeadingPos {
    /// 0-indexed line of the heading (the text line for a setext heading).
    index: usize,
    /// Heading level (1-6).
    level: u8,
    /// Whether the heading uses setext underlining (occupies two source lines).
    is_setext: bool,
    /// Whether the heading's anchor id came from a folded next-line `{#id}`
    /// attribute list rather than inline `{#id}` syntax in the heading text.
    /// Only a folded attribute list is part of the heading; an inline-id heading
    /// followed by a `{#id}` line leaves that line as ordinary content.
    id_from_next_line: bool,
    /// Heading text, for the diagnostic message.
    text: String,
}

#[derive(Debug, Clone, Default)]
pub struct MD082NoEmptySections {
    config: MD082Config,
}

impl MD082NoEmptySections {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_config_struct(config: MD082Config) -> Self {
        Self { config }
    }

    /// Whether the line at `idx` (0-indexed) is a real section body line.
    /// Blank lines, HTML comments, reference definitions, and lone thematic
    /// breaks do not count as content. A heading's folded `{#id}` anchor line is
    /// handled in `check` by advancing the scan past it, not here, because an
    /// attribute list that is NOT folded renders as ordinary text and is content.
    fn is_content_line(&self, ctx: &LintContext, idx: usize) -> bool {
        let Some(li) = ctx.lines.get(idx) else {
            return false;
        };
        if li.is_blank || li.in_html_comment {
            return false;
        }
        // A lone thematic break is not a section body, but a `---` inside a
        // blockquote or list is that container's content (it renders), so only a
        // top-level thematic break is excluded.
        if li.is_horizontal_rule && li.blockquote.is_none() && !li.in_list_block {
            return false;
        }
        // Reference definitions: probe the first non-whitespace byte so an
        // indented definition is still recognised by the byte-range lookup.
        if ctx.is_in_reference_def(li.byte_offset + li.indent) {
            return false;
        }
        true
    }

    fn warn_empty_section(&self, ctx: &LintContext, heading: &HeadingPos) -> LintWarning {
        let line_content = ctx.lines.get(heading.index).map_or("", |l| l.content(ctx.content));
        let end_column = line_content.chars().count() + 1;
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line: heading.index + 1,
            column: 1,
            end_line: heading.index + 1,
            end_column,
            message: format!("Heading '{}' has no content before the next heading", heading.text),
            fix: None,
        }
    }
}

impl Rule for MD082NoEmptySections {
    fn name(&self) -> &'static str {
        "MD082"
    }

    fn description(&self) -> &'static str {
        "Headings should have content before the next heading"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.has_valid_headings()
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let headings: Vec<HeadingPos> = ctx
            .valid_headings()
            .map(|h| HeadingPos {
                index: h.line_num - 1,
                level: h.heading.level,
                is_setext: matches!(h.heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2),
                // The id was folded from the next line when the heading has an id
                // but its own text carries no inline `{#id}`.
                id_from_next_line: h.heading.custom_id.is_some()
                    && crate::utils::header_id_utils::extract_header_id(&h.heading.raw_text)
                        .1
                        .is_none(),
                text: h.heading.text.clone(),
            })
            .collect();

        if headings.len() < 2 {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        for pair in headings.windows(2) {
            let cur = &pair[0];
            let next = &pair[1];

            if cur.level < self.config.level {
                continue;
            }

            // The section body begins after the heading construct. A setext
            // heading occupies two source lines (text + underline); an ATX
            // heading occupies one. Skipping the underline avoids counting it
            // as content.
            let content_start = if cur.is_setext { cur.index + 2 } else { cur.index + 1 };

            // rumdl folds a `{#id}` attribute list on the line immediately after
            // the heading into the heading's anchor (only when the heading has no
            // inline id). That folded line is part of the heading, not the body,
            // so skip it. Match by id so an unrelated attribute list - or one
            // after a heading that already had an inline id - still counts as
            // content, matching how the parser renders it.
            let mut scan_start = content_start;
            if cur.id_from_next_line
                && let Some(li) = ctx.lines.get(content_start)
                && crate::utils::header_id_utils::is_standalone_attr_list(li.content(ctx.content))
            {
                scan_start = content_start + 1;
            }

            let has_content = (scan_start..next.index).any(|idx| self.is_content_line(ctx, idx));
            if !has_content {
                warnings.push(self.warn_empty_section(ctx, cur));
            }
        }

        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detection only: inventing a section body would be guesswork, so
        // fixing is a no-op that returns the content unchanged.
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD082Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::rule::LintWarning;

    fn check(content: &str, config: MD082Config) -> Vec<LintWarning> {
        let rule = MD082NoEmptySections::from_config_struct(config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    fn check_default(content: &str) -> Vec<LintWarning> {
        check(content, MD082Config::default())
    }

    #[test]
    fn default_level_is_one() {
        assert_eq!(MD082Config::default().level, 1);
    }

    #[test]
    fn flags_atx_heading_immediately_followed_by_heading() {
        let w = check_default("# A\n## B\n\nBody text\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
        assert!(w[0].message.contains('A'), "got: {}", w[0].message);
    }

    #[test]
    fn accepts_heading_with_paragraph_body() {
        let w = check_default("# A\n\nSome text\n\n## B\n\nMore text\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn flags_nested_empty_section_from_issue() {
        // The issue's second example: H1 has a body, but the H2 runs straight
        // into an H3 with nothing between, so the H2 section is empty.
        let content =
            "# Level 1 heading\n\nLevel 1 content\n\n## Empty Section\n### Level 3 heading\n\nLevel 3 content\n";
        let w = check_default(content);
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 5);
        assert!(w[0].message.contains("Empty Section"));
    }

    #[test]
    fn default_level_flags_h1_into_h2() {
        let w = check_default("# Title\n## Section\n\nBody\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn level_2_exempts_h1_but_flags_h2() {
        let config = MD082Config { level: 2 };
        // H1 -> H2 with no body: exempt at level 2.
        assert!(check("# Title\n## Section\n\nBody\n", config.clone()).is_empty());
        // H2 -> H3 with no body: still flagged at level 2.
        let w = check("# Title\n\nIntro\n\n## A\n### B\n\nBody\n", config);
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 5);
    }

    #[test]
    fn flags_setext_heading_into_setext_heading() {
        // "Title" (setext H1) runs straight into "Section" (setext H2) with only
        // the underline between, so the H1 section is empty.
        let w = check_default("Title\n=====\nSection\n-------\ncontent\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn accepts_setext_heading_with_body() {
        let w = check_default("Title\n=====\n\nSome body\n\nSection\n-------\n\nMore\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn blank_lines_do_not_count_as_content() {
        let w = check_default("# A\n\n\n## B\n\ncontent\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn html_comment_does_not_count_as_content() {
        let w = check_default("# A\n\n<!-- a comment -->\n\n## B\n\ncontent\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn reference_definition_does_not_count_as_content() {
        let w = check_default("# A\n\n[ref]: https://example.com\n\n## B\n\ncontent\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn thematic_break_does_not_count_as_content() {
        // All three CommonMark thematic-break styles are treated as non-content.
        for marker in ["---", "***", "___"] {
            let input = format!("# A\n\n{marker}\n\n## B\n\ncontent\n");
            let w = check_default(&input);
            assert_eq!(w.len(), 1, "marker {marker:?}: got: {w:?}");
            assert_eq!(w[0].line, 1, "marker {marker:?}");
        }
    }

    #[test]
    fn code_block_counts_as_content() {
        let w = check_default("# A\n\n```\ncode\n```\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn list_counts_as_content() {
        let w = check_default("# A\n\n- item\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn raw_html_block_counts_as_content() {
        let w = check_default("# A\n\n<div>hello</div>\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn trailing_heading_at_eof_is_not_flagged() {
        // Only heading-into-heading is in scope; a final heading with no body is
        // not flagged.
        let w = check_default("# A\n\nbody\n\n## B\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn single_heading_is_not_flagged() {
        assert!(check_default("# Only heading\n\ncontent\n").is_empty());
    }

    #[test]
    fn document_without_headings_is_not_flagged() {
        assert!(check_default("Just some text\nand more text\n").is_empty());
    }

    #[test]
    fn invalid_heading_renders_as_content() {
        // `#nospace` (lowercase, no space after `#`) is not a CommonMark heading;
        // it renders as a paragraph, so it counts as a body and the section is
        // not empty. (Uppercase-first variants are classified as valid by the
        // heading detector's heuristic; use lowercase to ensure invalidity.)
        let w = check_default("# A\n\n#nospace is text\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn standalone_attr_list_does_not_count_as_content() {
        // `{#a}` on its own line is folded into the heading's id, not a body.
        let w = check_default("# A\n{#a}\n## B\n\ntext\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn setext_standalone_attr_list_does_not_count_as_content() {
        // For setext the attr list sits on the line after the underline.
        let w = check_default("Title\n=====\n{#a}\nSection\n-------\ntext\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 1);
    }

    #[test]
    fn non_folded_attr_list_counts_as_content() {
        // An attribute list separated from the heading by a blank line is NOT
        // folded into the heading; the parser renders it as an ordinary
        // paragraph, so it is real content and the section is not empty.
        let w = check_default("## A\n\n{#stray}\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn setext_non_folded_attr_list_counts_as_content() {
        // Setext heading with a blank line between the underline and the attr
        // list: not folded, so the attr list counts as content.
        let w = check_default("Title\n=====\n\n{#a}\nSection\n-------\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn inline_id_heading_with_following_attr_list_counts_as_content() {
        // The heading already has an inline id, so the next-line attr list is
        // NOT folded; it renders as a paragraph and counts as content.
        let w = check_default("## A {#x}\n{#y}\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn inline_id_then_matching_attr_list_counts_as_content() {
        // The heading has an INLINE id `{#x}`, so the parser does not fold the
        // following `{#x}` line; it renders as a paragraph and counts as content
        // even though its id matches the heading's. Must not flag.
        let w = check_default("## A {#x}\n{#x}\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn setext_inline_id_then_matching_attr_list_counts_as_content() {
        // Same as above for a setext heading with an inline id.
        let w = check_default("Title {#x}\n======\n{#x}\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }

    #[test]
    fn blockquoted_thematic_break_counts_as_content() {
        // `> ---` renders as a blockquote containing a thematic break: visible
        // content, not a bare top-level break. Must not flag.
        let w = check_default("# A\n\n> ---\n\n## B\n\ntext\n");
        assert!(w.is_empty(), "got: {w:?}");
    }
}
