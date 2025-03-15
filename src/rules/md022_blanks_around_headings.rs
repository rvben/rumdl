use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use once_cell::sync::Lazy;
use regex::Regex;
use std::cmp;

static SETEXT_HEADING_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap());
static ATX_HEADING_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s*)(#{1,6})(\s*).*$").unwrap());
static CODE_BLOCK_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s*)```").unwrap());
static FRONT_MATTER_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^---\s*$").unwrap());

#[derive(Debug)]
pub struct MD022BlanksAroundHeadings {
    lines_above: usize,
    lines_below: usize,
}

impl Default for MD022BlanksAroundHeadings {
    fn default() -> Self {
        Self {
            lines_above: 1,
            lines_below: 1,
        }
    }
}

impl MD022BlanksAroundHeadings {
    pub fn new(lines_above: usize, lines_below: usize) -> Self {
        MD022BlanksAroundHeadings {
            lines_above,
            lines_below,
        }
    }
    
    fn is_setext_heading_underline(&self, line: &str) -> bool {
        SETEXT_HEADING_PATTERN.is_match(line)
    }
    
    fn is_heading(&self, line: &str) -> bool {
        ATX_HEADING_PATTERN.is_match(line)
    }
    
    fn is_setext_heading(&self, line: &str, next_line: Option<&str>) -> bool {
        if let Some(next) = next_line {
            !line.trim().is_empty() && self.is_setext_heading_underline(next)
        } else {
            false
        }
    }
    
    fn is_code_block_delimiter(&self, line: &str) -> bool {
        CODE_BLOCK_PATTERN.is_match(line)
    }
    
    fn is_front_matter_delimiter(&self, line: &str) -> bool {
        FRONT_MATTER_PATTERN.is_match(line)
    }
    
    fn find_previous_non_blank_line(&self, lines: &[&str], current_line: usize) -> usize {
        let mut line_index = current_line;
        while line_index > 0 {
            line_index -= 1;
            if !lines[line_index].trim().is_empty() {
                return line_index;
            }
        }
        0
    }
    
    fn count_blank_lines_above(&self, lines: &[&str], current_line: usize) -> usize {
        let mut blank_lines = 0;
        let mut line_index = current_line;
        
        while line_index > 0 {
            line_index -= 1;
            if lines[line_index].trim().is_empty() {
                blank_lines += 1;
            } else {
                break;
            }
        }
        
        blank_lines
    }
    
    fn count_blank_lines_below(&self, lines: &[&str], current_line: usize) -> usize {
        let mut blank_lines = 0;
        let mut line_index = current_line + 1;
        
        while line_index < lines.len() {
            if lines[line_index].trim().is_empty() {
                blank_lines += 1;
                line_index += 1;
            } else {
                break;
            }
        }
        
        blank_lines
    }
    
    // Helper method to check spacing around a heading and generate warnings/fixes
    fn check_heading_spacing(&self, content: &str, lines: &[&str], heading_line: usize, need_blank_above: bool, is_setext: bool, warnings: &mut Vec<LintWarning>) {
        let blank_lines_above = self.count_blank_lines_above(lines, heading_line);
        let underline_line = if is_setext { heading_line + 1 } else { heading_line };
        let blank_lines_below = self.count_blank_lines_below(lines, underline_line);
        
        // Check blank lines above
        if need_blank_above && blank_lines_above < self.lines_above {
            let message = format!("Heading should have at least {} blank {} above", 
                                self.lines_above, 
                                if self.lines_above == 1 { "line" } else { "lines" });
            
            // Calculate byte offset to the start of the heading line
            let mut heading_start_pos = 0;
            for i in 0..heading_line {
                heading_start_pos += lines[i].len() + 1; // +1 for newline
            }
            
            // Create a replacement that includes the required number of blank lines
            let mut replacement = String::new();
            
            // If there are already some blank lines, we only add what's missing
            for _ in 0..(self.lines_above - blank_lines_above) {
                replacement.push('\n');
            }
            
            let range = heading_start_pos..heading_start_pos;
            let fix = Fix {
                range,
                replacement,
            };
            
            warnings.push(LintWarning {
                message,
                line: heading_line + 1,
                column: 1,
                severity: Severity::Warning,
                fix: Some(fix),
            });
        }
        
        // Check blank lines below
        if blank_lines_below < self.lines_below {
            let heading_end_line = if is_setext { underline_line } else { heading_line };
            let message = format!("Heading should have at least {} blank {} below", 
                                self.lines_below, 
                                if self.lines_below == 1 { "line" } else { "lines" });
            
            // Calculate byte offset to the end of the heading/underline line
            let mut end_pos = 0;
            for i in 0..=heading_end_line {
                end_pos += lines[i].len() + 1; // +1 for newline
            }
            
            // If we're at the end of the content, we might need to adjust
            let end_pos = if end_pos > content.len() {
                content.len()
            } else {
                end_pos
            };
            
            // Create a replacement that includes the required number of blank lines
            let mut replacement = String::new();
            
            // If there are already some blank lines, we only add what's missing
            for _ in 0..(self.lines_below - blank_lines_below) {
                replacement.push('\n');
            }
            
            let range = end_pos..end_pos;
            let fix = Fix {
                range,
                replacement,
            };
            
            warnings.push(LintWarning {
                message,
                line: heading_end_line + 1,
                column: 1,
                severity: Severity::Warning,
                fix: Some(fix),
            });
        }
    }

    fn internal_check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        
        let mut in_code_block = false;
        let mut is_front_matter = false;
        
        // Split content into lines
        let lines: Vec<&str> = content.lines().collect();
        
        // Skip YAML front matter if present
        if lines.first().map_or(false, |&line| line.trim() == "---") {
            is_front_matter = true;
        }
        
        // Process each line
        for (i, line) in lines.iter().enumerate() {
            // Check for end of front matter
            if is_front_matter && i > 0 && *line == "---" {
                is_front_matter = false;
                continue;
            }
            
            // Skip processing in front matter
            if is_front_matter {
                continue;
            }
            
            // Check for code blocks
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            
            // Skip processing in code blocks
            if in_code_block {
                continue;
            }
            
            // Check for ATX headings (# style)
            if line.trim().starts_with('#') && line.trim().chars().nth(1).map_or(true, |c| c.is_whitespace() || c == '#') {
                // Need blank line above except for first line
                let need_blank_above = i > 0;
                self.check_heading_spacing(content, &lines, i, need_blank_above, false, &mut warnings);
            } 
            // Check for Setext headings (underlined style)
            else if i > 0 && !line.trim().is_empty() && 
                     (line.trim().chars().all(|c| c == '=') || line.trim().chars().all(|c| c == '-')) && 
                     !lines[i-1].trim().is_empty() {
                // This is a Setext heading underline, the actual heading is on the previous line
                self.check_heading_spacing(content, &lines, i-1, true, true, &mut warnings);
            }
        }
        
        Ok(warnings)
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "blanks-around-headings"
    }
    
    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }
    
    fn check(&self, content: &str) -> LintResult {
        self.internal_check(content)
    }
    
    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Get warnings but ignore their fixes - we'll implement a more direct line-based approach
        let warnings = self.internal_check(content)?;
        
        // If there are no warnings, return the original content
        if warnings.is_empty() {
            return Ok(content.to_string());
        }
        
        // Split content into lines for processing
        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = Vec::with_capacity(lines.len() * 2); // Allocate extra space for blank lines
        
        let mut in_code_block = false;
        let mut is_front_matter = false;
        let mut i = 0;
        
        // Check for YAML front matter
        if !lines.is_empty() && lines[0].trim() == "---" {
            is_front_matter = true;
            fixed_lines.push(lines[0].to_string());
            i += 1;
        }
        
        // Process each line
        while i < lines.len() {
            // Check for end of front matter
            if is_front_matter && lines[i] == "---" {
                is_front_matter = false;
                fixed_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }
            
            // Skip processing in front matter
            if is_front_matter {
                fixed_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }
            
            // Check for code blocks
            if lines[i].trim().starts_with("```") {
                in_code_block = !in_code_block;
                fixed_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }
            
            // Skip processing in code blocks
            if in_code_block {
                fixed_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }
            
            // Check for ATX headings (# style)
            if lines[i].trim().starts_with('#') && 
               lines[i].trim().chars().nth(1).map_or(true, |c| c.is_whitespace() || c == '#') {
                
                // Insert blank lines above (unless it's the first line)
                if i > 0 && !is_front_matter {
                    // Count the existing blank lines above
                    let mut blank_above = 0;
                    let mut j = i - 1;
                    while j < i && lines[j].trim().is_empty() {
                        blank_above += 1;
                        j = j.wrapping_sub(1);
                        if j >= lines.len() { break; } // Prevent underflow
                    }
                    
                    // Add any missing blank lines above
                    if blank_above < self.lines_above {
                        // Remove existing blank lines from fixed_lines
                        for _ in 0..blank_above {
                            if !fixed_lines.is_empty() {
                                fixed_lines.pop();
                            }
                        }
                        
                        // Add the correct number of blank lines
                        for _ in 0..self.lines_above {
                            fixed_lines.push(String::new());
                        }
                    }
                }
                
                // Add the heading line
                fixed_lines.push(lines[i].to_string());
                i += 1;
                
                // Insert blank lines below
                let mut blank_below = 0;
                let mut j = i;
                while j < lines.len() && lines[j].trim().is_empty() {
                    blank_below += 1;
                    j += 1;
                }
                
                // Skip existing blank lines
                i += blank_below;
                
                // Add the correct number of blank lines below
                for _ in 0..self.lines_below {
                    fixed_lines.push(String::new());
                }
                
                continue;
            }
            
            // Check for Setext headings (underlined style)
            if i + 1 < lines.len() && !lines[i].trim().is_empty() && 
               (lines[i+1].trim().chars().all(|c| c == '=') || 
                lines[i+1].trim().chars().all(|c| c == '-')) {
                
                // Insert blank lines above (unless it's the first line)
                if i > 0 {
                    // Count the existing blank lines above
                    let mut blank_above = 0;
                    let mut j = i - 1;
                    while j < i && lines[j].trim().is_empty() {
                        blank_above += 1;
                        j = j.wrapping_sub(1);
                        if j >= lines.len() { break; } // Prevent underflow
                    }
                    
                    // Add any missing blank lines above
                    if blank_above < self.lines_above {
                        // Remove existing blank lines from fixed_lines
                        for _ in 0..blank_above {
                            if !fixed_lines.is_empty() {
                                fixed_lines.pop();
                            }
                        }
                        
                        // Add the correct number of blank lines
                        for _ in 0..self.lines_above {
                            fixed_lines.push(String::new());
                        }
                    }
                }
                
                // Add the heading and underline lines
                fixed_lines.push(lines[i].to_string());
                fixed_lines.push(lines[i+1].to_string());
                i += 2;
                
                // Insert blank lines below
                let mut blank_below = 0;
                let mut j = i;
                while j < lines.len() && lines[j].trim().is_empty() {
                    blank_below += 1;
                    j += 1;
                }
                
                // Skip existing blank lines
                i += blank_below;
                
                // Add the correct number of blank lines below
                for _ in 0..self.lines_below {
                    fixed_lines.push(String::new());
                }
                
                continue;
            }
            
            // Not a heading - just add the line
            fixed_lines.push(lines[i].to_string());
            i += 1;
        }
        
        // Join lines back together
        let fixed_content = fixed_lines.join("\n");
        
        // Ensure the content ends with a newline if the original did
        let mut result = fixed_content;
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        
        Ok(result)
    }
}