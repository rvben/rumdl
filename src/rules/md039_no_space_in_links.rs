use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::regex_cache::get_cached_regex;

// Regex patterns
const ALL_WHITESPACE_STR: &str = r"^\s*$";

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
        if start >= end { "" } else { &text[start..end] }
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Use centralized link parsing from LintContext
        for link in &ctx.links {
            // Skip reference links (markdownlint doesn't check these)
            if link.is_reference {
                continue;
            }

            // Skip links inside Jinja templates
            if ctx.is_in_jinja_range(link.byte_offset) {
                continue;
            }

            // Skip links inside JSX expressions or MDX comments
            if ctx.is_in_jsx_expression(link.byte_offset) || ctx.is_in_mdx_comment(link.byte_offset) {
                continue;
            }

            // Fast check if trimming is needed
            if !self.needs_trimming(&link.text) {
                continue;
            }

            // Optimized unescaping for whitespace check
            let unescaped = self.unescape_fast(&link.text);

            let needs_warning = if get_cached_regex(ALL_WHITESPACE_STR)
                .map(|re| re.is_match(&unescaped))
                .unwrap_or(false)
            {
                true
            } else {
                let trimmed = link.text.trim_matches(|c: char| c.is_whitespace());
                link.text.as_ref() != trimmed
            };

            if needs_warning {
                let url = if link.is_reference {
                    if let Some(ref_id) = &link.reference_id {
                        format!("[{ref_id}]")
                    } else {
                        "[]".to_string()
                    }
                } else {
                    format!("({})", link.url)
                };

                let fixed = if get_cached_regex(ALL_WHITESPACE_STR)
                    .map(|re| re.is_match(&unescaped))
                    .unwrap_or(false)
                {
                    format!("[]{url}")
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(&link.text);
                    format!("[{trimmed}]{url}")
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: link.line,
                    column: link.start_col + 1, // Convert to 1-indexed
                    end_line: link.line,
                    end_column: link.end_col + 1, // Convert to 1-indexed
                    message: WARNING_MESSAGE.to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link.byte_offset..link.byte_end,
                        replacement: fixed,
                    }),
                });
            }
        }

        // Also check images
        for image in &ctx.images {
            // Skip reference images (markdownlint doesn't check these)
            if image.is_reference {
                continue;
            }

            // Skip images inside JSX expressions or MDX comments
            if ctx.is_in_jsx_expression(image.byte_offset) || ctx.is_in_mdx_comment(image.byte_offset) {
                continue;
            }

            // Skip images inside Jinja templates
            if ctx.is_in_jinja_range(image.byte_offset) {
                continue;
            }

            // Fast check if trimming is needed
            if !self.needs_trimming(&image.alt_text) {
                continue;
            }

            // Optimized unescaping for whitespace check
            let unescaped = self.unescape_fast(&image.alt_text);

            let needs_warning = if get_cached_regex(ALL_WHITESPACE_STR)
                .map(|re| re.is_match(&unescaped))
                .unwrap_or(false)
            {
                true
            } else {
                let trimmed = image.alt_text.trim_matches(|c: char| c.is_whitespace());
                image.alt_text.as_ref() != trimmed
            };

            if needs_warning {
                let url = if image.is_reference {
                    if let Some(ref_id) = &image.reference_id {
                        format!("[{ref_id}]")
                    } else {
                        "[]".to_string()
                    }
                } else {
                    format!("({})", image.url)
                };

                let fixed = if get_cached_regex(ALL_WHITESPACE_STR)
                    .map(|re| re.is_match(&unescaped))
                    .unwrap_or(false)
                {
                    format!("![]{url}")
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(&image.alt_text);
                    format!("![{trimmed}]{url}")
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: image.line,
                    column: image.start_col + 1, // Convert to 1-indexed
                    end_line: image.line,
                    end_column: image.end_col + 1, // Convert to 1-indexed
                    message: WARNING_MESSAGE.to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: image.byte_offset..image.byte_end,
                        replacement: fixed,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut fixes = Vec::new();

        // Process links
        for link in &ctx.links {
            // Skip reference links (markdownlint doesn't check these)
            if link.is_reference {
                continue;
            }

            // Skip links inside Jinja templates
            if ctx.is_in_jinja_range(link.byte_offset) {
                continue;
            }

            if !self.needs_trimming(&link.text) {
                continue;
            }

            let unescaped = self.unescape_fast(&link.text);

            let needs_fix = if get_cached_regex(ALL_WHITESPACE_STR)
                .map(|re| re.is_match(&unescaped))
                .unwrap_or(false)
            {
                true
            } else {
                let trimmed = link.text.trim_matches(|c: char| c.is_whitespace());
                link.text.as_ref() != trimmed
            };

            if needs_fix {
                let url_part = if link.is_reference {
                    if let Some(ref_id) = &link.reference_id {
                        format!("[{ref_id}]")
                    } else {
                        "[]".to_string()
                    }
                } else {
                    format!("({})", link.url)
                };

                let replacement = if get_cached_regex(ALL_WHITESPACE_STR)
                    .map(|re| re.is_match(&unescaped))
                    .unwrap_or(false)
                {
                    format!("[]{url_part}")
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(&link.text);
                    format!("[{trimmed}]{url_part}")
                };

                fixes.push((link.byte_offset, link.byte_end, replacement));
            }
        }

        // Process images
        for image in &ctx.images {
            // Skip reference images (markdownlint doesn't check these)
            if image.is_reference {
                continue;
            }

            // Skip images inside Jinja templates
            if ctx.is_in_jinja_range(image.byte_offset) {
                continue;
            }

            if !self.needs_trimming(&image.alt_text) {
                continue;
            }

            let unescaped = self.unescape_fast(&image.alt_text);

            let needs_fix = if get_cached_regex(ALL_WHITESPACE_STR)
                .map(|re| re.is_match(&unescaped))
                .unwrap_or(false)
            {
                true
            } else {
                let trimmed = image.alt_text.trim_matches(|c: char| c.is_whitespace());
                image.alt_text.as_ref() != trimmed
            };

            if needs_fix {
                let url_part = if image.is_reference {
                    if let Some(ref_id) = &image.reference_id {
                        format!("[{ref_id}]")
                    } else {
                        "[]".to_string()
                    }
                } else {
                    format!("({})", image.url)
                };

                let replacement = if get_cached_regex(ALL_WHITESPACE_STR)
                    .map(|re| re.is_match(&unescaped))
                    .unwrap_or(false)
                {
                    format!("![]{url_part}")
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(&image.alt_text);
                    format!("![{trimmed}]{url_part}")
                };

                fixes.push((image.byte_offset, image.byte_end, replacement));
            }
        }

        if fixes.is_empty() {
            return Ok(content.to_string());
        }

        // Sort fixes by position to apply them in order
        fixes.sort_by_key(|&(start, _, _)| start);

        // Apply fixes efficiently
        let mut result = String::with_capacity(content.len());
        let mut last_pos = 0;

        for (start, end, replacement) in fixes {
            if start < last_pos {
                // This should not happen if fixes are properly sorted and non-overlapping
                return Err(LintError::FixFailed(format!(
                    "Overlapping fixes detected: last_pos={last_pos}, start={start}"
                )));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link](url) and [another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_spaces_both_ends() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_start() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link](url) and [ another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_end() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link ](url) and [another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another](url) in one line");
    }

    #[test]
    fn test_link_with_internal_spaces() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[this is link](url) and [ this is also link ](url)";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
    }

    #[test]
    fn test_link_with_punctuation() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link! ](url) and [ link? ](url) here";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link!](url) and [link?](url) here");
    }

    #[test]
    fn test_parity_only_whitespace_and_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[   \n  ](url) and [\t\n\t](url)";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint removes all whitespace, resulting in empty link text
        assert_eq!(fixed, "[](url) and [](url)");
    }

    #[test]
    fn test_parity_internal_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\ntext](url) and [ another\nlink ](url)";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint trims only leading/trailing whitespace, preserves internal newlines
        assert_eq!(fixed, "[link\ntext](url) and [another\nlink](url)");
    }

    #[test]
    fn test_parity_escaped_brackets_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\\]](url) and [link\\[]](url)";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            content.push_str(&format!("Line {i} with [ spaced link {i} ](url{i}) and text.\n"));
        }

        // Add valid links (should be fast to skip)
        for i in 0..500 {
            content.push_str(&format!(
                "Line {} with [valid link {}](url{}) and text.\n",
                i + 500,
                i,
                i
            ));
        }

        println!(
            "MD039 Performance Test - Content: {} bytes, {} lines",
            content.len(),
            content.lines().count()
        );

        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

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
        println!(
            "- Average check time: {:?} ({:.2} ms)",
            avg_check_duration,
            avg_check_duration.as_secs_f64() * 1000.0
        );
        println!("- Found {warnings_count} warnings");
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
            avg_check_duration.as_millis() < 200,
            "MD039 check should complete in under 200ms, took {}ms",
            avg_check_duration.as_millis()
        );

        // Verify we're finding the expected number of warnings (500 links with spaces)
        assert_eq!(warnings_count, 500, "Should find 500 warnings for links with spaces");
    }
}
