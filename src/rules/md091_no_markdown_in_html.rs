//! Rule MD091: Markdown syntax inside an HTML block renders as literal text.
//!
//! CommonMark stops parsing markdown inside an HTML *block*, so a link written
//! there is published as its own source. `<details><summary>[Docs](/docs)</summary>`
//! puts the characters `[Docs](/docs)` on the page instead of a link, and nothing
//! else reports it: the document lints clean, the HTML is valid, and the page
//! renders without an error. The only symptom is markdown source visible to
//! readers, which an author usually discovers long after publishing.
//!
//! The dividing line is the block, not the tag. Markdown is fully alive inside
//! *inline* HTML, so `Intro <details><summary>[Docs](/docs)</summary>` (the tag
//! preceded by text on the same line) parses the link normally and is not
//! reported. Within one `<details>` a link on the line after the opener can be
//! dead while emphasis four lines later is live, because a blank line ends the
//! type-6 block and everything after it is markdown again. The rule therefore
//! asks only what the parser already decided: is this line inside an HTML block.
//!
//! The test for a real finding is whether the construct would have rendered
//! differently outside the block. Two exclusions follow from that directly:
//!
//! - **Undefined reference labels.** `[text][ref]` renders as a link only when
//!   `ref` is defined somewhere in the document. With no definition it is literal
//!   text *in both contexts*, so there is nothing the HTML block broke. This is
//!   what separates a dead link from `arr[i][j]`, `[tab][tab]` and the CSS and
//!   regex grammar (`[ a | b ]`, `[a-z][0-9]`) that fills HTML tables in
//!   reference documentation. Definedness is not a heuristic for that
//!   distinction, it is exactly the condition that makes the two differ.
//! - **Inside a tag.** `<div title="see [docs](/docs)">` puts the construct in an
//!   attribute value, which no markdown parser processes in any context.
//!
//! Two more are about intent rather than rendering, and the difference matters
//! because these constructs genuinely do render differently. An inline `<code>`
//! element does not stop markdown, so `<code>[a](b)</code>` outside a block
//! really does produce a link inside the code element; a backtick span outside a
//! block really does become `<code>`. Both are nevertheless silent here:
//!
//! - **`<code>` elements** and **backtick spans.** An author who wraps a
//!   construct in either is asking for it to be shown, not followed, and inside
//!   the block that is what they get. Reporting a broken *link* there would name
//!   the wrong problem.
//!
//! Three exclusions are about reachability:
//!
//! - **Raw-text elements.** `pre`, `script`, `style` and `textarea` hold literal
//!   text by design, so markdown-looking characters there are content.
//! - **`markdown="1"` containers.** kramdown, Python-Markdown and MkDocs parse
//!   those bodies, so the markdown really is markdown for those users.
//! - **HTML comments.** Nothing inside them reaches the page at all.
//!
//! Detection only. Rewriting `[text](url)` to `<a href="url">text</a>` is a
//! content transform in a domain rumdl cannot arbitrate: the right answer is
//! often to add a blank line instead, which moves the markdown out of the block
//! and changes the rendered structure.

use crate::lint_context::{LintContext, image_pattern, link_pattern};
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::html_block::TYPE_1_BLOCK_ELEMENTS;
use crate::utils::range_utils::byte_to_char_count;
use regex::Regex;
use std::sync::LazyLock;

/// Opening or closing tag of a CommonMark type-1 raw-text element, anywhere on a
/// line. A nested `<pre>` inside a `<table>` shares the enclosing block, so the
/// line-start classifier cannot see it.
static TYPE_1_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)<(/?)({})\b", TYPE_1_BLOCK_ELEMENTS.join("|"))).unwrap());

/// A `<code>` element's span on one line, including an unclosed opener running
/// to the end of the line.
static CODE_ELEMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<code\b[^>]*>.*?(?:</code\s*>|$)").unwrap());

/// A backtick code span. Inside an HTML block the backticks are literal, but the
/// author still wrote them to mean "show this", so the span's content is not a
/// construct the block broke.
static BACKTICK_SPAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`]*`").unwrap());

