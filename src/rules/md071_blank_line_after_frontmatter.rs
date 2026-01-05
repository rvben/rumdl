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
            let line_index = &ctx.line_index;
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: "Missing blank line after frontmatter".to_string(),
                line: fm_end_line, // Report on the closing delimiter line
                column: 1,
                end_line: fm_end_line,
                end_column: lines.get(fm_end_line - 1).map_or(1, |l| l.len() + 1),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index
                        .line_col_to_byte_range(fm_end_line, lines.get(fm_end_line - 1).map_or(1, |l| l.len() + 1)),
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

        Ok(result.join("\n"))
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
}
