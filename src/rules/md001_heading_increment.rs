use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::HeadingUtils;
use crate::HeadingStyle;

#[derive(Debug, Default)]
pub struct MD001HeadingIncrement;

impl Rule for MD001HeadingIncrement {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut prev_level = 0;
        let mut prev_style = None;

        let lines: Vec<&str> = content.lines().collect();
        for line_num in 0..lines.len() {
            if let Some(heading) = HeadingUtils::parse_heading(content, line_num) {
                if prev_level > 0 && heading.level > prev_level + 1 {
                    let indentation = HeadingUtils::get_indentation(lines[line_num]);
                    let mut fixed_heading = heading.clone();
                    fixed_heading.level = prev_level + 1;
                    let style = prev_style.unwrap_or(heading.style.clone());
                    let replacement = HeadingUtils::convert_heading_style(
                        &fixed_heading,
                        &style
                    );
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: indentation + 1,
                        message: format!("Heading level should be {} for this level", prev_level + 1),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: indentation + 1,
                            replacement: format!("{}{}", " ".repeat(indentation), replacement),
                        }),
                    });
                }
                prev_level = heading.level;
                prev_style = Some(heading.style);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut prev_level = 0;
        let mut prev_style = None;
        
        let lines: Vec<&str> = content.lines().collect();
        let mut line_num = 0;
        while line_num < lines.len() {
            let line = lines[line_num];
            if let Some(heading) = HeadingUtils::parse_heading(content, line_num) {
                if prev_level > 0 && heading.level > prev_level + 1 {
                    let indentation = HeadingUtils::get_indentation(line);
                    let mut fixed_heading = heading.clone();
                    fixed_heading.level = prev_level + 1;
                    let style = prev_style.unwrap_or(fixed_heading.style.clone());
                    let replacement = HeadingUtils::convert_heading_style(
                        &fixed_heading,
                        &style
                    );
                    result.push_str(&format!("{}{}\n", " ".repeat(indentation), replacement));
                    prev_level = prev_level + 1;
                } else {
                    result.push_str(line);
                    result.push('\n');
                    prev_level = heading.level;
                }
                prev_style = Some(heading.style);
                
                // Skip the next line if this was a setext heading
                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                    line_num += 1;
                    if line_num < lines.len() {
                        result.push_str(lines[line_num]);
                        result.push('\n');
                    }
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
            line_num += 1;
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 