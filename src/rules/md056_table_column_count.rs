use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::{LineIndex, calculate_line_range};
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
    fn fix_table_row(&self, row: &str, expected_count: usize) -> Option<String> {
        let current_count = TableUtils::count_cells(row);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        let trimmed = row.trim();
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
            result.push_str(&format!(" {cell} "));
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

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();

        // Use shared table detection for better performance
        let table_blocks = TableUtils::find_table_blocks(content, ctx);

        for table_block in table_blocks {
            // Determine expected column count from header row
            let expected_count = TableUtils::count_cells(lines[table_block.header_line]);

            if expected_count == 0 {
                continue; // Skip invalid tables
            }

            // Check all rows in the table
            let all_lines = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied());

            for line_idx in all_lines {
                let line = lines[line_idx];
                let count = TableUtils::count_cells(line);

                if count > 0 && count != expected_count {
                    let fix_result = self.fix_table_row(line, expected_count);

                    // Calculate precise character range for the entire table row
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!("Table row has {count} cells, but expected {expected_count}"),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: fix_result.map(|fixed_row| Fix {
                            range: LineIndex::new(content.to_string()).line_col_to_byte_range(line_idx + 1, 1),
                            replacement: fixed_row,
                        }),
                    });
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 9);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    #[ignore = "Table utils doesn't handle escaped pipes in code correctly yet"]
    fn test_table_with_escaped_pipes() {
        let rule = MD056TableColumnCount;
        let content = "| Command | Description |
|---------|-------------|
| `echo \\| grep` | Pipe example |
| `ls` | List files |";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should handle escaped pipes correctly
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD056TableColumnCount;
        let content = "";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(!fixed.ends_with('\n'));
        assert!(fixed.contains("| 1 | 2 |  |"));
    }
}
