use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use crate::utils::table_utils::TableUtils;

/// Rule MD058: Blanks around tables
///
/// See [docs/md058.md](../../docs/md058.md) for full documentation, configuration, and examples.
///
/// Ensures tables have blank lines before and after them

#[derive(Clone)]
pub struct MD058BlanksAroundTables;

impl Default for MD058BlanksAroundTables {
    fn default() -> Self {
        MD058BlanksAroundTables
    }
}

impl MD058BlanksAroundTables {
    /// Check if a line is blank
    fn is_blank_line(&self, line: &str) -> bool {
        line.trim().is_empty()
    }
}

impl Rule for MD058BlanksAroundTables {
    fn name(&self) -> &'static str {
        "MD058"
    }

    fn description(&self) -> &'static str {
        "Tables should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();

        // Use shared table detection for better performance
        let table_blocks = TableUtils::find_table_blocks(content, ctx);

        for table_block in table_blocks {
            // Check for blank line before table
            if table_block.start_line > 0 && !self.is_blank_line(lines[table_block.start_line - 1]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line before table".to_string(),
                    line: table_block.start_line + 1,
                    column: 1,
                    end_line: table_block.start_line + 1,
                    end_column: 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(table_block.start_line + 1, 1),
                        replacement: format!("\n{}", lines[table_block.start_line]),
                    }),
                });
            }

            // Check for blank line after table
            if table_block.end_line < lines.len() - 1 && !self.is_blank_line(lines[table_block.end_line + 1]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line after table".to_string(),
                    line: table_block.end_line + 1,
                    column: lines[table_block.end_line].len() + 1,
                    end_line: table_block.end_line + 1,
                    end_column: lines[table_block.end_line].len() + 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index
                            .line_col_to_byte_range(table_block.end_line + 1, lines[table_block.end_line].len() + 1),
                        replacement: format!("{}\n", lines[table_block.end_line]),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let warning_before = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message == "Missing blank line before table");

            if let Some(idx) = warning_before {
                result.push("".to_string());
                warnings.remove(idx);
            }

            result.push(lines[i].to_string());

            let warning_after = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message == "Missing blank line after table");

            if let Some(idx) = warning_after {
                result.push("".to_string());
                warnings.remove(idx);
            }

            i += 1;
        }

        Ok(result.join("\n"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD058BlanksAroundTables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_table_with_blanks() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text before.

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

Some text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_missing_blank_before() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text before.
| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

Some text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Missing blank line before table"));
    }

    #[test]
    fn test_table_missing_blank_after() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text before.

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
Some text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
        assert!(result[0].message.contains("Missing blank line after table"));
    }

    #[test]
    fn test_table_missing_both_blanks() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text before.
| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
Some text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Missing blank line before table"));
        assert!(result[1].message.contains("Missing blank line after table"));
    }

    #[test]
    fn test_table_at_start_of_document() {
        let rule = MD058BlanksAroundTables;
        let content = "| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

Some text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // No blank line needed before table at start of document
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_at_end_of_document() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text before.

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // No blank line needed after table at end of document
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_tables() {
        let rule = MD058BlanksAroundTables;
        let content = "Text before first table.
| Col 1 | Col 2 |
|--------|-------|
| Data 1 | Val 1 |
Text between tables.
| Col A | Col B |
|--------|-------|
| Data 2 | Val 2 |
Text after second table.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4);
        // First table missing blanks
        assert!(result[0].message.contains("Missing blank line before table"));
        assert!(result[1].message.contains("Missing blank line after table"));
        // Second table missing blanks
        assert!(result[2].message.contains("Missing blank line before table"));
        assert!(result[3].message.contains("Missing blank line after table"));
    }

    #[test]
    fn test_consecutive_tables() {
        let rule = MD058BlanksAroundTables;
        let content = "Some text.

| Col 1 | Col 2 |
|--------|-------|
| Data 1 | Val 1 |

| Col A | Col B |
|--------|-------|
| Data 2 | Val 2 |

More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Tables separated by blank line should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consecutive_tables_no_blank() {
        let rule = MD058BlanksAroundTables;
        // Add a non-table line between tables to force detection as separate tables
        let content = "Some text.

| Col 1 | Col 2 |
|--------|-------|
| Data 1 | Val 1 |
Text between.
| Col A | Col B |
|--------|-------|
| Data 2 | Val 2 |

More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should flag missing blanks around both tables
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Missing blank line after table"));
        assert!(result[1].message.contains("Missing blank line before table"));
    }

    #[test]
    fn test_fix_missing_blanks() {
        let rule = MD058BlanksAroundTables;
        let content = "Text before.
| Header | Col 2 |
|--------|-------|
| Cell   | Data  |
Text after.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Text before.

| Header | Col 2 |
|--------|-------|
| Cell   | Data  |

Text after.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_multiple_tables() {
        let rule = MD058BlanksAroundTables;
        let content = "Start
| T1 | C1 |
|----|----|
| D1 | V1 |
Middle
| T2 | C2 |
|----|----|
| D2 | V2 |
End";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Start

| T1 | C1 |
|----|----|
| D1 | V1 |

Middle

| T2 | C2 |
|----|----|
| D2 | V2 |

End";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD058BlanksAroundTables;
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_tables() {
        let rule = MD058BlanksAroundTables;
        let content = "Just regular text.
No tables here.
Only paragraphs.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_block_with_table() {
        let rule = MD058BlanksAroundTables;
        let content = "Text before.
```
| Not | A | Table |
|-----|---|-------|
| In  | Code | Block |
```
Text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Tables in code blocks should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_with_complex_content() {
        let rule = MD058BlanksAroundTables;
        let content = "# Heading
| Column 1 | Column 2 | Column 3 |
|:---------|:--------:|---------:|
| Left     | Center   | Right    |
| Data     | More     | Info     |
## Another Heading";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Missing blank line before table"));
        assert!(result[1].message.contains("Missing blank line after table"));
    }

    #[test]
    fn test_table_with_empty_cells() {
        let rule = MD058BlanksAroundTables;
        let content = "Text.

|     |     |     |
|-----|-----|-----|
|     | X   |     |
| O   |     | X   |

More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_with_unicode() {
        let rule = MD058BlanksAroundTables;
        let content = "Unicode test.
| 名前 | 年齢 | 都市 |
|------|------|------|
| 田中 | 25   | 東京 |
| 佐藤 | 30   | 大阪 |
End.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_table_with_long_cells() {
        let rule = MD058BlanksAroundTables;
        let content = "Before.

| Short | Very very very very very very very very long header |
|-------|-----------------------------------------------------|
| Data  | This is an extremely long cell content that goes on |

After.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_without_content_rows() {
        let rule = MD058BlanksAroundTables;
        let content = "Text.
| Header 1 | Header 2 |
|----------|----------|
Next paragraph.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should still require blanks around header-only table
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_indented_table() {
        let rule = MD058BlanksAroundTables;
        let content = "List item:
    
    | Indented | Table |
    |----------|-------|
    | Data     | Here  |
    
    More content.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Indented tables should be detected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_single_column_table_not_detected() {
        let rule = MD058BlanksAroundTables;
        let content = "Text before.
| Single |
|--------|
| Column |
Text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Single column tables are not detected by table_utils (requires 2+ columns)
        assert_eq!(result.len(), 0);
    }
}
