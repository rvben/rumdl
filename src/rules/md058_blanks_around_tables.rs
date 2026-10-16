use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::mkdocs_attr_list::is_block_attribute_line;
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

    /// Check if a line is blank (including blockquote continuation lines and
    /// lines holding nothing but HTML comments)
    ///
    /// Delegates to the shared `is_blank_in_blockquote_context` utility function.
    /// This ensures consistent blank line detection across all rules that need
    /// to handle blockquote-prefixed blank lines (MD058, MD065, etc.), and to
    /// `is_blank_or_comment_only` for the comment convention the blank-line
    /// rules share (#866).
    fn is_blank_line(&self, line: &str) -> bool {
        crate::utils::regex_cache::is_blank_in_blockquote_context(line)
            || crate::utils::blank_lines::is_blank_or_comment_only(line)
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

/// Three or more `-`, `*` or `_` of one kind, spaces and tabs allowed between.
fn is_thematic_break(text: &str) -> bool {
    let mut chars = text.chars().filter(|c| !matches!(c, ' ' | '\t'));
    let Some(first) = chars.next().filter(|c| matches!(c, '-' | '*' | '_')) else {
        return false;
    };
    let mut count = 1;
    for c in chars {
        if c != first {
            return false;
        }
        count += 1;
    }
    count >= 3
}

/// True when `line` holds only container markers (blockquote `>` and list
/// markers) and the last one is a list marker, e.g. `-`, `> 1.` or `- -`.
fn is_bare_list_marker_line(line: &str) -> bool {
    let mut rest = line;
    let mut last_is_list = false;
    let mut markers = 0usize;
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        let Some(first) = rest.chars().next() else {
            break;
        };
        if is_thematic_break(rest) {
            return false;
        }
        let after_marker = |len: usize| rest[len..].chars().next().is_none_or(|c| c == ' ' || c == '\t');
        if first == '>' {
            rest = &rest[1..];
            last_is_list = false;
        } else if matches!(first, '-' | '*' | '+') && after_marker(1) {
            rest = &rest[1..];
            last_is_list = true;
        } else {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            let terminator = rest[digits..].chars().next();
            if (1..=9).contains(&digits) && matches!(terminator, Some('.' | ')')) && after_marker(digits + 1) {
                rest = &rest[digits + 1..];
                last_is_list = true;
            } else {
                return false;
            }
        }
        markers += 1;
    }
    last_is_list && markers > 0
}

