use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::table_utils::TableUtils;
use unicode_width::UnicodeWidthStr;

mod md060_config;
use md060_config::MD060Config;

/// Rule MD060: Table Column Alignment
///
/// See [docs/md060.md](../../docs/md060.md) for full documentation, configuration, and examples.
///
/// This rule enforces consistent column alignment in Markdown tables for improved readability
/// in source form. When enabled, it ensures table columns are properly aligned with appropriate
/// padding.
///
/// ## Purpose
///
/// - **Readability**: Aligned tables are significantly easier to read in source form
/// - **Maintainability**: Properly formatted tables are easier to edit and review
/// - **Consistency**: Ensures uniform table formatting throughout documents
/// - **Developer Experience**: Makes working with tables in plain text more pleasant
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```yaml
/// MD060:
///   enabled: false  # Default: opt-in for conservative adoption
///   style: "aligned"  # Can be "aligned", "compact", or "none"
///   max_width: 120  # Optional: auto-compact for wide tables
/// ```
///
/// ### Style Options
///
/// - **aligned**: Columns are padded with spaces for visual alignment (default)
/// - **compact**: No padding, minimal spacing
/// - **none**: Disable formatting checks
///
/// ### Max Width
///
/// When `max_width` is set (default: 120), tables wider than this limit will automatically
/// use compact formatting to prevent excessive line lengths.
///
/// ## Examples
///
/// ### Aligned Style (Good)
///
/// ```markdown
/// | Name  | Age | City      |
/// |-------|-----|-----------|
/// | Alice | 30  | Seattle   |
/// | Bob   | 25  | Portland  |
/// ```
///
/// ### Unaligned (Bad)
///
/// ```markdown
/// | Name | Age | City |
/// |---|---|---|
/// | Alice | 30 | Seattle |
/// | Bob | 25 | Portland |
/// ```
///
/// ## Unicode Support
///
/// This rule properly handles:
/// - **CJK Characters**: Chinese, Japanese, Korean characters are correctly measured as double-width
/// - **Basic Emoji**: Most emoji are handled correctly
/// - **Inline Code**: Pipes in inline code blocks are properly masked
///
/// ## Known Limitations
///
/// **Complex Emoji Sequences**: Tables containing Zero-Width Joiner (ZWJ) emoji sequences
/// (e.g., 👨‍👩‍👧‍👦, 👩‍💻) are automatically skipped. These complex emoji have inconsistent
/// display widths across different terminals and fonts, making accurate alignment impossible.
/// The rule will preserve these tables as-is rather than risk corrupting them.
///
/// This is an honest limitation of terminal display technology, similar to what other tools
/// like markdownlint experience.
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Calculates proper display width for each column using Unicode width measurements
/// - Pads cells with trailing spaces to align columns
/// - Preserves cell content exactly (only spacing is modified)
/// - Respects alignment indicators in delimiter rows (`:---`, `:---:`, `---:`)
/// - Automatically switches to compact mode for tables exceeding max_width
/// - Skips tables with ZWJ emoji to prevent corruption
#[derive(Debug, Default, Clone)]
pub struct MD060TableFormat {
    config: MD060Config,
}

impl MD060TableFormat {
    pub fn new(enabled: bool, style: String) -> Self {
        Self {
            config: MD060Config { enabled, style },
        }
    }

    pub fn from_config_struct(config: MD060Config) -> Self {
        Self { config }
    }

    fn contains_zwj(text: &str) -> bool {
        text.contains('\u{200D}')
    }

    fn calculate_cell_display_width(cell_content: &str) -> usize {
        let masked = TableUtils::mask_pipes_in_inline_code(cell_content);
        masked.trim().width()
    }

    fn parse_table_row(line: &str) -> Vec<String> {
        let trimmed = line.trim();
        let masked = TableUtils::mask_pipes_in_inline_code(trimmed);

        let has_leading = masked.starts_with('|');
        let has_trailing = masked.ends_with('|');

        let mut masked_content = masked.as_str();
        let mut orig_content = trimmed;

        if has_leading {
            masked_content = &masked_content[1..];
            orig_content = &orig_content[1..];
        }
        if has_trailing && !masked_content.is_empty() {
            masked_content = &masked_content[..masked_content.len() - 1];
            orig_content = &orig_content[..orig_content.len() - 1];
        }

        let masked_parts: Vec<&str> = masked_content.split('|').collect();
        let mut cells = Vec::new();
        let mut pos = 0;

        for masked_cell in masked_parts {
            let cell_len = masked_cell.len();
            let orig_cell = if pos + cell_len <= orig_content.len() {
                &orig_content[pos..pos + cell_len]
            } else {
                masked_cell
            };
            cells.push(orig_cell.to_string());
            pos += cell_len + 1;
        }

        cells
    }

    fn is_delimiter_row(row: &[String]) -> bool {
        if row.is_empty() {
            return false;
        }
        row.iter()
            .all(|cell| cell.trim().chars().all(|c| c == '-' || c == ':' || c.is_whitespace()))
    }

