use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnorderedListStyle {
    Asterisk,  // * * *
    Plus,      // + + +
    Dash,      // - - -
    Consistent, // Use first marker style found
}

impl Default for UnorderedListStyle {
    fn default() -> Self {
        Self::Consistent
    }
}

#[derive(Debug)]
pub struct MD004UnorderedListStyle {
    pub style: UnorderedListStyle,
}

impl Default for MD004UnorderedListStyle {
    fn default() -> Self {
        Self {
            style: UnorderedListStyle::default(),
        }
    }
}

impl MD004UnorderedListStyle {
    pub fn new(style: UnorderedListStyle) -> Self {
        Self { style }
    }

    fn is_ordered_list(line: &str) -> bool {
        let re = Regex::new(r"^\s*\d+[.)]\s").unwrap();
        re.is_match(line)
    }

    fn get_list_marker_and_indent(line: &str) -> Option<(char, usize)> {
        let re = Regex::new(r"^(\s*)([*+-])(?:\s+[^*+\-\s]|\s*$)").unwrap();
        if Self::is_ordered_list(line) {
            return None;
        }
        re.captures(line).map(|cap| {
            let indent = cap[1].len();
            let marker = cap[2].chars().next().unwrap();
            (marker, indent)
        })
    }

    fn detect_first_marker_style(&self, content: &str) -> Option<UnorderedListStyle> {
        let re = Regex::new(r"^(\s*)([*+-])(?:\s+[^*+\-\s]|\s*$)").unwrap();
        let mut in_code_block = false;
        let mut in_blockquote = false;

        for line in content.lines() {
            let trimmed = line.trim_start();
            
            // Handle code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            // Handle blockquotes
            if trimmed.starts_with('>') {
                in_blockquote = true;
                continue;
            } else if trimmed.is_empty() {
                in_blockquote = false;
                continue;
            }
            if in_blockquote {
                continue;
            }

            if Self::is_ordered_list(line) {
                continue;
            }

            if let Some(cap) = re.captures(line) {
                return Some(match &cap[2] {
                    "*" => UnorderedListStyle::Asterisk,
                    "+" => UnorderedListStyle::Plus,
                    "-" => UnorderedListStyle::Dash,
                    _ => unreachable!(),
                });
            }
        }
        None
    }

    fn get_marker_char(style: UnorderedListStyle) -> char {
        match style {
            UnorderedListStyle::Asterisk => '*',
            UnorderedListStyle::Plus => '+',
            UnorderedListStyle::Dash => '-',
            UnorderedListStyle::Consistent => unreachable!(),
        }
    }
}

impl Rule for MD004UnorderedListStyle {
    fn name(&self) -> &'static str {
        "MD004"
    }

    fn description(&self) -> &'static str {
        "Unordered list style should be consistent throughout the document"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let target_style = match self.style {
            UnorderedListStyle::Consistent => {
                self.detect_first_marker_style(content)
                    .unwrap_or(UnorderedListStyle::Dash)
            }
            style => style,
        };

        let mut in_code_block = false;
        let mut in_blockquote = false;

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            
            // Handle code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            // Handle blockquotes
            if trimmed.starts_with('>') {
                in_blockquote = true;
                continue;
            } else if trimmed.is_empty() {
                in_blockquote = false;
                continue;
            }
            if in_blockquote {
                continue;
            }

            // Handle list items
            if let Some((marker, indent)) = Self::get_list_marker_and_indent(line) {
                let current_style = match marker {
                    '*' => UnorderedListStyle::Asterisk,
                    '+' => UnorderedListStyle::Plus,
                    '-' => UnorderedListStyle::Dash,
                    _ => unreachable!(),
                };

                if current_style != target_style {
                    warnings.push(LintWarning {
                        message: format!(
                            "Unordered list marker '{}' should be '{}'",
                            marker,
                            Self::get_marker_char(target_style)
                        ),
                        line: line_num + 1,
                        column: indent + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: indent + 1,
                            replacement: format!(
                                "{}{}{}",
                                " ".repeat(indent),
                                Self::get_marker_char(target_style),
                                &line[indent + 1..]
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let target_style = match self.style {
            UnorderedListStyle::Consistent => {
                self.detect_first_marker_style(content)
                    .unwrap_or(UnorderedListStyle::Dash)
            }
            style => style,
        };

        let mut result = String::new();
        let mut in_code_block = false;
        let mut in_blockquote = false;

        for line in content.lines() {
            let trimmed = line.trim_start();
            
            // Handle code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // Handle blockquotes
            if trimmed.starts_with('>') {
                in_blockquote = true;
                result.push_str(line);
                result.push('\n');
                continue;
            } else if trimmed.is_empty() {
                in_blockquote = false;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_blockquote {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // Fix list markers
            if let Some((_, indent)) = Self::get_list_marker_and_indent(line) {
                result.push_str(&format!(
                    "{}{}{}",
                    " ".repeat(indent),
                    Self::get_marker_char(target_style),
                    &line[indent + 1..]
                ));
                result.push('\n');
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        Ok(result)
    }
} 