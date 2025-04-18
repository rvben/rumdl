use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::early_returns;
use crate::utils::regex_cache;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Simple pattern to quickly check if a line might contain a URL
    static ref URL_QUICK_CHECK: Regex = Regex::new(r#"(?:https?|ftp)://"#).unwrap();

    // Comprehensive URL detection pattern - fixed escaping and simplified
    // It now requires the URL to end with a character not typically part of markdown structure
    // or be followed by whitespace or end of line.
    static ref URL_REGEX: Regex = Regex::new(r#"(?:https?|ftp)://(?:[^\s<>[\\]()\\\'\"]*[^\s<>[\\]()\"\\\'.,;:!?])"#).unwrap();

    // Pattern to match markdown link format - capture destination carefully
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r#"\[[^]]*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap();

    // Pattern to match angle bracket link format
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r#"<((?:https?|ftp)://[^>]+)>"#).unwrap();

    // Pattern to match code fences
    static ref CODE_FENCE_RE: Regex = Regex::new(r#"^(`{3,}|~{3,})"#).unwrap();
}

#[derive(Default)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    pub fn should_skip(&self, content: &str) -> bool {
        !early_returns::has_urls(content)
    }

    // Quick check if a URL is already within a markdown link or angle bracket link
    #[inline]
    fn is_url_in_link(&self, line: &str, url_start: usize, url_end: usize) -> bool {
        // Check angle bracket links first
        for cap in ANGLE_LINK_PATTERN.captures_iter(line) {
            if let Some(m) = cap.get(0) { // Check the full match <...>
                // Ensure the bare URL is strictly inside the angle brackets
                if m.start() < url_start && m.end() > url_end {
                    return true;
                }
            }
        }

        // Check markdown links
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(line) {
             // Check if the URL range exactly matches the link destination part (...)
             if let Some(link_dest_match) = cap.get(1) { // Group 1 captures the URL destination itself
                 // Compare the start/end of the bare URL match with the captured destination
                 if link_dest_match.start() == url_start && link_dest_match.end() == url_end {
                     return true;
                 }
                 // Also handle case where the URL is *contained* within the destination,
                 // but might have surrounding whitespace or title ignored by URL_REGEX.
                 // We need to check if the bare URL range is inside the overall link destination range
                 // captured by the regex group, which might be slightly larger than the bare URL match.
                 let full_link_match = cap.get(0).unwrap(); // Full [...] (...) match
                 if full_link_match.start() < url_start && full_link_match.end() > url_end {
                     // Additional check: ensure the URL is within the parenthesis part
                     let paren_start = line[full_link_match.start()..]
                         .find('(')
                         .map(|p| full_link_match.start() + p)
                         .unwrap_or(usize::MAX);
                     let paren_end = line[..full_link_match.end()]
                         .rfind(')')
                         .unwrap_or(0);

                     if url_start > paren_start && url_end < paren_end {
                        // Heuristic: If the captured destination (group 1) contains the bare url,
                        // it's likely part of the intended link.
                        if line[link_dest_match.range()].contains(&line[url_start..url_end]) {
                            return true;
                        }
                     }
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
        if !line.contains("http://") && !line.contains("https://") && !line.contains("ftp://") {
            return warnings;
        }

        for url_match in URL_REGEX.find_iter(line) {
            let url_start = url_match.start();
            let url_end = url_match.end();
            let url = url_match.as_str();

            // Skip if this URL is within a code span (using DocumentStructure)
            if structure.is_in_code_span(line_idx + 1, url_start + 1) {
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

    // New: check_with_structure using DocumentStructure for code block and code span detection
    pub fn check_with_structure(&self, content: &str, structure: &crate::utils::document_structure::DocumentStructure) -> LintResult {
        if self.should_skip(content) {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        for (i, line) in content.lines().enumerate() {
            // Skip lines in code blocks
            if structure.is_in_code_block(i + 1) {
                continue;
            }
            warnings.extend(self.find_bare_urls_with_structure(line, i, structure));
        }
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

    fn check(&self, content: &str) -> LintResult {
        // Use DocumentStructure for all code block and code span logic
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fast path - if content doesn't contain URL schemes, return content as-is
        if self.should_skip(content) {
            return Ok(content.to_string());
        }

        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        let mut result = String::with_capacity(content.len() + 100);
        let lines: Vec<&str> = content.split('\n').collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip processing lines in code blocks
            if structure.is_in_code_block(i + 1) {
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

            // Find bare URLs and fix them using DocumentStructure for code span detection
            let mut last_end = 0;
            let mut has_url = false;

            for url_match in URL_REGEX.find_iter(line) {
                let url_start = url_match.start();
                let url_end = url_match.end();
                let url = url_match.as_str();

                // Skip if URL is in a code span or already in a link
                if structure.is_in_code_span(i + 1, url_start + 1)
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

    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_quick_check() {
        assert!(URL_QUICK_CHECK.is_match("This is a URL: https://example.com"));
        assert!(!URL_QUICK_CHECK.is_match("This has no URL"));
    }
}
