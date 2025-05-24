/// Rule MD025: Document must have a single top-level heading
///
/// See [docs/md025.md](../../docs/md025.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    // Pattern for quick check if content has any headings at all
    static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();
}

#[derive(Clone)]
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for headings
        if !content.contains('#') && !content.contains('=') && !content.contains('-') {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let content = _ctx.content;
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Check for front matter title if configured
        let mut _found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() && structure.has_front_matter {
            if let Some((start, end)) = structure.front_matter_range {
                // Extract front matter content
                let front_matter_content: String = _ctx
                    .content
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

        // Find all level-1 headings (both ATX and Setext) not in code blocks
        let lines: Vec<&str> = _ctx.content.lines().collect();
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

                // Ignore if inside a fenced code block
                if structure.is_in_code_block(idx + 1) {
                    continue;
                }

                let line = lines[idx];
                let trimmed = line.trim_start();
                let leading_spaces = line.len() - trimmed.len();

                // Ignore if indented 4+ spaces (code block)
                if leading_spaces >= 4 {
                    continue;
                }

                // Accept both ATX and Setext headings (DocumentStructure already parsed them correctly)
                target_level_headings.push(idx);
            }
        }

        // If we have multiple target level headings, flag all subsequent ones (not the first)
        if target_level_headings.len() > 1 {
            // Skip the first heading, flag the rest
            for &line in &target_level_headings[1..] {
                // Skip if out of bounds
                if line >= lines.len() {
                    continue;
                }
                let line_content = lines[line];
                // For ATX headings, find the '#' position; for Setext, use column 1
                let col = if line_content.trim_start().starts_with('#') {
                    line_content.find('#').unwrap_or(0)
                } else {
                    0 // Setext headings start at column 1
                };
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
                            if line_content.trim_start().starts_with('#') {
                                &line_content[(col + self.level)..]
                            } else {
                                line_content.trim() // For Setext, use the whole line
                            }
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
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.content.contains('#') && !ctx.content.contains('=') && !ctx.content.contains('-'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("level".to_string(), toml::Value::Integer(self.level as i64));
        map.insert(
            "front_matter_title".to_string(),
            toml::Value::String(self.front_matter_title.clone()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let level =
            crate::config::get_rule_config_value::<u32>(config, "MD025", "level").unwrap_or(1);
        let front_matter_title =
            crate::config::get_rule_config_value::<String>(config, "MD025", "front_matter_title")
                .unwrap_or_else(|| "title".to_string());
        Box::new(MD025SingleTitle::new(level as usize, &front_matter_title))
    }
}

impl DocumentStructureExtensions for MD025SingleTitle {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
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
        let result = rule
            .check_with_structure(&crate::lint_context::LintContext::new(content), &structure)
            .unwrap();
        assert!(result.is_empty());

        // Test with multiple level-1 headings
        let content = "# Title 1\n\n## Section 1\n\n# Title 2\n\n## Section 2";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&crate::lint_context::LintContext::new(content), &structure)
            .unwrap();
        assert_eq!(result.len(), 1); // Should flag the second level-1 heading
        assert_eq!(result[0].line, 5);

        // Test with front matter title and a level-1 heading
        let content = "---\ntitle: Document Title\n---\n\n# Main Heading\n\n## Section 1";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&crate::lint_context::LintContext::new(content), &structure)
            .unwrap();
        assert!(
            result.is_empty(),
            "Should not flag a single title after front matter"
        );
    }
}
