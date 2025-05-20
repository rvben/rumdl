/// Rule MD034: No bare URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::early_returns;
use crate::utils::regex_cache;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use crate::lint_context::LintContext;
use markdown::mdast::Node;

lazy_static! {
    // Simple pattern to quickly check if a line might contain a URL
    static ref URL_QUICK_CHECK: Regex = Regex::new(r#"(?:https?|ftp)://"#).unwrap();

    // Use fancy-regex for look-behind/look-ahead
    static ref URL_REGEX: FancyRegex = FancyRegex::new(r#"(?<![\w\[\(\<])((?:https?|ftp)://[^
\s<>\[\]()\\'\"]+)(?![\w\]\)\>])"#).unwrap();
    static ref URL_FIX_REGEX: FancyRegex = FancyRegex::new(r#"(?<![\w\[\(\<])((?:https?|ftp)://[^
\s<>\[\]()\\'\"]*[^
\s<>\[\]()\\'\".,;:!?])(?![\w\]\)\>])"#).unwrap();

    // Pattern to match markdown link format - capture destination in Group 1
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r#"\[[^\]]*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap();

    // Pattern to match angle bracket link format
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r#"<((?:https?|ftp)://[^>]+)>"#).unwrap();

    // Pattern to match code fences
    static ref CODE_FENCE_RE: Regex = Regex::new(r#"^(`{3,}|~{3,})"#).unwrap();

    // Add regex to identify lines containing only a badge link
    static ref BADGE_LINK_LINE: Regex = Regex::new(r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#).unwrap();

    // Add pattern to check if link text is *only* an image
    static ref IMAGE_ONLY_LINK_TEXT_PATTERN: Regex = Regex::new(r#"^!\s*\[[^\]]*\]\s*\([^)]*\)$"#).unwrap();

    // Captures full image in 0, alt text in 1, src in 2
    static ref MARKDOWN_IMAGE_PATTERN: Regex = Regex::new(r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap();

    // Add a simple regex for candidate URLs (no look-behind/look-ahead)
    static ref SIMPLE_URL_REGEX: Regex = Regex::new(r#"(https?|ftp)://[^\s<>\[\]()\\'\"`]+"#).unwrap();

    // Add regex for reference definitions
    static ref REFERENCE_DEF_RE: Regex = Regex::new(r"^\s*\[[^\]]+\]:\s*https?://\S+$").unwrap();
}

#[derive(Default, Clone)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    pub fn should_skip(&self, content: &str) -> bool {
        !early_returns::has_urls(content)
    }

    #[inline]
    fn is_url_in_link(&self, line: &str, url_start: usize, url_end: usize) -> bool {
        // Quick check - if line doesn't contain any brackets, it can't be in a link
        if !line.contains('[') && !line.contains('<') {
            return false;
        }

        // Check angle bracket links first (simpler pattern)
        if let Some(cap) = ANGLE_LINK_PATTERN.captures(line) {
            if let Some(m) = cap.get(0) {
                if m.start() < url_start && m.end() > url_end {
                    return true;
                }
            }
        }

        // Check if the URL is part of an image definition ![alt](URL)
        if line.contains("![") {
            for cap in MARKDOWN_IMAGE_PATTERN.captures_iter(line) {
                if let Some(img_src_match) = cap.get(2) {
                    if img_src_match.start() <= url_start && img_src_match.end() >= url_end {
                        return true;
                    }
                }
            }
        }

        // Check standard markdown links [...](URL)
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(line) {
            if let Some(dest_match) = cap.get(1) {
                if dest_match.start() <= url_start && dest_match.end() >= url_end {
                    return true;
                }
            }
        }

        false
    }

    // Find all bare URLs in a line, using DocumentStructure for code span detection
    fn find_bare_urls_with_structure(
        &self,
        line: &str,
        line_idx: usize,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Fast path - check if line potentially contains a URL
        if !URL_QUICK_CHECK.is_match(line) {
            return warnings;
        }

        // Skip lines that consist only of a badge link
        if BADGE_LINK_LINE.is_match(line) {
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
        // Sort and merge overlapping ranges
        excluded_ranges.sort_by_key(|r| r.0);
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in excluded_ranges {
            if let Some((last_start, last_end)) = merged.last_mut() {
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
        warnings
    }

    // Uses DocumentStructure for code block and code span detection in check_with_structure.
    pub fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        if self.should_skip(content) {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        for (i, line) in content.lines().enumerate() {
            // Skip lines in code blocks
            if structure.is_in_code_block(i + 1) {
                continue;
            }
            // Skip reference link definitions
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
        // Use AST-based detection if available
        // (ctx.ast is always present in rumdl)
        return self.check_ast(ctx, &ctx.ast);
        // Fallback: old logic (for legacy/test)
        // let content = ctx.content;
        // let structure = crate::utils::document_structure::DocumentStructure::new(content);
        // self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if self.should_skip(content) {
            return Ok(content.to_string());
        }
        // Use AST-based fix: only wrap true bare URLs in angle brackets
        let mut result = String::with_capacity(content.len() + 100);
        let mut last_offset = 0;
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        // Walk the AST and collect bare URL ranges
        fn walk(node: &Node, parent_is_link_or_image: bool, edits: &mut Vec<(usize, usize, String)>, ctx: &LintContext) {
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
                            let end = pos.start.offset + url_end;
                            edits.push((offset, end, format!("<{}>", &text_str[url_start..url_end])));
                        }
                    }
                }
                Link(link) => {
                    for child in &link.children {
                        walk(child, true, edits, ctx);
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
                            let end = pos.start.offset + url_end;
                            edits.push((offset, end, format!("<{}>", &alt_str[url_start..url_end])));
                        }
                    }
                }
                Code(_) | InlineCode(_) | Html(_) => {
                    // Skip code and HTML nodes
                }
                _ => {
                    if let Some(children) = node.children() {
                        for child in children {
                            walk(child, false, edits, ctx);
                        }
                    }
                }
            }
        }
        walk(&ctx.ast, false, &mut edits, ctx);
        // Sort edits by start offset
        edits.sort_by_key(|e| e.0);
        let mut last = 0;
        for (start, end, replacement) in edits {
            if start >= last {
                result.push_str(&content[last..start]);
                result.push_str(&replacement);
                last = end;
            }
        }
        result.push_str(&content[last..]);
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    /// Check if this rule should be skipped based on content
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !regex_cache::contains_url(ctx.content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD034NoBareUrls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use std::fs::write;

    #[test]
    fn test_url_quick_check() {
        assert!(URL_QUICK_CHECK.is_match("This is a URL: https://example.com"));
        assert!(!URL_QUICK_CHECK.is_match("This has no URL"));
    }

    #[test]
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
            println!("MD034 warnings: {:#?}", result);
        }
        assert!(result.is_empty(), "Multiple badges and links on one line should not be flagged as bare URLs");
    }

    #[test]
    fn test_bare_urls() {
        let rule = MD034NoBareUrls;
        let content = "This is a bare URL: https://example.com/foobar";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    }
}
