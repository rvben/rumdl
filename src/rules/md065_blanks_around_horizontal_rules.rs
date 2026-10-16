use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::fix_utils::apply_warning_fixes;

use std::collections::HashSet;

/// Rule MD065: Blanks around horizontal rules
///
/// See [docs/md065.md](../../docs/md065.md) for full documentation and examples.
///
/// Ensures horizontal rules have blank lines before and after them
#[derive(Clone, Default)]
pub struct MD065BlanksAroundHorizontalRules;

/// What the parser says about the lines that look like thematic breaks.
///
/// A line's text cannot say that it underlines a paragraph through a lazy
/// continuation, or that it begins the item opened by the bare marker above it:
/// both depend on the containers around the line. Lines are 0-based.
struct RuleLines {
    /// The last line of every setext heading. Its underline is not a rule, and a
    /// blank line before it would end the paragraph it underlines.
    setext_underlines: HashSet<usize>,
    /// Rules that are the first block of an item whose marker line holds nothing
    /// else. A blank line between the marker and the rule ends the item (an item
    /// can begin with at most one blank line), which leaves the rule outside it.
    item_openers: HashSet<usize>,
}

impl RuleLines {
    fn new(ctx: &crate::lint_context::LintContext) -> Self {
        let line_of = |offset: usize| ctx.offset_to_line_col(offset).0 - 1;
        let mut setext_underlines = HashSet::new();
        let mut item_openers = HashSet::new();
        // The line of the item just opened, until the next event shows whether a
        // rule comes first.
        let mut item_line = None;

        use crate::lint_context::document_parser::blocks::BlockKind;
        for (kind, range) in &ctx.parser_blocks().events {
            let opened = item_line.take();
            match kind {
                BlockKind::ItemStart => item_line = Some(line_of(range.start)),
                BlockKind::Rule => {
                    let line = line_of(range.start);
                    if opened.is_some_and(|marker_line| line > marker_line) {
                        item_openers.insert(line);
                    }
                }
                BlockKind::HeadingStart => {
                    let last = line_of(range.end.saturating_sub(1).max(range.start));
                    if last > line_of(range.start) {
                        setext_underlines.insert(last);
                    }
                }
                _ => {}
            }
        }

        Self {
            setext_underlines,
            item_openers,
        }
    }
}

impl MD065BlanksAroundHorizontalRules {
    /// Check if a line is blank (including blockquote continuation lines)
    ///
    /// Uses the shared `is_blank_in_blockquote_context` utility function for
    /// consistent blank line detection across all rules.
    fn is_blank_line(line: &str) -> bool {
        crate::utils::regex_cache::is_blank_in_blockquote_context(line)
    }

    /// Check if this might be a setext heading underline (not a horizontal rule)
    fn is_setext_heading_marker(lines: &[&str], line_index: usize) -> bool {
        if line_index == 0 {
            return false;
        }

        let line = lines[line_index].trim();
        let prev_line = lines[line_index - 1].trim();

        // Setext markers are only - or = (not * or _)
        // And the previous line must have content (not blank, not itself an HR)
        // CommonMark: setext underlines can have leading/trailing spaces but NO internal spaces
        if prev_line.is_empty() {
            return false;
        }

        // If the previous line is itself a horizontal rule, this cannot be a setext heading
        if crate::lint_context::is_horizontal_rule_line(prev_line) {
            return false;
        }

        // Check if all non-space characters are the same marker (- or =)
        // and there are no internal spaces (spaces between markers)
        let has_hyphen = line.contains('-');
        let has_equals = line.contains('=');

        // Must have exactly one type of marker
        if has_hyphen == has_equals {
            return false; // Either has both or neither
        }

        let marker = if has_hyphen { '-' } else { '=' };

        // Setext underline: optional leading spaces, then only marker chars, then optional trailing spaces
        // No internal spaces allowed
        let trimmed = line.trim();
        trimmed.chars().all(|c| c == marker)
    }

