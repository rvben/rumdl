/// Rule MD025: Document must have a single top-level heading
///
/// See [docs/md025.md](../../docs/md025.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_match_range, LineIndex};
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
    /// Allow multiple H1s if they appear to be document sections (appendices, references, etc.)
    allow_document_sections: bool,
    /// Allow multiple H1s if separated by horizontal rules
    allow_with_separators: bool,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
            allow_document_sections: true, // More lenient by default
            allow_with_separators: true,
        }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, front_matter_title: &str) -> Self {
        Self {
            level,
            front_matter_title: front_matter_title.to_string(),
            allow_document_sections: true,
            allow_with_separators: true,
        }
    }

    pub fn strict() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
            allow_document_sections: false,
            allow_with_separators: false,
        }
    }

    /// Check if a heading text suggests it's a legitimate document section
    fn is_document_section_heading(&self, heading_text: &str) -> bool {
        if !self.allow_document_sections {
            return false;
        }

        let lower_text = heading_text.to_lowercase();
        
        // Common section names that are legitimate as separate H1s
        let section_indicators = [
            "appendix", "appendices",
            "reference", "references", "bibliography",
            "index", "indices",
            "glossary", "glossaries",
            "conclusion", "conclusions",
            "summary", "executive summary",
            "acknowledgment", "acknowledgments", "acknowledgement", "acknowledgements",
            "about", "contact", "license", "legal",
            "changelog", "change log", "history",
            "faq", "frequently asked questions",
            "troubleshooting", "support",
            "installation", "setup", "getting started",
            "api reference", "api documentation",
            "examples", "tutorials", "guides",
        ];

        // Check if the heading starts with these patterns
        section_indicators.iter().any(|&indicator| {
            lower_text.starts_with(indicator) ||
            lower_text.starts_with(&format!("{}:", indicator)) ||
            lower_text.contains(&format!(" {}", indicator)) ||
            // Handle appendix numbering like "Appendix A", "Appendix 1"
            (indicator == "appendix" && (
                lower_text.matches("appendix").count() == 1 && 
                (lower_text.contains(" a") || lower_text.contains(" b") || 
                 lower_text.contains(" 1") || lower_text.contains(" 2") ||
                 lower_text.contains(" i") || lower_text.contains(" ii"))
            ))
        })
    }

    /// Check if headings are separated by horizontal rules
    fn has_separator_before_heading(&self, _structure: &DocumentStructure, _heading_line: usize) -> bool {
        // TODO: Implement when DocumentStructure supports horizontal rules
        // For now, just return false to disable this feature
        false
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
        // unless they are legitimate document sections
        if target_level_headings.len() > 1 {
            // Skip the first heading, check the rest for legitimacy
            for &line in &target_level_headings[1..] {
                // Skip if out of bounds
                if line >= lines.len() {
                    continue;
                }
                let line_content = lines[line];

                // Extract the heading text content
                let heading_text = if line_content.trim_start().starts_with('#') {
                    // ATX heading: extract text after hash markers
                    let trimmed = line_content.trim_start();
                    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                    trimmed[hash_count..].trim().trim_end_matches('#').trim()
                } else {
                    // Setext heading: use the entire line content
                    line_content.trim()
                };

                // Check if this heading should be allowed
                let should_allow = self.is_document_section_heading(heading_text) ||
                    self.has_separator_before_heading(structure, line + 1);

                if should_allow {
                    continue; // Skip flagging this heading
                }

                // Calculate precise character range for the heading text content
                let text_start_in_line = if let Some(pos) = line_content.find(heading_text) {
                    pos
                } else {
                    // Fallback: find after hash markers for ATX headings
                    if line_content.trim_start().starts_with('#') {
                        let trimmed = line_content.trim_start();
                        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                        let after_hashes = &trimmed[hash_count..];
                        let text_start_in_trimmed = after_hashes.find(heading_text).unwrap_or(0);
                        (line_content.len() - trimmed.len()) + hash_count + text_start_in_trimmed
                    } else {
                        0 // Setext headings start at beginning
                    }
                };

                let (start_line, start_col, end_line, end_col) = calculate_match_range(
                    line + 1, // Convert to 1-indexed
                    line_content,
                    text_start_in_line,
                    heading_text.len(),
                );

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
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line + 1, col + 1),
                        replacement: format!(
                            "{} {}",
                            "#".repeat(self.level + 1),
                            if line_content.trim_start().starts_with('#') {
                                // Add bounds checking to prevent panic
                                let slice_start = col + self.level;
                                if slice_start < line_content.len() {
                                    &line_content[slice_start..]
                                } else {
                                    "" // If bounds exceeded, use empty string
                                }
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
        ctx.content.is_empty()
            || (!ctx.content.contains('#')
                && !ctx.content.contains('=')
                && !ctx.content.contains('-'))
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
        map.insert(
            "allow_document_sections".to_string(),
            toml::Value::Boolean(self.allow_document_sections),
        );
        map.insert(
            "allow_with_separators".to_string(),
            toml::Value::Boolean(self.allow_with_separators),
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
        let allow_document_sections =
            crate::config::get_rule_config_value::<bool>(config, "MD025", "allow_document_sections")
                .unwrap_or(true); // Default to true for better UX
        let allow_with_separators =
            crate::config::get_rule_config_value::<bool>(config, "MD025", "allow_with_separators")
                .unwrap_or(true);

        Box::new(MD025SingleTitle {
            level: level as usize,
            front_matter_title,
            allow_document_sections,
            allow_with_separators,
        })
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

        // Test with multiple level-1 headings (non-section names) - should flag
        let content = "# Title 1\n\n## Section 1\n\n# Another Title\n\n## Section 2";
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

    #[test]
    fn test_allow_document_sections() {
        let rule = MD025SingleTitle::default(); // Has allow_document_sections = true

        // Test valid document sections that should NOT be flagged
        let valid_cases = vec![
            "# Main Title\n\n## Content\n\n# Appendix A\n\nAppendix content",
            "# Introduction\n\nContent here\n\n# References\n\nRef content",
            "# Guide\n\nMain content\n\n# Bibliography\n\nBib content",
            "# Manual\n\nContent\n\n# Index\n\nIndex content",
            "# Document\n\nContent\n\n# Conclusion\n\nFinal thoughts",
            "# Tutorial\n\nContent\n\n# FAQ\n\nQuestions and answers",
            "# Project\n\nContent\n\n# Acknowledgments\n\nThanks",
        ];

        for case in valid_cases {
            let structure = DocumentStructure::new(case);
            let result = rule
                .check_with_structure(&crate::lint_context::LintContext::new(case), &structure)
                .unwrap();
            assert!(
                result.is_empty(),
                "Should not flag document sections in: {}",
                case
            );
        }

        // Test invalid cases that should still be flagged
        let invalid_cases = vec![
            "# Main Title\n\n## Content\n\n# Random Other Title\n\nContent",
            "# First\n\nContent\n\n# Second Title\n\nMore content",
        ];

        for case in invalid_cases {
            let structure = DocumentStructure::new(case);
            let result = rule
                .check_with_structure(&crate::lint_context::LintContext::new(case), &structure)
                .unwrap();
            assert!(
                !result.is_empty(),
                "Should flag non-section headings in: {}",
                case
            );
        }
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD025SingleTitle::strict(); // Has allow_document_sections = false

        // Even document sections should be flagged in strict mode
        let content = "# Main Title\n\n## Content\n\n# Appendix A\n\nAppendix content";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&crate::lint_context::LintContext::new(content), &structure)
            .unwrap();
        assert_eq!(result.len(), 1, "Strict mode should flag all multiple H1s");
    }

    #[test]
    fn test_bounds_checking_bug() {
        // Test case that could trigger bounds error in fix generation
        // When col + self.level exceeds line_content.len()
        let rule = MD025SingleTitle::default();

        // Create content with very short second heading
        let content = "# First\n#";
        let ctx = crate::lint_context::LintContext::new(content);

        // This should not panic
        let result = rule.check(&ctx);
        assert!(result.is_ok());

        // Test the fix as well
        let fix_result = rule.fix(&ctx);
        assert!(fix_result.is_ok());
    }

    #[test]
    fn test_bounds_checking_edge_case() {
        // Test case that specifically targets the bounds checking fix
        // Create a heading where col + self.level would exceed line length
        let rule = MD025SingleTitle::default();

        // Create content where the second heading is just "#" (length 1)
        // col will be 0, self.level is 1, so col + self.level = 1
        // This should not exceed bounds for "#" but tests the edge case
        let content = "# First Title\n#";
        let ctx = crate::lint_context::LintContext::new(content);

        // This should not panic and should handle the edge case gracefully
        let result = rule.check(&ctx);
        assert!(result.is_ok());

        if let Ok(warnings) = result {
            if !warnings.is_empty() {
                // Check that the fix doesn't cause a panic
                let fix_result = rule.fix(&ctx);
                assert!(fix_result.is_ok());

                // The fix should produce valid content
                if let Ok(fixed_content) = fix_result {
                    assert!(!fixed_content.is_empty());
                    // Should convert the second "#" to "##" (or "## " if there's content)
                    assert!(fixed_content.contains("##"));
                }
            }
        }
    }
}
