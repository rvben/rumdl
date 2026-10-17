//! Opt-in sorting of table body rows by a named column.

#[cfg(test)]
mod tests;

use pulldown_cmark::{BrokenLink, CowStr, Event, Options, Parser};
use serde::{Deserialize, Serialize};

use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::fix_utils::{apply_warning_fixes, filter_warnings_by_inline_config};
use crate::utils::parser_options::rumdl_parser_options;
use crate::utils::table_utils::TableUtils;
use crate::utils::text_reflow::normalize_reference_label;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum SortOrder {
    #[default]
    #[serde(rename = "asc", alias = "ASC")]
    Asc,
    #[serde(rename = "desc", alias = "DESC")]
    Desc,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MD096Config {
    // Config::apply_per_rule_enabled folds this into the global rule lists.
    #[serde(default)]
    pub(super) enabled: bool,
    /// No implicit first column: enabling the rule alone must not reorder tables.
    #[serde(default)]
    pub(super) column: Option<String>,
    #[serde(default)]
    pub(super) order: SortOrder,
}

impl RuleConfig for MD096Config {
    const RULE_NAME: &'static str = "MD096";
}

#[derive(Debug, Clone, Default)]
pub struct MD096TableSort {
    config: MD096Config,
}

impl MD096TableSort {
    fn from_config_struct(config: MD096Config) -> Self {
        Self { config }
    }

    /// Parse inline content in a paragraph, never as a heading, list, reference
    /// definition or HTML block. The sentinel starts the paragraph and is removed
    /// after collecting text, even when the parser splits it across events.
    fn sort_key(cell: &str, ctx: &LintContext) -> String {
        let source = format!("x {}", cell.trim());
        let mut options = rumdl_parser_options();
        options.remove(Options::ENABLE_SMART_PUNCTUATION | Options::ENABLE_HEADING_ATTRIBUTES);
        if ctx.flavor != MarkdownFlavor::Obsidian {
            options.remove(Options::ENABLE_WIKILINKS);
        }
        let resolve = |link: BrokenLink<'_>| {
            ctx.reference_definition(&normalize_reference_label(&link.reference))
                .map(|_| (CowStr::Borrowed(""), CowStr::Borrowed("")))
        };
        let mut text = String::new();
        for event in Parser::new_with_broken_link_callback(&source, options, Some(resolve)) {
            match event {
                Event::Text(value) | Event::InlineMath(value) | Event::DisplayMath(value) => text.push_str(&value),
                // GFM removes a pipe's table escape even inside a code span.
                Event::Code(value) => {
                    if ctx.flavor == MarkdownFlavor::MkDocs {
                        text.push_str(&value);
                    } else {
                        text.push_str(&value.replace("\\|", "|"));
                    }
                }
                Event::SoftBreak | Event::HardBreak => text.push(' '),
                _ => {}
            }
        }
        // The parser drops a trailing space when the cell is empty. Strip only
        // the sentinel character, then trim the remaining separator and text.
        text.strip_prefix('x').unwrap_or(&text).trim().to_lowercase()
    }
}

