mod md041_config;

pub use md041_config::MD041Config;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::range_utils::calculate_line_range;
use crate::utils::regex_cache::HTML_HEADING_PATTERN;
use regex::Regex;

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

        // Skip badge/shield images - common pattern at top of READMEs
        // Matches: ![badge](url) or [![badge](url)](url)
        if Self::is_badge_image_line(trimmed) {
            return true;
        }

        false
    }

    /// Check if a line consists only of badge/shield images
    /// Common patterns:
    /// - `![badge](url)`
    /// - `[![badge](url)](url)` (linked badge)
    /// - Multiple badges on one line
    fn is_badge_image_line(line: &str) -> bool {
        if line.is_empty() {
            return false;
        }

        // Must start with image syntax
        if !line.starts_with('!') && !line.starts_with('[') {
            return false;
        }

        // Check if line contains only image/link patterns and whitespace
        let mut remaining = line;
        while !remaining.is_empty() {
            remaining = remaining.trim_start();
            if remaining.is_empty() {
                break;
            }

            // Linked image: [![alt](img-url)](link-url)
            if remaining.starts_with("[![") {
                if let Some(end) = Self::find_linked_image_end(remaining) {
                    remaining = &remaining[end..];
                    continue;
                }
                return false;
            }

            // Simple image: ![alt](url)
            if remaining.starts_with("![") {
                if let Some(end) = Self::find_image_end(remaining) {
                    remaining = &remaining[end..];
                    continue;
                }
                return false;
            }

            // Not an image pattern
            return false;
        }

        true
    }

    /// Find the end of an image pattern ![alt](url)
    fn find_image_end(s: &str) -> Option<usize> {
        if !s.starts_with("![") {
            return None;
        }
        // Find ]( after ![
        let alt_end = s[2..].find("](")?;
        let paren_start = 2 + alt_end + 2; // Position after ](
        // Find closing )
        let paren_end = s[paren_start..].find(')')?;
        Some(paren_start + paren_end + 1)
    }

    /// Find the end of a linked image pattern [![alt](img-url)](link-url)
    fn find_linked_image_end(s: &str) -> Option<usize> {
        if !s.starts_with("[![") {
            return None;
        }
        // Find the inner image first
        let inner_end = Self::find_image_end(&s[1..])?;
        let after_inner = 1 + inner_end;
        // Should be followed by ](url)
        if !s[after_inner..].starts_with("](") {
            return None;
        }
        let link_start = after_inner + 2;
        let link_end = s[link_start..].find(')')?;
        Some(link_start + link_end + 1)
    }

    /// Check if a line is an HTML heading using the centralized HTML parser
    fn is_html_heading(ctx: &crate::lint_context::LintContext, first_line_idx: usize, level: usize) -> bool {
        // Check for single-line HTML heading using regex (fast path)
        let first_line_content = ctx.lines[first_line_idx].content(ctx.content);
        if let Ok(Some(captures)) = HTML_HEADING_PATTERN.captures(first_line_content.trim())
            && let Some(h_level) = captures.get(1)
            && h_level.as_str().parse::<usize>().unwrap_or(0) == level
        {
            return true;
        }

        // Use centralized HTML parser for multi-line headings
        let html_tags = ctx.html_tags();
        let target_tag = format!("h{level}");

        // Find opening tag on first line
        let opening_index = html_tags.iter().position(|tag| {
            tag.line == first_line_idx + 1 // HtmlTag uses 1-indexed lines
                && tag.tag_name == target_tag
                && !tag.is_closing
        });

        let Some(open_idx) = opening_index else {
            return false;
        };

        // Walk HTML tags to find the corresponding closing tag, allowing arbitrary nesting depth.
        // This avoids brittle line-count heuristics and handles long headings with nested content.
        let mut depth = 1usize;
        for tag in html_tags.iter().skip(open_idx + 1) {
            // Ignore tags that appear before the first heading line (possible when multiple tags share a line)
            if tag.line <= first_line_idx + 1 {
                continue;
            }

            if tag.tag_name == target_tag {
                if tag.is_closing {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                } else if !tag.is_self_closing {
                    depth += 1;
                }
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
        let mut warnings = Vec::new();

        // Check if we should skip this file
        if self.should_skip(ctx) {
            return Ok(warnings);
        }

        // Find the first non-blank line after front matter using cached info
        let mut first_content_line_num = None;
        let mut skip_lines = 0;

        // Check for front matter
        if ctx.lines.first().map(|l| l.content(ctx.content).trim()) == Some("---") {
            // Skip front matter
            for (idx, line_info) in ctx.lines.iter().enumerate().skip(1) {
                if line_info.content(ctx.content).trim() == "---" {
                    skip_lines = idx + 1;
                    break;
                }
            }
        }

        for (line_num, line_info) in ctx.lines.iter().enumerate().skip(skip_lines) {
            let line_content = line_info.content(ctx.content).trim();
            // Skip ESM blocks in MDX files (import/export statements)
            if line_info.in_esm_block {
                continue;
            }
            // Skip HTML comments - they are non-visible and should not affect MD041
            if line_info.in_html_comment {
                continue;
            }
            if !line_content.is_empty() && !Self::is_non_content_line(line_info.content(ctx.content)) {
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
            // Check for HTML heading (both single-line and multi-line)
            Self::is_html_heading(ctx, first_line_idx, self.level)
        };

        if !is_correct_heading {
            // Calculate precise character range for the entire first line
            let first_line = first_line_idx + 1; // Convert to 1-indexed
            let first_line_content = first_line_info.content(ctx.content);
            let (start_line, start_col, end_line, end_col) = calculate_line_range(first_line, first_line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("First line in file should be a level {} heading", self.level),
                severity: Severity::Warning,
                fix: None, // MD041 no longer provides auto-fix suggestions
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD041 should not auto-fix - adding content/titles is a decision that should be made by the document author
        // This rule now only detects and warns about missing titles, but does not automatically add them
        Ok(ctx.content.to_string())
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip files that are purely preprocessor directives (e.g., mdBook includes).
        // These files are composition/routing metadata, not standalone content.
        // Example: A file containing only "{{#include ../../README.md}}" is a
        // pointer to content, not content itself, and shouldn't need a heading.
        let only_directives = !ctx.content.is_empty()
            && ctx.content.lines().filter(|l| !l.trim().is_empty()).all(|l| {
                let t = l.trim();
                // mdBook directives: {{#include}}, {{#playground}}, {{#rustdoc_include}}, etc.
                (t.starts_with("{{#") && t.ends_with("}}"))
                        // HTML comments often accompany directives
                        || (t.starts_with("<!--") && t.ends_with("-->"))
            });

        ctx.content.is_empty()
            || (self.front_matter_title && self.has_front_matter_title(ctx.content))
            || only_directives
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Load config using serde with kebab-case support
        let md041_config = crate::rule_config_serde::load_rule_config::<MD041Config>(config);

        let use_front_matter = !md041_config.front_matter_title.is_empty();

        Box::new(MD041FirstLineHeading::with_pattern(
            md041_config.level.as_usize(),
            use_front_matter,
            md041_config.front_matter_title_pattern,
        ))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        Some((
            "MD041".to_string(),
            toml::toml! {
                level = 1
                front-matter-title = "title"
                front-matter-title-pattern = ""
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when empty lines precede a valid heading"
        );

        // Empty lines before non-heading content (should fail)
        let content = "\n\nNot a heading\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6); // First content line after front matter
    }

    #[test]
    fn test_front_matter_disabled() {
        let rule = MD041FirstLineHeading::new(1, false);

        // Front matter with title field but front_matter_title is false (should fail)
        let content = "---\ntitle: My Document\n---\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5); // First content line after front matter
    }

    #[test]
    fn test_html_comments_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment before heading (should pass - comments are skipped, issue #155)
        let content = "<!-- This is a comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_multiline_html_comment_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML comment before heading (should pass - issue #155)
        let content = "<!--\nThis is a multi-line\nHTML comment\n-->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML comments should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_html_comment_with_blank_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment with blank lines before heading (should pass - issue #155)
        let content = "<!-- This is a comment -->\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments with blank lines should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_html_comment_before_html_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment before HTML heading (should pass - issue #155)
        let content = "<!-- This is a comment -->\n<h1>My Document</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped before HTML headings"
        );
    }

    #[test]
    fn test_document_with_only_html_comments() {
        let rule = MD041FirstLineHeading::default();

        // Document with only HTML comments (should pass - no warnings for comment-only files)
        let content = "<!-- This is a comment -->\n<!-- Another comment -->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Documents with only HTML comments should not trigger MD041"
        );
    }

    #[test]
    fn test_html_comment_followed_by_non_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment followed by non-heading content (should still fail - issue #155)
        let content = "<!-- This is a comment -->\nThis is not a heading\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "HTML comment followed by non-heading should still trigger MD041"
        );
        assert_eq!(
            result[0].line, 2,
            "Warning should be on the first non-comment, non-heading line"
        );
    }

    #[test]
    fn test_multiple_html_comments_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multiple HTML comments before heading (should pass - issue #155)
        let content = "<!-- First comment -->\n<!-- Second comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multiple HTML comments should all be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_with_wrong_level_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment followed by wrong-level heading (should fail - issue #155)
        let content = "<!-- This is a comment -->\n## Wrong Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "HTML comment followed by wrong-level heading should still trigger MD041"
        );
        assert!(
            result[0].message.contains("level 1 heading"),
            "Should require level 1 heading"
        );
    }

    #[test]
    fn test_html_comment_mixed_with_reference_definitions() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment mixed with reference definitions before heading (should pass - issue #155)
        let content = "<!-- Comment -->\n[ref]: https://example.com\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments and reference definitions should both be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_after_front_matter() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment after front matter, before heading (should pass - issue #155)
        let content = "---\nauthor: John\n---\n<!-- Comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments after front matter should be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_not_at_start_should_not_affect_rule() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment in middle of document should not affect MD041 check
        let content = "# Valid Heading\n\nSome content.\n\n<!-- Comment in middle -->\n\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments in middle of document should not affect MD041 (only first content matters)"
        );
    }

    #[test]
    fn test_multiline_html_comment_followed_by_non_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML comment followed by non-heading (should still fail - issue #155)
        let content = "<!--\nMulti-line\ncomment\n-->\nThis is not a heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Multi-line HTML comment followed by non-heading should still trigger MD041"
        );
        assert_eq!(
            result[0].line, 5,
            "Warning should be on the first non-comment, non-heading line"
        );
    }

    #[test]
    fn test_different_heading_levels() {
        // Test with level 2 requirement
        let rule = MD041FirstLineHeading::new(2, false);

        let content = "## Second Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for correct level 2 heading");

        // Wrong level
        let content = "# First Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 2 heading"));
    }

    #[test]
    fn test_setext_headings() {
        let rule = MD041FirstLineHeading::default();

        // Setext style level 1 heading (should pass)
        let content = "My Document\n===========\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for setext level 1 heading");

        // Setext style level 2 heading (should fail with level 1 requirement)
        let content = "My Document\n-----------\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_empty_document() {
        let rule = MD041FirstLineHeading::default();

        // Empty document (should pass - no warnings)
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for empty document");
    }

    #[test]
    fn test_whitespace_only_document() {
        let rule = MD041FirstLineHeading::default();

        // Document with only whitespace (should pass - no warnings)
        let content = "   \n\n   \t\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for whitespace-only document");
    }

    #[test]
    fn test_front_matter_then_whitespace() {
        let rule = MD041FirstLineHeading::default();

        // Front matter followed by only whitespace (should pass - no warnings)
        let content = "---\ntitle: Test\n---\n\n   \n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));

        // JSON front matter with title (should fail - doesn't have "title:" pattern, has "\"title\":")
        let content = "{\n\"title\": \"My Document\"\n}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));

        // YAML front matter with title field (standard case)
        let content = "---\ntitle: My Document\n---\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for YAML front matter with title"
        );

        // Test mixed format edge case - YAML-style in TOML
        let content = "+++\ntitle: My Document\n+++\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings when title: pattern is found");
    }

    #[test]
    fn test_malformed_front_matter() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Malformed front matter with title
        let content = "- --\ntitle: My Document\n- --\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when first line after front matter is correct heading"
        );
    }

    #[test]
    fn test_no_fix_suggestion() {
        let rule = MD041FirstLineHeading::default();

        // Check that NO fix suggestion is provided (MD041 is now detection-only)
        let content = "Not a heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_none(), "MD041 should not provide fix suggestions");
    }

    #[test]
    fn test_complex_document_structure() {
        let rule = MD041FirstLineHeading::default();

        // Complex document with various elements - HTML comment should be skipped (issue #155)
        let content =
            "---\nauthor: John\n---\n\n<!-- Comment -->\n\n\n# Valid Heading\n\n## Subheading\n\nContent here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped, so first heading after comment should be valid"
        );
    }

    #[test]
    fn test_heading_with_special_characters() {
        let rule = MD041FirstLineHeading::default();

        // Heading with special characters and formatting
        let content = "# Welcome to **My** _Document_ with `code`\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for correct level {level} heading"
            );

            // Wrong level
            let wrong_level = if level == 1 { 2 } else { 1 };
            let content = format!("{} Wrong Level Heading\n\nContent.", "#".repeat(wrong_level));
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result[0].message.contains(&format!("level {level} heading")));
        }
    }

    #[test]
    fn test_issue_152_multiline_html_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML h1 heading (should pass - issue #152)
        let content = "<h1>\nSome text\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #152: Multi-line HTML h1 should be recognized as valid heading"
        );
    }

    #[test]
    fn test_multiline_html_heading_with_attributes() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading with attributes
        let content = "<h1 class=\"title\" id=\"main\">\nHeading Text\n</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading with attributes should be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_wrong_level() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML h2 heading (should fail with level 1 requirement)
        let content = "<h2>\nSome text\n</h2>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_multiline_html_heading_with_content_after() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading followed by content
        let content = "<h1>\nMy Document\n</h1>\n\nThis is the document content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading followed by content should be valid"
        );
    }

    #[test]
    fn test_multiline_html_heading_incomplete() {
        let rule = MD041FirstLineHeading::default();

        // Incomplete multi-line HTML heading (missing closing tag)
        let content = "<h1>\nSome text\n\nMore content without closing tag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_singleline_html_heading_still_works() {
        let rule = MD041FirstLineHeading::default();

        // Single-line HTML heading should still work
        let content = "<h1>My Document</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Single-line HTML headings should still be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_with_nested_tags() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading with nested tags
        let content = "<h1>\n<strong>Bold</strong> Heading\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading with nested tags should be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_various_levels() {
        // Test multi-line headings at different levels
        for level in 1..=6 {
            let rule = MD041FirstLineHeading::new(level, false);

            // Correct level multi-line
            let content = format!("<h{level}>\nHeading Text\n</h{level}>\n\nContent.");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Multi-line HTML heading at level {level} should be recognized"
            );

            // Wrong level multi-line
            let wrong_level = if level == 1 { 2 } else { 1 };
            let content = format!("<h{wrong_level}>\nHeading Text\n</h{wrong_level}>\n\nContent.");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result[0].message.contains(&format!("level {level} heading")));
        }
    }

    #[test]
    fn test_issue_152_nested_heading_spans_many_lines() {
        let rule = MD041FirstLineHeading::default();

        let content = "<h1>\n  <div>\n    <img\n      href=\"https://example.com/image.png\"\n      alt=\"Example Image\"\n    />\n    <a\n      href=\"https://example.com\"\n    >Example Project</a>\n    <span>Documentation</span>\n  </div>\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Nested multi-line HTML heading should be recognized");
    }

    #[test]
    fn test_issue_152_picture_tag_heading() {
        let rule = MD041FirstLineHeading::default();

        let content = "<h1>\n  <picture>\n    <source\n      srcset=\"https://example.com/light.png\"\n      media=\"(prefers-color-scheme: light)\"\n    />\n    <source\n      srcset=\"https://example.com/dark.png\"\n      media=\"(prefers-color-scheme: dark)\"\n    />\n    <img src=\"https://example.com/default.png\" />\n  </picture>\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Picture tag inside multi-line HTML heading should be recognized"
        );
    }

    #[test]
    fn test_badge_images_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Single badge before heading
        let content = "![badge](https://img.shields.io/badge/test-passing-green)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Badge image should be skipped");

        // Multiple badges on one line
        let content = "![badge1](url1) ![badge2](url2)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Multiple badges should be skipped");

        // Linked badge (clickable)
        let content = "[![badge](https://img.shields.io/badge/test-pass-green)](https://example.com)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Linked badge should be skipped");
    }

    #[test]
    fn test_multiple_badge_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multiple lines of badges
        let content = "[![Crates.io](https://img.shields.io/crates/v/example)](https://crates.io)\n[![docs.rs](https://img.shields.io/docsrs/example)](https://docs.rs)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Multiple badge lines should be skipped");
    }

    #[test]
    fn test_badges_without_heading_still_warns() {
        let rule = MD041FirstLineHeading::default();

        // Badges followed by paragraph (not heading)
        let content = "![badge](url)\n\nThis is not a heading.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should warn when badges followed by non-heading");
    }

    #[test]
    fn test_mixed_content_not_badge_line() {
        let rule = MD041FirstLineHeading::default();

        // Image with text is not a badge line
        let content = "![badge](url) Some text here\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Mixed content line should not be skipped");
    }

    #[test]
    fn test_is_badge_image_line_unit() {
        // Unit tests for is_badge_image_line
        assert!(MD041FirstLineHeading::is_badge_image_line("![badge](url)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("[![badge](img)](link)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("![a](b) ![c](d)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("[![a](b)](c) [![d](e)](f)"));

        // Not badge lines
        assert!(!MD041FirstLineHeading::is_badge_image_line(""));
        assert!(!MD041FirstLineHeading::is_badge_image_line("Some text"));
        assert!(!MD041FirstLineHeading::is_badge_image_line("![badge](url) text"));
        assert!(!MD041FirstLineHeading::is_badge_image_line("# Heading"));
    }
}
