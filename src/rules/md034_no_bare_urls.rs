use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::early_returns;
use crate::utils::regex_cache;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Simple pattern to quickly check if a line might contain a URL
    static ref URL_QUICK_CHECK: Regex = Regex::new(r"(?:https?|ftp)://").unwrap();

    // Comprehensive URL detection pattern
    static ref URL_REGEX: Regex = Regex::new(r#"(?:https?|ftp)://[^\s<>\[\]()'"]+[^\s<>\[\]()"'.,]"#).unwrap();

    // Pattern to match markdown link format
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();

    // Pattern to match angle bracket link format
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r"<((?:https?|ftp)://[^>]+)>").unwrap();

    // Pattern to match code fences
    static ref CODE_FENCE_RE: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
}

#[derive(Default)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    // Method to quickly check if the rule needs to run at all
    pub fn should_skip(&self, content: &str) -> bool {
        !early_returns::has_urls(content)
    }

    // Optimized function to detect code blocks with cached results
    #[inline]
    fn is_in_code_block(&self, line_idx: usize, code_blocks: &[(usize, usize)]) -> bool {
        // Binary search for improved performance with large documents
        let mut low = 0;
        let mut high = code_blocks.len();

        while low < high {
            let mid = low + (high - low) / 2;
            let (start, end) = code_blocks[mid];

            if line_idx < start {
                high = mid;
            } else if line_idx > end {
                low = mid + 1;
            } else {
                return true;
            }
        }
        false
    }

    // Fast binary search to check if a position is within any code span
    #[inline]
    fn is_in_code_span(&self, pos: usize, spans: &[(usize, usize)]) -> bool {
        let mut low = 0;
        let mut high = spans.len();

        while low < high {
            let mid = low + (high - low) / 2;
            let (start, end) = spans[mid];

            if pos < start {
                high = mid;
            } else if pos > end {
                low = mid + 1;
            } else {
                return true;
            }
        }
        false
    }

    // Compute inline code spans for a line
    #[inline]
    fn compute_inline_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut in_code = false;
        let mut start = 0;

        // Fast path - no backticks
        if !line.contains('`') {
            return spans;
        }

        for (i, c) in line.char_indices() {
            if c == '`' {
                if in_code {
                    spans.push((start, i));
                    in_code = false;
                } else {
                    start = i;
                    in_code = true;
                }
            }
        }
        spans
    }

    // Quick check if a URL is already within a markdown link
    #[inline]
    fn is_url_in_link(&self, line: &str, url_start: usize, url_end: usize) -> bool {
        if ANGLE_LINK_PATTERN.is_match(line) {
            for cap in ANGLE_LINK_PATTERN.captures_iter(line) {
                let m = cap.get(0).unwrap();
                if m.start() <= url_start && m.end() >= url_end {
                    return true;
                }
            }
        }

        if MARKDOWN_LINK_PATTERN.is_match(line) {
            for cap in MARKDOWN_LINK_PATTERN.captures_iter(line) {
                if let Some(m) = cap.get(2) {
                    if m.start() <= url_start && m.end() >= url_end {
                        return true;
                    }
                }
            }
        }

        false
    }

    // Find all bare URLs in a line
    fn find_bare_urls(
        &self,
        line: &str,
        line_idx: usize,
        spans: &[(usize, usize)],
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Fast path - check if line potentially contains a URL
        if !line.contains("http://") && !line.contains("https://") && !line.contains("ftp://") {
            return warnings;
        }

        for url_match in URL_REGEX.find_iter(line) {
            let url_start = url_match.start();
            let url_end = url_match.end();
            let url = url_match.as_str();

            // Skip if this URL is within a code span
            if self.is_in_code_span(url_start, spans) {
                continue;
            }

            // Skip if URL is already in a link
            if self.is_url_in_link(line, url_start, url_end) {
                continue;
            }

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: line_idx + 1,
                column: url_start + 1,
                message: format!("Bare URL found: {}", url),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: url_start..url_end,
                    replacement: format!("<{}>", url),
                }),
            });
        }

        warnings
    }

    // Detect code blocks in content
    fn detect_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        let lines: Vec<&str> = content.split('\n').collect();
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut start_line = 0;
        let mut fence_char = "";

        for (i, line) in lines.iter().enumerate() {
            if let Some(cap) = CODE_FENCE_RE.captures(line) {
                let fence = cap.get(1).unwrap().as_str();
                if !in_code_block {
                    in_code_block = true;
                    start_line = i;
                    fence_char = fence;
                } else if fence.starts_with(fence_char.chars().next().unwrap())
                    && fence.len() >= fence_char.len()
                {
                    code_blocks.push((start_line, i));
                    in_code_block = false;
                }
            }
        }

        // Handle unclosed code block
        if in_code_block {
            code_blocks.push((start_line, lines.len() - 1));
        }

        code_blocks
    }
}

