use std::collections::HashSet;

use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::table_utils::TableUtils;

/// Rule MD075: Orphaned table rows / headerless tables
///
/// See [docs/md075.md](../../docs/md075.md) for full documentation and examples.
///
/// Detects two cases:
/// 1. Pipe-delimited rows separated from a preceding table by blank lines (auto-fixable)
/// 2. Standalone pipe-formatted rows without a table header/delimiter (warn only)
#[derive(Clone, Default)]
pub struct MD075OrphanedTableRows;

/// Represents a group of orphaned rows after a table (Case 1)
struct OrphanedGroup {
    /// First blank line separating orphaned rows from the table
    blank_start: usize,
    /// Last blank line before the orphaned rows
    blank_end: usize,
    /// The orphaned row lines (0-indexed)
    row_lines: Vec<usize>,
}

/// Represents standalone headerless pipe content (Case 2)
struct HeaderlessGroup {
    /// The first line of the group (0-indexed)
    start_line: usize,
    /// All lines in the group (0-indexed)
    lines: Vec<usize>,
}

impl MD075OrphanedTableRows {
    /// Check if a line should be skipped (frontmatter, code block, HTML, ESM, mkdocstrings)
    fn should_skip_line(&self, ctx: &crate::lint_context::LintContext, line_idx: usize) -> bool {
        if let Some(line_info) = ctx.lines.get(line_idx) {
            line_info.in_front_matter
                || line_info.in_code_block
                || line_info.in_html_block
                || line_info.in_html_comment
                || line_info.in_esm_block
                || line_info.in_mkdocstrings
        } else {
            false
        }
    }

    /// Check if a line is a potential table row, handling blockquote prefixes
    fn is_table_row_line(&self, line: &str) -> bool {
        let content = Self::strip_blockquote_prefix(line);
        TableUtils::is_potential_table_row(content)
    }

    /// Check if a line is a delimiter row, handling blockquote prefixes
    fn is_delimiter_line(&self, line: &str) -> bool {
        let content = Self::strip_blockquote_prefix(line);
        TableUtils::is_delimiter_row(content)
    }

    /// Strip blockquote prefix from a line, returning the content after it
    fn strip_blockquote_prefix(line: &str) -> &str {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') {
            return line;
        }
        let mut rest = trimmed;
        while rest.starts_with('>') {
            rest = rest[1..].trim_start();
        }
        rest
    }

    /// Check if a line is blank (including blockquote continuation lines like ">")
    fn is_blank_line(line: &str) -> bool {
        crate::utils::regex_cache::is_blank_in_blockquote_context(line)
    }

    /// Detect Case 1: Orphaned rows after existing tables
    fn detect_orphaned_rows(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
        table_line_set: &HashSet<usize>,
    ) -> Vec<OrphanedGroup> {
        let mut groups = Vec::new();

        for table_block in &ctx.table_blocks {
            let end = table_block.end_line;

            // Scan past end of table for blank lines followed by pipe rows
            let mut i = end + 1;
            let mut blank_start = None;
            let mut blank_end = None;

            // Find blank lines after the table
            while i < content_lines.len() {
                if self.should_skip_line(ctx, i) {
                    break;
                }
                if Self::is_blank_line(content_lines[i]) {
                    if blank_start.is_none() {
                        blank_start = Some(i);
                    }
                    blank_end = Some(i);
                    i += 1;
                } else {
                    break;
                }
            }

            // If no blank lines found, no orphan scenario
            let (Some(bs), Some(be)) = (blank_start, blank_end) else {
                continue;
            };

            // Now check if the lines after the blanks are pipe rows not in any table
            let mut orphan_rows = Vec::new();
            let mut j = be + 1;
            while j < content_lines.len() {
                if self.should_skip_line(ctx, j) {
                    break;
                }
                if table_line_set.contains(&j) {
                    break;
                }
                if self.is_table_row_line(content_lines[j]) {
                    orphan_rows.push(j);
                    j += 1;
                } else {
                    break;
                }
            }

            if !orphan_rows.is_empty() {
                groups.push(OrphanedGroup {
                    blank_start: bs,
                    blank_end: be,
                    row_lines: orphan_rows,
                });
            }
        }

        groups
    }

    /// Detect Case 2: Standalone headerless pipe content
    fn detect_headerless_tables(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
        table_line_set: &HashSet<usize>,
        orphaned_line_set: &HashSet<usize>,
    ) -> Vec<HeaderlessGroup> {
        let mut groups = Vec::new();
        let mut i = 0;

        while i < content_lines.len() {
            // Skip lines in skip contexts, existing tables, or orphaned groups
            if self.should_skip_line(ctx, i) || table_line_set.contains(&i) || orphaned_line_set.contains(&i) {
                i += 1;
                continue;
            }

            // Look for consecutive pipe rows
            if self.is_table_row_line(content_lines[i]) {
                let start = i;
                let mut group_lines = vec![i];
                i += 1;

                while i < content_lines.len()
                    && !self.should_skip_line(ctx, i)
                    && !table_line_set.contains(&i)
                    && !orphaned_line_set.contains(&i)
                    && self.is_table_row_line(content_lines[i])
                {
                    group_lines.push(i);
                    i += 1;
                }

                // Need at least 2 consecutive pipe rows to flag
                if group_lines.len() >= 2 {
                    // Check that none of these lines is a delimiter row that would make
                    // them a valid table header+delimiter combination
                    let has_delimiter = group_lines
                        .iter()
                        .any(|&idx| self.is_delimiter_line(content_lines[idx]));

                    if !has_delimiter {
                        // Verify consistent column count
                        let first_content = Self::strip_blockquote_prefix(content_lines[group_lines[0]]);
                        let first_count = TableUtils::count_cells(first_content);
                        let consistent = group_lines.iter().all(|&idx| {
                            let content = Self::strip_blockquote_prefix(content_lines[idx]);
                            TableUtils::count_cells(content) == first_count
                        });

                        if consistent && first_count > 0 {
                            groups.push(HeaderlessGroup {
                                start_line: start,
                                lines: group_lines,
                            });
                        }
                    }
                }
            } else {
                i += 1;
            }
        }

        groups
    }
}

