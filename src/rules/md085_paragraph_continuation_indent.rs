//! Rule MD085: Paragraph continuation lines should not be indented.
//!
//! CommonMark strips leading whitespace from every line of a paragraph after the
//! first, so indentation there is invisible in the rendered output and only makes
//! the source inconsistent. This rule removes it.
//!
//! Indentation is structural everywhere else, so the rule is deliberately narrow:
//! it only touches paragraphs that no container owns. Inside a list item, a
//! blockquote, a definition list, a footnote body, a table or an MkDocs container,
//! the leading whitespace carries meaning and is left alone.
//!
//! Two more things keep it invisible. A continuation line whose indentation is the
//! only reason it is not a block start stays where it is, because at the margin it
//! would interrupt the paragraph. And a region opened by a line beginning with `<`
//! stays untouched until the blank line that ends it, because that is where an HTML
//! block ends and everything inside one is the author's raw markup.

use crate::lint_context::{HeadingStyle, LineInfo, LintContext};
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// What the line just examined was, which decides how to read the next one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    /// No block is open: the next non-blank line starts one.
    Between,
    /// A top-level paragraph is open, so its next line is a continuation.
    Paragraph,
    /// A container is open. Its later lines can look like bare prose (a lazy
    /// blockquote or list continuation carries no markers of its own), so the state
    /// has to outlive them.
    Container,
}

#[derive(Debug, Clone, Default)]
pub struct MD085ParagraphContinuationIndent;

impl MD085ParagraphContinuationIndent {
    pub fn new() -> Self {
        Self
    }

    /// Whether this line can belong to a paragraph that no container owns.
    ///
    /// `is_paragraph_context` rules out the non-paragraph blocks (code, HTML, math,
    /// headings, front matter, extension blocks); `in_container` rules out the
    /// containers, where the leading whitespace is what puts the line inside them;
    /// `looks_like_html` rules out the raw HTML the block detector leaves unmarked.
    fn is_top_level_prose(line: &LineInfo, content: &str) -> bool {
        line.is_paragraph_context()
            && !line.is_blank
            && !Self::in_container(line)
            && !Self::looks_like_html(line, content)
    }

    /// Whether this line is raw HTML rather than prose.
    ///
    /// An HTML block runs to the next blank line, so every line under one is raw HTML
    /// whose whitespace is the author's. rumdl marks a block that opens and closes on
    /// the same line, and a stray closing tag, on that line alone, and does not treat
    /// an inline-level tag as a block start at all. A line that begins with `<`
    /// therefore opens the region here, and the state machine keeps it open until the
    /// blank line that ends it.
    fn looks_like_html(line: &LineInfo, content: &str) -> bool {
        line.content(content).trim_start().starts_with('<')
    }

    /// Whether a container owns this line.
    fn in_container(line: &LineInfo) -> bool {
        line.in_list_block
            || line.blockquote.is_some()
            || line.in_table_block
            || line.in_definition_list
            || line.in_footnote_definition
            || line.in_admonition
            || line.in_content_tab
            || line.in_mkdocs_html_markdown
            || line.in_pandoc_div
            || line.in_mkdocstrings
            || line.in_myst_directive
            || line.in_jsx_block
            || line.in_jsx_expression
            || line.in_esm_block
            || line.in_mdx_comment
            || line.in_obsidian_comment
    }

    /// Whether a non-paragraph line ends its block, so the line after it starts a new
    /// one.
    ///
    /// Only a block that can own a lazy continuation has to outlive its last line: a
    /// lazy line carries no marker of its own, so it reads exactly like top-level
    /// prose. What a lazy line continues is a paragraph, so a container whose last
    /// line is one of these has no lazy continuation to expect either.
    ///
    /// A heading inside a container is the same case but is invisible here, because
    /// headings are recorded only at the top level. Prose below one therefore keeps the
    /// container's benefit of the doubt and goes unchecked.
    ///
    /// Every block listed here carries its own end: a closing fence, a dedent, a
    /// delimiter line. HTML is the exception and is deliberately absent, because an
    /// HTML block ends at a blank line rather than at its closing tag.
    fn ends_its_block(line: &LineInfo) -> bool {
        if let Some(heading) = &line.heading {
            // A setext heading's underline is still to come.
            return heading.style == HeadingStyle::ATX;
        }
        line.in_code_block
            || line.in_front_matter
            || line.in_math_block
            || line.is_horizontal_rule
            || line.is_kramdown_block_ial
            || line.is_myst_comment
    }

