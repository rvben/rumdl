//! Rule MD086: Comment delimiters must be closed.
//!
//! An opener with no closer does not fail loudly. `<!--` with no `-->` after it
//! is an HTML block that runs to the end of the document, so every heading,
//! list and paragraph below it disappears from the rendered page while the
//! source still looks complete. Mid-paragraph the failure inverts: CommonMark
//! renders the unmatched `<!--` as literal text, so the note the author meant to
//! hide is published instead.
//!
//! Either way no other rule reports the missing closer, and `rumdl fmt` will
//! not add one, so the document lints clean without this rule. The only visible
//! symptom is content that stops appearing on the rendered page.
//!
//! In the Obsidian flavor the same applies to `%%`, whose closer is another
//! `%%`. Other flavors treat `%%` as ordinary text and are not checked for it.
//!
//! A degenerate `<!-->` or `<!--->` is a complete comment in CommonMark (the
//! opener's own dashes close it) and is not reported.
//!
//! Detection only. Where a missing `-->` belongs is a guess: appending one at
//! the end of the document would comment out everything the author meant to
//! publish, and inserting it after the first line would hide nothing but assume
//! the comment was a one-liner.

use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// A comment syntax whose opener was never closed.
struct UnclosedComment {
    /// Byte offset of the opener.
    offset: usize,
    /// The opener as written, which is also its length in characters.
    opener: &'static str,
    /// The closer the document is missing.
    closer: &'static str,
    /// Name of the comment syntax, for the message.
    syntax: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct MD086NoUnclosedComments;

impl MD086NoUnclosedComments {
    pub fn new() -> Self {
        Self
    }

    fn warning(&self, ctx: &LintContext, unclosed: &UnclosedComment) -> LintWarning {
        let (line, column) = ctx.offset_to_line_col(unclosed.offset);
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line,
            column,
            end_line: line,
            end_column: column + unclosed.opener.chars().count(),
            message: format!(
                "Unclosed {} comment: '{}' has no matching '{}'",
                unclosed.syntax, unclosed.opener, unclosed.closer
            ),
            fix: None,
        }
    }
}

