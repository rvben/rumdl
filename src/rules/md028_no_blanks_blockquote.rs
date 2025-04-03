use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::blockquote_utils::BlockquoteUtils;

#[derive(Debug, Default)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Checks if a line is completely empty (just whitespace)
    fn is_completely_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Generates the replacement for a blank blockquote line
    fn get_replacement(indent: &str, level: usize) -> String {
        let mut result = indent.to_string();

        // For nested blockquotes: ">>" or ">" based on level
        for _ in 0..level {
            result.push('>');
        }
        // Add a single space after the last '>'
        result.push(' ');

        result
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let mut in_blockquote = false;

        for (i, &line) in lines.iter().enumerate() {
            if Self::is_completely_empty_line(line) {
                // A completely empty line separates blockquotes
                in_blockquote = false;
                continue;
            }

            if BlockquoteUtils::is_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);

                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }

                // Check if this is an empty blockquote line
                if BlockquoteUtils::is_empty_blockquote(line) {
                    let indent = BlockquoteUtils::extract_indentation(line);

                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: "Blank line inside blockquote".to_string(),
                        line: i + 1,
                        column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: Self::get_replacement(&indent, level),
                        }),
                    });
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::with_capacity(lines.len());

        let mut in_blockquote = false;

        for line in lines {
            if Self::is_completely_empty_line(line) {
                // Add empty lines as-is
                in_blockquote = false;
                result.push(line.to_string());
                continue;
            }

            if BlockquoteUtils::is_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);

                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }

                // Handle empty blockquote lines
                if BlockquoteUtils::is_empty_blockquote(line) {
                    let indent = BlockquoteUtils::extract_indentation(line);
                    result.push(Self::get_replacement(&indent, level));
                } else {
                    // Add the line as is
                    result.push(line.to_string());
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
                result.push(line.to_string());
            }
        }

        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }
}
