use regex::Regex;

/// Represents different styles of Markdown headings
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum HeadingStyle {
    Atx,             // # Heading
    AtxClosed,       // # Heading #
    Setext1,         // Heading
                     // =======
    Setext2,         // Heading
                     // -------
}

/// Represents a heading in a Markdown document
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub style: HeadingStyle,
}

/// Utility functions for working with Markdown headings
pub struct HeadingUtils;

impl HeadingUtils {
    /// Check if a line is an ATX heading (starts with #)
    pub fn is_atx_heading(line: &str) -> bool {
        let re = Regex::new(r"^#{1,6}(?:\s+.+|\s*$)").unwrap();
        re.is_match(line)
    }

    /// Parse a line into a Heading struct if it's a valid heading
    pub fn parse_heading(content: &str, line_num: usize) -> Option<Heading> {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return None;
        }

        let line = lines[line_num];
        
        // ATX style (#)
        if let Some(atx_heading) = Self::parse_atx_heading(line) {
            return Some(atx_heading);
        }

        // Check for setext style (=== or ---)
        if line_num + 1 < lines.len() {
            let next_line = lines[line_num + 1];
            let next_trimmed = next_line.trim();

            // Check if next line is a valid setext underline
            if !next_trimmed.is_empty() && next_trimmed.chars().all(|c| c == '=' || c == '-') {
                let level = if next_trimmed.starts_with('=') { 1 } else { 2 };
                let style = if level == 1 { HeadingStyle::Setext1 } else { HeadingStyle::Setext2 };
                
                // Get the indentation of both lines
                let heading_indent = line.len() - line.trim_start().len();
                let underline_indent = next_line.len() - next_line.trim_start().len();
                
                // For setext headings, we allow any indentation as long as it's consistent
                if heading_indent == underline_indent {
                    return Some(Heading { 
                        level, 
                        text: line.trim_start().to_string(), // Keep any trailing spaces and formatting
                        style 
                    });
                }
            }
        }

        None
    }

    fn parse_atx_heading(line: &str) -> Option<Heading> {
        let re = Regex::new(r"^(#{1,6})(?:\s+(.+?))?(?:\s+#*)?$").unwrap();
        if let Some(cap) = re.captures(line) {
            let level = cap[1].len();
            let text = cap.get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            let style = if line.trim_end().matches('#').count() > level {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            Some(Heading { level, text, style })
        } else {
            None
        }
    }

    /// Convert a heading to a different style
    pub fn convert_heading_style(heading: &Heading, target_style: &HeadingStyle) -> String {
        match target_style {
            HeadingStyle::Atx => {
                format!("{}{}", "#".repeat(heading.level), 
                    if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
            },
            HeadingStyle::AtxClosed => {
                if heading.level > 6 {
                    format!("{}{}", "#".repeat(heading.level),
                        if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
                } else {
                    let hashes = "#".repeat(heading.level);
                    if heading.text.is_empty() {
                        format!("{} {}", hashes, hashes)
                    } else {
                        format!("{} {} {}", hashes, heading.text.trim(), hashes)
                    }
                }
            },
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                if heading.level > 2 {
                    // Fall back to ATX style for levels > 2
                    format!("{}{}", "#".repeat(heading.level),
                        if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
                } else {
                    let text = heading.text.clone(); // Keep original formatting
                    let underline_char = if heading.level == 1 { '=' } else { '-' };
                    let underline = underline_char.to_string().repeat(text.trim().chars().count().max(3));
                    format!("{}\n{}", text, underline)
                }
            }
        }
    }

    pub fn get_indentation(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    pub fn get_heading_text(line: &str) -> Option<String> {
        if let Some(heading) = Self::parse_heading(line, 0) {
            Some(heading.text)
        } else {
            None
        }
    }
} 