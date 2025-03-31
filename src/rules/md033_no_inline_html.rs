use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Optimized regex patterns with improved precision
    static ref HTML_TAG_PATTERN: Regex = Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*)?(/?)>").unwrap();
    static ref BACKTICK_PATTERN: Regex = Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap();
    // Pattern to quickly check for HTML tag presence (faster than the full pattern)
    static ref HTML_TAG_QUICK_CHECK: Regex = Regex::new(r"</?[a-zA-Z]").unwrap();
    // Code fence patterns
    static ref CODE_FENCE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    // Pattern to match div tags with align attribute
    static ref DIV_ALIGN_PATTERN: Regex = Regex::new(r"^<div\s+align=").unwrap();
    // Markdown link pattern - links like [text](<url>) should not be considered HTML
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[.*?\]\(<.*?>\)").unwrap();
}

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

    // Simplify the implementation to focus on the core functionality
    fn is_in_code_block(&self, content: &str, line_number: usize) -> bool {
        let mut in_code_block = false;
        let mut fence_char = None;

        for (i, line) in content.lines().enumerate() {
            if i >= line_number {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if Some(&trimmed[..3]) == fence_char {
                    in_code_block = false;
                }
            }
        }

        in_code_block
    }

    fn contains_code_span(&self, line: &str) -> bool {
        line.contains('`')
    }

    fn is_tag_allowed(&self, tag: &str) -> bool {
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
    fn is_in_markdown_link(&self, line: &str, tag_start: usize) -> bool {
        let before_tag = &line[..tag_start];
        let after_tag_start = &line[tag_start..];
        
        // Check if there's a markdown link pattern that covers this position
        if let Some(m) = MARKDOWN_LINK_PATTERN.find(line) {
            // If the tag start position is within the markdown link range
            return tag_start >= m.start() && tag_start < m.end();
        }
        
        // Check for pattern [text](<url>)
        // Look backwards for '[' followed by text and ']('
        if before_tag.contains('[') && before_tag.ends_with("](") &&
           // Look forward for closing ')'
           after_tag_start.contains('>') && after_tag_start.contains(')') {
            return true;
        }
        
        false
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
        if content.is_empty() || !content.contains('<') {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        // Process content line by line
        for (i, line) in content.lines().enumerate() {
            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Skip lines in code blocks
            if self.is_in_code_block(content, i) {
                continue;
            }

            // If line has a code span, we need more careful processing
            if self.contains_code_span(line) {
                // Find all potential HTML tags in the line
                let re = Regex::new(r"<[^>]+>").unwrap();
                
                for cap in re.captures_iter(line) {
                    let html_tag = cap.get(0).unwrap().as_str();
                    let start_pos = cap.get(0).unwrap().start();
                    
                    // Skip if this is part of a markdown link with angle brackets
                    if self.is_in_markdown_link(line, start_pos) {
                        continue;
                    }
                    
                    // Calculate the byte position in the full content
                    let line_start_pos = content.lines()
                        .take(i)
                        .map(|l| l.len() + 1)
                        .sum::<usize>();
                    let _tag_pos = line_start_pos + start_pos;
                    
                    // Skip if in code span (simplified check - not perfect but handles most cases)
                    let mut backtick_count = 0;
                    
                    // Count backticks before the tag to determine if we're in a code span
                    for c in line[..start_pos].chars() {
                        if c == '`' {
                            backtick_count += 1;
                        }
                    }
                    
                    // If odd number of backticks before the tag, it's in a code span
                    if backtick_count % 2 == 1 {
                        continue;
                    }
                    
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
            } else {
                // No code spans, just check for HTML tags directly
                let re = Regex::new(r"<[^>]+>").unwrap();
                
                for cap in re.captures_iter(line) {
                    let html_tag = cap.get(0).unwrap().as_str();
                    let start_pos = cap.get(0).unwrap().start();
                    
                    // Skip if this is part of a markdown link with angle brackets
                    if self.is_in_markdown_link(line, start_pos) {
                        continue;
                    }
                    
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
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() || !content.contains('<') {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Keep lines in code blocks unchanged
            if self.is_in_code_block(content, i) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Handle lines with code spans carefully
            if self.contains_code_span(line) {
                let mut fixed_line = line.to_string();
                let re = Regex::new(r"<[^>]+>").unwrap();
                
                // We need to track code spans to know if a tag is inside one
                let mut spans = Vec::new();
                let mut in_span = false;
                let mut span_start = 0;
                
                for (pos, c) in line.char_indices() {
                    if c == '`' {
                        if !in_span {
                            in_span = true;
                            span_start = pos;
                        } else {
                            in_span = false;
                            spans.push((span_start, pos));
                        }
                    }
                }
                
                // Process each tag, checking if it's in a code span
                let mut offset = 0;
                for cap in re.captures_iter(line) {
                    let html_tag = cap.get(0).unwrap().as_str();
                    let start_pos = cap.get(0).unwrap().start();
                    let end_pos = cap.get(0).unwrap().end();
                    
                    // Check if this tag is in a code span
                    let in_code_span = spans.iter().any(|(start, end)| 
                        start_pos >= *start && end_pos <= *end);
                    
                    if !in_code_span && !self.is_tag_allowed(html_tag) {
                        // Remove the tag, adjusting for previous removals
                        let adjusted_start = start_pos - offset;
                        let adjusted_end = end_pos - offset;
                        fixed_line.replace_range(adjusted_start..adjusted_end, "");
                        offset += end_pos - start_pos;
                    }
                }
                
                result.push_str(&fixed_line);
            } else {
                // No code spans, just remove HTML tags that aren't allowed
                let mut fixed_line = line.to_string();
                let re = Regex::new(r"<[^>]+>").unwrap();
                
                let mut offset = 0;
                for cap in re.captures_iter(line) {
                    let html_tag = cap.get(0).unwrap().as_str();
                    let start_pos = cap.get(0).unwrap().start();
                    let end_pos = cap.get(0).unwrap().end();
                    
                    if !self.is_tag_allowed(html_tag) {
                        // Remove the tag, adjusting for previous removals
                        let adjusted_start = start_pos - offset;
                        let adjusted_end = end_pos - offset;
                        fixed_line.replace_range(adjusted_start..adjusted_end, "");
                        offset += end_pos - start_pos;
                    }
                }
                
                result.push_str(&fixed_line);
            }
            
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
}
