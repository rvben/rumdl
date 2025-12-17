//!
//! Rule MD033: No HTML tags
//!
//! See [docs/md033.md](../../docs/md033.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::kramdown_utils::{is_kramdown_block_attribute, is_kramdown_extension};
use crate::utils::regex_cache::*;
use std::collections::HashSet;

mod md033_config;
use md033_config::MD033Config;

#[derive(Clone)]
pub struct MD033NoInlineHtml {
    config: MD033Config,
    allowed: HashSet<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        let config = MD033Config::default();
        let allowed = config.allowed_set();
        Self { config, allowed }
    }
}

impl MD033NoInlineHtml {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allowed(allowed_vec: Vec<String>) -> Self {
        let config = MD033Config {
            allowed: allowed_vec.clone(),
        };
        let allowed = config.allowed_set();
        Self { config, allowed }
    }

    pub fn from_config_struct(config: MD033Config) -> Self {
        let allowed = config.allowed_set();
        Self { config, allowed }
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

    /// Check if a tag name is a valid HTML element or custom element.
    /// Returns false for placeholder syntax like `<NAME>`, `<resource>`, `<actual>`.
    ///
    /// Per HTML spec, custom elements must contain a hyphen (e.g., `<my-component>`).
    #[inline]
    fn is_html_element_or_custom(tag_name: &str) -> bool {
        const HTML_ELEMENTS: &[&str] = &[
            // Document structure
            "html",
            "head",
            "body",
            "title",
            "base",
            "link",
            "meta",
            "style",
            // Sections
            "article",
            "section",
            "nav",
            "aside",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "hgroup",
            "header",
            "footer",
            "address",
            "main",
            "search",
            // Grouping
            "p",
            "hr",
            "pre",
            "blockquote",
            "ol",
            "ul",
            "menu",
            "li",
            "dl",
            "dt",
            "dd",
            "figure",
            "figcaption",
            "div",
            // Text-level
            "a",
            "em",
            "strong",
            "small",
            "s",
            "cite",
            "q",
            "dfn",
            "abbr",
            "ruby",
            "rt",
            "rp",
            "data",
            "time",
            "code",
            "var",
            "samp",
            "kbd",
            "sub",
            "sup",
            "i",
            "b",
            "u",
            "mark",
            "bdi",
            "bdo",
            "span",
            "br",
            "wbr",
            // Edits
            "ins",
            "del",
            // Embedded
            "picture",
            "source",
            "img",
            "iframe",
            "embed",
            "object",
            "param",
            "video",
            "audio",
            "track",
            "map",
            "area",
            "svg",
            "math",
            "canvas",
            // Tables
            "table",
            "caption",
            "colgroup",
            "col",
            "tbody",
            "thead",
            "tfoot",
            "tr",
            "td",
            "th",
            // Forms
            "form",
            "label",
            "input",
            "button",
            "select",
            "datalist",
            "optgroup",
            "option",
            "textarea",
            "output",
            "progress",
            "meter",
            "fieldset",
            "legend",
            // Interactive
            "details",
            "summary",
            "dialog",
            // Scripting
            "script",
            "noscript",
            "template",
            "slot",
            // Deprecated but recognized
            "acronym",
            "applet",
            "basefont",
            "big",
            "center",
            "dir",
            "font",
            "frame",
            "frameset",
            "isindex",
            "noframes",
            "strike",
            "tt",
        ];

        let lower = tag_name.to_ascii_lowercase();
        if HTML_ELEMENTS.contains(&lower.as_str()) {
            return true;
        }
        // Custom elements must contain a hyphen per HTML spec
        tag_name.contains('-')
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
            && content.chars().all(|c| c.is_alphanumeric() || "@.-_+".contains(c))
            && content.split('@').count() == 2
            && content.split('@').all(|part| !part.is_empty())
    }

    // Check if a tag has the markdown attribute (MkDocs/Material for MkDocs)
    #[inline]
    fn has_markdown_attribute(&self, tag: &str) -> bool {
        // Check for various forms of markdown attribute
        // Examples: <div markdown>, <div markdown="1">, <div class="result" markdown>
        tag.contains(" markdown>") || tag.contains(" markdown=") || tag.contains(" markdown ")
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

    /// Calculate fix to remove HTML tags while keeping content
    ///
    /// For self-closing tags like `<br/>`, returns a single fix to remove the tag.
    /// For paired tags like `<span>text</span>`, returns the replacement text (just the content).
    ///
    /// Returns (range, replacement_text) where range is the bytes to replace
    /// and replacement_text is what to put there (content without tags, or empty for self-closing).
    fn calculate_fix(
        &self,
        content: &str,
        opening_tag: &str,
        tag_byte_start: usize,
    ) -> Option<(std::ops::Range<usize>, String)> {
        // Check if it's a self-closing tag (ends with />)
        if opening_tag.ends_with("/>") {
            return Some((tag_byte_start..tag_byte_start + opening_tag.len(), String::new()));
        }

        // Extract tag name from opening tag (e.g., "<div>" -> "div", "<span class='x'>" -> "span")
        let tag_name = opening_tag
            .trim_start_matches('<')
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()?
            .to_lowercase();

        // Build the closing tag pattern
        let closing_tag = format!("</{tag_name}>");

        // Search for the closing tag after the opening tag
        let search_start = tag_byte_start + opening_tag.len();
        if let Some(closing_pos) = content[search_start..].find(&closing_tag) {
            let closing_byte_start = search_start + closing_pos;
            let closing_byte_end = closing_byte_start + closing_tag.len();

            // Extract the content between tags
            let inner_content = &content[search_start..closing_byte_start];

            return Some((tag_byte_start..closing_byte_end, inner_content.to_string()));
        }

        // If no closing tag found, just remove the opening tag
        Some((tag_byte_start..tag_byte_start + opening_tag.len(), String::new()))
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

        // Early return: if no HTML tags at all, skip processing
        if content.is_empty() || !ctx.likely_has_html() {
            return Ok(Vec::new());
        }

        // Quick check for HTML tag pattern before expensive processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track nomarkdown and comment blocks (Kramdown extension)
        let mut in_nomarkdown = false;
        let mut in_comment = false;
        let mut nomarkdown_ranges: Vec<(usize, usize)> = Vec::new();
        let mut nomarkdown_start = 0;
        let mut comment_start = 0;

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;

            // Check for nomarkdown start
            if line.trim() == "{::nomarkdown}" {
                in_nomarkdown = true;
                nomarkdown_start = line_num;
            } else if line.trim() == "{:/nomarkdown}" && in_nomarkdown {
                in_nomarkdown = false;
                nomarkdown_ranges.push((nomarkdown_start, line_num));
            }

            // Check for comment blocks
            if line.trim() == "{::comment}" {
                in_comment = true;
                comment_start = line_num;
            } else if line.trim() == "{:/comment}" && in_comment {
                in_comment = false;
                nomarkdown_ranges.push((comment_start, line_num));
            }
        }

        // Use centralized HTML parser to get all HTML tags (including multiline)
        let html_tags = ctx.html_tags();

        for html_tag in html_tags.iter() {
            // Skip closing tags (only warn on opening tags)
            if html_tag.is_closing {
                continue;
            }

            let line_num = html_tag.line;
            let tag_byte_start = html_tag.byte_offset;

            // Reconstruct tag string from byte offsets
            let tag = &content[html_tag.byte_offset..html_tag.byte_end];

            // Skip tags in code blocks (uses proper code block detection from LintContext)
            if ctx.line_info(line_num).is_some_and(|info| info.in_code_block) {
                continue;
            }

            // Skip Kramdown extensions and block attributes
            if let Some(line) = lines.get(line_num.saturating_sub(1))
                && (is_kramdown_extension(line) || is_kramdown_block_attribute(line))
            {
                continue;
            }

            // Skip lines inside nomarkdown blocks
            if nomarkdown_ranges
                .iter()
                .any(|(start, end)| line_num >= *start && line_num <= *end)
            {
                continue;
            }

            // Skip HTML tags inside HTML comments
            if ctx.is_in_html_comment(tag_byte_start) {
                continue;
            }

            // Skip HTML comments themselves
            if self.is_html_comment(tag) {
                continue;
            }

            // Skip angle brackets inside link reference definition titles
            // e.g., [ref]: url "Title with <angle brackets>"
            if ctx.is_in_link_title(tag_byte_start) {
                continue;
            }

            // Skip JSX components in MDX files (e.g., <Chart />, <MyComponent>)
            if ctx.flavor.supports_jsx() && html_tag.tag_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }

            // Skip non-HTML elements (placeholder syntax like <NAME>, <resource>)
            if !Self::is_html_element_or_custom(&html_tag.tag_name) {
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

            // Skip tags inside code spans (use byte offset for reliable multi-line span detection)
            if ctx.is_byte_offset_in_code_span(tag_byte_start) {
                continue;
            }

            // Skip allowed tags
            if self.is_tag_allowed(tag) {
                continue;
            }

            // Skip tags with markdown attribute in MkDocs mode
            if ctx.flavor == crate::config::MarkdownFlavor::MkDocs && self.has_markdown_attribute(tag) {
                continue;
            }

            // Calculate fix to remove HTML tags but keep content
            let fix = self
                .calculate_fix(content, tag, tag_byte_start)
                .map(|(range, replacement)| Fix { range, replacement });

            // Report the HTML tag
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: line_num,
                column: html_tag.start_col + 1,   // Convert to 1-indexed
                end_line: line_num,               // TODO: calculate actual end line for multiline tags
                end_column: html_tag.end_col + 1, // Convert to 1-indexed
                message: format!("Inline HTML found: {tag}"),
                severity: Severity::Warning,
                fix,
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No fix for MD033: do not remove or alter HTML, just return the input unchanged
        Ok(ctx.content.to_string())
    }

    fn fix_capability(&self) -> crate::rule::FixCapability {
        crate::rule::FixCapability::Unfixable
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Html
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_html()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD033Config>(config);
        Box::new(Self::from_config_struct(rule_config))
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only reports opening tags, not closing tags
        assert_eq!(result.len(), 1); // Only <div>, not </div>
        assert!(result[0].message.starts_with("Inline HTML found: <div>"));
    }

    #[test]
    fn test_md033_case_insensitive() {
        let rule = MD033NoInlineHtml::default();
        let content = "<DiV>Some <B>content</B></dIv>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only reports opening tags, not closing tags
        assert_eq!(result.len(), 2); // <DiV>, <B> (not </B>, </dIv>)
        assert_eq!(result[0].message, "Inline HTML found: <DiV>");
        assert_eq!(result[1].message, "Inline HTML found: <B>");
    }

    #[test]
    fn test_md033_allowed_tags() {
        let rule = MD033NoInlineHtml::with_allowed(vec!["div".to_string(), "br".to_string()]);
        let content = "<div>Allowed</div><p>Not allowed</p><br/>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only warnings for non-allowed opening tags (<p> only, div and br are allowed)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Inline HTML found: <p>");

        // Test case-insensitivity of allowed tags
        let content2 = "<DIV>Allowed</DIV><P>Not allowed</P><BR/>";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 1); // Only <P> flagged
        assert_eq!(result2[0].message, "Inline HTML found: <P>");
    }

