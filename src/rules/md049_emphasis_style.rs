use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref UNDERSCORE_PATTERN: Regex = Regex::new(r"_[^_\\]+_").unwrap();
    static ref ASTERISK_PATTERN: Regex = Regex::new(r"\*[^*\\]+\*").unwrap();
    static ref URL_PATTERN: Regex = Regex::new(r"https?://[^\s)]+").unwrap();
    static ref INLINE_CODE_PATTERN: Regex = Regex::new(r"`[^`]+`").unwrap();
}

/// Rule MD049: Emphasis style should be consistent
pub struct MD049EmphasisStyle {
    style: EmphasisStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmphasisStyle {
    Asterisk,
    Underscore,
    Consistent,
}

impl MD049EmphasisStyle {
    pub fn new(style: EmphasisStyle) -> Self {
        Self { style }
    }

    fn detect_style(&self, content: &str) -> Option<EmphasisStyle> {
        // Find the first occurrence of either style

        let first_asterisk = ASTERISK_PATTERN.find(content);

        let first_underscore = UNDERSCORE_PATTERN.find(content);

        match (first_asterisk, first_underscore) {
            (Some(a), Some(u)) => {
                // Whichever pattern appears first determines the style
                if a.start() < u.start() {
                    Some(EmphasisStyle::Asterisk)
                } else {
                    Some(EmphasisStyle::Underscore)
                }
            }
            (Some(_), None) => Some(EmphasisStyle::Asterisk),
            (None, Some(_)) => Some(EmphasisStyle::Underscore),
            (None, None) => None,
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

    fn is_in_url(&self, text: &str, pos: usize) -> bool {
        for url_match in URL_PATTERN.find_iter(text) {
            if pos >= url_match.start() && pos < url_match.end() {
                return true;
            }
        }
        false
    }

    fn is_in_inline_code(&self, text: &str, pos: usize) -> bool {
        for code_match in INLINE_CODE_PATTERN.find_iter(text) {
            if pos >= code_match.start() && pos < code_match.end() {
                return true;
            }
        }
        false
    }

    fn convert_style(
        &self,
        text: &str,
        found_style: EmphasisStyle,
        expected_style: EmphasisStyle,
    ) -> String {
        match (found_style, expected_style) {
            (EmphasisStyle::Asterisk, EmphasisStyle::Underscore) => {
                format!("_{}_", &text[1..text.len() - 1])
            }
            (EmphasisStyle::Underscore, EmphasisStyle::Asterisk) => {
                format!("*{}*", &text[1..text.len() - 1])
            }
            _ => unreachable!(),
        }
    }
}

impl Rule for MD049EmphasisStyle {
    fn name(&self) -> &'static str {
        "MD049"
    }

    fn description(&self) -> &'static str {
        "Emphasis style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let target_style = match self.style {
            EmphasisStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(EmphasisStyle::Asterisk),
            _ => self.style,
        };

        let emphasis_regex = match target_style {
            EmphasisStyle::Asterisk => &*UNDERSCORE_PATTERN,
            EmphasisStyle::Underscore => &*ASTERISK_PATTERN,
            EmphasisStyle::Consistent => unreachable!(),
        };

        for (line_num, line) in content.lines().enumerate() {
            for m in emphasis_regex.find_iter(line) {
                // Skip this match if it's escaped or within a URL/code
                if self.is_escaped(line, m.start())
                    || self.is_in_url(line, m.start())
                    || self.is_in_inline_code(line, m.start())
                {
                    continue;
                }

                let _text = &line[m.start() + 1..m.end() - 1];
                let expected_style = match target_style {
                    EmphasisStyle::Asterisk => EmphasisStyle::Asterisk,
                    EmphasisStyle::Underscore => EmphasisStyle::Underscore,
                    EmphasisStyle::Consistent => unreachable!(),
                };
                let found_style = if emphasis_regex.as_str() == UNDERSCORE_PATTERN.as_str() {
                    EmphasisStyle::Underscore
                } else if emphasis_regex.as_str() == ASTERISK_PATTERN.as_str() {
                    EmphasisStyle::Asterisk
                } else {
                    unreachable!()
                };

                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: m.start() + 1,
                    message: format!(
                        "Emphasis style should be {} ({})",
                        if expected_style == EmphasisStyle::Asterisk {
                            "asterisk"
                        } else {
                            "underscore"
                        },
                        if expected_style == EmphasisStyle::Asterisk {
                            "*"
                        } else {
                            "_"
                        }
                    ),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, m.start() + 1),
                        replacement: self.convert_style(m.as_str(), found_style, expected_style),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let target_style = match self.style {
            EmphasisStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(EmphasisStyle::Asterisk),
            _ => self.style,
        };

        let emphasis_regex = match target_style {
            EmphasisStyle::Asterisk => &*UNDERSCORE_PATTERN,
            EmphasisStyle::Underscore => &*ASTERISK_PATTERN,
            EmphasisStyle::Consistent => unreachable!(),
        };

        // Store matches with their positions, filtering out URLs and code

        let matches: Vec<(usize, usize)> = emphasis_regex
            .find_iter(content)
            .filter(|m| {
                !self.is_escaped(content, m.start())
                    && !self.is_in_url(content, m.start())
                    && !self.is_in_inline_code(content, m.start())
            })
            .map(|m| (m.start(), m.end()))
            .collect();

        // Process matches in reverse order to maintain correct indices

        let mut result = content.to_string();
        for (start, end) in matches.into_iter().rev() {
            let _text = &result[start + 1..end - 1];
            let replacement = self.convert_style(
                &result[start..end],
                if emphasis_regex.as_str() == UNDERSCORE_PATTERN.as_str() {
                    EmphasisStyle::Underscore
                } else if emphasis_regex.as_str() == ASTERISK_PATTERN.as_str() {
                    EmphasisStyle::Asterisk
                } else {
                    unreachable!()
                },
                target_style,
            );
            result.replace_range(start..end, &replacement);
        }

        Ok(result)
    }
}
