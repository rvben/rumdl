use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
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
        let mut result = String::new();
        let target_style = self.style;
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let remaining = &lines[i..].join("\n");
            if let Some(heading) = HeadingUtils::parse_heading(remaining, 0) {
                let indentation = HeadingUtils::get_indentation(lines[i]);
                
                // Convert heading while preserving formatting
                let replacement = if matches!(target_style, HeadingStyle::Setext1 | HeadingStyle::Setext2) 
                    && heading.level <= 2 {
                    let text = heading.text.clone();
                    let underline = if heading.level == 1 { "=" } else { "-" }
                        .repeat(text.trim().chars().count().max(3));
                    format!("{}\n{}", text, underline)
                } else if matches!(target_style, HeadingStyle::AtxClosed) {
                    let hashes = "#".repeat(heading.level);
                    format!("{} {} {}", hashes, heading.text.trim(), hashes)
                } else {
                    format!("{} {}", "#".repeat(heading.level), heading.text.trim())
                };

                // Add indentation and handle newlines
                let replacement_lines: Vec<&str> = replacement.lines().collect();
                for (j, line) in replacement_lines.iter().enumerate() {
                    if j > 0 {
                        result.push('\n');
                    }
                    result.push_str(&format!("{}{}", " ".repeat(indentation), line));
                }

                // Handle spacing between headings
                if i + 1 < lines.len() {
                    if matches!(target_style, HeadingStyle::Setext1 | HeadingStyle::Setext2) 
                        && heading.level <= 2 
                        && !matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                        result.push_str("\n\n");
                    } else {
                        result.push('\n');
                    }
                }

                // Skip the underline for setext headings
                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                    i += 1;
                }
            } else {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(lines[i]);
            }
            i += 1;
        }

        Ok(result)
    }
} 