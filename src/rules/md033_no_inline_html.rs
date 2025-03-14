use crate::rule::{LintError, LintResult, LintWarning, Rule};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Optimized regex patterns with improved precision
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*)?(/?)>").unwrap();
    static ref BACKTICK_PATTERN: Regex = Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap();
    // Pattern to quickly check for HTML tag presence (faster than the full pattern)
    static ref HTML_TAG_QUICK_CHECK: Regex = Regex::new(r"</?[a-zA-Z]").unwrap();
    // Code fence patterns
    static ref CODE_FENCE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
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
    #[inline]
    fn find_html_tags(&self, line: &str) -> Vec<(String, usize)> {
        let _timer = crate::profiling::ScopedTimer::new("MD033_find_html_tags");
        
        // Skip processing if no '<' character in line (quick check)
        if !line.contains('<') {
            return Vec::new();
        }
        
        // Quick check with a simpler pattern before using the more complex one
        if !HTML_TAG_QUICK_CHECK.is_match(line) {
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
    
    // Optimized code span detection using cached ranges
    #[inline]
    fn compute_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        let _timer = crate::profiling::ScopedTimer::new("MD033_compute_code_spans");
        
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
                                i = j; // Skip ahead
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
        
        // Sort spans by start position to enable binary search
        spans.sort_by_key(|span| span.0);
        spans
    }
    
    // Check if position is inside any code span using binary search for better performance
    #[inline]
    fn is_in_code_span(&self, code_spans: &[(usize, usize)], position: usize) -> bool {
        // Binary search optimization for large number of spans
        if code_spans.len() > 10 {
            // Find the span that could potentially contain the position
            match code_spans.binary_search_by(|span| {
                if position < span.0 {
                    std::cmp::Ordering::Greater
                } else if position > span.1 {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            }) {
                Ok(_) => return true, // Found exact match
                Err(_) => {
                    // Check adjacent spans
                    for span in code_spans {
                        if position > span.0 && position < span.1 {
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        
        // For small number of spans, linear search is faster
        code_spans.iter().any(|(start, end)| position > *start && position < *end)
    }
    
    // Simple function to detect code blocks
    fn detect_code_blocks(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_code_block = false;
        let mut code_block_map = vec![false; lines.len()];
        
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // Check for code fence markers
            if CODE_FENCE_START.is_match(trimmed) {
                in_code_block = !in_code_block;
            }
            
            code_block_map[i] = in_code_block;
        }
        
        code_block_map
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
        let _timer = crate::profiling::ScopedTimer::new("MD033_check");
        
        // Early return for empty content or content without HTML tags
        if content.is_empty() || !content.contains('<') {
            return Ok(vec![]);
        }
        
        // Quick check with simplified pattern before doing full processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(vec![]);
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Use our custom code block detection which is more reliable
        let code_block_map = self.detect_code_blocks(&lines);
        
        // Pre-compute all code spans to avoid repetitive computation
        let mut line_code_spans: Vec<Vec<(usize, usize)>> = Vec::with_capacity(lines.len());
        for line in &lines {
            line_code_spans.push(if line.contains('`') {
                self.compute_code_spans(line)
            } else {
                Vec::new()
            });
        }
        
        for (line_num, line) in lines.iter().enumerate() {
            // Skip empty lines or lines without < (quick check)
            if line.trim().is_empty() || !line.contains('<') {
                continue;
            }
            
            // Skip checking inside code blocks
            if code_block_map[line_num] {
                continue;
            }

            // Get pre-computed code spans for this line
            let code_spans = &line_code_spans[line_num];
            
            // Check for HTML tags in the line
            for (tag, position) in self.find_html_tags(line) {
                // Skip if inside a code span
                if !code_spans.is_empty() && self.is_in_code_span(code_spans, position) {
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
        let _timer = crate::profiling::ScopedTimer::new("MD033_fix");
        
        // Early return if no HTML tags
        if content.is_empty() || !content.contains('<') {
            return Ok(content.to_string());
        }
        
        // Quick check with simplified pattern before doing full processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(content.to_string());
        }
        
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());
        
        // Use our custom code block detection which is more reliable
        let code_block_map = self.detect_code_blocks(&lines);
        
        for (line_num, line) in lines.iter().enumerate() {
            // If the line is in a code block, preserve it
            if code_block_map[line_num] {
                result_lines.push(line.to_string());
                continue;
            }
            
            // If the line has no HTML-like content, preserve it
            if !line.contains('<') {
                result_lines.push(line.to_string());
                continue;
            }
            
            // For lines with potential HTML, handle code spans
            let code_spans = self.compute_code_spans(line);
            
            if code_spans.is_empty() {
                // No code spans, can safely replace HTML tags
                let fixed_line = HTML_TAG_PATTERN.replace_all(line, "").to_string();
                result_lines.push(fixed_line);
            } else {
                // Line has code spans, need more careful processing
                let mut current_line = line.to_string();
                
                // Find tags but don't replace those in code spans
                for cap in HTML_TAG_PATTERN.captures_iter(line) {
                    if let Some(whole_match) = cap.get(0) {
                        let position = whole_match.start();
                        
                        // If not in a code span, replace it
                        if !self.is_in_code_span(&code_spans, position) {
                            // Calculate adjusted positions after previous replacements
                            let adjusted_start = position;
                            let adjusted_end = whole_match.end();
                            
                            if adjusted_start < current_line.len() && adjusted_end <= current_line.len() {
                                current_line.replace_range(adjusted_start..adjusted_end, "");
                            }
                        }
                    }
                }
                
                result_lines.push(current_line);
            }
        }
        
        // Join the lines, preserving trailing newline if present
        if content.ends_with('\n') {
            Ok(format!("{}\n", result_lines.join("\n")))
        } else {
            Ok(result_lines.join("\n"))
        }
    }
} 