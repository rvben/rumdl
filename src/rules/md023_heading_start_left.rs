/// Rule MD023: Headings must start at the left margin
///
/// See [docs/md023.md](../../docs/md023.md) for full documentation, configuration, and examples.
use crate::lint_context::HeadingContainer;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;

#[derive(Clone)]
pub struct MD023HeadingStartLeft;

impl MD023HeadingStartLeft {
    /// The warning for the indentation of one line of a heading held by
    /// `container`, or `None` when the line sits at its container's edge.
    ///
    /// The container's own indentation is not the heading's: removing it takes the
    /// heading out of the list item holding it. Only the columns past the item's
    /// content column are removed. The warning carries no fix where the edit
    /// cannot be shown to keep the heading where it is: a container the pass does
    /// not model, or a tab among the indentation of a line inside an item, whose
    /// width depends on the column it starts at.
    fn indentation_warning(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_idx: usize,
        container: HeadingContainer,
        continuation: bool,
        message: impl Fn(usize) -> String,
    ) -> Option<LintWarning> {
        let line = &ctx.lines[line_idx];
        let (edge, fixable) = match container {
            HeadingContainer::Outside => (0, true),
            HeadingContainer::Item { content_col } => {
                let tab_free = !line.content(ctx.content)[..line.indent].contains('\t');
                (content_col, tab_free)
            }
            HeadingContainer::Unknown => (0, false),
        };
        let indent = line.visual_indent.saturating_sub(edge);
        if indent == 0 {
            return None;
        }
        // A continuation line of a setext heading's text indented four columns
        // or more is text, whatever it spells. With the indentation gone it could
        // start a code fence, a quote or a list, and end the heading.
        let fixable = fixable && !(continuation && indent >= 4);
        // The indentation to remove is the bytes from the container's edge to the
        // text; the edge is spaces on every line that is fixed, so columns are bytes.
        let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
            line_idx + 1,
            edge.min(line.indent) + 1,
            line.indent.saturating_sub(edge),
        );
        let fix = fixable.then(|| {
            let line_start = ctx.line_start_byte(line_idx + 1).unwrap_or(0);
            Fix::new(
                line_start + edge.min(line.indent)..line_start + line.indent,
                String::new(),
            )
        });
        Some(LintWarning {
            rule_name: Some(self.name().to_string()),
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            severity: Severity::Warning,
            message: message(indent),
            fix,
        })
    }
}

impl Rule for MD023HeadingStartLeft {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.lines.is_empty() {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();

