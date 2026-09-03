//! Rule MD090: Flag a thematic break that sits directly above a heading.
//!
//! A heading already marks a section boundary, so a horizontal rule right
//! above it (`---`, `***`, `___`) draws the same line twice. Generated
//! Markdown produces the pattern constantly. This rule (opt-in) reports each
//! such break, and its fix deletes the break together with the blank lines
//! between it and the heading, keeping the blank line above the break so the
//! heading stays separated from the paragraph before it. A break with no
//! blank line above it is replaced by one blank line for the same reason:
//! text butted against a setext heading would merge into the heading.
//!
//! Only blank lines may sit between the break and the heading, and a blank
//! line holds nothing but spaces and tabs; other whitespace, such as a
//! no-break space, renders as content. A comment, a
//! reference definition or any other line in between means the break is not
//! directly above the heading and nothing is reported. Both the break and the
//! heading must be top-level: inside a blockquote or a list item a thematic
//! break is that container's content. Containers whose body is ordinary
//! Markdown (a fenced div, a MyST directive, a `markdown="1"` element) are
//! transparent: a break there is a real break, MD082 draws the same line,
//! and the fix has no container prefix to disturb. Inside such a container
//! only an ATX heading is a target: a container's opening marker followed by
//! a dash run is recorded as a setext heading though it marks no section, and
//! since deleting a break is destructive the rule declines every setext
//! heading in a container rather than trying to tell the two apart. A
//! container whose body is indented rather than fenced (a MkDocs admonition, a
//! content tab) reports nothing at all, because the shared line data does not
//! read an indented dash run as a break; that is rumdl-wide and MD035 is
//! silent there too. A markdown-bodied directive written with backticks
//! rather than colons is silent for the same shared reason: its body starts
//! as a code fence, so the break flag is settled to false before the fence is
//! reinterpreted, and MD082 misses the same break.
//!
//! Off by default because slide formats (Marp, Slidev, reveal.js, Pandoc) use
//! a thematic break as the slide separator, nearly always followed by the
//! slide's heading.

use crate::lint_context::{HeadingStyle, LineInfo, LintContext, is_setext_underline_content};
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

#[derive(Debug, Clone, Default)]
pub struct MD090NoHrBeforeHeading;

impl MD090NoHrBeforeHeading {
    pub fn new() -> Self {
        Self
    }

    /// A line outside blockquotes and list items, the two containers whose
    /// content carries a prefix the fix cannot safely edit. Containers whose
    /// body is ordinary Markdown, such as fenced divs and MyST directives,
    /// deliberately pass: MD082 applies the same test to the same lines.
    fn is_top_level(line: &LineInfo) -> bool {
        line.blockquote.is_none() && !line.in_list_block
    }

    /// Whether the detector recorded a setext heading on this line's text.
    fn is_setext_record(line: &LineInfo) -> bool {
        line.heading
            .as_deref()
            .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
    }

    /// Whether a setext record on this line is a phantom rather than a
    /// section boundary.
    ///
    /// A container's opening marker (`::: note`, `!!! note`, `/// note`) is
    /// structure, and the detector reads it as setext text whenever a dash
    /// run follows it. Distinguishing an opener from body text needs the exact
    /// opening syntax of every container in every flavor, and each one missed
    /// costs a deleted line, so this asks the wider question the shared line
    /// data already answers: is the line in a container at all? Inside one,
    /// only an ATX heading is treated as a section boundary. The cost is one
    /// unreported break above a setext heading written inside a container; the
    /// alternative cost is deleting a break that renders.
    fn is_phantom_container_heading(line: &LineInfo) -> bool {
        Self::is_setext_record(line) && line.in_flavor_container()
    }

