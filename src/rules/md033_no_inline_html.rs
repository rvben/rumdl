use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::range_utils::LineIndex;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use regex::Regex;
use lazy_static::lazy_static;
use std::collections::HashSet;
use once_cell::sync::Lazy;

lazy_static! {
    // Refined regex patterns with better performance characteristics
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*)?(/?)>").unwrap();
    
    // Pattern to quickly check for HTML tag presence (much faster than the full pattern)
    static ref HTML_TAG_QUICK_CHECK: Regex = Regex::new(r"</?[a-zA-Z]").unwrap();
    
    // Code fence patterns - using basic string patterns for fast detection
    static ref CODE_FENCE_START: Regex = Regex::new(r"^(```|~~~)").unwrap();
    
    // HTML/Markdown comment pattern
    static ref HTML_COMMENT_PATTERN: Regex = Regex::new(r"<!--.*?-->").unwrap();
    
    // Regex to find HTML tags with proper context awareness
    static ref HTML_TAG_FINDER: Regex = Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();
}

// Non-regex patterns for faster checks
static BACKTICK: Lazy<char> = Lazy::new(|| '`');
static MARKDOWN_LINK_START: Lazy<&str> = Lazy::new(|| "](");

#[derive(Debug)]
pub struct MD033NoInlineHtml {
    allowed: Vec<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        Self::new()
    }
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

    // Very fast code block detection - optimized for performance
    #[inline]
    fn detect_code_blocks(&self, content: &str) -> HashSet<usize> {
        let mut code_block_lines = HashSet::new();
        let mut in_code_block = false;
        let mut fence_marker: Option<&str> = None;
        
        for (i, line) in content.lines().enumerate() {
            // Skip processing if already known to be in a code block
            if in_code_block {
                code_block_lines.insert(i);
                
                // Check if this line ends the code block
                if let Some(marker) = fence_marker {
                    if line.trim().starts_with(marker) {
                        in_code_block = false;
                        fence_marker = None;
                        // Don't continue here - the closing fence is part of the code block
                    }
                }
                continue;
            }
            
            // Fast literal check for fence markers
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_code_block = true;
                fence_marker = Some("```");
                code_block_lines.insert(i);
            } else if trimmed.starts_with("~~~") {
                in_code_block = true;
                fence_marker = Some("~~~");
                code_block_lines.insert(i);
            }
        }
        
        code_block_lines
    }

    // Optimized code span detection using direct string scanning
    #[inline]
    fn is_in_code_span(&self, line: &str, position: usize) -> bool {
        let mut in_code_span = false;
        let mut code_start = 0;
        
        // Fast linear scan which avoids regex overhead
        for (pos, c) in line.char_indices() {
            if c == *BACKTICK {
                if !in_code_span {
                    in_code_span = true;
                    code_start = pos;
                } else {
                    // Found end of code span, check if position is within
                    if position >= code_start && position <= pos {
                        return true;
                    }
                    in_code_span = false;
                }
            }
            
            // If we've passed the position and not in a code span, we can return early
            if pos > position && !in_code_span {
                return false;
            }
        }
        
        // Check if position is in an unclosed code span
        in_code_span && position >= code_start
    }

    // Efficient check for allowed tags
    #[inline]
    fn is_tag_allowed(&self, tag: &str) -> bool {
        if self.allowed.is_empty() {
            return false;
        }
        
        // Extract tag name without angle brackets, attributes, or closing slash
        let tag_name = if tag.starts_with("</") {
            // Closing tag
            tag.trim_start_matches("</").trim_end_matches('>')
        } else if tag.ends_with("/>") {
            // Self-closing tag
            let inner = tag.trim_start_matches('<').trim_end_matches("/>");
            inner.split_whitespace().next().unwrap_or("")
        } else {
            // Opening tag
            let inner = tag.trim_start_matches('<').trim_end_matches('>');
            inner.split_whitespace().next().unwrap_or("")
        };
        
        self.allowed.iter().any(|a| a == tag_name)
    }
    
    // Check if a position is part of a markdown link
    #[inline]
    fn is_in_markdown_link(&self, line: &str, tag_start: usize) -> bool {
        // Very fast check for common case - looking for ]( before the <
        if tag_start >= 2 {
            let prefix = &line[..tag_start];
            if prefix.ends_with(*MARKDOWN_LINK_START) {
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
        
        // Very fast early return - no angle brackets, no HTML
        if !content.contains('<') {
            return Ok(Vec::new());
        }
        
        // Quick check for HTML tag patterns before doing detailed processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());
        
        // Pre-compute code blocks for fast lookup
        let code_block_lines = self.detect_code_blocks(content);

        // Process each line
        for (i, line) in content.lines().enumerate() {
            // Early skip optimizations
            if line.trim().is_empty() {
                continue;
            }

            // Skip lines in code blocks
            if code_block_lines.contains(&i) {
                continue;
            }
            
            // Skip if no angle brackets in this line
            if !line.contains('<') {
                continue;
            }
            
            // Skip if line has HTML comments
            if HTML_COMMENT_PATTERN.is_match(line) {
                continue;
            }

            // Find potential HTML tags
            for cap in HTML_TAG_FINDER.captures_iter(line) {
                let html_tag = cap.get(0).unwrap().as_str();
                let start_pos = cap.get(0).unwrap().start();
                
                // Skip HTML comments
                if self.is_html_comment(html_tag) {
                    continue;
                }
                
                // Skip if part of markdown link
                if self.is_in_markdown_link(line, start_pos) {
                    continue;
                }
                
                // Skip if in code span
                if line.contains(*BACKTICK) && self.is_in_code_span(line, start_pos) {
                    continue;
                }
                
                // Check if tag is allowed
                if !self.is_tag_allowed(html_tag) {
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
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

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        // Very fast early return - no angle brackets, no HTML
        if !content.contains('<') {
            return Ok(Vec::new());
        }
        
        // Quick check for HTML tag patterns before doing detailed processing
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        // Early return if no HTML tags are detected or this is explicitly HTML content
        if !structure.has_html {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        // Process each line
        for (i, line) in content.lines().enumerate() {
            // Early skip optimizations
            if line.trim().is_empty() {
                continue;
            }

            // Skip lines in code blocks - using document structure
            if structure.is_in_code_block(i + 1) {
                continue;
            }
            
            // Skip if no angle brackets in this line
            if !line.contains('<') {
                continue;
            }
            
            // Skip if line has HTML comments
            if HTML_COMMENT_PATTERN.is_match(line) {
                continue;
            }

            // Find potential HTML tags
            for cap in HTML_TAG_FINDER.captures_iter(line) {
                let html_tag = cap.get(0).unwrap().as_str();
                let start_pos = cap.get(0).unwrap().start();
                
                // Skip HTML comments
                if self.is_html_comment(html_tag) {
                    continue;
                }
                
                // Skip if part of markdown link
                if self.is_in_markdown_link(line, start_pos) {
                    continue;
                }
                
                // Skip if in code span - using document structure
                if structure.is_in_code_span(i + 1, start_pos + 1) {
                    continue;
                }
                
                // Check if tag is allowed
                if !self.is_tag_allowed(html_tag) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
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
        // Early returns for common cases
        if content.is_empty() {
            return Ok(String::new());
        }
        
        if !content.contains('<') {
            return Ok(content.to_string());
        }
        
        if !HTML_TAG_QUICK_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute code blocks
        let code_block_lines = self.detect_code_blocks(content);

        for (i, line) in lines.iter().enumerate() {
            // Keep code blocks unchanged
            if code_block_lines.contains(&i) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Skip HTML transformation for lines without angle brackets
            if !line.contains('<') {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Handle HTML comments specially - keep them
            if HTML_COMMENT_PATTERN.is_match(line) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // For lines with code spans, process carefully
            if line.contains(*BACKTICK) {
                let mut processed_line = String::with_capacity(line.len());
                let mut last_pos = 0;
                let mut in_code_span = false;
                let mut code_start = 0;
                
                for (pos, c) in line.char_indices() {
                    if c == *BACKTICK {
                        if !in_code_span {
                            // Starting code span - process text before it
                            if pos > last_pos {
                                let segment = &line[last_pos..pos];
                                let fixed = self.fix_html_in_text(segment)?;
                                processed_line.push_str(&fixed);
                            }
                            processed_line.push('`');
                            in_code_span = true;
                            code_start = pos + 1;
                        } else {
                            // Ending code span - add content unchanged
                            processed_line.push_str(&line[code_start..pos]);
                            processed_line.push('`');
                            in_code_span = false;
                            last_pos = pos + 1;
                        }
                    } else if in_code_span {
                        // Inside code span - do nothing, will add content in batch
                    }
                }
                
                // Add any remaining content
                if last_pos < line.len() {
                    if in_code_span {
                        // Unclosed code span - add as is
                        processed_line.push_str(&line[code_start..]);
                    } else {
                        // Regular text after last code span
                        let segment = &line[last_pos..];
                        let fixed = self.fix_html_in_text(segment)?;
                        processed_line.push_str(&fixed);
                    }
                }
                
                result.push_str(&processed_line);
            } else {
                // Standard case - fix HTML tags
                let fixed_line = self.fix_html_in_text(line)?;
                result.push_str(&fixed_line);
            }
            
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
    
    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Html
    }
    
    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !content.contains('<') || !HTML_TAG_QUICK_CHECK.is_match(content)
    }
}

impl MD033NoInlineHtml {
    // Helper method to fix HTML in text segments
    #[inline]
    fn fix_html_in_text(&self, text: &str) -> Result<String, LintError> {
        // Skip text without HTML or potential HTML
        if !text.contains('<') || !HTML_TAG_QUICK_CHECK.is_match(text) {
            return Ok(text.to_string());
        }
        
        let mut result = String::with_capacity(text.len());
        let mut last_pos = 0;
        
        // Find all HTML tags and process
        for cap in HTML_TAG_FINDER.captures_iter(text) {
            let html_tag = cap.get(0).unwrap().as_str();
            let start_pos = cap.get(0).unwrap().start();
            let end_pos = cap.get(0).unwrap().end();
            
            // Skip HTML comments or allowed tags
            if self.is_html_comment(html_tag) || self.is_tag_allowed(html_tag) {
                result.push_str(&text[last_pos..end_pos]);
                last_pos = end_pos;
                continue;
            }
            
            // Skip markdown links
            if self.is_in_markdown_link(text, start_pos) {
                result.push_str(&text[last_pos..end_pos]);
                last_pos = end_pos;
                continue;
            }
            
            // Add text before the tag
            result.push_str(&text[last_pos..start_pos]);
            
            // Skip the tag entirely (removing it)
            last_pos = end_pos;
        }
        
        // Add any remaining text
        if last_pos < text.len() {
            result.push_str(&text[last_pos..]);
        }
        
        Ok(result)
    }
}

impl DocumentStructureExtensions for MD033NoInlineHtml {
    fn has_relevant_elements(&self, content: &str, _doc_structure: &DocumentStructure) -> bool {
        // Rule is only relevant if content contains potential HTML tags
        content.contains('<') && content.contains('>')
    }
}
