use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::strong_style::StrongStyle;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref UNDERSCORE_PATTERN: Regex = Regex::new(r"__[^_\\]+__").unwrap();
    static ref ASTERISK_PATTERN: Regex = Regex::new(r"\*\*[^*\\]+\*\*").unwrap();
}

/// Rule MD050: Strong style should be consistent
///
/// This rule is triggered when strong markers (** or __) are used in an inconsistent way.
pub struct MD050StrongStyle {
    style: StrongStyle,
}

impl MD050StrongStyle {
    pub fn new(style: StrongStyle) -> Self {
        Self { style }
    }

    fn detect_style(&self, content: &str) -> Option<StrongStyle> {
        // Find the first occurrence of either style

        let first_asterisk = ASTERISK_PATTERN.find(content);

        let first_underscore = UNDERSCORE_PATTERN.find(content);

        match (first_asterisk, first_underscore) {
            (Some(a), Some(u)) => {
                // Whichever pattern appears first determines the style
                if a.start() < u.start() {
                    Some(StrongStyle::Asterisk)
                } else {
                    Some(StrongStyle::Underscore)
                }
            }
            (Some(_), None) => Some(StrongStyle::Asterisk),
            (None, Some(_)) => Some(StrongStyle::Underscore),
            (None, None) => None,
        }
    }

    fn is_escaped(&self, text: &str, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }

        let mut backslash_count = 0;
        let mut i = pos;
        while i > 0 {
            i -= 1;
            let c = text.chars().nth(i).unwrap_or(' ');
            if c != '\\' {
                break;
            }
            backslash_count += 1;
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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let target_style = match self.style {
            StrongStyle::Consistent => self.detect_style(content).unwrap_or(StrongStyle::Asterisk),
            _ => self.style,
        };

        let strong_regex = match target_style {
            StrongStyle::Asterisk => &*UNDERSCORE_PATTERN,
            StrongStyle::Underscore => &*ASTERISK_PATTERN,
            StrongStyle::Consistent => unreachable!(),
        };

        for (line_num, line) in content.lines().enumerate() {
            for m in strong_regex.find_iter(line) {
                if !self.is_escaped(line, m.start()) {
                    let text = &line[m.start() + 2..m.end() - 2];
                    let message = match target_style {
                        StrongStyle::Asterisk => "Strong emphasis should use asterisks",
                        StrongStyle::Underscore => "Strong emphasis should use underscores",
                        StrongStyle::Consistent => unreachable!(),
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num + 1,
                        column: m.start() + 1,
                        message: message.to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(line_num + 1, m.start() + 1),
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
        let _line_index = LineIndex::new(content.to_string());

        let target_style = match self.style {
            StrongStyle::Consistent => self.detect_style(content).unwrap_or(StrongStyle::Asterisk),
            _ => self.style,
        };

        let strong_regex = match target_style {
            StrongStyle::Asterisk => &*UNDERSCORE_PATTERN,
            StrongStyle::Underscore => &*ASTERISK_PATTERN,
            StrongStyle::Consistent => unreachable!(),
        };

        // Store matches with their positions

        let matches: Vec<(usize, usize)> = strong_regex
            .find_iter(content)
            .filter(|m| !self.is_escaped(content, m.start()))
            .map(|m| (m.start(), m.end()))
            .collect();

        // Process matches in reverse order to maintain correct indices

        let mut result = content.to_string();
        for (start, end) in matches.into_iter().rev() {
            let text = &result[start + 2..end - 2];
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
