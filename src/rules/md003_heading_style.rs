use crate::rule::{LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};

#[derive(Debug)]
pub struct MD003HeadingStyle {
    pub style: HeadingStyle,
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
        "Heading style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let remaining = &lines[i..].join("\n");
            if let Some(heading) = HeadingUtils::parse_heading(remaining, 0) {
                // For setext style, only check headings that could be setext (level 1-2)
                let should_check = match self.style {
                    HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                        // If target style is setext, only validate level 1-2 headings
                        // and allow both setext1 and setext2 styles
                        if heading.level <= 2 {
                            !matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2)
                        } else {
                            false
                        }
                    },
                    _ => heading.style != self.style
                };

                if should_check {
                    let indentation = HeadingUtils::get_indentation(lines[i]);
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: indentation + 1,
                        message: format!("Heading style should be {:?}", self.style),
                        fix: None,
                    });
                }
                // Skip the underline for setext headings
                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                    i += 1;
                }
            }
            i += 1;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = Vec::new();
        let target_style = self.style;
        let mut i = 0;
        let mut in_front_matter = false;

        // Process each line
        while i < lines.len() {
            let line = lines[i];
            
            // Handle front matter
            if i == 0 && line.trim() == "---" {
                in_front_matter = true;
                fixed_lines.push(line.to_string());
                i += 1;
                continue;
            }
            
            if in_front_matter {
                fixed_lines.push(line.to_string());
                if line.trim() == "---" {
                    in_front_matter = false;
                }
                i += 1;
                continue;
            }
            
            // Preserve blank lines
            if line.trim().is_empty() {
                fixed_lines.push(line.to_string());
                i += 1;
                continue;
            }
            
            let indentation = HeadingUtils::get_indentation(line);

            // Check if current line is a heading
            if let Some(heading) = HeadingUtils::parse_heading(content, i) {
                if matches!(target_style, HeadingStyle::Setext1 | HeadingStyle::Setext2) 
                    && heading.level <= 2 {
                    // For setext headings
                    let text = heading.text.trim();
                    let underline_char = if heading.level == 1 { '=' } else { '-' };
                    let underline = underline_char.to_string().repeat(text.chars().count().max(3));
                    
                    // Add the heading text with indentation
                    fixed_lines.push(format!("{}{}", " ".repeat(indentation), text));
                    
                    // Add the underline with same indentation
                    fixed_lines.push(format!("{}{}", " ".repeat(indentation), underline));
                    
                    // Skip the underline for source setext headings
                    if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                        i += 1;
                    }
                } else {
                    // For ATX or ATX Closed style
                    let converted = HeadingUtils::convert_heading_style(&heading, &target_style);
                    fixed_lines.push(format!("{}{}", " ".repeat(indentation), converted));

                    // Skip the underline for source setext headings
                    if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                        i += 1;
                    }
                }
            } else {
                // Not a heading, just copy the line
                fixed_lines.push(line.to_string());
            }
            
            i += 1;
        }

        Ok(fixed_lines.join("\n"))
    }
} 