/// An HTML tag. Everything between `<` and `>` is markup, so a construct there
/// sits in an attribute value and is never parsed as markdown anywhere. The
/// leading letter keeps prose like `a < b and c > d` from swallowing a line.
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)</?[A-Za-z][^>]*>").unwrap());

/// What a match is, for the message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Construct {
    Link,
    Image,
}

impl Construct {
    fn noun(self) -> &'static str {
        match self {
            Construct::Link => "link",
            Construct::Image => "image",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MD091NoMarkdownInHtml;

impl MD091NoMarkdownInHtml {
    pub fn new() -> Self {
        Self
    }

    /// Byte ranges on `line` that hold no reportable construct: `<code>`
    /// elements, backtick spans, and the inside of any tag.
    fn shielded_ranges(line: &str) -> Vec<(usize, usize)> {
        CODE_ELEMENT
            .find_iter(line)
            .chain(BACKTICK_SPAN.find_iter(line))
            .chain(HTML_TAG.find_iter(line))
            .map(|m| (m.start(), m.end()))
            .collect()
    }

    fn in_any(ranges: &[(usize, usize)], start: usize) -> bool {
        ranges.iter().any(|(s, e)| start >= *s && start < *e)
    }

    /// Whether a match in reference form would have been a link outside the
    /// block. `caps` group 7 is the explicit label; an empty one is the collapsed
    /// form `[text][]`, whose label is the text itself. An inline `[a](b)` has no
    /// group 7 and always resolves, so it is always reportable.
    fn reference_resolves(ctx: &LintContext, caps: &regex::Captures<'_>) -> bool {
        let Some(explicit) = caps.get(7) else {
            return true;
        };
        let label = if explicit.as_str().is_empty() {
            caps.get(1).map_or("", |g| g.as_str())
        } else {
            explicit.as_str()
        };
        ctx.reference_definition(label).is_some()
    }

    /// Whether this line's content is raw text, updating the nesting depth of
    /// type-1 elements as it goes. A line carrying the opener is itself raw text
    /// from that point on, and a line carrying the closer is still inside.
    fn update_raw_text_depth(line: &str, depth: &mut i32) -> bool {
        let was_inside = *depth > 0;
        let mut opened_here = false;
        for cap in TYPE_1_TAG.captures_iter(line) {
            if cap.get(1).is_some_and(|s| s.as_str() == "/") {
                *depth -= 1;
            } else {
                *depth += 1;
                opened_here = true;
            }
        }
        if *depth < 0 {
            *depth = 0;
        }
        was_inside || opened_here
    }

    fn warning(
        &self,
        line_num: usize,
        line: &str,
        range: (usize, usize),
        construct: Construct,
        opener: usize,
    ) -> LintWarning {
        // `byte_to_char_count` is already 1-indexed, and `end_column` is
        // 1-indexed exclusive, so the end is the start plus the char length.
        let column = byte_to_char_count(line, range.0);
        let end_column = column + line[range.0..range.1].chars().count();
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line: line_num,
            column,
            end_line: line_num,
            end_column,
            message: format!(
                "Markdown {} renders as literal text: this line is inside the HTML block opened at line {}",
                construct.noun(),
                opener
            ),
            fix: None,
        }
    }
}

impl Rule for MD091NoMarkdownInHtml {
    fn name(&self) -> &'static str {
        "MD091"
    }

    fn description(&self) -> &'static str {
        "Markdown inside an HTML block renders as literal text"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Html
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        // Every reportable construct needs both a tag and a bracket.
        !ctx.content.contains('<') || !ctx.content.contains('[')
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut raw_text_depth = 0i32;
        let mut opener_line = 0usize;
        let mut prev_in_block = false;

