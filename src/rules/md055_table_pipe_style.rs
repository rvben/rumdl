use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD055: Table pipe style should be consistent
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

    /// Check if a line is within a code block
    fn is_in_code_block(&self, lines: &[&str], line_index: usize) -> bool {
        let mut in_code_block = false;

        let mut code_fence = None;

        for (_i, line) in lines.iter().enumerate().take(line_index + 1) {
            let trimmed = line.trim();

            // Check for code fence markers
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    // Start of a code block
                    in_code_block = true;
                    code_fence = Some(if trimmed.starts_with("```") {
                        "```"
                    } else {
                        "~~~"
                    });
                } else if let Some(fence) = code_fence {
                    // End of a code block if the fence type matches
                    if trimmed.starts_with(fence) {
                        in_code_block = false;
                        code_fence = None;
                    }
                }
            }
        }

        in_code_block
    }

    /// Identify table sections (groups of lines that form a table)
    fn identify_tables(&self, lines: &[&str]) -> Vec<Vec<usize>> {
        let mut tables = Vec::new();

        let mut current_table_start: Option<usize> = None;

        let mut has_delimiter_row = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip lines in code blocks
            if self.is_in_code_block(lines, i) {
                // If we were tracking a table, end it
                if let Some(start) = current_table_start {
                    if has_delimiter_row && i - start >= 2 {
                        tables.push((start..i).collect());
                    }
                    current_table_start = None;
                    has_delimiter_row = false;
                }
                continue;
            }

            // Check if this is a potential table row
            if trimmed.contains('|') {
                // Check if this is a delimiter row (contains only |, -, :, and whitespace)
                let is_delimiter = self.is_delimiter_row(line);

                if is_delimiter {
                    has_delimiter_row = true;
                }

                // If we're not already tracking a table, start a new one
                if current_table_start.is_none() {
                    current_table_start = Some(i);
                }
            } else if trimmed.is_empty() {
                // Empty line - end the current table if we have one
                if let Some(start) = current_table_start {
                    if has_delimiter_row && i - start >= 2 {
                        tables.push((start..i).collect());
                    }
                    current_table_start = None;
                    has_delimiter_row = false;
                }
            }
        }

        // Handle case where table extends to EOF
        if let Some(start) = current_table_start {
            if has_delimiter_row && lines.len() - start >= 2 {
                tables.push((start..lines.len()).collect());
            }
        }

        tables
    }

    /// Check if a line is a delimiter row
    fn is_delimiter_row(&self, line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains('|')
            && trimmed
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
    }

    /// Fix a table row to match the desired style
    fn fix_table_row(&self, line: &str, target_style: &str) -> String {
        let trimmed = line.trim();

        // If the line is not a table row, return it unchanged
        if !trimmed.contains('|') {
            return line.to_string();
        }

        // Get the leading whitespace to preserve indentation

        let leading_whitespace = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let result = match target_style {
            "leading_and_trailing" => {
                let mut result = trimmed.to_string();
                if !result.starts_with('|') {
                    result = format!("| {}", result);
                }
                if !result.ends_with('|') {
                    result = format!("{} |", result);
                }
                result
            }
            "leading_only" => {
                let mut result = trimmed.to_string();
                if !result.starts_with('|') {
                    result = format!("| {}", result);
                }
                if result.ends_with('|') {
                    result = result[..result.len() - 1].trim_end().to_string();
                }
                result
            }
            "trailing_only" => {
                let mut result = trimmed.to_string();
                if result.starts_with('|') {
                    result = result[1..].trim_start().to_string();
                }
                if !result.ends_with('|') {
                    result = format!("{} |", result);
                }
                result
            }
            "no_leading_or_trailing" => {
                let mut result = trimmed.to_string();
                if result.starts_with('|') {
                    result = result[1..].trim_start().to_string();
                }
                if result.ends_with('|') {
                    result = result[..result.len() - 1].trim_end().to_string();
                }
                result
            }
            _ => trimmed.to_string(),
        };

        // Preserve the original indentation
        format!("{}{}", leading_whitespace, result)
    }
}

impl Rule for MD055TablePipeStyle {
    fn name(&self) -> &'static str {
        "MD055"
    }

    fn description(&self) -> &'static str {
        "Table pipe style"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Identify tables in the content

        let tables = self.identify_tables(&lines);

        for _table in tables {
            let mut table_styles = Vec::new();

            // First pass: collect all styles used in this table (excluding delimiter rows)
            for (line_idx, line) in lines.iter().enumerate() {
                if self.is_in_code_block(&lines, line_idx) {
                    continue;
                }

                if let Some(style) = self.determine_pipe_style(line) {
                    table_styles.push((line_idx, style));
                }
            }

            if table_styles.is_empty() {
                continue;
            }

            // Determine the expected style based on configuration or first row
            let expected_style = match self.style.as_str() {
                "consistent" => {
                    // If no style is configured, use the style of the first row
                    if let Some((_, style)) = table_styles.first() {
                        *style
                    } else {
                        continue;
                    }
                }
                "leading_and_trailing" => "leading_and_trailing",
                "leading_only" => "leading_only",
                "trailing_only" => "trailing_only",
                "no_leading_or_trailing" => "no_leading_or_trailing",
                _ => continue, // Invalid style configuration
            };

            // Second pass: check all rows against the expected style
            for (line_idx, line) in lines.iter().enumerate() {
                if self.is_in_code_block(&lines, line_idx) {
                    continue;
                }

                // For delimiter rows, we need to check if they match the expected style
                if self.is_delimiter_row(line) {
                    let trimmed = line.trim();
                    let has_leading = trimmed.starts_with('|');
                    let has_trailing = trimmed.ends_with('|');

                    let matches_style = match expected_style {
                        "leading_and_trailing" => has_leading && has_trailing,
                        "leading_only" => has_leading && !has_trailing,
                        "trailing_only" => !has_leading && has_trailing,
                        "no_leading_or_trailing" => !has_leading && !has_trailing,
                        _ => true,
                    };

                    if !matches_style {
                        warnings.push(LintWarning {
                            message: format!(
                                "Table pipe style for delimiter row is not consistent with {}",
                                if self.style == "consistent" {
                                    "the first row"
                                } else {
                                    "the configured style"
                                }
                            ),
                            line: line_idx + 1,
                            column: 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(line_idx + 1, 1),
                                replacement: self.fix_table_row(line, expected_style),
                            }),
                        });
                    }
                } else if let Some(style) = self.determine_pipe_style(line) {
                    if style != expected_style {
                        warnings.push(LintWarning {
                            message: format!(
                                "Table pipe style '{}' is not consistent with {}",
                                style,
                                if self.style == "consistent" {
                                    "the first row"
                                } else {
                                    "the configured style"
                                }
                            ),
                            line: line_idx + 1,
                            column: 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(line_idx + 1, 1),
                                replacement: self.fix_table_row(line, expected_style),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let warnings = self.check(content)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::with_capacity(lines.len());

        for (i, line) in lines.iter().enumerate() {
            let warning_idx = warnings.iter().position(|w| w.line == i + 1);
            if let Some(idx) = warning_idx {
                if let Some(fix) = &warnings[idx].fix {
                    result.push(fix.replacement.clone());
                    continue;
                }
            }
            result.push(line.to_string());
        }

        // Preserve the original line endings
        if content.ends_with('\n') {
            Ok(result.join("\n") + "\n")
        } else {
            Ok(result.join("\n"))
        }
    }
}
