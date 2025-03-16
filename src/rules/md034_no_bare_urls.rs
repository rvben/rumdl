use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Cached regex patterns for better performance
    static ref URL_PATTERN: Regex = Regex::new(r#"(?:https?|ftp)://[^\s<>\[\]()'"]+[^\s<>\[\]()'".,]"#).unwrap();
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[.*?\]\((?P<url>.*?)\)").unwrap();
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r"<(?:https?|ftp)://[^>]+>").unwrap();
    static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(?:\s*)(?:```|~~~)").unwrap();
    static ref HTML_BLOCK_START: Regex = Regex::new(r"^<\w+>").unwrap();
    static ref HTML_BLOCK_END: Regex = Regex::new(r"^</\w+>").unwrap();
    static ref URL_REGEX: Regex = Regex::new(r"(?:https?|ftp)://[^\s<>\[\]()']+[^\s<>\[\]()'.,]").unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(?:\s*)(?:```|~~~)").unwrap();
}

#[derive(Debug, Default)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    // Optimized inline code detection
    fn compute_inline_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        if !line.contains('`') {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut in_code = false;
        let mut code_start = 0;

        for (i, c) in line.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    code_start = i;
                    in_code = true;
                } else {
                    spans.push((code_start, i + 1)); // Include the closing backtick
                    in_code = false;
                }
            }
        }

        spans
    }
}

impl Rule for MD034NoBareUrls {
    fn name(&self) -> &'static str {
        "MD034"
    }

    fn description(&self) -> &'static str {
        "Bare URL detected"
    }

    fn check(&self, content: &str) -> LintResult {
        let line_index = LineIndex::new(content.to_string());
        // Fast path for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut in_html_block = false;

        for (i, line) in lines.iter().enumerate() {
            // Skip code blocks and HTML blocks
            if CODE_FENCE_REGEX.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if HTML_BLOCK_START.is_match(line) {
                in_html_block = true;
                continue;
            }

            if in_html_block && HTML_BLOCK_END.is_match(line) {
                in_html_block = false;
                continue;
            }

            if in_code_block || in_html_block {
                continue;
            }

            // Find inline code blocks first
            let inline_code_spans = self.compute_inline_code_spans(line);

            // Find URLs that are not in links or inline code
            let mut last_end = 0;
            while let Some(m) = URL_REGEX.find_at(line, last_end) {
                let url_start = m.start();
                let url_end = m.end();
                let url = &line[url_start..url_end];
                last_end = url_end;

                // Skip URLs in inline code blocks
                if inline_code_spans
                    .iter()
                    .any(|(start, end)| url_start >= *start && url_end <= *end)
                {
                    continue;
                }

                // Check if this URL is part of a link
                let mut in_link = false;
                let start_of_line = &line[..url_start];
                let end_of_line = &line[url_end..];

                // Check for Markdown link syntax: [text](url)
                if start_of_line.ends_with("](") && end_of_line.starts_with(")") {
                    in_link = true;
                }

                // Check for auto-link syntax: <url>
                if start_of_line.ends_with("<") && end_of_line.starts_with(">") {
                    in_link = true;
                }

                if !in_link {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: url_start + 1,
                        severity: Severity::Warning,
                        message: format!("URL '{}' should be enclosed in angle brackets", url),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, url_start + 1),
                            replacement: format!("<{}>", url),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content or content without URLs
        if content.is_empty() || (!content.contains("http") && !content.contains("ftp")) {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut in_html_block = false;

        for line in &lines {
            // Handle code blocks and HTML blocks
            if CODE_FENCE_REGEX.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if HTML_BLOCK_START.is_match(line) {
                in_html_block = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_html_block && HTML_BLOCK_END.is_match(line) {
                in_html_block = false;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block || in_html_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // Find inline code blocks first
            let inline_code_spans = self.compute_inline_code_spans(line);

            // Fix bare URLs
            let mut fixed_line = String::new();
            let mut last_end = 0;

            // Find and process all URLs in the line
            while let Some(m) = URL_REGEX.find_at(line, last_end) {
                let url_start = m.start();
                let url_end = m.end();
                let url = &line[url_start..url_end];

                // Skip URLs in inline code blocks
                if inline_code_spans
                    .iter()
                    .any(|(start, end)| url_start >= *start && url_end <= *end)
                {
                    // Add text up to the end of this URL
                    fixed_line.push_str(&line[last_end..url_end]);
                    last_end = url_end;
                    continue;
                }

                // Add the text between the last match and this match
                fixed_line.push_str(&line[last_end..url_start]);

                // Check if this URL is part of a link
                let mut in_link = false;
                let start_part = &line[..url_start];
                let end_part = &line[url_end..];

                // Check for Markdown link syntax: [text](url)
                if start_part.ends_with("](") && end_part.starts_with(")") {
                    in_link = true;
                }

                // Check for auto-link syntax: <url>
                if start_part.ends_with("<") && end_part.starts_with(">") {
                    in_link = true;
                }

                if in_link {
                    fixed_line.push_str(url);
                } else {
                    fixed_line.push_str(&format!("<{}>", url));
                }

                last_end = url_end;
            }

            // Add any remaining text
            fixed_line.push_str(&line[last_end..]);
            result.push_str(&fixed_line);
            result.push('\n');
        }

        // Remove the trailing newline if the original content didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
