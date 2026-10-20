/// Rule MD031: Blank lines around fenced code blocks
///
/// See [docs/md031.md](../../docs/md031.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::blank_lines::is_blank_or_comment_only;
use crate::utils::calculate_indentation_width_default;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_attr_list::is_block_attribute_line;
use crate::utils::pandoc;
use crate::utils::range_utils::calculate_line_range;
use serde::{Deserialize, Serialize};

/// Configuration for MD031 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD031Config {
    /// Whether to require blank lines around code blocks in lists
    #[serde(default = "default_list_items")]
    pub list_items: bool,
}

impl Default for MD031Config {
    fn default() -> Self {
        Self {
            list_items: default_list_items(),
        }
    }
}

fn default_list_items() -> bool {
    true
}

impl RuleConfig for MD031Config {
    const RULE_NAME: &'static str = "MD031";
}

/// Rule MD031: Fenced code blocks should be surrounded by blank lines
#[derive(Clone, Default)]
pub struct MD031BlanksAroundFences {
    config: MD031Config,
}

/// Memoized answers to the rule's textual list-membership check. A backward
/// scan stops at either a decisive line or an earlier answer, then shares that
/// answer with every continuation it crossed. Each source line is read at most
/// once even when fence ranges arrive out of order.
#[derive(Default)]
struct ListMembership {
    lines: Vec<Option<bool>>,
}

impl ListMembership {
    fn contains(&mut self, line_index: usize, lines: &[&str]) -> bool {
        if self.lines.len() <= line_index {
            self.lines.resize(line_index + 1, None);
        }
        let mut first = line_index;
        let in_list = loop {
            if let Some(in_list) = self.lines[first] {
                break in_list;
            }
            let line = lines[first];
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                break false;
            }
            if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let mut chars = trimmed.chars().skip_while(char::is_ascii_digit);
                if let Some(next) = chars.next()
                    && (next == '.' || next == ')')
                    && chars.next() == Some(' ')
                {
                    break true;
                }
            }
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                break true;
            }
            if calculate_indentation_width_default(line) < 3 || first == 0 {
                break false;
            }
            first -= 1;
        };
        self.lines[first..=line_index].fill(Some(in_list));
        in_list
    }
}

impl MD031BlanksAroundFences {
    pub fn new(list_items: bool) -> Self {
        Self {
            config: MD031Config { list_items },
        }
    }

    pub fn from_config_struct(config: MD031Config) -> Self {
        Self { config }
    }

    /// Check if a line is effectively empty (blank, a line holding nothing but
    /// HTML comments, or an empty blockquote line like ">")
    /// Uses the pre-computed blockquote info from LintContext for accurate detection
    fn is_effectively_empty_line(line_idx: usize, lines: &[&str], ctx: &crate::lint_context::LintContext) -> bool {
        let line = lines.get(line_idx).unwrap_or(&"");

        // A blank line, or one contributing nothing but comments (#866)
        if is_blank_or_comment_only(line) {
            return true;
        }

        // Check if this is an empty blockquote line (like ">", "> ", ">>", etc.)
        if let Some(line_info) = ctx.lines.get(line_idx)
            && let Some(ref bq) = line_info.blockquote
        {
            // If the blockquote content is empty, this is effectively a blank line
            return bq.content.trim().is_empty();
        }

        false
    }

    /// Check if the current line is immediately after frontmatter (prev line is closing delimiter)
    fn is_right_after_frontmatter(line_index: usize, ctx: &crate::lint_context::LintContext) -> bool {
        line_index > 0
            && ctx.lines.get(line_index - 1).is_some_and(|info| info.in_front_matter)
            && ctx.lines.get(line_index).is_some_and(|info| !info.in_front_matter)
    }

    /// Derive fenced code block line ranges from pre-computed code_block_details.
    ///
    /// Returns (opening_line_idx, last_line_idx, closed) for each fenced code block.
    /// The indices are 0-based line numbers, and `closed` says whether the last
    /// line is a closing fence. A block the document or its container ends
    /// first has no closing line to put a blank line after.
    fn fenced_block_line_ranges(ctx: &crate::lint_context::LintContext) -> Vec<(usize, usize, bool)> {
        let ambiguity_limit = super::code_fence_utils::fence_ambiguity_limit(ctx);

        ctx.code_block_details
            .iter()
            .filter(|d| d.is_fenced && !ctx.is_inside_template_code(d.start) && !ctx.is_inside_mdx_code(d.start))
            .filter(|d| ambiguity_limit.is_none_or(|limit| d.start < limit))
            .map(|detail| {
                // Convert start byte offset to line index
                let start_line = ctx
                    .line_offsets
                    .partition_point(|&off| off <= detail.start)
                    .saturating_sub(1);

                // Convert end byte offset to line index
                let end_byte = if detail.end > 0 { detail.end - 1 } else { 0 };
                let end_line = ctx
                    .line_offsets
                    .partition_point(|&off| off <= end_byte)
                    .saturating_sub(1);

                (start_line, end_line, detail.closed)
            })
            .collect()
    }

