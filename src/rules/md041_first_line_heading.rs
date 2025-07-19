use crate::utils::range_utils::{LineIndex, calculate_line_range};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern for HTML heading tags
    static ref HTML_HEADING_PATTERN: FancyRegex = FancyRegex::new(r"^\s*<h([1-6])(?:\s[^>]*)?>.*</h\1>\s*$").unwrap();
}

/// Rule MD041: First line in file should be a top-level heading
///
/// See [docs/md041.md](../../docs/md041.md) for full documentation, configuration, and examples.

#[derive(Clone)]
pub struct MD041FirstLineHeading {
    pub level: usize,
    pub front_matter_title: bool,
    pub front_matter_title_pattern: Option<Regex>,
}

impl Default for MD041FirstLineHeading {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: true,
            front_matter_title_pattern: None,
        }
    }
}

impl MD041FirstLineHeading {
    pub fn new(level: usize, front_matter_title: bool) -> Self {
        Self {
            level,
            front_matter_title,
            front_matter_title_pattern: None,
        }
    }

    pub fn with_pattern(level: usize, front_matter_title: bool, pattern: Option<String>) -> Self {
        let front_matter_title_pattern = pattern.and_then(|p| match Regex::new(&p) {
            Ok(regex) => Some(regex),
            Err(e) => {
                log::warn!("Invalid front_matter_title_pattern regex: {e}");
                None
            }
        });

        Self {
            level,
            front_matter_title,
            front_matter_title_pattern,
        }
    }

    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        // If we have a custom pattern, use it to search front matter content
        if let Some(ref pattern) = self.front_matter_title_pattern {
            let front_matter_lines = FrontMatterUtils::extract_front_matter(content);
            for line in front_matter_lines {
                if pattern.is_match(line) {
                    return true;
                }
            }
            return false;
        }

        // Default behavior: check for "title:" field
        FrontMatterUtils::has_front_matter_field(content, "title:")
    }

    /// Check if a line is a non-content token that should be skipped
    fn is_non_content_line(line: &str) -> bool {
        let trimmed = line.trim();

        // Skip reference definitions
        if trimmed.starts_with('[') && trimmed.contains("]: ") {
            return true;
        }

        // Skip abbreviation definitions
        if trimmed.starts_with('*') && trimmed.contains("]: ") {
            return true;
        }

        false
    }

    /// Check if a line is an HTML heading
    fn is_html_heading(line: &str, level: usize) -> bool {
        if let Ok(Some(captures)) = HTML_HEADING_PATTERN.captures(line.trim()) {
            if let Some(h_level) = captures.get(1) {
                return h_level.as_str().parse::<usize>().unwrap_or(0) == level;
            }
        }
        false
    }
}

impl Rule for MD041FirstLineHeading {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top level heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        if content.trim().is_empty() {
            return Ok(warnings);
        }
        if self.has_front_matter_title(content) {
            return Ok(warnings);
        }

        // Find the first non-blank line after front matter using cached info
        let mut first_content_line_num = None;
        let mut skip_lines = 0;

        // Check for front matter
        if ctx.lines.first().map(|l| l.content.trim()) == Some("---") {
            // Skip front matter
            for (idx, line_info) in ctx.lines.iter().enumerate().skip(1) {
                if line_info.content.trim() == "---" {
                    skip_lines = idx + 1;
                    break;
                }
            }
        }

        for (line_num, line_info) in ctx.lines.iter().enumerate().skip(skip_lines) {
            let line_content = line_info.content.trim();
            if !line_content.is_empty() && !Self::is_non_content_line(&line_info.content) {
                first_content_line_num = Some(line_num);
                break;
            }
        }

        if first_content_line_num.is_none() {
            // No non-blank lines after front matter
            return Ok(warnings);
        }

        let first_line_idx = first_content_line_num.unwrap();

        // Check if the first non-blank line is a heading of the required level
        let first_line_info = &ctx.lines[first_line_idx];
        let is_correct_heading = if let Some(heading) = &first_line_info.heading {
            heading.level as usize == self.level
        } else {
            // Check for HTML heading
            Self::is_html_heading(&first_line_info.content, self.level)
        };

        if !is_correct_heading {
            // Calculate precise character range for the entire first line
            let first_line = first_line_idx + 1; // Convert to 1-indexed
            let first_line_content = &first_line_info.content;
            let (start_line, start_col, end_line, end_col) = calculate_line_range(first_line, first_line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("First line in file should be a level {} heading", self.level),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: LineIndex::new(content.to_string()).line_col_to_byte_range_with_length(first_line, 1, 0),
                    replacement: format!("{} Title\n\n", "#".repeat(self.level)),
                }),
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let content = crate::rules::front_matter_utils::FrontMatterUtils::fix_malformed_front_matter(content);
        if content.trim().is_empty() || self.has_front_matter_title(&content) {
            return Ok(content.to_string());
        }

