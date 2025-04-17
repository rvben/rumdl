use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::document_structure::DocumentStructure;

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
}

impl Rule for MD041FirstLineHeading {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top level heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        if content.trim().is_empty() {
            return Ok(warnings);
        }
        if self.has_front_matter_title(content) {
            return Ok(warnings);
        }
        let structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut first_line = 0;
        // Skip front matter
        let mut start = 0;
        if structure.has_front_matter {
            if let Some((_, end)) = structure.front_matter_range {
                start = end;
            }
        }
        // Skip blank lines after front matter
        for (i, line) in lines.iter().enumerate().skip(start) {
            if !line.trim().is_empty() {
                first_line = i + 1; // 1-indexed
                break;
            }
        }
        if first_line == 0 {
            // No non-blank lines after front matter
            return Ok(warnings);
        }
        // Check if the first non-blank, non-front-matter line is a heading of the required level
        if structure.heading_lines.is_empty() || structure.heading_lines[0] != first_line || structure.heading_levels[0] != self.level {
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: first_line,
                column: 1,
                message: format!(
                    "First line in file should be a level {} heading",
                    self.level
                ),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: LineIndex::new(content.to_string()).line_col_to_byte_range(first_line, 1),
                    replacement: format!("{} Title\n{}", "#".repeat(self.level), lines[first_line - 1]),
                }),
            });
        }
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let content = crate::rules::front_matter_utils::FrontMatterUtils::fix_malformed_front_matter(content);
        if content.trim().is_empty() || self.has_front_matter_title(&content) {
            return Ok(content.to_string());
        }
        let structure = DocumentStructure::new(&content);
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        if structure.heading_lines.is_empty() {
            // Add a new title at the beginning
            result.push_str(&format!("{} Title\n\n{}", "#".repeat(self.level), content));
        } else {
            let first_heading_line = structure.heading_lines[0];
            let first_heading_level = structure.heading_levels[0];
            if first_heading_level != self.level {
                // Fix the existing heading level
                for (i, line) in lines.iter().enumerate() {
                    if i + 1 == first_heading_line {
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
                // No fix needed, return original
                return Ok(content.to_string());
            }
        }
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
