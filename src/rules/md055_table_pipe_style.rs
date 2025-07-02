use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::{LineIndex, calculate_line_range};
use crate::utils::table_utils::TableUtils;

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

    /// Fix a table row to match the target style
    fn fix_table_row(&self, line: &str, target_style: &str) -> String {
        let trimmed = line.trim();
        if !trimmed.contains('|') {
            return line.to_string();
        }

        // Check if this is a delimiter row (contains dashes)
        let is_delimiter_row = trimmed.contains('-')
            && trimmed
                .chars()
                .all(|c| c == '-' || c == ':' || c == '|' || c.is_whitespace());

        // Split the line by pipes to get the content
        let parts: Vec<&str> = trimmed.split('|').collect();
        let mut content_parts = Vec::new();

        // Extract the actual content (skip empty leading/trailing parts)
        let start_idx = if parts.first().is_some_and(|p| p.trim().is_empty()) {
            1
        } else {
            0
        };
        let end_idx = if parts.last().is_some_and(|p| p.trim().is_empty()) {
            if !parts.is_empty() { parts.len() - 1 } else { 0 }
        } else {
            parts.len()
        };

        for part in parts.iter().take(end_idx).skip(start_idx) {
            // Trim each part to remove extra spaces, but preserve the content
            content_parts.push(part.trim());
        }

        // Rebuild the line with the target style
        match target_style {
            "leading_and_trailing" => {
                if is_delimiter_row {
                    format!("| {} |", content_parts.join("|"))
                } else {
                    format!("| {} |", content_parts.join(" | "))
                }
            }
            "leading_only" => {
                if is_delimiter_row {
                    format!("| {}", content_parts.join("|"))
                } else {
                    format!("| {}", content_parts.join(" | "))
                }
            }
            "trailing_only" => {
                if is_delimiter_row {
                    format!("{} |", content_parts.join("|"))
                } else {
                    format!("{} |", content_parts.join(" | "))
                }
            }
            "no_leading_or_trailing" => {
                if is_delimiter_row {
                    content_parts.join("|")
                } else {
                    content_parts.join(" | ")
                }
            }
            _ => line.to_string(),
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

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

        // Use shared table detection for better performance
        let table_blocks = TableUtils::find_table_blocks(content, ctx);

        // Process each table block
        for table_block in table_blocks {
            let mut table_style = None;

            // First pass: determine the table's style for "consistent" mode
            if configured_style == "consistent" {
                // Check header row first
                if let Some(style) = TableUtils::determine_pipe_style(lines[table_block.header_line]) {
                    table_style = Some(style);
                } else {
                    // Check content rows if header doesn't have a clear style
                    for &line_idx in &table_block.content_lines {
                        if let Some(style) = TableUtils::determine_pipe_style(lines[line_idx]) {
                            table_style = Some(style);
                            break;
                        }
                    }
                }
            }

            // Determine target style for this table
            let target_style = if configured_style == "consistent" {
                table_style.unwrap_or("leading_and_trailing")
            } else {
                configured_style
            };

            // Check all rows in the table
            let all_lines = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied());

            for line_idx in all_lines {
                let line = lines[line_idx];
                if let Some(current_style) = TableUtils::determine_pipe_style(line) {
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

                        let fixed_line = self.fix_table_row(line, target_style);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            severity: Severity::Warning,
                            message,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            fix: Some(crate::rule::Fix {
                                range: line_index.whole_line_range(line_idx + 1),
                                replacement: if line_idx < lines.len() - 1 {
                                    format!("{fixed_line}\n")
                                } else {
                                    fixed_line
                                },
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

        // Use shared table detection for better performance
        let table_blocks = TableUtils::find_table_blocks(content, ctx);

        // Create a copy of lines that we can modify
        let mut result_lines = lines.iter().map(|&s| s.to_string()).collect::<Vec<String>>();

        // Process each table block
        for table_block in table_blocks {
            let mut table_style = None;

            // First pass: determine the table's style for "consistent" mode
            if configured_style == "consistent" {
                // Check header row first
                if let Some(style) = TableUtils::determine_pipe_style(lines[table_block.header_line]) {
                    table_style = Some(style);
                } else {
                    // Check content rows if header doesn't have a clear style
                    for &line_idx in &table_block.content_lines {
                        if let Some(style) = TableUtils::determine_pipe_style(lines[line_idx]) {
                            table_style = Some(style);
                            break;
                        }
                    }
                }
            }

            // Determine target style for this table
            let target_style = if configured_style == "consistent" {
                table_style.unwrap_or("leading_and_trailing")
            } else {
                configured_style
            };

            // Fix all rows in the table
            let all_lines = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied());

            for line_idx in all_lines {
                let line = lines[line_idx];
                let fixed_line = self.fix_table_row(line, target_style);
                result_lines[line_idx] = fixed_line;
            }
        }

        Ok(result_lines.join("\n"))
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
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // With the fixed implementation, the delimiter row should have pipes removed
        let expected = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1 | Data 2 | Data 3";

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
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Output the actual result for debugging
        log::info!("Actual leading_and_trailing result:\n{}", result.replace('\n', "\\n"));

        // The delimiter row should have pipes added with spacing as in the implementation
        let expected =
            "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";

        assert_eq!(result, expected);
    }

    #[test]
    fn test_md055_check_finds_delimiter_row_issues() {
        // Test that check() correctly identifies delimiter rows that don't match style
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

        let content = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";
        let ctx = crate::lint_context::LintContext::new(content);
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
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // The table should be fixed, with delimiter row pipes properly removed
        let expected = "# Table Example\n\nHere's a table with leading and trailing pipes:\n\nHeader 1 | Header 2 | Header 3\n----------|----------|----------\nData 1 | Data 2 | Data 3\nData 4 | Data 5 | Data 6\n\nMore content after the table.";

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
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Output the actual result for debugging
        log::info!("Actual result with invalid style:\n{}", result.replace('\n', "\\n"));

        // Should default to "leading_and_trailing" and fix any inconsistencies with that style
        let expected =
            "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";

        // Should match the expected output after processing with the default style
        assert_eq!(result, expected);

        // Now check a content that needs actual modification
        let content = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";
        let ctx2 = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx2).unwrap();

        // Should add pipes to match the default "leading_and_trailing" style
        let expected =
            "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";
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
}
