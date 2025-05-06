/// Rule MD034: No bare URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::early_returns;
use crate::utils::regex_cache;
use lazy_static::lazy_static;
use regex::Regex;
use fancy_regex::Regex as FancyRegex;

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

        for cap in URL_REGEX.captures_iter(line) {
            if let Ok(cap) = cap {
                let url = cap.get(1).unwrap();
                let url_start = url.start();
                let url_end = url.end();

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
                    message: format!("Bare URL found: {}", url.as_str()),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: url_start..url_end,
                        replacement: format!("<{}>", url.as_str()),
                    }),
                });
            }
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Use DocumentStructure for all code block and code span logic
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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

            for cap in URL_FIX_REGEX.captures_iter(line) {
                if let Ok(cap) = cap {
                    let url = cap.get(1).unwrap();
                    let url_start = url.start();
                    let url_end = url.end();

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
                    result.push_str(&format!("<{}>", url.as_str()));

                    last_end = url_end;
                }
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

    #[test]
    fn test_url_quick_check() {
        assert!(URL_QUICK_CHECK.is_match("This is a URL: https://example.com"));
        assert!(!URL_QUICK_CHECK.is_match("This has no URL"));
    }
}
