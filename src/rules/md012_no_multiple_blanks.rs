use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::lint_context::types::HeadingStyle;
use crate::utils::LineIndex;
use crate::utils::range_utils::calculate_line_range;
use std::collections::HashSet;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;

mod md012_config;
use md012_config::MD012Config;

/// Rule MD012: No multiple consecutive blank lines
///
/// See [docs/md012.md](../../docs/md012.md) for full documentation, configuration, and examples.

#[derive(Debug, Clone)]
pub struct MD012NoMultipleBlanks {
    config: MD012Config,
    /// Maximum blank lines allowed adjacent to headings (above).
    /// Derived from MD022's lines-above config to avoid conflicts.
    heading_blanks_above: usize,
    /// Maximum blank lines allowed adjacent to headings (below).
    /// Derived from MD022's lines-below config to avoid conflicts.
    heading_blanks_below: usize,
}

impl Default for MD012NoMultipleBlanks {
    fn default() -> Self {
        Self {
            config: MD012Config::default(),
            heading_blanks_above: 1,
            heading_blanks_below: 1,
        }
    }
}

impl MD012NoMultipleBlanks {
    pub fn new(maximum: usize) -> Self {
        use crate::types::PositiveUsize;
        Self {
            config: MD012Config {
                maximum: PositiveUsize::new(maximum).unwrap_or(PositiveUsize::from_const(1)),
            },
            heading_blanks_above: 1,
            heading_blanks_below: 1,
        }
    }

    pub const fn from_config_struct(config: MD012Config) -> Self {
        Self {
            config,
            heading_blanks_above: 1,
            heading_blanks_below: 1,
        }
    }

    /// Set heading blank line limits derived from MD022 config.
    /// `above` and `below` are the maximum blank lines MD022 allows above/below headings.
    pub fn with_heading_limits(mut self, above: usize, below: usize) -> Self {
        self.heading_blanks_above = above;
        self.heading_blanks_below = below;
        self
    }

    /// The effective maximum blank lines allowed for heading-adjacent runs.
    /// Returns the larger of MD012's own maximum and the relevant MD022 limit,
    /// so MD012 never flags blanks that MD022 requires.
    fn effective_max_above(&self) -> usize {
        self.config.maximum.get().max(self.heading_blanks_above)
    }

    fn effective_max_below(&self) -> usize {
        self.config.maximum.get().max(self.heading_blanks_below)
    }