        // Re-create context for the potentially fixed content
        let fixed_ctx = crate::lint_context::LintContext::new(&content);

        // Find the first non-blank line after front matter
        let mut first_content_line_num = None;
        let mut skip_lines = 0;

        // Check for front matter
        if fixed_ctx.lines.first().map(|l| l.content.trim()) == Some("---") {
            // Skip front matter
            for (idx, line_info) in fixed_ctx.lines.iter().enumerate().skip(1) {
                if line_info.content.trim() == "---" {
                    skip_lines = idx + 1;
                    break;
                }
            }
        }

        for (line_num, line_info) in fixed_ctx.lines.iter().enumerate().skip(skip_lines) {
            let line_content = line_info.content.trim();
            if !line_content.is_empty() && !Self::is_non_content_line(&line_info.content) {
                first_content_line_num = Some(line_num);
                break;
            }
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        // Check if we have any headings at all
        let has_any_heading = fixed_ctx.lines.iter().any(|line| line.heading.is_some());

        if !has_any_heading {
            // Add a new title at the beginning
            result.push_str(&format!("{} Title\n\n{}", "#".repeat(self.level), content));
        } else if let Some(first_line_idx) = first_content_line_num {
            // Check if first content line is a heading of correct level
            let first_line_info = &fixed_ctx.lines[first_line_idx];

            if let Some(heading) = &first_line_info.heading {
                if heading.level as usize != self.level {
                    // Fix the existing heading level
                    for (i, line) in lines.iter().enumerate() {
                        if i == first_line_idx {
                            result.push_str(&format!("{} {}", "#".repeat(self.level), heading.text));
                        } else {
                            result.push_str(line);
                        }
                        if i < lines.len() - 1 {
                            result.push('\n');
                        }
                    }
                } else {
                    // No fix needed, return original
                    return Ok(content.to_string());
                }
            } else if Self::is_html_heading(&first_line_info.content, self.level) {
                // HTML heading with correct level, no fix needed
                return Ok(content.to_string());
            } else {
                // First line is not a heading, add a new title before it
                for (i, line) in lines.iter().enumerate() {
                    if i == first_line_idx {
                        result.push_str(&format!("{} Title\n\n", "#".repeat(self.level)));
                    }
                    result.push_str(line);
                    if i < lines.len() - 1 {
                        result.push('\n');
                    }
                }
            }
        } else {
            // No content after front matter
            return Ok(content.to_string());
        }

        Ok(result)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (self.front_matter_title && self.has_front_matter_title(ctx.content))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let level = crate::config::get_rule_config_value::<u32>(config, "MD041", "level").unwrap_or(1);
        let front_matter_title = crate::config::get_rule_config_value::<String>(config, "MD041", "front_matter_title")
            .unwrap_or_else(|| "title".to_string());
        let front_matter_title_pattern =
            crate::config::get_rule_config_value::<String>(config, "MD041", "front_matter_title_pattern");

        let level_usize = level as usize;
        let use_front_matter = !front_matter_title.is_empty();

        Box::new(MD041FirstLineHeading::with_pattern(
            level_usize,
            use_front_matter,
            front_matter_title_pattern,
        ))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        Some((
            "MD041".to_string(),
            toml::toml! {
                level = 1
                // Pattern for matching title in front matter (regex)
                // front_matter_title_pattern = "^(title|header):"
            }
            .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_first_line_is_heading_correct_level() {
        let rule = MD041FirstLineHeading::default();

        // First line is a level 1 heading (should pass)
        let content = "# My Document\n\nSome content here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when first line is a level 1 heading"
        );
    }

    #[test]
    fn test_first_line_is_heading_wrong_level() {
        let rule = MD041FirstLineHeading::default();

        // First line is a level 2 heading (should fail with level 1 requirement)
        let content = "## My Document\n\nSome content here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_first_line_not_heading() {
        let rule = MD041FirstLineHeading::default();

        // First line is plain text (should fail)
        let content = "This is not a heading\n\n# This is a heading";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_empty_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Empty lines before first heading (should pass - rule skips empty lines)
        let content = "\n\n# My Document\n\nSome content.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when empty lines precede a valid heading"
        );

        // Empty lines before non-heading content (should fail)
        let content = "\n\nNot a heading\n\nSome content.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3); // First non-empty line
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_front_matter_with_title() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Front matter with title field (should pass)
        let content = "---\ntitle: My Document\nauthor: John Doe\n---\n\nSome content here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when front matter has title field"
        );
    }