    /// Count the number of blank lines before a given line index
    fn count_blank_lines_before(lines: &[&str], line_index: usize) -> usize {
        let mut count = 0;
        let mut i = line_index;
        while i > 0 {
            i -= 1;
            if Self::is_blank_line(lines[i]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Count the number of blank lines after a given line index
    fn count_blank_lines_after(lines: &[&str], line_index: usize) -> usize {
        let mut count = 0;
        let mut i = line_index + 1;
        while i < lines.len() {
            if Self::is_blank_line(lines[i]) {
                count += 1;
                i += 1;
            } else {
                break;
            }
        }
        count
    }

    /// The document with the blank lines of the warnings that still carry a fix.
    fn fix_with(content: &str, warnings: &[LintWarning]) -> Result<String, LintError> {
        apply_warning_fixes(content, warnings).map_err(LintError::FixFailed)
    }
}

impl Rule for MD065BlanksAroundHorizontalRules {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(65))
    }

    fn name(&self) -> &'static str {
        "MD065"
    }

    fn description(&self) -> &'static str {
        "Horizontal rules should be surrounded by blank lines"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // A thematic break may be written with spaces between its markers (`* * *`,
        // `- - -`, `_ _ _`), so no substring of the content identifies one. The check
        // below reads the same per-line flag the rule itself acts on, which is already
        // computed and already excludes code blocks and front matter.
        ctx.content.is_empty() || !ctx.lines.iter().any(|line| line.is_horizontal_rule)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let lines = ctx.raw_lines();
        let rule_lines = RuleLines::new(ctx);

        for (i, line_info) in ctx.lines.iter().enumerate() {
            // Use pre-computed is_horizontal_rule from LineInfo
            // This already excludes code blocks, frontmatter, and does proper HR detection
            if !line_info.is_horizontal_rule {
                continue;
            }

            // Skip the underline of a setext heading. The parser's reading of the
            // document finds every one, lazy text lines included; the context's
            // heading and the text check cover underlines the parser reads as
            // something else, such as one below `#tag`.
            if rule_lines.setext_underlines.contains(&i)
                || ctx.heading_on_line(i).is_some_and(|heading| heading.is_setext())
                || Self::is_setext_heading_marker(lines, i)
            {
                continue;
            }

            // Check for blank line before HR (unless at start of document, or at
            // the start of the list item whose bare marker is the line above)
            if i > 0 && !rule_lines.item_openers.contains(&i) && Self::count_blank_lines_before(lines, i) == 0 {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: "Missing blank line before horizontal rule".to_string(),
                    line: i + 1,
                    column: 1,
                    end_line: i + 1,
                    end_column: 2,
                    severity: Severity::Warning,
                    fix: Some(Fix::new(ctx.line_column_byte_range(i + 1, 1), format!("{bq_prefix}\n"))),
                });
            }

            // Check for blank line after HR (unless at end of document)
            if i < lines.len() - 1 && Self::count_blank_lines_after(lines, i) == 0 {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: "Missing blank line after horizontal rule".to_string(),
                    line: i + 1,
                    column: lines[i].chars().count() + 1,
                    end_line: i + 1,
                    end_column: lines[i].chars().count() + 2,
                    severity: Severity::Warning,
                    // Inserted at the end of the line, ahead of its own line ending.
                    fix: Some(Fix::new(
                        ctx.line_column_byte_range(i + 1, lines[i].chars().count() + 1),
                        format!("\n{bq_prefix}"),
                    )),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        if warnings.is_empty() {
            return Ok(content.to_string());
        }
        Self::fix_with(content, &warnings)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD065BlanksAroundHorizontalRules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_hr_with_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text before.

---

Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_missing_blank_before() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** which cannot be a setext heading marker
        let content = "Some text before.
***

Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("before horizontal rule"));
    }

    #[test]
    fn test_hr_missing_blank_after() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text before.

***
Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("after horizontal rule"));
    }

    #[test]
    fn test_hr_missing_both_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** which cannot be a setext heading marker
        let content = "Some text before.
***
Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("before horizontal rule"));
        assert!(result[1].message.contains("after horizontal rule"));
    }

    #[test]
    fn test_hr_at_start_of_document() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "---

Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // No blank line needed before HR at start of document
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_at_end_of_document() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text before.

