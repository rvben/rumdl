use std::collections::HashSet;

use super::md060_table_format::{MD060Config, MD060TableFormat};
use crate::md013_line_length::MD013Config;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::ensure_consistent_line_endings;
use crate::utils::fix_utils::apply_warning_fixes;
use crate::utils::table_utils::TableUtils;

/// Rule MD075: Orphaned table rows / headerless tables
///
/// See [docs/md075.md](../../docs/md075.md) for full documentation and examples.
///
/// Detects two cases:
/// 1. Pipe-delimited rows separated from a preceding table by blank lines (auto-fixable)
/// 2. Standalone pipe-formatted rows without a table header/delimiter (warn only)
#[derive(Clone)]
pub struct MD075OrphanedTableRows {
    md060_formatter: MD060TableFormat,
}

/// Represents a group of orphaned rows after a table (Case 1)
struct OrphanedGroup {
    /// Start line of the preceding table block (0-indexed)
    table_start: usize,
    /// End line of the preceding table block (0-indexed)
    table_end: usize,
    /// Expected table column count derived from the original table header
    expected_columns: usize,
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
    fn with_formatter(md060_formatter: MD060TableFormat) -> Self {
        Self { md060_formatter }
    }

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

    /// Heuristic to detect templating syntax (Liquid/Jinja-style markers).
    fn contains_template_marker(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains("{%")
            || trimmed.contains("%}")
            || trimmed.contains("{{")
            || trimmed.contains("}}")
            || trimmed.contains("{#")
            || trimmed.contains("#}")
    }

    /// Detect lines that are pure template directives (e.g., `{% data ... %}`).
    fn is_template_directive_line(line: &str) -> bool {
        let trimmed = line.trim();
        (trimmed.starts_with("{%")
            || trimmed.starts_with("{%-")
            || trimmed.starts_with("{{")
            || trimmed.starts_with("{{-"))
            && (trimmed.ends_with("%}")
                || trimmed.ends_with("-%}")
                || trimmed.ends_with("}}")
                || trimmed.ends_with("-}}"))
    }

    /// Pipe-bearing lines with template markers are often generated fragments, not literal tables.
    fn is_templated_pipe_line(line: &str) -> bool {
        let content = Self::strip_blockquote_prefix(line).trim();
        content.contains('|') && Self::contains_template_marker(content)
    }

    /// Row-like line with pipes that is not itself a valid table row, often used
    /// as an in-table section divider (for example: `Search||`).
    fn is_sparse_table_row_hint(line: &str) -> bool {
        let content = Self::strip_blockquote_prefix(line).trim();
        if content.is_empty()
            || !content.contains('|')
            || Self::contains_template_marker(content)
            || TableUtils::is_delimiter_row(content)
            || TableUtils::is_potential_table_row(content)
        {
            return false;
        }

        let has_edge_pipe = content.starts_with('|') || content.ends_with('|');
        let has_repeated_pipe = content.contains("||");
        let non_empty_parts = content.split('|').filter(|part| !part.trim().is_empty()).count();

        non_empty_parts >= 1 && (has_edge_pipe || has_repeated_pipe)
    }

    /// Headerless groups after a sparse row that is itself inside a table context
    /// are likely false positives caused by parser table-block boundaries.
    fn preceded_by_sparse_table_context(content_lines: &[&str], start_line: usize) -> bool {
        let mut idx = start_line;
        while idx > 0 {
            idx -= 1;
            let content = Self::strip_blockquote_prefix(content_lines[idx]).trim();
            if content.is_empty() {
                continue;
            }

            if !Self::is_sparse_table_row_hint(content) {
                return false;
            }

            let mut scan = idx;
            while scan > 0 {
                scan -= 1;
                let prev = Self::strip_blockquote_prefix(content_lines[scan]).trim();
                if prev.is_empty() {
                    break;
                }
                if TableUtils::is_delimiter_row(prev) {
                    return true;
                }
            }

            return false;
        }

        false
    }