        // Process all headings using cached heading information
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            // Skip lines inside PyMdown blocks, admonitions, and content tabs:
            // those containers are indentation-scoped, so a heading indented
            // there is intentionally nested, not an accidental indent. This
            // deliberately excludes markdown="1" HTML divs (tag-scoped, content
            // needs no indentation, and detected in every flavor).
            if line_info.in_pymdown_block || line_info.in_admonition || line_info.in_content_tab {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                // Skip hashtag-like patterns (e.g., #tag, #123, #29039) for ATX level 1
                // These are likely issue refs or social hashtags, not intended headings
                if heading.level == 1 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    // Get first "word" of heading text (up to space, comma, or closing paren)
                    let first_word: String = heading
                        .text
                        .trim()
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ')')
                        .collect();
                    if let Some(first_char) = first_word.chars().next() {
                        // Skip if first word starts with lowercase or number
                        if first_char.is_lowercase() || first_char.is_numeric() {
                            continue;
                        }
                    }
                }

                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                );
                let container = ctx.heading_container(line_num + 1);

                // A setext heading's text is the whole paragraph its underline
                // ends, and CommonMark strips the leading whitespace of every one
                // of those lines, so each of them carries indentation of its own
                if is_setext {
                    let underline_line = line_num + 1;
                    let first_text_line = line_num + 1 - heading.text_lines;

                    for text_line in first_text_line..=line_num {
                        if let Some(warning) =
                            self.indentation_warning(ctx, text_line, container, text_line > first_text_line, |indent| {
                                format!("Setext heading should not be indented by {indent} spaces")
                            })
                        {
                            warnings.push(warning);
                        }
                    }

                    // The underline is indented independently of the text
                    if underline_line < ctx.lines.len()
                        && let Some(warning) = self.indentation_warning(ctx, underline_line, container, false, |_| {
                            "Setext heading underline should not be indented".to_string()
                        })
                    {
                        warnings.push(warning);
                    }
                } else if let Some(warning) = self.indentation_warning(ctx, line_num, container, false, |indent| {
                    format!("Heading should not be indented by {indent} spaces")
                }) {
                    warnings.push(warning);
                }
            }
        }

        // Protect the indentation edits themselves: visible template headings
        // can have Markdown indentation outside the template delimiters.
        warnings.retain(|warning| {
            warning
                .fix
                .as_ref()
                .is_none_or(|fix| !ctx.overlaps_template_code(fix.range.start, fix.range.end))
        });
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings
        if !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist
        ctx.lines.iter().all(|line| line.heading.is_none())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD023HeadingStartLeft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    #[test]
    fn test_basic_functionality() {
        let rule = MD023HeadingStartLeft;

        // Test with properly aligned headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with indented headings
        let content = "  # Heading 1\n ## Heading 2\n   ### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Should flag all three indented headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);

        // Test with setext headings
        let content = "Heading 1\n=========\n  Heading 2\n  ---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the indented heading and underline
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_issue_refs_skipped_but_real_headings_caught() {
        let rule = MD023HeadingStartLeft;

        // Issue refs should NOT be flagged (starts with number)
        let content = "- fix: issue\n  #29039)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#29039) should not be flagged as indented heading. Got: {result:?}"
        );

        // Hashtags should NOT be flagged (starts with lowercase)
        let content = "Some text\n  #hashtag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#hashtag should not be flagged as indented heading. Got: {result:?}"
        );

        // A `#` run with no space after it is paragraph text in CommonMark,
        // whatever follows it, so there is no heading to move left
        for content in [
            "Some text\n  #Summary",
            "Some text\n  ##introduction",
            "Some text\n  ##123",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "{content:?} is not a heading. Got: {result:?}");
        }

        // The same line with its space is an indented heading
        let content = "Some text\n\n  # Summary";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "indented `# Summary` is flagged. Got: {result:?}");

        // Properly aligned headings should pass
        let content = "# Summary\n## Details";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Properly aligned headings should pass. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_admonition_indented_heading_not_flagged() {
        // A heading intentionally indented to stay nested
        // inside a MkDocs admonition body must not be flagged, since the
        // indentation is required for it to belong to the admonition.
        let rule = MD023HeadingStartLeft;
        let content = "!!! note\n\n    # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "heading nested in an admonition body should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_content_tab_indented_heading_not_flagged() {
        // The same false positive occurs inside a MkDocs content tab body.
        let rule = MD023HeadingStartLeft;
        let content = "=== \"Tab A\"\n\n    # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "heading nested in a content tab body should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_accidental_indent_still_flagged() {
        // Control: a heading indented outside any admonition/tab (accidental
        // indentation, not container nesting) must still be flagged under
        // MkDocs flavor, and its fix must still de-indent it.
        let rule = MD023HeadingStartLeft;
        let content = "Some text\n\n  # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "accidentally indented top-level heading should still be flagged, got: {result:?}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Some text\n\n# Foo", "fix should still de-indent it");
    }

    #[test]
    fn test_standard_flavor_indented_heading_still_flagged_and_fixed() {
        // Control: standard flavor has no MkDocs container concept, so an
        // indented heading is still an accidental indent and must still be
        // flagged and de-indented by the fix. Uses 3 spaces: at 4+ spaces
        // CommonMark parses the line as an indented code block rather than a
        // heading at all, which is unrelated to this rule's guard.
        let rule = MD023HeadingStartLeft;
        let content = "Some text\n\n   # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "indented heading should still be flagged under standard flavor, got: {result:?}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Some text\n\n# Foo", "fix should still de-indent it");
    }

    #[test]
    fn test_html_markdown_div_indented_heading_still_flagged() {
        // A markdown="1" HTML div is tag-scoped, not indentation-scoped:
        // content needs no indentation to belong to it, so an indented
        // heading inside one is an accidental indent in every flavor.
        let rule = MD023HeadingStartLeft;
        let content = "<div markdown=\"1\">\n\n  # Bar\n\n</div>";
        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                1,
                "indented heading inside a markdown=\"1\" div must stay flagged under {flavor:?}, got: {result:?}"
            );
        }
    }
}