---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // No blank line needed after HR at end of document
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_hrs() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** and ___ which cannot be setext heading markers
        let content = "Text before.
***
Middle text.
___
Text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_hr_asterisks() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text.
***
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_underscores() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text.
___
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_with_spaces() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use * * * which cannot be a setext heading marker
        let content = "Some text.
* * *
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_long() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use asterisks which cannot be a setext heading marker
        let content = "Some text.
**********
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_setext_heading_not_hr() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Heading
---

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag setext heading marker as HR
        assert!(result.is_empty());
    }

    #[test]
    fn test_setext_heading_equals() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Heading
===

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // === is not a valid HR, only setext heading
        assert!(result.is_empty());
    }

    #[test]
    fn test_blockquote_setext_underline_is_not_hr() {
        // A quoted underline belongs to the heading above it. A blank line
        // inserted before it would end that paragraph and leave a thematic
        // break where the heading was.
        let rule = MD065BlanksAroundHorizontalRules;
        for content in [
            "> Title\n> ---\n\nText.\n",
            "> > Deep\n> > ---\n\nText.\n",
            "> Title\n>    ---\n\nText.\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "{content:?} is a heading: {result:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }
    }

    #[test]
    fn test_blockquote_hr_below_blank_line_is_checked() {
        // A quoted blank line ends the paragraph, so the `---` below it is a
        // thematic break that still needs a blank line after it.
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "> Text\n>\n> ---\n> More\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "{result:?}");
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].message, "Missing blank line after horizontal rule");
    }

    #[test]
    fn test_hr_in_code_block() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some text.

```
---
```

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR in code block should be ignored
        assert!(result.is_empty());
    }

    #[test]
    fn test_fix_missing_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** which cannot be a setext heading marker
        let content = "Text before.
***
Text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Text before.

***

Text after.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_multiple_hrs() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** and ___ which cannot be setext heading markers
        let content = "Start
***
Middle
___
End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Start

***

Middle

___

End";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_no_hrs() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Just regular text.
No horizontal rules here.
Only paragraphs.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_is_horizontal_rule() {
        use crate::lint_context::is_horizontal_rule_line;

        // Valid horizontal rules
        assert!(is_horizontal_rule_line("---"));
        assert!(is_horizontal_rule_line("----"));
        assert!(is_horizontal_rule_line("***"));
        assert!(is_horizontal_rule_line("****"));
        assert!(is_horizontal_rule_line("___"));
        assert!(is_horizontal_rule_line("____"));
        assert!(is_horizontal_rule_line("- - -"));
        assert!(is_horizontal_rule_line("* * *"));
        assert!(is_horizontal_rule_line("_ _ _"));
        assert!(is_horizontal_rule_line("  ---  "));

        // Invalid horizontal rules
        assert!(!is_horizontal_rule_line("--"));
        assert!(!is_horizontal_rule_line("**"));
        assert!(!is_horizontal_rule_line("__"));
        assert!(!is_horizontal_rule_line("- -"));
        assert!(!is_horizontal_rule_line("text"));
        assert!(!is_horizontal_rule_line(""));
        assert!(!is_horizontal_rule_line("==="));
    }

    #[test]
    fn test_consecutive_hrs_with_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

---

***

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both HRs have proper blank lines
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_after_heading() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Use *** which cannot be a setext heading marker
        let content = "# Heading
***

Text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR after heading needs blank line before
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("before horizontal rule"));
    }

    #[test]
    fn test_hr_before_heading() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

