use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{HeadingStyle, HeadingUtils};

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

        for (line_num, line) in content.lines().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(line, line_num + 1) {
                if self.has_trailing_punctuation(&heading.text) {
                    let indentation = HeadingUtils::get_indentation(line);
                    let fixed_text = self.remove_trailing_punctuation(&heading.text);
                    let replacement = match heading.style {
                        HeadingStyle::Atx => format!("{}{} {}", 
                            " ".repeat(indentation),
                            "#".repeat(heading.level),
                            fixed_text
                        ),
                        HeadingStyle::AtxClosed => format!("{}{} {} {}", 
                            " ".repeat(indentation),
                            "#".repeat(heading.level),
                            fixed_text,
                            "#".repeat(heading.level)
                        ),
                        _ => format!("{}{} {}", 
                            " ".repeat(indentation),
                            "#".repeat(heading.level),
                            fixed_text
                        ),
                    };
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: indentation + 1,
                        message: format!("Trailing punctuation in heading '{}'", heading.text),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: indentation + 1,
                            replacement,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();

        for line in content.lines() {
            if let Some(heading) = HeadingUtils::parse_heading(line, 0) {
                if self.has_trailing_punctuation(&heading.text) {
                    let indentation = HeadingUtils::get_indentation(line);
                    let fixed_text = self.remove_trailing_punctuation(&heading.text);
                    match heading.style {
                        HeadingStyle::Atx => {
                            result.push_str(&format!("{}{} {}\n", 
                                " ".repeat(indentation),
                                "#".repeat(heading.level),
                                fixed_text
                            ));
                        },
                        HeadingStyle::AtxClosed => {
                            result.push_str(&format!("{}{} {} {}\n", 
                                " ".repeat(indentation),
                                "#".repeat(heading.level),
                                fixed_text,
                                "#".repeat(heading.level)
                            ));
                        },
                        _ => {
                            result.push_str(&format!("{}{} {}\n", 
                                " ".repeat(indentation),
                                "#".repeat(heading.level),
                                fixed_text
                            ));
                        },
                    }
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 