        for (idx, line) in ctx.content.lines().enumerate() {
            let line_num = idx + 1;

            if !ctx.is_in_html_block(line_num) {
                prev_in_block = false;
                raw_text_depth = 0;
                continue;
            }
            // A run of consecutive block lines is one block; its first line is
            // the opener the message points at.
            if !prev_in_block {
                opener_line = line_num;
                raw_text_depth = 0;
            }
            prev_in_block = true;

            if Self::update_raw_text_depth(line, &mut raw_text_depth) {
                continue;
            }

            let Some(info) = ctx.line_info(line_num) else {
                continue;
            };
            // A `markdown="1"` container really is markdown for kramdown,
            // Python-Markdown and MkDocs users; a comment reaches no reader.
            if info.in_mkdocs_container() || info.in_html_comment {
                continue;
            }

            let shielded = Self::shielded_ranges(line);
            let mut image_ranges: Vec<(usize, usize)> = Vec::new();

            for caps in image_pattern().captures_iter(line) {
                let m = caps.get(0).expect("group 0 always matches");
                // Every image occupies its span whether or not it is reported,
                // so a link nested in one is never reported twice.
                image_ranges.push((m.start(), m.end()));
                if Self::in_any(&shielded, m.start()) || !Self::reference_resolves(ctx, &caps) {
                    continue;
                }
                warnings.push(self.warning(line_num, line, (m.start(), m.end()), Construct::Image, opener_line));
            }

            for caps in link_pattern().captures_iter(line) {
                let m = caps.get(0).expect("group 0 always matches");
                // An image is `!` plus a link; report it once, as an image.
                if image_ranges.iter().any(|(s, e)| m.start() >= *s && m.end() <= *e) {
                    continue;
                }
                if Self::in_any(&shielded, m.start()) || !Self::reference_resolves(ctx, &caps) {
                    continue;
                }
                warnings.push(self.warning(line_num, line, (m.start(), m.end()), Construct::Link, opener_line));
            }
        }