    #[test]
    fn test_front_matter_without_title() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Front matter without title field (should fail)
        let content = "---\nauthor: John Doe\ndate: 2024-01-01\n---\n\nSome content here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6); // First content line after front matter
    }

    #[test]
    fn test_front_matter_disabled() {
        let rule = MD041FirstLineHeading::new(1, false);

        // Front matter with title field but front_matter_title is false (should fail)
        let content = "---\ntitle: My Document\n---\n\nSome content here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5); // First content line after front matter
    }

    #[test]
    fn test_html_comments_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment before heading (should fail)
        let content = "<!-- This is a comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1); // HTML comment is the first line
    }

    #[test]
    fn test_different_heading_levels() {
        // Test with level 2 requirement
        let rule = MD041FirstLineHeading::new(2, false);

        let content = "## Second Level Heading\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for correct level 2 heading");

        // Wrong level
        let content = "# First Level Heading\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 2 heading"));
    }

    #[test]
    fn test_setext_headings() {
        let rule = MD041FirstLineHeading::default();

        // Setext style level 1 heading (should pass)
        let content = "My Document\n===========\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for setext level 1 heading");

        // Setext style level 2 heading (should fail with level 1 requirement)
        let content = "My Document\n-----------\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_empty_document() {
        let rule = MD041FirstLineHeading::default();

        // Empty document (should pass - no warnings)
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for empty document");
    }

    #[test]
    fn test_whitespace_only_document() {
        let rule = MD041FirstLineHeading::default();

        // Document with only whitespace (should pass - no warnings)
        let content = "   \n\n   \t\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for whitespace-only document");
    }

    #[test]
    fn test_front_matter_then_whitespace() {
        let rule = MD041FirstLineHeading::default();

        // Front matter followed by only whitespace (should pass - no warnings)
        let content = "---\ntitle: Test\n---\n\n   \n\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when no content after front matter"
        );
    }

    #[test]
    fn test_multiple_front_matter_types() {
        let rule = MD041FirstLineHeading::new(1, true);

        // TOML front matter with title (should fail - rule only checks for "title:" pattern)
        let content = "+++\ntitle = \"My Document\"\n+++\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));

        // JSON front matter with title (should fail - doesn't have "title:" pattern, has "\"title\":")
        let content = "{\n\"title\": \"My Document\"\n}\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));

        // YAML front matter with title field (standard case)
        let content = "---\ntitle: My Document\n---\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for YAML front matter with title"
        );

        // Test mixed format edge case - YAML-style in TOML
        let content = "+++\ntitle: My Document\n+++\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings when title: pattern is found");
    }

    #[test]
    fn test_malformed_front_matter() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Malformed front matter with title
        let content = "- --\ntitle: My Document\n- --\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for malformed front matter with title"
        );
    }

    #[test]
    fn test_front_matter_with_heading() {
        let rule = MD041FirstLineHeading::default();

        // Front matter without title field followed by correct heading
        let content = "---\nauthor: John Doe\n---\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when first line after front matter is correct heading"
        );
    }

    #[test]
    fn test_fix_suggestion() {
        let rule = MD041FirstLineHeading::default();

        // Check that fix suggestion is provided
        let content = "Not a heading\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_some());

        let fix = result[0].fix.as_ref().unwrap();
        assert!(fix.replacement.contains("# Title"));
    }

    #[test]
    fn test_complex_document_structure() {
        let rule = MD041FirstLineHeading::default();

        // Complex document with various elements
        let content =
            "---\nauthor: John\n---\n\n<!-- Comment -->\n\n\n# Valid Heading\n\n## Subheading\n\nContent here.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5); // The comment line
    }

    #[test]
    fn test_heading_with_special_characters() {
        let rule = MD041FirstLineHeading::default();

        // Heading with special characters and formatting
        let content = "# Welcome to **My** _Document_ with `code`\n\nContent.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for heading with inline formatting"
        );
    }

    #[test]
    fn test_level_configuration() {
        // Test various level configurations
        for level in 1..=6 {
            let rule = MD041FirstLineHeading::new(level, false);

            // Correct level
            let content = format!("{} Heading at Level {}\n\nContent.", "#".repeat(level), level);
            let ctx = LintContext::new(&content);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for correct level {level} heading"
            );

            // Wrong level
            let wrong_level = if level == 1 { 2 } else { 1 };
            let content = format!("{} Wrong Level Heading\n\nContent.", "#".repeat(wrong_level));
            let ctx = LintContext::new(&content);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result[0].message.contains(&format!("level {level} heading")));
        }
    }
}
