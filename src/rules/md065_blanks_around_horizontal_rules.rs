use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD065: Blanks around horizontal rules
///
/// See [docs/md065.md](../../docs/md065.md) for full documentation and examples.
///
/// Ensures horizontal rules have blank lines before and after them

#[derive(Clone, Default)]
pub struct MD065BlanksAroundHorizontalRules;

impl MD065BlanksAroundHorizontalRules {
    /// Check if a line is blank
    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Check if a line is a horizontal rule (---, ***, ___)
    fn is_horizontal_rule(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.len() < 3 {
            return false;
        }

        // Check for patterns like ---, ***, ___ (with optional spaces between)
        let chars: Vec<char> = trimmed.chars().collect();
        let first_non_space = chars.iter().find(|&&c| c != ' ');

        if let Some(&marker) = first_non_space {
            if marker != '-' && marker != '*' && marker != '_' {
                return false;
            }

            // Count marker characters (ignoring spaces)
            let marker_count = chars.iter().filter(|&&c| c == marker).count();
            let other_count = chars.iter().filter(|&&c| c != marker && c != ' ').count();

            // Must have at least 3 markers and only spaces otherwise
            marker_count >= 3 && other_count == 0
        } else {
            false
        }
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
        !prev_line.is_empty()
            && (line.chars().all(|c| c == '-' || c == ' ') || line.chars().all(|c| c == '=' || c == ' '))
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

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or front matter
            if let Some(line_info) = ctx.lines.get(i)
                && (line_info.in_code_block || line_info.in_front_matter)
            {
                continue;
            }

            if !Self::is_horizontal_rule(line) {
                continue;
            }

            // Skip if this is actually a setext heading marker
            if Self::is_setext_heading_marker(&lines, i) {
                continue;
            }

            // Check for blank line before HR (unless at start of document)
            if i > 0 && Self::count_blank_lines_before(&lines, i) == 0 {
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
                        replacement: "\n".to_string(),
                    }),
                });
            }

            // Check for blank line after HR (unless at end of document)
            if i < lines.len() - 1 && Self::count_blank_lines_after(&lines, i) == 0 {
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
                        replacement: "\n".to_string(),
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
                result.push("".to_string());
                warnings.remove(idx);
            }

            result.push((*line).to_string());

            // Check for warning about missing blank line after this line
            let warning_after = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message.contains("after horizontal rule"));

            if let Some(idx) = warning_after {
                result.push("".to_string());
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
        // Valid horizontal rules
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("---"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("----"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("***"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("****"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("___"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("____"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("- - -"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("* * *"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("_ _ _"));
        assert!(MD065BlanksAroundHorizontalRules::is_horizontal_rule("  ---  "));

        // Invalid horizontal rules
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("--"));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("**"));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("__"));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("- -"));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("text"));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule(""));
        assert!(!MD065BlanksAroundHorizontalRules::is_horizontal_rule("==="));
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
}