    /// Whether this line is the text of a setext heading, whose underline follows on
    /// the next line and carries no markers of its own.
    fn opens_setext_heading(line: &LineInfo) -> bool {
        match &line.heading {
            Some(heading) => heading.style != HeadingStyle::ATX,
            None => false,
        }
    }

    /// The leading whitespace CommonMark strips from a paragraph continuation line,
    /// which is spaces and tabs only. Other Unicode whitespace is content and renders,
    /// so removing it would change the document.
    fn strippable_indent(text: &str) -> usize {
        text.len() - text.trim_start_matches([' ', '\t']).len()
    }

    /// Whether the line would open a block once its indentation is gone.
    ///
    /// A continuation line is free whitespace only for as long as it stays inside the
    /// paragraph. At column zero `#` opens a heading, `>` a blockquote, `-`, `+`, `*`
    /// and a digit a list item, `-`, `*` and `_` a thematic break, `-` and `=` a setext
    /// underline, `` ` `` and `~` a fence, `<` an HTML block, `|` a table row, `$` a math
    /// block, `[` a link reference or footnote definition, `:` a definition, `!` an
    /// admonition, `{` an attribute list and `%` a comment. Each of those interrupts the
    /// paragraph in at least one of the dialects rumdl reads, so here the indentation is
    /// what keeps the line prose and removing it would restructure the document.
    ///
    /// The set is not conditioned on the flavor in force, because a document is often read
    /// by more than one tool and a marker that is inert to one is structural to the next.
    ///
    /// The test is the first byte rather than the whole marker. That accepts lines which
    /// would have stayed prose, which costs a finding and never a document.
    fn would_open_a_block(text: &str) -> bool {
        const BLOCK_STARTERS: &[u8] = b"#>-+*_=`~<|[{:$!%0123456789";
        match text.trim_start_matches([' ', '\t']).as_bytes().first() {
            Some(byte) => BLOCK_STARTERS.contains(byte),
            None => false,
        }
    }
}