impl Rule for MD058BlanksAroundTables {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(58))
    }

    fn name(&self) -> &'static str {
        "MD058"
    }

    fn description(&self) -> &'static str {
        "Tables should be surrounded by blank lines"
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
        let mut warnings = Vec::new();

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

        let lines = ctx.raw_lines();

        // Use pre-computed table blocks from context
        let table_blocks = &ctx.table_blocks;

        for table_block in table_blocks.iter().filter(|block| block.parser_confirmed) {
            // Check for sufficient blank lines before table
            // A table that is the content of a bare marker's item has no line to put
            // a blank before. Only a marker the parser reads as an item counts: an
            // empty item cannot interrupt a paragraph, so under paragraph text the
            // marker underlines it as a setext heading and the table follows it.
            let item_content = table_block.list_context.is_some()
                && table_block.start_line > 0
                && is_bare_list_marker_line(lines[table_block.start_line - 1])
                && ctx.list_item_on_line(table_block.start_line).is_some();
            // The opening tag of a `markdown="1"` element starts the block its content
            // belongs to, so a table right under it follows no other block.
            // In MDX `markdown` is an ordinary JSX prop and opens nothing.
            let opens_container = table_block.start_line > 0
                && ctx.flavor != crate::config::MarkdownFlavor::MDX
                && ctx.lines[table_block.start_line - 1].in_mkdocs_html_markdown
                && crate::utils::mkdocs_html_markdown::opens_markdown_block_alone(lines[table_block.start_line - 1]);
            // The first line of an admonition body has no blank line to put before it:
            // the opener line is the container edge, and the table renders inside it.
            let admonition_first_line = ctx.flavor == crate::config::MarkdownFlavor::MkDocs
                && table_block.start_line > 0
                && crate::utils::mkdocs_admonitions::is_admonition_start(lines[table_block.start_line - 1])
                && crate::utils::mkdocs_admonitions::is_admonition_content(
                    lines[table_block.start_line],
                    lines[table_block.start_line - 1].len() - lines[table_block.start_line - 1].trim_start().len(),
                );
            if table_block.start_line > 0 && !item_content && !opens_container && !admonition_first_line {
                let blank_lines_before = self.count_blank_lines_before(lines, table_block.start_line);
                if blank_lines_before < self.config.minimum_before {
                    let needed = self.config.minimum_before - blank_lines_before;
                    let message = if self.config.minimum_before == 1 {
                        "Missing blank line before table".to_string()
                    } else {
                        format!("Missing {needed} blank lines before table")
                    };

                    let bq_prefix = ctx.blockquote_prefix_for_blank_line(table_block.start_line);
                    let replacement = format!("{bq_prefix}\n").repeat(needed);
                    let fix = (ctx.flavor == crate::config::MarkdownFlavor::MDG
                        || !crate::utils::table_utils::TableUtils::header_is_indented_as_code(
                            lines[table_block.start_line],
                            lines[table_block.delimiter_line],
                            table_block,
                        ))
                    .then(|| Fix::new(ctx.line_column_byte_range(table_block.start_line + 1, 1), replacement));
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message,
                        line: table_block.start_line + 1,
                        column: 1,
                        end_line: table_block.start_line + 1,
                        end_column: 2,
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }

            // Check for sufficient blank lines after table
            if table_block.end_line < lines.len() - 1 {
                // Check if the next line is a block attribute list attached to the table
                let next_line_is_attribute = if table_block.end_line + 1 < lines.len() {
                    is_block_attribute_line(lines[table_block.end_line + 1], ctx.flavor)
                        // The closing tag of a `markdown` element ends the block its table is in.
                        || (ctx.flavor != crate::config::MarkdownFlavor::MDX
                            && ctx.lines[table_block.end_line + 1].closes_markdown_element)
                } else {
                    false
                };

                // Skip check if next line is a block attribute
                if !next_line_is_attribute {
                    let blank_lines_after = self.count_blank_lines_after(lines, table_block.end_line);
                    if blank_lines_after < self.config.minimum_after {
                        let needed = self.config.minimum_after - blank_lines_after;
                        let message = if self.config.minimum_after == 1 {
                            "Missing blank line after table".to_string()
                        } else {
                            format!("Missing {needed} blank lines after table")
                        };

                        let bq_prefix = ctx.blockquote_prefix_for_blank_line(table_block.end_line);
                        let replacement = format!("{bq_prefix}\n").repeat(needed);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message,
                            line: table_block.end_line + 1,
                            column: lines[table_block.end_line].chars().count() + 1,
                            end_line: table_block.end_line + 1,
                            end_column: lines[table_block.end_line].chars().count() + 2,
                            severity: Severity::Warning,
                            fix: Some(Fix::new(
                                ctx.line_column_byte_range(
                                    table_block.end_line + 1,
                                    lines[table_block.end_line].len() + 1,
                                ),
                                replacement,
                            )),
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
        let mut warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        warnings.retain(|warning| warning.fix.is_some());
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines = ctx.raw_lines();
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

                // Add the required number of blank lines with blockquote prefix
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                for _ in 0..needed_blanks {
                    result.push(bq_prefix.clone());
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

                // Add the required number of blank lines with blockquote prefix
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                for _ in 0..needed_blanks {
                    result.push(bq_prefix.clone());
                }
                warnings.remove(idx);
            }

            i += 1;
        }

        let mut fixed = result.join("\n");
        if content.ends_with('\n') {
            fixed.push('\n');
        }

        Ok(fixed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD058Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::table_utils::TableUtils;

    #[test]
    fn test_table_with_blanks() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Some text before.

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## Some text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## Some text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## Between tables
| Col A | Col B |
|--------|-------|
| Data 2 | Val 2 |
## After the tables";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tables separated by blank line should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consecutive_tables_no_blank() {
        let rule = MD058BlanksAroundTables::default();
        // Add a block between tables to force detection as separate tables
        let content = "Some text.

| Col 1 | Col 2 |
|--------|-------|
| Data 1 | Val 1 |
## Between
| Col A | Col B |
|--------|-------|
| Data 2 | Val 2 |

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
# Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Text before.

| Header | Col 2 |
|--------|-------|
| Cell   | Data  |

# Text after";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_multiple_tables() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Start
| T1 | C1 |
|----|----|
| D1 | V1 |
# Middle
| T2 | C2 |
|----|----|
| D2 | V2 |
# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Start

| T1 | C1 |
|----|----|
| D1 | V1 |

# Middle

| T2 | C2 |
|----|----|
| D2 | V2 |

# End";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD058BlanksAroundTables::default();
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_tables() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Just regular text.
No tables here.
Only paragraphs.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_without_content_rows() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Text.
| Header 1 | Header 2 |
|----------|----------|
## Next section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single column tables ARE now detected (fixed to support 1+ columns)
        // Expects 2 warnings: missing blank before and after table
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("before"));
        assert!(result[1].message.contains("after"));
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
## More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
# Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Text before.


| Header | Col 2 |
|--------|-------|
| Cell   | Data  |



# Text after";
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

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

    #[test]
    fn test_fix_preserves_blockquote_prefix_before_table() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
> | H1 | H2 |
> |----|---|
> | a  | b |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted before the table should have the blockquote prefix
        let expected = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line before table, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_after_table() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD058BlanksAroundTables::default();

        let content = "> | H1 | H2 |
> |----|---|
> | a  | b |
> # Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted after the table should have the blockquote prefix
        let expected = "> | H1 | H2 |
> |----|---|
> | a  | b |
>
> # Text after";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line after table, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_nested_blockquote_prefix_for_table() {
        // Nested blockquotes should preserve the full prefix
        let rule = MD058BlanksAroundTables::default();

        let content = ">> Nested quote
>> | H1 |
>> |----|
>> | a  |
>> # More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should insert ">>" blank lines
        let expected = ">> Nested quote
>>
>> | H1 |
>> |----|
>> | a  |
>>
>> # More text";
        assert_eq!(fixed, expected, "Fix should preserve nested blockquote prefix '>>'");
    }

    #[test]
    fn test_fix_preserves_triple_nested_blockquote_prefix_for_table() {
        // Triple-nested blockquotes should preserve full prefix
        let rule = MD058BlanksAroundTables::default();

        let content = ">>> Triple nested
>>> | A | B |
>>> |---|---|
>>> | 1 | 2 |
>>> # More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = ">>> Triple nested
>>>
>>> | A | B |
>>> |---|---|
>>> | 1 | 2 |
>>>
>>> # More text";
        assert_eq!(
            fixed, expected,
            "Fix should preserve triple-nested blockquote prefix '>>>'"
        );
    }

    // =========================================================================
    // Issue #305: Tables inside blockquotes with existing blank lines
    // These tests verify that MD058 correctly recognizes blockquote continuation
    // lines (e.g., ">") as "blank" lines for table spacing purposes.
    // =========================================================================

    #[test]
    fn test_is_blank_line_with_blockquote_continuation() {
        // Unit tests for is_blank_line recognizing blockquote blanks
        let rule = MD058BlanksAroundTables::default();

        // Regular blank lines
        assert!(rule.is_blank_line(""));
        assert!(rule.is_blank_line("   "));
        assert!(rule.is_blank_line("\t"));
        assert!(rule.is_blank_line("  \t  "));

        // Blockquote continuation lines (should be treated as blank)
        assert!(rule.is_blank_line(">"));
        assert!(rule.is_blank_line("> "));
        assert!(rule.is_blank_line(">  "));
        assert!(rule.is_blank_line(">>"));
        assert!(rule.is_blank_line(">> "));
        assert!(rule.is_blank_line(">>>"));
        assert!(rule.is_blank_line("> > "));
        assert!(rule.is_blank_line("> > > "));
        assert!(rule.is_blank_line("  >  ")); // With leading/trailing whitespace

        // Lines with content (should NOT be treated as blank)
        assert!(!rule.is_blank_line("text"));
        assert!(!rule.is_blank_line("> text"));
        assert!(!rule.is_blank_line(">> text"));
        assert!(!rule.is_blank_line("> | table |"));
        assert!(!rule.is_blank_line("| table |"));
    }

    #[test]
    fn test_issue_305_no_warning_blockquote_with_existing_blank_before_table() {
        // Issue #305: Table inside blockquote with existing blank line before
        // should NOT trigger MD058 warning
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not warn when blockquote already has blank line before table"
        );
    }

    #[test]
    fn test_issue_305_no_warning_blockquote_with_existing_blank_after_table() {
        // Issue #305: Table inside blockquote with existing blank line after
        // should NOT trigger MD058 warning
        let rule = MD058BlanksAroundTables::default();

        let content = "> | H1 | H2 |
> |----|---|
> | a  | b |
>
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not warn when blockquote already has blank line after table"
        );
    }

    #[test]
    fn test_issue_305_no_warning_blockquote_with_both_blank_lines() {
        // Issue #305: Complete example from the issue report
        let rule = MD058BlanksAroundTables::default();

        let content = "> The following options are available:
>
> | Option | Default   | Description       |
> |--------|-----------|-------------------|
> | port   | 3000      | Server port       |
> | host   | localhost | Server host       |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Issue #305: Should not warn for valid table inside blockquote with blank line"
        );
    }

    #[test]
    fn test_issue_305_no_warning_nested_blockquote_with_blank_lines() {
        // Nested blockquote with blank lines should not warn
        let rule = MD058BlanksAroundTables::default();

        let content = ">> Nested text
>>
>> | Col1 | Col2 |
>> |------|------|
>> | val1 | val2 |
>>
>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not warn for nested blockquote table with blank lines"
        );
    }

    #[test]
    fn test_issue_305_no_warning_triple_nested_blockquote_with_blank_lines() {
        // Triple-nested blockquote with blank lines should not warn
        let rule = MD058BlanksAroundTables::default();

        let content = ">>> Deep nesting
>>>
>>> | A | B |
>>> |---|---|
>>> | 1 | 2 |
>>>
>>> End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not warn for triple-nested blockquote table with blank lines"
        );
    }

    #[test]
    fn test_issue_305_fix_does_not_corrupt_valid_blockquote_table() {
        // Critical: Verify that fix() doesn't corrupt already-valid content
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |
>
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content, "Fix should not modify already-valid blockquote table");
    }

    #[test]
    fn test_issue_305_blockquote_blank_with_trailing_space() {
        // Blockquote blank line with trailing space ("> ") should be recognized
        let rule = MD058BlanksAroundTables::default();

        // Note: The "> " has a trailing space
        let content = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should recognize '> ' (with trailing space) as blank line"
        );
    }

    #[test]
    fn test_issue_305_spaced_nested_blockquote() {
        // "> > " style nested blockquote should be recognized
        let rule = MD058BlanksAroundTables::default();

        let content = "> > Nested text
> >
> > | H1 |
> > |----|
> > | a  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should recognize '> > ' style nested blockquote blank line"
        );
    }

    #[test]
    fn test_mixed_regular_and_blockquote_tables() {
        // Document with both regular tables and blockquote tables
        let rule = MD058BlanksAroundTables::default();

        let content = "# Mixed Content

Regular table:

| A | B |
|---|---|
| 1 | 2 |

And a blockquote table:

> Quote text
>
> | X | Y |
> |---|---|
> | 3 | 4 |
>
> End quote

Final paragraph.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should handle mixed regular and blockquote tables correctly"
        );
    }

    #[test]
    fn test_blockquote_table_at_document_start() {
        // Table in blockquote at very start of document
        let rule = MD058BlanksAroundTables::default();

        let content = "> | H1 | H2 |
> |----|---|
> | a  | b |
>
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not require blank line before table at document start (even in blockquote)"
        );
    }

    #[test]
    fn test_blockquote_table_at_document_end() {
        // Table in blockquote at very end of document
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not require blank line after table at document end"
        );
    }

    #[test]
    fn test_blockquote_table_missing_blank_still_detected() {
        // Ensure we still detect ACTUAL missing blank lines in blockquotes
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
> | H1 | H2 |
> |----|---|
> | a  | b |
> # Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have 2 warnings: missing blank before AND after table
        assert_eq!(
            result.len(),
            2,
            "Should still detect missing blank lines in blockquote tables"
        );
        assert!(result[0].message.contains("before table"));
        assert!(result[1].message.contains("after table"));
    }

    #[test]
    fn test_blockquote_table_fix_adds_correct_prefix() {
        // Verify fix adds blockquote-prefixed blank lines when needed
        let rule = MD058BlanksAroundTables::default();

        let content = "> Text before
> | H1 | H2 |
> |----|---|
> | a  | b |
> # Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "> Text before
>
> | H1 | H2 |
> |----|---|
> | a  | b |
>
> # Text after";
        assert_eq!(fixed, expected, "Fix should add blockquote-prefixed blank lines");
    }

    #[test]
    fn test_multiple_blockquote_tables_with_valid_spacing() {
        // Multiple tables in same blockquote, all with proper spacing
        let rule = MD058BlanksAroundTables::default();

        let content = "> First table:
>
> | A | B |
> |---|---|
> | 1 | 2 |
>
> Second table:
>
> | X | Y |
> |---|---|
> | 3 | 4 |
>
> End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should handle multiple blockquote tables with valid spacing"
        );
    }

    #[test]
    fn test_blockquote_table_with_minimum_before_config() {
        // Test with custom minimum_before config
        let config = MD058Config {
            minimum_before: 2,
            minimum_after: 1,
        };
        let rule = MD058BlanksAroundTables::from_config_struct(config);

        let content = "> Text
>
> | H1 |
> |----|
> | a  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should warn because only 1 blank line, but config requires 2
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("before table"));
    }

    // === Pandoc construct reachability tests ===
    //
    // These tests document that MD058 does not flag Pandoc-specific constructs
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
    fn md058_pandoc_grid_tables_not_flagged() {
        let rule = MD058BlanksAroundTables::default();
        // Grid table with no surrounding blank lines.
        // If grid tables were in table_blocks, MD058 would flag missing blanks.
        let content = "Some text before.
+---+---+
| a | b |
+===+===+
| 1 | 2 |
+---+---+
Some text after.";

        // Under Pandoc: grid tables excluded from table_blocks — no warnings.
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD058 should not flag blank lines around Pandoc grid tables (excluded by table_blocks): {result:?}"
        );

        // Under Standard: same content — grid table not recognized as pipe table.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD058 should not flag grid-table-like content under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md058_pandoc_multi_line_tables_not_flagged() {
        let rule = MD058BlanksAroundTables::default();
        let content = "Some text.
