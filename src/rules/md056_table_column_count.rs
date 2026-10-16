use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
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
        expected_count: usize,
        flavor: crate::config::MarkdownFlavor,
        table_block: &crate::utils::table_utils::TableBlock,
        line_index: usize,
        original_line: &str,
        header: &str,
    ) -> Option<String> {
        let row_content = TableUtils::extract_table_row_content(original_line, table_block, line_index);
        let current_count = TableUtils::count_cells_with_flavor(row_content, flavor);

        if current_count == expected_count || current_count == 0 {
            return None;
        }

        let is_delimiter = line_index == 1;
        let fixed = self.fix_row_cell_count(row_content, expected_count, is_delimiter, flavor)?;
        Some(self.restore_prefixes(&fixed, table_block, line_index, original_line, header))
    }

    /// Restore list/blockquote prefixes to a fixed row
    fn restore_prefixes(
        &self,
        fixed_content: &str,
        table_block: &crate::utils::table_utils::TableBlock,
        line_index: usize,
        original_line: &str,
        header: &str,
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
            // No list context: the row keeps the quote and list markers it was written after
            let prefix = TableUtils::row_prefix(original_line, header, table_block, line_index);
            format!("{prefix}{fixed_content}")
        }
    }

    /// Fix a table row's cell count without losing text.
    ///
    /// A short row is padded: empty cells for a body row (what GFM renders for
    /// missing cells anyway) and `---` for the delimiter row, where an empty
    /// cell is not a valid delimiter. A long row is cut back only when every
    /// extra cell is empty or the row is the delimiter row, whose cells hold
    /// no text. Otherwise there is no lossless fix, since only the author
    /// knows whether the header lacks a column or the row has a stray pipe,
    /// and this returns `None`.
    fn fix_row_cell_count(
        &self,
        row: &str,
        expected_count: usize,
        is_delimiter: bool,
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

        match current_count.cmp(&expected_count) {
            std::cmp::Ordering::Greater => {
                let extra = cell_contents.get(expected_count..).unwrap_or_default();
                if !is_delimiter && extra.iter().any(|cell| !cell.is_empty()) {
                    return None;
                }
                cell_contents.truncate(expected_count);
            }
            std::cmp::Ordering::Less => {
                let padding = if is_delimiter { "---" } else { "" };
                cell_contents.resize(expected_count, padding);
            }
            std::cmp::Ordering::Equal => {}
        }

        // An empty last cell exists only before a closing pipe: without one it
        // is trailing whitespace, and the row reads a cell short again.
        let close_row = has_trailing_pipe || cell_contents.last().is_some_and(|cell| cell.is_empty());

        // Reconstruct row
        let mut result = String::new();
        if has_leading_pipe {
            result.push('|');
        }

        for (i, cell) in cell_contents.iter().enumerate() {
            result.push_str(&format!(" {cell} "));
            if i < cell_contents.len() - 1 || close_row {
                result.push('|');
            }
        }

        Some(result)
    }
}