    /// Headerless rows immediately following a template directive are likely generated table fragments.
    fn preceded_by_template_directive(content_lines: &[&str], start_line: usize) -> bool {
        let mut idx = start_line;
        while idx > 0 {
            idx -= 1;
            let content = Self::strip_blockquote_prefix(content_lines[idx]).trim();
            if content.is_empty() {
                continue;
            }

            return Self::is_template_directive_line(content);
        }

        false
    }

    /// Count visual indentation width where tab is treated as 4 spaces.
    fn indentation_width(line: &str) -> usize {
        let mut width = 0;
        for b in line.bytes() {
            match b {
                b' ' => width += 1,
                b'\t' => width += 4,
                _ => break,
            }
        }
        width
    }

    /// Count blockquote nesting depth for context matching.
    fn blockquote_depth(line: &str) -> usize {
        let (prefix, _) = TableUtils::extract_blockquote_prefix(line);
        prefix.bytes().filter(|&b| b == b'>').count()
    }

    /// Ensure candidate orphan rows are in the same render context as the table.
    ///
    /// This prevents removing blank lines across boundaries where merging is invalid,
    /// such as table -> blockquote row transitions or list-context changes.
    fn row_matches_table_context(
        &self,
        table_block: &crate::utils::table_utils::TableBlock,
        content_lines: &[&str],
        row_idx: usize,
    ) -> bool {
        let table_start_line = content_lines[table_block.start_line];
        let candidate_line = content_lines[row_idx];

        if Self::blockquote_depth(table_start_line) != Self::blockquote_depth(candidate_line) {
            return false;
        }

        let (_, candidate_after_blockquote) = TableUtils::extract_blockquote_prefix(candidate_line);
        let (candidate_list_prefix, _, _) = TableUtils::extract_list_prefix(candidate_after_blockquote);
        let candidate_indent = Self::indentation_width(candidate_after_blockquote);

        if let Some(list_ctx) = &table_block.list_context {
            // Table continuation rows in lists must stay continuation rows, not new list items.
            if !candidate_list_prefix.is_empty() {
                return false;
            }
            candidate_indent >= list_ctx.content_indent && candidate_indent < list_ctx.content_indent + 4
        } else {
            // Avoid crossing into list/code contexts for non-list tables.
            candidate_list_prefix.is_empty() && candidate_indent < 4
        }
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
            let header_content =
                TableUtils::extract_table_row_content(content_lines[table_block.start_line], table_block, 0);
            let expected_columns = TableUtils::count_cells_with_flavor(header_content, ctx.flavor);

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
                if self.is_table_row_line(content_lines[j])
                    && self.row_matches_table_context(table_block, content_lines, j)
                {
                    orphan_rows.push(j);
                    j += 1;
                } else {
                    break;
                }
            }