impl Rule for MD075OrphanedTableRows {
    fn name(&self) -> &'static str {
        "MD075"
    }

    fn description(&self) -> &'static str {
        "Orphaned table rows or headerless pipe content"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Table
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Need at least 4 pipe characters for 2 pipe rows
        ctx.char_count('|') < 4
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content_lines = ctx.raw_lines();
        let mut warnings = Vec::new();

        // Build set of all lines belonging to existing table blocks
        let mut table_line_set = HashSet::new();
        for table_block in &ctx.table_blocks {
            for line_idx in table_block.start_line..=table_block.end_line {
                table_line_set.insert(line_idx);
            }
        }

        // Case 1: Orphaned rows after tables
        let orphaned_groups = self.detect_orphaned_rows(ctx, content_lines, &table_line_set);
        let mut orphaned_line_set = HashSet::new();
        for group in &orphaned_groups {
            for &line_idx in &group.row_lines {
                orphaned_line_set.insert(line_idx);
            }
            // Also mark blank lines as part of the orphan group for dedup
            for line_idx in group.blank_start..=group.blank_end {
                orphaned_line_set.insert(line_idx);
            }
        }

        for group in &orphaned_groups {
            let first_orphan = group.row_lines[0];
            let last_orphan = *group.row_lines.last().unwrap();
            let num_blanks = group.blank_end - group.blank_start + 1;

            // Range from start of first blank line to start of first orphan row,
            // which removes all blank lines including their trailing newlines
            let start_range = ctx.line_index.line_col_to_byte_range(group.blank_start + 1, 1);
            let end_range = ctx.line_index.line_col_to_byte_range(first_orphan + 1, 1);
            let fix_range = start_range.start..end_range.start;

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!("Orphaned table row(s) separated from preceding table by {num_blanks} blank line(s)"),
                line: first_orphan + 1,
                column: 1,
                end_line: last_orphan + 1,
                end_column: content_lines[last_orphan].len() + 1,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: fix_range,
                    replacement: String::new(),
                }),
            });
        }

        // Case 2: Headerless pipe content
        let headerless_groups = self.detect_headerless_tables(ctx, content_lines, &table_line_set, &orphaned_line_set);

        for group in &headerless_groups {
            let start = group.start_line;
            let end = *group.lines.last().unwrap();

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: "Pipe-formatted rows without a table header/delimiter row".to_string(),
                line: start + 1,
                column: 1,
                end_line: end + 1,
                end_column: content_lines[end].len() + 1,
                severity: Severity::Warning,
                fix: None,
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let warnings = self.check(ctx)?;

        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all blank line indices to remove (only from Case 1 / fixable warnings)
        let content_lines = ctx.raw_lines();
        let mut table_line_set = HashSet::new();
        for table_block in &ctx.table_blocks {
            for line_idx in table_block.start_line..=table_block.end_line {
                table_line_set.insert(line_idx);
            }
        }

        let orphaned_groups = self.detect_orphaned_rows(ctx, content_lines, &table_line_set);
        let mut lines_to_remove: HashSet<usize> = HashSet::new();
        for group in &orphaned_groups {
            for line_idx in group.blank_start..=group.blank_end {
                lines_to_remove.insert(line_idx);
            }
        }

        if lines_to_remove.is_empty() {
            return Ok(content.to_string());
        }

        // Build output excluding removed lines
        let result: Vec<&str> = content_lines
            .iter()
            .enumerate()
            .filter(|(i, _)| !lines_to_remove.contains(i))
            .map(|(_, line)| *line)
            .collect();

        Ok(result.join("\n"))
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::ConditionallyFixable
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD075OrphanedTableRows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    // =========================================================================
    // Case 1: Orphaned rows after a table
    // =========================================================================

    #[test]
    fn test_orphaned_rows_after_table() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| Value        | Description       |
| ------------ | ----------------- |
| `consistent` | Default style     |

| `fenced`     | Fenced style      |
| `indented`   | Indented style    |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned table row"));
        assert!(result[0].fix.is_some());
    }

    #[test]
    fn test_orphaned_single_row_after_table() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned table row"));
    }

    #[test]
    fn test_orphaned_rows_multiple_blank_lines() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |


