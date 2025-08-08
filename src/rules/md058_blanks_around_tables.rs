use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::kramdown_utils::is_kramdown_block_attribute;
use crate::utils::range_utils::LineIndex;
use crate::utils::table_utils::TableUtils;
use serde::{Deserialize, Serialize};

/// Rule MD058: Blanks around tables
///
/// See [docs/md058.md](../../docs/md058.md) for full documentation, configuration, and examples.
///
/// Ensures tables have blank lines before and after them
///
/// Configuration for MD058 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD058Config {
    /// Minimum number of blank lines before tables
    #[serde(default = "default_minimum_before")]
    pub minimum_before: usize,
    /// Minimum number of blank lines after tables
    #[serde(default = "default_minimum_after")]
    pub minimum_after: usize,
}

impl Default for MD058Config {
    fn default() -> Self {
        Self {
            minimum_before: default_minimum_before(),
            minimum_after: default_minimum_after(),
        }
    }
}

fn default_minimum_before() -> usize {
    1
}

fn default_minimum_after() -> usize {
    1
}

impl RuleConfig for MD058Config {
    const RULE_NAME: &'static str = "MD058";
}

#[derive(Clone, Default)]
pub struct MD058BlanksAroundTables {
    config: MD058Config,
}

impl MD058BlanksAroundTables {
    /// Create a new instance with the given configuration
    pub fn from_config_struct(config: MD058Config) -> Self {
        Self { config }
    }

    /// Check if a line is blank
    fn is_blank_line(&self, line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Count the number of blank lines before a given line index
    fn count_blank_lines_before(&self, lines: &[&str], line_index: usize) -> usize {
        let mut count = 0;
        let mut i = line_index;
        while i > 0 {
            i -= 1;
            if self.is_blank_line(lines[i]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Count the number of blank lines after a given line index
    fn count_blank_lines_after(&self, lines: &[&str], line_index: usize) -> usize {
        let mut count = 0;
        let mut i = line_index + 1;
        while i < lines.len() {
            if self.is_blank_line(lines[i]) {
                count += 1;
                i += 1;
            } else {
                break;
            }
        }
        count
    }
}

impl Rule for MD058BlanksAroundTables {
    fn name(&self) -> &'static str {
        "MD058"
    }

    fn description(&self) -> &'static str {
        "Tables should be surrounded by blank lines"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no pipe characters present (no tables)
        !ctx.content.contains('|')
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
            // Check for sufficient blank lines before table
            if table_block.start_line > 0 {
                let blank_lines_before = self.count_blank_lines_before(&lines, table_block.start_line);
                if blank_lines_before < self.config.minimum_before {
                    let needed = self.config.minimum_before - blank_lines_before;
                    let message = if self.config.minimum_before == 1 {
                        "Missing blank line before table".to_string()
                    } else {
                        format!("Missing {needed} blank lines before table")
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message,
                        line: table_block.start_line + 1,
                        column: 1,
                        end_line: table_block.start_line + 1,
                        end_column: 2,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(table_block.start_line + 1, 1),
                            replacement: format!("{}{}", "\n".repeat(needed), lines[table_block.start_line]),
                        }),
                    });
                }
            }