            if !orphan_rows.is_empty() {
                groups.push(OrphanedGroup {
                    table_start: table_block.start_line,
                    table_end: table_block.end_line,
                    expected_columns,
                    blank_start: bs,
                    blank_end: be,
                    row_lines: orphan_rows,
                });
            }
        }

        groups
    }

    /// Detect pipe rows that directly continue a parsed table block but may not be
    /// recognized by `table_blocks` (for example rows with inline fence markers).
    ///
    /// These rows should not be treated as standalone headerless tables (Case 2).
    fn detect_table_continuation_rows(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
        table_line_set: &HashSet<usize>,
    ) -> HashSet<usize> {
        let mut continuation_rows = HashSet::new();

        for table_block in &ctx.table_blocks {
            let mut i = table_block.end_line + 1;
            while i < content_lines.len() {
                if self.should_skip_line(ctx, i) || table_line_set.contains(&i) {
                    break;
                }
                if self.is_table_row_line(content_lines[i])
                    && self.row_matches_table_context(table_block, content_lines, i)
                {
                    continuation_rows.insert(i);
                    i += 1;
                } else {
                    break;
                }
            }
        }

        continuation_rows
    }

    /// Detect Case 2: Standalone headerless pipe content
    fn detect_headerless_tables(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
        table_line_set: &HashSet<usize>,
        orphaned_line_set: &HashSet<usize>,
        continuation_line_set: &HashSet<usize>,
    ) -> Vec<HeaderlessGroup> {
        if self.is_probable_headerless_fragment_file(ctx, content_lines) {
            return Vec::new();
        }

        let mut groups = Vec::new();
        let mut i = 0;

        while i < content_lines.len() {
            // Skip lines in skip contexts, existing tables, or orphaned groups
            if self.should_skip_line(ctx, i)
                || table_line_set.contains(&i)
                || orphaned_line_set.contains(&i)
                || continuation_line_set.contains(&i)
            {
                i += 1;
                continue;
            }

            // Look for consecutive pipe rows
            if self.is_table_row_line(content_lines[i]) {
                if Self::is_templated_pipe_line(content_lines[i]) {
                    i += 1;
                    continue;
                }

                // Suppress headerless detection for likely template-generated table fragments.
                if Self::preceded_by_template_directive(content_lines, i) {
                    i += 1;
                    while i < content_lines.len()
                        && !self.should_skip_line(ctx, i)
                        && !table_line_set.contains(&i)
                        && !orphaned_line_set.contains(&i)
                        && !continuation_line_set.contains(&i)
                        && self.is_table_row_line(content_lines[i])
                    {
                        i += 1;
                    }
                    continue;
                }

                // Suppress headerless detection for rows that likely continue an
                // existing table through sparse section-divider rows.
                if Self::preceded_by_sparse_table_context(content_lines, i) {
                    i += 1;
                    while i < content_lines.len()
                        && !self.should_skip_line(ctx, i)
                        && !table_line_set.contains(&i)
                        && !orphaned_line_set.contains(&i)
                        && !continuation_line_set.contains(&i)
                        && self.is_table_row_line(content_lines[i])
                    {
                        i += 1;
                    }
                    continue;
                }

                let start = i;
                let mut group_lines = vec![i];
                i += 1;

                while i < content_lines.len()
                    && !self.should_skip_line(ctx, i)
                    && !table_line_set.contains(&i)
                    && !orphaned_line_set.contains(&i)
                    && !continuation_line_set.contains(&i)
                    && self.is_table_row_line(content_lines[i])
                {
                    if Self::is_templated_pipe_line(content_lines[i]) {
                        break;
                    }
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

    /// Some repositories store reusable table-row snippets as standalone files
    /// (headerless by design). Suppress Case 2 warnings for those fragment files.
    fn is_probable_headerless_fragment_file(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
    ) -> bool {
        if !ctx.table_blocks.is_empty() {
            return false;
        }

        let mut row_count = 0usize;

        for (idx, line) in content_lines.iter().enumerate() {
            if self.should_skip_line(ctx, idx) {
                continue;
            }

            let content = Self::strip_blockquote_prefix(line).trim();
            if content.is_empty() {
                continue;
            }

            if Self::is_template_directive_line(content) {
                continue;
            }

            if TableUtils::is_delimiter_row(content) {
                return false;
            }

            // Allow inline template gate rows like `| {% ifversion ... %} |`.
            if Self::contains_template_marker(content) && content.contains('|') {
                continue;
            }

            if self.is_table_row_line(content) {
                let cols = TableUtils::count_cells_with_flavor(content, ctx.flavor);
                // Require 3+ columns to avoid suppressing common 2-column headerless issues.
                if cols < 3 {
                    return false;
                }
                row_count += 1;
                continue;
            }

            return false;
        }

        row_count >= 2
    }

    /// Build fix edit for a single orphaned-row group by replacing the local table block.
    fn build_orphan_group_fix(
        &self,
        ctx: &crate::lint_context::LintContext,
        content_lines: &[&str],
        group: &OrphanedGroup,
    ) -> Result<Option<Fix>, LintError> {
        if group.row_lines.is_empty() {
            return Ok(None);
        }

        let last_orphan = *group
            .row_lines
            .last()
            .expect("row_lines is non-empty after early return");

        // Be conservative: only auto-merge when orphan rows match original table width.
        let has_column_mismatch = group
            .row_lines
            .iter()
            .any(|&idx| TableUtils::count_cells_with_flavor(content_lines[idx], ctx.flavor) != group.expected_columns);
        if has_column_mismatch {
            return Ok(None);
        }

        let replacement_range = ctx.line_index.multi_line_range(group.table_start + 1, last_orphan + 1);
        let original_block = &ctx.content[replacement_range.clone()];
        let block_has_trailing_newline = original_block.ends_with('\n');

        let mut merged_table_lines: Vec<&str> = (group.table_start..=group.table_end)
            .map(|idx| content_lines[idx])
            .collect();
        merged_table_lines.extend(group.row_lines.iter().map(|&idx| content_lines[idx]));

        let mut merged_block = merged_table_lines.join("\n");
        if block_has_trailing_newline {
            merged_block.push('\n');
        }

        let block_ctx = crate::lint_context::LintContext::new(&merged_block, ctx.flavor, None);
        let mut normalized_block = self.md060_formatter.fix(&block_ctx)?;

        if !block_has_trailing_newline {
            normalized_block = normalized_block.trim_end_matches('\n').to_string();
        } else if !normalized_block.ends_with('\n') {
            normalized_block.push('\n');
        }

        let replacement = ensure_consistent_line_endings(original_block, &normalized_block);

        if replacement == original_block {
            Ok(None)
        } else {
            Ok(Some(Fix {
                range: replacement_range,
                replacement,
            }))
        }
    }
}

impl Default for MD075OrphanedTableRows {
    fn default() -> Self {
        Self {
            // MD075 should normalize merged rows even when MD060 is not explicitly enabled.
            md060_formatter: MD060TableFormat::new(true, "aligned".to_string()),
        }
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
        // Need at least 2 pipe characters for two minimal rows like:
        // a | b
        // c | d
        ctx.char_count('|') < 2
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
        let orphan_group_fixes: Vec<Option<Fix>> = orphaned_groups
            .iter()
            .map(|group| self.build_orphan_group_fix(ctx, content_lines, group))
            .collect::<Result<Vec<_>, _>>()?;
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
        let continuation_line_set = self.detect_table_continuation_rows(ctx, content_lines, &table_line_set);

        for (group, group_fix) in orphaned_groups.iter().zip(orphan_group_fixes.iter()) {
            let first_orphan = group.row_lines[0];
            let last_orphan = *group.row_lines.last().unwrap();
            let num_blanks = group.blank_end - group.blank_start + 1;

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!("Orphaned table row(s) separated from preceding table by {num_blanks} blank line(s)"),
                line: first_orphan + 1,
                column: 1,
                end_line: last_orphan + 1,
                end_column: content_lines[last_orphan].len() + 1,
                severity: Severity::Warning,
                fix: group_fix.clone(),
            });
        }

        // Case 2: Headerless pipe content
        let headerless_groups = self.detect_headerless_tables(
            ctx,
            content_lines,
            &table_line_set,
            &orphaned_line_set,
            &continuation_line_set,
        );

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
        let warnings = self.check(ctx)?;
        if warnings.iter().all(|warning| warning.fix.is_none()) {
            return Ok(ctx.content.to_string());
        }

        apply_warning_fixes(ctx.content, &warnings).map_err(LintError::FixFailed)
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
        let mut md060_config = crate::rule_config_serde::load_rule_config::<MD060Config>(_config);
        if md060_config.style == "any" {
            // MD075 should normalize merged tables by default; "any" preserves broken alignment.
            md060_config.style = "aligned".to_string();
        }
        let md013_config = crate::rule_config_serde::load_rule_config::<MD013Config>(_config);
        let md013_disabled = _config
            .global
            .disable
            .iter()
            .chain(_config.global.extend_disable.iter())
            .any(|rule| rule.trim().eq_ignore_ascii_case("MD013"));
        let formatter = MD060TableFormat::from_config_struct(md060_config, md013_config, md013_disabled);
        Box::new(Self::with_formatter(formatter))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;
    use crate::utils::fix_utils::apply_warning_fixes;

    // =========================================================================
    // Case 1: Orphaned rows after a table
    // =========================================================================

    #[test]
    fn test_orphaned_rows_after_table() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |


| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
| H1  | H2  |
| --- | --- |
| a   | b   |
| c   | d   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_no_orphan_with_text_between() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        assert!(result[0].fix.is_none());
    }

    // =========================================================================
    // Case 2: Headerless pipe content
    // =========================================================================

    #[test]
    fn test_headerless_pipe_content() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
        let content = "Just regular text.\nNo pipes here.\nOnly paragraphs.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD075OrphanedTableRows::default();
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_orphaned_rows_in_blockquote() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
        let content = "\
> | H1 | H2 |
> |----|-----|
> | a  | b   |
>
> | c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
> | H1  | H2  |
> | --- | --- |
> | a   | b   |
> | c   | d   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_table_at_end_of_document_no_orphans() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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

| H1  | H2  |
| --- | --- |
| a   | b   |
| c   | d   |

Some text after.";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_multiple_orphan_groups() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
| H1  | H2  |
| --- | --- |
| a   | b   |
| c   | d   |

| H3  | H4  |
| --- | --- |
| e   | f   |
| g   | h   |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_orphaned_rows_with_delimiter_form_new_table() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
        let content = "a | b";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_should_not_skip_two_pipes_without_outer_pipes() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
a | b
c | d";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        assert!(!rule.should_skip(&ctx));
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("without a table header"));
    }

    #[test]
    fn test_fix_capability() {
        let rule = MD075OrphanedTableRows::default();
        assert_eq!(rule.fix_capability(), FixCapability::ConditionallyFixable);
    }

    #[test]
    fn test_category() {
        let rule = MD075OrphanedTableRows::default();
        assert_eq!(rule.category(), RuleCategory::Table);
    }

    #[test]
    fn test_issue_420_exact_example() {
        // The exact example from issue #420, including inline code fence markers.
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| Value        | Description                                       |
| ------------ | ------------------------------------------------- |
| `consistent` | All code blocks must use the same style (default) |

| `fenced` | All code blocks must use fenced style (``` or ~~~) |
| `indented` | All code blocks must use indented style (4 spaces) |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Orphaned"));
        assert_eq!(result[0].line, 5);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "\
| Value        | Description                                        |
| ------------ | -------------------------------------------------- |
| `consistent` | All code blocks must use the same style (default)  |
| `fenced`     | All code blocks must use fenced style (``` or ~~~) |
| `indented`   | All code blocks must use indented style (4 spaces) |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_prose_with_double_backticks_and_pipes_not_flagged() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
Use ``a|b`` or ``c|d`` in docs.
Prefer ``x|y`` and ``z|w`` examples.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_liquid_filter_lines_not_flagged_as_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
If you encounter issues, see [Troubleshooting]({{ '/docs/troubleshooting/' | relative_url }}).
Use our [guides]({{ '/docs/installation/' | relative_url }}) for OS-specific steps.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_rows_after_template_directive_not_flagged_as_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
{% data reusables.enterprise-migration-tool.placeholder-table %}
DESTINATION | The name you want the new organization to have.
ENTERPRISE | The slug for your destination enterprise.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_templated_pipe_rows_not_flagged_as_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| Feature{%- for version in group_versions %} | {{ version }}{%- endfor %} |
|:----{%- for version in group_versions %}|:----:{%- endfor %}|
| {{ feature }} | {{ value }} |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_escaped_pipe_rows_in_table_not_flagged_as_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
Written as                             | Interpreted as
---------------------------------------|-----------------------------------------
`!foo && bar`                          | `(!foo) && bar`
<code>!foo \\|\\| bar </code>            | `(!foo) \\|\\| bar`
<code>foo \\|\\| bar && baz </code>      | <code>foo \\|\\| (bar && baz)</code>
<code>!foo && bar \\|\\| baz </code>     | <code>(!foo && bar) \\|\\| baz</code>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_rows_after_sparse_section_row_in_table_not_flagged() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
Key|Command|Command id
---|-------|----------
Search||
`kb(history.showNext)`|Next Search Term|`history.showNext`
`kb(history.showPrevious)`|Previous Search Term|`history.showPrevious`
Extensions||
`unassigned`|Update All Extensions|`workbench.extensions.action.updateAllExtensions`";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_sparse_row_without_table_context_does_not_suppress_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
Notes ||
`alpha` | `beta`
`gamma` | `delta`";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("without a table header"));
    }

    #[test]
    fn test_reusable_three_column_fragment_not_flagged_as_headerless() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