        warnings.sort_by_key(|w| (w.line, w.column));
        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detection only: converting the construct to HTML and adding a blank
        // line to end the block are different documents, and which one the
        // author meant is not derivable from the source.
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
        MD091NoMarkdownInHtml::new().check(&ctx).unwrap()
    }

    fn check(content: &str) -> Vec<LintWarning> {
        check_with(content, MarkdownFlavor::Standard)
    }

    #[test]
    fn reports_the_reported_shape() {
        let content = "<div align=\"center\">\n[Docs](/docs)\n</div>\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (2, 1));
        // end_column is 1-indexed exclusive, so it is one past the final `)`.
        assert_eq!(warnings[0].end_column, 14);
        assert_eq!(
            warnings[0].message,
            "Markdown link renders as literal text: this line is inside the HTML block opened at line 1"
        );
        assert!(warnings[0].fix.is_none());
    }

    #[test]
    fn reports_an_image() {
        let warnings = check("<div align=\"center\">\n![Screenshot](shot.png)\n</div>\n");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].end_column, 24, "the extent covers the whole image");
        assert!(warnings[0].message.contains("Markdown image"));
    }

    #[test]
    fn reports_an_image_once_not_also_as_its_inner_link() {
        // `![alt](url)` contains `[alt](url)`; only the image is reported.
        let warnings = check("<div>\n![a](b.png)\n</div>\n");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(warnings[0].message.contains("image"));
    }

    #[test]
    fn a_blank_line_ends_the_block_so_the_link_is_live() {
        // The dividing line is the block, not the tag: a blank line closes the
        // type-6 block and everything after it is markdown again.
        assert!(check("<div align=\"center\">\n\n[Docs](/docs)\n\n</div>\n").is_empty());
    }

    #[test]
    fn inline_html_does_not_open_a_block() {
        // Text before the tag on the same line keeps it inline, where markdown
        // is fully alive.
        assert!(check("Intro <details><summary>[Docs](/docs)</summary></details>\n").is_empty());
    }

    #[test]
    fn reports_a_reference_link_whose_label_is_defined() {
        // Defined outside the block: it would have been a link, so the block
        // broke it.
        let warnings = check("<div>\n[text][ref]\n</div>\n\n[ref]: https://example.com\n");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].column), (2, 1));
    }

    #[test]
    fn accepts_a_reference_link_whose_label_is_undefined() {
        // With no definition this is literal text inside the block AND outside
        // it, so there is no defect to report.
        assert!(check("<div>\n[text][nope]\n</div>\n").is_empty());
    }

    #[test]
    fn accepts_array_indexing() {
        // The whole `arr[i][j]` class falls out of the definedness gate.
        for content in [
            "<div>\narr[i][j]\n</div>\n",
            "<div>\nmatrix[0][1]\n</div>\n",
            "<div>\npress [tab][tab] to complete\n</div>\n",
            "<div>\n<td>[a-z][0-9]</td>\n</div>\n",
        ] {
            assert!(check(content).is_empty(), "flagged: {content:?}");
        }
    }

    #[test]
    fn collapsed_references_follow_the_same_gate() {
        // `[text][]` takes its label from the text itself.
        let defined = check("<div>\n[text][]\n</div>\n\n[text]: https://example.com\n");
        assert_eq!(defined.len(), 1, "got: {defined:?}");
        assert!(check("<div>\n[text][]\n</div>\n").is_empty());
    }

    #[test]
    fn reference_images_follow_the_same_gate() {
        let defined = check("<div>\n![alt][img]\n</div>\n\n[img]: shot.png\n");
        assert_eq!(defined.len(), 1, "got: {defined:?}");
        assert!(check("<div>\n![alt][missing]\n</div>\n").is_empty());
    }

    #[test]
    fn accepts_a_construct_inside_a_tag() {
        // An attribute value is markup, never parsed as markdown in any context.
        assert!(check("<div title=\"see [docs](/docs)\">\nbody\n</div>\n").is_empty());
        assert!(check("<div data-x=\"![a](b.png)\">\nbody\n</div>\n").is_empty());
    }

    #[test]
    fn accepts_a_construct_inside_a_code_element_or_backticks() {
        assert!(check("<div>\n<code>[a](b.md)</code>\n</div>\n").is_empty());
        assert!(check("<div>\n`[a](b.md)`\n</div>\n").is_empty());
        assert!(check("<div>\n`[a-z0-9]([a-z0-9-]{0,61})`\n</div>\n").is_empty());
    }

    #[test]
    fn accepts_raw_text_elements() {
        assert!(check("<pre>\n[a](b.md)\n</pre>\n").is_empty());
        assert!(check("<div>\n<pre>\n[a](b.md)\n</pre>\n</div>\n").is_empty());
        assert!(check("<script>\nvar x = [a](b);\n</script>\n").is_empty());
    }

    #[test]
    fn accepts_a_markdown_container() {
        // kramdown, Python-Markdown and MkDocs parse this body as markdown.
        assert!(check("<div markdown=\"1\">\n[Docs](/docs)\n</div>\n").is_empty());
    }

    #[test]
    fn accepts_an_html_comment() {
        assert!(check("<!-- [Docs](/docs) -->\n").is_empty());
        assert!(check("<!--\n[Docs](/docs)\n-->\n").is_empty());
        assert!(check("<div>\n<!-- [Docs](/docs) -->\n</div>\n").is_empty());
    }

    #[test]
    fn each_block_names_its_own_opener() {
        // `<span>` would not do here: it is not a CommonMark type-6 tag, so it
        // opens no block at all.
        let content = "<div>\n[a](1)\n</div>\n\ntext\n\n<section>\n[b](2)\n</section>\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 2, "got: {warnings:?}");
        assert!(warnings[0].message.ends_with("opened at line 1"));
        assert!(warnings[1].message.ends_with("opened at line 7"));
    }

    #[test]
    fn is_detection_only() {
        let rule = MD091NoMarkdownInHtml::new();
        assert!(matches!(rule.fix_capability(), FixCapability::Unfixable));
        let content = "<div>\n[Docs](/docs)\n</div>\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn skips_documents_that_cannot_contain_a_finding() {
        let ctx = LintContext::new("# Just a heading\n", MarkdownFlavor::Standard, None);
        assert!(MD091NoMarkdownInHtml::new().should_skip(&ctx));
    }
}
