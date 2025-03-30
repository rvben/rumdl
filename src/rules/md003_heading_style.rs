use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)(=+)\s*$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)(-+)\s*$").unwrap();
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

        for (i, _) in lines.iter().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                let level = heading.level;
                let style = heading.style;
                
                if self.style == HeadingStyle::Setext1 {
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
                    let target_style = if level > 2 && (self.style == HeadingStyle::Setext1 || self.style == HeadingStyle::Setext2) {
                        HeadingStyle::Atx
                    } else {
                        self.style
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
        let mut i = 0;

        while i < lines.len() {
            // If we're at a heading, get its details and replace it with the appropriate style
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                let level = heading.level;
                let style = heading.style;
                let text = heading.text;
                let indentation = heading.indentation;
                
                // Determine if this heading's style needs to be changed
                let should_fix = if self.style == HeadingStyle::Setext1 || self.style == HeadingStyle::Setext2 {
                    // For Setext target styles, check if:
                    // 1. Heading level is ≤ 2 and not already Setext, or
                    // 2. Heading is Setext2 but style should be Setext1, or
                    // 3. Heading is Setext1 but style should be Setext2, or
                    // 4. Heading level is > 2 and not already ATX
                    (level <= 2 && style != self.style && 
                     (style != HeadingStyle::Setext1 && style != HeadingStyle::Setext2)) ||
                    (level <= 2 && self.style == HeadingStyle::Setext1 && style == HeadingStyle::Setext2) ||
                    (level <= 2 && self.style == HeadingStyle::Setext2 && style == HeadingStyle::Setext1) ||
                    (level > 2 && style != HeadingStyle::Atx)
                } else {
                    // For other styles, all headings should match the target style
                    style != self.style
                };

                if should_fix {
                    // For level 3+, always use ATX regardless of target style
                    let target_style = if level > 2 && (self.style == HeadingStyle::Setext1 || self.style == HeadingStyle::Setext2) {
                        HeadingStyle::Atx
                    } else {
                        self.style
                    };
                    
                    // Convert to the target style
                    let _fixed = if target_style == HeadingStyle::Setext1 || target_style == HeadingStyle::Setext2 {
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
}

