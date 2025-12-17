use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::table_utils::TableUtils;

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
    /// Try to fix a table row to match the expected column count
    fn fix_table_row(&self, row: &str, expected_count: usize, flavor: crate::config::MarkdownFlavor) -> Option<String> {
        let current_count = TableUtils::count_cells_with_flavor(row, flavor);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        // For standard flavor with too many cells, first try escaping pipes in inline code.
        // This preserves content and produces valid GitHub-compatible output.
        if flavor == crate::config::MarkdownFlavor::Standard && current_count > expected_count {
            let escaped_row = TableUtils::escape_pipes_in_inline_code(row);
            let escaped_count = TableUtils::count_cells_with_flavor(&escaped_row, flavor);

            // If escaping pipes in inline code fixes the cell count, use that
            if escaped_count == expected_count {
                return Some(escaped_row.trim().to_string());
            }

            // If escaping reduced cell count, continue fixing with escaped version
            if escaped_count < current_count {
                return self.fix_row_by_truncation(&escaped_row, expected_count, flavor);
            }
        }

        self.fix_row_by_truncation(row, expected_count, flavor)
    }

    /// Fix a table row by truncating or adding cells
    fn fix_row_by_truncation(
        &self,
        row: &str,
        expected_count: usize,
        flavor: crate::config::MarkdownFlavor,
    ) -> Option<String> {
        let current_count = TableUtils::count_cells_with_flavor(row, flavor);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        let trimmed = row.trim();
        let has_leading_pipe = trimmed.starts_with('|');
        let has_trailing_pipe = trimmed.ends_with('|');

        // Use flavor-aware cell splitting
        let cells = Self::split_row_into_cells(trimmed, flavor);

        let mut cell_contents: Vec<&str> = Vec::new();
        for (i, cell) in cells.iter().enumerate() {
            // Skip empty leading/trailing parts
            if (i == 0 && cell.trim().is_empty() && has_leading_pipe)
                || (i == cells.len() - 1 && cell.trim().is_empty() && has_trailing_pipe)
            {
                continue;
            }
            cell_contents.push(cell.trim());
        }

        // Adjust cell count to match expected count
        match current_count.cmp(&expected_count) {
            std::cmp::Ordering::Greater => {
                // Too many cells, remove excess
                cell_contents.truncate(expected_count);
            }
            std::cmp::Ordering::Less => {
                // Too few cells, add empty ones
                while cell_contents.len() < expected_count {
                    cell_contents.push("");
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

        for (i, cell) in cell_contents.iter().enumerate() {
            result.push_str(&format!(" {cell} "));
            if i < cell_contents.len() - 1 || has_trailing_pipe {
                result.push('|');
            }
        }

        Some(result)
    }

    /// Split a table row into cells, respecting flavor-specific behavior
    ///
    /// For Standard/GFM flavor, pipes in inline code ARE cell delimiters.
    /// For MkDocs flavor, pipes in inline code are NOT cell delimiters.
    fn split_row_into_cells(row: &str, flavor: crate::config::MarkdownFlavor) -> Vec<String> {
        // First, mask escaped pipes (same for all flavors)
        let masked = TableUtils::mask_pipes_for_table_parsing(row);

        // For MkDocs flavor, also mask pipes inside inline code
        let final_masked = if flavor == crate::config::MarkdownFlavor::MkDocs {
            TableUtils::mask_pipes_in_inline_code(&masked)
        } else {
            masked
        };

        // Split by pipes on the masked string, then extract corresponding
        // original content from the unmasked row
        let masked_parts: Vec<&str> = final_masked.split('|').collect();
        let mut cells = Vec::new();
        let mut pos = 0;

        for masked_part in masked_parts {
            let cell_len = masked_part.len();
            if pos + cell_len <= row.len() {
                cells.push(row[pos..pos + cell_len].to_string());
            } else {
                cells.push(masked_part.to_string());
            }
            pos += cell_len + 1; // +1 for the pipe delimiter
        }

        cells
    }
}

impl Rule for MD056TableColumnCount {
    fn name(&self) -> &'static str {
        "MD056"
    }

    fn description(&self) -> &'static str {
        "Table column count should be consistent"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no tables present
        !ctx.likely_has_tables()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let flavor = ctx.flavor;
        let mut warnings = Vec::new();

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        for table_block in table_blocks {
            // Determine expected column count from header row
            let expected_count = TableUtils::count_cells_with_flavor(lines[table_block.header_line], flavor);

            if expected_count == 0 {
                continue; // Skip invalid tables
            }

            // Collect all table lines for building the whole-table fix
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            // Build the whole-table fix once for all warnings in this table
            // This ensures that applying Quick Fix on any row fixes the entire table
            let table_start_line = table_block.start_line + 1; // Convert to 1-indexed
            let table_end_line = table_block.end_line + 1; // Convert to 1-indexed

            // Build the complete fixed table content
            let mut fixed_table_lines: Vec<String> = Vec::with_capacity(all_line_indices.len());
            for &line_idx in &all_line_indices {
                let line = lines[line_idx];
                let fixed_line = self
                    .fix_table_row(line, expected_count, flavor)
                    .unwrap_or_else(|| line.to_string());
                if line_idx < lines.len() - 1 {
                    fixed_table_lines.push(format!("{fixed_line}\n"));
                } else {
                    fixed_table_lines.push(fixed_line);
                }
            }
            let table_replacement = fixed_table_lines.concat();
            let table_range = ctx.line_index.multi_line_range(table_start_line, table_end_line);

            // Check all rows in the table
            for &line_idx in &all_line_indices {
                let line = lines[line_idx];
                let count = TableUtils::count_cells_with_flavor(line, flavor);

                if count > 0 && count != expected_count {
                    // Calculate precise character range for the entire table row
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, line);

                    // Each warning uses the same whole-table fix
                    // This ensures Quick Fix on any row fixes the entire table
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!("Table row has {count} cells, but expected {expected_count}"),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: table_range.clone(),
                            replacement: table_replacement.clone(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let flavor = ctx.flavor;
        let lines: Vec<&str> = content.lines().collect();
        let table_blocks = &ctx.table_blocks;

        let mut result_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();

        for table_block in table_blocks {
            // Determine expected column count from header row
            let expected_count = TableUtils::count_cells_with_flavor(lines[table_block.header_line], flavor);

            if expected_count == 0 {
                continue; // Skip invalid tables
            }

            // Fix all rows in the table
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            for &line_idx in &all_line_indices {
                let line = lines[line_idx];
                if let Some(fixed_line) = self.fix_table_row(line, expected_count, flavor) {
                    result_lines[line_idx] = fixed_line;
                }
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

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD056TableColumnCount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_table() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_too_few_columns() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   |
| Cell 4   | Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    fn test_too_many_columns() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   | Cell 3   | Cell 4   |
| Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("has 4 cells, but expected 2"));
    }

    #[test]
    fn test_delimiter_row_mismatch() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3 |
|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    fn test_fix_too_few_columns() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   |
| Cell 4   | Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("| Cell 1 | Cell 2 |  |"));
    }

    #[test]
    fn test_fix_too_many_columns() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   | Cell 3   | Cell 4   |
| Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("| Cell 1 | Cell 2 |"));
        assert!(!fixed.contains("Cell 3"));
        assert!(!fixed.contains("Cell 4"));
    }

    #[test]
    fn test_no_leading_pipe() {
        let rule = MD056TableColumnCount;
        let content = "Header 1 | Header 2 | Header 3 |
---------|----------|----------|
Cell 1   | Cell 2   |
Cell 4   | Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_no_trailing_pipe() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3
|----------|----------|----------
| Cell 1   | Cell 2
| Cell 4   | Cell 5   | Cell 6";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_no_pipes_at_all() {
        let rule = MD056TableColumnCount;
        let content = "This is not a table
Just regular text
No pipes here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_cells() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
|          |          |          |
| Cell 1   |          | Cell 3   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_tables() {
        let rule = MD056TableColumnCount;
        let content = "| Table 1 Col 1 | Table 1 Col 2 |
|----------------|----------------|
| Data 1         | Data 2         |

Some text in between.

| Table 2 Col 1 | Table 2 Col 2 | Table 2 Col 3 |
|----------------|----------------|----------------|
| Data 3         | Data 4         |
| Data 5         | Data 6         | Data 7         |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 9);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    fn test_table_with_escaped_pipes() {
        let rule = MD056TableColumnCount;

        // Single backslash escapes the pipe: \| keeps pipe as content (2 columns)
        let content = "| Command | Description |
|---------|-------------|
| `echo \\| grep` | Pipe example |
| `ls` | List files |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "escaped pipe \\| should not split cells");

        // Double backslash + pipe: \\| means escaped backslash + pipe delimiter (3 columns)
        let content_double = "| Command | Description |
|---------|-------------|
| `echo \\\\| grep` | Pipe example |
| `ls` | List files |";
        let ctx2 = LintContext::new(content_double, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        // Line 3 has \\| which becomes 3 cells, but header expects 2
        assert_eq!(result2.len(), 1, "double backslash \\\\| should split cells");
    }

    #[test]
    fn test_empty_content() {
        let rule = MD056TableColumnCount;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_block_with_table() {
        let rule = MD056TableColumnCount;
        let content = "```
| This | Is | Code |
|------|----|----|
| Not  | A  | Table |
```

| Real | Table |
|------|-------|
| Data | Here  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not check tables inside code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_preserves_pipe_style() {
        let rule = MD056TableColumnCount;
        // Test with no trailing pipes
        let content = "| Header 1 | Header 2 | Header 3
|----------|----------|----------
| Cell 1   | Cell 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        assert!(!lines[2].ends_with('|'));
        assert!(lines[2].contains("Cell 1"));
        assert!(lines[2].contains("Cell 2"));
    }

    #[test]
    fn test_single_column_table() {
        let rule = MD056TableColumnCount;
        let content = "| Header |
|---------|
| Cell 1  |
| Cell 2  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_complex_delimiter_row() {
        let rule = MD056TableColumnCount;
        let content = "| Left | Center | Right |
|:-----|:------:|------:|
| L    | C      | R     |
| Left | Center |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_unicode_content() {
        let rule = MD056TableColumnCount;
        let content = "| 名前 | 年齢 | 都市 |
|------|------|------|
| 田中 | 25   | 東京 |
| 佐藤 | 30   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_very_long_cells() {
        let rule = MD056TableColumnCount;
        let content = "| Short | Very very very very very very very very very very long header | Another |
|-------|--------------------------------------------------------------|---------|
| Data  | This is an extremely long cell content that goes on and on   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    fn test_fix_with_newline_ending() {
        let rule = MD056TableColumnCount;
        let content = "| A | B | C |
|---|---|---|
| 1 | 2 |
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.ends_with('\n'));
        assert!(fixed.contains("| 1 | 2 |  |"));
    }

    #[test]
    fn test_fix_without_newline_ending() {
        let rule = MD056TableColumnCount;
        let content = "| A | B | C |
|---|---|---|
| 1 | 2 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(!fixed.ends_with('\n'));
        assert!(fixed.contains("| 1 | 2 |  |"));
    }
}