`label` | `object` | The label added or removed from the issue.
`label[name]` | `string` | The name of the label.
`label[color]` | `string` | The hex color code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_orphan_detection_does_not_cross_blockquote_context() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

> | c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();

        assert_eq!(orphan_warnings.len(), 0);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_orphan_fix_does_not_cross_list_context() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
- | H1 | H2 |
  |----|-----|
  | a  | b   |

| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        let orphan_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Orphaned")).collect();

        assert_eq!(orphan_warnings.len(), 0);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_fix_normalizes_only_merged_table() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |

| Name | Age |
|---|---|
|alice|30|";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("| H1  | H2  |"));
        assert!(fixed.contains("| c   | d   |"));
        // Unrelated second table should keep original compact formatting.
        assert!(fixed.contains("|---|---|"));
        assert!(fixed.contains("|alice|30|"));
    }

    #[test]
    fn test_html_comment_pipe_rows_ignored() {
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
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
        let rule = MD075OrphanedTableRows::default();
        let content = "\
# Before

| H1 | H2 |
|----|-----|
| a  | b   |

| c  | d   |

# After";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let expected = "\
# Before

| H1  | H2  |
| --- | --- |
| a   | b   |
| c   | d   |

# After";

        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().unwrap();
        assert!(fix.range.start > 0);
        assert!(fix.range.end < content.len());

        let cli_fixed = rule.fix(&ctx).unwrap();
        assert_eq!(cli_fixed, expected);

        let lsp_fixed = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(lsp_fixed, expected);
        assert_eq!(lsp_fixed, cli_fixed);
    }

    #[test]
    fn test_fix_range_multiple_blanks() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
