use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
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
    /// Try to fix a table row content (with list context awareness)
    fn fix_table_row_content(
        &self,
        row_content: &str,
        expected_count: usize,
        flavor: crate::config::MarkdownFlavor,
        table_block: &crate::utils::table_utils::TableBlock,
        line_index: usize,
        original_line: &str,
    ) -> Option<String> {
        let current_count = TableUtils::count_cells_with_flavor(row_content, flavor);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        let fixed = self.fix_row_by_truncation(row_content, expected_count, flavor)?;
        Some(self.restore_prefixes(&fixed, table_block, line_index, original_line))
    }

    /// Restore list/blockquote prefixes to a fixed row
    fn restore_prefixes(
        &self,
        fixed_content: &str,
        table_block: &crate::utils::table_utils::TableBlock,
        line_index: usize,
        original_line: &str,
    ) -> String {
        // Extract blockquote prefix from original
        let (blockquote_prefix, _) = TableUtils::extract_blockquote_prefix(original_line);

        // Handle list context
        if let Some(ref list_ctx) = table_block.list_context {
            if line_index == 0 {
                // Header line: use list prefix
                format!("{blockquote_prefix}{}{fixed_content}", list_ctx.list_prefix)
            } else {
                // Continuation lines: use indentation
                let indent = " ".repeat(list_ctx.content_indent);
                format!("{blockquote_prefix}{indent}{fixed_content}")
            }
        } else {
            // No list context, just blockquote
            if blockquote_prefix.is_empty() {
                fixed_content.to_string()
            } else {
                format!("{blockquote_prefix}{fixed_content}")
            }
        }
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

        // Delegate to shared cell splitting (returns only cell contents, no empty leading/trailing parts)
        let cells = TableUtils::split_table_row_with_flavor(trimmed, flavor);
        let mut cell_contents: Vec<&str> = cells.iter().map(|c| c.trim()).collect();

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
}

