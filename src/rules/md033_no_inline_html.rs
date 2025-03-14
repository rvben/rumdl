use crate::rule::{LintError, LintResult, LintWarning, Rule};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Optimized regex patterns
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)(?:\s+[^>]*)?(/?)>").unwrap();
    static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^```|^~~~").unwrap();
    static ref BACKTICK_PATTERN: Regex = Regex::new(r"(`+)(.+?)(`+)").unwrap();
}

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

    // Optimized HTML tag finding using cached regex
    fn find_html_tags(&self, line: &str) -> Vec<(String, usize)> {
        // Skip processing if no '<' character in line (quick check)
        if !line.contains('<') {
            return Vec::new();
        }
        
        let mut tags = Vec::new();
        
        for cap in HTML_TAG_PATTERN.captures_iter(line) {
            let tag_name = cap[2].to_string().to_lowercase();
            let position = cap.get(0).unwrap().start();
            
            if !self.allowed_elements.contains(&tag_name) {
                tags.push((tag_name, position));
            }
        }
        
        tags
    }
    
    // Pre-compute code block regions to avoid repetitive processing
    fn precompute_code_blocks(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_code_block = false;
        let mut code_block_map = vec![false; lines.len()];
        
        for (i, line) in lines.iter().enumerate() {
            // Check for code fence markers
            if CODE_FENCE_PATTERN.is_match(line.trim()) {
                in_code_block = !in_code_block;
            }
            
            code_block_map[i] = in_code_block;
        }
        
        code_block_map
    }
    
    // Optimized code span detection using cached ranges
    fn compute_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        // Skip processing if no backtick character (quick check)
        if !line.contains('`') {
            return Vec::new();
        }
        
        let mut spans = Vec::new();
        
        // First try using regex for simple code spans
        for cap in BACKTICK_PATTERN.captures_iter(line) {
            if let (Some(start), Some(end)) = (cap.get(0).map(|m| m.start()), cap.get(0).map(|m| m.end())) {
                spans.push((start, end));
            }
        }
        
        // If no spans found with regex (could be complex or unmatched), fallback to manual parsing
        if spans.is_empty() {
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
                                spans.push((start_pos, j));
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
        }
        
        spans
    }
    
    // Check if position is inside any code span
    fn is_in_code_span(&self, code_spans: &[(usize, usize)], position: usize) -> bool {
        code_spans.iter().any(|(start, end)| position > *start && position < *end)
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
        // Early return for empty content or content without HTML tags
        if content.is_empty() || !content.contains('<') {
            return Ok(vec![]);
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute code block regions (more efficient than checking for each line)
        let code_block_map = self.precompute_code_blocks(&lines);
        
        for (line_num, line) in lines.iter().enumerate() {
            // Skip empty lines or lines without < (quick check)
            if line.trim().is_empty() || !line.contains('<') {
                continue;
            }
            
            // Skip checking inside code blocks
            if code_block_map[line_num] {
                continue;
            }

            // Pre-compute code spans in the line
            let code_spans = self.compute_code_spans(line);
            
            // Check for HTML tags in the line
            for (tag, position) in self.find_html_tags(line) {
                // Skip if inside a code span
                if !code_spans.is_empty() && self.is_in_code_span(&code_spans, position) {
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
        // Early return if no HTML tags
        if !content.contains('<') {
            return Ok(content.to_string());
        }
        
        // Replace HTML tags with empty strings, preserving content
        let result = HTML_TAG_PATTERN.replace_all(content, "").to_string();
        
        Ok(result)
    }
} 