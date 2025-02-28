use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;
        for (i, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            if i + 1 == line_num {
                break;
            }
        }
        in_code_block
    }

    fn is_in_inline_code(&self, line: &str, position: usize) -> bool {
        let mut in_code = false;
        let mut code_start = 0;
        
        for (i, c) in line.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    code_start = i;
                    in_code = true;
                } else {
                    if position >= code_start && position < i {
                        return true;
                    }
                    in_code = false;
                }
            }
            
            if i >= position && !in_code {
                break;
            }
        }
        
        false
    }
    
    fn is_in_markdown_link(&self, line: &str, position: usize) -> bool {
        // Check for standard markdown links: [text](url)
        let link_re = Regex::new(r"\[.*?\]\((?P<url>.*?)\)").unwrap();
        for cap in link_re.captures_iter(line) {
            if let Some(url_match) = cap.name("url") {
                let start = url_match.start();
                let end = url_match.end();
                if position >= start && position < end {
                    return true;
                }
            }
        }
        
        // Check for angle-bracket enclosed URLs: <http://example.com>
        let angle_re = Regex::new(r"<(?:https?|ftp)://[^>]+>").unwrap();
        for cap in angle_re.find_iter(line) {
            let start = cap.start();
            let end = cap.end();
            if position >= start && position < end {
                return true;
            }
        }
        
        false
    }

    fn find_bare_urls(&self, line: &str) -> Vec<(usize, String)> {
        let mut urls = Vec::new();
        let url_re = Regex::new(r#"(?:https?|ftp)://[^\s<>\[\]()'"]+[^\s<>\[\]()'".,]"#).unwrap();

        for cap in url_re.find_iter(line) {
            let url = cap.as_str().to_string();
            let position = cap.start();
            
            // Skip URLs that are in inline code or markdown links
            if !self.is_in_inline_code(line, position) && !self.is_in_markdown_link(line, position) {
                urls.push((position, url));
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
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, line_num + 1) {
                for (col, url) in self.find_bare_urls(line) {
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
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let mut fixed_line = line.to_string();
            if !self.is_in_code_block(content, i + 1) {
                let mut urls = self.find_bare_urls(line);
                urls.reverse(); // Process URLs from right to left to maintain correct indices
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