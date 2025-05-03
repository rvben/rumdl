use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD056: Table column count
///
/// See [docs/md056.md](../../docs/md056.md) for full documentation, configuration, and examples.
/// Ensures all rows in a table have the same number of cells
#[derive(Debug, Clone)]
pub struct MD056TableColumnCount;

impl Default for MD056TableColumnCount {
    fn default() -> Self {
        MD056TableColumnCount
    }
}

impl MD056TableColumnCount {
    /// Check if a line is in a code block
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

    /// Count cells in a table row
    fn count_cells(&self, row: &str) -> usize {
        let trimmed = row.trim();

        // Skip non-table rows
        if !trimmed.contains('|') {
            return 0;
        }

        // Handle case with leading/trailing pipes
        let mut cell_count = 0;
        let parts: Vec<&str> = trimmed.split('|').collect();

        for (i, part) in parts.iter().enumerate() {
            // Skip first part if it's empty and there's a leading pipe
            if i == 0 && part.trim().is_empty() && parts.len() > 1 {
                continue;
            }

            // Skip last part if it's empty and there's a trailing pipe
            if i == parts.len() - 1 && part.trim().is_empty() && parts.len() > 1 {
                continue;
            }

            cell_count += 1;
        }

        cell_count
    }

    /// Identify table sections (groups of lines that form a table)
    fn identify_tables(&self, lines: &[&str]) -> Vec<(usize, usize)> {
        let mut tables = Vec::new();
        let mut current_table_start: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            if self.is_in_code_block(lines, i) {
                // If we were tracking a table, end it
                if let Some(start) = current_table_start {
                    if i - start >= 2 {
                        // At least header + delimiter rows
                        tables.push((start, i - 1));
                    }
                    current_table_start = None;
                }
                continue;
            }

            let trimmed = line.trim();
            let is_table_row = trimmed.contains('|');

            // Possible table row
            if is_table_row {
                if current_table_start.is_none() {
                    current_table_start = Some(i);
                }
            } else if current_table_start.is_some() && !is_table_row && !trimmed.is_empty() {
                // End of table
                if let Some(start) = current_table_start {
                    if i - start >= 2 {
                        // At least header + delimiter rows
                        tables.push((start, i - 1));
                    }
                }
                current_table_start = None;
            }
        }

        // Handle case where table ends at EOF
        if let Some(start) = current_table_start {
            if lines.len() - start >= 2 {
                tables.push((start, lines.len() - 1));
            }
        }

        tables
    }

    /// Try to fix a table row to match the expected column count
    fn fix_table_row(&self, row: &str, expected_count: usize) -> Option<String> {
        let trimmed = row.trim();
        let current_count = self.count_cells(trimmed);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        let has_leading_pipe = trimmed.starts_with('|');
        let has_trailing_pipe = trimmed.ends_with('|');

        let parts: Vec<&str> = trimmed.split('|').collect();
        let mut cells = Vec::new();

        // Extract actual cell content
        for (i, part) in parts.iter().enumerate() {
            // Skip empty leading/trailing parts
            if (i == 0 && part.trim().is_empty() && has_leading_pipe)
                || (i == parts.len() - 1 && part.trim().is_empty() && has_trailing_pipe)
            {
                continue;
            }
            cells.push(part.trim());
        }

        // Adjust cell count to match expected count
        match current_count.cmp(&expected_count) {
            std::cmp::Ordering::Greater => {
                // Too many cells, remove excess
                cells.truncate(expected_count);
            }
            std::cmp::Ordering::Less => {
                // Too few cells, add empty ones
                while cells.len() < expected_count {
                    cells.push("");
                }
            }
            std::cmp::Ordering::Equal => {
                // Perfect number of cells, no adjustment needed
            }
        }

        // Reconstruct row
        let mut result = String::new();
        if has_leading_pipe {
            result.push('|');
        }

        for (i, cell) in cells.iter().enumerate() {
            result.push_str(&format!(" {} ", cell));
            if i < cells.len() - 1 || has_trailing_pipe {
                result.push('|');
            }
        }

        Some(result)
    }
}

impl Rule for MD056TableColumnCount {
    fn name(&self) -> &'static str {
        "MD056"
    }

    fn description(&self) -> &'static str {
        "Table column count should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let tables = self.identify_tables(&lines);

        for (table_start, table_end) in tables {
            // Find first non-empty row to determine expected column count
            let mut expected_count = 0;
            let mut found_header = false;

            for i in table_start..=table_end {
                if self.is_in_code_block(&lines, i) {
                    continue;
                }

                let count = self.count_cells(lines[i]);
                if count > 0 {
                    if !found_header {
                        expected_count = count;
                        found_header = true;
                    } else if count != expected_count {
                        let fix_result = self.fix_table_row(lines[i], expected_count);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!(
                                "Table row has {} cells, but expected {}",
                                count, expected_count
                            ),
                            line: i + 1,
                            column: 1,
                            severity: Severity::Warning,
                            fix: fix_result.map(|fixed_row| Fix {
                                range: LineIndex::new(content.to_string())
                                    .line_col_to_byte_range(i + 1, 1),
                                replacement: fixed_row,
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
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();

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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD056TableColumnCount)
    }
}
