use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug)]
pub struct MD033NoInlineHtml {
    allowed_elements: Vec<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        Self {
            allowed_elements: Vec::new(),
        }
    }
}

impl MD033NoInlineHtml {
    pub fn new(allowed_elements: Vec<String>) -> Self {
        Self { allowed_elements }
    }

    fn find_html_tags(&self, line: &str) -> Vec<(String, usize)> {
        let mut tags = Vec::new();
        // Match both opening and closing tags
        let re = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)(?:\s+[^>]*)?(/?)>").unwrap();
        
        for cap in re.captures_iter(line) {
            let tag_name = cap[2].to_string().to_lowercase();
            let position = cap.get(0).unwrap().start();
            
            if !self.allowed_elements.contains(&tag_name) {
                tags.push((tag_name, position));
            }
        }
        
        tags
    }
    
    fn is_in_code_block(&self, lines: &[&str], current_line: usize) -> bool {
        let mut in_code_block = false;
        let code_fence_re = Regex::new(r"^```|^~~~").unwrap();
        
        for i in 0..=current_line {
            if code_fence_re.is_match(lines[i].trim()) {
                in_code_block = !in_code_block;
            }
        }
        
        in_code_block
    }
    
    fn is_in_code_span(&self, line: &str, position: usize) -> bool {
        let mut code_span_starts = Vec::new();
        let mut code_span_ends = Vec::new();
        
        // First, identify all code spans in the line
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            if chars[i] == '`' {
                let start_pos = i;
                i += 1;
                
                // Handle cases with multiple backticks
                while i < chars.len() && chars[i] == '`' {
                    i += 1;
                }
                
                // Find the closing backticks
                let backtick_count = i - start_pos;
                let mut j = i;
                
                while j < chars.len() {
                    if chars[j] == '`' {
                        // Count consecutive backticks
                        let _end_start = j;
                        let mut end_count = 1;
                        j += 1;
                        
                        while j < chars.len() && chars[j] == '`' {
                            end_count += 1;
                            j += 1;
                        }
                        
                        // If we found matching backticks, record the code span
                        if end_count == backtick_count {
                            code_span_starts.push(start_pos);
                            code_span_ends.push(j - 1);
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
            } else {
                i += 1;
            }
        }
        
        // Now check if the position is inside any code span
        for (start, end) in code_span_starts.iter().zip(code_span_ends.iter()) {
            if position > *start && position < *end {
                return true;
            }
        }
        
        false
    }
}

impl Rule for MD033NoInlineHtml {
    fn name(&self) -> &'static str {
        "MD033"
    }

    fn description(&self) -> &'static str {
        "Inline HTML"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip checking inside code blocks
            if self.is_in_code_block(&lines, line_num) {
                continue;
            }

            // Check for HTML tags in the line
            for (tag, position) in self.find_html_tags(line) {
                // Skip if inside a code span
                if self.is_in_code_span(line, position) {
                    continue;
                }
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: position + 1,
                    message: format!("HTML tag '{}' is not allowed", tag),
                    fix: None,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = content.to_string();
        let html_tag_re = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)(?:\s+[^>]*)?(/?)>").unwrap();
        
        // Replace HTML tags with empty strings, preserving content
        result = html_tag_re.replace_all(&result, "").to_string();
        
        Ok(result)
    }
} 