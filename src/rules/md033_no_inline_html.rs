//!
//! Rule MD033: No inline HTML
//!
//! See [docs/md033.md](../../docs/md033.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_html_tag_range;
use crate::utils::regex_cache::*;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // HTML/Markdown comment pattern (specific to MD033)
    static ref HTML_COMMENT_PATTERN: Regex = Regex::new(r"<!--.*?-->").unwrap();
}

#[derive(Clone)]
pub struct MD033NoInlineHtml {
    allowed: HashSet<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        Self::new()
    }
}

impl MD033NoInlineHtml {
    pub fn new() -> Self {
        Self {
            allowed: HashSet::new(),
        }
    }

    pub fn with_allowed(allowed_vec: Vec<String>) -> Self {
        // Store allowed tags in lowercase for case-insensitive matching
        Self {
            allowed: allowed_vec.into_iter().map(|s| s.to_lowercase()).collect(),
        }
    }

    // Efficient check for allowed tags using HashSet (case-insensitive)
    #[inline]
    fn is_tag_allowed(&self, tag: &str) -> bool {
        if self.allowed.is_empty() {
            return false;
        }
        // Remove angle brackets and slashes, then split by whitespace or '>'
        let tag = tag.trim_start_matches('<').trim_start_matches('/');
        let tag_name = tag
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");
        self.allowed.contains(&tag_name.to_lowercase())
    }

    // Check if a tag is an HTML comment
    #[inline]
    fn is_html_comment(&self, tag: &str) -> bool {
        tag.starts_with("<!--") && tag.ends_with("-->")
    }

    // Check if a tag is likely a programming type annotation rather than HTML
    #[inline]
    fn is_likely_type_annotation(&self, tag: &str) -> bool {
        // Common programming type names that are often used in generics
        const COMMON_TYPES: &[&str] = &[
            "string",
            "number",
            "any",
            "void",
            "null",
            "undefined",
            "array",
            "promise",
            "function",
            "error",
            "date",
            "regexp",
            "symbol",
            "bigint",
            "map",
            "set",
            "weakmap",
            "weakset",
            "iterator",
            "generator",
            "t",
            "u",
            "v",
            "k",
            "e", // Common single-letter type parameters
            "userdata",
            "apiresponse",
            "config",
            "options",
            "params",
            "result",
            "response",
            "request",
            "data",
            "item",
            "element",
            "node",
        ];

        let tag_content = tag
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim_start_matches('/');
        let tag_name = tag_content
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");

        // Check if it's a simple tag (no attributes) with a common type name
        if !tag_content.contains(' ') && !tag_content.contains('=') {
            COMMON_TYPES.contains(&tag_name.to_ascii_lowercase().as_str())
        } else {
            false
        }
    }

    // Check if a tag is actually an email address in angle brackets
    #[inline]
    fn is_email_address(&self, tag: &str) -> bool {
        let content = tag.trim_start_matches('<').trim_end_matches('>');
        // Simple email pattern: contains @ and has reasonable structure
        content.contains('@')
            && content
                .chars()
                .all(|c| c.is_alphanumeric() || "@.-_+".contains(c))
            && content.split('@').count() == 2
            && content.split('@').all(|part| !part.is_empty())
    }

    // Check if a tag is actually a URL in angle brackets
    #[inline]
    fn is_url_in_angle_brackets(&self, tag: &str) -> bool {
        let content = tag.trim_start_matches('<').trim_end_matches('>');
        // Check for common URL schemes
        content.starts_with("http://")
            || content.starts_with("https://")
            || content.starts_with("ftp://")
            || content.starts_with("ftps://")
            || content.starts_with("mailto:")
    }

