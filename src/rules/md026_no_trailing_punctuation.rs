use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Optimized regex patterns for heading detection
    static ref ATX_HEADING_DETAILED: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.+?)(\s*)(#*)(\s*)$").unwrap();
    static ref ATX_HEADING_SIMPLE: Regex = Regex::new(r"^\s*#{1,6}\s+.+$").unwrap();
    static ref SETEXT_HEADING_UNDERLINE1: Regex = Regex::new(r"^=+\s*$").unwrap();
    static ref SETEXT_HEADING_UNDERLINE2: Regex = Regex::new(r"^-+\s*$").unwrap();
}

#[derive(Debug)]
pub struct MD026NoTrailingPunctuation {
    punctuation: String,
}

impl Default for MD026NoTrailingPunctuation {
    fn default() -> Self {
        Self {
            punctuation: ".,;:!?".to_string(),
        }
    }
}

impl MD026NoTrailingPunctuation {
    pub fn new(punctuation: String) -> Self {
        Self { punctuation }
    }

    #[inline]
    fn has_trailing_punctuation(&self, text: &str) -> bool {
        if let Some(last_char) = text.trim_end().chars().last() {
            self.punctuation.contains(last_char)
        } else {
            false
        }
    }

    #[inline]
    fn remove_trailing_punctuation(&self, text: &str) -> String {
        let mut result = text.trim_end().to_string();
        while let Some(last_char) = result.chars().last() {
            if self.punctuation.contains(last_char) {
                result.pop();
            } else {
                break;
            }
        }
        result
    }
    
    // Parse ATX style headings directly for trailing punctuation check
    fn parse_atx_heading(&self, line: &str) -> Option<(usize, String, HeadingStyle)> {
        // Try detailed pattern first
        if let Some(caps) = ATX_HEADING_DETAILED.captures(line) {
            let _indent = caps.get(1).map_or("", |m| m.as_str()).len();
            let level = caps.get(2).map_or("", |m| m.as_str()).len();
            let text = caps.get(4).map_or("", |m| m.as_str()).to_string();
            let trailing_hashes = !caps.get(6).map_or("", |m| m.as_str()).is_empty();
            
            let style = if trailing_hashes {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            
            return Some((level, text, style));
        }
        
        // Fall back to simpler pattern if needed
        if ATX_HEADING_SIMPLE.is_match(line) {
            let _indent = line.len() - line.trim_start().len();
            let trimmed = line.trim_start();
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            let text_start = trimmed.find(|c| c != '#' && c != ' ').unwrap_or(trimmed.len());
            let text = trimmed[text_start..].trim();
            
            return Some((level, text.to_string(), HeadingStyle::Atx));
        }
        
        None
    }
    
    // Parse Setext style headings directly
    fn is_setext_heading(&self, line: &str, next_line: Option<&str>) -> Option<(usize, String, HeadingStyle)> {
        if let Some(next) = next_line {
            let next_trimmed = next.trim();
            if !next_trimmed.is_empty() {
                if SETEXT_HEADING_UNDERLINE1.is_match(next_trimmed) {
                    return Some((1, line.trim().to_string(), HeadingStyle::Setext1));
                } else if SETEXT_HEADING_UNDERLINE2.is_match(next_trimmed) {
                    return Some((2, line.trim().to_string(), HeadingStyle::Setext2));
                }
            }
        }
        None
    }
}

impl Rule for MD026NoTrailingPunctuation {
    fn name(&self) -> &'static str {
        "MD026"
    }

    fn description(&self) -> &'static str {
        "Trailing punctuation in heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let _timer = crate::profiling::ScopedTimer::new("MD026_check");
        
        // Early returns for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }
        