    /// Whether line `idx` is a thematic break that renders as one.
    ///
    /// `is_horizontal_rule` is computed from the line text alone, so it is also
    /// set on the `---` underline of a setext heading. Where the detector
    /// recorded that heading, its text line carries the record and settles the
    /// question. The detector declines to record a setext heading whose text
    /// line opens like another construct (`*Label*`, `<span>`), so a dash run
    /// it left unrecorded still counts as an underline whenever the line above
    /// it is top-level paragraph text: deleting it would demote a real heading
    /// to a paragraph, while the cost of reading a break as an underline is
    /// one unreported break. An ATX record without the space after its `#`s
    /// (`#hashtag`) is structurally paragraph text, as its `is_valid` says,
    /// so it stays eligible to hold an underline; and a setext record on a line
    /// inside a flavor container settles nothing, because the record may be the
    /// container's own marker rather than setext text, so the dash run below it
    /// is judged like any other line. In a flavor that gives the marker no
    /// meaning the line is in no container, is ordinary paragraph text, and its
    /// record is real. Only a plain dash run is ambiguous; `***`, `___`
    /// and spaced forms like `- - -` can never underline. A table row reads as
    /// paragraph context but cannot carry an underline: a dash run below one
    /// ends the table and is a break, so `in_table_block` takes the row out of
    /// the ambiguous set. The flag is populated from the parser's table blocks,
    /// so a lone pipe line in a flavor without tables stays ordinary paragraph
    /// text and keeps its underline reading. Two false negatives
    /// are accepted, both errors of silence: a paragraph line lazily
    /// continuing a blockquote reads here as paragraph text though its
    /// underline reading is forbidden, and the `=` underline of a recorded
    /// heading carries a second heading record the detector re-reads it
    /// into, so a dash run under it looks like that phantom's underline.
    /// Untangling either needs the detector's own analysis, and being wrong
    /// the other way deletes a heading.
    fn is_top_level_break(ctx: &LintContext, lines: &[LineInfo], idx: usize) -> bool {
        let line = &lines[idx];
        if !line.is_horizontal_rule || !Self::is_top_level(line) {
            return false;
        }
        if idx == 0 {
            return true;
        }
        let above = &lines[idx - 1];
        if Self::is_setext_record(above) && !Self::is_phantom_container_heading(above) {
            return false;
        }
        let above_content = above.content(ctx.content);
        let may_underline = is_setext_underline_content(line.content(ctx.content))
            && !Self::is_blank_line(above_content)
            && Self::is_top_level(above)
            && !above.in_table_block
            && (above.is_paragraph_context() || above.heading.as_deref().is_some_and(|h| !h.is_valid));
        !may_underline
    }

    /// Whether a line is blank the way CommonMark defines it: empty, or
    /// spaces and tabs only. Other Unicode whitespace, such as a no-break
    /// space, renders as content, so a line holding one keeps the break
    /// from sitting directly above the heading.
    fn is_blank_line(text: &str) -> bool {
        text.chars().all(|c| c == ' ' || c == '\t')
    }
}