            // Check for sufficient blank lines after table
            if table_block.end_line < lines.len() - 1 {
                // Check if the next line is a Kramdown block attribute
                let next_line_is_attribute = if table_block.end_line + 1 < lines.len() {
                    is_kramdown_block_attribute(lines[table_block.end_line + 1])
                } else {
                    false
                };

                // Skip check if next line is a block attribute
                if !next_line_is_attribute {
                    let blank_lines_after = self.count_blank_lines_after(&lines, table_block.end_line);
                    if blank_lines_after < self.config.minimum_after {
                        let needed = self.config.minimum_after - blank_lines_after;
                        let message = if self.config.minimum_after == 1 {
                            "Missing blank line after table".to_string()
                        } else {
                            format!("Missing {needed} blank lines after table")
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message,
                            line: table_block.end_line + 1,
                            column: lines[table_block.end_line].len() + 1,
                            end_line: table_block.end_line + 1,
                            end_column: lines[table_block.end_line].len() + 2,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(
                                    table_block.end_line + 1,
                                    lines[table_block.end_line].len() + 1,
                                ),
                                replacement: format!("{}{}", lines[table_block.end_line], "\n".repeat(needed)),
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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            // Check for warnings about missing blank lines before table
            let warning_before = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message.contains("before table"));

            if let Some(idx) = warning_before {
                let warning = &warnings[idx];
                // Extract number of needed blank lines from the message or use config default
                let needed_blanks = if warning.message.contains("Missing blank line before") {
                    1
                } else if let Some(start) = warning.message.find("Missing ") {
                    if let Some(end) = warning.message.find(" blank lines before") {
                        warning.message[start + 8..end].parse::<usize>().unwrap_or(1)
                    } else {
                        1
                    }
                } else {
                    1
                };

                // Add the required number of blank lines
                for _ in 0..needed_blanks {
                    result.push("".to_string());
                }
                warnings.remove(idx);
            }

            result.push(lines[i].to_string());

            // Check for warnings about missing blank lines after table
            let warning_after = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message.contains("after table"));

            if let Some(idx) = warning_after {
                let warning = &warnings[idx];
                // Extract number of needed blank lines from the message or use config default
                let needed_blanks = if warning.message.contains("Missing blank line after") {
                    1
                } else if let Some(start) = warning.message.find("Missing ") {
                    if let Some(end) = warning.message.find(" blank lines after") {
                        warning.message[start + 8..end].parse::<usize>().unwrap_or(1)
                    } else {
                        1
                    }
                } else {
                    1
                };

                // Add the required number of blank lines
                for _ in 0..needed_blanks {
                    result.push("".to_string());
                }
                warnings.remove(idx);
            }

            i += 1;
        }

        Ok(result.join("\n"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD058Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD058Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD058Config>(config);
        Box::new(MD058BlanksAroundTables::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_table_with_blanks() {
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_tables() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Just regular text.
No tables here.
Only paragraphs.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_block_with_table() {
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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
        let rule = MD058BlanksAroundTables::default();
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

    #[test]
    fn test_config_minimum_before() {
        let config = MD058Config {
            minimum_before: 2,
            minimum_after: 1,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

        let content = "Text before.

| Header | Col 2 |
|--------|-------|
| Cell   | Data  |

Text after.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should pass with 1 blank line before (but we configured to require 2)
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Missing 1 blank lines before table"));
    }

    #[test]
    fn test_config_minimum_after() {
        let config = MD058Config {
            minimum_before: 1,
            minimum_after: 3,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

        let content = "Text before.

| Header | Col 2 |
|--------|-------|
| Cell   | Data  |

More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should fail with only 1 blank line after (but we configured to require 3)
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Missing 2 blank lines after table"));
    }

    #[test]
    fn test_config_both_minimum() {
        let config = MD058Config {
            minimum_before: 2,
            minimum_after: 2,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

        let content = "Text before.
| Header | Col 2 |
|--------|-------|
| Cell   | Data  |
More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should fail both before and after
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Missing 2 blank lines before table"));
        assert!(result[1].message.contains("Missing 2 blank lines after table"));
    }

    #[test]
    fn test_config_zero_minimum() {
        let config = MD058Config {
            minimum_before: 0,
            minimum_after: 0,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

        let content = "Text before.
| Header | Col 2 |
|--------|-------|
| Cell   | Data  |
More text.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should pass with zero blank lines required
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_with_custom_config() {
        let config = MD058Config {
            minimum_before: 2,
            minimum_after: 3,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

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
    fn test_default_config_section() {
        let rule = MD058BlanksAroundTables::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD058");

        // Should contain both minimum_before and minimum_after options with default values
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("minimum-before"));
            assert!(table.contains_key("minimum-after"));
            assert_eq!(table["minimum-before"], toml::Value::Integer(1));
            assert_eq!(table["minimum-after"], toml::Value::Integer(1));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_blank_lines_counting() {
        let rule = MD058BlanksAroundTables::default();
        let lines = vec!["text", "", "", "table", "more", "", "end"];

        // Test counting blank lines before line index 3 (table)
        assert_eq!(rule.count_blank_lines_before(&lines, 3), 2);

        // Test counting blank lines after line index 4 (more)
        assert_eq!(rule.count_blank_lines_after(&lines, 4), 1);

        // Test at beginning
        assert_eq!(rule.count_blank_lines_before(&lines, 0), 0);

        // Test at end
        assert_eq!(rule.count_blank_lines_after(&lines, 6), 0);
    }

    #[test]
    fn test_issue_25_table_with_long_line() {
        // Test case from issue #25 - table with very long line
        let rule = MD058BlanksAroundTables::default();
        let content = "# Title\n\nThis is a table:\n\n| Name          | Query                                                    |\n| ------------- | -------------------------------------------------------- |\n| b             | a                                                        |\n| c             | a                                                        |\n| d             | a                                                        |\n| long          | aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa |\n| e             | a                                                        |\n| f             | a                                                        |\n| g             | a                                                        |";
        let ctx = LintContext::new(content);

        // Debug: Print detected table blocks
        let table_blocks = TableUtils::find_table_blocks(content, &ctx);
        for (i, block) in table_blocks.iter().enumerate() {
            eprintln!(
                "Table {}: start={}, end={}, header={}, delimiter={}, content_lines={:?}",
                i + 1,
                block.start_line + 1,
                block.end_line + 1,
                block.header_line + 1,
                block.delimiter_line + 1,
                block.content_lines.iter().map(|x| x + 1).collect::<Vec<_>>()
            );
        }

        let result = rule.check(&ctx).unwrap();

        // This should detect one table, not multiple tables
        assert_eq!(table_blocks.len(), 1, "Should detect exactly one table block");

        // Should not flag any issues since table is complete and doesn't need blanks
        assert_eq!(result.len(), 0, "Should not flag any MD058 issues for a complete table");
    }
}
