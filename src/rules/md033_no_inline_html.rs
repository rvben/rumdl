use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::time::Instant;

lazy_static! {
    // Refined regex patterns with better performance characteristics
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(?:(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*)?(?:(/?)>)?)").unwrap();

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
    allowed: HashSet<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        Self::new()
    }
}

impl MD033NoInlineHtml {
    pub fn new() -> Self {
        Self {
            allowed: HashSet::new(),
        }
    }

    pub fn with_allowed(allowed_vec: Vec<String>) -> Self {
        Self {
            allowed: allowed_vec.into_iter().collect(),
        }
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

    // Efficient check for allowed tags using HashSet
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

        self.allowed.contains(tag_name)
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
        let structure = DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        if content.is_empty() || !content.contains('<') || !HTML_TAG_QUICK_CHECK.is_match(content) || !structure.has_html {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;

            if line.trim().is_empty() || structure.is_in_code_block(line_num) || !line.contains('<') || HTML_COMMENT_PATTERN.is_match(line) {
                continue;
            }

            for cap in HTML_TAG_FINDER.captures_iter(line) {
                let tag_match = cap.get(0).unwrap();
                let html_tag = tag_match.as_str();
                let start_byte_offset_in_line = tag_match.start();
                let end_byte_offset_in_line = tag_match.end();
                let start_col = line[..start_byte_offset_in_line].chars().count() + 1;

                if self.is_html_comment(html_tag) || self.is_in_markdown_link(line, start_byte_offset_in_line) || structure.is_in_code_span(line_num, start_col) {
                    continue;
                }

                if !self.is_tag_allowed(html_tag) {
                    if let Some(line_start_byte) = line_index.get_line_start_byte(line_num) {
                        let global_start_byte = line_start_byte + start_byte_offset_in_line;
                        let global_end_byte = line_start_byte + end_byte_offset_in_line;
                        let warning_range = global_start_byte..global_end_byte;

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num,
                            column: start_col,
                            message: format!("Found inline HTML tag: {}", html_tag),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: warning_range,
                                replacement: String::new(),
                            }),
                        });
                    } else {
                        eprintln!("Warning: Could not find line start for line {} in MD033", line_num);
                    }
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

    fn as_any(&self) -> &dyn std::any::Any { self }
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
