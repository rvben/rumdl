use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern for quick check if content has any headings at all
    static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();
}

#[derive(Debug)]
pub struct MD025SingleTitle {
    level: usize,
    front_matter_title: String,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
        }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, front_matter_title: &str) -> Self {
        Self {
            level,
            front_matter_title: front_matter_title.to_string(),
        }
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
        let structure = DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');
        let structure = DocumentStructure::new(content);
        let mut fixed_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

        // Find all headings at the target level
        let mut found_first = false;
        for (idx, &_line_num) in structure.heading_lines.iter().enumerate() {
            let level = structure.heading_levels[idx];
            if level == self.level {
                if !found_first {
                    found_first = true;
                } else {
                    // Demote this heading to the next level
                    let region = structure.heading_regions[idx];
                    let start = region.0 - 1;
                    let style = if region.0 != region.1 {
                        if lines
                            .get(region.1 - 1)
                            .map_or("", |l| l.trim())
                            .starts_with('=')
                        {
                            crate::rules::heading_utils::HeadingStyle::Setext1
                        } else {
                            crate::rules::heading_utils::HeadingStyle::Setext2
                        }
                    } else {
                        crate::rules::heading_utils::HeadingStyle::Atx
                    };
                    let text = lines[start].trim_start_matches('#').trim();
                    let replacement =
                        crate::rules::heading_utils::HeadingUtils::convert_heading_style(
                            text,
                            (self.level + 1).try_into().unwrap(),
                            style,
                        );
                    fixed_lines[start] = replacement;
                }
            }
        }

        let mut result = fixed_lines.join("\n");
        if ends_with_newline {
            result.push('\n');
        }
        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Check for front matter title if configured
        let mut _found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() && structure.has_front_matter {
            if let Some((start, end)) = structure.front_matter_range {
                // Extract front matter content
                let front_matter_content: String = content
                    .lines()
                    .skip(start - 1) // Convert from 1-indexed to 0-indexed
                    .take(end - start + 1)
                    .collect::<Vec<&str>>()
                    .join("\n");

                // Check if it contains a title field
                _found_title_in_front_matter = front_matter_content.lines().any(|line| {
                    line.trim()
                        .starts_with(&format!("{}:", self.front_matter_title))
                });
            }
        }

        // Find all ATX level-1 headings not in code blocks or indented 4+ spaces
        let lines: Vec<&str> = content.lines().collect();
        let mut target_level_headings = Vec::new();
        for (i, &_line_num) in structure.heading_lines.iter().enumerate() {
            if i < structure.heading_levels.len() && structure.heading_levels[i] == self.level {
                // Use heading_regions to get the correct line for heading text
                let (content_line, _marker_line) = if i < structure.heading_regions.len() {
                    structure.heading_regions[i]
                } else {
                    (i, i)
                };
                let idx = content_line - 1;
                if idx >= lines.len() {
                    continue;
                }
                let line = lines[idx];
                // Only consider ATX headings (not setext)
                let trimmed = line.trim_start();
                let leading_spaces = line.len() - trimmed.len();
                // Ignore if indented 4+ spaces (code block)
                if leading_spaces >= 4 {
                    continue;
                }
                // Only consider lines starting with '#' (ATX)
                if !trimmed.starts_with('#') {
                    continue;
                }
                // Ignore if inside a fenced code block (structure should already handle this, but double-check)
                if structure.is_in_code_block(idx + 1) {
                    continue;
                }
                target_level_headings.push(idx);
            }
        }

        // If we already found a title in front matter, allow the first H1 in the content, flag subsequent ones
        let start_index = 1;

        // If we have any target level headings after accounting for front matter, warn as needed
        if target_level_headings.len() > start_index {
            for &line in &target_level_headings[start_index..] {
                // Skip if out of bounds
                if line >= lines.len() {
                    continue;
                }
                let line_content = lines[line];
                let col = line_content.find('#').unwrap_or(0);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Multiple top-level headings (level {}) in the same document",
                        self.level
                    ),
                    line: line + 1, // Convert back to 1-indexed
                    column: col + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line + 1, col + 1),
                        replacement: format!(
                            "{} {}",
                            "#".repeat(self.level + 1),
                            &line_content[(col + self.level)..]
                        ),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl DocumentStructureExtensions for MD025SingleTitle {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD025SingleTitle::default();

        // Test with only one level-1 heading
        let content = "# Title\n\n## Section 1\n\n## Section 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with multiple level-1 headings
        let content = "# Title 1\n\n## Section 1\n\n# Title 2\n\n## Section 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1); // Should flag the second level-1 heading
        assert_eq!(result[0].line, 5);

        // Test with front matter title and a level-1 heading
        let content = "---\ntitle: Document Title\n---\n\n# Main Heading\n\n## Section 1";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag a single title after front matter"
        );
    }
}
