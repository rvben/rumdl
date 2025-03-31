use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use regex::Regex;
use lazy_static::lazy_static;
use std::collections::HashSet;
use once_cell::sync::Lazy;

lazy_static! {
    // Optimized regex patterns with improved precision
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*)?(/?)>").unwrap();
    // Pattern to quickly check for HTML tag presence (faster than the full pattern)
    static ref HTML_TAG_QUICK_CHECK: Regex = Regex::new(r"</?[a-zA-Z]").unwrap();
    // Code fence patterns
    static ref CODE_FENCE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    // HTML/Markdown comment pattern
    static ref HTML_COMMENT_PATTERN: Regex = Regex::new(r"<!--[\s\S]*?-->").unwrap();
    // Regex to find HTML tags with proper context awareness
    static ref HTML_TAG_FINDER: Regex = Regex::new(r"<[^>]+>").unwrap();
}

// Non-regex patterns for fast checks
static BACKTICK: Lazy<char> = Lazy::new(|| '`');
static LESS_THAN: Lazy<char> = Lazy::new(|| '<');
static MARKDOWN_LINK_START: Lazy<&str> = Lazy::new(|| "](");

#[derive(Debug)]
pub struct MD033NoInlineHtml {
    allowed: Vec<String>,
}

impl MD033NoInlineHtml {
    pub fn new() -> Self {
        Self {
            allowed: Vec::new(),
        }
    }

    pub fn with_allowed(allowed: Vec<String>) -> Self {
        Self { allowed }
    }

    pub fn default() -> Self {
        Self::new()
    }

    // Optimized code block detection
    #[inline]
    fn detect_code_blocks(&self, content: &str) -> HashSet<usize> {
        let mut code_block_lines = HashSet::new();
        let mut in_code_block = false;
        let mut fence_marker = String::new();
        
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            
            // Fast check for code fence markers
            if (trimmed.starts_with("```") || trimmed.starts_with("~~~")) && 
                (trimmed == "```" || trimmed == "~~~" || 
                 (trimmed.starts_with("```") && !trimmed[3..].contains('`')) || 
                 (trimmed.starts_with("~~~") && !trimmed[3..].contains('~'))) {
                
                if !in_code_block {
                    // Start code block
                    fence_marker = if trimmed.starts_with("```") { "```".to_string() } else { "~~~".to_string() };
                    in_code_block = true;
                } else if trimmed.starts_with(&fence_marker) {
                    // End code block
                    in_code_block = false;
                }
            }
            
            if in_code_block {
                code_block_lines.insert(i);
            }
        }
        
        code_block_lines
    }

    // Fast check for code spans in a line
    #[inline]
    fn get_code_span_positions(&self, line: &str) -> Vec<(usize, usize)> {
        let mut positions = Vec::new();
        let mut start_pos = None;
        let mut backtick_count = 0;
        
        for (i, c) in line.char_indices() {
            if c == *BACKTICK {
                if start_pos.is_none() {
                    // Start of potential code span
                    start_pos = Some(i);
                    backtick_count = 1;
                } else if backtick_count > 0 {
                    // Continuing backticks
                    backtick_count += 1;
                } else {
                    // End of code span
                    if let Some(start) = start_pos {
                        positions.push((start, i));
                        start_pos = None;
                    }
                }
            } else if let Some(_start) = start_pos {
                if backtick_count > 0 {
                    // End of backticks, now in code content
                    backtick_count = 0;
                } else if c == *LESS_THAN && line[i..].contains('>') {
                    // Found a potential HTML tag in code span
                    // We'll keep collecting until the matching backtick
                }
            }
        }
        
        // Handle unclosed code spans (shouldn't happen in valid markdown)
        if let Some(start) = start_pos {
            if backtick_count == 0 {
                positions.push((start, line.len()));
            }
        }
        
        positions
    }

    // Check if position is inside a code span
    #[inline]
    fn is_in_code_span(&self, line: &str, position: usize) -> bool {
        let spans = self.get_code_span_positions(line);
        
        // Binary search for faster lookup with many code spans
        if spans.len() > 10 {
            let mut left = 0;
            let mut right = spans.len();
            
            while left < right {
                let mid = (left + right) / 2;
                let (start, end) = spans[mid];
                
                if position < start {
                    right = mid;
                } else if position > end {
                    left = mid + 1;
                } else {
                    return true;
                }
            }
            
            false
        } else {
            // Linear search for fewer code spans
            spans.iter().any(|(start, end)| position >= *start && position <= *end)
        }
    }

    // Check if a tag is allowed in the configuration
    #[inline]
    fn is_tag_allowed(&self, tag: &str) -> bool {
        if self.allowed.is_empty() {
            return false;
        }
        
        if tag.starts_with("</") {
            // Check closing tags without the </...>
            let tag_name = tag.trim_start_matches("</").trim_end_matches('>');
            self.allowed.iter().any(|a| a == tag_name)
        } else {
            // Check opening tags without the <...>
            let tag_name = if tag.ends_with("/>") {
                // Self-closing tag
                tag.trim_start_matches('<').trim_end_matches("/>")
            } else {
                tag.trim_start_matches('<').trim_end_matches('>')
            };
            
            // Extract just the tag name without attributes
            let tag_name = tag_name.split_whitespace().next().unwrap_or("");
            self.allowed.iter().any(|a| a == tag_name)
        }
    }
    
    // Check if a potential HTML tag is actually part of a markdown link
    #[inline]
    fn is_in_markdown_link(&self, line: &str, tag_start: usize) -> bool {
        // Quick check using substring search instead of regex
        if tag_start > 2 {
            let before_tag = &line[..tag_start];
            if before_tag.contains('[') && before_tag.ends_with(*MARKDOWN_LINK_START) {
                // This is likely a markdown link with angle brackets
                return true;
            }
        }
        
        false
    }

    // Check if a tag is an HTML comment
    #[inline]
    fn is_html_comment(&self, tag: &str) -> bool {
        tag.starts_with("<!--") && tag.ends_with("-->")
    }
}

