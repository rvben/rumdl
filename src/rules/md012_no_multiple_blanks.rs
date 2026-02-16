use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::lint_context::types::HeadingStyle;
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

/// Check if the given 0-based line index is part of a heading.
///
/// Returns true if:
/// - The line has heading info (covers ATX headings and Setext text lines), OR
/// - The previous line is a Setext heading text line (covers the Setext underline)
fn is_heading_context(ctx: &LintContext, line_idx: usize) -> bool {
    if let Some(line_info) = ctx.lines.get(line_idx) {
        if line_info.heading.is_some() {
            return true;
        }
    }
    // Check if previous line is a Setext heading text line — if so, this line is the underline
    if line_idx > 0 {
        if let Some(prev_info) = ctx.lines.get(line_idx - 1) {
            if let Some(ref heading) = prev_info.heading {
                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                    return true;
                }
            }
        }
    }
    false
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
        let lines = ctx.raw_lines();
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
        // Track the last non-blank content line for heading adjacency checks
        let mut prev_content_line_num: Option<usize> = None;

        // Use HashSet for O(1) lookups of lines that need to be checked
        let mut lines_to_check: HashSet<usize> = HashSet::new();

        // Use filtered_lines to automatically skip front-matter, code blocks, Quarto divs, math blocks,
        // PyMdown blocks, and Obsidian comments.
        // The in_code_block field in LineInfo is pre-computed using pulldown-cmark
        // and correctly handles both fenced code blocks and indented code blocks.
        // Flavor-specific fields (in_quarto_div, in_pymdown_block, in_obsidian_comment) are only
        // set for their respective flavors, so the skip filters have no effect otherwise.
        for filtered_line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_quarto_divs()
            .skip_math_blocks()
            .skip_obsidian_comments()
            .skip_pymdown_blocks()
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
                    let heading_adjacent = prev_content_line_num.is_some_and(|idx| is_heading_context(ctx, idx));
                    if !heading_adjacent {
                        warnings.extend(self.generate_excess_warnings(
                            blank_start,
                            blank_count,
                            lines,
                            &lines_to_check,
                            line_index,
                        ));
                    }
                }
                blank_count = 0;
                lines_to_check.clear();
                // Reset heading context across skipped regions (code blocks, etc.)
                prev_content_line_num = None;
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
                    // Skip warnings if blanks are between content and a heading.
                    // Start-of-file blanks (blank_start == 0) before a heading are still
                    // flagged — no MD022 config requires blanks at the start of a file.
                    let heading_adjacent = prev_content_line_num.is_some_and(|idx| is_heading_context(ctx, idx))
                        || (blank_start > 0 && is_heading_context(ctx, line_num));
                    if !heading_adjacent {
                        warnings.extend(self.generate_excess_warnings(
                            blank_start,
                            blank_count,
                            lines,
                            &lines_to_check,
                            line_index,
                        ));
                    }
                }
                blank_count = 0;
                lines_to_check.clear();
                prev_content_line_num = Some(line_num);
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
        // Track whether the last emitted content line is heading-adjacent
        let mut last_content_is_heading: bool = false;
        // Track whether we've seen any content (for start-of-file detection)
        let mut has_seen_content: bool = false;

        // Process ALL lines (don't skip front-matter in fix mode)
        for filtered_line in ctx.filtered_lines() {
            let line = filtered_line.content;
            let line_idx = filtered_line.line_num - 1; // Convert to 0-based

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
                    last_content_is_heading = false;
                }
                result.push(line);
                continue;
            } else if in_front_matter {
                // Exiting front-matter
                in_front_matter = false;
                last_content_is_heading = false;
            }

            // Track code blocks
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                // Handle accumulated blank lines before code block
                if !in_code_block {
                    let heading_adjacent = last_content_is_heading;
                    if heading_adjacent {
                        // Preserve all blanks adjacent to headings
                        for _ in 0..blank_count {
                            result.push("");
                        }
                    } else {
                        let allowed_blanks = blank_count.min(self.config.maximum.get());
                        if allowed_blanks > 0 {
                            result.extend(vec![""; allowed_blanks]);
                        }
                    }
                    blank_count = 0;
                    last_content_is_heading = false;
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
                // Check if blanks are between content and a heading.
                // Start-of-file blanks before a heading are still reduced.
                let heading_adjacent =
                    last_content_is_heading || (has_seen_content && is_heading_context(ctx, line_idx));
                if heading_adjacent {
                    // Preserve all blanks adjacent to headings
                    for _ in 0..blank_count {
                        result.push("");
                    }
                } else {
                    // Add allowed blank lines before content
                    let allowed_blanks = blank_count.min(self.config.maximum.get());
                    if allowed_blanks > 0 {
                        result.extend(vec![""; allowed_blanks]);
                    }
                }
                blank_count = 0;
                last_content_is_heading = is_heading_context(ctx, line_idx);
                has_seen_content = true;
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
        // Blanks adjacent to headings are heading spacing (MD022's domain)
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Section 1\n\nContent\n\n\n# Section 2\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks adjacent to headings should not be flagged");
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
        // Blanks between code block and heading are heading-adjacent
        let rule = MD012NoMultipleBlanks::default();
        let content = "## Input\n\n```javascript\ncode\n```\n\n\n## Error\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks adjacent to heading should not be flagged");
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
        // Blanks adjacent to headings are heading spacing (MD022's domain)
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Blanks are adjacent to a heading, so no warnings
        assert!(warnings.is_empty(), "Blanks adjacent to heading should not be flagged");
    }

    // Heading awareness tests (issue #429)
    // Heading spacing is MD022's domain, so MD012 skips heading-adjacent blanks

    #[test]
    fn test_heading_aware_atx_blanks_below() {
        // Blanks below an ATX heading should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks below ATX heading should not be flagged");
    }

    #[test]
    fn test_heading_aware_atx_blanks_above() {
        // Blanks above an ATX heading should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Paragraph\n\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks above ATX heading should not be flagged");
    }

    #[test]
    fn test_heading_aware_atx_blanks_between() {
        // Blanks between two ATX headings should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading 1\n\n\n## Heading 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks between headings should not be flagged");
    }

    #[test]
    fn test_heading_aware_setext_equals_blanks_below() {
        // Blanks below a Setext heading (===) should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Heading\n=======\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Blanks below Setext === heading should not be flagged"
        );
    }

    #[test]
    fn test_heading_aware_setext_dashes_blanks_below() {
        // Blanks below a Setext heading (---) should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Heading\n-------\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Blanks below Setext --- heading should not be flagged"
        );
    }

    #[test]
    fn test_heading_aware_setext_blanks_above() {
        // Blanks above a Setext heading should not be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Paragraph\n\n\nHeading\n=======\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blanks above Setext heading should not be flagged");
    }

    #[test]
    fn test_heading_aware_non_heading_blanks_still_flagged() {
        // Blanks between non-heading content should still be flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "Paragraph 1\n\n\nParagraph 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Non-heading blanks should still be flagged");
    }

    #[test]
    fn test_heading_aware_md022_coexistence() {
        // The exact issue scenario: MD022 lines-above=2 with blanks before heading
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Title\n\n\n## Subtitle\n\nContent\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow blanks for MD022 heading spacing");
    }

    #[test]
    fn test_heading_aware_fix_preserves_heading_blanks() {
        // Fix should preserve heading-adjacent blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Heading\n\n\n\nParagraph\n",
            "Fix should preserve heading-adjacent blanks"
        );
    }

    #[test]
    fn test_heading_aware_fix_reduces_non_heading_blanks() {
        // Fix should still reduce non-heading blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "Paragraph 1\n\n\n\nParagraph 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "Paragraph 1\n\nParagraph 2\n",
            "Fix should reduce non-heading blanks"
        );
    }

    #[test]
    fn test_heading_aware_mixed_heading_and_non_heading() {
        // Document with both heading-adjacent and non-heading blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n\nParagraph 1\n\n\nParagraph 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only the blanks between Paragraph 1 and Paragraph 2 should be flagged
        assert_eq!(result.len(), 1, "Should flag only non-heading blanks");
        assert_eq!(result[0].line, 6, "Warning should be on the non-heading blank");
    }

    #[test]
    fn test_heading_aware_blanks_at_start_before_heading_still_flagged() {
        // Start-of-file blanks are always flagged, even before a heading.
        // No MD022 config requires blanks at the absolute start of a file.
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Start-of-file blanks should be flagged even before heading"
        );
        assert!(result[0].message.contains("at start of file"));
    }

    #[test]
    fn test_heading_aware_eof_blanks_after_heading_still_flagged() {
        // EOF blanks should still be flagged even after a heading
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "EOF blanks should still be flagged");
        assert!(result[0].message.contains("at end of file"));
    }

    #[test]
    fn test_heading_aware_custom_maximum_with_headings() {
        // Custom maximum should not affect heading-adjacent skipping
        let rule = MD012NoMultipleBlanks::new(2);
        let content = "# Heading\n\n\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Any number of heading-adjacent blanks should be allowed"
        );
    }

    #[test]
    fn test_heading_aware_blanks_after_code_then_heading() {
        // Blanks after code block followed by heading should not be flagged
        // Tests that prev_content_line_num is reset across code blocks
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n```\ncode\n```\n\n\n\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The blanks are between code block and "More text" (not heading-adjacent)
        assert_eq!(result.len(), 2, "Non-heading blanks after code block should be flagged");
    }

    #[test]
    fn test_heading_aware_fix_mixed_document() {
        // Fix should preserve heading blanks but reduce non-heading blanks
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Title\n\n\n## Section\n\n\nPara 1\n\n\nPara 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Heading-adjacent blanks preserved, non-heading blanks reduced
        assert_eq!(fixed, "# Title\n\n\n## Section\n\n\nPara 1\n\nPara 2\n");
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
