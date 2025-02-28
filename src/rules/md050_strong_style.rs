use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

/// Rule MD050: Strong emphasis style should be consistent
pub struct MD050StrongStyle {
    style: StrongStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrongStyle {
    Asterisk,
    Underscore,
    Consistent,
}

impl MD050StrongStyle {
    pub fn new(style: StrongStyle) -> Self {
        Self { style }
    }

    fn detect_style(&self, content: &str) -> Option<StrongStyle> {
        let asterisk_count = content.matches("**").count();
        let underscore_count = content.matches("__").count();

        if asterisk_count > underscore_count {
            Some(StrongStyle::Asterisk)
        } else if underscore_count > asterisk_count {
            Some(StrongStyle::Underscore)
        } else {
            None
        }
    }

    fn is_escaped(&self, text: &str, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }
        let mut backslash_count = 0;
        for c in text[..pos].chars().rev() {
            if c == '\\' {
                backslash_count += 1;
            } else {
                break;
            }
        }
        backslash_count % 2 == 1
    }
}

impl Rule for MD050StrongStyle {
    fn name(&self) -> &'static str {
        "MD050"
    }

    fn description(&self) -> &'static str {
        "Strong emphasis style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let target_style = match self.style {
            StrongStyle::Consistent => self.detect_style(content).unwrap_or(StrongStyle::Asterisk),
            _ => self.style.clone(),
        };

        let strong_regex = match target_style {
            StrongStyle::Asterisk => Regex::new(r"__[^_\\]+__").unwrap(),
            StrongStyle::Underscore => Regex::new(r"\*\*[^*\\]+\*\*").unwrap(),
            StrongStyle::Consistent => unreachable!(),
        };

        for (line_num, line) in content.lines().enumerate() {
            for m in strong_regex.find_iter(line) {
                if !self.is_escaped(line, m.start()) {
                    let text = &line[m.start()+2..m.end()-2];
                    let message = match target_style {
                        StrongStyle::Asterisk => "Strong emphasis should use asterisks",
                        StrongStyle::Underscore => "Strong emphasis should use underscores",
                        StrongStyle::Consistent => unreachable!(),
                    };

                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: m.start() + 1,
                        message: message.to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: m.start() + 1,
                            replacement: match target_style {
                                StrongStyle::Asterisk => format!("**{}**", text),
                                StrongStyle::Underscore => format!("__{}__", text),
                                StrongStyle::Consistent => unreachable!(),
                            },
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let target_style = match self.style {
            StrongStyle::Consistent => self.detect_style(content).unwrap_or(StrongStyle::Asterisk),
            _ => self.style.clone(),
        };

        let mut result = content.to_string();
        let strong_regex = match target_style {
            StrongStyle::Asterisk => Regex::new(r"__[^_\\]+__").unwrap(),
            StrongStyle::Underscore => Regex::new(r"\*\*[^*\\]+\*\*").unwrap(),
            StrongStyle::Consistent => unreachable!(),
        };

        // Store matches with their positions
        let matches: Vec<(usize, usize)> = strong_regex.find_iter(&result)
            .filter(|m| !self.is_escaped(&result, m.start()))
            .map(|m| (m.start(), m.end()))
            .collect();

        // Process matches in reverse order to maintain correct indices
        for (start, end) in matches.into_iter().rev() {
            let text = &result[start+2..end-2];
            let replacement = match target_style {
                StrongStyle::Asterisk => format!("**{}**", text),
                StrongStyle::Underscore => format!("__{}__", text),
                StrongStyle::Consistent => unreachable!(),
            };
            result.replace_range(start..end, &replacement);
        }

        Ok(result)
    }
} 