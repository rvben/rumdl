use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};
use regex::Regex;

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

    fn has_trailing_punctuation(&self, text: &str) -> bool {
        if let Some(last_char) = text.trim_end().chars().last() {
            self.punctuation.contains(last_char)
        } else {
            false
        }
    }

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
        let re = Regex::new(r"^(\s*)(#{1,6})(\s+)(.+?)(\s*)(#*)(\s*)$").unwrap();
        
        if let Some(caps) = re.captures(line) {
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
        
        // Simpler regex for basic headings without capturing groups
        let simple_re = Regex::new(r"^\s*#{1,6}\s+.+$").unwrap();
        if simple_re.is_match(line) {
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
                if next_trimmed.chars().all(|c| c == '=') {
                    return Some((1, line.trim().to_string(), HeadingStyle::Setext1));
                } else if next_trimmed.chars().all(|c| c == '-') {
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
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (line_num, line) in lines.iter().enumerate() {
            // Check for ATX headings
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
            
            // Check for setext headings
            if line_num + 1 < lines.len() {
                let next_line = Some(lines[line_num + 1]);
                if let Some((_level, text, _)) = self.is_setext_heading(line, next_line) {
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
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Handle ATX headings
            if let Some((level, text, style)) = self.parse_atx_heading(line) {
                if self.has_trailing_punctuation(&text) {
                    let indentation = HeadingUtils::get_indentation(line);
                    let fixed_text = self.remove_trailing_punctuation(&text);
                    match style {
                        HeadingStyle::AtxClosed => {
                            // Preserve the trailing hashes
                            let trailing_hashes = "#".repeat(level);
                            result.push_str(&format!("{}{} {} {}\n", 
                                " ".repeat(indentation),
                                "#".repeat(level),
                                fixed_text,
                                trailing_hashes
                            ));
                        },
                        _ => {
                            result.push_str(&format!("{}{} {}\n", 
                                " ".repeat(indentation),
                                "#".repeat(level),
                                fixed_text
                            ));
                        },
                    }
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
                continue;
            }
            
            // Handle setext headings
            if line_num + 1 < lines.len() {
                let next_line = Some(lines[line_num + 1]);
                if let Some((_, text, _)) = self.is_setext_heading(line, next_line) {
                    if self.has_trailing_punctuation(&text) {
                        let indentation = HeadingUtils::get_indentation(line);
                        let fixed_text = self.remove_trailing_punctuation(&text);
                        result.push_str(&format!("{}{}\n", " ".repeat(indentation), fixed_text));
                        continue;
                    }
                }
            }
            
            // Just copy other lines
            result.push_str(line);
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 