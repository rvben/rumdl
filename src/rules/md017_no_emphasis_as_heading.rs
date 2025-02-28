use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref SINGLE_EMPHASIS_PATTERN: Regex = Regex::new(r"^\s*[*_]([^*_\n]+)[*_]\s*$").unwrap();
    static ref DOUBLE_EMPHASIS_PATTERN: Regex = Regex::new(r"^\s*(?:\*\*|__)([^*_\n]+)(?:\*\*|__)\s*$").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug)]
pub struct MD017NoEmphasisAsHeading {
    pub allow_emphasis_headings: bool,
}

impl Default for MD017NoEmphasisAsHeading {
    fn default() -> Self {
        Self { allow_emphasis_headings: false }
    }
}

impl MD017NoEmphasisAsHeading {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_emphasis_headings(allow_emphasis_headings: bool) -> Self {
        Self { allow_emphasis_headings }
    }

    fn is_single_emphasis_heading(&self, line: &str) -> bool {
        SINGLE_EMPHASIS_PATTERN.is_match(line)
    }

    fn is_double_emphasis_heading(&self, line: &str) -> bool {
        DOUBLE_EMPHASIS_PATTERN.is_match(line)
    }

    fn fix_single_emphasis(&self, line: &str) -> String {
        let captures = SINGLE_EMPHASIS_PATTERN.captures(line).unwrap();
        let content = captures.get(1).unwrap().as_str();
        format!("# {}", content)
    }

    fn fix_double_emphasis(&self, line: &str) -> String {
        let captures = DOUBLE_EMPHASIS_PATTERN.captures(line).unwrap();
        let content = captures.get(1).unwrap().as_str();
        format!("## {}", content)
    }
}

impl Rule for MD017NoEmphasisAsHeading {
    fn name(&self) -> &'static str {
        "MD017"
    }

    fn description(&self) -> &'static str {
        "Emphasis should not be used as a heading"
    }

    fn check(&self, content: &str) -> LintResult {
        if self.allow_emphasis_headings {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block {
                if self.is_single_emphasis_heading(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Single emphasis should not be used as a heading".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_single_emphasis(line),
                        }),
                    });
                } else if self.is_double_emphasis_heading(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Double emphasis should not be used as a heading".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_double_emphasis(line),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if self.allow_emphasis_headings {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
            } else if !in_code_block {
                if self.is_single_emphasis_heading(line) {
                    result.push_str(&self.fix_single_emphasis(line));
                } else if self.is_double_emphasis_heading(line) {
                    result.push_str(&self.fix_double_emphasis(line));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 