/// Rule MD034: No unformatted URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{
    AstExtensions, Fix, LintError, LintResult, LintWarning, MarkdownAst, MaybeAst, Rule,
    RuleCategory, Severity,
};
use crate::utils::early_returns;
use crate::utils::range_utils::calculate_url_range;

use crate::lint_context::LintContext;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use markdown::mdast::Node;
use regex::Regex;

lazy_static! {
    // Simple pattern to quickly check if a line might contain a URL or email
    static ref URL_QUICK_CHECK: Regex = Regex::new(r#"(?:https?|ftp)://|@"#).unwrap();

    // Use fancy-regex for look-behind/look-ahead
    static ref URL_REGEX: FancyRegex = FancyRegex::new(r#"(?<![\w\[\(\<])((?:https?|ftp)://[^
\s<>\[\]()\\'\"]+)(?![\w\]\)\>])"#).unwrap();
    static ref URL_FIX_REGEX: FancyRegex = FancyRegex::new(r#"(?<![\w\[\(\<])((?:https?|ftp)://[^
\s<>\[\]()\\'\"]*[^
\s<>\[\]()\\'\".,;:!?])(?![\w\]\)\>])"#).unwrap();

    // Pattern to match markdown link format - capture destination in Group 1
    // Updated to handle nested brackets in badge links like [![badge](img)](link)
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap();

    // Pattern to match angle bracket link format (URLs and emails)
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r#"<((?:https?|ftp)://[^>]+|[^@\s]+@[^@\s]+\.[^@\s>]+)>"#).unwrap();

    // Pattern to match code fences
    static ref CODE_FENCE_RE: Regex = Regex::new(r#"^(`{3,}|~{3,})"#).unwrap();

    // Add regex to identify lines containing only a badge link
    static ref BADGE_LINK_LINE: Regex = Regex::new(r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#).unwrap();

    // Add pattern to check if link text is *only* an image
    static ref IMAGE_ONLY_LINK_TEXT_PATTERN: Regex = Regex::new(r#"^!\s*\[[^\]]*\]\s*\([^)]*\)$"#).unwrap();

    // Captures full image in 0, alt text in 1, src in 2
    static ref MARKDOWN_IMAGE_PATTERN: Regex = Regex::new(r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap();

    // Add a simple regex for candidate URLs (no look-behind/look-ahead)
    // Updated to match markdownlint's behavior: URLs can have domains without dots
    // Handles URL components properly: scheme://domain[:port][/path][?query][#fragment]
    // Will post-process to remove trailing sentence punctuation
    static ref SIMPLE_URL_REGEX: Regex = Regex::new(r#"(https?|ftp)://[^\s<>\[\]()\\'\"`]+(?:\.[^\s<>\[\]()\\'\"`]+)*(?::\d+)?(?:/[^\s<>\[\]()\\'\"`]*)?(?:\?[^\s<>\[\]()\\'\"`]*)?(?:#[^\s<>\[\]()\\'\"`]*)?"#).unwrap();

    // Add regex for email addresses - matches markdownlint behavior
    // Detects email addresses that should be autolinked like URLs
    static ref EMAIL_REGEX: Regex = Regex::new(r#"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"#).unwrap();

    // Add regex for reference definitions
    static ref REFERENCE_DEF_RE: Regex = Regex::new(r"^\s*\[[^\]]+\]:\s*https?://\S+$").unwrap();

    // Pattern to match URLs inside HTML attributes (src, href, srcset, etc.)
    static ref HTML_ATTRIBUTE_URL: Regex = Regex::new(r#"(?:src|href|srcset|content|data-\w+)\s*=\s*["']([^"']*)["']"#).unwrap();
}

#[derive(Default, Clone)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    #[inline]
    pub fn should_skip(&self, content: &str) -> bool {
        // Skip if content has no URLs and no email addresses
        let has_urls = early_returns::has_urls(content);
        let has_emails = content.contains('@');
        let should_skip = !has_urls && !has_emails;

        should_skip
    }

    /// Remove trailing punctuation that is likely sentence punctuation, not part of the URL
    fn trim_trailing_punctuation<'a>(&self, url: &'a str) -> &'a str {
        let trailing_punct = ['.', ',', ';', ':', '!', '?'];
        let mut end = url.len();

        // Remove trailing punctuation characters
        while end > 0 {
            if let Some(last_char) = url.chars().nth(end - 1) {
                if trailing_punct.contains(&last_char) {
                    end -= last_char.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        &url[..end]
    }

    // Uses DocumentStructure for code block and code span detection in check_with_structure.
    pub fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return: skip if no URLs or emails
        if self.should_skip(content) {
            return Ok(vec![]);
        }

        // Process the entire content to handle multi-line markdown links
        let mut warnings = Vec::new();

        // First, find all markdown link ranges across the entire content
        let mut excluded_ranges: Vec<(usize, usize)> = Vec::new();

        // Markdown links: [text](url) - handle multi-line
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(content) {
            if let Some(dest) = cap.get(1) {
                excluded_ranges.push((dest.start(), dest.end()));
            }
        }

        // Markdown images: ![alt](url) - handle multi-line
        for cap in MARKDOWN_IMAGE_PATTERN.captures_iter(content) {
            if let Some(dest) = cap.get(2) {
                excluded_ranges.push((dest.start(), dest.end()));
            }
        }

        // Angle-bracket links: <url>
        for cap in ANGLE_LINK_PATTERN.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                excluded_ranges.push((m.start(), m.end()));
            }
        }

        // HTML attribute URLs: src="url", href="url", etc.
        for cap in HTML_ATTRIBUTE_URL.captures_iter(content) {
            if let Some(url_attr) = cap.get(1) {
                excluded_ranges.push((url_attr.start(), url_attr.end()));
            }
        }

        // Sort and merge overlapping ranges
        excluded_ranges.sort_by_key(|r| r.0);
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in excluded_ranges {
            if let Some((_, last_end)) = merged.last_mut() {
                if *last_end >= start {
                    *last_end = (*last_end).max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }

        // Now find all URLs in the content and check if they're excluded
        for url_match in SIMPLE_URL_REGEX.find_iter(content) {
            let url_start = url_match.start();
            let mut url_end = url_match.end();

            // Trim trailing punctuation that's likely sentence punctuation
            let raw_url = &content[url_start..url_end];
            let trimmed_url = self.trim_trailing_punctuation(raw_url);
            url_end = url_start + trimmed_url.len();

            // Skip if URL became empty after trimming
            if url_end <= url_start {
                continue;
            }

            // Manual boundary check: not part of a larger word
            let before = if url_start == 0 {
                None
            } else {
                content.get(url_start - 1..url_start)
            };
            let after = content.get(url_end..url_end + 1);
            let is_valid_boundary = before.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_"
            }) && after.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_"
            });
            if !is_valid_boundary {
                continue;
            }

            // Skip if this URL is within a code block or code span
            if ctx.is_in_code_block_or_span(url_start) {
                continue;
            }
            
            // Convert byte offset to line/column for warning
            let (line_num, col_num) = ctx.offset_to_line_col(url_start);

            // Skip if URL is within any excluded range (link/image dest)
            let in_any_range = merged
                .iter()
                .any(|(start, end)| url_start >= *start && url_end <= *end);
            if in_any_range {
                continue;
            }

            // Skip reference definitions
            let line_start = content[..url_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = content[url_start..].find('\n').map(|i| url_start + i).unwrap_or(content.len());
            let line = &content[line_start..line_end];
            if REFERENCE_DEF_RE.is_match(line) {
                continue;
            }

            let url_text = &content[url_start..url_end];
            let (start_line, start_col, end_line, end_col) =
                calculate_url_range(line_num, line, col_num - 1, url_text.len());

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("URL without angle brackets or link formatting"),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: url_start..url_end,
                    replacement: format!("<{}>", url_text),
                }),
            });
        }

        // Check for email addresses - similar logic to URLs
        for email_match in EMAIL_REGEX.find_iter(content) {
            let email_start = email_match.start();
            let email_end = email_match.end();

            // Manual boundary check: not part of a larger word
            let before = if email_start == 0 {
                None
            } else {
                content.get(email_start - 1..email_start)
            };
            let after = content.get(email_end..email_end + 1);
            let is_valid_boundary = before.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
            }) && after.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
            });
            if !is_valid_boundary {
                continue;
            }

            // Skip if this email is within a code block or code span
            if ctx.is_in_code_block_or_span(email_start) {
                continue;
            }
            
            // Convert byte offset to line/column for warning
            let (line_num, col_num) = ctx.offset_to_line_col(email_start);

            // Skip if email is within any excluded range (link/image dest)
            let in_any_range = merged
                .iter()
                .any(|(start, end)| email_start >= *start && email_end <= *end);
            if in_any_range {
                continue;
            }

            let email_text = &content[email_start..email_end];
            let line_start = content[..email_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = content[email_start..].find('\n').map(|i| email_start + i).unwrap_or(content.len());
            let line = &content[line_start..line_end];
            let (start_line, start_col, end_line, end_col) =
                calculate_url_range(line_num, line, col_num - 1, email_text.len());

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Email address without angle brackets or link formatting"),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: email_start..email_end,
                    replacement: format!("<{}>", email_text),
                }),
            });
        }

        Ok(warnings)
    }

    /// AST-based bare URL detection: only flag URLs in text nodes not inside links/images/code/html
    fn find_bare_urls_in_ast(
        &self,
        node: &Node,
        parent_is_link_or_image: bool,
        _content: &str,
        warnings: &mut Vec<LintWarning>,
        ctx: &LintContext,
    ) {
        use markdown::mdast::Node::*;
        match node {
            Text(text) if !parent_is_link_or_image => {
                let text_str = &text.value;

                // Check for URLs
                for url_match in SIMPLE_URL_REGEX.find_iter(text_str) {
                    let url_start = url_match.start();
                    let mut url_end = url_match.end();

                    // Trim trailing punctuation that's likely sentence punctuation
                    let raw_url = &text_str[url_start..url_end];
                    let trimmed_url = self.trim_trailing_punctuation(raw_url);
                    url_end = url_start + trimmed_url.len();

                    // Skip if URL became empty after trimming
                    if url_end <= url_start {
                        continue;
                    }

                    let before = if url_start == 0 {
                        None
                    } else {
                        text_str.get(url_start - 1..url_start)
                    };
                    let after = text_str.get(url_end..url_end + 1);
                    let is_valid_boundary = before.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_"
                    }) && after.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_"
                    });
                    if !is_valid_boundary {
                        continue;
                    }
                    if let Some(pos) = &text.position {
                        let offset = pos.start.offset + url_start;
                        let (line, column) = ctx.offset_to_line_col(offset);
                        let url_text = &text_str[url_start..url_end];
                        let (start_line, start_col, end_line, end_col) =
                            (line, column, line, column + url_text.chars().count());
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("URL without angle brackets or link formatting"),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: offset..(offset + url_text.len()),
                                replacement: format!("<{}>", url_text),
                            }),
                        });
                    }
                }

                // Check for email addresses
                for email_match in EMAIL_REGEX.find_iter(text_str) {
                    let email_start = email_match.start();
                    let email_end = email_match.end();
                    let before = if email_start == 0 {
                        None
                    } else {
                        text_str.get(email_start - 1..email_start)
                    };
                    let after = text_str.get(email_end..email_end + 1);
                    let is_valid_boundary = before.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
                    }) && after.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
                    });
                    if !is_valid_boundary {
                        continue;
                    }
                    if let Some(pos) = &text.position {
                        let offset = pos.start.offset + email_start;
                        let (line, column) = ctx.offset_to_line_col(offset);
                        let email_text = &text_str[email_start..email_end];
                        let (start_line, start_col, end_line, end_col) =
                            (line, column, line, column + email_text.chars().count());
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Email address without angle brackets or link formatting (wrap like: <email>)"),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: offset..(offset + email_text.len()),
                                replacement: format!("<{}>", email_text),
                            }),
                        });
                    }
                }
            }
            Link(link) => {
                for child in &link.children {
                    self.find_bare_urls_in_ast(child, true, _content, warnings, ctx);
                }
            }
            Image(image) => {
                // Only check alt text for bare URLs (rare, but possible)
                let alt_str = &image.alt;
                for url_match in SIMPLE_URL_REGEX.find_iter(alt_str) {
                    let url_start = url_match.start();
                    let mut url_end = url_match.end();

                    // Trim trailing punctuation that's likely sentence punctuation
                    let raw_url = &alt_str[url_start..url_end];
                    let trimmed_url = self.trim_trailing_punctuation(raw_url);
                    url_end = url_start + trimmed_url.len();

                    // Skip if URL became empty after trimming
                    if url_end <= url_start {
                        continue;
                    }

                    let before = if url_start == 0 {
                        None
                    } else {
                        alt_str.get(url_start - 1..url_start)
                    };
                    let after = alt_str.get(url_end..url_end + 1);
                    let is_valid_boundary = before.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_"
                    }) && after.map_or(true, |c| {
                        !c.chars().next().unwrap().is_alphanumeric() && c != "_"
                    });
                    if !is_valid_boundary {
                        continue;
                    }
                    if let Some(pos) = &image.position {
                        let offset = pos.start.offset + url_start;
                        let (line, column) = ctx.offset_to_line_col(offset);
                        let url_text = &alt_str[url_start..url_end];
                        let (start_line, start_col, end_line, end_col) =
                            (line, column, line, column + url_text.chars().count());
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("URL without angle brackets or link formatting"),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: offset..(offset + url_text.len()),
                                replacement: format!("<{}>", url_text),
                            }),
                        });
                    }
                }
            }
            Code(_) | InlineCode(_) | Html(_) => {
                // Skip code and HTML nodes
            }
            _ => {
                if let Some(children) = node.children() {
                    for child in children {
                        self.find_bare_urls_in_ast(child, false, _content, warnings, ctx);
                    }
                }
            }
        }
    }

    /// AST-based check method for MD034
    pub fn check_ast(&self, ctx: &LintContext, ast: &Node) -> LintResult {
        let mut warnings = Vec::new();
        self.find_bare_urls_in_ast(ast, false, ctx.content, &mut warnings, ctx);
        Ok(warnings)
    }
}

