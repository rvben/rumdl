use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Cached regex patterns for better performance
    static ref URL_PATTERN: Regex = Regex::new(r#"(?:https?|ftp)://[^\s<>\[\]()'"]+[^\s<>\[\]()'".,]"#).unwrap();
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[.*?\]\((?P<url>.*?)\)").unwrap();
    static ref ANGLE_LINK_PATTERN: Regex = Regex::new(r"<(?:https?|ftp)://[^>]+>").unwrap();
    static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(?:\s*)(?:```|~~~)").unwrap();
}

#[derive(Debug, Default)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    // Pre-compute code blocks to avoid repetitive processing
    fn precompute_code_blocks(&self, content: &str) -> Vec<bool> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut code_block_map = vec![false; lines.len()];
        
        for (i, line) in lines.iter().enumerate() {
            if CODE_FENCE_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
            }
            code_block_map[i] = in_code_block;
        }
        
        code_block_map
    }

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
                    spans.push((code_start, i));
                    in_code = false;
                }
            }
        }
        
        spans
    }
    
    // Check if position is within a code span
    fn is_in_inline_code(&self, code_spans: &[(usize, usize)], position: usize) -> bool {
        code_spans.iter().any(|(start, end)| position >= *start && position < *end)
    }
    
    // Optimized link detection with cached regex
    fn compute_markdown_link_spans(&self, line: &str) -> Vec<(usize, usize)> {
        if !line.contains('[') && !line.contains('<') {
            return Vec::new();
        }
        
        let mut spans = Vec::new();
        
        // Check for standard markdown links: [text](url)
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(line) {
            if let Some(url_match) = cap.name("url") {
                spans.push((url_match.start(), url_match.end()));
            }
        }
        
        // Check for angle-bracket enclosed URLs: <http://example.com>
        for cap in ANGLE_LINK_PATTERN.find_iter(line) {
            spans.push((cap.start(), cap.end()));
        }
        
        spans
    }

    // Optimized URL finding with pre-computed regions to avoid
    fn find_bare_urls(&self, line: &str, code_spans: &[(usize, usize)], link_spans: &[(usize, usize)]) -> Vec<(usize, String)> {
        // Early return if no URL protocol identifier in line
        if !line.contains("http") && !line.contains("ftp") {
            return Vec::new();
        }
        
        let mut urls = Vec::new();
        
        for cap in URL_PATTERN.find_iter(line) {
            let url = cap.as_str();
            let position = cap.start();
            
            // Skip URLs that are in inline code or markdown links
            if !self.is_in_inline_code(code_spans, position) && 
               !link_spans.iter().any(|(start, end)| position >= *start && position < *end) {
                urls.push((position, url.to_string()));
            }
        }

        urls
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
        // Early return for empty content or content without URLs
        if content.is_empty() || (!content.contains("http") && !content.contains("ftp")) {
            return Ok(vec![]);
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute code block regions
        let code_block_map = self.precompute_code_blocks(content);

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines without URLs
            if !line.contains("http") && !line.contains("ftp") {
                continue;
            }
            
            // Skip code blocks
            if code_block_map[line_num] {
                continue;
            }
            
            // Compute code spans and link spans once per line
            let code_spans = self.compute_inline_code_spans(line);
            let link_spans = self.compute_markdown_link_spans(line);
            
            // Find bare URLs
            for (col, url) in self.find_bare_urls(line, &code_spans, &link_spans) {
                warnings.push(LintWarning {
                    message: format!("Bare URL should be enclosed in angle brackets or as a proper Markdown link: {}", url),
                    line: line_num + 1,
                    column: col + 1,
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: col + 1,
                        replacement: format!("<{}>", url),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content or content without URLs
        if content.is_empty() || (!content.contains("http") && !content.contains("ftp")) {
            return Ok(content.to_string());
        }
        
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len() + 100); // Allocate extra space for angle brackets
        
        // Pre-compute code block regions
        let code_block_map = self.precompute_code_blocks(content);

        for (i, line) in lines.iter().enumerate() {
            // Skip processing for lines without URLs
            if !line.contains("http") && !line.contains("ftp") {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }
            
            let mut fixed_line = line.to_string();
            
            if !code_block_map[i] {
                // Compute code spans and link spans once per line
                let code_spans = self.compute_inline_code_spans(line);
                let link_spans = self.compute_markdown_link_spans(line);
                
                // Find and fix bare URLs (process from right to left to maintain correct indices)
                let mut urls = self.find_bare_urls(line, &code_spans, &link_spans);
                urls.reverse();
                
                for (col, url) in urls {
                    fixed_line.replace_range(col..col + url.len(), &format!("<{}>", url));
                }
            }
            
            result.push_str(&fixed_line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 