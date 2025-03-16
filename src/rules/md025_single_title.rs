use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;

#[derive(Debug)]
pub struct MD025SingleTitle {
    level: usize,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, _front_matter_title: &str) -> Self {
        Self { level }
    }
}

impl Rule for MD025SingleTitle {
    fn name(&self) -> &'static str {
        "MD025"
    }

    fn description(&self) -> &'static str {
        "Multiple top-level headings in the same document"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut found_title = false;

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip processing if line is in a code block or front matter
            if HeadingUtils::is_in_code_block(content, i)
                || FrontMatterUtils::is_in_front_matter(content, i)
            {
                continue;
            }

            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count();
                if level == self.level {
                    if found_title {
                        warnings.push(LintWarning {
                            message: format!(
                                "Multiple top-level headings (level {}) in the same document",
                                self.level
                            ),
                            line: i + 1,
                            column: line.find('#').unwrap_or(0) + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index
                                    .line_col_to_byte_range(i + 1, line.find('#').unwrap_or(0) + 1),
                                replacement: format!(
                                    "{} {}",
                                    "#".repeat(level + 1),
                                    &line[level + line.find('#').unwrap_or(0)..]
                                ),
                            }),
                        });
                    }
                    found_title = true;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let mut found_first_title = false;

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Don't modify lines in code blocks or front matter
            if HeadingUtils::is_in_code_block(&content, i)
                || FrontMatterUtils::is_in_front_matter(&content, i)
            {
                result.push_str(line);
            } else {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    let level = trimmed.chars().take_while(|&c| c == '#').count();
                    let indent = line.len() - trimmed.len();

                    if level == self.level {
                        if found_first_title {
                            // This is a duplicate level-n heading - add one more # to increase level
                            let modified = format!(
                                "{}{}{}",
                                " ".repeat(indent),
                                "#".repeat(level + 1),
                                &trimmed[level..]
                            );
                            result.push_str(&modified);
                        } else {
                            // This is the first level-n heading - keep it as is
                            result.push_str(line);
                            found_first_title = true;
                        }
                    } else {
                        // Not a level-n heading, keep it as is
                        result.push_str(line);
                    }
                } else {
                    // Not a heading, keep it as is
                    result.push_str(line);
                }
            }

            // Add newline between lines (except after the last line)
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