| c  | d   |
| e  | f   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("2 blank line(s)"));
    }

    #[test]
    fn test_fix_orphaned_rows() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| Value        | Description       |
| ------------ | ----------------- |
| `consistent` | Default style     |

| `fenced`     | Fenced style      |
| `indented`   | Indented style    |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
| Value        | Description       |
| ------------ | ----------------- |
| `consistent` | Default style     |
| `fenced`     | Fenced style      |
| `indented`   | Indented style    |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_orphaned_rows_multiple_blanks() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |


| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
| H1 | H2 |
|----|-----|
| a  | b   |
| c  | d   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_no_orphan_with_text_between() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

Some text here.

| c  | d   |
| e  | f   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Non-blank content between table and pipe rows means not orphaned
        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();
        assert_eq!(orphan_warnings.len(), 0);
    }

    #[test]
    fn test_valid_consecutive_tables_not_flagged() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| H3 | H4 |
|----|-----|
| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Two valid tables separated by a blank line produce no warnings
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_orphaned_rows_with_different_column_count() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 | H3 |
|----|-----|-----|
| a  | b   | c   |

| d  | e   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Different column count should still flag as orphaned
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned"));
    }

    // =========================================================================
    // Case 2: Headerless pipe content
    // =========================================================================

    #[test]
    fn test_headerless_pipe_content() {
        let rule = MD075OrphanedTableRows;
        let content = "\
Some text.

| value1 | description1 |
| value2 | description2 |

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("without a table header"));
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_single_pipe_row_not_flagged() {
        let rule = MD075OrphanedTableRows;
        let content = "\
Some text.

| value1 | description1 |

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single standalone pipe row is not flagged (Case 2 requires 2+)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_headerless_multiple_rows() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| a | b |
| c | d |
| e | f |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("without a table header"));
    }

    #[test]
    fn test_headerless_inconsistent_columns_not_flagged() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| a | b |
