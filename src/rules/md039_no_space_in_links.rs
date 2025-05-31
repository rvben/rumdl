use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pre-compiled regex patterns for performance - using DOTALL flag to match newlines
    static ref LINK_PATTERN: Regex = Regex::new(r"(?s)!?\[([^\]]*)\]\(([^)]*)\)").unwrap();

    // Fast check patterns - simple string-based checks are faster than complex regex
    static ref WHITESPACE_CHECK: Regex = Regex::new(r"^\s+|\s+$").unwrap();
    static ref ALL_WHITESPACE: Regex = Regex::new(r"^\s*$").unwrap();
}

/// Rule MD039: No space inside link text
///
/// See [docs/md039.md](../../docs/md039.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when link text has leading or trailing spaces which can cause
/// unexpected rendering in some Markdown parsers.
#[derive(Debug, Default, Clone)]
pub struct MD039NoSpaceInLinks;

// Static definition for the warning message
const WARNING_MESSAGE: &str = "Remove spaces inside link text";

impl MD039NoSpaceInLinks {
    pub fn new() -> Self {
        Self
    }

    /// Optimized fast check to see if content has any potential links or images
    #[inline]
    fn has_links_or_images(&self, content: &str) -> bool {
        LINK_PATTERN.is_match(content)
    }

    /// Optimized link parsing using regex with early returns
    fn parse_links_and_images(
        content: &str,
    ) -> Vec<(bool, &str, &str, usize, usize, usize, usize)> {
        let mut results = Vec::new();

        // Early return if no potential links
        if !LINK_PATTERN.is_match(content) {
            return results;
        }

        // Pre-compute code block ranges once for efficiency
        let code_block_ranges =
            crate::utils::code_block_utils::CodeBlockUtils::detect_code_blocks(content);

        // Use optimized regex parsing instead of character-by-character iteration
        for m in LINK_PATTERN.find_iter(content) {
            let match_start = m.start();
            let match_end = m.end();

            // Skip if in code block (optimized check)
            if code_block_ranges
                .iter()
                .any(|&(start, end)| match_start >= start && match_start < end)
            {
                continue;
            }

            let full_match = m.as_str();
            let is_image = full_match.starts_with('!');

            // Extract using the regex capture groups for better performance
            if let Some(captures) = LINK_PATTERN.captures(full_match) {
                if let (Some(text_match), Some(url_match)) = (captures.get(1), captures.get(2)) {
                    let text = text_match.as_str();
                    let url = url_match.as_str();

                    // Calculate absolute positions
                    let text_start = match_start + text_match.start() + if is_image { 2 } else { 1 };
                    let text_end = match_start + text_match.end() + if is_image { 2 } else { 1 };

                    results.push((
                        is_image,
                        text,
                        url,
                        match_start,
                        match_end,
                        text_start,
                        text_end,
                    ));
                }
            }
        }
        results
    }

    #[inline]
    fn trim_link_text_preserve_escapes(text: &str) -> &str {
        // Optimized trimming that preserves escapes
        let start = text
            .char_indices()
            .find(|&(_, c)| !c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        let end = text
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if start >= end {
            ""
        } else {
            &text[start..end]
        }
    }

    /// Optimized whitespace checking for link text
    #[inline]
    fn needs_trimming(&self, text: &str) -> bool {
        // Simple and fast check: compare with trimmed version
        text != text.trim_matches(|c: char| c.is_whitespace())
    }

    /// Optimized unescaping for performance-critical path
    #[inline]
    fn unescape_fast(&self, text: &str) -> String {
        if !text.contains('\\') {
            return text.to_string();
        }

        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    result.push(next);
                    chars.next();
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }
        result
    }
}

