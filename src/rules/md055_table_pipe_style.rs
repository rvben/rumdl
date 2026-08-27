use crate::config::MarkdownFlavor;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{FlavorOverrideNotice, option_is_explicit};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::table_utils::{TableBlock, TableUtils};

mod md055_config;
use md055_config::MD055Config;

/// Reports the MDG style override once per process; see [`FlavorOverrideNotice`].
static MDG_STYLE_OVERRIDE: FlavorOverrideNotice = FlavorOverrideNotice::new();

/// Rule MD055: Table pipe style
///
/// See [docs/md055.md](../../docs/md055.md) for full documentation, configuration, and examples.
///
/// This rule enforces consistent use of leading and trailing pipe characters in Markdown tables,
/// which improves readability and ensures uniform document styling.
///
/// ## Purpose
///
/// - **Consistency**: Ensures uniform table formatting throughout documents
/// - **Readability**: Well-formatted tables are easier to read and understand
/// - **Maintainability**: Consistent table syntax makes documents easier to maintain
/// - **Compatibility**: Some Markdown processors handle different table styles differently
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```yaml
/// MD055:
///   style: "consistent"  # Can be "consistent", "leading_and_trailing", or "no_leading_or_trailing"
/// ```
///
/// ### Style Options
///
/// - **consistent**: All tables must use the same style (default)
/// - **leading_and_trailing**: All tables must have both leading and trailing pipes
/// - **no_leading_or_trailing**: Tables must not have leading or trailing pipes
///
/// ## Examples
///
/// ### Leading and Trailing Pipes
///
/// ```markdown
/// | Header 1 | Header 2 | Header 3 |
/// |----------|----------|----------|
/// | Cell 1   | Cell 2   | Cell 3   |
/// | Cell 4   | Cell 5   | Cell 6   |
/// ```
///
/// ### No Leading or Trailing Pipes
///
/// ```markdown
/// Header 1 | Header 2 | Header 3
/// ---------|----------|---------
/// Cell 1   | Cell 2   | Cell 3
/// Cell 4   | Cell 5   | Cell 6
/// ```
///
/// ## Behavior Details
///
/// - The rule analyzes each table in the document to determine its pipe style
/// - With "consistent" style, the first table's style is used as the standard for all others
/// - The rule handles both the header row, separator row, and content rows
/// - Tables inside code blocks are ignored
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds or removes leading and trailing pipes as needed
/// - Preserves the content and alignment of table cells
/// - Maintains proper spacing around pipe characters
/// - Updates both header and content rows to match the required style
///
/// ## Performance Considerations
///
/// The rule includes performance optimizations:
/// - Efficient table detection with quick checks before detailed analysis
/// - Smart line-by-line processing to avoid redundant operations
/// - Optimized string manipulation for pipe character handling
///
/// Enforces consistent use of leading and trailing pipe characters in tables
#[derive(Debug, Default, Clone)]
pub struct MD055TablePipeStyle {
    config: MD055Config,
    /// Whether `style` came from the configuration rather than from the
    /// default. MDG enforces leading-and-trailing either way; this only decides
    /// whether the user is told that the style they asked for was not adopted.
    style_explicit: bool,
}

impl MD055TablePipeStyle {
    pub fn new(style: String) -> Self {
        Self {
            config: MD055Config { style },
            style_explicit: true,
        }
    }

    pub fn from_config_struct(config: MD055Config) -> Self {
        Self {
            config,
            style_explicit: false,
        }
    }

    /// Resolve the style MD055 should converge on.
    ///
    /// Gherkin recognizes a Data Table or Examples table row only when an indent
    /// is followed directly by a pipe, so a style that strips the leading pipe
    /// does not restyle the table, it deletes it from the document. MDG
    /// therefore always converges on leading-and-trailing: `consistent` resolves
    /// to it rather than to whichever form happens to be more prevalent, and an
    /// explicit style that drops the leading pipe is not adopted.
    fn effective_configured_style(&self, ctx: &crate::lint_context::LintContext) -> &str {
        if ctx.flavor == MarkdownFlavor::MDG {
            self.warn_once_about_overridden_style();
            return "leading_and_trailing";
        }

        match self.config.style.as_str() {
            "leading_and_trailing" | "no_leading_or_trailing" | "leading_only" | "trailing_only" | "consistent" => {
                self.config.style.as_str()
            }
            _ => {
                // Invalid style provided, default to "leading_and_trailing"
                "leading_and_trailing"
            }
        }
    }

