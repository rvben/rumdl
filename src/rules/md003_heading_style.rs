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

    fn determine_style(content: &str) -> Option<HeadingStyle> {
        let mut atx_count = 0;
        let mut atx_closed_count = 0;
        let mut setext_count = 0;
        let lines: Vec<&str> = content.lines().collect();
        
        let mut i = 0;
        while i < lines.len() {
            let remaining = &lines[i..].join("\n");
            if let Some(heading) = HeadingUtils::parse_heading(remaining, 0) {
                match heading.style {
                    HeadingStyle::Atx => atx_count += 1,
                    HeadingStyle::AtxClosed => atx_closed_count += 1,
                    HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                        setext_count += 1;
                        i += 1; // Skip underline
                    }
                }
            }
            i += 1;
        }

        // Return the style specified in the struct, or determine based on usage
        Some(if setext_count > 0 && (setext_count >= atx_count && setext_count >= atx_closed_count) {
            HeadingStyle::Setext1
        } else if atx_closed_count > 0 && atx_closed_count >= atx_count {
            HeadingStyle::AtxClosed
        } else {
            HeadingStyle::Atx
        })
    }

    fn is_setext_underline(line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed.chars().all(|c| c == '=' || c == '-')
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
        let mut fixed_lines = Vec::new();
        let target_style = self.style;
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let current_and_next = if i + 1 < lines.len() {
                &lines[i..=i+1].join("\n")
            } else {
                lines[i]
            };
            
            if let Some(heading) = HeadingUtils::parse_heading(current_and_next, 0) {
                let indentation = HeadingUtils::get_indentation(lines[i]);
                
                // Convert heading while preserving formatting
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
                } else if matches!(target_style, HeadingStyle::AtxClosed) {
                    // For closed ATX headings
                    let hashes = "#".repeat(heading.level);
                    fixed_lines.push(format!("{}{} {} {}", 
                        " ".repeat(indentation), 
                        hashes, 
                        heading.text.trim(), 
                        hashes
                    ));
                    
                    // Skip the underline for source setext headings
                    if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                        i += 1;
                    }
                } else {
                    // For regular ATX headings
                    fixed_lines.push(format!("{}{} {}", 
                        " ".repeat(indentation), 
                        "#".repeat(heading.level), 
                        heading.text.trim()
                    ));
                    
                    // Skip the underline for source setext headings
                    if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                        i += 1;
                    }
                }
            } else {
                // Not a heading, keep the line as is
                fixed_lines.push(lines[i].to_string());
            }
            i += 1;
        }

        // Preserve trailing newline if original content had one
        Ok(fixed_lines.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }
} 