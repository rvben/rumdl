use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)(=+)\s*$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)(-+)\s*$").unwrap();
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
}

pub struct MD003HeadingStyle {
    style: HeadingStyle,
}

impl Default for MD003HeadingStyle {
    fn default() -> Self {
        Self {
            style: HeadingStyle::Atx,
        }
    }
}

impl MD003HeadingStyle {
    pub fn new(style: HeadingStyle) -> Self {
        Self { style }
    }
    
    /// Detects the first heading style in the document for "consistent" mode
    fn detect_first_heading_style(&self, content: &str) -> Option<HeadingStyle> {
        let lines: Vec<&str> = content.lines().collect();
        for i in 0..lines.len() {
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                return Some(heading.style);
            }
        }
        None
    }
    
    /// Check if content starts with front matter
    fn has_front_matter(&self, content: &str) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return false;
        }
        
        // Check if content starts with front matter delimiter
        if !FRONT_MATTER_DELIMITER.is_match(lines[0]) {
            return false;
        }
        
        // Search for the closing front matter delimiter
        for i in 1..lines.len() {
            if FRONT_MATTER_DELIMITER.is_match(lines[i]) {
                return true;
            }
        }
        
        false
    }
    
    /// Get the line number where front matter ends
    fn front_matter_end_line(&self, content: &str) -> Option<usize> {
        if !self.has_front_matter(content) {
            return None;
        }
        
        let lines: Vec<&str> = content.lines().collect();
        
        // Find the ending front matter delimiter (starting from line 1)
        for i in 1..lines.len() {
            if FRONT_MATTER_DELIMITER.is_match(lines[i]) {
                return Some(i + 1); // Return 1-indexed line number
            }
        }
        
        None
    }
}