    /// The 0-based lines that are the first line of content of a list item whose
    /// marker has nothing after it on its line (`-`, `1.`, `> -`).
    ///
    /// A list item may begin with at most one blank line, so a blank line
    /// inserted between such a marker and its content leaves the item empty and
    /// puts the content outside it. The parser settles which lines these are:
    /// the textual shape of the line above cannot tell an item from a setext
    /// underline (`a\n-`), a marker inside indented code, or the marker of an
    /// outer item that has an inner one after it (`- -`).
    fn bare_item_content_lines(ctx: &crate::lint_context::LintContext) -> std::collections::HashSet<usize> {
        let content = ctx.content;
        let line_of = |offset: usize| ctx.line_offsets.partition_point(|&off| off <= offset).saturating_sub(1);
        let mut found = std::collections::HashSet::new();

        let items = ctx
            .parser_blocks()
            .events
            .iter()
            .filter(|(kind, _)| *kind == crate::lint_context::document_parser::blocks::BlockKind::ItemStart)
            .map(|(_, range)| range.clone());

        for range in items {
            let marker_line = line_of(range.start);
            let Some(&next_line_start) = ctx.line_offsets.get(marker_line + 1) else {
                continue;
            };
            let line_end = next_line_start.min(content.len());
            let after_marker = content[range.start..line_end]
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .get(1..)
                .unwrap_or("");
            if after_marker.trim().is_empty() && range.end > next_line_start {
                found.insert(marker_line + 1);
            }
        }
        found
    }

    /// Whether the blank line a warning asks for above `line_idx` would be the
    /// first line of a list item, which it cannot be without emptying the item.
    fn begins_bare_item(
        ctx: &crate::lint_context::LintContext,
        cache: &std::cell::OnceCell<std::collections::HashSet<usize>>,
        line_idx: usize,
    ) -> bool {
        let Some(above) = line_idx.checked_sub(1).and_then(|i| ctx.raw_lines().get(i).copied()) else {
            return false;
        };
        // Only a line ending in a marker character can be a bare marker; this
        // keeps documents without one from paying for the parse.
        above.trim_end().ends_with(['-', '+', '*', '.', ')'])
            && cache
                .get_or_init(|| Self::bare_item_content_lines(ctx))
                .contains(&line_idx)
    }

    /// Convert colon fence byte ranges from LintContext into (opener_line, closer_line) pairs.
    fn colon_fence_line_ranges(ctx: &crate::lint_context::LintContext) -> Vec<(usize, usize)> {
        ctx.colon_fence_details()
            .iter()
            .map(|fence| {
                let (start, end) = (fence.start, fence.end);
                let start_line = ctx.line_offsets.partition_point(|&off| off <= start).saturating_sub(1);
                let end_byte = if end > 0 { end - 1 } else { 0 };
                let end_line = ctx
                    .line_offsets
                    .partition_point(|&off| off <= end_byte)
                    .saturating_sub(1);
                (start_line, end_line)
            })
            .collect()
    }

    /// Convert MyST directive byte ranges from LintContext into (opener_line, closer_line) pairs.
    fn myst_directive_line_ranges(ctx: &crate::lint_context::LintContext) -> Vec<(usize, usize)> {
        ctx.myst_directive_ranges()
            .iter()
            .map(|&(start, end)| {
                let start_line = ctx.line_offsets.partition_point(|&off| off <= start).saturating_sub(1);
                let end_byte = if end > 0 { end - 1 } else { 0 };
                let end_line = ctx
                    .line_offsets
                    .partition_point(|&off| off <= end_byte)
                    .saturating_sub(1);
                (start_line, end_line)
            })
            .collect()
    }
}