    /// Tell the user once that MDG did not adopt the style they configured.
    ///
    /// Only the three styles that drop a pipe are worth reporting: they are the
    /// settings MDG cannot satisfy. `consistent` asks for no particular form,
    /// and leading-and-trailing is what MDG picks for it anyway.
    fn warn_once_about_overridden_style(&self) {
        if !self.style_explicit
            || !matches!(
                self.config.style.as_str(),
                "no_leading_or_trailing" | "leading_only" | "trailing_only"
            )
        {
            return;
        }

        MDG_STYLE_OVERRIDE.report(
            "MD055",
            "style",
            &self.config.style,
            "leading_and_trailing",
            "a Gherkin table row is an indent followed directly by a pipe",
        );
    }

    /// Determine the most prevalent table style in a table block
    fn determine_table_style(&self, table_block: &TableBlock, lines: &[&str]) -> Option<&'static str> {
        let mut leading_and_trailing_count = 0;
        let mut no_leading_or_trailing_count = 0;
        let mut leading_only_count = 0;
        let mut trailing_only_count = 0;

        // Count style of header row (table line index 0)
        let header_content = TableUtils::extract_table_row_content(lines[table_block.header_line], table_block, 0);
        if let Some(style) = TableUtils::determine_pipe_style(header_content) {
            match style {
                "leading_and_trailing" => leading_and_trailing_count += 1,
                "no_leading_or_trailing" => no_leading_or_trailing_count += 1,
                "leading_only" => leading_only_count += 1,
                "trailing_only" => trailing_only_count += 1,
                _ => {}
            }
        }

        // Count style of content rows (table line indices 2, 3, 4, ...)
        for (i, &line_idx) in table_block.content_lines.iter().enumerate() {
            let content = TableUtils::extract_table_row_content(lines[line_idx], table_block, 2 + i);
            if let Some(style) = TableUtils::determine_pipe_style(content) {
                match style {
                    "leading_and_trailing" => leading_and_trailing_count += 1,
                    "no_leading_or_trailing" => no_leading_or_trailing_count += 1,
                    "leading_only" => leading_only_count += 1,
                    "trailing_only" => trailing_only_count += 1,
                    _ => {}
                }
            }
        }

        // Determine most prevalent style
        // In case of tie, prefer leading_and_trailing (most common, widely supported)
        let max_count = leading_and_trailing_count
            .max(no_leading_or_trailing_count)
            .max(leading_only_count)
            .max(trailing_only_count);

