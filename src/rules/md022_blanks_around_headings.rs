use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
    static ref FRONT_MATTER_PATTERN: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref SETEXT_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();
    static ref ATX_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*).*$").unwrap();
}

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
        Self {
            lines_above,
            lines_below,
        }
    }

    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    fn count_blank_lines_above(&self, lines: &[&str], current_line: usize) -> usize {
        let mut count = 0;
        let mut line_idx = current_line;
        while line_idx > 0 {
            line_idx -= 1;
            if Self::is_blank_line(lines[line_idx]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn count_blank_lines_below(&self, lines: &[&str], current_line: usize) -> usize {
        let mut count = 0;
        let mut line_idx = current_line;
        while line_idx < lines.len() - 1 {
            line_idx += 1;
            if Self::is_blank_line(lines[line_idx]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn is_setext_heading_underline(&self, line: &str) -> bool {
        SETEXT_HEADING_PATTERN.is_match(line)
    }

    fn is_heading(&self, line: &str) -> bool {
        ATX_HEADING_PATTERN.is_match(line)
    }

    fn is_setext_heading(&self, line: &str, next_line: Option<&str>) -> bool {
        if let Some(next) = next_line {
            !Self::is_blank_line(line) && self.is_setext_heading_underline(next)
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

    fn check_heading_spacing(&self, lines: &[&str], line_num: usize, need_blank_above: bool, is_setext: bool, warnings: &mut Vec<LintWarning>) {
        let heading_line = line_num;
        let underline_line = if is_setext { line_num + 1 } else { line_num };
        
        // For setext headings, we need to check blank lines above the heading text and below the underline
        let blank_lines_above = self.count_blank_lines_above(lines, heading_line);
        let blank_lines_below = self.count_blank_lines_below(lines, underline_line);
        
        // Required blank lines above depends on if it's the first heading or after another heading
        let required_lines_above = if !need_blank_above || heading_line == 0 {
            0
        } else {
            self.lines_above
        };
        
        // Check blank lines above
        if blank_lines_above < required_lines_above {
            // Only add a warning if blank lines are required above
            if required_lines_above > 0 {
                // Get the heading line content with its indentation
                let heading_content = lines[heading_line];
                
                warnings.push(LintWarning {
                    line: heading_line + 1,
                    column: 1,
                    message: format!(
                        "Heading should have {} blank line{} above",
                        required_lines_above,
                        if required_lines_above == 1 { "" } else { "s" }
                    ),
                    fix: Some(Fix {
                        line: heading_line + 1,
                        column: 1,
                        // We need to preserve the original heading content with its indentation
                        replacement: format!("{}\n{}", "\n".repeat(required_lines_above - blank_lines_above), heading_content),
                    }),
                });
            }
        }
        
        // Check blank lines below
        if blank_lines_below < self.lines_below {
            // Calculate position for the warning
            let warning_line = underline_line + 1;
            
            // Get the content for the fix
            let next_content = if warning_line < lines.len() {
                lines[warning_line]
            } else {
                ""
            };
            
            warnings.push(LintWarning {
                line: if warning_line < lines.len() { warning_line + 1 } else { lines.len() },
                column: 1,
                message: format!(
                    "Heading should have {} blank line{} below",
                    self.lines_below,
                    if self.lines_below == 1 { "" } else { "s" }
                ),
                fix: Some(Fix {
                    line: if warning_line < lines.len() { warning_line + 1 } else { lines.len() + 1 },
                    column: 1,
                    replacement: format!("{}{}", "\n".repeat(self.lines_below - blank_lines_below), next_content),
                }),
            });
        }
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_heading = true;
        
        let mut i = 0;
        while i < lines.len() {
            // Skip content in code blocks and front matter
            if self.is_code_block_delimiter(&lines[i]) {
                in_code_block = !in_code_block;
                i += 1;
                continue;
            }
            
            if i == 0 && self.is_front_matter_delimiter(&lines[i]) {
                in_front_matter = true;
                i += 1;
                continue;
            }
            
            if in_front_matter && self.is_front_matter_delimiter(&lines[i]) {
                in_front_matter = false;
                i += 1;
                continue;
            }
            
            if in_code_block || in_front_matter {
                i += 1;
                continue;
            }
            
            // Check if this is a heading and the previous line was also a heading
            let is_after_heading = i > 0 && (
                self.is_heading(&lines[i-1]) || 
                (i > 1 && self.is_setext_heading(&lines[i-2], Some(&lines[i-1])))
            );
            
            // Check ATX headings
            if self.is_heading(&lines[i]) {
                let need_blank_above = !first_heading && !is_after_heading;
                
                self.check_heading_spacing(&lines, i, need_blank_above, false, &mut warnings);
                first_heading = false;
                i += 1;
                continue;
            }
            
            // Check setext headings
            if i + 1 < lines.len() && self.is_setext_heading(&lines[i], Some(&lines[i + 1])) {
                let need_blank_above = !first_heading && !is_after_heading;
                
                self.check_heading_spacing(&lines, i, need_blank_above, true, &mut warnings);
                first_heading = false;
                i += 2; // Skip the underline, we've already processed it
                continue;
            }
            
            i += 1;
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Get warnings with their fixes
        let warnings = self.check(content)?;
        
        // If there are no warnings, return the original content
        if warnings.is_empty() {
            return Ok(content.to_string());
        }
        
        // Split content into lines for processing
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::with_capacity(lines.len() * 2); // Pre-allocate with extra space for blank lines
        
        // Create mappings for blank lines to add before and after specific line indices
        let mut add_blanks_above: HashMap<usize, usize> = HashMap::new();
        let mut add_blanks_below: HashMap<usize, usize> = HashMap::new();
        
        // Process warnings to determine blank lines needed
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                // Line number is 1-indexed in warnings but 0-indexed in our lines vector
                let line_idx = fix.line - 1;
                
                if fix.replacement.starts_with('\n') {
                    // This is a warning about blank lines above content
                    let blanks = fix.replacement.chars().take_while(|&c| c == '\n').count();
                    add_blanks_above.insert(line_idx, blanks);
                } else if fix.replacement.ends_with('\n') {
                    // This is a warning about blank lines below content
                    let blanks = fix.replacement.chars().filter(|&c| c == '\n').count();
                    add_blanks_below.insert(line_idx - 1, blanks);
                }
            }
        }
        
        // Process each line and apply fixes
        for (i, line) in lines.iter().enumerate() {
            // Add blank lines above if needed
            if let Some(blanks) = add_blanks_above.get(&i) {
                for _ in 0..*blanks {
                    result.push("");
                }
            }
            
            // Add the current line
            result.push(line);
            
            // Add blank lines below if needed
            if let Some(blanks) = add_blanks_below.get(&i) {
                for _ in 0..*blanks {
                    result.push("");
                }
            }
        }
        
        // Join lines with newlines
        let mut fixed_content = result.join("\n");
        
        // Preserve trailing newline if present in original content
        if content.ends_with('\n') && !fixed_content.ends_with('\n') {
            fixed_content.push('\n');
        }
        
        Ok(fixed_content)
    }
} 