impl Rule for MD034NoBareUrls {
    fn name(&self) -> &'static str {
        "MD034"
    }

    fn description(&self) -> &'static str {
        "Bare URL used"
    }

    fn check(&self, content: &str) -> LintResult {
        // Fast path - if content doesn't contain URL schemes, return empty result
        if self.should_skip(content) {
            return Ok(Vec::new());
        }

        let code_blocks = self.detect_code_blocks(content);
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.split('\n').collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if self.is_in_code_block(i, &code_blocks) {
                continue;
            }

            // Skip HTML blocks - simplified detection
            if line.trim_start().starts_with('<') && line.trim_end().ends_with('>') {
                continue;
            }

            // Skip front matter
            if (i == 0 && *line == "---") || (i == 0 && *line == "+++") {
                continue;
            }

            // Compute code spans for this line
            let spans = self.compute_inline_code_spans(line);

            // Find bare URLs in this line
            let line_warnings = self.find_bare_urls(line, i, &spans);
            warnings.extend(line_warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fast path - if content doesn't contain URL schemes, return content as-is
        if self.should_skip(content) {
            return Ok(content.to_string());
        }

        let code_blocks = self.detect_code_blocks(content);
        let mut result = String::with_capacity(content.len() + 100);
        let lines: Vec<&str> = content.split('\n').collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip processing lines in code blocks
            if self.is_in_code_block(i, &code_blocks) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Skip HTML blocks and front matter
            if line.trim_start().starts_with('<') && line.trim_end().ends_with('>')
                || (i == 0 && *line == "---")
                || (i == 0 && *line == "+++")
            {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Compute code spans
            let spans = self.compute_inline_code_spans(line);

            // Find bare URLs and fix them
            let mut last_end = 0;
            let mut has_url = false;

            for url_match in URL_REGEX.find_iter(line) {
                let url_start = url_match.start();
                let url_end = url_match.end();
                let url = url_match.as_str();

                // Skip if URL is in a code span or already in a link
                if self.is_in_code_span(url_start, &spans)
                    || self.is_url_in_link(line, url_start, url_end)
                {
                    continue;
                }

                has_url = true;

                // Add text before the URL
                result.push_str(&line[last_end..url_start]);

                // Add the URL with angle brackets
                result.push_str(&format!("<{}>", url));

                last_end = url_end;
            }

            // Add any remaining text
            if has_url {
                result.push_str(&line[last_end..]);
            } else {
                result.push_str(line);
            }

            // Add newline for all lines except the last
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    /// Check if this rule should be skipped based on content
    fn should_skip(&self, content: &str) -> bool {
        !regex_cache::contains_url(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_quick_check() {
        assert!(URL_QUICK_CHECK.is_match("This is a URL: https://example.com"));
        assert!(!URL_QUICK_CHECK.is_match("This has no URL"));
    }

    #[test]
    fn test_is_in_code_span() {
        let rule = MD034NoBareUrls;
        let spans = vec![(5, 10), (15, 20), (25, 30)];

        assert!(rule.is_in_code_span(7, &spans));
        assert!(rule.is_in_code_span(5, &spans));
        assert!(rule.is_in_code_span(10, &spans));
        assert!(rule.is_in_code_span(15, &spans));
        assert!(rule.is_in_code_span(20, &spans));
        assert!(!rule.is_in_code_span(12, &spans));
        assert!(!rule.is_in_code_span(31, &spans));
    }
}