impl Rule for MD033NoInlineHtml {
    fn name(&self) -> &'static str {
        "MD033"
    }

    fn description(&self) -> &'static str {
        "Inline HTML is not allowed"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        // Early return for content without any HTML tags or potential HTML tags
        if !content.contains('<') {
            return Ok(Vec::new());
        }
        
        // Quick check for HTML tag patterns before detailed processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());
        
        // Pre-compute which lines are in code blocks for faster lookup
        let code_block_lines = self.detect_code_blocks(content);

        // Process content line by line
        for (i, line) in content.lines().enumerate() {
            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Skip lines in code blocks
            if code_block_lines.contains(&i) {
                continue;
            }
            
            // Check for HTML comments early and skip them
            if HTML_COMMENT_PATTERN.is_match(line) {
                continue;
            }

            // Find all potential HTML tags in the line
            for cap in HTML_TAG_FINDER.captures_iter(line) {
                let html_tag = cap.get(0).unwrap().as_str();
                let start_pos = cap.get(0).unwrap().start();
                
                // Skip known non-HTML patterns
                
                // Skip HTML comments
                if self.is_html_comment(html_tag) {
                    continue;
                }
                
                // Skip if this is part of a markdown link with angle brackets
                if self.is_in_markdown_link(line, start_pos) {
                    continue;
                }
                
                // Skip if in code span
                if line.contains(*BACKTICK) && self.is_in_code_span(line, start_pos) {
                    continue;
                }
                
                // If tag is not in allowed list, add a warning
                if !self.is_tag_allowed(html_tag) {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: start_pos + 1,
                        message: format!("Found inline HTML tag: {}", html_tag),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, start_pos + 1),
                            replacement: String::new(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Early return for content without any HTML tags
        if !content.contains('<') {
            return Ok(content.to_string());
        }
        
        // Quick check for HTML tag patterns before detailed processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute which lines are in code blocks for faster lookup
        let code_block_lines = self.detect_code_blocks(content);

        for (i, line) in lines.iter().enumerate() {
            // Keep lines in code blocks unchanged
            if code_block_lines.contains(&i) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Process lines with potential HTML tags
            let current_line = line.to_string();
            
            // Handle HTML comments specially - keep them
            if HTML_COMMENT_PATTERN.is_match(&current_line) {
                result.push_str(&current_line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Handle complex lines with code spans
            if current_line.contains(*BACKTICK) {
                let spans = self.get_code_span_positions(&current_line);
                if !spans.is_empty() {
                    // Preserve code spans while fixing HTML tags
                    let mut processed_line = String::with_capacity(current_line.len());
                    let mut last_end = 0;
                    
                    for (start, end) in spans {
                        // Fix part before code span
                        if last_end < start {
                            let part = &current_line[last_end..start];
                            let fixed_part = self.fix_html_in_text(part)?;
                            processed_line.push_str(&fixed_part);
                        }
                        
                        // Keep code span unchanged
                        processed_line.push_str(&current_line[start..=end]);
                        last_end = end + 1;
                    }
                    
                    // Fix part after last code span
                    if last_end < current_line.len() {
                        let part = &current_line[last_end..];
                        let fixed_part = self.fix_html_in_text(part)?;
                        processed_line.push_str(&fixed_part);
                    }
                    
                    result.push_str(&processed_line);
                    if i < lines.len() - 1 {
                        result.push('\n');
                    }
                    continue;
                }
            }
            
            // Standard case - fix HTML tags in the line
            let fixed_line = self.fix_html_in_text(&current_line)?;
            result.push_str(&fixed_line);
            
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
}

impl MD033NoInlineHtml {
    // Helper method to fix HTML in text segments
    #[inline]
    fn fix_html_in_text(&self, text: &str) -> Result<String, LintError> {
        let mut result = text.to_string();
        
        // Skip if no HTML tags
        if !text.contains('<') || !HTML_TAG_QUICK_CHECK.is_match(text) {
            return Ok(result);
        }
        
        // Fix HTML tags by removing them while preserving content
        loop {
            if let Some(cap) = HTML_TAG_FINDER.captures(&result) {
                let html_tag = cap.get(0).unwrap().as_str();
                let start_pos = cap.get(0).unwrap().start();
                
                // Skip HTML comments
                if self.is_html_comment(html_tag) {
                    break;
                }
                
                // Skip if part of a markdown link
                if self.is_in_markdown_link(&result, start_pos) {
                    // Advance past this tag
                    if let Some(end_pos) = result[start_pos..].find('>') {
                        result = format!("{}{}", &result[..start_pos], &result[start_pos + end_pos + 1..]);
                    } else {
                        break;
                    }
                    continue;
                }
                
                // Skip if tag is allowed
                if self.is_tag_allowed(html_tag) {
                    break;
                }
                
                // Remove the HTML tag
                result = result.replacen(html_tag, "", 1);
            } else {
                break;
            }
        }
        
        Ok(result)
    }
}
