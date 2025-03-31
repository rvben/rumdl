use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Pattern for quick check if content has any headings at all
    static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();
}

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

    #[inline]
    fn get_special_lines(&self, content: &str) -> HashSet<usize> {
        let mut special_lines = HashSet::new();
        
        // Add code block lines
        for (i, _) in content.lines().enumerate() {
            if HeadingUtils::is_in_code_block(content, i) || 
               FrontMatterUtils::is_in_front_matter(content, i) {
                special_lines.insert(i);
            }
        }
        
        special_lines
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
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        // Quick check if there are any headings at all
        if !HEADING_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut found_title = false;
        
        // Pre-compute special lines (code blocks or front matter)
        let special_lines = self.get_special_lines(content);

        // Process each line
        for (i, line) in content.lines().enumerate() {
            // Skip processing if line is in a code block or front matter
            if special_lines.contains(&i) {
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
                                range: line_index
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
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Quick check if there are any headings at all
        if !HEADING_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let mut found_first_title = false;
        
        // Pre-compute special lines (code blocks or front matter)
        let special_lines = self.get_special_lines(content);

        for (i, line) in content.lines().enumerate() {
            // Don't modify lines in code blocks or front matter
            if special_lines.contains(&i) {
                result.push_str(line);
            } else {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    let level = trimmed.chars().take_while(|&c| c == '#').count();
                    let indent = line.len() - trimmed.len();

                    if level == self.level {
                        if found_first_title {
                            // This is a duplicate level-n heading - add one more # to increase level
                            result.push_str(&" ".repeat(indent));
                            result.push_str(&"#".repeat(level + 1));
                            result.push_str(&trimmed[level..]);
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
            if i < content.lines().count() - 1 {
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
