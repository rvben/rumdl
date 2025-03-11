use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{Heading, HeadingUtils, HeadingStyle};

#[derive(Debug)]
pub struct MD002FirstHeadingH1 {
    pub level: usize,
}

impl Default for MD002FirstHeadingH1 {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl MD002FirstHeadingH1 {
    pub fn new(level: usize) -> Self {
        Self { level }
    }

    // Find the first heading in the document, skipping front matter
    fn find_first_heading(&self, content: &str) -> Option<(Heading, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut line_num = 0;

        // Skip front matter if present
        if content.starts_with("---\n") || (lines.len() > 0 && lines[0] == "---") {
            line_num += 1;
            while line_num < lines.len() && lines[line_num] != "---" {
                line_num += 1;
            }
            // Skip the closing --- line
            if line_num < lines.len() {
                line_num += 1;
            }
        }

        // Find first heading
        while line_num < lines.len() {
            let line = lines[line_num];
            
            // Check for ATX headings (with possible indentation)
            if line.trim_start().starts_with('#') {
                let trimmed = line.trim_start();
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                if hash_count >= 1 && hash_count <= 6 {
                    let after_hash = &trimmed[hash_count..];
                    if after_hash.is_empty() || after_hash.starts_with(' ') {
                        let text = after_hash.trim_start().trim_end_matches(|c| c == '#' || c == ' ').to_string();
                        let style = if after_hash.trim_end().ends_with('#') {
                            HeadingStyle::AtxClosed
                        } else {
                            HeadingStyle::Atx
                        };
                        return Some((Heading { level: hash_count, text, style }, line_num));
                    }
                }
            } 
            // Check for Setext headings (with possible indentation)
            else if line_num + 1 < lines.len() {
                let next_line = lines[line_num + 1];
                let next_trimmed = next_line.trim_start();
                if !next_trimmed.is_empty() && next_trimmed.chars().all(|c| c == '=' || c == '-') {
                    let level = if next_trimmed.starts_with('=') { 1 } else { 2 };
                    let style = if level == 1 { HeadingStyle::Setext1 } else { HeadingStyle::Setext2 };
                    return Some((Heading { 
                        level, 
                        text: line.trim_start().to_string(),
                        style 
                    }, line_num));
                }
            }
            
            line_num += 1;
        }

        None
    }

    // Helper method to generate replacement text for a heading
    fn generate_replacement(&self, heading: &Heading, indentation: usize) -> String {
        let indent = " ".repeat(indentation);
        
        // Create the correct heading marker based on the style
        match heading.style {
            HeadingStyle::Atx => {
                // For ATX style, use the exact number of # characters needed for the desired level
                format!("{}{} {}", indent, "#".repeat(self.level), heading.text)
            },
            HeadingStyle::AtxClosed => {
                // For closed ATX, ensure we use the correct number of # characters
                format!("{}{} {} {}", indent, "#".repeat(self.level), heading.text, "#".repeat(self.level))
            },
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                // Convert setext to ATX with the correct level
                format!("{}{} {}", indent, "#".repeat(self.level), heading.text)
            }
        }
    }
}

impl Rule for MD002FirstHeadingH1 {
    fn name(&self) -> &'static str {
        "MD002"
    }

    fn description(&self) -> &'static str {
        "First heading should be a top level heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Get the first heading in the document
        if let Some((first_heading, line_num)) = self.find_first_heading(content) {
            // Check if the heading is not at the expected level
            if first_heading.level != self.level {
                let indentation = HeadingUtils::get_indentation(lines[line_num]);
                
                // Generate a warning with the appropriate fix
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: indentation + 1,
                    message: format!("First heading level should be {}", self.level),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: indentation + 1,
                        replacement: self.generate_replacement(&first_heading, indentation),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if let Some((heading, mut line_num)) = self.find_first_heading(content) {
            if heading.level != self.level {
                let lines: Vec<&str> = content.lines().collect();
                let mut result = Vec::new();
                
                // Copy lines before the heading
                for i in 0..line_num {
                    result.push(lines[i].to_string());
                }
                
                // Replace the heading with the correct level
                match heading.style {
                    HeadingStyle::Atx | HeadingStyle::AtxClosed => {
                        let indentation = lines[line_num].len() - lines[line_num].trim_start().len();
                        let indent_str = " ".repeat(indentation);
                        let hashes = "#".repeat(self.level);
                        
                        if heading.style == HeadingStyle::AtxClosed {
                            result.push(format!("{}{} {} {}", indent_str, hashes, heading.text, hashes));
                        } else {
                            result.push(format!("{}{} {}", indent_str, hashes, heading.text));
                        }
                    },
                    HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                        // For Setext headings, convert to ATX style with the correct level
                        let indentation = lines[line_num].len() - lines[line_num].trim_start().len();
                        let indent_str = " ".repeat(indentation);
                        let hashes = "#".repeat(self.level);
                        
                        result.push(format!("{}{} {}", indent_str, hashes, heading.text));
                        
                        // Skip the original underline
                        line_num += 1;
                    }
                }
                
                // Copy remaining lines
                for i in (line_num + 1)..lines.len() {
                    result.push(lines[i].to_string());
                }
                
                // Preserve trailing newline if original had it
                let result_str = if content.ends_with('\n') {
                    format!("{}\n", result.join("\n"))
                } else {
                    result.join("\n")
                };
                
                return Ok(result_str);
            }
        }
        
        Ok(content.to_string())
    }
} 