impl Rule for MD031BlanksAroundFences {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(31))
    }

    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;
        let is_pandoc = ctx.flavor.is_pandoc_compatible();

        // Detect fenced code blocks using pulldown-cmark (handles list-indented fences correctly)
        let fenced_blocks = Self::fenced_block_line_ranges(ctx);
        // Built on first use: see `begins_bare_item`.
        let bare_items = std::cell::OnceCell::new();
        let mut list_membership = ListMembership::default();
        let mut should_require_blank_line = |line| self.config.list_items || !list_membership.contains(line, lines);

        // Helper to check if a line is a Pandoc/Quarto div marker (opening or closing)
        let is_pandoc_div_marker =
            |line: &str| -> bool { is_pandoc && (pandoc::is_div_open(line) || pandoc::is_div_close(line)) };

        // Check blank lines around each fenced code block
        for (opening_line, closing_line, closed) in &fenced_blocks {
            // Skip fenced code blocks in skipped regions (comments, front-matter, etc.)
            if ctx.line_info(*opening_line + 1).is_some_and(|info| {
                info.in_html_comment
                    || info.in_mdx_comment
                    || info.in_front_matter
                    || info.in_mkdocstrings
                    || info.in_jsx_block
                    || info.in_kramdown_extension_block
                    || info.in_pymdown_block
            }) {
                continue;
            }

            // Check for blank line before opening fence
            // Skip if right after frontmatter
            // Skip if right after a Pandoc/Quarto div marker in Pandoc-compatible flavor
            // Use is_effectively_empty_line to handle blockquote blank lines (issue #284)
            let prev_line_is_pandoc_marker = *opening_line > 0 && is_pandoc_div_marker(lines[*opening_line - 1]);
            if *opening_line > 0
                && !Self::is_effectively_empty_line(*opening_line - 1, lines, ctx)
                && !Self::is_right_after_frontmatter(*opening_line, ctx)
                && !prev_line_is_pandoc_marker
                && should_require_blank_line(*opening_line)
                && !Self::begins_bare_item(ctx, &bare_items, *opening_line)
            {
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(*opening_line + 1, lines[*opening_line]);

                let bq_prefix = ctx.blockquote_prefix_for_blank_line(*opening_line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "No blank line before fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(
                        ctx.line_column_byte_range_with_length(*opening_line + 1, 1, 0),
                        format!("{bq_prefix}\n"),
                    )),
                });
            }

            // Check for blank line after closing fence
            // Allow block attribute lists attached to the fence (Kramdown IALs in any
            // flavor, Hugo/MkDocs bare attr lists when the flavor enables them)
            // Skip if followed by a Pandoc/Quarto div marker in Pandoc-compatible flavor
            // Use is_effectively_empty_line to handle blockquote blank lines (issue #284)
            let next_line_is_pandoc_marker =
                *closing_line + 1 < lines.len() && is_pandoc_div_marker(lines[*closing_line + 1]);
            if *closed
                && *closing_line + 1 < lines.len()
                && !Self::is_effectively_empty_line(*closing_line + 1, lines, ctx)
                && !is_block_attribute_line(lines[*closing_line + 1], ctx.flavor)
                && !next_line_is_pandoc_marker
                && should_require_blank_line(*closing_line)
            {
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(*closing_line + 1, lines[*closing_line]);

                let bq_prefix = ctx.blockquote_prefix_for_blank_line(*closing_line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "No blank line after fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(
                        ctx.line_column_byte_range_with_length(*closing_line + 2, 1, 0),
                        format!("{bq_prefix}\n"),
                    )),
                });
            }
        }

        // Enforce blank lines around Azure DevOps colon code fences.
        if ctx.flavor.supports_colon_code_fences() {
            let colon_blocks = Self::colon_fence_line_ranges(ctx);
            for (opening_line, closing_line) in &colon_blocks {
                // Check for blank line before opener
                if *opening_line > 0
                    && !Self::is_effectively_empty_line(*opening_line - 1, lines, ctx)
                    && !Self::is_right_after_frontmatter(*opening_line, ctx)
                    && should_require_blank_line(*opening_line)
                    && !Self::begins_bare_item(ctx, &bare_items, *opening_line)
                {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(*opening_line + 1, lines[*opening_line]);
                    let bq_prefix = ctx.blockquote_prefix_for_blank_line(*opening_line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "No blank line before colon code fence".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(*opening_line + 1, 1, 0),
                            format!("{bq_prefix}\n"),
                        )),
                    });
                }

                // Check for blank line after closer
                if *closing_line + 1 < lines.len()
                    && !Self::is_effectively_empty_line(*closing_line + 1, lines, ctx)
                    && should_require_blank_line(*closing_line)
                {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(*closing_line + 1, lines[*closing_line]);
                    let bq_prefix = ctx.blockquote_prefix_for_blank_line(*closing_line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "No blank line after colon code fence".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(*closing_line + 2, 1, 0),
                            format!("{bq_prefix}\n"),
                        )),
                    });
                }
            }
        }

        // Enforce blank lines around MyST colon directives (:::{name} ... :::)
        if ctx.flavor.supports_myst_directives() {
            let myst_blocks = Self::myst_directive_line_ranges(ctx);
            for (opening_line, closing_line) in &myst_blocks {
                // Check for blank line before opener
                if *opening_line > 0
                    && !Self::is_effectively_empty_line(*opening_line - 1, lines, ctx)
                    && !Self::is_right_after_frontmatter(*opening_line, ctx)
                    && should_require_blank_line(*opening_line)
                    && !Self::begins_bare_item(ctx, &bare_items, *opening_line)
                {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(*opening_line + 1, lines[*opening_line]);
                    let bq_prefix = ctx.blockquote_prefix_for_blank_line(*opening_line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "No blank line before MyST directive".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(*opening_line + 1, 1, 0),
                            format!("{bq_prefix}\n"),
                        )),
                    });
                }

                // Check for blank line after closer
                if *closing_line + 1 < lines.len()
                    && !Self::is_effectively_empty_line(*closing_line + 1, lines, ctx)
                    && should_require_blank_line(*closing_line)
                {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(*closing_line + 1, lines[*closing_line]);
                    let bq_prefix = ctx.blockquote_prefix_for_blank_line(*closing_line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "No blank line after MyST directive".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(*closing_line + 2, 1, 0),
                            format!("{bq_prefix}\n"),
                        )),
                    });
                }
            }
        }

        // Handle MkDocs admonitions separately
        if is_mkdocs {
            // Walk the filtered fence ranges once alongside the source lines.
            // Sort a separate copy to preserve diagnostic order above, and keep
            // the furthest end so overlapping ranges retain union semantics.
            let mut fences = fenced_blocks.clone();
            fences.sort_unstable_by_key(|&(start, _, _)| start);
            let mut fences = fences.into_iter().peekable();
            let mut fenced_through = None;
            let mut in_admonition = false;
            let mut admonition_indent = 0;
            let mut i = 0;

            while i < lines.len() {
                let line = lines[i];

                // Skip if this line is inside a fenced code block
                while let Some(&(start, end, _)) = fences.peek() {
                    if start > i {
                        break;
                    }
                    fenced_through = Some(fenced_through.map_or(end, |previous: usize| previous.max(end)));
                    fences.next();
                }
                if fenced_through.is_some_and(|end| i <= end) {
                    i += 1;
                    continue;
                }

                // Skip if this line is inside a PyMdown block
                if ctx.line_info(i + 1).is_some_and(|info| info.in_pymdown_block) {
                    i += 1;
                    continue;
                }

                // Check for MkDocs admonition start
                if mkdocs_admonitions::is_admonition_start(line) {
                    // Check for blank line before admonition
                    if i > 0
                        && !Self::is_effectively_empty_line(i - 1, lines, ctx)
                        && !Self::is_right_after_frontmatter(i, ctx)
                        && should_require_blank_line(i)
                        && !Self::begins_bare_item(ctx, &bare_items, i)
                    {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                        let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "No blank line before admonition block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix::new(
                                ctx.line_column_byte_range_with_length(i + 1, 1, 0),
                                format!("{bq_prefix}\n"),
                            )),
                        });
                    }

                    in_admonition = true;
                    admonition_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);
                    i += 1;
                    continue;
                }

                // Check if we're exiting an admonition
                if in_admonition
                    && !line.trim().is_empty()
                    && !mkdocs_admonitions::is_admonition_content(line, admonition_indent)
                {
                    in_admonition = false;

                    // Check for blank line after admonition
                    // We need a blank line between the admonition content and the current line
                    // Check if the previous line (i-1) is a blank line separator
                    if i > 0 && !Self::is_effectively_empty_line(i - 1, lines, ctx) && should_require_blank_line(i - 1)
                    {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                        let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "No blank line after admonition block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix::new(
                                ctx.line_column_byte_range_with_length(i + 1, 1, 0),
                                format!("{bq_prefix}\n"),
                            )),
                        });
                    }

                    admonition_indent = 0;
                }

                i += 1;
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if ctx.content.is_empty() {
            return true;
        }
        let has_fences = ctx.likely_has_code() || ctx.has_char('~');
        let has_mkdocs_admonitions = ctx.flavor == crate::config::MarkdownFlavor::MkDocs && ctx.content.contains("!!!");
        let has_colon_fences = ctx.flavor.supports_colon_code_fences() && ctx.content.contains(":::");
        let has_myst_directives = ctx.flavor.supports_myst_directives() && ctx.content.contains(":::");
        !has_fences && !has_mkdocs_admonitions && !has_colon_fences && !has_myst_directives
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD031Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn list_membership_preserves_textual_boundaries_in_any_query_order() {
        let cases = [
            ("   orphan", false),
            ("- item", true),
            ("   continuation", true),
            ("\tcontinued", true),
            ("", false),
            ("   after blank", false),
            ("1. ordered", true),
            ("   2)\tcontinuation", true),
            ("2)\tnot a marker", false),
            ("   after non-marker", false),
            ("22) marker", true),
            ("  two-space indent", false),
            ("* bullet", true),
            ("\u{a0}", false),
            ("\u{a0}+ bullet", true),
            ("   continuation", true),
            ("> - quoted marker", false),
            ("   tail", false),
        ];
        let lines: Vec<_> = cases.iter().map(|&(line, _)| line).collect();
        for order in [
            (0..cases.len()).collect::<Vec<_>>(),
            (0..cases.len()).rev().collect(),
            (0..cases.len()).map(|i| i * 7 % cases.len()).collect(),
        ] {
            let mut membership = ListMembership::default();
            for line in order.iter().chain(order.iter()) {
                assert_eq!(membership.contains(*line, &lines), cases[*line].1, "line {line}");
            }
        }
    }

    #[test]
    fn test_basic_functionality() {
        let rule = MD031BlanksAroundFences::default();

        // Test with properly formatted code blocks
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\n\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for properly formatted code blocks"
        );

        // Test with missing blank line before
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\n\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line before");
        assert_eq!(warnings[0].line, 2, "Warning should be on line 2");
        assert!(
            warnings[0].message.contains("before"),
            "Warning should be about blank line before"
        );

        // Test with missing blank line after
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line after");
        assert_eq!(warnings[0].line, 5, "Warning should be on line 5");
        assert!(
            warnings[0].message.contains("after"),
            "Warning should be about blank line after"
        );

        // Test with missing blank lines both before and after
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for missing blank lines before and after"
        );
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD031BlanksAroundFences::default();

        // Test that nested code blocks are not flagged
        let content = r#"````markdown
