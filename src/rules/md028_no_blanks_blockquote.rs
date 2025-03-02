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
        
        // Find the content after the last '>' character
        let mut chars = trimmed.chars().peekable();
        let mut seen_gt = false;
        
        while let Some(&c) = chars.peek() {
            if c == '>' {
                seen_gt = true;
                chars.next();
            } else if seen_gt && c.is_whitespace() {
                // Skip a single whitespace after '>'
                chars.next();
                seen_gt = false;
            } else {
                break;
            }
        }
        
        // If we've consumed the entire string or the rest is just whitespace, it's an empty blockquote
        chars.collect::<String>().trim().is_empty()
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
        let mut count = 0;
        let mut chars = trimmed.chars().peekable();
        
        while let Some(&c) = chars.peek() {
            if c == '>' {
                count += 1;
                chars.next();
                // Skip a single space after '>' if present
                if chars.peek().map_or(false, |&c| c.is_whitespace()) {
                    chars.next();
                }
            } else if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        
        count
    }
    
    /// Generates the replacement for a blank blockquote line
    fn get_replacement(indent: &str, level: usize) -> String {
        let mut result = indent.to_string();
        
        if level == 1 {
            // For single level blockquotes: "> "
            result.push('>');
            result.push(' ');
        } else {
            // For nested blockquotes: ">>" (no space between '>' characters)
            for _ in 0..level {
                result.push('>');
            }
            // Add a single space after the last '>'
            result.push(' ');
        }
        
        result
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