    /// Generate warnings for excess blank lines beyond the given maximum.
    fn generate_excess_warnings(
        &self,
        blank_start: usize,
        blank_count: usize,
        effective_max: usize,
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

        for i in effective_max..blank_count {
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
    if ctx.lines.get(line_idx).is_some_and(|li| li.heading.is_some()) {
        return true;
    }
    // Check if previous line is a Setext heading text line — if so, this line is the underline
    if line_idx > 0
        && let Some(prev_info) = ctx.lines.get(line_idx - 1)
        && let Some(ref heading) = prev_info.heading
        && matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2)
    {
        return true;
    }
    false
}

/// Extract the maximum blank line requirement across all heading levels.
/// Returns `usize::MAX` if any level is Unlimited (-1), since MD012 should
/// never flag blanks that MD022 permits unconditionally.
fn max_heading_limit(
    level_config: &crate::rules::md022_blanks_around_headings::md022_config::HeadingLevelConfig,
) -> usize {
    let mut max_val: usize = 0;
    for level in 1..=6 {
        match level_config.get_for_level(level).required_count() {
            None => return usize::MAX, // Unlimited: MD012 should never flag
            Some(count) => max_val = max_val.max(count),
        }
    }
    max_val
}

impl Rule for MD012NoMultipleBlanks {
    fn name(&self) -> &'static str {
        "MD012"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive blank lines"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
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
                let effective_max = if prev_content_line_num.is_some_and(|idx| is_heading_context(ctx, idx)) {
                    self.effective_max_below()
                } else {
                    self.config.maximum.get()
                };
                if blank_count > effective_max {
                    warnings.extend(self.generate_excess_warnings(
                        blank_start,
                        blank_count,
                        effective_max,
                        lines,
                        &lines_to_check,
                        line_index,
                    ));
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
                // Determine effective maximum for this blank run.
                // Heading-adjacent blanks use the higher of MD012's maximum
                // and MD022's required blank lines, so MD012 doesn't conflict.
                // Start-of-file blanks (blank_start == 0) before a heading use
                // the normal maximum — no rule requires blanks at file start.
                let heading_below = prev_content_line_num.is_some_and(|idx| is_heading_context(ctx, idx));
                let heading_above = blank_start > 0 && is_heading_context(ctx, line_num);
                let effective_max = if heading_below && heading_above {
                    // Between two headings: use the larger of above/below limits
                    self.effective_max_above().max(self.effective_max_below())
                } else if heading_below {
                    self.effective_max_below()
                } else if heading_above {
                    self.effective_max_above()
                } else {
                    self.config.maximum.get()
                };

                if blank_count > effective_max {
                    warnings.extend(self.generate_excess_warnings(
                        blank_start,
                        blank_count,
                        effective_max,
                        lines,
                        &lines_to_check,
                        line_index,
                    ));
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
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
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
        use crate::rules::md022_blanks_around_headings::md022_config::MD022Config;

        let rule_config = crate::rule_config_serde::load_rule_config::<MD012Config>(config);

        // Read MD022 config to determine heading blank line limits.
        // If MD022 is disabled, don't apply special heading limits.
        let md022_disabled = config.global.disable.iter().any(|r| r == "MD022")
            || config.global.extend_disable.iter().any(|r| r == "MD022");

        let (heading_above, heading_below) = if md022_disabled {
            // MD022 disabled: no special heading treatment, use MD012's own maximum
            (rule_config.maximum.get(), rule_config.maximum.get())
        } else {
            let md022_config = crate::rule_config_serde::load_rule_config::<MD022Config>(config);
            (
                max_heading_limit(&md022_config.lines_above),
                max_heading_limit(&md022_config.lines_below),
            )
        };

        Box::new(Self {
            config: rule_config,
            heading_blanks_above: heading_above,
            heading_blanks_below: heading_below,
        })
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
        // With heading limits from MD022, heading-adjacent excess is allowed up to the limit
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 1);
        let content = "# Section 1\n\nContent\n\n\n# Section 2\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "2 blanks above heading allowed with heading_blanks_above=2"
        );
    }