--------- -----------
Header 1   Header 2
--------- -----------
Cell 1     Cell 2
--------- -----------
More text.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD058 should not flag Pandoc multi-line tables: {result:?}"
        );

        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD058 should not flag multi-line table content under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md058_pandoc_line_blocks_not_flagged() {
        let rule = MD058BlanksAroundTables::default();
        // Pandoc line blocks are not recognized as pipe tables (no trailing `|`).
        let content = "Some text.
| First line
| Second line
More text.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD058 should not treat Pandoc line blocks as tables: {result:?}"
        );

        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD058 should not treat line-block-like content as tables under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md058_pandoc_pipe_table_captions_not_flagged() {
        let rule = MD058BlanksAroundTables::default();
        // Pipe-table captions (`: caption`) have no `|` — they are not table rows
        // and are never included in table_blocks, so MD058 ignores them.
        let content = "\
Some text.

| H1 | H2 |
|----|-----|
| a  | b  |

: My table caption
More text.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD058 should not flag the pipe-table caption line as needing blank lines: {result:?}"
        );

        // Under Standard: caption line has no `|` — excluded from table_blocks.
        // Table itself has proper blanks before and after.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "MD058 table with caption — caption not a table row under Standard: {result_std:?}"
        );
    }

    #[test]
    fn md058_hugo_block_attribute_after_table_not_flagged() {
        // Issue #756: a Goldmark/Hugo block attribute list directly under a table
        // describes that table, so MD058 must not treat it as content needing a blank.
        let rule = MD058BlanksAroundTables::default();
        let content = "\
Some text.

| H1 | H2 |
|----|-----|
| a  | b  |
{class=\"table table-striped\"}

More text.";

        for flavor in [
            crate::config::MarkdownFlavor::Hugo,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::Kramdown,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "MD058 should not flag the block attribute line under {flavor:?}: {result:?}"
            );
        }

        // Under Standard the attribute line is literal text, and GFM reads it as
        // one more row of the table, so no blank line is missing there either.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_std = rule.check(&ctx_std).unwrap();
        assert!(
            result_std.is_empty(),
            "the attribute line is a table row under Standard: {result_std:?}"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD058BlanksAroundTables::default();

        let content = "Intro\n| a | b |\n| --- | --- |\n| 1 | 2 |\n# After\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
        assert_eq!(fixed, "Intro\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n# After\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD058BlanksAroundTables::default();

        let content = "Intro\n| a | b |\n| --- | --- |\n| 1 | 2 |\n# After";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
        assert_eq!(fixed, "Intro\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n# After");
    }

    #[test]
    fn test_indented_lazy_rows_in_blockquote_are_not_a_table() {
        let rule = MD058BlanksAroundTables::default();
        for content in [
            "> o\n>     |a|b|\n>     |-|-|\n",
            ">o\n>     |a|b|\n>     |-|-|\n",
            "> > o\n> >     |a|b|\n> >     |-|-|\n",
            "> o\n> |a|b|\n>     |-|-|\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content, "{content:?}");
        }
    }

    #[test]
    fn test_real_table_after_lazy_rows_in_blockquote_is_still_checked() {
        let rule = MD058BlanksAroundTables::default();
        let content = "> o\n>     |a|b|\n>     |-|-|\n> |c|d|\n> |-|-|\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "> o\n>     |a|b|\n>     |-|-|\n>\n> |c|d|\n> |-|-|\n"
        );
    }

    #[test]
    fn test_admonition_first_body_line_table_needs_no_blank_before() {
        let rule = MD058BlanksAroundTables::default();
        for opener in ["!!! note", "??? note", "???+ note", "!!! note \"Title\"", "  !!! note"] {
            let indent = if opener.starts_with("  ") { "      " } else { "    " };
            let content = format!("{opener}\n{indent}| a | b |\n{indent}|---|---|\n{indent}| 1 | 2 |\n\nafter\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::MkDocs, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "table opening an admonition body is not missing a blank line: {content:?}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must not touch {content:?}");
        }
    }

    #[test]
    fn test_admonition_table_after_body_text_still_flagged() {
        let rule = MD058BlanksAroundTables::default();
        // Python-Markdown renders this as one paragraph, so the blank line is missing.
        let content = "!!! note\n\n    text\n    | a | b |\n    |---|---|\n    | 1 | 2 |\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.iter().any(|w| w.message.contains("before table")), "{result:?}");
    }
}