# Before

| H1 | H2 |
|----|-----|
| a  | b   |


| c  | d   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let expected = "\
# Before

| H1  | H2  |
| --- | --- |
| a   | b   |
| c   | d   |";

        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().unwrap();
        assert!(fix.range.start > 0);
        assert_eq!(fix.range.end, content.len());

        let cli_fixed = rule.fix(&ctx).unwrap();
        assert_eq!(cli_fixed, expected);

        let lsp_fixed = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(lsp_fixed, expected);
        assert_eq!(lsp_fixed, cli_fixed);
    }

    #[test]
    fn test_warning_fixes_match_rule_fix_for_multiple_orphan_groups() {
        let rule = MD075OrphanedTableRows::default();
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
        let warnings = rule.check(&ctx).unwrap();

        let orphan_warnings: Vec<_> = warnings.iter().filter(|w| w.message.contains("Orphaned")).collect();
        assert_eq!(orphan_warnings.len(), 2);

        let lsp_fixed = apply_warning_fixes(content, &warnings).unwrap();
        let cli_fixed = rule.fix(&ctx).unwrap();

        assert_eq!(lsp_fixed, cli_fixed);
        assert_ne!(cli_fixed, content);
    }

    #[test]
    fn test_issue_420_fix_is_idempotent() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| Value        | Description                                       |
