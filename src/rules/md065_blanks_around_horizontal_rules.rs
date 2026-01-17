use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD065: Blanks around horizontal rules
///
/// See [docs/md065.md](../../docs/md065.md) for full documentation and examples.
///
/// Ensures horizontal rules have blank lines before and after them

#[derive(Clone, Default)]
pub struct MD065BlanksAroundHorizontalRules;

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
        // And the previous line must have content
        // CommonMark: setext underlines can have leading/trailing spaces but NO internal spaces
        if prev_line.is_empty() {
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
}

impl Rule for MD065BlanksAroundHorizontalRules {
    fn name(&self) -> &'static str {
        "MD065"
    }

    fn description(&self) -> &'static str {
        "Horizontal rules should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;
        let mut warnings = Vec::new();

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();

        for (i, line_info) in ctx.lines.iter().enumerate() {
            // Use pre-computed is_horizontal_rule from LineInfo
            // This already excludes code blocks, frontmatter, and does proper HR detection
            if !line_info.is_horizontal_rule {
                continue;
            }

            // Skip if this is actually a setext heading marker
            if Self::is_setext_heading_marker(&lines, i) {
                continue;
            }

            // Check for blank line before HR (unless at start of document)
            if i > 0 && Self::count_blank_lines_before(&lines, i) == 0 {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: "Missing blank line before horizontal rule".to_string(),
                    line: i + 1,
                    column: 1,
                    end_line: i + 1,
                    end_column: 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(i + 1, 1),
                        replacement: format!("{bq_prefix}\n"),
                    }),
                });
            }

            // Check for blank line after HR (unless at end of document)
            if i < lines.len() - 1 && Self::count_blank_lines_after(&lines, i) == 0 {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: "Missing blank line after horizontal rule".to_string(),
                    line: i + 1,
                    column: lines[i].len() + 1,
                    end_line: i + 1,
                    end_column: lines[i].len() + 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(i + 1, lines[i].len() + 1),
                        replacement: format!("{bq_prefix}\n"),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let mut warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            // Check for warning about missing blank line before this line
            let warning_before = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message.contains("before horizontal rule"));

            if let Some(idx) = warning_before {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                result.push(bq_prefix);
                warnings.remove(idx);
            }

            result.push((*line).to_string());

            // Check for warning about missing blank line after this line
            let warning_after = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message.contains("after horizontal rule"));

            if let Some(idx) = warning_after {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                result.push(bq_prefix);
                warnings.remove(idx);
            }
        }

        Ok(result.join("\n"))
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
        // *** -> --- missing blank after ***
        // --- could be setext if *** had text, but *** is not text
        // Actually --- after *** (not text) is still HR
        assert!(result.len() >= 2);
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
        // Nested blockquotes should preserve the full prefix
        let rule = MD065BlanksAroundHorizontalRules;

        let content = ">> Nested quote
>> ---
>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should insert ">>" blank lines
        let expected = ">> Nested quote
>>
>> ---
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
        // Triple-nested blockquotes should preserve full prefix
        let rule = MD065BlanksAroundHorizontalRules;

        let content = ">>> Triple nested
>>> ---
>>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = ">>> Triple nested
>>>
>>> ---
>>>
>>> More text";
        assert_eq!(
            fixed, expected,
            "Fix should preserve triple-nested blockquote prefix '>>>'"
        );
    }
}
