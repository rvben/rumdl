use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref UNDERSCORE_PATTERN: Regex = Regex::new(r"_[^_\\]+_").unwrap();
    static ref ASTERISK_PATTERN: Regex = Regex::new(r"\*[^*\\]+\*").unwrap();
    static ref URL_PATTERN: Regex = Regex::new(r"https?://[^\s)]+").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
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
        let mut in_code = false;
        let mut code_start = 0;
        let mut backtick_count = 0;
        let mut current_backticks = 0;

        for (i, c) in text.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    // Start counting backticks for potential code span start
                    current_backticks = 1;
                    code_start = i;
                } else if i > 0 && text.chars().nth(i - 1) == Some('`') {
                    // Continue counting backticks
                    current_backticks += 1;
                } else if current_backticks == backtick_count {
                    // Found matching end backticks
                    if pos >= code_start && pos <= i {
                        return true;
                    }
                    in_code = false;
                    current_backticks = 1;
                    code_start = i;
                }
            } else {
                if current_backticks > 0 && !in_code {
                    // Just finished counting backticks at start of potential code span
                    backtick_count = current_backticks;
                    in_code = true;
                }
                current_backticks = 0;
            }
        }

        // Check if we're still in a code span at the end
        in_code && pos >= code_start
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
        let line_index = LineIndex::new(content.to_string());

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

        let mut in_code_block = false;
        for (line_num, line) in content.lines().enumerate() {
            // Handle code block transitions
            if CODE_BLOCK_PATTERN.find(line).is_some() {
                in_code_block = !in_code_block;
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            for m in emphasis_regex.find_iter(line) {
                // Skip this match if it's escaped or within a URL/code
                if self.is_escaped(line, m.start())
                    || self.is_in_url(line, m.start())
                    || self.is_in_inline_code(line, m.start())
                {
                    continue;
                }

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
                        range: line_index.line_col_to_byte_range(line_num + 1, m.start() + 1),
                        replacement: self.convert_style(m.as_str(), found_style, expected_style),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
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