    /// Find HTML tags that span multiple lines
    fn find_multiline_html_tags(
        &self,
        content: &str,
        structure: &DocumentStructure,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Early return: if content has no incomplete tags at line ends, skip processing
        if !content.contains('<') || !content.lines().any(|line| line.trim_end().ends_with('<')) {
            return;
        }

        // Simple approach: use regex to find patterns like <tagname and then look for closing >
        lazy_static::lazy_static! {
            static ref INCOMPLETE_TAG_START: regex::Regex = regex::Regex::new(r"(?i)<[a-zA-Z][^>]*$").unwrap();
        }

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;

            // Skip code blocks and empty lines
            if line.trim().is_empty() || structure.is_in_code_block(line_num) {
                continue;
            }

            // Early return: skip lines that don't end with incomplete tags
            if !line.contains('<') {
                continue;
            }

            // Look for incomplete HTML tags at the end of the line
            if let Some(incomplete_match) = INCOMPLETE_TAG_START.find(line) {
                let start_column = incomplete_match.start() + 1; // 1-indexed

                // Build the complete tag by looking at subsequent lines
                let mut complete_tag = incomplete_match.as_str().to_string();
                let mut found_end = false;

                // Look for the closing > in subsequent lines (limit search to 10 lines)
                for (j, next_line) in lines.iter().enumerate().skip(i + 1).take(10) {
                    let next_line_num = j + 1;

                    // Stop if we hit a code block
                    if structure.is_in_code_block(next_line_num) {
                        break;
                    }

                    complete_tag.push(' '); // Add space to normalize whitespace
                    complete_tag.push_str(next_line.trim());

                    if next_line.contains('>') {
                        found_end = true;
                        break;
                    }
                }

                if found_end {
                    // Extract just the tag part (up to the first >)
                    if let Some(end_pos) = complete_tag.find('>') {
                        let final_tag = &complete_tag[0..=end_pos];

                        // Apply the same filters as single-line tags
                        if !self.is_html_comment(final_tag)
                            && !self.is_likely_type_annotation(final_tag)
                            && !self.is_email_address(final_tag)
                            && !self.is_url_in_angle_brackets(final_tag)
                            && !self.is_tag_allowed(final_tag)
                            && HTML_TAG_FINDER.is_match(final_tag)
                        {
                            // Check for duplicates (avoid flagging the same position twice)
                            let already_warned = warnings
                                .iter()
                                .any(|w| w.line == line_num && w.column == start_column);

                            if !already_warned {
                                let (start_line, start_col, end_line, end_col) =
                                    calculate_html_tag_range(
                                        line_num,
                                        line,
                                        incomplete_match.start(),
                                        incomplete_match.len(),
                                    );
                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    line: start_line,
                                    column: start_col,
                                    end_line,
                                    end_column: end_col,
                                    message: format!("Inline HTML found (use Markdown syntax instead)"),
                                    severity: Severity::Warning,
                                    fix: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Rule for MD033NoInlineHtml {
    fn name(&self) -> &'static str {
        "MD033"
    }

    fn description(&self) -> &'static str {
        "Inline HTML is not allowed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return: if no HTML tags at all, skip processing
        if content.is_empty() || !has_html_tags(content) {
            return Ok(Vec::new());
        }

        // Quick check for HTML tag pattern before expensive processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // First pass: find single-line HTML tags
        // To match markdownlint behavior, report one warning per HTML tag
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;

            if line.trim().is_empty() {
                continue;
            }
            if structure.is_in_code_block(line_num) {
                continue;
            }

            // Find all HTML tags in the line using regex
            for tag_match in HTML_TAG_FINDER.find_iter(line) {
                let tag = tag_match.as_str();

                // Skip HTML comments
                if self.is_html_comment(tag) {
                    continue;
                }

                // Skip likely programming type annotations
                if self.is_likely_type_annotation(tag) {
                    continue;
                }

                // Skip email addresses in angle brackets
                if self.is_email_address(tag) {
                    continue;
                }

                // Skip URLs in angle brackets
                if self.is_url_in_angle_brackets(tag) {
                    continue;
                }

                // Skip tags inside code spans
                let tag_start_col = tag_match.start() + 1; // 1-indexed
                if structure.is_in_code_span(line_num, tag_start_col) {
                    continue;
                }

                // Skip allowed tags
                if self.is_tag_allowed(tag) {
                    continue;
                }

                // Report each HTML tag individually (true markdownlint compatibility)
                let (start_line, start_col, end_line, end_col) =
                    calculate_html_tag_range(line_num, line, tag_match.start(), tag_match.len());
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Inline HTML found (use Markdown syntax instead)"),
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }

        // Second pass: find multi-line HTML tags
        self.find_multiline_html_tags(ctx.content, structure, &mut warnings);

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No fix for MD033: do not remove or alter HTML, just return the input unchanged
        Ok(ctx.content.to_string())
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Html
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !has_html_tags(content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let allowed_vec: Vec<toml::Value> = self
            .allowed
            .iter()
            .cloned()
            .map(toml::Value::String)
            .collect();
        let mut map = toml::map::Map::new();
        map.insert("allowed".to_string(), toml::Value::Array(allowed_vec));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let allowed_vec = crate::config::get_rule_config_value::<Vec<String>>(
            config,
            "MD033",
            "allowed_elements",
        )
        .unwrap_or_default();
        // Convert Vec to HashSet for the struct field
        let allowed: HashSet<String> = allowed_vec.into_iter().map(|s| s.to_lowercase()).collect();
        Box::new(MD033NoInlineHtml { allowed })
    }
}

impl DocumentStructureExtensions for MD033NoInlineHtml {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // Rule is only relevant if content contains potential HTML tags
        ctx.content.contains('<') && ctx.content.contains('>')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_md033_basic_html() {
        let rule = MD033NoInlineHtml::default();
        let content = "<div>Some content</div>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Reports one warning per HTML tag (true markdownlint compatibility)
        assert_eq!(result.len(), 2); // <div> and </div>
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[1].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_case_insensitive() {
        let rule = MD033NoInlineHtml::default();
        let content = "<DiV>Some <B>content</B></dIv>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Reports one warning per HTML tag (true markdownlint compatibility)
        assert_eq!(result.len(), 4); // <DiV>, <B>, </B>, </dIv>
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[1].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[2].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[3].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_allowed_tags() {
        let rule = MD033NoInlineHtml::with_allowed(vec!["div".to_string(), "br".to_string()]);
        let content = "<div>Allowed</div><p>Not allowed</p><br/>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Only warnings for non-allowed tags (<p> and </p>, div and br are allowed)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[1].message, "Inline HTML found (use Markdown syntax instead)");

        // Test case-insensitivity of allowed tags
        let content2 = "<DIV>Allowed</DIV><P>Not allowed</P><BR/>";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 2); // <P> and </P> flagged
        assert_eq!(result2[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result2[1].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_html_comments() {
        let rule = MD033NoInlineHtml::default();
        let content = "<!-- This is a comment --> <p>Not a comment</p>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should detect warnings for HTML tags (comments are skipped)
        assert_eq!(result.len(), 2); // <p> and </p>
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[1].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_tags_in_links() {
        let rule = MD033NoInlineHtml::default();
        let content = "[Link](http://example.com/<div>)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // The <div> in the URL should be detected as HTML (not skipped)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");

        let content2 = "[Link <a>text</a>](url)";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.check(&ctx2).unwrap();
        // Reports one warning per HTML tag (true markdownlint compatibility)
        assert_eq!(result2.len(), 2); // <a> and </a>
        assert_eq!(result2[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result2[1].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_fix_escaping() {
        let rule = MD033NoInlineHtml::default();
        let content = "Text with <div> and <br/> tags.";
        let ctx = LintContext::new(content);
        let fixed_content = rule.fix(&ctx).unwrap();
        // No fix for HTML tags; output should be unchanged
        assert_eq!(fixed_content, content);
    }

    #[test]
    fn test_md033_in_code_blocks() {
        let rule = MD033NoInlineHtml::default();
        let content = "```html\n<div>Code</div>\n```\n<div>Not code</div>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Reports one warning per HTML tag (true markdownlint compatibility)
        assert_eq!(result.len(), 2); // <div> and </div> outside code block
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
        assert_eq!(result[1].message, "Inline HTML found (use Markdown syntax instead)");
    }

    #[test]
    fn test_md033_in_code_spans() {
        let rule = MD033NoInlineHtml::default();
        let content = "Text with `<p>in code</p>` span. <br/> Not in span.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should detect <br/> outside code span, but not tags inside code span
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Inline HTML found (use Markdown syntax instead)");
    }
}