impl Rule for MD086NoUnclosedComments {
    fn name(&self) -> &'static str {
        "MD086"
    }

    fn description(&self) -> &'static str {
        "Comments should be closed"
    }

    fn category(&self) -> RuleCategory {
        // Not `Html`: that category is skipped for content without a `<`, which
        // would drop every Obsidian `%%` comment.
        RuleCategory::Other
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.unterminated_html_comment().is_none() && ctx.unterminated_obsidian_comment().is_none()
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        // Both scanners run during context construction and each reports its
        // first unclosed opener.
        //
        // An `<!--` that an Obsidian comment hides is already gone by this
        // point: the HTML scan is re-resolved against those comments when the
        // context is built, and Obsidian hides everything from an unclosed `%%`
        // to the end of the note.
        //
        // The reverse is deliberately not suppressed. A `%%` below an unclosed
        // `<!--` is still a closer the author has to add, and the document
        // already fails on the opener above it, so listing it costs one line on
        // a file that is failing either way - while dropping it would lose a
        // real missing closer.
        let html = ctx.unterminated_html_comment().map(|offset| UnclosedComment {
            offset,
            opener: "<!--",
            closer: "-->",
            syntax: "HTML",
        });
        let obsidian = ctx.unterminated_obsidian_comment().map(|offset| UnclosedComment {
            offset,
            opener: "%%",
            closer: "%%",
            syntax: "Obsidian",
        });
        let mut unclosed: Vec<UnclosedComment> = [html, obsidian].into_iter().flatten().collect();
        unclosed.sort_by_key(|c| c.offset);

        Ok(unclosed.iter().map(|c| self.warning(ctx, c)).collect())
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detection only: any inserted closer would decide for the author which
        // part of the document was meant to be hidden.
        Ok(ctx.content.to_string())
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

    fn check_with(content: &str, flavor: MarkdownFlavor) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, flavor, None);
        MD086NoUnclosedComments::new().check(&ctx).unwrap()
    }

    fn check(content: &str) -> Vec<LintWarning> {
        check_with(content, MarkdownFlavor::Standard)
    }

    #[test]
    fn reports_an_html_comment_that_is_never_closed() {
        let content = "# Title\n\n<!-- a note that never ends\n\n## Section\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (3, 1));
        assert_eq!(warnings[0].end_column, 5, "the warning spans the opener");
        assert_eq!(
            warnings[0].message,
            "Unclosed HTML comment: '<!--' has no matching '-->'"
        );
        assert!(warnings[0].fix.is_none(), "the closer's place is a guess");
    }

    #[test]
    fn accepts_a_closed_html_comment() {
        assert!(check("# Title\n\n<!-- a note -->\n\n## Section\n").is_empty());
    }

    #[test]
    fn accepts_a_multi_line_html_comment() {
        assert!(check("<!--\nline one\nline two\n-->\n\nText\n").is_empty());
    }

    #[test]
    fn accepts_degenerate_comments() {
        // CommonMark closes these with the opener's own dashes, so the text
        // after them renders and the document has no unclosed comment.
        for content in ["<!--> text\n", "<!---> text\n", "<!----> text\n"] {
            assert!(check(content).is_empty(), "{content:?} is a complete comment");
        }
    }

    #[test]
    fn reports_an_unclosed_opener_after_a_closed_comment() {
        let content = "<!-- first -->\n\nText\n\n<!-- second\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (5, 1));
    }

    #[test]
    fn reports_an_unclosed_opener_inside_a_paragraph() {
        // Here CommonMark publishes the marker as literal text rather than
        // hiding what follows, but the author still wrote a comment that is not
        // one.
        let content = "Some prose <!-- an aside\n\nMore prose.\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (1, 12));
    }

    #[test]
    fn ignores_an_opener_inside_a_fenced_code_block() {
        let content = "```html\n<!-- sample markup\n```\n\nText\n";
        assert!(check(content).is_empty(), "code shows delimiters, it does not use them");
    }

    #[test]
    fn ignores_an_opener_inside_a_code_span() {
        assert!(check("An opener is written `<!--` in HTML.\n").is_empty());
    }

    #[test]
    fn reports_a_real_opener_that_follows_a_literal_one() {
        let content = "An opener is written `<!--` in HTML.\n\n<!-- and here is a real one\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (3, 1));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let content = "Работа <!-- заметка\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (1, 8));
        assert_eq!(warnings[0].end_column, 12);
    }

    #[test]
    fn reports_an_unclosed_obsidian_comment() {
        let content = "# Title\n\n%% a note that never ends\n\n## Section\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (3, 1));
        assert_eq!(warnings[0].end_column, 3, "the warning spans the opener");
        assert_eq!(
            warnings[0].message,
            "Unclosed Obsidian comment: '%%' has no matching '%%'"
        );
    }

    #[test]
    fn accepts_a_closed_obsidian_comment() {
        assert!(check_with("Text %% a note %% more text\n", MarkdownFlavor::Obsidian).is_empty());
    }

    #[test]
    fn accepts_an_obsidian_comment_closing_at_the_end_of_the_document() {
        // The closed range ends at the end of the content, exactly like an
        // unclosed one would, so this is what tells the two apart.
        assert!(check_with("Text %% a note %%", MarkdownFlavor::Obsidian).is_empty());
    }

    #[test]
    fn ignores_obsidian_comments_outside_the_obsidian_flavor() {
        let content = "# Title\n\n%% a note that never ends\n";
        assert!(check(content).is_empty(), "%% is ordinary text in other flavors");
    }

    #[test]
    fn ignores_an_html_opener_inside_an_unclosed_obsidian_comment() {
        // Obsidian hides everything from an unclosed `%%` to the end of the
        // note, so the `<!--` on line 3 is text inside that comment rather than
        // a second unclosed opener.
        let content = "%% an Obsidian note\n\n<!-- an HTML note\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].line, 1);
        assert!(warnings[0].message.contains("Obsidian"));
    }

    #[test]
    fn reports_an_obsidian_opener_below_an_unclosed_inline_html_opener() {
        // The reverse does not hold: mid-paragraph CommonMark renders `<!--` as
        // literal text, so it hides nothing and the `%%` below it is its own
        // problem. Reporting only the first would lose it.
        let content = "Some prose <!-- an aside\n\n%% an Obsidian note\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert_eq!(warnings.len(), 2, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (1, 12));
        assert!(warnings[0].message.contains("HTML"));
        assert_eq!((warnings[1].line, warnings[1].column), (3, 1));
        assert!(warnings[1].message.contains("Obsidian"));
    }

    #[test]
    fn reports_an_obsidian_opener_below_an_unclosed_html_block() {
        // A line-start `<!--` opens an HTML block that runs to the end of the
        // document, so the `%%` below it renders as nothing. It is still a
        // closer the author has to add once the block above it is closed, and
        // the file already fails on that opener, so both are listed.
        let content = "<!-- an aside\n\n%% an Obsidian note\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert_eq!(warnings.len(), 2, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (1, 1));
        assert!(warnings[0].message.contains("HTML"));
        assert_eq!((warnings[1].line, warnings[1].column), (3, 1));
        assert!(warnings[1].message.contains("Obsidian"));
    }

    #[test]
    fn ignores_an_html_opener_inside_a_closed_obsidian_comment() {
        // Obsidian hides the text between the `%%` pair, so the `<!--` there is
        // never a comment opener.
        let content = "# Title\n\n%% note <!-- marker %%\n\nVisible text.\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn reports_a_real_opener_below_one_hidden_in_an_obsidian_comment() {
        // Suppressing the hidden opener must resume the search rather than end
        // it: the opener on line 5 is the one the author has to close.
        let content = "%% note <!-- marker %%\n\n<!-- a genuinely unclosed one\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (3, 1));
        assert!(warnings[0].message.contains("HTML"));
    }

    #[test]
    fn ignores_an_opener_in_front_matter() {
        // `<!--` in a YAML value is data, not a delimiter, and renderers strip
        // front matter before parsing markdown at all.
        let content = "---\nauthor: \"a <!-- b\"\n---\n\n# Title\n";
        assert!(check(content).is_empty(), "got: {:?}", check(content));
    }

    #[test]
    fn ignores_an_obsidian_opener_in_front_matter() {
        let content = "---\ntitle: \"50%% off\"\n---\n\n# Title\n";
        let warnings = check_with(content, MarkdownFlavor::Obsidian);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn reports_a_body_opener_below_front_matter_holding_one() {
        let content = "---\nauthor: \"a <!-- b\"\n---\n\n# Title\n\n<!-- a real one\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (7, 1));
    }

    #[test]
    fn accepts_a_document_with_no_comments() {
        assert!(check("# Title\n\nJust prose.\n").is_empty());
    }

    #[test]
    fn fix_leaves_the_document_alone() {
        let content = "# Title\n\n<!-- a note that never ends\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let rule = MD086NoUnclosedComments::new();
        assert_eq!(rule.fix(&ctx).unwrap(), content);
        assert_eq!(rule.fix_capability(), FixCapability::Unfixable);
    }
}
