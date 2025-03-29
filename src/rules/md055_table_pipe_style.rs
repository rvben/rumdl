use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::code_block_utils::CodeBlockUtils;

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

    /// Check if a line is a delimiter row (contains only |, -, :, and whitespace)
    fn is_delimiter_row(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
    }

    /// Fix a table row to match the target style
    fn fix_table_row(&self, line: &str, target_style: &str) -> String {
        let trimmed = line.trim();
        
        // Don't modify empty lines or delimiter rows
        if trimmed.is_empty() || self.is_delimiter_row(line) {
            return line.to_string();
        }

        match target_style {
            "leading_and_trailing" => {
                let mut result = String::new();
                if !trimmed.starts_with('|') {
                    result.push('|');
                }
                result.push_str(trimmed);
                if !trimmed.ends_with('|') {
                    result.push('|');
                }
                result
            }
            "no_leading_or_trailing" => {
                let mut result = trimmed.to_string();
                if result.starts_with('|') {
                    result = result[1..].to_string();
                }
                if result.ends_with('|') {
                    result.pop();
                }
                result.trim().to_string()
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

    fn check(&self, content: &str) -> LintResult {
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Use CodeBlockUtils to properly detect code blocks
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);

        // Track table state
        let mut in_table = false;
        let mut table_style = None;
        let mut current_table_start = None;

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Skip if this line is in a code block
            if code_blocks.iter().any(|(start, end)| line_start >= *start && line_start < *end) {
                continue;
            }

            let trimmed = line.trim();

            // Check for table start/end
            if trimmed.contains('|') {
                if !in_table {
                    in_table = true;
                    current_table_start = Some(i);
                }

                // Skip delimiter rows for style checks
                if self.is_delimiter_row(line) {
                    continue;
                }

                let line_style = self.determine_pipe_style(line);
                
                if let Some(style) = line_style {
                    // For "consistent" mode, use the first table's style
                    if table_style.is_none() && self.style == "consistent" {
                        table_style = Some(style);
                    }

                    let target_style = if self.style == "consistent" {
                        table_style.unwrap_or("leading_and_trailing")
                    } else {
                        &self.style
                    };

                    if style != target_style {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: format!("Table pipe style should be {}", target_style.replace('_', " ")),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(i + 1, 1),
                                replacement: self.fix_table_row(line, target_style),
                            }),
                        });
                    }
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

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        let mut result = String::new();
        let mut table_style = None;
        let mut in_table = false;

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Don't modify lines in code blocks
            if code_blocks.iter().any(|(start, end)| line_start >= *start && line_start < *end) {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            let trimmed = line.trim();

            if trimmed.contains('|') {
                if !in_table {
                    in_table = true;
                }

                if let Some(style) = self.determine_pipe_style(line) {
                    // For "consistent" mode, use the first table's style
                    if table_style.is_none() && self.style == "consistent" {
                        table_style = Some(style);
                    }

                    let target_style = if self.style == "consistent" {
                        table_style.unwrap_or("leading_and_trailing")
                    } else {
                        &self.style
                    };

                    result.push_str(&self.fix_table_row(line, target_style));
                } else {
                    result.push_str(line);
                }
            } else {
                if trimmed.is_empty() {
                    in_table = false;
                    if !in_table {
                        table_style = None;
                    }
                }
                result.push_str(line);
            }
            result.push('\n');
        }

        Ok(result)
    }
}