/// A heading written inside a list item is indented by the item's own content
/// column. Only the indentation past that column is the heading's, and removing
/// the rest moves the heading out of the item.
#[cfg(test)]
mod list_item_tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// The warnings for `content` and the document after the rule's fix.
    fn run(content: &str) -> (Vec<LintWarning>, String) {
        let rule = MD023HeadingStartLeft;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();
        (warnings, fixed)
    }

    /// Content the rule leaves alone: no warning and no change.
    fn assert_clean(content: &str) {
        let (warnings, fixed) = run(content);
        assert!(warnings.is_empty(), "{content:?} warned: {warnings:?}");
        assert_eq!(fixed, content, "{content:?} was rewritten");
    }

    /// Content the rule flags once per entry of `lines`, fixed to `expected`,
    /// and that stays fixed when fixed again.
    fn assert_fixed(content: &str, lines: &[usize], expected: &str) {
        let (warnings, fixed) = run(content);
        assert_eq!(
            warnings.iter().map(|warning| warning.line).collect::<Vec<_>>(),
            lines,
            "{content:?}: {warnings:?}"
        );
        assert_eq!(fixed, expected, "{content:?}");
        assert_clean(expected);
    }

    #[test]
    fn a_heading_at_its_items_content_column_is_not_indented() {
        for content in [
            "-\n  ## a\n  text\n",
            "- x\n\n  ## a\n\n  text\n",
            "- x\n  ## a\n  text\n",
            "1. x\n\n   ## a\n",
            "10. x\n\n    ## a\n",
            "- x\n  - y\n\n    ## a\n",
            "-\n  -\n    ## a\n",
            "- x\n\n  a\n  ---\n",
            "1. x\n\n   a\n   ===\n",
            "-\n  a\n  ---\n  text\n",
            "-\tx\n\n    ## a\n",
            "-     code\n\n  ## a\n",
        ] {
            assert_clean(content);
        }
    }

    #[test]
    fn only_the_indentation_past_the_content_column_is_removed() {
        assert_fixed("- x\n\n    ## a\n", &[3], "- x\n\n  ## a\n");
        assert_fixed("- x\n  - y\n\n      ## a\n", &[4], "- x\n  - y\n\n    ## a\n");
        assert_fixed("1. x\n\n    ## a\n", &[3], "1. x\n\n   ## a\n");
        assert_fixed("- x\n\n    a\n    ---\n", &[3, 4], "- x\n\n  a\n  ---\n");
        assert_fixed("- x\n\n    a\n    b\n    ===\n", &[3, 4, 5], "- x\n\n  a\n  b\n  ===\n");
    }

    #[test]
    fn the_item_is_found_by_what_the_line_re_enters() {
        // Text between the item and the heading does not end the item.
        assert_fixed("- x\n  text\n    ## a\n", &[3], "- x\n  text\n  ## a\n");
        // A lazy continuation line holds the heading in the item.
        assert_clean("- x\nlazy\n  ## a\n");
        assert_clean("- x\n\n  a\nlazy\n  ---\n");
        // An item that opened empty is closed by the blank line, and the heading
        // belongs to the item around it, whose margin it keeps.
        assert_fixed("-\n  -\n\n    ## a\n", &[4], "-\n  -\n\n  ## a\n");
        // A paragraph that re-enters only the outer item closes the inner one.
        assert_fixed(
            "- x\n  - y\n\n  paragraph\n\n    ## a\n",
            &[6],
            "- x\n  - y\n\n  paragraph\n\n  ## a\n",
        );
    }

    #[test]
    fn a_tab_in_the_indentation_is_reported_but_not_rewritten() {
        let content = "- x\n\n\t## a\n";
        let (warnings, fixed) = run(content);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].fix.is_none(), "{:?}", warnings[0].fix);
        assert_eq!(fixed, content);
        // Two tabs reach column 8, six past the item's edge: indented code.
        assert_clean("- x\n\n\t\t## a\n");
    }

    #[test]
    fn a_heading_outside_every_item_is_still_moved_to_the_margin() {
        assert_fixed("  ## a\n", &[1], "## a\n");
        assert_fixed("a\n\n  b\n  ---\n", &[3, 4], "a\n\nb\n---\n");
        // Written under the content column of its item, a heading is outside it.
        assert_fixed("1. x\n\n  ## a\n", &[3], "1. x\n\n## a\n");
        assert_fixed("- x\n\n ## a\n", &[3], "- x\n\n## a\n");
        assert_fixed("- x\n\nplain\n\n  ## a\n", &[5], "- x\n\nplain\n\n## a\n");
        assert_fixed("-\n\n  ## a\n", &[3], "-\n\n## a\n");
    }

    #[test]
    fn deep_continuation_lines_of_setext_text_are_reported_but_not_rewritten() {
        // The fence line is text of the heading; unindented it would open a fence.
        for content in ["- x\n\n  a\n      ```\n  ---\n", "a\n    ```\n---\n"] {
            let (warnings, fixed) = run(content);
            assert!(!warnings.is_empty(), "{content:?}");
            assert_eq!(fixed, content, "{content:?}");
        }
        // The shallow continuation line and the underline are still fixed.
        let (_, fixed) = run("- x\n\n  a\n    b\n  ---\n");
        assert_eq!(fixed, "- x\n\n  a\n  b\n  ---\n");
    }
}