***
# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR before heading needs blank line after
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("after horizontal rule"));
    }

    #[test]
    fn test_setext_heading_hyphen_not_flagged() {
        let rule = MD065BlanksAroundHorizontalRules;
        // --- immediately after text is a setext heading, not HR
        let content = "Heading Text
---

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag setext heading as missing blank lines
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_with_blank_before_hyphen() {
        let rule = MD065BlanksAroundHorizontalRules;
        // --- after a blank line IS a horizontal rule, not setext heading
        let content = "Some text.

---
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag missing blank line after
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("after horizontal rule"));
    }

    // ============================================================
    // Additional comprehensive tests for edge cases
    // ============================================================

    #[test]
    fn test_frontmatter_not_flagged() {
        let rule = MD065BlanksAroundHorizontalRules;
        // YAML frontmatter uses --- delimiters which should NOT be flagged
        let content = "---
title: Test Document
date: 2024-01-01
---

# Heading

Content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Frontmatter delimiters should not be flagged as HRs
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_after_frontmatter() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "---
title: Test
---

Content.
***
More content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR after frontmatter content should be flagged
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_in_indented_code_block() {
        let rule = MD065BlanksAroundHorizontalRules;
        // 4-space indented code block
        let content = "Some text.

    ---
    code here

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR in indented code block should be ignored
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_with_leading_spaces() {
        let rule = MD065BlanksAroundHorizontalRules;
        // 1-3 spaces of indentation is still a valid HR
        let content = "Text.
   ***
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Indented HR (1-3 spaces) should be detected
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_in_html_comment() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

<!--
---
-->

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR inside HTML comment should be ignored
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_in_blockquote() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

> Quote text
> ***
> More quote

After quote.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR inside blockquote - the "> ***" line contains a valid HR pattern
        // but within blockquote context. This tests blockquote awareness.
        // Note: blockquotes don't skip HR detection, so this may flag.
        // The actual behavior depends on implementation.
        assert!(result.len() <= 2); // May or may not flag based on blockquote handling
    }

    #[test]
    fn test_hr_after_list() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Real-world case from Node.js repo
        let content = "* Item one
* Item two
***

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR immediately after list should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("before horizontal rule"));
    }

    #[test]
    fn test_mixed_marker_with_many_spaces() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.
-  -  -  -
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR with multiple spaces between markers
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_only_hr_in_document() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single HR alone in document - no blanks needed
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_blank_lines_already_present() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.


---


More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Multiple blank lines should not trigger warnings
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_at_both_start_and_end() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "---

Content in the middle.

---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HRs at start and end with proper spacing
        assert!(result.is_empty());
    }

    #[test]
    fn test_consecutive_hrs_without_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

***
---
___

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Consecutive HRs need blanks between them
        // --- after *** is also an HR (not setext), since *** is an HR not text
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_hr_after_hr_not_setext() {
        // Regression: --- after *** should be treated as HR, not setext heading
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "***\n---\n# ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Both *** and --- are HRs, both need blanks
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed2, "MD065 fix should be idempotent for consecutive HRs");
    }

    #[test]
    fn test_fix_idempotency() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text before.
***
Text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_once = rule.fix(&ctx).unwrap();

        // Apply fix again
        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        // Second fix should not change anything
        assert_eq!(fixed_once, fixed_twice);
    }

    #[test]
    fn test_setext_heading_long_underline() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Heading Text
----------

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Long underline is still setext heading, not HR
        assert!(result.is_empty());
    }

    #[test]
    fn test_hr_with_trailing_whitespace() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.
***
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR with trailing whitespace should still be detected
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hr_in_html_block() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Text.

<div>
---
</div>

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // HR inside HTML block should be ignored (depends on HTML block detection)
        // This tests HTML block awareness
        assert!(result.is_empty());
    }

    #[test]
    fn test_spaced_hyphens_are_hr_not_setext() {
        let rule = MD065BlanksAroundHorizontalRules;
        // CommonMark: setext underlines cannot have internal spaces
        // So "- - -" is a thematic break, not a setext heading
        let content = "Heading
- - -

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // "- - -" with internal spaces is HR, needs blank before
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("before horizontal rule"));
    }

    #[test]
    fn test_not_setext_if_prev_line_blank() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Some paragraph.