impl Rule for MD085ParagraphContinuationIndent {
    fn name(&self) -> &'static str {
        "MD085"
    }

    fn description(&self) -> &'static str {
        "Paragraph continuation lines should not be indented"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        // Nothing to strip unless some line begins with a space or a tab.
        !ctx.lines
            .iter()
            .any(|line| !line.is_blank && Self::strippable_indent(line.content(ctx.content)) > 0)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut state = Block::Between;
        let mut expect_setext_underline = false;

        for (idx, line) in ctx.lines.iter().enumerate() {
            if std::mem::take(&mut expect_setext_underline) {
                state = Block::Between;
                continue;
            }

            if line.is_blank {
                state = Block::Between;
                continue;
            }

            if !Self::is_top_level_prose(line, ctx.content) {
                expect_setext_underline = Self::opens_setext_heading(line);
                state = if Self::ends_its_block(line) {
                    Block::Between
                } else {
                    Block::Container
                };
                continue;
            }

            match state {
                // The line that opens a paragraph is its first line, whose indentation
                // decides what the block is. Only later lines are free whitespace.
                Block::Between => state = Block::Paragraph,
                // Prose reached while a container is open is a lazy continuation of
                // that container, not a paragraph of its own.
                Block::Container => {}
                Block::Paragraph => {
                    // Leading whitespace inside a multi-line code span is span content.
                    if line.in_code_span_continuation {
                        continue;
                    }
                    // A shortcode tag spanning several lines is one token to the
                    // renderer, so how its arguments are laid out inside it is the
                    // author's, not paragraph indentation to strip.
                    if ctx.is_in_shortcode(line.byte_offset) {
                        continue;
                    }
                    let text = line.content(ctx.content);
                    let indent = Self::strippable_indent(text);
                    if indent == 0 {
                        continue;
                    }
                    // At column zero the line would stop being prose.
                    if Self::would_open_a_block(text) {
                        continue;
                    }

                    let line_num = idx + 1;
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: line_num,
                        column: 1,
                        end_line: line_num,
                        end_column: 1 + indent,
                        severity: Severity::Warning,
                        message: "Paragraph continuation line should not be indented".to_string(),
                        fix: Some(Fix::new(line.byte_offset..line.byte_offset + indent, String::new())),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

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
        Box::new(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use pulldown_cmark::{Options, Parser};

    fn warned_lines(content: &str, flavor: MarkdownFlavor) -> Vec<usize> {
        let ctx = LintContext::new(content, flavor, None);
        MD085ParagraphContinuationIndent::new()
            .check(&ctx)
            .unwrap()
            .iter()
            .map(|w| w.line)
            .collect()
    }

    fn fixed(content: &str, flavor: MarkdownFlavor) -> String {
        let ctx = LintContext::new(content, flavor, None);
        let rule = MD085ParagraphContinuationIndent::new();
        let out = rule.fix(&ctx).unwrap();
        // A warning and a rewrite have to agree, in both directions.
        assert_eq!(
            rule.check(&ctx).unwrap().is_empty(),
            out == content,
            "warnings and fix disagree for {content:?}"
        );
        out
    }

    /// The document as a reader sees it. CommonMark already strips the indentation of a
    /// paragraph continuation line, so a rewrite the rule is allowed to make renders
    /// byte-for-byte identically and any other one does not.
    fn render(markdown: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_MATH);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        let mut out = String::new();
        pulldown_cmark::html::push_html(&mut out, Parser::new_ext(markdown, options));
        out
    }

    /// Content the rule must leave exactly as it found it.
    fn assert_unchanged(content: &str, flavor: MarkdownFlavor) {
        assert!(
            warned_lines(content, flavor).is_empty(),
            "unexpected warning for {content:?}"
        );
        assert_eq!(fixed(content, flavor), content);
    }

    #[test]
    fn strips_indentation_from_continuation_lines() {
        let content = "This is some paragraph\n with line breaks\n  and indentation.\n";
        assert_eq!(warned_lines(content, MarkdownFlavor::Standard), vec![2, 3]);
        assert_eq!(
            fixed(content, MarkdownFlavor::Standard),
            "This is some paragraph\nwith line breaks\nand indentation.\n"
        );
    }

    #[test]
    fn leaves_the_first_line_of_a_paragraph_alone() {
        // The first line's indentation is what decides which block this is, so it is
        // never free whitespace. Only the lines after it are.
        assert_eq!(
            fixed("  Indented start\n   continuation\n", MarkdownFlavor::Standard),
            "  Indented start\ncontinuation\n"
        );
    }

    #[test]
    fn strips_a_continuation_indented_like_code() {
        // An indented code block cannot interrupt a paragraph, so four spaces here are
        // prose, and removing them renders identically.
        assert_eq!(
            fixed("para\n    four spaces\n", MarkdownFlavor::Standard),
            "para\nfour spaces\n"
        );
        assert_eq!(fixed("para\n\ttab\n", MarkdownFlavor::Standard), "para\ntab\n");
    }

    #[test]
    fn leaves_an_indented_code_block_alone() {
        assert_unchanged("para\n\n    real code block\n", MarkdownFlavor::Standard);
        assert_unchanged("```\n  fenced\n```\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn leaves_container_content_alone() {
        // Indentation is structural in every one of these: removing it reparents or
        // ends the construct.
        for content in [
            "- item\n  continuation\n",
            "1. item\n   continuation\n",
            "> quote\n>  continuation\n",
            "| a | b |\n|---|---|\n|  1 |  2 |\n",
            "[^1]: note\n  more of the note\n",
            "<div>\n  inner\n</div>\n",
            "$$\n  x = 1\n$$\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn leaves_html_alone_until_the_blank_line_that_ends_it() {
        // An HTML block runs to the next blank line whatever its tags do, so everything
        // under one is raw HTML whose whitespace belongs to the author. These are the
        // shapes rumdl's block detector does not mark past their first line: a tag that
        // opens and closes on one line, a stray closing tag, an inline-level tag, and a
        // comment.
        for content in [
            "<div>\nhtml\n</div>\npara\n  cont\n",
            "<h2>x</h2>\npara\n  cont\n",
            "</div>\npara\n  cont\n",
            "<em>\n  cont\n",
            "<!-- comment -->\npara\n  cont\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::Standard);
        }
        // The blank line ends it, and the paragraph below is checked again.
        assert_eq!(
            fixed("<h2>x</h2>\n\npara\n  cont\n", MarkdownFlavor::Standard),
            "<h2>x</h2>\n\npara\ncont\n"
        );
    }

    #[test]
    fn leaves_a_continuation_that_would_open_a_block_alone() {
        // At four spaces none of these can interrupt the paragraph, so each one is prose
        // that the rule sees. At column zero every one of them opens a block instead, so
        // the indentation is what keeps the line where the author put it.
        for line in [
            "# heading",
            "> quote",
            "- item",
            "+ item",
            "* item",
            "1. item",
            "***",
            "---",
            "===",
            "```",
            "~~~",
            "<div>",
            "| a | b |",
            "[^1]: note",
            "{: .class }",
            ": definition",
            "$$",
            "!!! note",
        ] {
            assert_unchanged(&format!("para\n    {line}\n"), MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn a_stripped_continuation_renders_identically() {
        // The rule's whole premise is that the indentation it removes is invisible.
        // Sweeping every printable ASCII character through the shapes that open a block
        // makes a missed block starter fail here instead of corrupting a document. Four
        // spaces is the interesting width: nothing can interrupt a paragraph from there,
        // so every one of these lines reaches the rule as prose.
        for byte in 0x21u8..=0x7e {
            let c = byte as char;
            for template in [
                "para\n    @ tail\n",
                "para\n    @@@\n",
                "para\n    @. tail\n",
                "para\n    @tail\n",
                "para\n    @ tail\n    more\n",
            ] {
                let content = template.replace('@', &c.to_string());
                let out = fixed(&content, MarkdownFlavor::Standard);
                assert_eq!(
                    render(&out),
                    render(&content),
                    "removing the indentation changed the rendering of {content:?}"
                );
            }
        }
    }

    #[test]
    fn a_stripped_continuation_is_still_prose_in_every_flavor() {
        // The render sweep above can only speak for standard Markdown, because the
        // reference parser knows no flavor. Every flavor adds markers of its own, so the
        // oracle here is rumdl's own reading of the document: a line the rule rewrote has
        // to still be the top-level prose it was, and so does every other line, since a
        // marker at the margin also reparents what follows it.
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::MDX,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
            MarkdownFlavor::MyST,
        ] {
            for byte in 0x21u8..=0x7e {
                let c = byte as char;
                for template in [
                    "para\n    @ tail\n",
                    "para\n    @@ tail\n",
                    "para\n    @@@\n",
                    "para\n    @tail\n",
                    "para\n    @ tail\n  more\n",
                ] {
                    let content = template.replace('@', &c.to_string());
                    let out = fixed(&content, flavor);
                    if out == content {
                        continue;
                    }
                    let before = LintContext::new(&content, flavor, None);
                    let after = LintContext::new(&out, flavor, None);
                    for (idx, (was, now)) in before.lines.iter().zip(after.lines.iter()).enumerate() {
                        assert_eq!(
                            MD085ParagraphContinuationIndent::is_top_level_prose(was, &content),
                            MD085ParagraphContinuationIndent::is_top_level_prose(now, &out),
                            "line {} stopped being prose under {flavor:?} when {content:?} became {out:?}",
                            idx + 1
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn leaves_lazy_continuation_of_a_container_alone() {
        // A lazy continuation line carries no marker of its own, so it reads exactly
        // like top-level prose. What it continues is the container above it.
        assert_unchanged("> quote\n  lazy one\n  lazy two\n", MarkdownFlavor::Standard);
        assert_unchanged("- item\n lazy one space\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn leaves_a_setext_underline_and_what_follows_alone() {
        // The underline carries no markers of its own, so without tracking the heading
        // it would read as a paragraph and the line after it as a continuation.
        assert_unchanged("Title\n=====\n  after underline\n", MarkdownFlavor::Standard);
        assert_unchanged("Title\n---\n  after underline\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn a_paragraph_after_a_closed_block_is_still_checked() {
        // None of these can own a lazy continuation line, so the prose below each one
        // is a paragraph of its own even without a blank line between them.
        for prefix in [
            "# Heading\n",
            "***\n",
            "```\ncode\n```\n",
            "$$\nx = 1\n$$\n",
            "Setext heading\n==============\n",
            "---\ntitle: front matter\n---\n",
        ] {
            let content = format!("{prefix}para\n  cont\n");
            assert_eq!(
                fixed(&content, MarkdownFlavor::Standard),
                format!("{prefix}para\ncont\n"),
                "continuation after {prefix:?} was not checked"
            );
        }
    }

    #[test]
    fn a_paragraph_after_a_closed_block_inside_a_container_is_still_checked() {
        // A lazy continuation line continues a paragraph. Neither of these is one, so the
        // blockquote ends with it and the prose below starts a paragraph of its own.
        for prefix in ["> ---\n", "> ```\n> code\n> ```\n"] {
            let content = format!("{prefix}para\n  cont\n");
            assert_eq!(
                fixed(&content, MarkdownFlavor::Standard),
                format!("{prefix}para\ncont\n"),
                "continuation after {prefix:?} was not checked"
            );
        }
    }

    #[test]
    fn real_world_documents_survive_the_fix() {
        // Minimized from documents the rule rewrote into something that renders
        // differently: a closing tag whose HTML block rumdl stops tracking, a list
        // marker and a template tag under a paragraph, and a setext underline.
        for content in [
            "```\n```\n</div>\n  dateformat.i18n = require('./lang/' + l)\n  return true;\n",
            ":   A site that uses `just-the-docs` automatically\n    - features that are likely to be removed.\n",
            "```html\n```\n  {% for stylesheet in page.page_css %}\n    <link rel=\"stylesheet\" href=\"{{ stylesheet }}\">\n",
            "![Image *\"alt\"* & \\\"\n    <em>html</em>\n    ---\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::Standard);
            assert_eq!(
                render(&fixed(content, MarkdownFlavor::Standard)),
                render(content),
                "the fix changed the rendering of {content:?}"
            );
        }
    }

    #[test]
    fn strips_only_the_whitespace_commonmark_strips() {
        // CommonMark strips spaces and tabs from a continuation line. Other Unicode
        // whitespace is content that renders, so removing it changes the document.
        assert_unchanged("para\n\u{a0}cont\n", MarkdownFlavor::Standard);
        assert_unchanged("para\n\u{3000}cont\n", MarkdownFlavor::Standard);
        assert_eq!(
            fixed("para\n  \u{3000}cont\n", MarkdownFlavor::Standard),
            "para\n\u{3000}cont\n"
        );
    }

    #[test]
    fn leaves_a_multi_line_code_span_alone() {
        // The leading whitespace on the closing line sits inside the span, so it is
        // content rather than indentation.
        assert_eq!(
            fixed("para `code\n  more` tail\n  cont\n", MarkdownFlavor::Standard),
            "para `code\n  more` tail\ncont\n"
        );
    }

    #[test]
    fn leaves_mkdocs_containers_alone() {
        for content in [
            "!!! note\n    body line\n    more body\n",
            "=== \"Tab\"\n    body line\n    more body\n",
            "Term\n\n: definition\n  more of the definition\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::MkDocs);
        }
    }

    #[test]
    fn preserves_hard_line_breaks() {
        // The rule only removes leading whitespace, so a two-space or backslash break
        // at the end of the previous line survives.
        assert_eq!(fixed("para  \n  next\n", MarkdownFlavor::Standard), "para  \nnext\n");
        assert_eq!(fixed("para\\\n  next\n", MarkdownFlavor::Standard), "para\\\nnext\n");
    }

    #[test]
    fn preserves_trailing_blank_lines_and_a_missing_final_newline() {
        assert_eq!(
            fixed("para\n  cont\n\n\n", MarkdownFlavor::Standard),
            "para\ncont\n\n\n"
        );
        assert_eq!(fixed("para\n  cont", MarkdownFlavor::Standard), "para\ncont");
    }

    #[test]
    fn fix_is_idempotent() {
        for content in [
            "This is some paragraph\n with line breaks\n  and indentation.\n",
            "  Indented start\n   continuation\n",
            "para\n  cont\n\n\n",
            "> quote\n  lazy\n",
        ] {
            let once = fixed(content, MarkdownFlavor::Standard);
            let ctx = LintContext::new(&once, MarkdownFlavor::Standard, None);
            assert_eq!(
                MD085ParagraphContinuationIndent::new().fix(&ctx).unwrap(),
                once,
                "fix is not idempotent for {content:?}"
            );
        }
    }

    #[test]
    fn warning_spans_the_indentation_it_removes() {
        let ctx = LintContext::new("para\n   cont\n", MarkdownFlavor::Standard, None);
        let warnings = MD085ParagraphContinuationIndent::new().check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].column, 1);
        assert_eq!(warnings[0].end_line, 2);
        assert_eq!(warnings[0].end_column, 4);
    }

    #[test]
    fn empty_and_blank_documents_are_untouched() {
        for content in ["", "\n", "   \n", "\n\n\n"] {
            assert_unchanged(content, MarkdownFlavor::Standard);
        }
    }
}