impl Rule for MD056TableColumnCount {
    fn name(&self) -> &'static str {
        "MD056"
    }

    fn description(&self) -> &'static str {
        "Table column count should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Table
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

        let lines = ctx.raw_lines();

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        for table_block in table_blocks {
            // Collect all table lines for building the whole-table fix
            let all_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            // Determine expected column count from header row (strip list/blockquote prefix first)
            let header_content = TableUtils::extract_table_row_content(lines[table_block.header_line], table_block, 0);
            let expected_count = TableUtils::count_cells_with_flavor(header_content, flavor);

            if expected_count == 0 {
                continue; // Skip invalid tables
            }

            // Build the whole-table fix once for all warnings in this table
            // This ensures that applying Quick Fix on any row fixes the entire table
            let table_start_line = table_block.start_line + 1; // Convert to 1-indexed
            let table_end_line = table_block.end_line + 1; // Convert to 1-indexed

            // Build the complete fixed table content
            let mut fixed_table_lines: Vec<String> = Vec::with_capacity(all_line_indices.len());
            for (i, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                let row_content = TableUtils::extract_table_row_content(line, table_block, i);
                let fixed_line = self
                    .fix_table_row_content(row_content, expected_count, flavor, table_block, i, line)
                    .unwrap_or_else(|| line.to_string());
                if line_idx < lines.len() - 1 {
                    fixed_table_lines.push(format!("{fixed_line}\n"));
                } else {
                    fixed_table_lines.push(fixed_line);
                }
            }
            let mut table_replacement = fixed_table_lines.concat();
            let table_range = ctx.line_index.multi_line_range(table_start_line, table_end_line);

            // Preserve trailing newline: if the original range ends with '\n' but
            // the replacement does not, append '\n' so apply_warning_fixes does not
            // strip it when the table is at the end of the document.
            if content[table_range.clone()].ends_with('\n') && !table_replacement.ends_with('\n') {
                table_replacement.push('\n');
            }

            // Check all rows in the table
            for (i, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                let row_content = TableUtils::extract_table_row_content(line, table_block, i);
                let count = TableUtils::count_cells_with_flavor(row_content, flavor);

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

        // Double backslash + pipe inside code span: pipe is still masked by code span
        let content_double = "| Command | Description |
|---------|-------------|
| `echo \\\\| grep` | Pipe example |
| `ls` | List files |";
        let ctx2 = LintContext::new(content_double, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        // The \\| is inside backticks, so the pipe is content, not a delimiter
        assert_eq!(result2.len(), 0, "pipes inside code spans should not split cells");
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

    #[test]
    fn test_blockquote_table_column_mismatch() {
        let rule = MD056TableColumnCount;
        let content = "> | Header 1 | Header 2 | Header 3 |
> |----------|----------|----------|
> | Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("has 2 cells, but expected 3"));
    }

    #[test]
    fn test_fix_blockquote_table_preserves_prefix() {
        let rule = MD056TableColumnCount;
        let content = "> | Header 1 | Header 2 | Header 3 |
> |----------|----------|----------|
> | Cell 1   | Cell 2   |
> | Cell 4   | Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Each line should still start with "> "
        for line in fixed.lines() {
            assert!(line.starts_with("> "), "Line should preserve blockquote prefix: {line}");
        }
        // The fixed row should have 3 cells
        assert!(fixed.contains("> | Cell 1 | Cell 2 |  |"));
    }

    #[test]
    fn test_fix_nested_blockquote_table() {
        let rule = MD056TableColumnCount;
        let content = ">> | A | B | C |
>> |---|---|---|
>> | 1 | 2 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Each line should preserve the nested blockquote prefix
        for line in fixed.lines() {
            assert!(
                line.starts_with(">> "),
                "Line should preserve nested blockquote prefix: {line}"
            );
        }
        assert!(fixed.contains(">> | 1 | 2 |  |"));
    }

    #[test]
    fn test_blockquote_table_too_many_columns() {
        let rule = MD056TableColumnCount;
        let content = "> | A | B |
> |---|---|
> | 1 | 2 | 3 | 4 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve blockquote prefix while truncating columns
        assert!(fixed.lines().nth(2).unwrap().starts_with("> "));
        assert!(fixed.contains("> | 1 | 2 |"));
        assert!(!fixed.contains("| 3 |"));
    }

    // === Roundtrip safety tests ===

    fn assert_fix_roundtrip(content: &str) {
        let rule = MD056TableColumnCount;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx2).unwrap();
        assert!(
            remaining.is_empty(),
            "After fix(), check() should find 0 violations.\nOriginal: {content:?}\nFixed: {fixed:?}\nRemaining: {remaining:?}"
        );
    }

    #[test]
    fn test_roundtrip_too_few_columns() {
        assert_fix_roundtrip("| A | B | C |\n|---|---|---|\n| 1 | 2 |");
    }

    #[test]
    fn test_roundtrip_too_many_columns() {
        assert_fix_roundtrip("| A | B |\n|---|---|\n| 1 | 2 | 3 | 4 |");
    }

    #[test]
    fn test_roundtrip_with_trailing_newline() {
        assert_fix_roundtrip("| A | B | C |\n|---|---|---|\n| 1 | 2 |\n");
    }

    #[test]
    fn test_roundtrip_blockquote_table() {
        assert_fix_roundtrip("> | A | B | C |\n> |---|---|---|\n> | 1 | 2 |");
    }

    #[test]
    fn test_roundtrip_clean_table() {
        assert_fix_roundtrip("| A | B |\n|---|---|\n| 1 | 2 |");
    }

    #[test]
    fn test_roundtrip_multiple_tables() {
        assert_fix_roundtrip("| A | B |\n|---|---|\n| 1 | 2 |\n\nText\n\n| C | D | E |\n|---|---|---|\n| 3 | 4 |");
    }
}