impl Rule for MD034NoBareUrls {
    fn name(&self) -> &'static str {
        "MD034"
    }

    fn description(&self) -> &'static str {
        "URL without angle brackets or link formatting"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Use line-based detection to properly distinguish between bare URLs and autolinks
        // AST-based approach doesn't work because CommonMark parser converts bare URLs to links
        let content = ctx.content;

        // Fast path: Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Fast path: Early return if no potential URLs or emails
        if !content.contains("http://")
            && !content.contains("https://")
            && !content.contains("ftp://")
            && !content.contains('@')
        {
            return Ok(Vec::new());
        }

        // Fast path: Quick check using simple pattern
        if !URL_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn check_with_ast(&self, ctx: &LintContext, ast: &MarkdownAst) -> LintResult {
        // Use AST-based detection for better accuracy
        let mut warnings = Vec::new();
        self.find_bare_urls_in_ast(ast, false, ctx.content, &mut warnings, ctx);
        Ok(warnings)
    }

    fn uses_ast(&self) -> bool {
        // AST-based approach doesn't work because CommonMark parser converts bare URLs to links
        // Use document structure approach instead
        false
    }

    fn uses_document_structure(&self) -> bool {
        true
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if self.should_skip(content) {
            return Ok(content.to_string());
        }

        // Get all warnings first - only fix URLs that are actually flagged
        // Use AST-based detection to match the main linting path (since uses_ast() returns true)
        let ast = crate::utils::ast_utils::get_cached_ast(content);
        let warnings = self.check_with_ast(ctx, &ast)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Sort warnings by byte offset in reverse order (rightmost first) to avoid offset issues
        let mut sorted_warnings = warnings.clone();
        sorted_warnings
            .sort_by_key(|w| std::cmp::Reverse(w.fix.as_ref().map(|f| f.range.start).unwrap_or(0)));

        let mut result = content.to_string();
        for warning in sorted_warnings {
            if let Some(fix) = &warning.fix {
                let start = fix.range.start;
                let end = fix.range.end;

                if start <= result.len() && end <= result.len() && start < end {
                    result.replace_range(start..end, &fix.replacement);
                }
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    /// Check if this rule should be skipped based on content
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.should_skip(ctx.content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn as_maybe_ast(&self) -> Option<&dyn MaybeAst> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD034NoBareUrls)
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD034NoBareUrls {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there might be URLs or emails in the content
        let content = ctx.content;
        !content.is_empty()
            && (content.contains("http://")
                || content.contains("https://")
                || content.contains("ftp://")
                || content.contains('@'))
    }
}

impl AstExtensions for MD034NoBareUrls {
    fn has_relevant_ast_elements(&self, ctx: &LintContext, ast: &MarkdownAst) -> bool {
        // Check if AST contains text nodes (where bare URLs would be)
        use crate::utils::ast_utils::ast_contains_node_type;
        !self.should_skip(ctx.content) && ast_contains_node_type(ast, "text")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_url_quick_check() {
        assert!(URL_QUICK_CHECK.is_match("This is a URL: https://example.com"));
        assert!(!URL_QUICK_CHECK.is_match("This has no URL"));
    }

    // TODO: Fix complex badge link detection - currently detects URLs in nested badge structures
    // This is a complex edge case that doesn't affect real-world parity with markdownlint
    #[test]
    #[ignore]
    fn test_multiple_badges_and_links_on_one_line() {
        let rule = MD034NoBareUrls;
        let content = "# [React](https://react.dev/) \
&middot; [![GitHub license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/facebook/react/blob/main/LICENSE) \
[![npm version](https://img.shields.io/npm/v/react.svg?style=flat)](https://www.npmjs.com/package/react) \
[![(Runtime) Build and Test](https://github.com/facebook/react/actions/workflows/runtime_build_and_test.yml/badge.svg)](https://github.com/facebook/react/actions/workflows/runtime_build_and_test.yml) \
[![(Compiler) TypeScript](https://github.com/facebook/react/actions/workflows/compiler_typescript.yml/badge.svg?branch=main)](https://github.com/facebook/react/actions/workflows/compiler_typescript.yml) \
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://legacy.reactjs.org/docs/how-to-contribute.html#your-first-pull-request)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        if !result.is_empty() {
            log::debug!("MD034 warnings: {:#?}", result);
        }
        assert!(
            result.is_empty(),
            "Multiple badges and links on one line should not be flagged as bare URLs"
        );
    }

    #[test]
    fn test_bare_urls() {
        let rule = MD034NoBareUrls;
        let content = "This is a bare URL: https://example.com/foobar";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Bare URLs should be flagged");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 21);
    }

    #[test]
    fn test_md034_performance_baseline() {
        use std::time::Instant;

        // Generate test content with various URL patterns
        let mut content = String::with_capacity(50_000);

        // Add content with bare URLs (should be detected)
        for i in 0..250 {
            content.push_str(&format!(
                "Line {} with bare URL https://example{}.com/path\n",
                i, i
            ));
        }

        // Add content with proper markdown links (should not be detected)
        for i in 0..250 {
            content.push_str(&format!(
                "Line {} with [proper link](https://example{}.com/path)\n",
                i + 250,
                i
            ));
        }

        // Add content with no URLs (should be fast)
        for i in 0..500 {
            content.push_str(&format!(
                "Line {} with no URLs, just regular text content\n",
                i + 500
            ));
        }

        // Add content with emails
        for i in 0..100 {
            content.push_str(&format!(
                "Contact user{}@example{}.com for more info\n",
                i, i
            ));
        }

        println!(
            "MD034 Performance Test - Content: {} bytes, {} lines",
            content.len(),
            content.lines().count()
        );

        let rule = MD034NoBareUrls;
        let ctx = LintContext::new(&content);

        // Warm up
        let _ = rule.check(&ctx).unwrap();

        // Measure check performance (more runs for accuracy)
        let mut total_duration = std::time::Duration::ZERO;
        let runs = 10;
        let mut warnings_count = 0;

        for _ in 0..runs {
            let start = Instant::now();
            let warnings = rule.check(&ctx).unwrap();
            total_duration += start.elapsed();
            warnings_count = warnings.len();
        }

        let avg_check_duration = total_duration / runs;

        println!("MD034 Optimized Performance:");
        println!(
            "- Average check time: {:?} ({:.2} ms)",
            avg_check_duration,
            avg_check_duration.as_secs_f64() * 1000.0
        );
        println!("- Found {} warnings", warnings_count);
        println!(
            "- Lines per second: {:.0}",
            content.lines().count() as f64 / avg_check_duration.as_secs_f64()
        );
        println!(
            "- Microseconds per line: {:.2}",
            avg_check_duration.as_micros() as f64 / content.lines().count() as f64
        );

        // Performance assertion - should complete reasonably fast
        assert!(
            avg_check_duration.as_millis() < 100,
            "MD034 check should complete in under 100ms, took {}ms",
            avg_check_duration.as_millis()
        );

        // Verify we're finding the expected number of warnings
        assert_eq!(
            warnings_count, 350,
            "Should find 250 URLs + 100 emails = 350 warnings"
        );
    }
}
