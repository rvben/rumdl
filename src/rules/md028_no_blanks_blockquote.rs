use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Checks if a line is a blockquote line (starts with '>')
    fn is_blockquote_line(line: &str) -> bool {
        line.trim_start().starts_with('>')
    }

    /// Checks if a line is an empty blockquote line
    fn is_empty_blockquote_line(line: &str) -> bool {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') {
            return false;
        }
        
        // Count consecutive '>' characters
        let blockquote_level = Self::get_blockquote_level(line);
        
        // Check if there's only whitespace after the blockquote markers
        let content_start_index = trimmed.find('>').unwrap() + blockquote_level;
        let content = if content_start_index < trimmed.len() {
            trimmed[content_start_index..].trim()
        } else {
            ""
        };
        
        content.is_empty()
    }

    /// Checks if a line is completely empty (just whitespace)
    fn is_completely_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Gets the indentation (whitespace prefix) of a line
    fn get_indentation(line: &str) -> String {
        line.chars()
            .take_while(|c| c.is_whitespace())
            .collect()
    }

    /// Gets the blockquote level (number of '>' characters)
    fn get_blockquote_level(line: &str) -> usize {
        let trimmed = line.trim_start();
        trimmed.chars()
            .take_while(|&c| c == '>')
            .count()
    }
    
    /// Generates the replacement for a blank blockquote line
    fn get_replacement(indent: &str, level: usize) -> String {
        if level == 1 {
            format!("{}> ", indent)
        } else {
            format!("{}{} ", indent, ">".repeat(level))
        }
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_blockquote = false;
        
        for (i, &line) in lines.iter().enumerate() {
            if Self::is_completely_empty_line(line) {
                // A completely empty line separates blockquotes
                in_blockquote = false;
                continue;
            }
            
            if Self::is_blockquote_line(line) {
                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }
                
                // Check if this is an empty blockquote line
                if Self::is_empty_blockquote_line(line) {
                    let level = Self::get_blockquote_level(line);
                    let indent = Self::get_indentation(line);
                    
                    warnings.push(LintWarning {
                        message: "Blank line inside blockquote".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: Self::get_replacement(&indent, level),
                        }),
                    });
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        let mut in_blockquote = false;
        
        for line in lines {
            if Self::is_completely_empty_line(line) {
                // Add empty lines as-is
                in_blockquote = false;
                result.push(line.to_string());
                continue;
            }
            
            if Self::is_blockquote_line(line) {
                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }
                
                // Handle empty blockquote lines
                if Self::is_empty_blockquote_line(line) {
                    let level = Self::get_blockquote_level(line);
                    let indent = Self::get_indentation(line);
                    result.push(Self::get_replacement(&indent, level));
                } else {
                    // Add the line as is
                    result.push(line.to_string());
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
                result.push(line.to_string());
            }
        }
        
        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }
} 