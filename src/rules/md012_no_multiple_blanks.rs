use crate::utils::range_utils::{LineIndex, calculate_line_range};
use std::collections::HashSet;
use toml;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;

mod md012_config;
use md012_config::MD012Config;

/// Rule MD012: No multiple consecutive blank lines
///
/// See [docs/md012.md](../../docs/md012.md) for full documentation, configuration, and examples.

#[derive(Debug, Clone, Default)]
pub struct MD012NoMultipleBlanks {
    config: MD012Config,
}

impl MD012NoMultipleBlanks {
    pub fn new(maximum: usize) -> Self {
        Self {
            config: MD012Config { maximum },
        }
    }

    pub fn from_config_struct(config: MD012Config) -> Self {
        Self { config }
    }
}

impl Rule for MD012NoMultipleBlanks {
    fn name(&self) -> &'static str {
        "MD012"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive blank lines"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for consecutive newlines or potential whitespace-only lines before processing
        // Look for multiple consecutive lines that could be blank (empty or whitespace-only)
        let lines: Vec<&str> = content.lines().collect();
        let has_potential_blanks = lines
            .windows(2)
            .any(|pair| pair[0].trim().is_empty() && pair[1].trim().is_empty());

        if !has_potential_blanks {
            return Ok(Vec::new());
        }

        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        // Single-pass algorithm with immediate counter reset
        let mut blank_count = 0;
        let mut blank_start = 0;
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut code_fence_marker = "";

        // Use HashSet for O(1) lookups of lines that need to be checked
        let mut lines_to_check: HashSet<usize> = HashSet::new();