impl Rule for MD090NoHrBeforeHeading {
    fn name(&self) -> &'static str {
        "MD090"
    }

    fn description(&self) -> &'static str {
        "Horizontal rules should not precede headings"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.has_valid_headings() || !ctx.lines.iter().any(|line| line.is_horizontal_rule)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let lines = &ctx.lines;
        let mut warnings: Vec<LintWarning> = Vec::new();

        for heading in ctx.valid_headings() {
            let heading_idx = heading.line_num - 1;
            // A container's opening line carrying a phantom setext record
            // marks no section, so a break above it is a real break to keep.
            if !Self::is_top_level(heading.line_info) || Self::is_phantom_container_heading(heading.line_info) {
                continue;
            }

            // Walk up from the heading over blank lines. Each break found is
            // deleted up to the line the previous deletion started on: the
            // heading for the nearest break, then the break below it for a run
            // of breaks, so the ranges abut without overlapping.
            let mut delete_end = heading_idx;
            let mut idx = heading_idx;
            while idx > 0 {
                idx -= 1;
                // Only a line that is blank in the source is skipped.
                // `LineInfo::is_blank` is container-aware and reports a bare
                // `>` as blank, but that line is an empty blockquote the fix
                // must not delete.
                if Self::is_blank_line(lines[idx].content(ctx.content)) {
                    continue;
                }
                if !Self::is_top_level_break(ctx, lines, idx) {
                    // The walk stopped on content. When that content sits on
                    // the line directly above the topmost deleted break,
                    // deleting the break would butt it against what follows,
                    // merging it into a setext heading's text, so that
                    // deletion leaves one blank line behind.
                    if delete_end == idx + 1
                        && delete_end != heading_idx
                        && let Some(warning) = warnings.last_mut()
                        && let Some(fix) = warning.fix.as_mut()
                    {
                        fix.replacement = "\n".to_string();
                    }
                    break;
                }
                let break_line = &lines[idx];
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    severity: Severity::Warning,
                    line: idx + 1,
                    column: 1,
                    end_line: idx + 1,
                    end_column: break_line.content(ctx.content).chars().count() + 1,
                    message: format!("Horizontal rule before heading '{}' is redundant", heading.heading.text),
                    fix: Some(Fix::new(
                        break_line.byte_offset..lines[delete_end].byte_offset,
                        String::new(),
                    )),
                });
                delete_end = idx;
            }
        }

        // A run of breaks is discovered bottom-up; report in document order.
        warnings.sort_by_key(|w| w.line);
        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn check_in(content: &str, flavor: MarkdownFlavor) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, flavor, None);
        MD090NoHrBeforeHeading::new().check(&ctx).unwrap()
    }

    fn check(content: &str) -> Vec<LintWarning> {
        check_in(content, MarkdownFlavor::Standard)
    }

    fn fix(content: &str) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD090NoHrBeforeHeading::new().fix(&ctx).unwrap()
    }

    /// The 1-based lines the rule reported, in output order.
    fn lines(content: &str) -> Vec<usize> {
        check(content).iter().map(|w| w.line).collect()
    }

    // Detection

    #[test]
    fn flags_break_between_paragraph_and_heading() {
        let content = "# Title\n\n## Topic\n\nProse.\n\n---\n\n## Next Topic\n\nMore.\n";
        let w = check(content);
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 7);
        assert_eq!(w[0].column, 1);
        assert_eq!(w[0].end_line, 7);
        assert_eq!(w[0].end_column, 4, "extent covers the three marker characters");
        assert_eq!(w[0].message, "Horizontal rule before heading 'Next Topic' is redundant");
    }

    #[test]
    fn warning_carries_deletion_of_break_and_blank_lines_below_it() {
        let content = "Prose.\n\n---\n\n## Next\n";
        let w = check(content);
        let fix = w[0].fix.as_ref().expect("fix is populated");
        assert_eq!(&content[fix.range.clone()], "---\n\n");
        assert_eq!(fix.replacement, "");
    }

    #[test]
    fn setext_underline_is_not_a_break() {
        // `---` directly under text is the underline of a level-2 setext
        // heading, so there is no thematic break in this document at all.
        assert!(lines("Prose\n---\n\n## Next\n").is_empty());
    }

    #[test]
    fn emphasis_setext_underline_is_not_a_break() {
        // The heading detector skips a setext text line opening with `*`, so
        // this heading has no record; the rule still must not delete its
        // underline. CommonMark reads `*Label*` + `---` as a level-2 heading.
        let content = "*Label*\n---\n\n## Next\n";
        assert!(lines(content).is_empty());
        assert_eq!(fix(content), content);
    }

    #[test]
    fn inline_html_setext_underline_is_not_a_break() {
        // Same detector gap for a text line opening with `<`: inline HTML is
        // paragraph text, so the `---` under it underlines a setext heading.
        let content = "<span>Label</span>\n---\n\n## Next\n";
        assert!(lines(content).is_empty());
        assert_eq!(fix(content), content);
    }

    #[test]
    fn star_run_under_paragraph_text_is_still_a_break() {
        // Only a dash run can underline; `***` under paragraph text is a
        // thematic break interrupting the paragraph, and the tight fix
        // leaves a blank line so `*Label*` stays separated from the heading.
        let content = "*Label*\n***\n\n## Next\n";
        assert_eq!(lines(content), [2]);
        assert_eq!(fix(content), "*Label*\n\n## Next\n");
    }

    #[test]
    fn atx_heading_above_dash_run_keeps_it_a_break() {
        // An ATX heading is not paragraph text, so the `---` under it is a
        // thematic break, not an underline.
        assert_eq!(lines("## A\n---\n\n## B\n"), [2]);
    }

    #[test]
    fn list_item_above_dash_run_keeps_it_a_break() {
        // A `---` cannot lazily underline a paragraph inside a list item, so
        // it closes the list as a top-level thematic break.
        assert_eq!(lines("- item\n---\n\n## H\n"), [2]);
    }

    #[test]
    fn table_row_above_dash_run_keeps_it_a_break() {
        // A dash run below a table row ends the table and is a thematic
        // break: a table row cannot carry a setext underline.
        assert_eq!(lines("| a |\n| - |\n| x |\n---\n\n## H\n"), [4]);
    }

    #[test]
    fn pipe_paragraph_that_is_no_table_keeps_its_underline() {
        // Without a delimiter row the pipes are ordinary text, so the line is
        // paragraph content and the dash run under it is its setext underline.
        // The text opens with `*`, so the detector records no setext heading
        // and the rule's own underline reading is what answers: a guard keyed
        // on the pipe rather than on the table block would delete the
        // underline here.
        assert!(check("*Label | x*\n---\n\n## H\n").is_empty());
    }

    #[test]
    fn closing_fence_above_dash_run_keeps_it_a_break() {
        // A closed code block holds no paragraph open, so the `---` under
        // its closing fence is a thematic break.
        assert_eq!(lines("Text\n\n```\ncode\n```\n---\n\n## H\n"), [6]);
    }

    #[test]
    fn equals_underline_above_dash_run_is_the_accepted_false_negative() {
        // The `===` underlines `Title`, so the dash run under it renders as
        // a thematic break. The detector also re-reads that `===` as the
        // text of a second heading underlined by the dash run, and the rule
        // honors the record rather than re-deriving the chain: the break
        // goes unreported, and nothing is deleted.
        assert!(lines("Title\n===\n---\n\n## H\n").is_empty());
    }

    #[test]
    fn equals_paragraph_at_document_start_is_underlined_not_broken() {
        // With nothing above it, `===` is a paragraph of its own, and the
        // dash run underlines it into a level-2 heading.
        assert!(lines("===\n---\n\n## H\n").is_empty());
    }

    #[test]
    fn lazy_blockquote_continuation_above_dash_run_is_the_accepted_false_negative() {
        // `Foo` lazily continues the blockquote's paragraph, and a setext
        // underline cannot be lazy, so the `---` renders as a real break.
        // Telling that apart from an underline needs the detector's
        // continuation analysis; the rule reads `Foo` as plain paragraph
        // text and deliberately declines the break rather than risk deleting
        // an underline. One unreported break is the accepted cost.
        assert!(lines("> q\nFoo\n---\n\n## H\n").is_empty());
    }

    #[test]
    fn invalid_atx_above_dash_run_is_a_setext_underline() {
        // `#hashtag` has no space after its `#`, so it renders as paragraph
        // text - its heading record says `is_valid == false` - and the dash
        // run under it underlines a level-2 setext heading.
        let content = "#hashtag\n---\n\n## H\n";
        assert!(lines(content).is_empty());
        assert_eq!(fix(content), content);
    }

    #[test]
    fn star_run_under_invalid_atx_is_still_a_break() {
        // `***` cannot underline, so it is a thematic break interrupting the
        // `#hashtag` paragraph.
        assert_eq!(lines("#hashtag\n***\n\n## H\n"), [2]);
    }

    #[test]
    fn div_marker_above_dash_run_keeps_it_a_break() {
        // The detector records `::: note` as setext text underlined by the
        // dash run, but a div marker is never setext text: the record is a
        // phantom, and the `---` opens the div's body as a real break.
        let content = "::: note\n---\n\n# H\n:::\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let rule = MD090NoHrBeforeHeading::new();
        let w = rule.check(&ctx).unwrap();
        assert_eq!(w.iter().map(|w| w.line).collect::<Vec<_>>(), [2]);
        assert_eq!(rule.fix(&ctx).unwrap(), "::: note\n\n# H\n:::\n");
    }

    #[test]
    fn break_above_a_container_opener_is_kept() {
        // Same phantom record, read as a target this time: an opening line
        // carries a container marker, so it marks no section a break above it
        // could duplicate. Each flavor's own detection populates the flag, so
        // every marker is read in the flavor that gives it meaning, and a
        // marker nested inside another container of the same kind carries it
        // exactly as an outermost one does.
        let cases: &[(&str, MarkdownFlavor, &str)] = &[
            ("pandoc div", MarkdownFlavor::Quarto, "***\n::: note\n---\n\n# H\n:::\n"),
            (
                "myst directive",
                MarkdownFlavor::MyST,
                "***\n:::{note}\n---\n\n# H\n:::\n",
            ),
            (
                "mkdocs content tab",
                MarkdownFlavor::MkDocs,
                "***\n=== \"Tab\"\n---\n\n# H\n",
            ),
            (
                "mkdocs admonition",
                MarkdownFlavor::MkDocs,
                "***\n!!! note\n---\n\n# H\n",
            ),
            (
                "mkdocstrings",
                MarkdownFlavor::MkDocs,
                "***\n::: mod.path\n---\n\n# H\n",
            ),
            (
                "pymdown block",
                MarkdownFlavor::MkDocs,
                "***\n/// note\n---\n\n# H\n///\n",
            ),
            (
                "div nested in a div",
                MarkdownFlavor::Quarto,
                ":::: outer\n\n***\n::: inner\n---\n\n# H\n:::\n::::\n",
            ),
        ];
        for (name, flavor, content) in cases {
            let ctx = LintContext::new(content, *flavor, None);
            let rule = MD090NoHrBeforeHeading::new();
            let break_line = content.lines().position(|l| l == "***").unwrap() + 1;
            let w = rule.check(&ctx).unwrap();
            assert!(
                !w.iter().any(|w| w.line == break_line),
                "{name}: reported the break above the opener: {w:?}"
            );
            assert!(
                rule.fix(&ctx).unwrap().contains("***\n"),
                "{name}: the fix deleted the break above the opener"
            );
        }

        // The quarto case in full: the break inside the div is still removed,
        // so a guard that simply gave up on containers would fail here.
        let content = "***\n::: note\n---\n\n# H\n:::\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let rule = MD090NoHrBeforeHeading::new();
        assert_eq!(
            rule.check(&ctx).unwrap().iter().map(|w| w.line).collect::<Vec<_>>(),
            [3]
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "***\n::: note\n\n# H\n:::\n");
    }

    #[test]
    fn setext_heading_inside_a_container_is_not_a_target_but_atx_is() {
        // The accepted cost of not telling an opener from body text: a setext
        // heading inside a container is left alone, because from the line data
        // it is indistinguishable from a container marker with a dash run
        // under it. An ATX heading in the same position carries no such
        // ambiguity and is still reported, so the exemption stays narrow.
        for content in [
            "::: note\n\nProse\n\n***\n\nHeading\n-------\n:::\n",
            "::: note\nProse\n\n***\n\nHeading\n-------\n:::\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
            let rule = MD090NoHrBeforeHeading::new();
            assert!(rule.check(&ctx).unwrap().is_empty(), "content {content:?} was reported");
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }

        let content = "::: note\n\nProse\n\n***\n\n## Heading\n:::\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let rule = MD090NoHrBeforeHeading::new();
        assert_eq!(
            rule.check(&ctx).unwrap().iter().map(|w| w.line).collect::<Vec<_>>(),
            [5]
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "::: note\n\nProse\n\n## Heading\n:::\n");
    }

    #[test]
    fn backtick_myst_directive_body_is_the_accepted_false_negative() {
        // A markdown-bodied directive written with backticks starts as a code
        // fence, so `is_horizontal_rule` is settled to false on its body
        // before MyST detection clears `in_code_block`, and no rule reading
        // that flag sees the break. This is rumdl-wide, not this rule's:
        // MD082 draws the same line and is silent on the same document. The
        // colon-fenced form is the positive control - identical text, one
        // finding - so this test fails the day the shared flag is settled and
        // the false negative can be retired.
        let backtick = "# T\n\n```{note}\nIntro\n\n---\n\n## H\n```\n";
        let ctx = LintContext::new(backtick, MarkdownFlavor::MyST, None);
        assert!(!ctx.lines[5].is_horizontal_rule, "the shared flag was settled");
        assert!(MD090NoHrBeforeHeading::new().check(&ctx).unwrap().is_empty());

        let colon = "# T\n\n:::{note}\nIntro\n\n---\n\n## H\n:::\n";
        let ctx = LintContext::new(colon, MarkdownFlavor::MyST, None);
        let rule = MD090NoHrBeforeHeading::new();
        assert_eq!(
            rule.check(&ctx).unwrap().iter().map(|w| w.line).collect::<Vec<_>>(),
            [6]
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "# T\n\n:::{note}\nIntro\n\n## H\n:::\n");
    }

    #[test]
    fn break_above_a_colon_paragraph_is_reported_in_standard() {
        // Standard flavor gives `:::` no meaning, so `::: note` really is
        // setext text and the heading it forms is a section boundary: the
        // break above it is redundant, and the dash run stays its underline.
        let content = "***\n::: note\n---\n\n# H\n";
        assert_eq!(lines(content), [1]);
        assert_eq!(fix(content), "::: note\n---\n\n# H\n");
    }

    #[test]
    fn colon_paragraph_above_dash_run_is_a_setext_underline_in_standard() {
        // Standard flavor gives `:::` no meaning, so `::: note` is ordinary
        // paragraph text and the dash run under it is its real underline.
        let content = "::: note\n---\n\n# H\n";
        assert!(lines(content).is_empty());
        assert_eq!(fix(content), content);
    }

    #[test]
    fn multi_line_setext_break_is_the_accepted_false_negative() {
        // `Foo\nbar\n===` is one setext heading, so the break above the
        // blank line stands directly before it. The detector's record names
        // only `bar`, the underline's neighbor, so the upward walk stops on
        // `Foo`; reaching the break would mean re-deriving the paragraph's
        // extent here. One unreported break is the accepted cost.
        assert!(lines("***\n\nFoo\nbar\n===\n").is_empty());
    }

    #[test]
    fn tight_spacing_is_still_a_break_before_a_heading() {
        assert_eq!(lines("Prose\n\n---\n## Next\n"), [3]);
    }

    #[test]
    fn break_on_first_line_is_flagged() {
        // A lone `---` on line 1 with no later `---` is a thematic break.
        assert_eq!(lines("---\n\n# Title\n"), [1]);
    }

    #[test]
    fn leading_break_pair_is_front_matter_not_a_run() {
        // A document that starts with `---` and contains a later `---` is
        // YAML front matter to the parser, so neither line is a break here.
        assert!(lines("---\n\n---\n\n## H\n").is_empty());
    }

    #[test]
    fn front_matter_delimiters_are_not_breaks() {
        assert!(lines("---\ntitle: x\n---\n\n# Title\n").is_empty());
    }

    #[test]
    fn break_after_front_matter_is_flagged() {
        assert_eq!(lines("---\ntitle: x\n---\n\n---\n\n# Title\n"), [5]);
    }

    #[test]
    fn every_break_spelling_is_flagged() {
        for marker in ["***", "___", "- - -", "* * *", "   ---", "-----"] {
            let content = format!("Prose\n\n{marker}\n\n## H\n");
            assert_eq!(lines(&content), [3], "marker {marker:?}");
        }
    }

    #[test]
    fn indented_code_is_not_a_break() {
        assert!(lines("Prose\n\n    ---\n\n## H\n").is_empty());
    }

    #[test]
    fn break_before_setext_heading_is_flagged() {
        assert_eq!(lines("Prose\n\n---\n\nNext topic\n----------\n"), [3]);
    }

    #[test]
    fn run_of_breaks_flags_each_with_disjoint_ranges() {
        let content = "Prose\n\n---\n\n---\n\n## H\n";
        let w = check(content);
        assert_eq!(w.iter().map(|w| w.line).collect::<Vec<_>>(), [3, 5]);
        let first = w[0].fix.as_ref().unwrap().range.clone();
        let second = w[1].fix.as_ref().unwrap().range.clone();
        assert_eq!(&content[first.clone()], "---\n\n");
        assert_eq!(&content[second.clone()], "---\n\n");
        assert_eq!(first.end, second.start, "the two deletions abut and do not overlap");
    }

    #[test]
    fn comment_between_break_and_heading_is_not_adjacent() {
        assert!(lines("Prose\n\n---\n\n<!-- c -->\n\n## H\n").is_empty());
    }

    #[test]
    fn reference_definition_between_is_not_adjacent() {
        assert!(lines("Prose\n\n---\n\n[ref]: https://example.com\n\n## H\n").is_empty());
    }

    #[test]
    fn break_after_heading_is_not_flagged() {
        assert!(lines("## H\n\n---\n\nProse\n").is_empty());
    }

    #[test]
    fn break_inside_blockquote_is_left_alone() {
        assert!(lines("> ---\n>\n> ## H\n").is_empty());
    }

    #[test]
    fn break_inside_list_item_is_left_alone() {
        assert!(lines("- item\n\n  ---\n\n  ## H\n").is_empty());
    }

    #[test]
    fn break_that_ends_a_list_is_flagged() {
        // A thematic break at column 0 closes the list, so it is top-level.
        assert_eq!(lines("- item\n\n---\n\n## H\n"), [3]);
    }

    #[test]
    fn breaks_hidden_in_fences_comments_and_math_are_ignored() {
        assert!(lines("```\n---\n```\n\n## H\n").is_empty());
        assert!(
            lines(" ```\n---\n```\n\n## H\n").is_empty(),
            "a fence may be indented up to three spaces"
        );
        assert!(lines("<!--\n---\n-->\n\n## H\n").is_empty());
        assert!(lines("$$\n---\n$$\n\n## H\n").is_empty());
    }

    #[test]
    fn hashtag_is_not_a_heading() {
        assert!(lines("Prose\n\n---\n\n#hashtag\n").is_empty());
    }

    #[test]
    fn headings_missing_their_space_follow_the_parser_verdict() {
        // `valid_headings()` is rumdl's shared definition: `##hashtag` and
        // `#Hashtag` are headings missing their space (MD018 fixes them, MD022
        // spaces them), so the break above them is redundant just the same.
        assert_eq!(lines("Prose\n\n---\n\n##hashtag\n"), [3]);
        assert_eq!(lines("Prose\n\n---\n\n#Hashtag\n"), [3]);
    }

    #[test]
    fn attribute_line_between_is_content() {
        // A standalone `{#id}` line is not blank, so the break is not directly
        // before the heading.
        assert!(lines("Prose\n\n---\n\n{#custom}\n## H\n").is_empty());
    }

    #[test]
    fn break_inside_markdown_html_block_is_flagged_in_every_flavor() {
        // `markdown="1"` opts the element's content into Markdown, and the blank
        // line after the opening tag ends the HTML block, so the break and the
        // heading are ordinary top-level lines under both flavors.
        let content = "<div markdown=\"1\">\n\n---\n\n## H\n\n</div>\n";
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
            let reported: Vec<usize> = check_in(content, flavor).iter().map(|w| w.line).collect();
            assert_eq!(reported, [3], "flavor {flavor:?}");
        }
    }

    #[test]
    fn break_inside_pandoc_div_is_flagged_and_fix_keeps_the_fences() {
        // A fenced div's body is ordinary Markdown, so the break is real and
        // its lines carry no prefix; the deletion touches no fence line.
        let content = "# T\n\n::: note\nIntro\n\n---\n\n## H\n\nBody\n:::\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let rule = MD090NoHrBeforeHeading::new();
        let w = rule.check(&ctx).unwrap();
        assert_eq!(w.iter().map(|w| w.line).collect::<Vec<_>>(), [6]);
        assert_eq!(rule.fix(&ctx).unwrap(), "# T\n\n::: note\nIntro\n\n## H\n\nBody\n:::\n");
    }

    #[test]
    fn break_inside_myst_directive_is_flagged() {
        let content = "# T\n\n:::{note}\nIntro\n\n---\n\n## H\n\nBody\n:::\n";
        let reported: Vec<usize> = check_in(content, MarkdownFlavor::MyST).iter().map(|w| w.line).collect();
        assert_eq!(reported, [6]);
    }

    #[test]
    fn heading_inside_blockquote_is_left_alone() {
        assert!(lines("Prose\n\n---\n\n> ## H\n").is_empty());
    }

    #[test]
    fn empty_blockquote_line_between_is_content_not_a_blank() {
        // `LineInfo::is_blank` is true for a bare `>`; the walk must read the
        // source instead, or the fix deletes the blockquote.
        assert!(lines("Prose\n\n---\n\n>\n\n## H\n").is_empty());
        assert!(lines("Prose\n\n---\n\n> \n\n## H\n").is_empty());
    }

    #[test]
    fn nbsp_line_between_break_and_heading_is_content() {
        // A no-break space renders as content, so the line holding it is not
        // blank and the break is not directly before the heading.
        assert!(lines("Prose\n\n***\n\u{00A0}\n## H\n").is_empty());
    }

    #[test]
    fn space_and_tab_line_between_break_and_heading_is_blank() {
        assert_eq!(lines("Prose\n\n---\n \t\n## H\n"), [3]);
    }

    #[test]
    fn skips_documents_without_headings_or_breaks() {
        let ctx = LintContext::new("Prose\n\n---\n\nMore prose\n", MarkdownFlavor::Standard, None);
        assert!(MD090NoHrBeforeHeading::new().should_skip(&ctx));
        let ctx = LintContext::new("# Only a heading\n", MarkdownFlavor::Standard, None);
        assert!(MD090NoHrBeforeHeading::new().should_skip(&ctx));
        let ctx = LintContext::new("Prose\n\n---\n\n## H\n", MarkdownFlavor::Standard, None);
        assert!(!MD090NoHrBeforeHeading::new().should_skip(&ctx));
    }

    // Fix

    #[test]
    fn fix_removes_break_and_keeps_blank_above_it() {
        assert_eq!(
            fix("# Title\n\n## Topic\n\nProse.\n\n---\n\n## Next Topic\n\nMore.\n"),
            "# Title\n\n## Topic\n\nProse.\n\n## Next Topic\n\nMore.\n"
        );
    }

    #[test]
    fn fix_tight_spacing_leaves_one_blank_line() {
        assert_eq!(fix("Prose\n\n---\n## Next\n"), "Prose\n\n## Next\n");
    }

    #[test]
    fn fix_tight_break_above_setext_heading_keeps_separation() {
        // With no blank line above the break, a plain deletion would butt
        // `Prose` against `Next`, merging both into one setext heading.
        assert_eq!(fix("Prose\n***\nNext\n====\n"), "Prose\n\nNext\n====\n");
    }

    #[test]
    fn fix_tight_break_above_atx_heading_keeps_separation() {
        assert_eq!(fix("Prose\n***\n## Next\n"), "Prose\n\n## Next\n");
    }

    #[test]
    fn fix_tight_run_leaves_a_single_blank_line() {
        assert_eq!(fix("Prose\n***\n***\n## H\n"), "Prose\n\n## H\n");
    }

    #[test]
    fn fix_break_on_first_line_puts_heading_first() {
        assert_eq!(fix("---\n\n# Title\n"), "# Title\n");
    }

    #[test]
    fn fix_break_after_front_matter() {
        assert_eq!(
            fix("---\ntitle: x\n---\n\n---\n\n# Title\n"),
            "---\ntitle: x\n---\n\n# Title\n"
        );
    }

    #[test]
    fn fix_break_before_setext_heading() {
        assert_eq!(
            fix("Prose\n\n---\n\nNext topic\n----------\n"),
            "Prose\n\nNext topic\n----------\n"
        );
    }

    #[test]
    fn fix_run_of_breaks_in_one_pass_and_is_idempotent() {
        let once = fix("Prose\n\n---\n\n---\n\n## H\n");
        assert_eq!(once, "Prose\n\n## H\n");
        assert_eq!(fix(&once), once);
    }

    #[test]
    fn fix_preserves_crlf_line_endings() {
        // The LSP hands the rule the editor's text as is, so a deletion must
        // remove the break's own CRLF and nothing else.
        assert_eq!(fix("Prose\r\n\r\n---\r\n\r\n## H\r\n"), "Prose\r\n\r\n## H\r\n");
    }

    #[test]
    fn fix_returns_clean_document_unchanged() {
        let content = "Prose\n\n## H\n\nMore\n\n---\n\nTail\n";
        assert_eq!(fix(content), content);
    }
}