impl Rule for MD039NoSpaceInLinks {
    fn name(&self) -> &'static str {
        "MD039"
    }

    fn description(&self) -> &'static str {
        "Spaces inside link text"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !self.has_links_or_images(content)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if self.should_skip(ctx) {
            return Ok(Vec::new());
        }

        let content = ctx.content;
        let mut warnings = Vec::new();

        // Parse links and images once with optimized algorithm
        let links_and_images = Self::parse_links_and_images(content);

        // Early return if no links found
        if links_and_images.is_empty() {
            return Ok(Vec::new());
        }

        for (is_image, text, url, link_start, _link_end, _text_start, _text_end) in links_and_images {
            // Fast check if trimming is needed
            if !self.needs_trimming(text) {
                continue;
            }

            // Optimized unescaping for whitespace check
            let unescaped = self.unescape_fast(text);

            let needs_warning = if ALL_WHITESPACE.is_match(&unescaped) {
                true
            } else {
                let trimmed = text.trim_matches(|c: char| c.is_whitespace());
                text != trimmed
            };

            if needs_warning {
                let original = if is_image {
                    format!("![{}]({})", text, url)
                } else {
                    format!("[{}]({})", text, url)
                };

                let fixed = if ALL_WHITESPACE.is_match(&unescaped) {
                    if is_image {
                        format!("![]({})", url)
                    } else {
                        format!("[]({})", url)
                    }
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(text);
                    if is_image {
                        format!("![{}]({})", trimmed, url)
                    } else {
                        format!("[{}]({})", trimmed, url)
                    }
                };

                let (line, column) = ctx.offset_to_line_col(link_start);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line,
                    column,
                    end_line: line,
                    end_column: column + original.len(),
                    message: WARNING_MESSAGE.to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link_start..link_start + original.len(),
                        replacement: fixed,
                    }),
                });
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        let content = ctx.content;
        let links_and_images = Self::parse_links_and_images(content);

        if links_and_images.is_empty() {
            return Ok(content.to_string());
        }

        let mut fixes = Vec::new();

        for (is_image, text, url, link_start, link_end, _text_start, _text_end) in links_and_images {
            // Fast check if trimming is needed
            if !self.needs_trimming(text) {
                continue;
            }

            // Optimized unescaping for whitespace check
            let unescaped = self.unescape_fast(text);

            let replacement = if ALL_WHITESPACE.is_match(&unescaped) {
                if is_image {
                    format!("![]({})", url)
                } else {
                    format!("[]({})", url)
                }
            } else {
                let trimmed = Self::trim_link_text_preserve_escapes(text);
                if is_image {
                    format!("![{}]({})", trimmed, url)
                } else {
                    format!("[{}]({})", trimmed, url)
                }
            };
            fixes.push((link_start, link_end, replacement));
        }

        if fixes.is_empty() {
            return Ok(content.to_string());
        }

        // Apply fixes efficiently
        let mut result = String::with_capacity(content.len());
        let mut last_pos = 0;

        for (start, end, replacement) in fixes {
            result.push_str(&content[last_pos..start]);
            result.push_str(&replacement);
            last_pos = end;
        }
        result.push_str(&content[last_pos..]);

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD039NoSpaceInLinks {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        !doc_structure.links.is_empty() || !doc_structure.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link](url) and [another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_spaces_both_ends() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_start() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link](url) and [ another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_end() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link ](url) and [another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_link_in_code_block() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "```
[ link ](url)
```
[ link ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            "```
[ link ](url)
```
[link](url)"
        );
    }

    #[test]
    fn test_multiple_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another ](url) in one line";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another](url) in one line");
    }

    #[test]
    fn test_link_with_internal_spaces() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[this is link](url) and [ this is also link ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
    }

    #[test]
    fn test_link_with_punctuation() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link! ](url) and [ link? ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link!](url) and [link?](url) here");
    }

    #[test]
    fn test_parity_only_whitespace_and_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[   \n  ](url) and [\t\n\t](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint removes all whitespace, resulting in empty link text
        assert_eq!(fixed, "[](url) and [](url)");
    }

    #[test]
    fn test_parity_internal_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\ntext](url) and [ another\nlink ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint trims only leading/trailing whitespace, preserves internal newlines
        assert_eq!(fixed, "[link\ntext](url) and [another\nlink](url)");
    }

    #[test]
    fn test_parity_escaped_brackets_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\\]](url) and [link\\[]](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint does not trim or remove escapes, so output should be unchanged
        assert_eq!(fixed, "[link\\]](url) and [link\\[]](url)");
    }

    #[test]
    fn test_performance_md039() {
        use std::time::Instant;

        let rule = MD039NoSpaceInLinks::new();

        // Generate test content with many links
        let mut content = String::with_capacity(100_000);

        // Add links with spaces (should be detected and fixed)
        for i in 0..500 {
            content.push_str(&format!("Line {} with [ spaced link {} ](url{}) and text.\n", i, i, i));
        }

        // Add valid links (should be fast to skip)
        for i in 0..500 {
            content.push_str(&format!("Line {} with [valid link {}](url{}) and text.\n", i + 500, i, i));
        }

        println!("MD039 Performance Test - Content: {} bytes, {} lines", content.len(), content.lines().count());

        let ctx = crate::lint_context::LintContext::new(&content);

        // Warm up
        let _ = rule.check(&ctx).unwrap();

        // Measure check performance
        let mut total_duration = std::time::Duration::ZERO;
        let runs = 5;
        let mut warnings_count = 0;

        for _ in 0..runs {
            let start = Instant::now();
            let warnings = rule.check(&ctx).unwrap();
            total_duration += start.elapsed();
            warnings_count = warnings.len();
        }

        let avg_check_duration = total_duration / runs;

        println!("MD039 Optimized Performance:");
        println!("- Average check time: {:?} ({:.2} ms)", avg_check_duration, avg_check_duration.as_secs_f64() * 1000.0);
        println!("- Found {} warnings", warnings_count);
        println!("- Lines per second: {:.0}", content.lines().count() as f64 / avg_check_duration.as_secs_f64());
        println!("- Microseconds per line: {:.2}", avg_check_duration.as_micros() as f64 / content.lines().count() as f64);

        // Performance assertion - should complete reasonably fast
        assert!(avg_check_duration.as_millis() < 200, "MD039 check should complete in under 200ms, took {}ms", avg_check_duration.as_millis());

        // Verify we're finding the expected number of warnings (500 links with spaces)
        assert_eq!(warnings_count, 500, "Should find 500 warnings for links with spaces");
    }
}