| c | d | e |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Inconsistent column count is not flagged as headerless table
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_headerless_not_flagged_when_has_delimiter() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Valid table with header/delimiter produces no warnings
        assert_eq!(result.len(), 0);
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn test_pipe_rows_in_code_block_ignored() {
        let rule = MD075OrphanedTableRows;
        let content = "\
```
| a | b |
| c | d |
```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_pipe_rows_in_frontmatter_ignored() {
        let rule = MD075OrphanedTableRows;
        let content = "\
---
title: test
---

| a | b |
| c | d |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Frontmatter is skipped, standalone pipe rows after it are flagged
        let warnings: Vec<_> = result
            .iter()
            .filter(|w| w.message.contains("without a table header"))
            .collect();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_no_pipes_at_all() {
        let rule = MD075OrphanedTableRows;
        let content = "Just regular text.\nNo pipes here.\nOnly paragraphs.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD075OrphanedTableRows;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_orphaned_rows_in_blockquote() {
        let rule = MD075OrphanedTableRows;
        let content = "\
> | H1 | H2 |
> |----|-----|
> | a  | b   |
>
> | c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned"));
    }

    #[test]
    fn test_fix_orphaned_rows_in_blockquote() {
        let rule = MD075OrphanedTableRows;
        let content = "\
> | H1 | H2 |
> |----|-----|
> | a  | b   |
>
> | c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
> | H1 | H2 |
> |----|-----|
> | a  | b   |
> | c  | d   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_table_at_end_of_document_no_orphans() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_followed_by_text_no_orphans() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

Some text after the table.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_preserves_content_around_orphans() {
        let rule = MD075OrphanedTableRows;
        let content = "\
# Title

| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |

Some text after.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
# Title

| H1 | H2 |
|----|-----|
| a  | b   |
| c  | d   |

Some text after.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_multiple_orphan_groups() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |

| H3 | H4 |
|----|-----|
| e  | f   |

| g  | h   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();
        assert_eq!(orphan_warnings.len(), 2);
    }

    #[test]
    fn test_fix_multiple_orphan_groups() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |

| H3 | H4 |
|----|-----|
| e  | f   |

| g  | h   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
| H1 | H2 |
|----|-----|
| a  | b   |
| c  | d   |

| H3 | H4 |
|----|-----|
| e  | f   |
| g  | h   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_orphaned_rows_with_delimiter_form_new_table() {
        let rule = MD075OrphanedTableRows;
        // Rows after a blank that themselves form a valid table (header+delimiter)
        // are recognized as a separate table by table_blocks, not as orphans
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |
|----|-----|";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The second group forms a valid table, so no orphan warning
        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();
        assert_eq!(orphan_warnings.len(), 0);
    }

    #[test]
    fn test_headerless_not_confused_with_orphaned() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

Some text.

| c  | d   |
| e  | f   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Non-blank content between table and pipe rows means not orphaned
        // The standalone rows should be flagged as headerless (Case 2)
        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();
        let headerless_warnings: Vec<_> = result
            .iter()
            .filter(|w| w.message.contains("without a table header"))
            .collect();

        assert_eq!(orphan_warnings.len(), 0);
        assert_eq!(headerless_warnings.len(), 1);
    }

    #[test]
    fn test_fix_does_not_modify_headerless() {
        let rule = MD075OrphanedTableRows;
        let content = "\
Some text.

| value1 | description1 |
| value2 | description2 |

More text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Case 2 has no fix, so content should be unchanged
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_should_skip_few_pipes() {
        let rule = MD075OrphanedTableRows;
        let content = "a | b";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_fix_capability() {
        let rule = MD075OrphanedTableRows;
        assert_eq!(rule.fix_capability(), FixCapability::ConditionallyFixable);
    }

    #[test]
    fn test_category() {
        let rule = MD075OrphanedTableRows;
        assert_eq!(rule.category(), RuleCategory::Table);
    }

    #[test]
    fn test_issue_420_exact_example() {
        // The exact example from issue #420
        let rule = MD075OrphanedTableRows;
        let content = "\
| Value        | Description       |
| ------------ | ----------------- |
| `consistent` | Default style     |

| `fenced`     | Fenced style      |
| `indented`   | Indented style    |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned"));
        assert_eq!(result[0].line, 5);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "\
| Value        | Description       |
| ------------ | ----------------- |
| `consistent` | Default style     |
| `fenced`     | Fenced style      |
| `indented`   | Indented style    |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_html_comment_pipe_rows_ignored() {
        let rule = MD075OrphanedTableRows;
        let content = "\
<!--
| a | b |
| c | d |
-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_orphan_detection_does_not_cross_skip_contexts() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

```
| c  | d   |
```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Pipe rows inside code block should not be flagged as orphaned
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_pipe_rows_in_esm_block_ignored() {
        let rule = MD075OrphanedTableRows;
        // ESM blocks use import/export statements; pipe rows inside should be skipped
        let content = "\
<script type=\"module\">
| a | b |
| c | d |
</script>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All pipe rows are inside an HTML/ESM block, no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_range_covers_blank_lines_correctly() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        let fix = result[0].fix.as_ref().unwrap();
        // The fix range should be non-empty and cover the blank line
        assert!(fix.range.start < fix.range.end);
        // Applying the fix by replacing the range should produce valid output
        let mut fixed = String::from(content);
        fixed.replace_range(fix.range.clone(), &fix.replacement);
        let expected = "\
| H1 | H2 |
|----|-----|
| a  | b   |
| c  | d   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_range_multiple_blanks() {
        let rule = MD075OrphanedTableRows;
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |


| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        let fix = result[0].fix.as_ref().unwrap();
        assert!(fix.range.start < fix.range.end);
        let mut fixed = String::from(content);
        fixed.replace_range(fix.range.clone(), &fix.replacement);
        let expected = "\
| H1 | H2 |
|----|-----|
| a  | b   |
| c  | d   |";
        assert_eq!(fixed, expected);
    }
}