        for (line_num, &line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Check for front matter boundaries (only at start of file)
            if trimmed == "---" {
                if line_num == 0 {
                    in_front_matter = true;
                } else if in_front_matter {
                    in_front_matter = false;
                }
                // Reset blank count when entering/exiting front matter
                blank_count = 0;
                continue;
            }

            // Check for code block boundaries
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    // Entering code block
                    in_code_block = true;
                    code_fence_marker = if trimmed.starts_with("```") { "```" } else { "~~~" };
                } else if trimmed.starts_with(code_fence_marker) {
                    // Exiting code block
                    in_code_block = false;
                    code_fence_marker = "";
                }
                // Reset blank count immediately when entering code block
                blank_count = 0;
                continue;
            }

            // Skip lines in code blocks or front matter
            if in_code_block || in_front_matter {
                // Reset counter to prevent counting across boundaries
                blank_count = 0;
                continue;
            }

            // Check for indented code blocks (4+ spaces)
            let is_indented_code = line.len() >= 4 && line.starts_with("    ") && !line.trim().is_empty();
            if is_indented_code {
                blank_count = 0;
                continue;
            }

            if line.trim().is_empty() {
                if blank_count == 0 {
                    blank_start = line_num;
                }
                blank_count += 1;
                // Store line numbers that exceed the limit
                if blank_count > self.config.maximum {
                    lines_to_check.insert(line_num);
                }
            } else {
                if blank_count > self.config.maximum {
                    // Generate warnings for each excess blank line
                    let location = if blank_start == 0 {
                        "at start of file"
                    } else {
                        "between content"
                    };

                    // Report warnings starting from the (maximum+1)th blank line
                    for i in self.config.maximum..blank_count {
                        let excess_line_num = blank_start + i;
                        if lines_to_check.contains(&excess_line_num) {
                            let excess_line = excess_line_num + 1; // +1 for 1-indexed lines
                            let excess_line_content = lines.get(excess_line_num).unwrap_or(&"");

                            // Calculate precise character range for the entire blank line
                            let (start_line, start_col, end_line, end_col) =
                                calculate_line_range(excess_line, excess_line_content);

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                severity: Severity::Warning,
                                message: format!(
                                    "Multiple consecutive blank lines {} (Expected: {}; Actual: {})",
                                    location, self.config.maximum, blank_count
                                ),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                fix: Some(Fix {
                                    range: {
                                        // Remove entire line including newline
                                        let line_start = _line_index.get_line_start_byte(excess_line).unwrap_or(0);
                                        let line_end = _line_index
                                            .get_line_start_byte(excess_line + 1)
                                            .unwrap_or(line_start + 1);
                                        line_start..line_end
                                    },
                                    replacement: String::new(), // Remove the excess line
                                }),
                            });
                        }
                    }
                }
                blank_count = 0;
                lines_to_check.clear();
            }
        }

        // Check for trailing blank lines
        if blank_count > self.config.maximum {
            let location = "at end of file";
            for i in self.config.maximum..blank_count {
                let excess_line_num = blank_start + i;
                if lines_to_check.contains(&excess_line_num) {
                    let excess_line = excess_line_num + 1;
                    let excess_line_content = lines.get(excess_line_num).unwrap_or(&"");

                    // Calculate precise character range for the entire blank line
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(excess_line, excess_line_content);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        severity: Severity::Warning,
                        message: format!(
                            "Multiple consecutive blank lines {} (Expected: {}; Actual: {})",
                            location, self.config.maximum, blank_count
                        ),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        fix: Some(Fix {
                            range: {
                                // Remove entire line including newline
                                let line_start = _line_index.get_line_start_byte(excess_line).unwrap_or(0);
                                let line_end = _line_index
                                    .get_line_start_byte(excess_line + 1)
                                    .unwrap_or(line_start + 1);
                                line_start..line_end
                            },
                            replacement: String::new(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = Vec::new();

        let mut blank_count = 0;

        let lines: Vec<&str> = content.lines().collect();

        let mut in_code_block = false;

        let mut in_front_matter = false;

        let mut code_block_blanks = Vec::new();

        for &line in lines.iter() {
            // Track code blocks and front matter
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                // Handle accumulated blank lines before code block
                if !in_code_block {
                    let allowed_blanks = blank_count.min(self.config.maximum);
                    if allowed_blanks > 0 {
                        result.extend(vec![""; allowed_blanks]);
                    }
                    blank_count = 0;
                } else {
                    // Add accumulated blank lines inside code block
                    result.append(&mut code_block_blanks);
                }
                in_code_block = !in_code_block;
                result.push(line);
                continue;
            }

            if line.trim() == "---" {
                in_front_matter = !in_front_matter;
                if blank_count > 0 {
                    result.extend(vec![""; blank_count]);
                    blank_count = 0;
                }
                result.push(line);
                continue;
            }

            if in_code_block {
                if line.trim().is_empty() {
                    code_block_blanks.push(line);
                } else {
                    result.append(&mut code_block_blanks);
                    result.push(line);
                }
            } else if in_front_matter {
                if blank_count > 0 {
                    result.extend(vec![""; blank_count]);
                    blank_count = 0;
                }
                result.push(line);
            } else if line.trim().is_empty() {
                blank_count += 1;
            } else {
                // Add allowed blank lines before content
                let allowed_blanks = blank_count.min(self.config.maximum);
                if allowed_blanks > 0 {
                    result.extend(vec![""; allowed_blanks]);
                }
                blank_count = 0;
                result.push(line);
            }
        }

        // Handle trailing blank lines
        if !in_code_block {
            let allowed_blanks = blank_count.min(self.config.maximum);
            if allowed_blanks > 0 {
                result.extend(vec![""; allowed_blanks]);
            }
        }

        // Join lines and handle final newline

        let mut output = result.join("\n");
        if content.ends_with('\n') {
            output.push('\n');
        }

        Ok(output)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or doesn't have consecutive newlines
        ctx.content.is_empty() || !ctx.content.contains("\n\n")
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD012Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD012Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD012Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD012NoMultipleBlanks {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // MD012 checks for consecutive blank lines, so it's relevant for any non-empty content
        !ctx.content.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_single_blank_line_allowed() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\nLine 2\n\nLine 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_blank_lines_flagged() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // 1 extra in first gap, 2 extra in second gap
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 6);
        assert_eq!(result[2].line, 7);
    }

    #[test]
    fn test_custom_maximum() {
        let rule = MD012NoMultipleBlanks::new(2);
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Only the fourth blank line is excessive
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_fix_multiple_blank_lines() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\n\nLine 2\n\nLine 3");
    }

    #[test]
    fn test_blank_lines_in_code_block() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n\nmore code\n```\n\nAfter";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Blank lines inside code blocks are ignored
    }

    #[test]
    fn test_fix_preserves_code_block_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n\n```\ncode\n\n\n\nmore code\n```\n\n\nAfter";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Before\n\n```\ncode\n\n\n\nmore code\n```\n\nAfter");
    }

    #[test]
    fn test_blank_lines_in_front_matter() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "---\ntitle: Test\n\n\nauthor: Me\n---\n\nContent";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Blank lines in front matter are ignored
    }

    #[test]
    fn test_blank_lines_at_start() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\nContent";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("at start of file"));
    }

    #[test]
    fn test_blank_lines_at_end() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Content\n\n\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at end of file"));
    }

    #[test]
    fn test_whitespace_only_lines() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n  \n\t\nLine 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Whitespace-only lines count as blank
    }

    #[test]
    fn test_indented_code_blocks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n    code\n    \n    \n    more code\n\nText";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Blank lines in indented code blocks are preserved
    }

    #[test]
    fn test_fix_with_final_newline() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\n\nLine 2\n");
        assert!(fixed.ends_with('\n'));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n~~~\nouter\n\n```\ninner\n\n\n```\n\n~~~\n\nAfter";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n\nno closing fence";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Unclosed code blocks still preserve blank lines
    }

    #[test]
    fn test_mixed_fence_styles() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n~~~\n\nAfter";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Mixed fence styles should work
    }

    #[test]
    fn test_config_from_toml() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("maximum".to_string(), toml::Value::Integer(3));
        config.rules.insert("MD012".to_string(), rule_config);

        let rule = MD012NoMultipleBlanks::from_config(&config);
        let content = "Line 1\n\n\n\nLine 2"; // 3 blank lines
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // 3 blank lines allowed with maximum=3
    }

    #[test]
    fn test_blank_lines_between_sections() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Section 1\n\nContent\n\n\n# Section 2\n\nContent";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_fix_preserves_indented_code() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n\n    code\n    \n    more code\n\n\nText";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // The fix removes the extra blank line, but this is expected behavior
        assert_eq!(fixed, "Text\n\n    code\n\n    more code\n\nText");
    }

    #[test]
    fn test_edge_case_only_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Two excessive blank lines
    }
}