        // Early return if no headings or punctuation characters exist
        if !content.contains('#') && 
           !content.contains('=') && 
           !content.contains('-') && 
           !self.punctuation.chars().any(|c| content.contains(c)) {
            return Ok(vec![]);
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines that definitely aren't headings
            if !line.contains('#') && 
               (line_num + 1 >= lines.len() || (!lines[line_num + 1].contains('=') && !lines[line_num + 1].contains('-'))) {
                continue;
            }
            
            // Check for ATX headings
            if line.contains('#') {
                if let Some((level, text, style)) = self.parse_atx_heading(line) {
                    if self.has_trailing_punctuation(&text) {
                        let indentation = HeadingUtils::get_indentation(line);
                        let fixed_text = self.remove_trailing_punctuation(&text);
                        let replacement = match style {
                            HeadingStyle::AtxClosed => {
                                // Preserve the trailing hashes
                                let trailing_hashes = "#".repeat(level);
                                format!("{}{} {} {}", 
                                    " ".repeat(indentation),
                                    "#".repeat(level),
                                    fixed_text,
                                    trailing_hashes
                                )
                            },
                            _ => format!("{}{} {}", 
                                " ".repeat(indentation),
                                "#".repeat(level),
                                fixed_text
                            ),
                        };
                        
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: indentation + 1,
                            message: format!("Trailing punctuation in heading '{}'", text),
                            fix: Some(Fix {
                                line: line_num + 1,
                                column: indentation + 1,
                                replacement,
                            }),
                        });
                    }
                    continue;
                }
            }
            
            // Check for setext headings
            if line_num + 1 < lines.len() {
                let next_line = lines[line_num + 1];
                
                // Quick check for setext underline characters
                if !next_line.contains('=') && !next_line.contains('-') {
                    continue;
                }
                
                if let Some((_level, text, _)) = self.is_setext_heading(line, Some(next_line)) {
                    if self.has_trailing_punctuation(&text) {
                        let indentation = HeadingUtils::get_indentation(line);
                        let fixed_text = self.remove_trailing_punctuation(&text);
                        
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: indentation + 1,
                            message: format!("Trailing punctuation in heading '{}'", text),
                            fix: Some(Fix {
                                line: line_num + 1,
                                column: indentation + 1,
                                replacement: format!("{}{}", " ".repeat(indentation), fixed_text),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _timer = crate::profiling::ScopedTimer::new("MD026_fix");
        
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Early return if no headings or punctuation characters exist
        if !content.contains('#') && 
           !content.contains('=') && 
           !content.contains('-') && 
           !self.punctuation.chars().any(|c| content.contains(c)) {
            return Ok(content.to_string());
        }
        
        let lines: Vec<&str> = content.lines().collect();
        let mut output_lines: Vec<String> = Vec::with_capacity(lines.len());
        
        let mut skip_next = false;
        for (i, line) in lines.iter().enumerate() {
            if skip_next {
                skip_next = false;
                output_lines.push(line.to_string());
                continue;
            }
            
            // Skip lines that definitely aren't headings
            if !line.contains('#') && 
               (i + 1 >= lines.len() || (!lines[i + 1].contains('=') && !lines[i + 1].contains('-'))) {
                output_lines.push(line.to_string());
                continue;
            }
            
            // Check for ATX headings and fix them
            if line.contains('#') {
                if let Some((level, text, style)) = self.parse_atx_heading(line) {
                    if self.has_trailing_punctuation(&text) {
                        let indentation = HeadingUtils::get_indentation(line);
                        let fixed_text = self.remove_trailing_punctuation(&text);
                        
                        match style {
                            HeadingStyle::AtxClosed => {
                                // Preserve the trailing hashes
                                let trailing_hashes = "#".repeat(level);
                                output_lines.push(format!("{}{} {} {}", 
                                    " ".repeat(indentation),
                                    "#".repeat(level),
                                    fixed_text,
                                    trailing_hashes
                                ));
                            },
                            _ => {
                                output_lines.push(format!("{}{} {}", 
                                    " ".repeat(indentation),
                                    "#".repeat(level),
                                    fixed_text
                                ));
                            }
                        }
                    } else {
                        output_lines.push(line.to_string());
                    }
                    continue;
                }
            }
            
            // Check and fix setext headings
            if i + 1 < lines.len() {
                let next_line = lines[i + 1];
                
                // Skip if next line clearly isn't a setext underline
                if !next_line.contains('=') && !next_line.contains('-') {
                    output_lines.push(line.to_string());
                    continue;
                }
                
                if let Some((_level, text, _)) = self.is_setext_heading(line, Some(next_line)) {
                    if self.has_trailing_punctuation(&text) {
                        let indentation = HeadingUtils::get_indentation(line);
                        let fixed_text = self.remove_trailing_punctuation(&text);
                        output_lines.push(format!("{}{}", " ".repeat(indentation), fixed_text));
                        skip_next = true;
                        output_lines.push(next_line.to_string());
                    } else {
                        output_lines.push(line.to_string());
                    }
                } else {
                    output_lines.push(line.to_string());
                }
            } else {
                output_lines.push(line.to_string());
            }
        }
        
        // Join lines and preserve trailing newline
        if content.ends_with('\n') {
            Ok(format!("{}\n", output_lines.join("\n")))
        } else {
            Ok(output_lines.join("\n"))
        }
    }
} 