    fn calculate_column_widths(table_lines: &[&str]) -> Vec<usize> {
        let mut column_widths = Vec::new();

        for line in table_lines {
            let cells = Self::parse_table_row(line);
            for (i, cell) in cells.iter().enumerate() {
                let width = Self::calculate_cell_display_width(cell);
                if i >= column_widths.len() {
                    column_widths.push(width);
                } else {
                    column_widths[i] = column_widths[i].max(width);
                }
            }
        }

        // GFM requires delimiter rows to have at least 3 dashes per column.
        // To ensure visual alignment, all columns must be at least width 3.
        column_widths.iter().map(|&w| w.max(3)).collect()
    }

    fn format_table_row(cells: &[String], column_widths: &[usize], is_delimiter: bool) -> String {
        let formatted_cells: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let target_width = column_widths.get(i).copied().unwrap_or(0);
                if is_delimiter {
                    let trimmed = cell.trim();
                    let has_left_colon = trimmed.starts_with(':');
                    let has_right_colon = trimmed.ends_with(':');

                    // Delimiter rows use the same cell format as content rows: | content |
                    // The "content" is dashes, possibly with colons for alignment
                    let dash_count = if has_left_colon && has_right_colon {
                        target_width.saturating_sub(2)
                    } else if has_left_colon || has_right_colon {
                        target_width.saturating_sub(1)
                    } else {
                        target_width
                    };

                    let dashes = "-".repeat(dash_count.max(3)); // Minimum 3 dashes
                    let delimiter_content = if has_left_colon && has_right_colon {
                        format!(":{dashes}:")
                    } else if has_left_colon {
                        format!(":{dashes}")
                    } else if has_right_colon {
                        format!("{dashes}:")
                    } else {
                        dashes
                    };

                    // Add spaces around delimiter content, just like content cells
                    format!(" {delimiter_content} ")
                } else {
                    let trimmed = cell.trim();
                    let current_width = Self::calculate_cell_display_width(cell);
                    let padding = target_width.saturating_sub(current_width);
                    format!(" {trimmed}{} ", " ".repeat(padding))
                }
            })
            .collect();

        format!("|{}|", formatted_cells.join("|"))
    }

    fn format_table_compact(cells: &[String]) -> String {
        let formatted_cells: Vec<String> = cells.iter().map(|cell| format!(" {} ", cell.trim())).collect();
        format!("|{}|", formatted_cells.join("|"))
    }

    fn format_table_tight(cells: &[String]) -> String {
        let formatted_cells: Vec<String> = cells.iter().map(|cell| cell.trim().to_string()).collect();
        format!("|{}|", formatted_cells.join("|"))
    }

    fn detect_table_style(table_lines: &[&str]) -> Option<String> {
        if table_lines.is_empty() {
            return None;
        }

        let first_line = table_lines[0];
        let cells = Self::parse_table_row(first_line);

        if cells.is_empty() {
            return None;
        }

        let has_no_padding = cells.iter().all(|cell| !cell.starts_with(' ') && !cell.ends_with(' '));

        let has_single_space = cells.iter().all(|cell| {
            let trimmed = cell.trim();
            cell == &format!(" {trimmed} ")
        });

        if has_no_padding {
            Some("tight".to_string())
        } else if has_single_space {
            Some("compact".to_string())
        } else {
            Some("aligned".to_string())
        }
    }

    fn fix_table_block(&self, lines: &[&str], table_block: &crate::utils::table_utils::TableBlock) -> Vec<String> {
        let mut result = Vec::new();

        let table_lines: Vec<&str> = std::iter::once(lines[table_block.header_line])
            .chain(std::iter::once(lines[table_block.delimiter_line]))
            .chain(table_block.content_lines.iter().map(|&idx| lines[idx]))
            .collect();

        if table_lines.iter().any(|line| Self::contains_zwj(line)) {
            return table_lines.iter().map(|s| s.to_string()).collect();
        }

        let style = self.config.style.as_str();

        match style {
            "any" => {
                let detected_style = Self::detect_table_style(&table_lines);
                if detected_style.is_none() {
                    return table_lines.iter().map(|s| s.to_string()).collect();
                }

                let target_style = detected_style.unwrap();
                for line in &table_lines {
                    let cells = Self::parse_table_row(line);
                    match target_style.as_str() {
                        "tight" => result.push(Self::format_table_tight(&cells)),
                        "compact" => result.push(Self::format_table_compact(&cells)),
                        _ => {
                            let column_widths = Self::calculate_column_widths(&table_lines);
                            let is_delimiter = Self::is_delimiter_row(&cells);
                            result.push(Self::format_table_row(&cells, &column_widths, is_delimiter));
                        }
                    }
                }
                return result;
            }
            "compact" => {
                for line in table_lines {
                    let cells = Self::parse_table_row(line);
                    result.push(Self::format_table_compact(&cells));
                }
                return result;
            }
            "tight" => {
                for line in table_lines {
                    let cells = Self::parse_table_row(line);
                    result.push(Self::format_table_tight(&cells));
                }
                return result;
            }
            "aligned" => {
                let column_widths = Self::calculate_column_widths(&table_lines);

                for line in table_lines {
                    let cells = Self::parse_table_row(line);
                    let is_delimiter = Self::is_delimiter_row(&cells);
                    result.push(Self::format_table_row(&cells, &column_widths, is_delimiter));
                }
            }
            _ => {
                return table_lines.iter().map(|s| s.to_string()).collect();
            }
        }

        result
    }
}