impl Rule for MD056TableColumnCount {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(56))
    }

    fn name(&self) -> &'static str {
        "MD056"
    }

    fn description(&self) -> &'static str {
        "Table column count should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Table
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::ConditionallyFixable
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

        for table_block in table_blocks
            .iter()
            .filter(|block| block.parser_confirmed || block.repairable)
        {
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

            // Check each row and emit a per-row fix. Per-row fixes ensure that
            // inline-disabling one row does not cause the fix on another row to
            // overwrite the disabled row's content.
            for (i, &line_idx) in all_line_indices.iter().enumerate() {
                let line = lines[line_idx];
                let row_content = TableUtils::extract_table_row_content(line, table_block, i);
                let count = TableUtils::count_cells_with_flavor(row_content, flavor);

                if count > 0 && count != expected_count {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, line);

                    // Build a per-row fix so inline-disabled rows are not
                    // overwritten by fixes on other rows in the same table.
                    // A row with no lossless fix is reported without one.
                    let fix = self
                        .fix_table_row_content(
                            expected_count,
                            flavor,
                            table_block,
                            i,
                            line,
                            lines[table_block.header_line],
                        )
                        .map(|fixed_line| {
                            let row_range =
                                ctx.line_column_byte_range_with_length(line_idx + 1, 1, line.chars().count());
                            Fix::new(row_range, fixed_line)
                        });

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!("Table row has {count} cells, but expected {expected_count}"),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
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

    /// Extra cells holding text are reported but never removed: only the
    /// author knows whether the header lacks a column or the row has a stray
    /// pipe, and removing them would delete the text.
    #[test]
    fn test_fix_keeps_extra_cells_with_content() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   | Cell 3   | Cell 4   |
| Cell 5   | Cell 6   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].fix.is_none(),
            "no fix may be offered: {:?}",
            warnings[0].fix
        );

        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_fix_keeps_extra_cell_from_issue_940() {
        let rule = MD056TableColumnCount;
        let content = "# T\n\n| A | B | C |\n| - | - | - |\n| one | two | three | four |\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    /// One non-empty cell anywhere past the header's width blocks the fix,
    /// including when empty cells sit between it and the last header column.
    #[test]
    fn test_fix_keeps_row_when_any_extra_cell_has_content() {
        let rule = MD056TableColumnCount;
        for content in [
            "| A | B |\n|---|---|\n| 1 | 2 |  | x |\n",
            "| A | B |\n|---|---|\n| 1 | 2 | x |  |\n",
            "| A | B |\n|---|---|\n| 1 | 2 | `x` |\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content:?}");
            assert!(warnings[0].fix.is_none(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }
    }

    /// Empty extra cells hold no text, so removing them loses nothing.
    #[test]
    fn test_fix_removes_empty_extra_cells() {
        let rule = MD056TableColumnCount;
        for (content, expected) in [
            (
                "| A | B |\n|---|---|\n| 1 | 2 |  |\n",
                "| A | B |\n|---|---|\n| 1 | 2 |\n",
            ),
            (
                "| A | B |\n|---|---|\n| 1 | 2 | |   |\n",
                "| A | B |\n|---|---|\n| 1 | 2 |\n",
            ),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
        }
    }

    /// A delimiter row's cells hold no text, so extra ones are removed and
    /// missing ones are added as `---`. An empty cell is not a delimiter
    /// cell, and padding with one leaves a row that is not a table.
    #[test]
    fn test_fix_delimiter_row() {
        let rule = MD056TableColumnCount;
        for (content, expected) in [
            (
                "| A | B | C |\n|---|---|\n| 1 | 2 | 3 |\n",
                "| A | B | C |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n",
            ),
            (
                "| A | B |\n|:--|--:|---|\n| 1 | 2 |\n",
                "| A | B |\n| :-- | --: |\n| 1 | 2 |\n",
            ),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected, "{content:?}");
            let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{fixed:?}");
        }
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

    /// Padding a row that has no closing pipe adds one, since an empty last
    /// cell without it is only trailing whitespace; a second fix pass then
    /// finds nothing left to do.
    #[test]
    fn padding_a_row_without_a_closing_pipe_closes_it() {
        let rule = MD056TableColumnCount;
        for (content, expected) in [
            ("a | b | c\n--|--|--\nx | y\n", "a | b | c\n--|--|--\n x | y |  |\n"),
            ("|  |  |\n| --- | --- |\n`|`\n", "|  |  |\n| --- | --- |\n `|` |  |\n"),
            (
                "| a | b | c |\n|--|--|--|\n| x | y |\n",
                "| a | b | c |\n|--|--|--|\n| x | y |  |\n",
            ),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected, "{content:?}");
            let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), fixed, "second pass over {fixed:?}");
        }
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
        // Removing an empty extra cell keeps a closed row closed and an open
        // delimiter row open
        let content = "| Header 1 | Header 2
