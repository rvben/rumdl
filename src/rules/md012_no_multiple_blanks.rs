use crate::filtered_lines::FilteredLinesExt;
use crate::utils::LineIndex;
use crate::utils::range_utils::calculate_line_range;
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
        use crate::types::PositiveUsize;
        Self {
            config: MD012Config {
                maximum: PositiveUsize::new(maximum).unwrap_or(PositiveUsize::from_const(1)),
            },
        }
    }

    pub const fn from_config_struct(config: MD012Config) -> Self {
        Self { config }
    }

    /// Generate warnings for excess blank lines, handling common logic for all contexts
    fn generate_excess_warnings(
        &self,
        blank_start: usize,
        blank_count: usize,
        lines: &[&str],
        lines_to_check: &HashSet<usize>,
        line_index: &LineIndex,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        let location = if blank_start == 0 {
            "at start of file"
        } else {
            "between content"
        };

        for i in self.config.maximum.get()..blank_count {
            let excess_line_num = blank_start + i;
            if lines_to_check.contains(&excess_line_num) {
                let excess_line = excess_line_num + 1;
                let excess_line_content = lines.get(excess_line_num).unwrap_or(&"");
                let (start_line, start_col, end_line, end_col) = calculate_line_range(excess_line, excess_line_content);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    severity: Severity::Warning,
                    message: format!("Multiple consecutive blank lines {location}"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    fix: Some(Fix {
                        range: {
                            let line_start = line_index.get_line_start_byte(excess_line).unwrap_or(0);
                            let line_end = line_index
                                .get_line_start_byte(excess_line + 1)
                                .unwrap_or(line_start + 1);
                            line_start..line_end
                        },
                        replacement: String::new(),
                    }),
                });
            }
        }

        warnings
    }
}

impl Rule for MD012NoMultipleBlanks {
    fn name(&self) -> &'static str {
        "MD012"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive blank lines"
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

        // Also check for blanks at EOF (markdownlint behavior)
        // Content is normalized to LF at I/O boundary
        let ends_with_multiple_newlines = content.ends_with("\n\n");

        if !has_potential_blanks && !ends_with_multiple_newlines {
            return Ok(Vec::new());
        }

        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        // Single-pass algorithm with immediate counter reset
        let mut blank_count = 0;
        let mut blank_start = 0;
        let mut last_line_num: Option<usize> = None;

        // Use HashSet for O(1) lookups of lines that need to be checked
        let mut lines_to_check: HashSet<usize> = HashSet::new();

        // Use filtered_lines to automatically skip front-matter, code blocks, Quarto divs, and math blocks
        // The in_code_block field in LineInfo is pre-computed using pulldown-cmark
        // and correctly handles both fenced code blocks and indented code blocks
        // The in_quarto_div field is only set for Quarto flavor, so skip_quarto_divs() has no effect otherwise
        // The in_math_block field tracks $$ delimited math blocks (Quarto/LaTeX)
        for filtered_line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_quarto_divs()
            .skip_math_blocks()
        {
            let line_num = filtered_line.line_num - 1; // Convert 1-based to 0-based for internal tracking
            let line = filtered_line.content;

            // Detect when lines were skipped (e.g., code block content)
            // If we jump more than 1 line, there was content between, which breaks blank sequences
            if let Some(last) = last_line_num
                && line_num > last + 1
            {
                // Lines were skipped (code block or similar)
                // Generate warnings for any accumulated blanks before the skip
                if blank_count > self.config.maximum.get() {
                    warnings.extend(self.generate_excess_warnings(
                        blank_start,
                        blank_count,
                        &lines,
                        &lines_to_check,
                        line_index,
                    ));
                }
                blank_count = 0;
                lines_to_check.clear();
            }
            last_line_num = Some(line_num);

            if line.trim().is_empty() {
                if blank_count == 0 {
                    blank_start = line_num;
                }
                blank_count += 1;
                // Store line numbers that exceed the limit
                if blank_count > self.config.maximum.get() {
                    lines_to_check.insert(line_num);
                }
            } else {
                if blank_count > self.config.maximum.get() {
                    warnings.extend(self.generate_excess_warnings(
                        blank_start,
                        blank_count,
                        &lines,
                        &lines_to_check,
                        line_index,
                    ));
                }
                blank_count = 0;
                lines_to_check.clear();
            }
        }

