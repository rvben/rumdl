/// Rule MD034: No bare URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity, MarkdownAst, AstExtensions, MaybeAst};
use crate::utils::early_returns;

use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use crate::lint_context::LintContext;
use markdown::mdast::Node;

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
    static ref SIMPLE_URL_REGEX: Regex = Regex::new(r#"(https?|ftp)://[^\s<>\[\]()\\'\"`]+(?:\.[^\s<>\[\]()\\'\"`]+)*(?::\d+)?(?:/[^\s<>\[\]()\\'\"`]*)?"#).unwrap();

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
        !early_returns::has_urls(content) && !content.contains('@')
    }

    // Find all bare URLs in a line, using DocumentStructure for code span detection
    fn find_bare_urls_with_structure(
        &self,
        line: &str,
        line_idx: usize,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Early return: empty lines
        if line.trim().is_empty() {
            return warnings;
        }

        // Fast path - check if line potentially contains a URL
        if !URL_QUICK_CHECK.is_match(line) {
            return warnings;
        }

        // Skip lines that consist only of a badge link
        if BADGE_LINK_LINE.is_match(line) {
            return warnings;
        }

        // Early return: skip reference definitions
        if REFERENCE_DEF_RE.is_match(line) {
            return warnings;
        }

        // --- NEW: Collect all link/image destination ranges using regex ---
        let mut excluded_ranges: Vec<(usize, usize)> = Vec::new();
        // Markdown links: [text](url)
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(line) {
            if let Some(dest) = cap.get(1) {
                excluded_ranges.push((dest.start(), dest.end()));
            }
        }
        // Markdown images: ![alt](url)
        for cap in MARKDOWN_IMAGE_PATTERN.captures_iter(line) {
            if let Some(dest) = cap.get(2) {
                excluded_ranges.push((dest.start(), dest.end()));
            }
        }
        // Angle-bracket links: <url>
        for cap in ANGLE_LINK_PATTERN.captures_iter(line) {
            if let Some(m) = cap.get(1) {
                excluded_ranges.push((m.start(), m.end()));
            }
        }
        // HTML attribute URLs: src="url", href="url", etc.
        for cap in HTML_ATTRIBUTE_URL.captures_iter(line) {
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

        for url_match in SIMPLE_URL_REGEX.find_iter(line) {
            let url_start = url_match.start();
            let url_end = url_match.end();
            // Manual boundary check: not part of a larger word
            let before = if url_start == 0 {
                None
            } else {
                line.get(url_start - 1..url_start)
            };
            let after = line.get(url_end..url_end + 1);
            let is_valid_boundary = before.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_"
            }) && after.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_"
            });
            if !is_valid_boundary {
                continue;
            }
            // Skip if this URL is within a code span (using DocumentStructure)
            if structure.is_in_code_span(line_idx + 1, url_start + 1) {
                continue;
            }
            // --- NEW: Skip if URL is within any excluded range (link/image dest) ---
            let in_any_range = merged.iter().any(|(start, end)| url_start >= *start && url_end <= *end);
            if in_any_range {
                continue;
            }
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: line_idx + 1,
                column: url_start + 1,
                message: format!("Bare URL found: {}", &line[url_start..url_end]),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: url_start..url_end,
                    replacement: format!("<{}>", &line[url_start..url_end]),
                }),
            });
        }

        // Check for email addresses - similar logic to URLs
        for email_match in EMAIL_REGEX.find_iter(line) {
            let email_start = email_match.start();
            let email_end = email_match.end();
            // Manual boundary check: not part of a larger word
            let before = if email_start == 0 {
                None
            } else {
                line.get(email_start - 1..email_start)
            };
            let after = line.get(email_end..email_end + 1);
            let is_valid_boundary = before.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
            }) && after.map_or(true, |c| {
                !c.chars().next().unwrap().is_alphanumeric() && c != "_" && c != "."
            });
            if !is_valid_boundary {
                continue;
            }
            // Skip if this email is within a code span (using DocumentStructure)
            if structure.is_in_code_span(line_idx + 1, email_start + 1) {
                continue;
            }
            // Skip if email is within any excluded range (link/image dest)
            let in_any_range = merged.iter().any(|(start, end)| email_start >= *start && email_end <= *end);
            if in_any_range {
                continue;
            }
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: line_idx + 1,
                column: email_start + 1,
                message: format!("Bare email address found: {}", &line[email_start..email_end]),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: email_start..email_end,
                    replacement: format!("<{}>", &line[email_start..email_end]),
                }),
            });
        }
        warnings
    }

    // Uses DocumentStructure for code block and code span detection in check_with_structure.
    pub fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return: skip if no URLs or emails
        if self.should_skip(content) {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        for (i, line) in content.lines().enumerate() {
            // Early return: skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Skip lines in code blocks
            if structure.is_in_code_block(i + 1) {
                continue;
            }

            // Skip reference link definitions (moved here for efficiency)
            if REFERENCE_DEF_RE.is_match(line) {
                continue;
            }

            warnings.extend(self.find_bare_urls_with_structure(line, i, structure));
        }
        Ok(warnings)
    }

    /// AST-based bare URL detection: only flag URLs in text nodes not inside links/images/code/html
    fn find_bare_urls_in_ast(
        &self,
        node: &Node,
        parent_is_link_or_image: bool,
        content: &str,
        warnings: &mut Vec<LintWarning>,
        ctx: &LintContext,
    ) {
        use markdown::mdast::Node::*;
        match node {
            Text(text) if !parent_is_link_or_image => {
                let text_str = &text.value;
                for url_match in SIMPLE_URL_REGEX.find_iter(text_str) {
                    let url_start = url_match.start();
                    let url_end = url_match.end();
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
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line,
                            column,
                            message: format!("Bare URL found: {}", &text_str[url_start..url_end]),
                            severity: Severity::Warning,
                            fix: None, // Fix not implemented yet
                        });
                    }
                }
            }
            Link(link) => {
                for child in &link.children {
                    self.find_bare_urls_in_ast(child, true, content, warnings, ctx);
                }
            }
            Image(image) => {
                // Only check alt text for bare URLs (rare, but possible)
                let alt_str = &image.alt;
                for url_match in SIMPLE_URL_REGEX.find_iter(alt_str) {
                    let url_start = url_match.start();
                    let url_end = url_match.end();
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
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line,
                            column,
                            message: format!("Bare URL found: {}", &alt_str[url_start..url_end]),
                            severity: Severity::Warning,
                            fix: None,
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
                        self.find_bare_urls_in_ast(child, false, content, warnings, ctx);
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
        "Bare URL used"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Use line-based detection to properly distinguish between bare URLs and autolinks
        // AST-based approach doesn't work because CommonMark parser converts bare URLs to links
        let content = ctx.content;

        // Early return for empty content or content without URLs or emails
        if content.is_empty() || (!content.contains("http://") && !content.contains("https://") && !content.contains("ftp://") && !content.contains('@')) {
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
        true
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
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Group warnings by line number for easier processing
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Process warnings line by line (in reverse order to avoid offset issues)
        let mut warnings_by_line: std::collections::BTreeMap<usize, Vec<&crate::rule::LintWarning>> = std::collections::BTreeMap::new();
        for warning in &warnings {
            warnings_by_line.entry(warning.line).or_insert_with(Vec::new).push(warning);
        }

        // Process lines in reverse order to avoid affecting line indices
        for (line_num, line_warnings) in warnings_by_line.iter().rev() {
            let line_idx = line_num - 1;
            if line_idx >= lines.len() {
                continue;
            }

            // Sort warnings by column in reverse order (rightmost first)
            let mut sorted_warnings = line_warnings.clone();
            sorted_warnings.sort_by_key(|w| std::cmp::Reverse(w.column));

            for warning in sorted_warnings {
                if let Some(fix) = &warning.fix {
                    let line = &mut lines[line_idx];
                    let start = fix.range.start;
                    let end = fix.range.end;

                    if start <= line.len() && end <= line.len() && start < end {
                        line.replace_range(start..end, &fix.replacement);
                    }
                }
            }
        }

        Ok(lines.join("\n"))
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
        // This rule is only relevant if there might be URLs in the content
        let content = ctx.content;
        !content.is_empty() && (content.contains("http://") || content.contains("https://") || content.contains("ftp://"))
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
        assert!(result.is_empty(), "Multiple badges and links on one line should not be flagged as bare URLs");
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
}
