use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;

/// Rule MD071: Blank line after frontmatter
///
/// Ensures there is a blank line after YAML/TOML/JSON frontmatter.
/// This improves readability and prevents issues with some markdown parsers.
///
/// See [docs/md071.md](../../docs/md071.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD071BlankLineAfterFrontmatter;

impl MD071BlankLineAfterFrontmatter {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD071BlankLineAfterFrontmatter {
    fn name(&self) -> &'static str {
        "MD071"
    }

    fn description(&self) -> &'static str {
        "Blank line after frontmatter"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        if content.is_empty() {
            return Ok(warnings);
        }

        let fm_end_line = FrontMatterUtils::get_front_matter_end_line(content);
        if fm_end_line == 0 {
            // No frontmatter
            return Ok(warnings);
        }

        let lines: Vec<&str> = content.lines().collect();

        // fm_end_line is 1-indexed, so the line after frontmatter is at index fm_end_line
        if let Some(next_line) = lines.get(fm_end_line)
            && !next_line.trim().is_empty()
        {
            // Missing blank line after frontmatter
            let end_col = lines.get(fm_end_line - 1).map_or(1, |l| l.len() + 1);
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: "Missing blank line after frontmatter".to_string(),
                line: fm_end_line, // Report on the closing delimiter line
                column: 1,
                end_line: fm_end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: ctx.line_index.line_col_to_byte_range(fm_end_line, end_col),
                    replacement: "\n".to_string(),
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let warnings = self.check(ctx)?;

        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let fm_end_line = FrontMatterUtils::get_front_matter_end_line(content);
        if fm_end_line == 0 {
            return Ok(content.to_string());
        }

        // Check if original content ended with newline
        let had_trailing_newline = content.ends_with('\n');

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            result.push((*line).to_string());

            // Insert blank line after frontmatter closing delimiter (index fm_end_line - 1)
            if i == fm_end_line - 1
                && let Some(next_line) = lines.get(i + 1)
                && !next_line.trim().is_empty()
            {
                result.push(String::new());
            }
        }

        let fixed = result.join("\n");

        // Preserve original trailing newline if it existed
        let final_result = if had_trailing_newline && !fixed.ends_with('\n') {
            format!("{fixed}\n")
        } else {
            fixed
        };

        Ok(final_result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD071BlankLineAfterFrontmatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    // ==================== Basic Tests ====================

    #[test]
    fn test_no_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "# Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_frontmatter_with_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_frontmatter_without_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Missing blank line"));
    }

    #[test]
    fn test_toml_frontmatter_without_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "+++\ntitle = \"Test\"\n+++\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_json_frontmatter_without_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "{\n\"title\": \"Test\"\n}\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_adds_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "---\ntitle: Test\n---\n\n# Heading\n\nContent.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_idempotent() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_once = rule.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        assert_eq!(fixed_once, fixed_twice);
    }

    #[test]
    fn test_frontmatter_at_end_of_file() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // No content after frontmatter, no warning needed
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_blank_lines_ok() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_frontmatter_with_text_immediately_after() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\nSome paragraph text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_whitespace_only_line_after_frontmatter_is_not_blank() {
        // A line with only spaces is NOT a blank line
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n   \n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Whitespace-only line should be treated as blank (trim().is_empty())
        assert!(result.is_empty());
    }

    #[test]
    fn test_tab_only_line_after_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n\t\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tab-only line should be treated as blank
        assert!(result.is_empty());
    }

    #[test]
    fn test_crlf_line_endings() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\r\ntitle: Test\r\n---\r\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect missing blank line with CRLF
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_crlf_with_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\r\ntitle: Test\r\n---\r\n\r\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_yaml_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Empty frontmatter still needs blank line after
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_yaml_frontmatter_with_blank_line() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_frontmatter_with_blank_lines_inside() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n\nauthor: John\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Blank lines inside frontmatter don't affect the rule
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_frontmatter_trailing_whitespace_on_delimiter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---   \n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Trailing whitespace on delimiter should still trigger
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_frontmatter_only_file() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Only frontmatter\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Trailing newline only, no actual content - no warning needed
        assert!(result.is_empty());
    }

    #[test]
    fn test_frontmatter_with_triple_dash_inside_value() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: \"Test --- with dashes\"\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The dashes inside the value shouldn't affect parsing
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_preserves_content_after_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# Heading\n\nParagraph 1.\n\nParagraph 2.\n\n- List item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify content is preserved
        assert!(fixed.contains("# Heading"));
        assert!(fixed.contains("Paragraph 1."));
        assert!(fixed.contains("Paragraph 2."));
        assert!(fixed.contains("- List item"));
        // Verify blank line was added
        assert!(fixed.contains("---\n\n#"));
    }

    #[test]
    fn test_fix_toml_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "+++\ntitle = \"Test\"\n+++\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("+++\n\n#"));
    }

    #[test]
    fn test_fix_json_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "{\n\"title\": \"Test\"\n}\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("}\n\n#"));
    }

    #[test]
    fn test_multiline_yaml_values() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ndescription: |\n  This is a\n  multiline value\ntitle: Test\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_list_values() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntags:\n  - rust\n  - markdown\ntitle: Test\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_unicode_content_after_frontmatter() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# 日本語の見出し";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# 日本語の見出し"));
    }

    #[test]
    fn test_fix_multiple_applications_still_idempotent() {
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ntitle: Test\n---\n# Heading";

        // Apply fix 5 times
        let mut current = content.to_string();
        for _ in 0..5 {
            let ctx = LintContext::new(&current, crate::config::MarkdownFlavor::Standard, None);
            current = rule.fix(&ctx).unwrap();
        }

        // Should only have one blank line
        assert_eq!(current.matches("\n\n").count(), 1);
        assert!(current.contains("---\n\n#"));
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD071BlankLineAfterFrontmatter;
        // Content WITH trailing newline
        let content = "---\ndate: 2026-01-06\n---\n# Title\n\nSome text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
        assert_eq!(fixed, "---\ndate: 2026-01-06\n---\n\n# Title\n\nSome text.\n");
    }

    #[test]
    fn test_fix_no_trailing_newline() {
        let rule = MD071BlankLineAfterFrontmatter;
        // Content WITHOUT trailing newline
        let content = "---\ntitle: Test\n---\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
    }

    #[test]
    fn test_fix_does_not_cause_md047() {
        // Regression test for issue #262
        let rule = MD071BlankLineAfterFrontmatter;
        let content = "---\ndate: 2026-01-06\n---\n# Title\n\nSome text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // First check MD071
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Should detect missing blank line");

        // Fix it
        let fixed = rule.fix(&ctx).unwrap();

        // The fixed content should still end with a single newline
        assert!(fixed.ends_with('\n'), "Should preserve trailing newline");
        assert!(!fixed.ends_with("\n\n"), "Should not end with multiple newlines");

        // Verify MD071 is now clean
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap();
        assert!(warnings2.is_empty(), "MD071 should be satisfied after fix");
    }
}