|----------|----------|---
| Cell 1   | Cell 2   |  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[1], "| ---------- | ---------- ");
        assert_eq!(lines[2], "| Cell 1 | Cell 2 |");
    }

    /// A row with no closing pipe whose extra cell holds text is kept as is.
    #[test]
    fn test_fix_keeps_open_row_with_extra_content() {
        let rule = MD056TableColumnCount;
        let content = "| Header 1 | Header 2
|----------|----------
| Cell 1   | Cell 2   | Cell 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
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
> | 1 | 2 | 3 | 4 |
> | 5 | 6 |  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The row with text in its extra cells is kept; the empty extra cell
        // is removed with the blockquote prefix preserved
        assert_eq!(fixed, "> | A | B |\n> |---|---|\n> | 1 | 2 | 3 | 4 |\n> | 5 | 6 |");
    }

    #[test]
    fn test_list_table_keeps_extra_cells_with_content() {
        let rule = MD056TableColumnCount;
        let content = "- item\n\n  | A | B |\n  |---|---|\n  | 1 | 2 | 3 |\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_fix_capability() {
        assert_eq!(
            MD056TableColumnCount.fix_capability(),
            FixCapability::ConditionallyFixable
        );
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
    fn test_roundtrip_empty_extra_columns() {
        assert_fix_roundtrip("| A | B |\n|---|---|\n| 1 | 2 |  |  |");
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

    // === Pandoc construct reachability tests ===
    //
    // These tests document that MD056 does not flag Pandoc-specific constructs
    // because `ctx.table_blocks` excludes them at the source:
    //
    // - Grid table delimiters use `+---+---+` (no `|`), so `is_delimiter_row`
    //   returns false and no `TableBlock` is created.
    // - Multi-line table separators have no `|`, same exclusion.
    // - Line blocks (`| First line`) end without `|`; `is_potential_table_row`
    //   requires `valid_parts >= 2` for non-outer-piped lines (only 1 found).
    // - Pipe-table captions (`: caption`) have no `|` — excluded.
    //
    // No production guard is needed. If `find_table_blocks` ever changes to
    // include these constructs, these tests will surface that.

    #[test]
    fn md056_pandoc_grid_tables_not_flagged() {
        let rule = MD056TableColumnCount;
        let content = "\
+---+---+
| a | b |
+===+===+
| 1 | 2 |
+---+---+
";
        // Grid table delimiters (`+===+===+`) contain no `|`, so `is_delimiter_row`
        // returns false and no TableBlock is created — no MD056 check runs.
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD056 should not flag Pandoc grid tables (excluded by table_blocks): {result:?}"
        );

        // Standard flavor: same content produces no warnings for the same reason.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD056 should not flag grid-table-like content under Standard either: {result_std:?}"
        );
    }

    #[test]
    fn md056_pandoc_multi_line_tables_not_flagged() {
        let rule = MD056TableColumnCount;
        let content = "\
--------- ----------- ------
Header 1   Header 2   Header 3
--------- ----------- ------
Cell 1     Cell 2     Cell 3
--------- ----------- ------
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD056 should not flag Pandoc multi-line tables: {result:?}"
        );

        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD056 should not flag multi-line table content under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md056_pandoc_line_blocks_not_flagged() {
        let rule = MD056TableColumnCount;
        // Pandoc line blocks: starts with `|` but no trailing `|`.
        // is_potential_table_row excludes them (valid_parts < 2).
        let content = "| First line\n| Second line\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD056 should not treat Pandoc line blocks as tables: {result:?}"
        );

        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD056 should not treat line-block-like content as tables under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md056_pandoc_pipe_table_captions_not_flagged() {
        let rule = MD056TableColumnCount;
        // Pipe-table captions (`: caption`) have no `|` — excluded from table_blocks.
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b  |

: My table caption
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD056 should not flag the pipe-table caption line: {result:?}"
        );

        // Under Standard: caption line is ignored; valid table has no warnings.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD056 already-valid table with caption should have no warnings under Standard: {result_std:?}"
        );
    }
}