impl Rule for MD096TableSort {
    fn name(&self) -> &'static str {
        "MD096"
    }

    fn description(&self) -> &'static str {
        "Table rows should be sorted by a named column"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Table
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.likely_has_tables()
            || self
                .config
                .column
                .as_ref()
                .is_none_or(|column| column.trim().is_empty())
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let Some(column) = self.config.column.as_ref().filter(|column| !column.trim().is_empty()) else {
            return Ok(Vec::new());
        };
        let column_key = column.trim().to_lowercase();
        let lines = ctx.raw_lines();
        let mut warnings = Vec::new();

        for table in ctx.table_blocks.iter().filter(|table| table.parser_confirmed) {
            if table.content_lines.len() < 2 || table.content_lines.windows(2).any(|pair| pair[1] != pair[0] + 1) {
                continue;
            }
            let table_span = ctx.line_span_byte_range(table.header_line + 1, table.end_line + 1);
            if ctx
                .parsed_table_range_at_line(table.header_line + 1)
                .is_some_and(|(_, end)| end > table_span.end)
            {
                // A GFM table can continue with a row containing no pipes.
                // The scanner does not own that row: do not sort a subset.
                continue;
            }
            let header = TableUtils::extract_table_row_content(lines[table.header_line], table, 0);
            let headers = TableUtils::split_table_row_for_rendering(header, ctx.flavor);
            let mut matches = headers
                .iter()
                .enumerate()
                .filter(|(_, cell)| Self::sort_key(cell, ctx) == column_key);
            let Some((column_index, _)) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                continue;
            }

            let mut rows = Vec::with_capacity(table.content_lines.len());
            let mut prefix = None;
            for (index, &line_index) in table.content_lines.iter().enumerate() {
                let line = lines[line_index];
                let content = TableUtils::extract_table_row_content(line, table, index + 2);
                debug_assert!(line.ends_with(content), "table row content must be a source suffix");
                let row_prefix = &line[..line.len() - content.len()];
                // Moving a lazy/container prefix to a different position can
                // change the parse. Leave mixed layouts to the author.
                if prefix.is_some_and(|prefix| prefix != row_prefix) {
                    break;
                }
                prefix = Some(row_prefix);
                let cells = TableUtils::split_table_row_for_rendering(content, ctx.flavor);
                if cells.len() != headers.len() {
                    break;
                }
                rows.push((line_index, Self::sort_key(&cells[column_index], ctx)));
            }
            if rows.len() != table.content_lines.len() {
                continue;
            }
            let compare = |a: &(usize, String), b: &(usize, String)| match self.config.order {
                SortOrder::Asc => a.1.cmp(&b.1),
                SortOrder::Desc => b.1.cmp(&a.1),
            };
            if rows.windows(2).all(|pair| !compare(&pair[0], &pair[1]).is_gt()) {
                continue;
            }
            // Stable in both directions: reversing an ascending result would
            // reverse equal keys as well.
            rows.sort_by(compare);
            let mut replacement = String::new();
            for (&destination, &(source, _)) in table.content_lines.iter().zip(&rows) {
                let source_range = ctx.line_content_byte_range(source + 1);
                let destination_content = ctx.line_content_byte_range(destination + 1);
                let destination_span = ctx.line_span_byte_range(destination + 1, destination + 1);
                replacement.push_str(&ctx.content[source_range]);
                // Line endings belong to positions, not rows. In particular,
                // the last row may have no newline and may move to the middle.
                replacement.push_str(&ctx.content[destination_content.end..destination_span.end]);
            }
            let first = table.content_lines[0] + 1;
            let last = table.content_lines[table.content_lines.len() - 1] + 1;
            // Comments can carry instructions whose scope changes when moved.
            // Keep the raw finding for suppression accounting (MD087), but do
            // not move a table containing comments automatically.
            let table_range = ctx.line_span_byte_range(table.header_line + 1, last);
            let comments = ctx.html_comment_ranges();
            let next_comment = comments.partition_point(|comment| comment.end <= table_range.start);
            let has_comments = comments
                .get(next_comment)
                .is_some_and(|comment| comment.start < table_range.end);
            let direction = match self.config.order {
                SortOrder::Asc => "ascending",
                SortOrder::Desc => "descending",
            };
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!("Table rows should be sorted by '{column}' ({direction})"),
                // A table-wide diagnostic lets the normal suppression pipeline
                // honor a disable on any row, including the header/delimiter,
                // and lets MD087 observe that suppression.
                line: table.header_line + 1,
                column: 1,
                end_line: last,
                end_column: lines[last - 1].chars().count() + 1,
                severity: Severity::Warning,
                fix: (!has_comments).then(|| Fix::new(ctx.line_span_byte_range(first, last), replacement)),
            });
        }
        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::ConditionallyFixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = filter_warnings_by_inline_config(self.check(ctx)?, ctx.inline_config(), self.name());
        apply_warning_fixes(ctx.content, &warnings).map_err(LintError::FixFailed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD096Config);
}