    #[test]
    fn test_md033_html_comments() {
        let rule = MD033NoInlineHtml::default();
        let content = "<!-- This is a comment --> <p>Not a comment</p>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should detect warnings for HTML opening tags (comments are skipped, closing tags not reported)
        assert_eq!(result.len(), 1); // Only <p>
        assert_eq!(result[0].message, "Inline HTML found: <p>");
    }

    #[test]
    fn test_md033_tags_in_links() {
        let rule = MD033NoInlineHtml::default();
        let content = "[Link](http://example.com/<div>)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The <div> in the URL should be detected as HTML (not skipped)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Inline HTML found: <div>");

        let content2 = "[Link <a>text</a>](url)";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        // Only reports opening tags
        assert_eq!(result2.len(), 1); // Only <a>
        assert_eq!(result2[0].message, "Inline HTML found: <a>");
    }

    #[test]
    fn test_md033_fix_escaping() {
        let rule = MD033NoInlineHtml::default();
        let content = "Text with <div> and <br/> tags.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_content = rule.fix(&ctx).unwrap();
        // No fix for HTML tags; output should be unchanged
        assert_eq!(fixed_content, content);
    }

    #[test]
    fn test_md033_in_code_blocks() {
        let rule = MD033NoInlineHtml::default();
        let content = "```html\n<div>Code</div>\n```\n<div>Not code</div>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only reports opening tags outside code block
        assert_eq!(result.len(), 1); // Only <div> outside code block
        assert_eq!(result[0].message, "Inline HTML found: <div>");
    }

    #[test]
    fn test_md033_in_code_spans() {
        let rule = MD033NoInlineHtml::default();
        let content = "Text with `<p>in code</p>` span. <br/> Not in span.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should detect <br/> outside code span, but not tags inside code span
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Inline HTML found: <br/>");
    }

    #[test]
    fn test_md033_issue_90_code_span_with_diff_block() {
        // Test for issue #90: inline code span followed by diff code block
        let rule = MD033NoInlineHtml::default();
        let content = r#"# Heading

`<env>`

```diff
- this
+ that
```"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should NOT detect <env> as HTML since it's inside backticks
        assert_eq!(result.len(), 0, "Should not report HTML tags inside code spans");
    }

    #[test]
    fn test_md033_multiple_code_spans_with_angle_brackets() {
        // Test multiple code spans on same line
        let rule = MD033NoInlineHtml::default();
        let content = "`<one>` and `<two>` and `<three>` are all code spans";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should not report HTML tags inside any code spans");
    }

    #[test]
    fn test_md033_nested_angle_brackets_in_code_span() {
        // Test nested angle brackets
        let rule = MD033NoInlineHtml::default();
        let content = "Text with `<<nested>>` brackets";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should handle nested angle brackets in code spans");
    }

    #[test]
    fn test_md033_code_span_at_end_before_code_block() {
        // Test code span at end of line before code block
        let rule = MD033NoInlineHtml::default();
        let content = "Testing `<test>`\n```\ncode here\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should handle code span before code block");
    }

    #[test]
    fn test_md033_quick_fix_inline_tag() {
        // Test Quick Fix for inline HTML tags - keeps content, removes tags
        let rule = MD033NoInlineHtml::default();
        let content = "This has <span>inline text</span> that should keep content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should find one HTML tag");
        assert!(result[0].fix.is_some(), "Should have a fix");

        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(&content[fix.range.clone()], "<span>inline text</span>");
        assert_eq!(fix.replacement, "inline text");
    }

    #[test]
    fn test_md033_quick_fix_multiline_tag() {
        // Test Quick Fix for multiline HTML tags - keeps content
        let rule = MD033NoInlineHtml::default();
        let content = "<div>\nBlock content\n</div>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should find one HTML tag");
        assert!(result[0].fix.is_some(), "Should have a fix");

        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(&content[fix.range.clone()], "<div>\nBlock content\n</div>");
        assert_eq!(fix.replacement, "\nBlock content\n");
    }

    #[test]
    fn test_md033_quick_fix_self_closing_tag() {
        // Test Quick Fix for self-closing tags - removes tag (no content)
        let rule = MD033NoInlineHtml::default();
        let content = "Self-closing: <br/>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should find one HTML tag");
        assert!(result[0].fix.is_some(), "Should have a fix");

        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(&content[fix.range.clone()], "<br/>");
        assert_eq!(fix.replacement, "");
    }

    #[test]
    fn test_md033_quick_fix_multiple_tags() {
        // Test Quick Fix with multiple HTML tags - keeps content for both
        let rule = MD033NoInlineHtml::default();
        let content = "<span>first</span> and <strong>second</strong>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should find two HTML tags");
        assert!(result[0].fix.is_some(), "First tag should have a fix");
        assert!(result[1].fix.is_some(), "Second tag should have a fix");

        let fix1 = result[0].fix.as_ref().unwrap();
        assert_eq!(&content[fix1.range.clone()], "<span>first</span>");
        assert_eq!(fix1.replacement, "first");

        let fix2 = result[1].fix.as_ref().unwrap();
        assert_eq!(&content[fix2.range.clone()], "<strong>second</strong>");
        assert_eq!(fix2.replacement, "second");
    }

    #[test]
    fn test_md033_skip_angle_brackets_in_link_titles() {
        // Angle brackets inside link reference definition titles should not be flagged as HTML
        let rule = MD033NoInlineHtml::default();
        let content = r#"# Test

[example]: <https://example.com> "Title with <Angle Brackets> inside"

Regular text with <div>content</div> HTML tag.
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag <div>, not <Angle Brackets> in the title (not a valid HTML element)
        // Opening tag only (markdownlint behavior)
        assert_eq!(result.len(), 1, "Should find opening div tag");
        assert!(
            result[0].message.contains("<div>"),
            "Should flag <div>, got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_md033_skip_angle_brackets_in_link_title_single_quotes() {
        // Test with single-quoted title
        let rule = MD033NoInlineHtml::default();
        let content = r#"[ref]: url 'Title <Help Wanted> here'

<span>text</span> here
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // <Help Wanted> is not a valid HTML element, so only <span> is flagged
        // Opening tag only (markdownlint behavior)
        assert_eq!(result.len(), 1, "Should find opening span tag");
        assert!(
            result[0].message.contains("<span>"),
            "Should flag <span>, got: {}",
            result[0].message
        );
    }
}