        if max_count > 0 {
            if leading_and_trailing_count == max_count {
                Some("leading_and_trailing")
            } else if no_leading_or_trailing_count == max_count {
                Some("no_leading_or_trailing")
            } else if leading_only_count == max_count {
                Some("leading_only")
            } else if trailing_only_count == max_count {
                Some("trailing_only")
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Simple table row fix for tests - creates a dummy TableBlock without list context
    #[cfg(test)]
    fn fix_table_row(&self, line: &str, target_style: &str) -> String {
        let dummy_block = TableBlock {
            start_line: 0,
            end_line: 0,
            header_line: 0,
            delimiter_line: 0,
            content_lines: vec![],
            list_context: None,
        };
        self.fix_table_row_with_context(line, target_style, &dummy_block, 0, MarkdownFlavor::Standard)
    }

    /// Fix a table row to match the target style, with full context for list tables
    ///
    /// This handles tables inside list items by stripping the list prefix,
    /// fixing the table content, then restoring the appropriate prefix.
    fn fix_table_row_with_context(
        &self,
        line: &str,
        target_style: &str,
        table_block: &TableBlock,
        table_line_index: usize,
        flavor: MarkdownFlavor,
    ) -> String {
        // Extract blockquote prefix first
        let (bq_prefix, after_bq) = TableUtils::extract_blockquote_prefix(line);

        // Handle list context if present
        if let Some(ref list_ctx) = table_block.list_context {
            if table_line_index == 0 {
                // Header line: strip list prefix (handles both markers and indentation)
                let stripped = after_bq
                    .strip_prefix(&list_ctx.list_prefix)
                    .unwrap_or_else(|| TableUtils::extract_list_prefix(after_bq).1);
                let fixed_content = self.fix_table_content(stripped.trim(), target_style);

                // Restore prefixes: blockquote + list prefix + fixed content
                let lp = &list_ctx.list_prefix;
                if bq_prefix.is_empty() && lp.is_empty() {
                    fixed_content
                } else {
                    format!("{bq_prefix}{lp}{fixed_content}")
                }
            } else {
                // Continuation lines: strip indentation, then restore it
                let content_indent = list_ctx.content_indent;
                let stripped = TableUtils::extract_table_row_content(line, table_block, table_line_index);
                let fixed_content = self.fix_table_content(stripped.trim(), target_style);

                // Restore prefixes: blockquote + indentation + fixed content
                let indent = " ".repeat(content_indent);
                format!("{bq_prefix}{indent}{fixed_content}")
            }
        } else {
            // No list context, just handle blockquote prefix
            let fixed_content = self.fix_table_content(after_bq.trim(), target_style);
            // Gherkin recognizes a table row by its indent, so left-aligning one
            // while restyling it would take the table out of the document that
            // the restyle exists to preserve. MD060 still owns the indent's width.
            let indent = if flavor == MarkdownFlavor::MDG {
                &after_bq[..after_bq.len() - after_bq.trim_start().len()]
            } else {
                ""
            };
            if bq_prefix.is_empty() && indent.is_empty() {
                fixed_content
            } else {
                format!("{bq_prefix}{indent}{fixed_content}")
            }
        }
    }

    /// Fix the table content (without any prefix handling)
    fn fix_table_content(&self, trimmed: &str, target_style: &str) -> String {
        if !trimmed.contains('|') {
            return trimmed.to_string();
        }

        let has_leading = trimmed.starts_with('|');
        let has_trailing = trimmed.ends_with('|');

        match target_style {
            "leading_and_trailing" => {
                let mut result = trimmed.to_string();

                // Add leading pipe if missing
                if !has_leading {
                    result = format!("| {result}");
                }

                // Add trailing pipe if missing
                if !has_trailing {
                    result = format!("{result} |");
                }

                result
            }
            "no_leading_or_trailing" => {
                let mut result = trimmed;

                // Remove leading pipe if present
                if has_leading {
                    result = result.strip_prefix('|').unwrap_or(result);
                    result = result.trim_start();
                }

                // Remove trailing pipe if present
                if has_trailing {
                    result = result.strip_suffix('|').unwrap_or(result);
                    result = result.trim_end();
                }

                result.to_string()
            }
            "leading_only" => {
                let mut result = trimmed.to_string();

                // Add leading pipe if missing
                if !has_leading {
                    result = format!("| {result}");
                }

                // Remove trailing pipe if present
                if has_trailing {
                    result = result.strip_suffix('|').unwrap_or(&result).trim_end().to_string();
                }

                result
            }
            "trailing_only" => {
                let mut result = trimmed;

                // Remove leading pipe if present
                if has_leading {
                    result = result.strip_prefix('|').unwrap_or(result).trim_start();
                }

                let mut result = result.to_string();

                // Add trailing pipe if missing
                if !has_trailing {
                    result = format!("{result} |");
                }

                result
            }
            _ => trimmed.to_string(),
        }
    }
}

impl Rule for MD055TablePipeStyle {
    fn name(&self) -> &'static str {
        "MD055"
    }

    fn description(&self) -> &'static str {
        "Table pipe style should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Table
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no tables present (uses cached pipe count)
        !ctx.likely_has_tables()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Early return handled by should_skip()

        let lines = ctx.raw_lines();

        let configured_style = self.effective_configured_style(ctx);

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        // Process each table block
        for table_block in table_blocks {
            // First pass: determine the table's style for "consistent" mode
            // Count all rows to determine most prevalent style (prevalence-based approach)
            let table_style = if configured_style == "consistent" {
                self.determine_table_style(table_block, lines)
            } else {
                None
            };

            // Determine target style for this table
            let target_style = if configured_style == "consistent" {
                table_style.unwrap_or("leading_and_trailing")
            } else {
                configured_style
            };

            // Collect all table lines for processing
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            // Check each row and emit a per-row fix. Per-row fixes ensure that
            // inline-disabling one row does not cause the fix on another row to
            // overwrite the disabled row's content.
            for (table_line_idx, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                // Extract content to properly check pipe style (handles list/blockquote prefixes)
                let content = TableUtils::extract_table_row_content(line, table_block, table_line_idx);
                if let Some(current_style) = TableUtils::determine_pipe_style(content) {
                    // Only flag lines with actual style mismatches
                    let needs_fixing = current_style != target_style;

                    if needs_fixing {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, line);

                        let message = format!(
                            "Table pipe style should be {}",
                            match target_style {
                                "leading_and_trailing" => "leading and trailing",
                                "no_leading_or_trailing" => "no leading or trailing",
                                "leading_only" => "leading only",
                                "trailing_only" => "trailing only",
                                _ => target_style,
                            }
                        );

                        // Build a per-row fix so inline-disabled rows are not
                        // overwritten by fixes on other rows in the same table.
                        let fixed_line = self.fix_table_row_with_context(
                            line,
                            target_style,
                            table_block,
                            table_line_idx,
                            ctx.flavor,
                        );
                        let row_range = ctx.line_column_byte_range_with_length(line_idx + 1, 1, line.chars().count());

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            severity: Severity::Warning,
                            message,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            fix: Some(crate::rule::Fix::new(row_range, fixed_line)),
                        });
                    }
                }
            }
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_sections!(MD055Config);

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD055Config>(config);
        let style_explicit = option_is_explicit(config, "MD055", "style");

        Box::new(Self {
            config: rule_config,
            style_explicit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Issue #611: kebab-case config values ignored, fallback to leading-and-trailing ===
    //
    // All style names must work identically whether the user writes kebab-case
    // (no-leading-or-trailing) or snake_case (no_leading_or_trailing) in config.

    fn rule_from_toml_style(style: &str) -> MD055TablePipeStyle {
        let config: md055_config::MD055Config =
            toml::from_str(&format!("style = \"{style}\"")).expect("valid style value");
        MD055TablePipeStyle::from_config_struct(config)
    }

    #[test]
    fn test_no_leading_or_trailing_kebab_accepts_conforming_table() {
        let rule = rule_from_toml_style("no-leading-or-trailing");
        let content = "A | B\n--- | ---\n1 | 2";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "no-leading-or-trailing should accept a table with no pipes: {warnings:?}"
        );
    }

    #[test]
    fn test_no_leading_or_trailing_kebab_rejects_nonconforming_table() {
        let rule = rule_from_toml_style("no-leading-or-trailing");
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            3,
            "no-leading-or-trailing should flag all 3 rows with pipes"
        );
        assert!(warnings.iter().all(|w| w.message.contains("no leading or trailing")));
    }

    #[test]
    fn test_leading_only_kebab_accepts_conforming_table() {
        let rule = rule_from_toml_style("leading-only");
        let content = "| A | B\n|---|---\n| 1 | 2";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "leading-only should accept a leading-only table: {warnings:?}"
        );
    }

    #[test]
    fn test_trailing_only_kebab_accepts_conforming_table() {
        let rule = rule_from_toml_style("trailing-only");
        let content = "A | B |\n---|---|\n1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "trailing-only should accept a trailing-only table: {warnings:?}"
        );
    }

    #[test]
    fn test_trailing_only_kebab_rejects_nonconforming_table() {
        let rule = rule_from_toml_style("trailing-only");
        // leading-and-trailing table must be flagged — proves the table is actually detected
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            3,
            "trailing-only should flag all 3 rows that have leading pipes"
        );
        assert!(warnings.iter().all(|w| w.message.contains("trailing only")));
    }

    #[test]
    fn test_leading_only_kebab_rejects_nonconforming_table() {
        let rule = rule_from_toml_style("leading-only");
        // trailing-only table must be flagged — proves the table is actually detected
        let content = "A | B |\n---|---|\n1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            3,
            "leading-only should flag all 3 rows that have trailing pipes"
        );
        assert!(warnings.iter().all(|w| w.message.contains("leading only")));
    }

    #[test]
    fn test_leading_and_trailing_kebab_accepts_conforming_table() {
        let rule = rule_from_toml_style("leading-and-trailing");
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "leading-and-trailing should accept a fully-piped table: {warnings:?}"
        );
    }

    #[test]
    fn test_kebab_and_snake_case_styles_are_equivalent() {
        // For every style, kebab and snake_case forms must produce identical warnings —
        // same count, same messages, same line numbers.
        let pairs = [
            ("no-leading-or-trailing", "no_leading_or_trailing"),
            ("leading-only", "leading_only"),
            ("trailing-only", "trailing_only"),
            ("leading-and-trailing", "leading_and_trailing"),
        ];
        // Mixed table so every style produces at least one warning, exercising the message path.
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        for (kebab, snake) in pairs {
            let kebab_rule = rule_from_toml_style(kebab);
            let snake_rule = rule_from_toml_style(snake);
            let kebab_warnings = kebab_rule.check(&ctx).unwrap();
            let snake_warnings = snake_rule.check(&ctx).unwrap();

            assert_eq!(
                kebab_warnings.len(),
                snake_warnings.len(),
                "'{kebab}' and '{snake}' must produce the same number of warnings"
            );
            for (i, (kw, sw)) in kebab_warnings.iter().zip(snake_warnings.iter()).enumerate() {
                assert_eq!(
                    kw.message, sw.message,
                    "warning[{i}] message differs between '{kebab}' and '{snake}'"
                );
                assert_eq!(
                    kw.line, sw.line,
                    "warning[{i}] line differs between '{kebab}' and '{snake}'"
                );
            }
        }
    }

    fn assert_fix_roundtrip_from_toml(style: &str, content: &str) {
        let rule = rule_from_toml_style(style);
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx2).unwrap();
        assert!(
            remaining.is_empty(),
            "style '{style}': after fix(), check() should find 0 violations.\n\
             Original: {content:?}\n\
             Fixed:    {fixed:?}\n\
             Remaining: {remaining:?}"
        );
    }

    #[test]
    fn test_roundtrip_kebab_no_leading_or_trailing() {
        assert_fix_roundtrip_from_toml("no-leading-or-trailing", "| H1 | H2 |\n|---|---|\n| a | b |");
    }

    #[test]
    fn test_roundtrip_kebab_leading_and_trailing() {
        assert_fix_roundtrip_from_toml("leading-and-trailing", "H1 | H2\n---|---\na | b");
    }

    #[test]
    fn test_roundtrip_kebab_leading_only() {
        assert_fix_roundtrip_from_toml("leading-only", "| H1 | H2 |\n|---|---|\n| a | b |");
    }

    #[test]
    fn test_roundtrip_kebab_trailing_only() {
        assert_fix_roundtrip_from_toml("trailing-only", "| H1 | H2 |\n|---|---|\n| a | b |");
    }

    #[test]
    fn test_md055_delimiter_row_handling() {
        // Test with no_leading_or_trailing style
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

        let content = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // With the fixed implementation, the delimiter row should have pipes removed
        // Spacing is preserved from original input
        let expected = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";

        assert_eq!(result, expected);

        // Test that the check method actually reports the delimiter row as an issue
        let warnings = rule.check(&ctx).unwrap();
        let delimiter_warning = &warnings[1]; // Second warning should be for delimiter row
        assert_eq!(delimiter_warning.line, 2);
        assert_eq!(
            delimiter_warning.message,
            "Table pipe style should be no leading or trailing"
        );

        // Test with leading_and_trailing style
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        let content = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // The delimiter row should have pipes added
        // Spacing is preserved from original input
        let expected = "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1   | Data 2   | Data 3 |";

        assert_eq!(result, expected);
    }

    #[test]
    fn test_md055_check_finds_delimiter_row_issues() {
        // Test that check() correctly identifies delimiter rows that don't match style
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

        let content = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should have 3 warnings - header row, delimiter row, and data row
        assert_eq!(warnings.len(), 3);

        // Specifically verify the delimiter row warning (line 2)
        let delimiter_warning = &warnings[1];
        assert_eq!(delimiter_warning.line, 2);
        assert_eq!(
            delimiter_warning.message,
            "Table pipe style should be no leading or trailing"
        );
    }

    #[test]
    fn test_md055_real_world_example() {
        // Test with a real-world example having content before and after the table
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

        let content = "# Table Example\n\nHere's a table with leading and trailing pipes:\n\n| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |\n| Data 4   | Data 5   | Data 6   |\n\nMore content after the table.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // The table should be fixed, with pipes removed
        // Spacing is preserved from original input
        let expected = "# Table Example\n\nHere's a table with leading and trailing pipes:\n\nHeader 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3\nData 4   | Data 5   | Data 6\n\nMore content after the table.";

        assert_eq!(result, expected);

        // Ensure we get warnings for all table rows
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 4); // All four table rows should have warnings

        // The line numbers should match the correct positions in the original content
        assert_eq!(warnings[0].line, 5); // Header row
        assert_eq!(warnings[1].line, 6); // Delimiter row
        assert_eq!(warnings[2].line, 7); // Data row 1
        assert_eq!(warnings[3].line, 8); // Data row 2
    }

    #[test]
    fn test_md055_invalid_style() {
        // Test with an invalid style setting
        let rule = MD055TablePipeStyle::new("leading_or_trailing".to_string()); // Invalid style

        let content = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Should default to "leading_and_trailing"
        // Already has leading and trailing pipes, so no changes needed - spacing is preserved
        let expected = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";

        assert_eq!(result, expected);

        // Now check a content that needs actual modification
        let content = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";
        let ctx2 = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx2).unwrap();

        // Should add pipes to match the default "leading_and_trailing" style
        // Spacing is preserved from original input
        let expected = "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1   | Data 2   | Data 3 |";
        assert_eq!(result, expected);

        // Check that warning messages also work with the fallback style
        let warnings = rule.check(&ctx2).unwrap();

        // Since content doesn't have leading/trailing pipes but defaults to "leading_and_trailing",
        // there should be warnings for all rows
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn test_underflow_protection() {
        // Test case to ensure no underflow when parts is empty
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        // Test with empty string (edge case)
        let result = rule.fix_table_row("", "leading_and_trailing");
        assert_eq!(result, "");

        // Test with string that doesn't contain pipes
        let result = rule.fix_table_row("no pipes here", "leading_and_trailing");
        assert_eq!(result, "no pipes here");

        // Test with minimal pipe content
        let result = rule.fix_table_row("|", "leading_and_trailing");
        // Should not panic and should handle gracefully
        assert!(!result.is_empty());
    }

    // === Issue #305: Blockquote table tests ===

    #[test]
    fn test_fix_table_row_in_blockquote() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        // Blockquote table without leading pipe
        let result = rule.fix_table_row("> H1 | H2", "leading_and_trailing");
        assert_eq!(result, "> | H1 | H2 |");

        // Blockquote table that already has pipes
        let result = rule.fix_table_row("> | H1 | H2 |", "leading_and_trailing");
        assert_eq!(result, "> | H1 | H2 |");

        // Removing pipes from blockquote table
        let result = rule.fix_table_row("> | H1 | H2 |", "no_leading_or_trailing");
        assert_eq!(result, "> H1 | H2");
    }

    #[test]
    fn test_fix_table_row_in_nested_blockquote() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        // Double-nested blockquote
        let result = rule.fix_table_row(">> H1 | H2", "leading_and_trailing");
        assert_eq!(result, ">> | H1 | H2 |");

        // Triple-nested blockquote
        let result = rule.fix_table_row(">>> H1 | H2", "leading_and_trailing");
        assert_eq!(result, ">>> | H1 | H2 |");
    }

    #[test]
    fn test_blockquote_table_full_document() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        // Full table in blockquote (2 columns, matching delimiter)
        let content = "> H1 | H2\n> ----|----\n> a  | b";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Each line should have the blockquote prefix preserved and pipes added
        // The leading_and_trailing style adds "| " after blockquote prefix
        assert!(
            result.starts_with("> |"),
            "Header should start with blockquote + pipe. Got:\n{result}"
        );
        // Delimiter row gets leading pipe added, so check for "> | ---" pattern
        assert!(
            result.contains("> | ----"),
            "Delimiter should have blockquote prefix + leading pipe. Got:\n{result}"
        );
    }

    #[test]
    fn test_blockquote_table_no_leading_trailing() {
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

        // Table with pipes that should be removed
        let content = "> | H1 | H2 |\n> |----|----|---|\n> | a  | b |";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Pipes should be removed but blockquote prefix preserved
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[0].starts_with("> "), "Line should start with blockquote prefix");
        assert!(
            !lines[0].starts_with("> |"),
            "Leading pipe should be removed. Got: {}",
            lines[0]
        );
    }

    #[test]
    fn test_mixed_regular_and_blockquote_tables() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

        // Document with both regular and blockquote tables
        let content = "H1 | H2\n---|---\na | b\n\n> H3 | H4\n> ---|---\n> c | d";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Both tables should be fixed
        assert!(result.contains("| H1 | H2 |"), "Regular table should have pipes added");
        assert!(
            result.contains("> | H3 | H4 |"),
            "Blockquote table should have pipes added with prefix preserved"
        );
    }

    // === Roundtrip safety tests ===

    fn assert_fix_roundtrip(rule: &MD055TablePipeStyle, content: &str) {
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx2).unwrap();
        assert!(
            remaining.is_empty(),
            "After fix(), check() should find 0 violations.\nOriginal: {content:?}\nFixed: {fixed:?}\nRemaining: {remaining:?}"
        );
    }

    #[test]
    fn test_roundtrip_leading_and_trailing() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        assert_fix_roundtrip(&rule, "H1 | H2\n---|---\na | b");
    }

    #[test]
    fn test_roundtrip_no_leading_or_trailing() {
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());
        assert_fix_roundtrip(&rule, "| H1 | H2 |\n|---|---|\n| a | b |");
    }

    #[test]
    fn test_roundtrip_consistent_mode() {
        let rule = MD055TablePipeStyle::default();
        assert_fix_roundtrip(&rule, "| H1 | H2 |\n|---|---|\nCell 1 | Cell 2");
    }

    #[test]
    fn test_roundtrip_blockquote_table() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        assert_fix_roundtrip(&rule, "> H1 | H2\n> ---|---\n> a | b");
    }

    #[test]
    fn test_roundtrip_mixed_tables() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        assert_fix_roundtrip(&rule, "H1 | H2\n---|---\na | b\n\n> H3 | H4\n> ---|---\n> c | d");
    }

    #[test]
    fn test_roundtrip_with_surrounding_content() {
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());
        assert_fix_roundtrip(&rule, "# Title\n\n| H1 | H2 |\n|---|---|\n| a | b |\n\nMore text.");
    }

    #[test]
    fn test_roundtrip_clean_content() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        assert_fix_roundtrip(&rule, "| H1 | H2 |\n|---|---|\n| a | b |");
    }

    // === Pandoc construct reachability tests ===
    //
    // These tests document that MD055 does not flag Pandoc-specific constructs
    // (grid tables, multi-line tables, line blocks, pipe-table captions) because
    // `ctx.table_blocks` excludes them at the source:
    //
    // - Grid table delimiters use `+---+---+` (no `|`), so `is_delimiter_row`
    //   returns false and no `TableBlock` is created.
    // - Multi-line table separators (`----------`) have no `|`, same exclusion.
    // - Line blocks (`| First line`) end without `|`, so `is_potential_table_row`
    //   requires `valid_parts >= 2` but finds only 1 — excluded.
    // - Pipe-table captions (`: caption`) have no `|` — excluded.
    //
    // No production guard is needed. These tests ensure that if `find_table_blocks`
    // ever changes to include these constructs, the failure is visible.

    #[test]
    fn md055_pandoc_grid_tables_not_flagged() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        let content = "\
+---+---+
| a | b |
+===+===+
| 1 | 2 |
+---+---+
";
        // Under Pandoc: grid tables are excluded from table_blocks (delimiter rows
        // use `+` not `|`), so no warnings are emitted.
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD055 should not flag Pandoc grid tables (excluded by table_blocks): {result:?}"
        );

        // Under Standard: same content also produces no warnings because the
        // `+---+---+` lines are not recognized as pipe-table delimiters.
        let ctx_std = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD055 should not flag grid-table-like content under Standard either: {result_std:?}"
        );
    }

    #[test]
    fn md055_pandoc_multi_line_tables_not_flagged() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        // Multi-line table (Pandoc extension): separator line has no `|`.
        let content = "\
--------- ----------- ------
Header 1   Header 2   Header 3
--------- ----------- ------
Cell 1     Cell 2     Cell 3
--------- ----------- ------
";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD055 should not flag Pandoc multi-line tables: {result:?}"
        );

        let ctx_std = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD055 should not flag multi-line table content under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md055_pandoc_line_blocks_not_flagged() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        // Pandoc line blocks: `| text` that starts with `|` but does not end with `|`.
        // is_potential_table_row requires valid_parts >= 2 for non-outer-piped lines.
        let content = "| First line\n| Second line\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD055 should not treat Pandoc line blocks as tables: {result:?}"
        );

        let ctx_std = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD055 should not treat line-block-like content as tables under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md055_pandoc_pipe_table_captions_not_flagged() {
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        // Pipe-table captions (`: caption`) have no `|`, so they are never included
        // in table_blocks.
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b  |

: My table caption
";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD055 should not flag the pipe-table caption line: {result:?}"
        );

        // Under Standard: same table rows are correctly checked; caption line is ignored.
        let ctx_std = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD055 already-valid table with caption should have no warnings under Standard: {result_std:?}"
        );
    }

    // === MDG: the Gherkin form is enforced over an incompatible style ===
    //
    // Gherkin recognizes a Data Table or Examples table row only when an indent
    // is followed directly by a pipe, so a style that strips the leading pipe
    // does not restyle the table, it deletes it from the document.

    /// Each style a Gherkin document cannot carry, paired with the rows a table
    /// written in that style would contain.
    const MDG_INCOMPATIBLE_STYLES: [(&str, [&str; 3]); 3] = [
        (
            "no_leading_or_trailing",
            ["start | eat | left", "----- | --- | ----", "12 | 5 | 7"],
        ),
        (
            "leading_only",
            ["| start | eat | left", "| ----- | --- | ----", "| 12 | 5 | 7"],
        ),
        (
            "trailing_only",
            ["start | eat | left |", "----- | --- | ---- |", "12 | 5 | 7 |"],
        ),
    ];

    const MDG_GHERKIN_ROWS: [&str; 3] = ["| start | eat | left |", "| ----- | --- | ---- |", "| 12 | 5 | 7 |"];

    fn examples_table(indent: usize, rows: [&str; 3]) -> String {
        let spaces = " ".repeat(indent);
        let [header, delimiter, body] = rows;
        format!("# Feature: Eating\n\n#### Examples:\n\n{spaces}{header}\n{spaces}{delimiter}\n{spaces}{body}\n")
    }

    #[test]
    fn test_mdg_enforces_leading_and_trailing_over_incompatible_styles() {
        // Two to five whitespace characters are all Gherkin accepts before the
        // pipe, so the whole range has to survive the correction.
        for (style, rows) in MDG_INCOMPATIBLE_STYLES {
            let rule = MD055TablePipeStyle::new(style.to_string());

            for indent in [2, 3, 4, 5] {
                let content = examples_table(indent, rows);
                let expected = examples_table(indent, MDG_GHERKIN_ROWS);
                let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);

                assert_eq!(
                    rule.check(&ctx).unwrap().len(),
                    3,
                    "style '{style}' at indent {indent}: every row is in a form MDG cannot accept"
                );

                let fixed = rule.fix(&ctx).unwrap();
                assert_eq!(
                    fixed, expected,
                    "style '{style}' at indent {indent}: MDG must enforce the Gherkin form and leave the indent alone"
                );

                let fixed_ctx = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
                assert!(rule.check(&fixed_ctx).unwrap().is_empty());
                assert_eq!(
                    rule.fix(&fixed_ctx).unwrap(),
                    fixed,
                    "style '{style}' at indent {indent}: MDG fix must be idempotent"
                );
            }
        }
    }

    #[test]
    fn test_mdg_leaves_a_table_already_in_the_required_form_alone() {
        // The reproducing case: rows Gherkin already recognizes must survive
        // every style the user could have configured.
        let content = examples_table(2, MDG_GHERKIN_ROWS);

        for style in [
            "consistent",
            "leading_and_trailing",
            "no_leading_or_trailing",
            "leading_only",
            "trailing_only",
        ] {
            let rule = MD055TablePipeStyle::new(style.to_string());
            let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);

            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "style '{style}': MDG enforces this form, so it cannot be reported"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), content, "style '{style}': nothing to correct");
        }
    }

    #[test]
    fn test_mdg_consistent_ignores_prevalence() {
        // `consistent` asks for no particular form, so it is never warned about;
        // MDG still resolves it to the Gherkin form rather than to whichever
        // form the table happens to be written in.
        let defaulted = MD055TablePipeStyle::default();
        assert_eq!(defaulted.config.style, "consistent");
        let explicit = MD055TablePipeStyle::new("consistent".to_string());

        for (style, rows) in MDG_INCOMPATIBLE_STYLES {
            let content = examples_table(2, rows);
            let expected = examples_table(2, MDG_GHERKIN_ROWS);

            for rule in [&defaulted, &explicit] {
                let standard_ctx =
                    crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
                assert!(
                    rule.check(&standard_ctx).unwrap().is_empty(),
                    "Standard resolves `consistent` to the table's own '{style}'"
                );

                let mdg_ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);
                assert_eq!(
                    rule.fix(&mdg_ctx).unwrap(),
                    expected,
                    "MDG must resolve `consistent` to the Gherkin form over a '{style}' table"
                );
            }
        }
    }

    #[test]
    fn test_standard_flavor_is_untouched_by_the_mdg_enforcement() {
        for (style, rows) in MDG_INCOMPATIBLE_STYLES {
            let rule = MD055TablePipeStyle::new(style.to_string());
            let content = examples_table(2, rows);
            let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "style '{style}': Standard must still honour it"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), content, "style '{style}': nothing to correct");

            // And it is still applied to a table written the other way round.
            let mdg_form = examples_table(2, MDG_GHERKIN_ROWS);
            let mdg_form_ctx =
                crate::lint_context::LintContext::new(&mdg_form, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(
                rule.check(&mdg_form_ctx).unwrap().len(),
                3,
                "style '{style}': Standard must still correct the leading-and-trailing form away"
            );
        }

        // Preserving the indent is a Gherkin concession; Standard keeps
        // left-aligning the rows it rewrites.
        let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
        let content = examples_table(2, MDG_INCOMPATIBLE_STYLES[0].1);
        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            examples_table(0, MDG_GHERKIN_ROWS),
            "Standard must keep stripping the indent"
        );
    }

    #[test]
    fn test_from_config_records_whether_style_was_configured() {
        // The MDG override applies either way, but the warning is only for a
        // style the user actually asked for, so a configured style has to be
        // told apart from a defaulted one.
        use crate::config::Config;
        use std::collections::BTreeMap;

        let mut values = BTreeMap::new();
        values.insert(
            "style".to_string(),
            toml::Value::String("no_leading_or_trailing".to_string()),
        );
        let mut config = Config::default();
        config.rules.insert(
            "MD055".to_string(),
            crate::config::RuleConfig { severity: None, values },
        );

        let configured = MD055TablePipeStyle::from_config(&config);
        let configured = configured.as_any().downcast_ref::<MD055TablePipeStyle>().unwrap();
        assert_eq!(configured.config.style, "no_leading_or_trailing");
        assert!(configured.style_explicit);

        let defaulted = MD055TablePipeStyle::from_config(&Config::default());
        let defaulted = defaulted.as_any().downcast_ref::<MD055TablePipeStyle>().unwrap();
        assert_eq!(defaulted.config.style, "consistent");
        assert!(!defaulted.style_explicit);

        // The override does not depend on the warning: a defaulted incompatible
        // style is enforced just the same.
        let unreported = MD055TablePipeStyle::from_config_struct(MD055Config {
            style: "no_leading_or_trailing".to_string(),
        });
        assert!(!unreported.style_explicit);
        let content = examples_table(2, MDG_INCOMPATIBLE_STYLES[0].1);
        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(unreported.fix(&ctx).unwrap(), examples_table(2, MDG_GHERKIN_ROWS));
    }
}