| ------------ | ------------------------------------------------- |
| `consistent` | All code blocks must use the same style (default) |

| `fenced` | All code blocks must use fenced style (``` or ~~~) |
| `indented` | All code blocks must use indented style (4 spaces) |";

        let initial_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_once = rule.fix(&initial_ctx).unwrap();

        let fixed_ctx = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let warnings_after_fix = rule.check(&fixed_ctx).unwrap();
        assert_eq!(warnings_after_fix.len(), 0);

        let fixed_twice = rule.fix(&fixed_ctx).unwrap();
        assert_eq!(fixed_twice, fixed_once);
    }

    #[test]
    fn test_from_config_respects_md060_compact_style_for_merged_table() {
        let mut config = crate::config::Config::default();
        let mut md060_rule_config = crate::config::RuleConfig::default();
        md060_rule_config
            .values
            .insert("style".to_string(), toml::Value::String("compact".to_string()));
        config.rules.insert("MD060".to_string(), md060_rule_config);

        let rule = <MD075OrphanedTableRows as Rule>::from_config(&config);
        let content = "\
| H1 | H2 |
|----|-----|
| long value | b |

| c | d |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "\
| H1 | H2 |
| ---- | ----- |
| long value | b |
| c | d |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_from_config_honors_extend_disable_for_md013_case_insensitive() {
        let mut config_enabled = crate::config::Config::default();

        let mut md060_rule_config = crate::config::RuleConfig::default();
        md060_rule_config
            .values
            .insert("style".to_string(), toml::Value::String("aligned".to_string()));
        config_enabled.rules.insert("MD060".to_string(), md060_rule_config);

        let mut md013_rule_config = crate::config::RuleConfig::default();
        md013_rule_config
            .values
            .insert("line-length".to_string(), toml::Value::Integer(40));
        md013_rule_config
            .values
            .insert("tables".to_string(), toml::Value::Boolean(true));
        config_enabled.rules.insert("MD013".to_string(), md013_rule_config);

        let mut config_disabled = config_enabled.clone();
        config_disabled.global.extend_disable.push("md013".to_string());

        let rule_enabled = <MD075OrphanedTableRows as Rule>::from_config(&config_enabled);
        let rule_disabled = <MD075OrphanedTableRows as Rule>::from_config(&config_disabled);

        let content = "\
| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |
|---|---|---|
| data | data | data |

| more | more | more |";

        let ctx_enabled = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_enabled = rule_enabled.fix(&ctx_enabled).unwrap();
        let enabled_lines: Vec<&str> = fixed_enabled.lines().collect();
        assert!(
            enabled_lines.len() >= 4,
            "Expected merged table to contain at least 4 lines"
        );
        assert_ne!(
            enabled_lines[0].len(),
            enabled_lines[1].len(),
            "With MD013 active and inherited max-width, wide merged table should auto-compact"
        );

        let ctx_disabled = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_disabled = rule_disabled.fix(&ctx_disabled).unwrap();
        let disabled_lines: Vec<&str> = fixed_disabled.lines().collect();
        assert!(
            disabled_lines.len() >= 4,
            "Expected merged table to contain at least 4 lines"
        );
        assert_eq!(
            disabled_lines[0].len(),
            disabled_lines[1].len(),
            "With MD013 disabled via extend-disable, inherited max-width should be unlimited (aligned table)"
        );
        assert_eq!(
            disabled_lines[1].len(),
            disabled_lines[2].len(),
            "Aligned table rows should share the same width"
        );
    }

    fn all_flavors() -> [MarkdownFlavor; 6] {
        [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::MDX,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
            MarkdownFlavor::Kramdown,
        ]
    }

    fn make_row(prefix: &str, cols: usize) -> String {
        let cells: Vec<String> = (1..=cols).map(|idx| format!("{prefix}{idx}")).collect();
        format!("| {} |", cells.join(" | "))
    }

    #[test]
    fn test_issue_420_orphan_fix_matrix_all_flavors() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| Value        | Description                                       |
| ------------ | ------------------------------------------------- |
| `consistent` | All code blocks must use the same style (default) |

| `fenced` | All code blocks must use fenced style (``` or ~~~) |
| `indented` | All code blocks must use indented style (4 spaces) |";

        for flavor in all_flavors() {
            let ctx = LintContext::new(content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "Expected one warning for flavor {}", flavor.name());
            assert!(
                warnings[0].fix.is_some(),
                "Expected fixable orphan warning for flavor {}",
                flavor.name()
            );
            let fixed = rule.fix(&ctx).unwrap();
            let fixed_ctx = LintContext::new(&fixed, flavor, None);
            assert!(
                rule.check(&fixed_ctx).unwrap().is_empty(),
                "Expected no remaining MD075 warnings after fix for flavor {}",
                flavor.name()
            );
        }
    }

    #[test]
    fn test_column_mismatch_orphan_not_fixable_matrix_all_flavors() {
        let rule = MD075OrphanedTableRows::default();
        let content = "\
| H1 | H2 | H3 |
| --- | --- | --- |
| a | b | c |

| d | e |";

        for flavor in all_flavors() {
            let ctx = LintContext::new(content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(
                warnings.len(),
                1,
                "Expected one mismatch warning for flavor {}",
                flavor.name()
            );
            assert!(
                warnings[0].fix.is_none(),
                "Mismatch must never auto-fix for flavor {}",
                flavor.name()
            );
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                content,
                "Mismatch fix must be no-op for flavor {}",
                flavor.name()
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_md075_fix_is_idempotent_for_orphaned_rows(
            cols in 2usize..6,
            base_rows in 1usize..5,
            orphan_rows in 1usize..4,
            blank_lines in 1usize..4,
            flavor in prop::sample::select(all_flavors().to_vec()),
        ) {
            let rule = MD075OrphanedTableRows::default();

            let mut lines = Vec::new();
            lines.push(make_row("H", cols));
            lines.push(format!("| {} |", (0..cols).map(|_| "---").collect::<Vec<_>>().join(" | ")));
            for idx in 0..base_rows {
                lines.push(make_row(&format!("r{}c", idx + 1), cols));
            }
            for _ in 0..blank_lines {
                lines.push(String::new());
            }
            for idx in 0..orphan_rows {
                lines.push(make_row(&format!("o{}c", idx + 1), cols));
            }

            let content = lines.join("\n");
            let ctx1 = LintContext::new(&content, flavor, None);
            let fixed_once = rule.fix(&ctx1).unwrap();

            let ctx2 = LintContext::new(&fixed_once, flavor, None);
            let fixed_twice = rule.fix(&ctx2).unwrap();

            prop_assert_eq!(fixed_once.as_str(), fixed_twice.as_str());
            prop_assert!(
                rule.check(&ctx2).unwrap().is_empty(),
                "MD075 warnings remained after fix in flavor {}",
                flavor.name()
            );
        }

        #[test]
        fn prop_md075_cli_lsp_fix_consistency(
            cols in 2usize..6,
            base_rows in 1usize..4,
            orphan_rows in 1usize..3,
            blank_lines in 1usize..3,
            flavor in prop::sample::select(all_flavors().to_vec()),
        ) {
            let rule = MD075OrphanedTableRows::default();

            let mut lines = Vec::new();
            lines.push(make_row("H", cols));
            lines.push(format!("| {} |", (0..cols).map(|_| "---").collect::<Vec<_>>().join(" | ")));
            for idx in 0..base_rows {
                lines.push(make_row(&format!("r{}c", idx + 1), cols));
            }
            for _ in 0..blank_lines {
                lines.push(String::new());
            }
            for idx in 0..orphan_rows {
                lines.push(make_row(&format!("o{}c", idx + 1), cols));
            }
            let content = lines.join("\n");

            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            prop_assert!(
                warnings.iter().any(|w| w.message.contains("Orphaned")),
                "Expected orphan warning for flavor {}",
                flavor.name()
            );

            let lsp_fixed = apply_warning_fixes(&content, &warnings).unwrap();
            let cli_fixed = rule.fix(&ctx).unwrap();
            prop_assert_eq!(lsp_fixed, cli_fixed);
        }

        #[test]
        fn prop_md075_column_mismatch_is_never_fixable(
            base_cols in 2usize..6,
            orphan_cols in 1usize..6,
            blank_lines in 1usize..4,
            flavor in prop::sample::select(all_flavors().to_vec()),
        ) {
            prop_assume!(base_cols != orphan_cols);
            let rule = MD075OrphanedTableRows::default();

            let mut lines = vec![
                make_row("H", base_cols),
                format!("| {} |", (0..base_cols).map(|_| "---").collect::<Vec<_>>().join(" | ")),
                make_row("r", base_cols),
            ];
            for _ in 0..blank_lines {
                lines.push(String::new());
            }
            lines.push(make_row("o", orphan_cols));

            let content = lines.join("\n");
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            prop_assert_eq!(warnings.len(), 1);
            prop_assert!(warnings[0].fix.is_none());
            prop_assert_eq!(rule.fix(&ctx).unwrap(), content);
        }
    }
}