        // Handle trailing blanks at EOF
        // Main loop only reports mid-document blanks (between content)
        // EOF handler reports trailing blanks with stricter rules (any blank at EOF is flagged)
        //
        // The blank_count at end of loop might include blanks BEFORE a code block at EOF,
        // which aren't truly "trailing blanks". We need to verify the actual last line is blank.
        let last_line_is_blank = lines.last().is_some_and(|l| l.trim().is_empty());

        // Check for trailing blank lines
        // EOF semantics: ANY blank line at EOF should be flagged (stricter than mid-document)
        // Only fire if the actual last line(s) of the file are blank
        if blank_count > 0 && last_line_is_blank {
            let location = "at end of file";

            // Report on the last line (which is blank)
            let report_line = lines.len();

            // Calculate fix: remove all trailing blank lines
            // Find where the trailing blanks start (blank_count tells us how many consecutive blanks)
            let fix_start = line_index
                .get_line_start_byte(report_line - blank_count + 1)
                .unwrap_or(0);
            let fix_end = content.len();

            // Report one warning for the excess blank lines at EOF
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                severity: Severity::Warning,
                message: format!("Multiple consecutive blank lines {location}"),
                line: report_line,
                column: 1,
                end_line: report_line,
                end_column: 1,
                fix: Some(Fix {
                    range: fix_start..fix_end,
                    // The fix_start already points to the first blank line, which is AFTER
                    // the last content line's newline. So we just remove everything from
                    // fix_start to end, and the last content line's newline is preserved.
                    replacement: String::new(),
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let mut result = Vec::new();
        let mut blank_count = 0;

        let mut in_code_block = false;
        let mut code_block_blanks = Vec::new();
        let mut in_front_matter = false;

        // Process ALL lines (don't skip front-matter in fix mode)
        for filtered_line in ctx.filtered_lines() {
            let line = filtered_line.content;

            // Pass through front-matter lines unchanged
            if filtered_line.line_info.in_front_matter {
                if !in_front_matter {
                    // Entering front-matter: flush any accumulated blanks
                    let allowed_blanks = blank_count.min(self.config.maximum.get());
                    if allowed_blanks > 0 {
                        result.extend(vec![""; allowed_blanks]);
                    }
                    blank_count = 0;
                    in_front_matter = true;
                }
                result.push(line);
                continue;
            } else if in_front_matter {
                // Exiting front-matter
                in_front_matter = false;
            }

            // Track code blocks
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                // Handle accumulated blank lines before code block
                if !in_code_block {
                    let allowed_blanks = blank_count.min(self.config.maximum.get());
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

            if in_code_block {
                if line.trim().is_empty() {
                    code_block_blanks.push(line);
                } else {
                    result.append(&mut code_block_blanks);
                    result.push(line);
                }
            } else if line.trim().is_empty() {
                blank_count += 1;
            } else {
                // Add allowed blank lines before content
                let allowed_blanks = blank_count.min(self.config.maximum.get());
                if allowed_blanks > 0 {
                    result.extend(vec![""; allowed_blanks]);
                }
                blank_count = 0;
                result.push(line);
            }
        }

        // Trailing blank lines at EOF are removed entirely (matching markdownlint-cli)

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
        // Skip if content is empty or doesn't have newlines (single line can't have multiple blanks)
        ctx.content.is_empty() || !ctx.has_char('\n')
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_single_blank_line_allowed() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\nLine 2\n\nLine 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_blank_lines_flagged() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Only the fourth blank line is excessive
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_fix_multiple_blank_lines() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\n\nLine 2\n\nLine 3");
    }

    #[test]
    fn test_blank_lines_in_code_block() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n\nmore code\n```\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Blank lines inside code blocks are ignored
    }

    #[test]
    fn test_fix_preserves_code_block_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n\n```\ncode\n\n\n\nmore code\n```\n\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Before\n\n```\ncode\n\n\n\nmore code\n```\n\nAfter");
    }

    #[test]
    fn test_blank_lines_in_front_matter() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "---\ntitle: Test\n\n\nauthor: Me\n---\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Blank lines in front matter are ignored
    }

    #[test]
    fn test_blank_lines_at_start() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("at start of file"));
    }

    #[test]
    fn test_blank_lines_at_end() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Content\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at end of file"));
    }

    #[test]
    fn test_single_blank_at_eof_flagged() {
        // Markdownlint behavior: ANY blank lines at EOF are flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Content\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at end of file"));
    }

    #[test]
    fn test_whitespace_only_lines() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n  \n\t\nLine 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Whitespace-only lines count as blank
    }

    #[test]
    fn test_indented_code_blocks() {
        // Per markdownlint-cli reference: blank lines inside indented code blocks are valid
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n    code\n    \n    \n    more code\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blanks inside indented code blocks");
    }

    #[test]
    fn test_blanks_in_indented_code_block() {
        // Per markdownlint-cli reference: blank lines inside indented code blocks are valid
        let content = "    code line 1\n\n\n    code line 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD012NoMultipleBlanks::default();
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "Should not flag blanks in indented code");
    }

    #[test]
    fn test_blanks_in_indented_code_block_with_heading() {
        // Per markdownlint-cli reference: blank lines inside indented code blocks are valid
        let content = "# Heading\n\n    code line 1\n\n\n    code line 2\n\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD012NoMultipleBlanks::default();
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Should not flag blanks in indented code after heading"
        );
    }

    #[test]
    fn test_blanks_after_indented_code_block_flagged() {
        // Blanks AFTER an indented code block end should still be flagged
        let content = "# Heading\n\n    code line\n\n\n\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD012NoMultipleBlanks::default();
        let warnings = rule.check(&ctx).unwrap();
        // There are 3 blank lines after the code block, so 2 extra should be flagged
        assert_eq!(warnings.len(), 2, "Should flag blanks after indented code block ends");
    }

    #[test]
    fn test_fix_with_final_newline() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\n\nLine 2\n");
        assert!(fixed.ends_with('\n'));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n~~~\nouter\n\n```\ninner\n\n\n```\n\n~~~\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n\nno closing fence";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // Unclosed code blocks still preserve blank lines
    }

    #[test]
    fn test_mixed_fence_styles() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n```\ncode\n\n\n~~~\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty()); // 3 blank lines allowed with maximum=3
    }

    #[test]
    fn test_blank_lines_between_sections() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Section 1\n\nContent\n\n\n# Section 2\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_fix_preserves_indented_code() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n\n    code\n    \n    more code\n\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // The fix removes the extra blank line, but this is expected behavior
        assert_eq!(fixed, "Text\n\n    code\n\n    more code\n\nText");
    }