impl Rule for MD003HeadingStyle {
    fn name(&self) -> &'static str {
        "MD003"
    }

    fn description(&self) -> &'static str {
        "Heading style"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Determine front matter boundaries
        let front_matter_end = self.front_matter_end_line(content);
        
        // For consistent style, detect the first heading style
        let target_style = if self.style == HeadingStyle::Atx && content.contains("# ") {
            self.detect_first_heading_style(content).unwrap_or(self.style)
        } else {
            self.style
        };

        for (i, _) in lines.iter().enumerate() {
            // Skip front matter
            if let Some(end_line) = front_matter_end {
                if i + 1 <= end_line {
                    continue;
                }
            }
            
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                let level = heading.level;
                let style = heading.style;
                
                // If the target style is "consistent", use the first heading's style
                let effective_style = if self.style == HeadingStyle::Atx && target_style != self.style {
                    target_style
                } else {
                    self.style
                };
                
                if effective_style == HeadingStyle::Setext1 {
                    // For Setext1 target style:
                    // 1. All level 1 and 2 headings should be Setext style (Setext1 or Setext2)
                    // 2. Level 3+ headings should be ATX
                    if level <= 2 {
                        // Check if it's not a Setext style at all
                        if style != HeadingStyle::Setext1 && style != HeadingStyle::Setext2 {
                            result.push(LintWarning {
                                line: heading.line_number,
                                column: 1,
                                message: format!(
                                    "Heading style should be Setext, found {:?}",
                                    style
                                ),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    } else if style != HeadingStyle::Atx {
                        result.push(LintWarning {
                            line: heading.line_number,
                            column: 1,
                            message: format!(
                                "Level 3+ heading style should be ATX, found {:?}",
                                style
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                } else {
                    // For other target styles, all headings should match
                    let target_style = if level > 2 && (effective_style == HeadingStyle::Setext1 || effective_style == HeadingStyle::Setext2) {
                        HeadingStyle::Atx
                    } else {
                        effective_style
                    };
                    
                    if style != target_style {
                        result.push(LintWarning {
                            line: heading.line_number,
                            column: 1,
                            message: format!(
                                "Heading style should be {:?}, found {:?}",
                                target_style,
                                style
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }
            }
        }

        Ok(result)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut fixed_lines: Vec<String> = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Determine front matter boundaries
        let front_matter_end = self.front_matter_end_line(content);
        
        // For consistent style, detect the first heading style
        let target_style = if self.style == HeadingStyle::Atx && content.contains("# ") {
            self.detect_first_heading_style(content).unwrap_or(self.style)
        } else {
            self.style
        };
        
        let mut i = 0;

        while i < lines.len() {
            // Add front matter lines unchanged
            if let Some(end_line) = front_matter_end {
                if i + 1 <= end_line {
                    fixed_lines.push(lines[i].to_string());
                    i += 1;
                    continue;
                }
            }
            
            // If we're at a heading, get its details and replace it with the appropriate style
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                let level = heading.level;
                let style = heading.style;
                let text = heading.text;
                let indentation = heading.indentation;
                
                // Determine effective target style (for consistent mode)
                let effective_style = if self.style == HeadingStyle::Atx && target_style != self.style {
                    target_style
                } else {
                    self.style
                };
                
                // Determine if this heading's style needs to be changed
                let should_fix = if effective_style == HeadingStyle::Setext1 || effective_style == HeadingStyle::Setext2 {
                    // For Setext target styles, check if:
                    // 1. Heading level is ≤ 2 and not already Setext, or
                    // 2. Heading is Setext2 but style should be Setext1, or
                    // 3. Heading is Setext1 but style should be Setext2, or
                    // 4. Heading level is > 2 and not already ATX
                    (level <= 2 && style != effective_style && 
                     (style != HeadingStyle::Setext1 && style != HeadingStyle::Setext2)) ||
                    (level <= 2 && effective_style == HeadingStyle::Setext1 && style == HeadingStyle::Setext2) ||
                    (level <= 2 && effective_style == HeadingStyle::Setext2 && style == HeadingStyle::Setext1) ||
                    (level > 2 && style != HeadingStyle::Atx)
                } else {
                    // For other styles, all headings should match the target style
                    style != effective_style
                };

                if should_fix {
                    // For level 3+, always use ATX regardless of target style
                    let target_style = if level > 2 && (effective_style == HeadingStyle::Setext1 || effective_style == HeadingStyle::Setext2) {
                        HeadingStyle::Atx
                    } else {
                        effective_style
                    };
                    
                    // Convert to the target style
                    if target_style == HeadingStyle::Setext1 || target_style == HeadingStyle::Setext2 {
                        let formatted = format!("{}{}", indentation, text.trim());
                        let underline_char = if target_style == HeadingStyle::Setext1 {
                            if level == 1 { '=' } else { '-' }
                        } else { 
                            '-' 
                        };
                        let underline_length = text.trim().chars().count().max(3);
                        let setext_line = format!("{}{}", indentation, underline_char.to_string().repeat(underline_length));
                        
                        // Add both the heading text and the setext underline
                        fixed_lines.push(formatted);
                        fixed_lines.push(setext_line);
                    } else {
                        // ATX or AtxClosed
                        let hashes = "#".repeat(level as usize);
                        if target_style == HeadingStyle::AtxClosed {
                            fixed_lines.push(format!("{}{} {} {}", indentation, hashes, text.trim(), hashes));
                        } else {
                            fixed_lines.push(format!("{}{} {}", indentation, hashes, text.trim()));
                        }
                    };
                    
                    // Skip over the original setext underline if needed
                    if style == HeadingStyle::Setext1 || style == HeadingStyle::Setext2 {
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else {
                    // Keep the original heading unchanged
                    if style == HeadingStyle::Setext1 || style == HeadingStyle::Setext2 {
                        fixed_lines.push(lines[i].to_string());
                        fixed_lines.push(lines[i + 1].to_string());
                        i += 2;
                    } else {
                        fixed_lines.push(lines[i].to_string());
                        i += 1;
                    }
                }
            } else {
                // Not a heading, keep line as is
                fixed_lines.push(lines[i].to_string());
                i += 1;
            }
        }

        Ok(fixed_lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atx_heading_style() {
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let result = rule.check(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_setext_heading_style() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
        let content = "Heading 1\n=========\n\nHeading 2\n---------";
        
        let result = rule.check(content).unwrap();
        assert!(result.is_empty(), "Expected no warnings for setext heading style");
        
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, content, "Content should remain unchanged");
    }
    
    #[test]
    fn test_front_matter() {
        let rule = MD003HeadingStyle::default();
        let content = "---\ntitle: Test Document\n---\n\n# Heading 1\n## Heading 2";
        
        let result = rule.check(content).unwrap();
        assert!(result.is_empty(), "Expected no warnings for document with front matter");
        
        assert!(rule.has_front_matter(content), "Should detect front matter");
        assert_eq!(rule.front_matter_end_line(content), Some(3), "Front matter should end at line 3");
    }
    
    #[test]
    fn test_consistent_heading_style() {
        // Create a rule with "consistent" style (using Atx as the default)
        let rule = MD003HeadingStyle::default(); // Uses Atx by default
        
        // Document using Setext style consistently
        let content = "Heading 1\n=========\n\nHeading 2\n---------\n\n### Heading 3";
        
        // When checking with consistent style, it should detect Setext as the first style
        let first_style = rule.detect_first_heading_style(content);
        assert_eq!(first_style, Some(HeadingStyle::Setext1), "Should detect Setext1 as first style");
        
        // No warnings should be generated for consistent usage
        let result = rule.check(content).unwrap();
        assert!(result.is_empty(), "Expected no warnings for consistent heading style");
        
        // Test with mixed styles
        let mixed_content = "Heading 1\n=========\n\n## Heading 2\n\n### Heading 3";
        let result = rule.check(mixed_content).unwrap();
        assert_eq!(result.len(), 1, "Expected warning for inconsistent style");
        assert!(result[0].message.contains("Setext"), "Warning should mention Setext style");
    }
}