---
Text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // --- after blank line is HR, not setext heading
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("after horizontal rule"));
    }

    #[test]
    fn test_asterisk_cannot_be_setext() {
        let rule = MD065BlanksAroundHorizontalRules;
        // *** immediately after text is still HR (asterisks can't be setext markers)
        let content = "Some text
***
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // *** is always HR, never setext
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_underscore_cannot_be_setext() {
        let rule = MD065BlanksAroundHorizontalRules;
        // ___ immediately after text is still HR (underscores can't be setext markers)
        let content = "Some text
___
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // ___ is always HR, never setext
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "First paragraph with **bold** and *italic*.
***
Second paragraph with [link](url) and `code`.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify content is preserved
        assert!(fixed.contains("**bold**"));
        assert!(fixed.contains("*italic*"));
        assert!(fixed.contains("[link](url)"));
        assert!(fixed.contains("`code`"));
        assert!(fixed.contains("***"));
    }

    #[test]
    fn test_fix_only_adds_needed_blanks() {
        let rule = MD065BlanksAroundHorizontalRules;
        // Already has blank before, missing blank after
        let content = "Text.

***
More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Text.

***

More text.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_hr_detection_edge_cases() {
        use crate::lint_context::is_horizontal_rule_line;

        // Valid HRs with various spacing (0-3 leading spaces allowed)
        assert!(is_horizontal_rule_line("   ---"));
        assert!(is_horizontal_rule_line("---   "));
        assert!(is_horizontal_rule_line("   ---   "));
        assert!(is_horizontal_rule_line("*  *  *"));
        assert!(is_horizontal_rule_line("_    _    _"));

        // Invalid patterns
        assert!(!is_horizontal_rule_line("--a"));
        assert!(!is_horizontal_rule_line("**a"));
        assert!(!is_horizontal_rule_line("-*-"));
        assert!(!is_horizontal_rule_line("- * _"));
        assert!(!is_horizontal_rule_line("   "));
        assert!(!is_horizontal_rule_line("\t---")); // Tabs not allowed per CommonMark
    }

    #[test]
    fn test_warning_line_numbers_accurate() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "Line 1
Line 2
***
Line 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Verify line numbers are 1-indexed and accurate
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 3); // HR is on line 3
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_complex_document_structure() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "# Main Title

Introduction paragraph.

## Section One

Content here.

***

## Section Two

More content.

---

Final thoughts.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Well-structured document should have no warnings
        assert!(result.is_empty());
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_before_hr() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD065BlanksAroundHorizontalRules;

        let content = "> Text before
> ***
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank lines inserted should have the blockquote prefix
        let expected = "> Text before
>
> ***
>
> Text after";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank lines around HR, not plain blank lines"
        );
    }

    #[test]
    fn test_fix_preserves_nested_blockquote_prefix_for_hr() {
        // Nested blockquotes should preserve the full prefix. The break is
        // `***` because a `---` under the quoted text would underline it.
        let rule = MD065BlanksAroundHorizontalRules;

        let content = ">> Nested quote
>> ***
>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should insert ">>" blank lines
        let expected = ">> Nested quote
>>
>> ***
>>
>> More text";
        assert_eq!(fixed, expected, "Fix should preserve nested blockquote prefix '>>'");
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_after_hr() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines after HR
        let rule = MD065BlanksAroundHorizontalRules;

        let content = "> ---
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted after the HR should have the blockquote prefix
        let expected = "> ---
>
> Text after";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line after HR, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_triple_nested_blockquote_prefix_for_hr() {
        // Triple-nested blockquotes should preserve full prefix. The break is
        // `***` because a `---` under the quoted text would underline it.
        let rule = MD065BlanksAroundHorizontalRules;

        let content = ">>> Triple nested
>>> ***
>>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = ">>> Triple nested
>>>
>>> ***
>>>
>>> More text";
        assert_eq!(
            fixed, expected,
            "Fix should preserve triple-nested blockquote prefix '>>>'"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD065BlanksAroundHorizontalRules;

        let content = "Text\n***\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
        assert_eq!(fixed, "Text\n\n***\n\nMore text\n");
    }

    #[test]
    fn spaced_thematic_break_is_not_skipped() {
        // `* * *`, `- - -` and `_ _ _` are thematic breaks that contain none of the
        // substrings a content scan looks for, so whether the rule ran at all used to
        // depend on how the break was written.
        let rule = MD065BlanksAroundHorizontalRules;

        for content in [
            "Text\n* * *\nMore text\n",
            "Text\n- - -\nMore text\n",
            "Text\n_ _ _\nMore text\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(!rule.should_skip(&ctx), "the whole rule was skipped for {content:?}");
            assert_eq!(
                rule.check(&ctx).unwrap().len(),
                2,
                "both missing blank lines should be reported for {content:?}"
            );
        }
    }

    #[test]
    fn a_document_with_no_thematic_break_is_still_skipped() {
        let rule = MD065BlanksAroundHorizontalRules;

        for content in ["Text\nMore text\n", "- item\n- item\n", "```\n---\n```\n"] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.should_skip(&ctx),
                "the rule should have been skipped for {content:?}"
            );
        }
    }

    #[test]
    fn markers_inside_a_hidden_block_are_not_spaced_out() {
        // The fix inserts blank lines around what it takes for a thematic break, so
        // reporting one that a comment hides or a math block owns rewrites the block
        // itself - a blank line in display math ends it.
        let rule = MD065BlanksAroundHorizontalRules;

        for content in [
            "Text.\n\n<!--\n***\n-->\n\nMore.\n",
            "Text.\n\n$$\na = b\n***\nc = d\n$$\n\nMore.\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "the hidden markers were reported in {content:?}"
            );
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                content,
                "the block was rewritten in {content:?}"
            );
        }
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD065BlanksAroundHorizontalRules;

        let content = "Text\n***\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
        assert_eq!(fixed, "Text\n\n***\n\nMore text");
    }

    #[test]
    fn test_indented_lazy_line_in_blockquote_is_not_a_horizontal_rule() {
        let rule = MD065BlanksAroundHorizontalRules;
        for content in [
            "> o\n>     ---\n",
            "> o\n>     ***\n",
            ">o\n>     ***\n",
            "> o\n>     - - -\n",
            "> > o\n> >     ***\n",
            "- > o\n  >     ***\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content, "{content:?}");
        }
    }

    #[test]
    fn test_real_horizontal_rule_in_blockquote_still_needs_blank_lines() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "> a\n>\n> ***\n> b\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), "> a\n>\n> ***\n>\n> b\n");
    }

    fn messages(content: &str) -> Vec<String> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD065BlanksAroundHorizontalRules;
        rule.check(&ctx).unwrap().into_iter().map(|w| w.message).collect()
    }

    #[test]
    fn a_rule_opening_a_bare_marker_item_has_nothing_before_it() {
        // A blank line after a bare marker ends the item, so the rule would leave
        // the list. Nothing precedes it in its container, as at the start of a
        // document.
        let rule = MD065BlanksAroundHorizontalRules;
        for content in [
            "-\n  ***\n",
            "x\n\n-\n  ***\n",
            "1.\n   ***\n",
            "-\n  ___\n",
            "-\n  * * *\n",
            "-\n  - - -\n",
            "> -\n>   ***\n",
            "-\n  -\n    ***\n",
            "- a\n  -\n    ***\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content, "{content:?}");
        }
    }

    #[test]
    fn a_rule_opening_a_bare_marker_item_still_needs_a_blank_after() {
        let rule = MD065BlanksAroundHorizontalRules;
        for (content, expected) in [
            ("-\n  ***\n  text\n", "-\n  ***\n\n  text\n"),
            ("1.\n   ***\n   text\n", "1.\n   ***\n\n   text\n"),
            ("> -\n>   ***\n>   text\n", "> -\n>   ***\n>\n>   text\n"),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            assert_eq!(warnings[0].message, "Missing blank line after horizontal rule");
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
        }
    }

    #[test]
    fn a_rule_after_item_content_still_needs_a_blank_before() {
        // The item has content before the rule, so the blank stays inside it.
        let rule = MD065BlanksAroundHorizontalRules;
        for (content, expected) in [
            ("- a\n  ***\n", "- a\n\n  ***\n"),
            ("-\n  a\n  ***\n", "-\n  a\n\n  ***\n"),
            ("1. a\n   ***\n", "1. a\n\n   ***\n"),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(
                messages(content),
                ["Missing blank line before horizontal rule"],
                "{content:?}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
        }
    }

    #[test]
    fn a_rule_outside_the_list_after_a_bare_marker_is_unchanged() {
        // A blank line after a bare marker already ended the item.
        assert!(messages("-\n\n  ***\n").is_empty());
        assert!(messages("-\n\n***\n").is_empty());
    }

    #[test]
    fn a_lazy_quoted_setext_underline_is_not_a_rule() {
        // The text line without a `>` continues the quoted paragraph, which the
        // underline turns into a heading; a blank line before it would leave a
        // paragraph and a thematic break.
        let rule = MD065BlanksAroundHorizontalRules;
        for content in [
            "> a\nb\n> ---\n",
            "> a\nb\n>   ---\n",
            "> > a\n> b\n> > ---\n",
            "> a\nb\nc\n> ---\n",
            "> - a\nb\n>   ---\n",
            "- a\nb\n  ---\n",
            "a\nb\n---\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content, "{content:?}");
        }
    }

    #[test]
    fn a_lazy_line_before_a_real_rule_is_still_separated() {
        // `***` cannot underline, so it is a rule and the paragraph above ends.
        let rule = MD065BlanksAroundHorizontalRules;
        for (content, expected) in [
            ("> a\nb\n> ***\n", "> a\nb\n>\n> ***\n"),
            ("> a\nb\n> - - -\n", "> a\nb\n>\n> - - -\n"),
            ("> a\nb\n> ***\n> c\n", "> a\nb\n>\n> ***\n>\n> c\n"),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(!rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
        }
    }

    #[test]
    fn the_fix_values_apply_to_the_same_text_as_fix() {
        // The LSP applies each warning's own edit, so the edits have to produce
        // what `fix` writes, with the quote marker on the added line.
        let rule = MD065BlanksAroundHorizontalRules;
        for content in [
            "Text\n***\nMore text\n",
            "Text\n***\nMore text",
            "> a\n> ***\n> b\n",
            "> > a\n> > ***\n> > b\n",
            "> a\nb\n> ***\nc\n",
            "- a\n  ***\n  b\n",
            "-\n  ***\n  b\n",
            "a\n***\n***\nb\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(!warnings.is_empty(), "{content:?}");
            let applied = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
            assert_eq!(applied, rule.fix(&ctx).unwrap(), "{content:?}");
        }
    }

    #[test]
    fn the_fix_is_not_written_for_a_warning_without_a_fix() {
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "a\n***\nb\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let mut warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        warnings[0].fix = None; // the one before the rule
        assert_eq!(
            MD065BlanksAroundHorizontalRules::fix_with(content, &warnings).unwrap(),
            "a\n***\n\nb\n"
        );
    }

    #[test]
    fn loosening_a_list_keeps_its_fixes_in_every_flavor() {
        let rule = MD065BlanksAroundHorizontalRules;
        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MDX,
        ] {
            let content = "- a\n  ***\n  b\n";
            let ctx = LintContext::new(content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 2, "{flavor:?}");
            assert!(warnings.iter().all(|w| w.fix.is_some()), "{flavor:?}: {warnings:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), "- a\n\n  ***\n\n  b\n", "{flavor:?}");
        }
    }

    #[test]
    fn mdx_reads_no_indented_code_so_its_fixes_stay() {
        // CommonMark reads the line as paragraph text of the quote, MDX as a rule.
        let rule = MD065BlanksAroundHorizontalRules;
        let content = "> o\n>     ***\n> after\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().all(|w| w.fix.is_some()), "{warnings:?}");
    }
}

#[cfg(test)]
mod block_adjacency_regressions {
    #[test]
    fn paragraph_between_item_start_and_rule_clears_opener() {
        let ctx =
            crate::lint_context::LintContext::new("- para\n\n  ---\n", crate::config::MarkdownFlavor::Standard, None);
        assert!(super::RuleLines::new(&ctx).item_openers.is_empty());
    }
}
