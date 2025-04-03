use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use regex::Regex;

#[derive(Debug)]
pub struct MD041FirstLineHeading {
    pub level: usize,
    pub front_matter_title: bool,
}

impl Default for MD041FirstLineHeading {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: true,
        }
    }
}

impl MD041FirstLineHeading {
    pub fn new(level: usize, front_matter_title: bool) -> Self {
        Self {
            level,
            front_matter_title,
        }
    }

    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        FrontMatterUtils::has_front_matter_field(content, "title:")
    }

    fn is_heading_line(&self, line: &str) -> Option<usize> {
        // Check for ATX style heading

        let re = Regex::new(r"^(#{1,6})(?:\s+.+)?(?:\s+#{0,})?$").unwrap();
        if let Some(cap) = re.captures(line) {
            return Some(cap[1].len());
        }

        // Check for Setext style heading would require next line,
        // but not needed for this rule's implementation
        None
    }

    fn find_first_heading(&self, content: &str) -> Option<(usize, usize)> {
        let lines: Vec<&str> = content.lines().collect();

        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for front matter
            if i == 0 && trimmed == "---" {
                in_front_matter = true;
                continue;
            }

            if in_front_matter {
                if trimmed == "---" {
                    in_front_matter = false;
                }
                continue;
            }

            // Skip blank lines after front matter
            if trimmed.is_empty() {
                continue;
            }

            // Check if line is a heading
            if let Some(level) = self.is_heading_line(trimmed) {
                return Some((i + 1, level));
            } else {
                // If we hit non-empty, non-heading content, no first heading exists
                return None;
            }
        }

        None
    }
}

impl Rule for MD041FirstLineHeading {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top level heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        if content.trim().is_empty() {
            return Ok(warnings);
        }

        if self.has_front_matter_title(content) {
            return Ok(warnings);
        }

        match self.find_first_heading(content) {
            None => {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: 1,
                    column: 1,
                    message: format!(
                        "First line in file should be a level {} heading",
                        self.level
                    ),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(1, 1),
                        replacement: format!("{} Title\n\n{}", "#".repeat(self.level), content),
                    }),
                });
            }
            Some((line_num, level)) => {
                if level != self.level {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num,
                        column: 1,
                        message: format!(
                            "First heading should be a level {} heading, found level {}",
                            self.level, level
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(line_num, 1),
                            replacement: format!(
                                "{} {}",
                                "#".repeat(self.level),
                                content
                                    .lines()
                                    .nth(line_num - 1)
                                    .unwrap()
                                    .trim_start()
                                    .trim_start_matches('#')
                                    .trim_start()
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());
        // Apply front matter fixes first if needed

        let content = FrontMatterUtils::fix_malformed_front_matter(content);

        if content.trim().is_empty() || self.has_front_matter_title(&content) {
            return Ok(content);
        }

        let mut result = String::new();

        let lines: Vec<&str> = content.lines().collect();

        match self.find_first_heading(&content) {
            None => {
                // Add a new title at the beginning
                result.push_str(&format!("{} Title\n\n", "#".repeat(self.level)));
                result.push_str(&content);
            }
            Some((line_num, level)) => {
                if level != self.level {
                    // Fix the existing heading level
                    for (i, line) in lines.iter().enumerate() {
                        if i + 1 == line_num {
                            result.push_str(&format!(
                                "{} {}",
                                "#".repeat(self.level),
                                line.trim_start().trim_start_matches('#').trim_start()
                            ));
                        } else {
                            result.push_str(line);
                        }

                        if i < lines.len() - 1 {
                            result.push('\n');
                        }
                    }
                } else {
                    // Heading is already at the correct level
                    return Ok(content);
                }
            }
        }

        // Preserve the original trailing newline state
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