```
content
```
````"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should not flag nested code blocks");

        // Test that fixes don't corrupt nested blocks
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should not modify nested code blocks");
    }

    #[test]
    fn test_nested_code_blocks_complex() {
        let rule = MD031BlanksAroundFences::default();

        // Test documentation example with nested code blocks
        let content = r#"# Documentation

## Examples

````markdown
```python
def hello():
    print("Hello, world!")
```

```javascript
console.log("Hello, world!");
```
````

More text here."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag any issues in properly formatted nested code blocks"
        );

        // Test with 5-backtick outer block
        let content_5 = r#"`````markdown
````python
```bash
echo "nested"
```
````
`````"#;

        let ctx_5 = LintContext::new(content_5, crate::config::MarkdownFlavor::Standard, None);
        let warnings_5 = rule.check(&ctx_5).unwrap();
        assert_eq!(warnings_5.len(), 0, "Should handle deeply nested code blocks");
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD031BlanksAroundFences::default();

        // Test content with trailing newline
        let content = "Some text\n```\ncode\n```\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the trailing newline
        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD031BlanksAroundFences::default();

        // Test content without trailing newline
        let content = "Some text\n```\ncode\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add trailing newline if original didn't have one
        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text");
    }

    #[test]
    fn test_list_items_config_true() {
        // Test with list_items: true (default) - should require blank lines even in lists
        let rule = MD031BlanksAroundFences::new(true);

        let content = "1. First item\n   ```python\n   code_in_list()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should flag missing blank lines before and after code block in list
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));
    }

    #[test]
    fn test_list_items_config_false() {
        // Test with list_items: false - should NOT require blank lines in lists
        let rule = MD031BlanksAroundFences::new(false);

        let content = "1. First item\n   ```python\n   code_in_list()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should not flag missing blank lines inside lists
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_list_items_config_false_outside_list() {
        // Test with list_items: false - should still require blank lines outside lists
        let rule = MD031BlanksAroundFences::new(false);

        let content = "Some text\n```python\ncode_outside_list()\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should still flag missing blank lines outside lists
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD031BlanksAroundFences::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD031");

        // Should contain the list_items option with default value true
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("list-items"));
            assert_eq!(table["list-items"], toml::Value::Boolean(true));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_fix_list_items_config_false() {
        // Test that fix respects list_items: false configuration
        let rule = MD031BlanksAroundFences::new(false);

        let content = "1. First item\n   ```python\n   code()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add blank lines when list_items is false
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_list_items_config_true() {
        // Test that fix respects list_items: true configuration
        let rule = MD031BlanksAroundFences::new(true);

        let content = "1. First item\n   ```python\n   code()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add blank lines when list_items is true
        let expected = "1. First item\n\n   ```python\n   code()\n   ```\n\n2. Second item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_no_warning_after_frontmatter() {
        // Code block immediately after frontmatter should not trigger MD031
        // This matches markdownlint behavior
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should not flag missing blank line before code block after frontmatter
        assert!(
            warnings.is_empty(),
            "Expected no warnings for code block after frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fix_does_not_add_blank_after_frontmatter() {
        // Fix should not add blank line between frontmatter and code block
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add blank line after frontmatter
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_frontmatter_with_blank_line_before_code() {
        // If there's already a blank line between frontmatter and code, that's fine
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn test_no_warning_for_admonition_after_frontmatter() {
        // Admonition immediately after frontmatter should not trigger MD031
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n!!! note\n    This is a note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Expected no warnings for admonition after frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_toml_frontmatter_before_code() {
        // TOML frontmatter should also be handled
        let rule = MD031BlanksAroundFences::default();

        let content = "+++\ntitle = \"Test\"\n+++\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Expected no warnings for code block after TOML frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fenced_code_in_list_with_4_space_indent_issue_276() {
        // Issue #276: Fenced code blocks inside lists with 4+ space indentation
        // were not being detected because of the old 0-3 space CommonMark limit.
        // Now we use pulldown-cmark which correctly handles list-indented fences.
        let rule = MD031BlanksAroundFences::new(true);

        // 4-space indented fenced code block in list (was not detected before fix)
        let content =
            "1. First item\n2. Second item with code:\n    ```python\n    print(\"Hello\")\n    ```\n3. Third item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should detect missing blank lines around the code block
        assert_eq!(
            warnings.len(),
            2,
            "Should detect fenced code in list with 4-space indent, got: {warnings:?}"
        );
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));

        // Test the fix adds blank lines
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "1. First item\n2. Second item with code:\n\n    ```python\n    print(\"Hello\")\n    ```\n\n3. Third item";
        assert_eq!(
            fixed, expected,
            "Fix should add blank lines around list-indented fenced code"
        );
    }

    #[test]
    fn test_fenced_code_in_list_with_mixed_indentation() {
        // Test both 3-space and 4-space indented fenced code blocks in same document
        let rule = MD031BlanksAroundFences::new(true);

        let content = r#"# Test

3-space indent:
1. First item
   ```python
   code
   ```
2. Second item

4-space indent:
1. First item
    ```python
    code
    ```
2. Second item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should detect all 4 missing blank lines (2 per code block)
        assert_eq!(
            warnings.len(),
            4,
            "Should detect all fenced code blocks regardless of indentation, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_before_fence() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD031BlanksAroundFences::default();

        let content = "> Text before
> ```
> code
> ```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted before the fence should have the blockquote prefix
        let expected = "> Text before
>
> ```
> code
> ```";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_after_fence() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD031BlanksAroundFences::default();

        let content = "> ```
> code
> ```
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted after the fence should have the blockquote prefix
        let expected = "> ```
> code
> ```
>
> Text after";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line after fence, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_nested_blockquote_prefix() {
        // Nested blockquotes should preserve the full prefix (e.g., ">>")
        let rule = MD031BlanksAroundFences::default();

        let content = ">> Nested quote
>> ```
>> code
>> ```
>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should insert ">>" blank lines, not ">" or plain
        let expected = ">> Nested quote
>>
>> ```
>> code
>> ```
>>
>> More text";
        assert_eq!(fixed, expected, "Fix should preserve nested blockquote prefix '>>'");
    }

    #[test]
    fn test_fix_preserves_triple_nested_blockquote_prefix() {
        // Triple-nested blockquotes should preserve full prefix
        let rule = MD031BlanksAroundFences::default();

        let content = ">>> Triple nested
>>> ```
>>> code
>>> ```
>>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = ">>> Triple nested
>>>
>>> ```
>>> code
>>> ```
>>>
>>> More text";
        assert_eq!(
            fixed, expected,
            "Fix should preserve triple-nested blockquote prefix '>>>'"
        );
    }

    // ==================== Quarto Flavor Tests ====================

    #[test]
    fn test_quarto_code_block_after_div_open() {
        // Code block immediately after Quarto div opening should not require blank line
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Should not require blank line after Quarto div opening: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_code_block_before_div_close() {
        // Code block immediately before Quarto div closing should not require blank line
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\nSome text\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should only warn about the blank before the code block (after "Some text"), not after
        assert!(
            warnings.len() <= 1,
            "Should not require blank line before Quarto div closing: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_code_block_outside_div_still_requires_blanks() {
        // Code block outside Quarto div should still require blank lines
        let rule = MD031BlanksAroundFences::default();
        let content = "Some text\n```python\ncode\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Should still require blank lines around code blocks outside divs"
        );
    }

    #[test]
    fn test_quarto_code_block_with_callout_note() {
        // Code block inside callout-note should work without blank lines at boundaries
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```r\n1 + 1\n```\n:::\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Callout note with code block should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_nested_divs_with_code() {
        // Nested divs with code blocks
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.outer}\n::: {.inner}\n```python\ncode\n```\n:::\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Nested divs with code blocks should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_div_markers_in_standard_flavor() {
        // In standard flavor, ::: is not special, so normal rules apply
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // In standard flavor, both before and after the code block need blank lines
        // (unless the ":::" lines are treated as text and thus need blanks)
        assert!(
            !warnings.is_empty(),
            "Standard flavor should require blanks around code blocks: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_fix_does_not_add_blanks_at_div_boundaries() {
        // Fix should not add blank lines at div boundaries
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Should remain unchanged - no blanks needed
        assert_eq!(fixed, content, "Fix should not add blanks at Quarto div boundaries");
    }

    #[test]
    fn test_quarto_code_block_with_content_before() {
        // Code block with content before it (inside div) needs blank
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\nHere is some code:\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should warn about missing blank before code block (after "Here is some code:")
        assert_eq!(
            warnings.len(),
            1,
            "Should require blank before code block inside div: {warnings:?}"
        );
        assert!(warnings[0].message.contains("before"));
    }

    #[test]
    fn test_quarto_code_block_with_content_after() {
        // Code block with content after it (inside div) needs blank
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\nMore content here.\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should warn about missing blank after code block (before "More content here.")
        assert_eq!(
            warnings.len(),
            1,
            "Should require blank after code block inside div: {warnings:?}"
        );
        assert!(warnings[0].message.contains("after"));
    }

    #[test]
    fn test_pandoc_code_block_after_div_open() {
        // Code block immediately after a Pandoc div opening should not require a blank line,
        // mirroring the Quarto behavior tested in test_quarto_code_block_after_div_open.
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD031 should not require blank line after Pandoc div opening: {warnings:?}"
        );
    }

    #[test]
    fn test_md031_html_comment() {
        let rule = MD031BlanksAroundFences::default();
        let content = "text <!--\n```rust\nconst x = 1;\n```\n-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD031 should not require blank lines around fenced blocks inside HTML comments: {warnings:?}"
        );
    }

    #[test]
    fn test_md031_block_attribute_after_fence_not_flagged() {
        // Issue #756: a block attribute list directly under a fenced code block
        // describes that block, so MD031 must not require a blank between them.
        let rule = MD031BlanksAroundFences::default();
        let content = "Some text.\n\n```rust\nlet x = 1;\n```\n{class=\"highlight\"}\n\nMore text.";

        for flavor in [
            crate::config::MarkdownFlavor::Hugo,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::Kramdown,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                warnings.is_empty(),
                "MD031 should not flag the block attribute line under {flavor:?}: {warnings:?}"
            );
        }

        // Negative control: in Standard the attr list is literal content, so the
        // closing fence genuinely has no blank line after it.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings_std = rule.check(&ctx_std).unwrap();
        assert!(
            warnings_std
                .iter()
                .any(|w| w.message == "No blank line after fenced code block"),
            "MD031 must flag the missing blank after the fence under Standard: {warnings_std:?}"
        );
    }
}

/// A fence written directly under a list marker that has nothing else on its
/// line is the first thing in the item: a blank line between the two would leave
/// the item empty and put the fence outside it. And a fence that is never closed
/// has no closing line to put a blank line after.
#[cfg(test)]
mod bare_marker_tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// The document after the rule's fix, and after applying the fixes of its
    /// warnings: the two paths must agree.
    fn fixed_with(rule: &MD031BlanksAroundFences, content: &str, flavor: MarkdownFlavor) -> String {
        let ctx = LintContext::new(content, flavor, None);
        let by_fix = rule.fix(&ctx).unwrap();
        let warnings = rule.check(&ctx).unwrap();
        let by_warnings = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(by_fix, by_warnings, "{content:?}: fix and warnings disagree");
        by_fix
    }

    fn fixed(content: &str) -> String {
        fixed_with(&MD031BlanksAroundFences::default(), content, MarkdownFlavor::Standard)
    }

    fn warnings_of(content: &str) -> Vec<String> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD031BlanksAroundFences::default()
            .check(&ctx)
            .unwrap()
            .into_iter()
            .map(|w| format!("{}:{}", w.line, w.message))
            .collect()
    }

    #[test]
    fn a_fence_under_a_bare_marker_gets_no_blank_line_above() {
        for content in [
            "-\n  ```\n  x\n  ```\n",
            "+\n  ```\n  x\n  ```\n",
            "*\n  ```\n  x\n  ```\n",
            "1.\n   ```\n   x\n   ```\n",
            "1)\n   ```\n   x\n   ```\n",
            "10.\n    ```\n    x\n    ```\n",
            "-   \n  ```\n  x\n  ```\n",
            "-\n  ~~~\n  x\n  ~~~\n",
            "-\n  ````rust\n  x\n  ````\n",
            "- a\n\n  -\n    ```\n    x\n    ```\n",
            "-\n  -\n    ```\n    x\n    ```\n",
            "- -\n    ```\n    x\n    ```\n",
            "1. -\n     ```\n     x\n     ```\n",
            "> -\n>   ```\n>   x\n>   ```\n",
            "> 1.\n>    ```\n>    x\n>    ```\n",
            "> > -\n> >   ```\n> >   x\n> >   ```\n",
            // The fence stands on the marker line of an item that is itself the
            // first content of a bare marker's item.
            "1.\n   - ```\n     x\n     ```\n",
            "-\n  1. ```\n     x\n     ```\n",
        ] {
            assert_eq!(warnings_of(content), Vec::<String>::new(), "{content:?}");
            assert_eq!(fixed(content), content, "{content:?}");
        }
    }

    #[test]
    fn the_list_items_option_does_not_change_that() {
        for list_items in [true, false] {
            let rule = MD031BlanksAroundFences::new(list_items);
            let content = "-\n  ```\n  x\n  ```\n";
            assert_eq!(fixed_with(&rule, content, MarkdownFlavor::Standard), content);
        }
    }

    #[test]
    fn a_blank_line_is_still_added_where_it_changes_no_container() {
        for (content, expected) in [
            ("a\n```\nx\n```\n", "a\n\n```\nx\n```\n"),
            ("- a\n  ```\n  x\n  ```\n", "- a\n\n  ```\n  x\n  ```\n"),
            // Outside the empty item the fence is outside it either way.
            ("-\n```\nx\n```\n", "-\n\n```\nx\n```\n"),
            ("-\n ```\n x\n ```\n", "-\n\n ```\n x\n ```\n"),
            // The fence belongs to the outer item; the inner one is empty with or
            // without the blank line.
            ("- -\n  ```\n  x\n  ```\n", "- -\n\n  ```\n  x\n  ```\n"),
            // A bare dash under text is a setext underline, not an item.
            ("a\n-\n  ```\n  x\n  ```\n", "a\n-\n\n  ```\n  x\n  ```\n"),
            // The blank line after the closing fence is unaffected.
            ("-\n  ```\n  x\n  ```\n  after\n", "-\n  ```\n  x\n  ```\n\n  after\n"),
            // Under item text the dash is a setext underline too.
            (
                "- a\n  -\n    ```\n    x\n    ```\n",
                "- a\n  -\n\n    ```\n    x\n    ```\n",
            ),
            // A fence on the marker line has the line above it, not a marker.
            ("a\n- ```\n  x\n  ```\n", "a\n\n- ```\n  x\n  ```\n"),
        ] {
            assert_eq!(fixed(content), expected, "{content:?}");
            assert_eq!(fixed(expected), expected, "{expected:?} is not stable");
        }
    }

    #[test]
    fn an_unclosed_fence_has_no_closing_line_to_follow() {
        for content in [
            "> ```\nfoo\n",
            "> ```\n> ~~~\nfoo\n",
            "> ````\n> ```\nfoo\n",
            "> ```\n>     ```\nfoo\n",
            "> ```\n> \u{a0}```\nfoo\n",
            "> ```\n> ```\u{a0}\nfoo\n",
            "> ```\n> ",
            "```\n  ",
            "> ```\n> ~~~\n> ",
            "- ```\nfoo\n",
            "-\n  ```\nfoo\n",
            "-\n  ```\n  x\nfoo\n",
        ] {
            assert_eq!(fixed(content), content, "{content:?}");
        }
    }

    #[test]
    fn a_closed_fence_still_gets_a_blank_line_after() {
        for (content, expected) in [
            ("> ```\n> x\n> ```\nfoo\n", "> ```\n> x\n> ```\n>\nfoo\n"),
            ("> ```\n> ```\nfoo\n", "> ```\n> ```\n>\nfoo\n"),
            ("> ````\n> x\n> `````\nfoo\n", "> ````\n> x\n> `````\n>\nfoo\n"),
            ("```\nx\n```\nfoo\n", "```\nx\n```\n\nfoo\n"),
        ] {
            assert_eq!(fixed(content), expected, "{content:?}");
        }
    }

    #[test]
    fn an_admonition_after_an_unclosed_fence_is_still_checked() {
        let rule = MD031BlanksAroundFences::default();
        let content = "> ```\nplain\n!!! note\n    body\nafter\n";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let messages: Vec<String> = rule.check(&ctx).unwrap().into_iter().map(|w| w.message).collect();
        assert_eq!(
            messages,
            vec![
                "No blank line before admonition block",
                "No blank line after admonition block"
            ]
        );
    }

    #[test]
    fn colon_fences_and_directives_stay_under_a_bare_marker() {
        let rule = MD031BlanksAroundFences::default();
        let colon = "-\n  :::python\n  x\n  :::\n";
        assert_eq!(fixed_with(&rule, colon, MarkdownFlavor::AzureDevOps), colon);
        let myst = "-\n  :::{note}\n  x\n  :::\n";
        assert_eq!(fixed_with(&rule, myst, MarkdownFlavor::MyST), myst);
        let admonition = "-\n  !!! note\n      body\n";
        assert_eq!(fixed_with(&rule, admonition, MarkdownFlavor::MkDocs), admonition);
        // Controls: with text above them they get the blank line.
        assert_eq!(
            fixed_with(&rule, "a\n:::python\nx\n:::\n", MarkdownFlavor::AzureDevOps),
            "a\n\n:::python\nx\n:::\n"
        );
        assert_eq!(
            fixed_with(&rule, "a\n!!! note\n    body\n", MarkdownFlavor::MkDocs),
            "a\n\n!!! note\n    body\n"
        );
    }
}