    #[test]
    fn test_fix_preserves_indented_code() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Text\n\n\n    code\n    \n    more code\n\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Fix removes excess blank lines outside code blocks but preserves
        // whitespace-only lines inside indented code blocks unchanged.
        assert_eq!(fixed, "Text\n\n    code\n    \n    more code\n\nText");
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
        // Blanks between code block and heading use heading_above limit
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 1);
        let content = "## Input\n\n```javascript\ncode\n```\n\n\n## Error\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "2 blanks before heading allowed with heading_blanks_above=2"
        );
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
        // With default limits (1,1), heading-adjacent excess blanks are flagged
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Excess heading-adjacent blanks flagged with default limits"
        );
    }

    // Heading awareness tests
    // MD012 reads MD022's config to determine heading blank line limits.
    // When MD022 requires N blank lines around headings, MD012 allows up to N.

    #[test]
    fn test_heading_aware_blanks_below_with_higher_limit() {
        // With heading_blanks_below = 2, 2 blanks below heading are allowed
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 2);
        let content = "# Heading\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "2 blanks below heading allowed with heading_blanks_below=2"
        );
    }

    #[test]
    fn test_heading_aware_blanks_above_with_higher_limit() {
        // With heading_blanks_above = 2, 2 blanks above heading are allowed
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 1);
        let content = "Paragraph\n\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "2 blanks above heading allowed with heading_blanks_above=2"
        );
    }

    #[test]
    fn test_heading_aware_blanks_between_headings() {
        // Between headings, use the larger of above/below limits
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 2);
        let content = "# Heading 1\n\n\n## Heading 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "2 blanks between headings allowed with limits=2");
    }

    #[test]
    fn test_heading_aware_excess_still_flagged() {
        // Even with heading limits, excess beyond the limit is flagged
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 2);
        let content = "# Heading\n\n\n\n\nParagraph\n"; // 4 blanks, limit is 2
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Excess beyond heading limit should be flagged");
    }

    #[test]
    fn test_heading_aware_setext_blanks_below() {
        // Setext headings with heading limits
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 2);
        let content = "Heading\n=======\n\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "2 blanks below Setext heading allowed with limit=2");
    }

    #[test]
    fn test_heading_aware_setext_blanks_above() {
        // Setext headings with heading limits
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 1);
        let content = "Paragraph\n\n\nHeading\n=======\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "2 blanks above Setext heading allowed with limit=2");
    }

    #[test]
    fn test_heading_aware_single_blank_allowed() {
        // 1 blank near heading is always allowed
        let rule = MD012NoMultipleBlanks::default();
        let content = "# Heading\n\nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Single blank near heading should be allowed");
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
    fn test_heading_aware_fix_caps_heading_blanks() {
        // MD012 fix caps heading-adjacent blanks at effective max
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 2);
        let content = "# Heading\n\n\n\nParagraph\n"; // 3 blanks, limit below is 2
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Heading\n\n\nParagraph\n",
            "Fix caps heading-adjacent blanks at effective max (2)"
        );
    }

    #[test]
    fn test_heading_aware_fix_preserves_allowed_heading_blanks() {
        // When blanks are within the heading limit, fix preserves them
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 3);
        let content = "# Heading\n\n\n\nParagraph\n"; // 3 blanks, limit below is 3
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Heading\n\n\n\nParagraph\n",
            "Fix preserves blanks within the heading limit"
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
        // With heading limits, heading-adjacent gaps use higher limit
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 2);
        let content = "# Heading\n\n\nParagraph 1\n\n\nParagraph 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // heading->para gap (2 blanks, limit=2): ok. para->para gap (2 blanks, limit=1): flagged
        assert_eq!(result.len(), 1, "Only non-heading excess should be flagged");
    }

    #[test]
    fn test_heading_aware_blanks_at_start_before_heading_still_flagged() {
        // Start-of-file blanks are always flagged, even before a heading.
        // No rule requires blanks at the absolute start of a file.
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(3, 3);
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
    fn test_heading_aware_unlimited_heading_blanks() {
        // With usize::MAX heading limit (Unlimited in MD022), MD012 never flags heading-adjacent
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(usize::MAX, usize::MAX);
        let content = "# Heading\n\n\n\n\nParagraph\n"; // 4 blanks below heading
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Unlimited heading limits means MD012 never flags near headings"
        );
    }

    #[test]
    fn test_heading_aware_blanks_after_code_then_heading() {
        // Blanks after code block are not heading-adjacent (prev_content_line_num reset)
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 2);
        let content = "# Heading\n\n```\ncode\n```\n\n\n\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The blanks are between code block and "More text" (not heading-adjacent)
        assert_eq!(result.len(), 2, "Non-heading blanks after code block should be flagged");
    }

    #[test]
    fn test_heading_aware_fix_mixed_document() {
        // MD012 fix with heading limits
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 2);
        let content = "# Title\n\n\n## Section\n\n\nPara 1\n\n\nPara 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Heading-adjacent blanks preserved (within limit=2), non-heading blanks reduced
        assert_eq!(fixed, "# Title\n\n\n## Section\n\n\nPara 1\n\nPara 2\n");
    }

    #[test]
    fn test_heading_aware_from_config_reads_md022() {
        // from_config reads MD022 config to determine heading limits
        let mut config = crate::config::Config::default();
        let mut md022_config = crate::config::RuleConfig::default();
        md022_config
            .values
            .insert("lines-above".to_string(), toml::Value::Integer(2));
        md022_config
            .values
            .insert("lines-below".to_string(), toml::Value::Integer(3));
        config.rules.insert("MD022".to_string(), md022_config);

        let rule = MD012NoMultipleBlanks::from_config(&config);
        // With MD022 lines-above=2: 2 blanks above heading should be allowed
        let content = "Paragraph\n\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "2 blanks above heading allowed when MD022 lines-above=2"
        );
    }

    #[test]
    fn test_heading_aware_from_config_md022_disabled() {
        // When MD022 is disabled, MD012 uses its own maximum everywhere
        let mut config = crate::config::Config::default();
        config.global.disable.push("MD022".to_string());

        let mut md022_config = crate::config::RuleConfig::default();
        md022_config
            .values
            .insert("lines-above".to_string(), toml::Value::Integer(3));
        config.rules.insert("MD022".to_string(), md022_config);

        let rule = MD012NoMultipleBlanks::from_config(&config);
        // MD022 disabled: heading-adjacent blanks treated like any other
        let content = "Paragraph\n\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "With MD022 disabled, heading-adjacent blanks are flagged"
        );
    }

    #[test]
    fn test_heading_aware_from_config_md022_unlimited() {
        // When MD022 has lines-above = -1 (Unlimited), MD012 never flags above headings
        let mut config = crate::config::Config::default();
        let mut md022_config = crate::config::RuleConfig::default();
        md022_config
            .values
            .insert("lines-above".to_string(), toml::Value::Integer(-1));
        config.rules.insert("MD022".to_string(), md022_config);

        let rule = MD012NoMultipleBlanks::from_config(&config);
        let content = "Paragraph\n\n\n\n\n# Heading\n"; // 4 blanks above heading
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Unlimited MD022 lines-above means MD012 never flags above headings"
        );
    }

    #[test]
    fn test_heading_aware_from_config_per_level() {
        // Per-level config: max_heading_limit takes the maximum across all levels.
        // lines-above = [2, 1, 1, 1, 1, 1] → heading_blanks_above = 2 (max of all levels).
        // This means 2 blanks above ANY heading is allowed, even if only H1 needs 2.
        // This is a deliberate trade-off: conservative (no false positives from MD012).
        let mut config = crate::config::Config::default();
        let mut md022_config = crate::config::RuleConfig::default();
        md022_config.values.insert(
            "lines-above".to_string(),
            toml::Value::Array(vec![
                toml::Value::Integer(2),
                toml::Value::Integer(1),
                toml::Value::Integer(1),
                toml::Value::Integer(1),
                toml::Value::Integer(1),
                toml::Value::Integer(1),
            ]),
        );
        config.rules.insert("MD022".to_string(), md022_config);

        let rule = MD012NoMultipleBlanks::from_config(&config);

        // 2 blanks above H2: MD012 allows it (max across levels is 2)
        let content = "Paragraph\n\n\n## H2 Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Per-level max (2) allows 2 blanks above any heading");

        // 3 blanks above H2: exceeds the per-level max of 2
        let content = "Paragraph\n\n\n\n## H2 Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "3 blanks exceeds per-level max of 2");
    }

    #[test]
    fn test_issue_449_reproduction() {
        // Exact reproduction case from GitHub issue #449.
        // With default settings, excess blanks around headings should be flagged.
        let rule = MD012NoMultipleBlanks::default();
        let content = "\
# Heading


Some introductory text.





## Heading level 2


Some text for this section.

Some more text for this section.


## Another heading level 2



Some text for this section.

Some more text for this section.
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Issue #449: excess blanks around headings should be flagged with default settings"
        );

        // Verify fix produces clean output
        let fixed = rule.fix(&ctx).unwrap();
        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&fixed_ctx).unwrap();
        assert!(recheck.is_empty(), "Fix should resolve all excess blank lines");

        // Verify the fixed output has exactly 1 blank line around each heading
        assert!(fixed.contains("# Heading\n\nSome"), "1 blank below first heading");
        assert!(
            fixed.contains("text.\n\n## Heading level 2"),
            "1 blank above second heading"
        );
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

    // Roundtrip safety tests: fix then re-check = 0 violations

    #[test]
    fn test_roundtrip_multiple_blank_lines() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(
            recheck.is_empty(),
            "Roundtrip: fix then check should be clean, got {recheck:?}"
        );
    }

    #[test]
    fn test_roundtrip_trailing_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Content\n\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: trailing blanks, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_leading_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\nContent\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: leading blanks, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_custom_maximum() {
        let rule = MD012NoMultipleBlanks::new(2);
        let content = "Line 1\n\n\n\n\nLine 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: max=2, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_code_blocks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Before\n\n\n```\ncode\n\n\n\nmore code\n```\n\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: code blocks, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_heading_limits() {
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(2, 2);
        let content = "# Heading\n\n\n\n\nParagraph\n\n\n\n## Heading 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: heading limits, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_front_matter() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "---\ntitle: Test\n\n\nauthor: Me\n---\n\n\n\nContent\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: front matter, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_only_blanks() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: only blanks, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_single_eof_blank() {
        let rule = MD012NoMultipleBlanks::default();
        let content = "Content\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(recheck.is_empty(), "Roundtrip: single EOF blank, got {recheck:?}");
    }

    #[test]
    fn test_roundtrip_mixed_heading_and_non_heading() {
        let rule = MD012NoMultipleBlanks::default().with_heading_limits(1, 2);
        let content = "# Heading\n\n\n\nParagraph 1\n\n\n\nParagraph 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let recheck = rule.check(&ctx2).unwrap();
        assert!(
            recheck.is_empty(),
            "Roundtrip: mixed heading/non-heading, got {recheck:?}"
        );
    }
}
