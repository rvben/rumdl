use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::table_utils::{TableBlock, TableUtils};

mod md055_config;
use md055_config::MD055Config;

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
}

impl MD055TablePipeStyle {
    pub fn new(style: String) -> Self {
        Self {
            config: MD055Config { style },
        }
    }

    pub fn from_config_struct(config: MD055Config) -> Self {
        Self { config }
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
        self.fix_table_row_with_context(line, target_style, &dummy_block, 0)
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
    ) -> String {
        // Extract blockquote prefix first
        let (bq_prefix, after_bq) = TableUtils::extract_blockquote_prefix(line);

        // Handle list context if present
        if let Some(ref list_ctx) = table_block.list_context {
            if table_line_index == 0 {
                // Header line: extract list prefix
                let (list_prefix, content, _) = TableUtils::extract_list_prefix(after_bq);
                let fixed_content = self.fix_table_content(content.trim(), target_style);

                // Restore prefixes: blockquote + list prefix + fixed content
                if bq_prefix.is_empty() && list_prefix.is_empty() {
                    fixed_content
                } else {
                    format!("{bq_prefix}{list_prefix}{fixed_content}")
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
            if bq_prefix.is_empty() {
                fixed_content
            } else {
                format!("{bq_prefix}{fixed_content}")
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no tables present (uses cached pipe count)
        !ctx.likely_has_tables()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;
        let mut warnings = Vec::new();

        // Early return handled by should_skip()

        let lines: Vec<&str> = content.lines().collect();

        // Get the configured style explicitly and validate it
        let configured_style = match self.config.style.as_str() {
            "leading_and_trailing" | "no_leading_or_trailing" | "leading_only" | "trailing_only" | "consistent" => {
                self.config.style.as_str()
            }
            _ => {
                // Invalid style provided, default to "leading_and_trailing"
                "leading_and_trailing"
            }
        };

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        // Process each table block
        for table_block in table_blocks {
            // First pass: determine the table's style for "consistent" mode
            // Count all rows to determine most prevalent style (prevalence-based approach)
            let table_style = if configured_style == "consistent" {
                self.determine_table_style(table_block, &lines)
            } else {
                None
            };

            // Determine target style for this table
            let target_style = if configured_style == "consistent" {
                table_style.unwrap_or("leading_and_trailing")
            } else {
                configured_style
            };

            // Collect all table lines for building the whole-table fix
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            // Build the whole-table fix once for all warnings in this table
            // This ensures that applying Quick Fix on any row fixes the entire table
            let table_start_line = table_block.start_line + 1; // Convert to 1-indexed
            let table_end_line = table_block.end_line + 1; // Convert to 1-indexed

            // Build the complete fixed table content with proper table line indices
            let mut fixed_table_lines: Vec<String> = Vec::with_capacity(all_line_indices.len());
            for (table_line_idx, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                let fixed_line = self.fix_table_row_with_context(line, target_style, table_block, table_line_idx);
                if line_idx < lines.len() - 1 {
                    fixed_table_lines.push(format!("{fixed_line}\n"));
                } else {
                    fixed_table_lines.push(fixed_line);
                }
            }
            let table_replacement = fixed_table_lines.concat();
            let table_range = line_index.multi_line_range(table_start_line, table_end_line);

            // Check all rows in the table
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

                        // Each warning uses the same whole-table fix
                        // This ensures Quick Fix on any row fixes the entire table
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            severity: Severity::Warning,
                            message,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            fix: Some(crate::rule::Fix {
                                range: table_range.clone(),
                                replacement: table_replacement.clone(),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();

        // Use the configured style but validate it first
        let configured_style = match self.config.style.as_str() {
            "leading_and_trailing" | "no_leading_or_trailing" | "leading_only" | "trailing_only" | "consistent" => {
                self.config.style.as_str()
            }
            _ => {
                // Invalid style provided, default to "leading_and_trailing"
                "leading_and_trailing"
            }
        };

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        // Create a copy of lines that we can modify
        let mut result_lines = lines.iter().map(|&s| s.to_string()).collect::<Vec<String>>();

        // Process each table block
        for table_block in table_blocks {
            // First pass: determine the table's style for "consistent" mode
            // Count all rows to determine most prevalent style (prevalence-based approach)
            let table_style = if configured_style == "consistent" {
                self.determine_table_style(table_block, &lines)
            } else {
                None
            };

            // Determine target style for this table
            let target_style = if configured_style == "consistent" {
                table_style.unwrap_or("leading_and_trailing")
            } else {
                configured_style
            };

            // Fix all rows in the table with proper table line indices
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            for (table_line_idx, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                let fixed_line = self.fix_table_row_with_context(line, target_style, table_block, table_line_idx);
                result_lines[line_idx] = fixed_line;
            }
        }

        let mut fixed = result_lines.join("\n");
        // Preserve trailing newline if original content had one
        if content.ends_with('\n') && !fixed.ends_with('\n') {
            fixed.push('\n');
        }
        Ok(fixed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD055Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