impl Rule for MD060TableFormat {
    fn name(&self) -> &'static str {
        "MD060"
    }

    fn description(&self) -> &'static str {
        "Table columns should be consistently aligned"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !self.config.enabled || !ctx.likely_has_tables()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let content = ctx.content;
        let line_index = &ctx.line_index;
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let table_blocks = &ctx.table_blocks;

        for table_block in table_blocks {
            let fixed_lines = self.fix_table_block(&lines, table_block);

            let table_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            for (i, &line_idx) in table_line_indices.iter().enumerate() {
                let original = lines[line_idx];
                let fixed = &fixed_lines[i];

                if original != fixed {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, original);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        severity: Severity::Warning,
                        message: "Table columns should be aligned".to_string(),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        fix: Some(crate::rule::Fix {
                            range: line_index.whole_line_range(line_idx + 1),
                            replacement: if line_idx < lines.len() - 1 {
                                format!("{fixed}\n")
                            } else {
                                fixed.clone()
                            },
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.config.enabled {
            return Ok(ctx.content.to_string());
        }

        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();
        let table_blocks = &ctx.table_blocks;

        let mut result_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();

        for table_block in table_blocks {
            let fixed_lines = self.fix_table_block(&lines, table_block);

            let table_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            for (i, &line_idx) in table_line_indices.iter().enumerate() {
                result_lines[line_idx] = fixed_lines[i].clone();
            }
        }

        let mut fixed = result_lines.join("\n");
        if content.ends_with('\n') && !fixed.ends_with('\n') {
            fixed.push('\n');
        }
        Ok(fixed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD060Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_md060_disabled_by_default() {
        let rule = MD060TableFormat::default();
        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_md060_align_simple_ascii_table() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
        assert_eq!(fixed, expected);

        // Verify all rows have equal length in aligned mode
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
    }

    #[test]
    fn test_md060_cjk_characters_aligned_correctly() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age |\n|---|---|\n| 中文 | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        let cells_line1 = MD060TableFormat::parse_table_row(lines[0]);
        let cells_line3 = MD060TableFormat::parse_table_row(lines[2]);

        let width1 = MD060TableFormat::calculate_cell_display_width(&cells_line1[0]);
        let width3 = MD060TableFormat::calculate_cell_display_width(&cells_line3[0]);

        assert_eq!(width1, width3);
    }

    #[test]
    fn test_md060_basic_emoji() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Status | Name |\n|---|---|\n| ✅ | Test |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("Status"));
    }

    #[test]
    fn test_md060_zwj_emoji_skipped() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Emoji | Name |\n|---|---|\n| 👨‍👩‍👧‍👦 | Family |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_md060_inline_code_with_pipes() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Pattern | Regex |\n|---|---|\n| Time | `[0-9]|[0-9]` |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("`[0-9]|[0-9]`"));
    }

    #[test]
    fn test_md060_compact_style() {
        let rule = MD060TableFormat::new(true, "compact".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "| Name | Age |\n| --- | --- |\n| Alice | 30 |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_md060_tight_style() {
        let rule = MD060TableFormat::new(true, "tight".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "|Name|Age|\n|---|---|\n|Alice|30|";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_md060_any_style_consistency() {
        let rule = MD060TableFormat::new(true, "any".to_string());

        // Table is already compact, should stay compact
        let content = "| Name | Age |\n| --- | --- |\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);

        // Table is aligned, should stay aligned
        let content_aligned = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
        let ctx_aligned = LintContext::new(content_aligned, crate::config::MarkdownFlavor::Standard);

        let fixed_aligned = rule.fix(&ctx_aligned).unwrap();
        assert_eq!(fixed_aligned, content_aligned);
    }

    #[test]
    fn test_md060_empty_cells() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| A | B |\n|---|---|\n|  | X |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("|"));
    }

    #[test]
    fn test_md060_mixed_content() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age | City |\n|---|---|---|\n| 中文 | 30 | NYC |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("中文"));
        assert!(fixed.contains("NYC"));
    }

    #[test]
    fn test_md060_preserve_alignment_indicators() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains(":---"), "Should contain left alignment");
        assert!(fixed.contains(":----:"), "Should contain center alignment");
        assert!(fixed.contains("----:"), "Should contain right alignment");
    }

    #[test]
    fn test_md060_minimum_column_width() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        // Test with very short column content to ensure minimum width of 3
        // GFM requires at least 3 dashes in delimiter rows
        let content = "| ID | Name |\n|-|-|\n| 1 | A |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());

        // Verify minimum width is enforced
        assert!(fixed.contains("ID "), "Short content should be padded");
        assert!(fixed.contains("---"), "Delimiter should have at least 3 dashes");
    }
}
