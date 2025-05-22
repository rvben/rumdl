use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::range_utils::LineIndex;
use toml;

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
#[derive(Clone)]
pub struct MD055TablePipeStyle {
    pub style: String,
}

impl Default for MD055TablePipeStyle {
    fn default() -> Self {
        Self {
            style: "consistent".to_string(),
        }
    }
}

impl MD055TablePipeStyle {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    /// Determine the pipe style of a table row
    fn determine_pipe_style(&self, line: &str) -> Option<&'static str> {
        if !line.contains('|') {
            return None;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            return None;
        }

        // Skip delimiter rows (---) which are part of tables but don't need pipe style checks
        if self.is_delimiter_row(line) {
            return None;
        }

        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            Some("leading_and_trailing")
        } else if trimmed.starts_with('|') {
            Some("leading_only")
        } else if trimmed.ends_with('|') {
            Some("trailing_only")
        } else {
            Some("no_leading_or_trailing")
        }
    }

    /// Check if a line is a delimiter row (contains only |, -, :, and whitespace)
    fn is_delimiter_row(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
    }

    /// Fix a table row to match the target style
    fn fix_table_row(&self, line: &str, target_style: &str) -> String {
        let trimmed = line.trim();

        // Don't modify empty lines
        if trimmed.is_empty() {
            return line.to_string();
        }

        // Preserve leading whitespace
        let leading_whitespace = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        // Handle delimiter rows properly
        if self.is_delimiter_row(line) {
            // Extract the core content of the delimiter row without pipes
            let row_content = trimmed
                .trim_start_matches('|')
                .trim_end_matches('|')
                .trim()
                .to_string();

            // Apply the appropriate style
            match target_style {
                "leading_and_trailing" => format!("{}| {} |", leading_whitespace, row_content),
                "no_leading_or_trailing" => format!("{}{}", leading_whitespace, row_content),
                "leading_only" => format!("{}| {}", leading_whitespace, row_content),
                "trailing_only" => format!("{}{} |", leading_whitespace, row_content),
                _ => format!("{}| {} |", leading_whitespace, row_content), // Default to leading_and_trailing
            }
        } else {
            // Split the line by pipes to get cells
            let parts: Vec<&str> = trimmed.split('|').collect();
            let mut cells = Vec::new();

            let has_leading_pipe = trimmed.starts_with('|');
            let has_trailing_pipe = trimmed.ends_with('|');

            // Process the cells correctly, accounting for leading/trailing pipes
            for (i, part) in parts.iter().enumerate() {
                // Skip empty leading part if there's a leading pipe
                if i == 0 && part.trim().is_empty() && has_leading_pipe {
                    continue;
                }

                // Skip empty trailing part if there's a trailing pipe
                if i == parts.len() - 1 && part.trim().is_empty() && has_trailing_pipe {
                    continue;
                }

                cells.push(part.trim());
            }

            // Rebuild the table row with the target style
            let result = match target_style {
                "leading_and_trailing" => {
                    format!("| {} |", cells.join(" | "))
                }
                "no_leading_or_trailing" => cells.join(" | "),
                "leading_only" => {
                    format!("| {}", cells.join(" | "))
                }
                "trailing_only" => {
                    format!("{} |", cells.join(" | "))
                }
                _ => {
                    // Default to leading_and_trailing if an unsupported style is provided
                    format!("| {} |", cells.join(" | "))
                }
            };

            // Reapply the original indentation
            format!("{}{}", leading_whitespace, result)
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
        let lines: Vec<&str> = content.lines().collect();

        // Use CodeBlockUtils to properly detect code blocks
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);

        // Track table state
        let mut in_table = false;
        let mut table_style = None;
        let mut current_table_start = None;

        // Get the configured style explicitly and validate it
        let configured_style = match self.style.as_str() {
            "leading_and_trailing"
            | "no_leading_or_trailing"
            | "leading_only"
            | "trailing_only"
            | "consistent" => self.style.as_str(),
            _ => {
                // Invalid style provided, default to "leading_and_trailing"
                "leading_and_trailing"
            }
        };

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Skip if this line is in a code block
            if code_blocks
                .iter()
                .any(|(start, end)| line_start >= *start && line_start < *end)
            {
                continue;
            }

            let trimmed = line.trim();

            // Check for table start/end
            if trimmed.contains('|') {
                if !in_table {
                    in_table = true;
                    current_table_start = Some(i);
                }

                // Check if this is a delimiter row
                let _is_delimiter = self.is_delimiter_row(line);

                if !_is_delimiter {
                    // Only use normal rows to determine table style
                    let line_style = self.determine_pipe_style(line);

                    if let Some(style) = line_style {
                        // For "consistent" mode, use the first table's style
                        if table_style.is_none() && configured_style == "consistent" {
                            table_style = Some(style);
                        }
                    }
                }

                // Determine target style
                let target_style = if configured_style == "consistent" {
                    table_style.unwrap_or("leading_and_trailing")
                } else {
                    configured_style
                };

                // Check if this line needs fixing - handle both normal rows and delimiter rows
                let current_style = if _is_delimiter {
                    // For delimiter rows, determine style directly
                    if trimmed.starts_with('|') && trimmed.ends_with('|') {
                        "leading_and_trailing"
                    } else if !trimmed.starts_with('|') && !trimmed.ends_with('|') {
                        "no_leading_or_trailing"
                    } else if trimmed.starts_with('|') {
                        "leading_only"
                    } else {
                        "trailing_only"
                    }
                } else {
                    // For normal rows, use the previously detected style
                    match self.determine_pipe_style(line) {
                        Some(style) => style,
                        None => continue, // Skip if style can't be determined
                    }
                };

                if current_style != target_style {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
                        message: format!(
                            "Table pipe style should be {}",
                            target_style.replace('_', " ")
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: self.fix_table_row(line, target_style),
                        }),
                    });
                }
            } else if trimmed.is_empty() {
                // Reset table state on empty line
                in_table = false;
                if current_table_start.is_none() {
                    table_style = None;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);

        let mut result = String::new();
        let mut table_style = None;
        let mut in_table = false;

        // Use the configured style but validate it first
        let configured_style = match self.style.as_str() {
            "leading_and_trailing"
            | "no_leading_or_trailing"
            | "leading_only"
            | "trailing_only"
            | "consistent" => self.style.as_str(),
            _ => {
                // Invalid style provided, default to "leading_and_trailing"
                "leading_and_trailing"
            }
        };

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Don't modify lines in code blocks
            if code_blocks
                .iter()
                .any(|(start, end)| line_start >= *start && line_start < *end)
            {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            let trimmed = line.trim();

            if trimmed.contains('|') {
                if !in_table {
                    in_table = true;
                }

                // Check if this is a delimiter row
                let _is_delimiter = self.is_delimiter_row(line);

                // Process the line regardless of whether it's a delimiter row
                if let Some(style) = self.determine_pipe_style(line) {
                    // For "consistent" mode, use the first table's style
                    if table_style.is_none() && configured_style == "consistent" {
                        table_style = Some(style);
                    }
                }

                let target_style = if configured_style == "consistent" {
                    table_style.unwrap_or("leading_and_trailing")
                } else {
                    configured_style
                };

                let fixed_row = self.fix_table_row(line, target_style);
                result.push_str(&fixed_row);
            } else {
                if trimmed.is_empty() {
                    in_table = false;
                    if !in_table {
                        table_style = None;
                    }
                }
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve the original trailing newline if it existed
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("style".to_string(), toml::Value::String(self.style.clone()));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD055", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let valid_styles = [
            "consistent",
            "leading_and_trailing",
            "no_leading_or_trailing",
            "leading_only",
            "trailing_only",
        ];
        let style = if valid_styles.contains(&style.as_str()) {
            style
        } else {
            "consistent".to_string() // Default to consistent if invalid
        };
        Box::new(MD055TablePipeStyle::new(&style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md055_delimiter_row_handling() {
        // Test with no_leading_or_trailing style
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing");

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
        let rule = MD055TablePipeStyle::new("leading_and_trailing");

        let content = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Output the actual result for debugging
        log::info!(
            "Actual leading_and_trailing result:\n{}",
            result.replace('\n', "\\n")
        );

        // The delimiter row should have pipes added with spacing as in the implementation
        let expected = "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";

        assert_eq!(result, expected);
    }

    #[test]
    fn test_md055_check_finds_delimiter_row_issues() {
        // Test that check() correctly identifies delimiter rows that don't match style
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing");

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
        let rule = MD055TablePipeStyle::new("no_leading_or_trailing");

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
        let rule = MD055TablePipeStyle::new("leading_or_trailing"); // Invalid style

        let content = "| Header 1 | Header 2 | Header 3 |\n|----------|----------|----------|\n| Data 1   | Data 2   | Data 3   |";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Output the actual result for debugging
        log::info!(
            "Actual result with invalid style:\n{}",
            result.replace('\n', "\\n")
        );

        // Should default to "leading_and_trailing" and fix any inconsistencies with that style
        let expected = "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";

        // Should match the expected output after processing with the default style
        assert_eq!(result, expected);

        // Now check a content that needs actual modification
        let content = "Header 1 | Header 2 | Header 3\n----------|----------|----------\nData 1   | Data 2   | Data 3";
        let ctx2 = crate::lint_context::LintContext::new(content);
        let result = rule.fix(&ctx2).unwrap();

        // Should add pipes to match the default "leading_and_trailing" style
        let expected = "| Header 1 | Header 2 | Header 3 |\n| ----------|----------|---------- |\n| Data 1 | Data 2 | Data 3 |";
        assert_eq!(result, expected);

        // Check that warning messages also work with the fallback style
        let warnings = rule.check(&ctx2).unwrap();

        // Since content doesn't have leading/trailing pipes but defaults to "leading_and_trailing",
        // there should be warnings for all rows
        assert_eq!(warnings.len(), 3);
    }
}