    #[test]
    fn test_edge_case_only_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // With the new EOF handling, we report once at EOF
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at end of file"));
    }

    // Regression tests for blanks after code blocks (GitHub issue #199 related)

    #[test]
    fn test_blanks_after_fenced_code_block_mid_document() {
        // This is the pattern from React repo test files that was being missed
        let rule = MD012NoMultipleBlanks::default();
        let content = "## Input\n\n```javascript\ncode\n```\n\n\n## Error\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the double blank between code block and next heading
        assert_eq!(result.len(), 1, "Should detect blanks after code block");
        assert_eq!(result[0].line, 7, "Warning should be on line 7 (second blank)");
        assert!(result[0].message.contains("between content"));
    }

    #[test]
    fn test_blanks_after_code_block_at_eof() {
        // Trailing blanks after code block at end of file
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n```\ncode\n```\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the trailing blanks at EOF
        assert_eq!(result.len(), 1, "Should detect trailing blanks after code block");
        assert!(result[0].message.contains("at end of file"));
    }

    #[test]
    fn test_single_blank_after_code_block_allowed() {
        // Single blank after code block is allowed (default max=1)
        let rule = MD012NoMultipleBlanks::default();
        let content = "## Input\n\n```\ncode\n```\n\n## Output\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Single blank after code block should be allowed");
    }

    #[test]
    fn test_multiple_code_blocks_with_blanks() {
        // Multiple code blocks, each followed by blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "```\ncode1\n```\n\n\n```\ncode2\n```\n\n\nEnd\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag both double-blank sequences
        assert_eq!(result.len(), 2, "Should detect blanks after both code blocks");
    }

    #[test]
    fn test_whitespace_only_lines_after_code_block_at_eof() {
        // Whitespace-only lines (not just empty) after code block at EOF
        // This matches the React repo pattern where lines have trailing spaces
        let rule = MD012NoMultipleBlanks::default();
        let content = "```\ncode\n```\n   \n   \n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should detect whitespace-only trailing blanks");
        assert!(result[0].message.contains("at end of file"));
    }

    // Tests for warning-based fix (used by LSP formatting)

    #[test]
    fn test_warning_fix_removes_single_trailing_blank() {
        // Regression test for issue #265: LSP formatting should work for EOF blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "hello foobar hello.\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_some(), "Warning should have a fix attached");

        let fix = warnings[0].fix.as_ref().unwrap();
        // The fix should remove the trailing blank line
        assert_eq!(fix.replacement, "", "Replacement should be empty");

        // Apply the fix and verify result
        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fixed, "hello foobar hello.\n", "Should end with single newline");
    }

    #[test]
    fn test_warning_fix_removes_multiple_trailing_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "content\n\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_some());

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fixed, "content\n", "Should end with single newline");
    }

    #[test]
    fn test_warning_fix_preserves_content_newline() {
        // Ensure the fix doesn't remove the content line's trailing newline
        let rule = MD012NoMultipleBlanks::default();
        let content = "line1\nline2\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fixed, "line1\nline2\n", "Should preserve all content lines");
    }

    #[test]
    fn test_warning_fix_mid_document_blanks() {
        // Test that mid-document blank line fixes also work via warnings
        let rule = MD012NoMultipleBlanks::default();
        // Content with 2 extra blank lines (3 blank lines total, should reduce to 1)
        let content = "# Heading\n\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // With maximum=1 (default), 3 consecutive blanks produces 2 warnings
        assert_eq!(warnings.len(), 2, "Should have 2 warnings for 2 extra blank lines");
        assert!(warnings[0].fix.is_some());
        assert!(warnings[1].fix.is_some());

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fixed, "# Heading\n\nParagraph\n", "Should reduce to single blank");
    }

    // Quarto flavor tests

    #[test]
    fn test_blank_lines_in_quarto_callout() {
        // Blank lines inside Quarto callout blocks should be allowed
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n::: {.callout-note}\nNote content\n\n\nMore content\n:::\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blanks inside Quarto callouts");
    }

    #[test]
    fn test_blank_lines_in_quarto_div() {
        // Blank lines inside generic Quarto divs should be allowed
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n::: {.bordered}\nContent\n\n\nMore\n:::\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blanks inside Quarto divs");
    }

    #[test]
    fn test_blank_lines_outside_quarto_div_flagged() {
        // Blank lines outside Quarto divs should still be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n\n::: {.callout-note}\nNote\n:::\n\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should flag blanks outside Quarto divs");
    }

    #[test]
    fn test_quarto_divs_ignored_in_standard_flavor() {
        // In standard flavor, Quarto div syntax is not special
        let rule = MD012NoMultipleBlanks::default();
        let content = "::: {.callout-note}\nNote content\n\n\nMore content\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // In standard flavor, the triple blank inside "div" is flagged
        assert!(!result.is_empty(), "Standard flavor should flag blanks in 'div'");
    }
}
