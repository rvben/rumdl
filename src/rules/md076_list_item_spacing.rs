use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::skip_context::is_table_line;

/// Rule MD076: Enforce consistent blank lines between list items
///
/// See [docs/md076.md](../../docs/md076.md) for full documentation and examples.
///
/// Enforces that the spacing between consecutive list items is consistent
/// within each list: either all gaps have a blank line (loose) or none do (tight).
///
/// ## Configuration
///
/// ```toml
/// [MD076]
/// style = "consistent"  # "loose", "tight", or "consistent" (default)
/// ```
///
/// - `"consistent"` — within each list, all gaps must use the same style (majority wins)
/// - `"loose"` — blank line required between every pair of items
/// - `"tight"` — no blank lines allowed between any items

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ListItemSpacingStyle {
    #[default]
    Consistent,
    Loose,
    Tight,
}

#[derive(Debug, Clone, Default)]
pub(super) struct MD076Config {
    pub style: ListItemSpacingStyle,
    /// When true, blank lines around continuation paragraphs within a list item
    /// are permitted even in tight mode. This allows tight inter-item spacing
    /// while using blank lines to visually separate continuation content.
    pub allow_loose_continuation: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MD076ListItemSpacing {
    config: MD076Config,
}

/// Classification of the spacing between two consecutive list items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapKind {
    /// No blank line between items.
    Tight,
    /// Blank line that is a genuine inter-item separator.
    Loose,
    /// Blank line required by another rule (MD031, MD058) around structural content.
    /// Excluded from consistency analysis — neither loose nor tight.
    Structural,
    /// Blank line after continuation content within a list item.
    /// Treated as `Structural` when `allow_loose_continuation` is enabled,
    /// or as `Loose` when disabled (default).
    ContinuationLoose,
}

/// Per-list analysis result shared by check() and fix().
struct ListAnalysis {
    /// 1-indexed line numbers of the list's items, in order.
    items: Vec<usize>,
    /// Classification of each inter-item gap.
    gaps: Vec<GapKind>,
    /// Whether loose gaps are violations (should have blank lines removed).
    warn_loose_gaps: bool,
    /// Whether tight gaps are violations (should have blank lines inserted).
    warn_tight_gaps: bool,
}

impl MD076ListItemSpacing {
    pub fn new(style: ListItemSpacingStyle) -> Self {
        Self {
            config: MD076Config {
                style,
                allow_loose_continuation: false,
            },
        }
    }

    pub fn with_allow_loose_continuation(mut self, allow: bool) -> Self {
        self.config.allow_loose_continuation = allow;
        self
    }

    /// Check whether a line is effectively blank, accounting for blockquote markers.
    ///
    /// A line like `>` or `> ` is considered blank in blockquote context even though
    /// its raw content is non-empty.
    fn is_effectively_blank(ctx: &LintContext, line_num: usize) -> bool {
        if let Some(info) = ctx.line_info(line_num) {
            let content = info.content(ctx.content);
            if content.trim().is_empty() {
                return true;
            }
            // In a blockquote, a line containing only markers (e.g., ">", "> ") is blank
            if let Some(ref bq) = info.blockquote {
                return bq.content.trim().is_empty();
            }
            false
        } else {
            false
        }
    }

    /// Check whether a non-blank line is structural content (code block, table, HTML block,
    /// or blockquote) whose trailing blank line is required by other rules (MD031, MD058).
    fn is_structural_content(ctx: &LintContext, line_num: usize) -> bool {
        if let Some(info) = ctx.line_info(line_num) {
            // Inside a code block (includes the closing fence itself)
            if info.in_code_block {
                return true;
            }
            // Inside an HTML block
            if info.in_html_block {
                return true;
            }
            // Inside a blockquote
            if info.blockquote.is_some() {
                return true;
            }
            // A table row or separator
            let content = info.content(ctx.content);
            // Strip blockquote prefix and list continuation indent before checking table syntax
            let effective = if let Some(ref bq) = info.blockquote {
                bq.content.as_str()
            } else {
                content
            };
            if is_table_line(effective.trim_start()) {
                return true;
            }
        }
        false
    }

    /// Check whether a list item opens a fenced code block on its marker line.
    ///
    /// MD031 requires a blank line before that fence. MD076 must treat the same
    /// blank as structural rather than removing it as an inter-item separator.
    ///
    /// The question goes to the parser, through the same `is_fenced` details MD031
    /// itself reads, so the two rules cannot disagree about what a fence is. Line
    /// text is not enough: a marker followed by five spaces and a fence is an
    /// *indented* code block, which MD031 says nothing about, and the `in_code_block`
    /// flag is true for indented blocks as well as fenced ones.
    fn is_fenced_code_block_list_item(ctx: &LintContext, line_num: usize) -> bool {
        let Some(info) = ctx.line_info(line_num) else {
            return false;
        };
        if info.list_item.is_none() {
            return false;
        }

        let line_range = info.byte_offset..info.byte_offset + info.byte_len;
        ctx.code_block_details
            .iter()
            .any(|detail| detail.is_fenced && line_range.contains(&detail.start))
    }

    /// Check whether a non-blank line is continuation content within a list item
    /// (indented prose that is not itself a list marker or structural content).
    ///
    /// `parent_content_col` is the content column of the parent list item marker
    /// (e.g., 2 for `- item`, 3 for `1. item`). Continuation must be indented
    /// to at least this column to belong to the parent item.
    fn is_continuation_content(ctx: &LintContext, line_num: usize, parent_content_col: usize) -> bool {
        let Some(info) = ctx.line_info(line_num) else {
            return false;
        };
        // Lines with a list marker are items, not continuation
        if info.list_item.is_some() {
            return false;
        }
        // Structural content is handled separately by is_structural_content
        if info.in_code_block
            || info.in_html_block
            || info.in_html_comment
            || info.in_mdx_comment
            || info.in_front_matter
            || info.in_math_block
            || info.blockquote.is_some()
        {
            return false;
        }
        let content = info.content(ctx.content);
        if content.trim().is_empty() {
            return false;
        }
        // Continuation must be indented to at least the parent item's content column
        let indent = content.len() - content.trim_start().len();
        indent >= parent_content_col
    }

    /// Classify the inter-item gap between two consecutive items.
    ///
    /// Returns `Tight` if there is no blank line, `Loose` if there is a genuine
    /// inter-item separator blank, `Structural` if the only blank line is
    /// required by another rule (MD031/MD058) after structural content, or
    /// `ContinuationLoose` if the blank line follows continuation content
    /// within a list item.
    fn classify_gap(ctx: &LintContext, first: usize, next: usize) -> GapKind {
        if next <= first + 1 {
            return GapKind::Tight;
        }
        // The gap has a blank line only if the line immediately before the next item is blank.
        if !Self::is_effectively_blank(ctx, next - 1) {
            return GapKind::Tight;
        }
        // A fence opened directly on a list marker is still part of that list
        // item. The blank before it belongs to MD031, not to MD076's spacing
        // consistency policy, so removing it would create a fix loop.
        if Self::is_fenced_code_block_list_item(ctx, next) {
            return GapKind::Structural;
        }
        // Walk backwards past blank lines to find the last non-blank content line.
        // If that line is structural content, the blank is required (not a separator).
        let mut scan = next - 1;
        while scan > first && Self::is_effectively_blank(ctx, scan) {
            scan -= 1;
        }
        // `scan` is now the last non-blank line before the next item
        if scan > first && Self::is_structural_content(ctx, scan) {
            return GapKind::Structural;
        }
        // Check if the last non-blank line is continuation content.
        // Use the first item's content column to verify proper indentation.
        let parent_content_col = ctx
            .line_info(first)
            .and_then(|li| li.list_item.as_ref())
            .map_or(2, |item| item.content_column);
        if scan > first && Self::is_continuation_content(ctx, scan, parent_content_col) {
            return GapKind::ContinuationLoose;
        }
        GapKind::Loose
    }

    /// Collect the 1-indexed line numbers of all inter-item blank lines in the gap.
    ///
    /// Walks backwards from the line before `next` collecting consecutive blank lines.
    /// These are the actual separator lines between items, not blank lines within
    /// multi-paragraph items. Structural blanks (after code blocks, tables, HTML blocks)
    /// are excluded.
    fn inter_item_blanks(ctx: &LintContext, first: usize, next: usize) -> Vec<usize> {
        let mut blanks = Vec::new();
        let mut line_num = next - 1;
        while line_num > first && Self::is_effectively_blank(ctx, line_num) {
            blanks.push(line_num);
            line_num -= 1;
        }
        // If the last non-blank line is structural content, these blanks are structural
        if line_num > first && Self::is_structural_content(ctx, line_num) {
            return Vec::new();
        }
        blanks.reverse();
        blanks
    }

    /// Analyze every list in the document: each block's items grouped into
    /// the lists they form, one per run of items at one nesting level, so a
    /// nested list is judged on its own spacing and never on its parent's.
    fn analyze(&self, ctx: &LintContext) -> Vec<ListAnalysis> {
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .filter_map(|items| {
                Self::analyze_list(ctx, items, &self.config.style, self.config.allow_loose_continuation)
            })
            .collect()
    }

    /// Analyze one list, given the lines of its items in order, to determine
    /// which gaps need fixing.
    ///
    /// Returns `None` if the list has fewer than 2 items or if no gaps violate
    /// the configured style.
    fn analyze_list(
        ctx: &LintContext,
        items: Vec<usize>,
        style: &ListItemSpacingStyle,
        allow_loose_continuation: bool,
    ) -> Option<ListAnalysis> {
        if items.len() < 2 {
            return None;
        }

        // Classify each inter-item gap.
        let gaps: Vec<GapKind> = items.windows(2).map(|w| Self::classify_gap(ctx, w[0], w[1])).collect();

        // Structural gaps and (when allowed) continuation gaps are excluded
        // from consistency analysis — they should not influence whether the
        // list is considered loose or tight.
        let loose_count = gaps
            .iter()
            .filter(|&&g| g == GapKind::Loose || (g == GapKind::ContinuationLoose && !allow_loose_continuation))
            .count();
        let tight_count = gaps.iter().filter(|&&g| g == GapKind::Tight).count();

        let (warn_loose_gaps, warn_tight_gaps) = match style {
            ListItemSpacingStyle::Loose => (false, true),
            ListItemSpacingStyle::Tight => (true, false),
            ListItemSpacingStyle::Consistent => {
                if loose_count == 0 || tight_count == 0 {
                    return None; // Already consistent (structural gaps excluded)
                }
                // Majority wins. On a tie, prefer tight (warn loose):
                //   - tight is the dominant style in real-world Markdown;
                //     loose is opt-in for multi-paragraph items,
                //   - matches the minimal-whitespace convention used by
                //     Prettier and most other Markdown formatters,
                //   - removes a blank line rather than inserting one, which
                //     is the lower-impact edit on a tied document.
                if tight_count >= loose_count {
                    (true, false)
                } else {
                    (false, true)
                }
            }
        };

        Some(ListAnalysis {
            items,
            gaps,
            warn_loose_gaps,
            warn_tight_gaps,
        })
    }
}

impl Rule for MD076ListItemSpacing {
    fn name(&self) -> &'static str {
        "MD076"
    }

    fn description(&self) -> &'static str {
        "List item spacing should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || ctx.list_blocks.is_empty()
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        let allow_cont = self.config.allow_loose_continuation;
        // The edits are applied to the document as the rule saw it, which an
        // editor hands over with its own line endings, so an inserted line
        // ends the way the document's lines do.
        let line_ending = crate::utils::line_ending::detect_line_ending(ctx.content);

        for analysis in self.analyze(ctx) {
            for (i, &gap) in analysis.gaps.iter().enumerate() {
                let is_loose_violation = match gap {
                    GapKind::Loose => analysis.warn_loose_gaps,
                    GapKind::ContinuationLoose => !allow_cont && analysis.warn_loose_gaps,
                    _ => false,
                };

                if is_loose_violation {
                    let next_item = analysis.items[i + 1];
                    let blanks = Self::inter_item_blanks(ctx, analysis.items[i], next_item);
                    if let Some(&blank_line) = blanks.first() {
                        let line_content = ctx.line_info(blank_line).map_or("", |li| li.content(ctx.content));
                        // The edit removes the whole run of blank lines the
                        // fix removes, so applying it alone closes the gap.
                        let fix = ctx
                            .line_start_byte(blank_line)
                            .zip(ctx.line_start_byte(next_item))
                            .map(|(start, end)| Fix::new(start..end, String::new()));
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: blank_line,
                            column: 1,
                            end_line: blank_line,
                            end_column: line_content.chars().count() + 1,
                            message: "Unexpected blank line between list items".to_string(),
                            severity: Severity::Warning,
                            fix,
                        });
                    }
                } else if gap == GapKind::Tight && analysis.warn_tight_gaps {
                    let next_item = analysis.items[i + 1];
                    let line_content = ctx.line_info(next_item).map_or("", |li| li.content(ctx.content));
                    // The blank line goes in front of the item, carrying the
                    // item's blockquote prefix as the fix writes it.
                    let fix = ctx.line_start_byte(next_item).map(|start| {
                        let prefix = ctx.blockquote_prefix_for_blank_line(next_item - 1);
                        Fix::new(start..start, format!("{prefix}{line_ending}"))
                    });
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: next_item,
                        column: 1,
                        end_line: next_item,
                        end_column: line_content.chars().count() + 1,
                        message: "Missing blank line between list items".to_string(),
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if ctx.content.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all inter-item blank lines to remove and lines to insert before.
        let mut insert_before: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut remove_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

        let allow_cont = self.config.allow_loose_continuation;

        for analysis in self.analyze(ctx) {
            for (i, &gap) in analysis.gaps.iter().enumerate() {
                let is_loose_violation = match gap {
                    GapKind::Loose => analysis.warn_loose_gaps,
                    GapKind::ContinuationLoose => !allow_cont && analysis.warn_loose_gaps,
                    _ => false,
                };

                if is_loose_violation {
                    for blank_line in Self::inter_item_blanks(ctx, analysis.items[i], analysis.items[i + 1]) {
                        remove_lines.insert(blank_line);
                    }
                } else if gap == GapKind::Tight && analysis.warn_tight_gaps {
                    insert_before.insert(analysis.items[i + 1]);
                }
            }
        }

        if insert_before.is_empty() && remove_lines.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let lines = ctx.raw_lines();
        let mut result: Vec<String> = Vec::with_capacity(lines.len());

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;

            // Skip modifications for lines where the rule is disabled via inline config
            if ctx.is_rule_disabled(self.name(), line_num) {
                result.push((*line).to_string());
                continue;
            }

            if remove_lines.contains(&line_num) {
                continue;
            }

            if insert_before.contains(&line_num) {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                result.push(bq_prefix);
            }

            result.push((*line).to_string());
        }

        let mut output = result.join("\n");
        if ctx.content.ends_with('\n') {
            output.push('\n');
        }
        Ok(output)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        let style_str = match self.config.style {
            ListItemSpacingStyle::Consistent => "consistent",
            ListItemSpacingStyle::Loose => "loose",
            ListItemSpacingStyle::Tight => "tight",
        };
        map.insert("style".to_string(), toml::Value::String(style_str.to_string()));
        map.insert(
            "allow-loose-continuation".to_string(),
            toml::Value::Boolean(self.config.allow_loose_continuation),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD076", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "loose" => ListItemSpacingStyle::Loose,
            "tight" => ListItemSpacingStyle::Tight,
            _ => ListItemSpacingStyle::Consistent,
        };
        let allow_loose_continuation =
            crate::config::get_rule_config_value::<bool>(config, "MD076", "allow-loose-continuation")
                .or_else(|| crate::config::get_rule_config_value::<bool>(config, "MD076", "allow_loose_continuation"))
                .unwrap_or(false);
        Box::new(Self::new(style).with_allow_loose_continuation(allow_loose_continuation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(content: &str, style: ListItemSpacingStyle) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD076ListItemSpacing::new(style);
        rule.check(&ctx).unwrap()
    }

    fn check_with_continuation(
        content: &str,
        style: ListItemSpacingStyle,
        allow_loose_continuation: bool,
    ) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD076ListItemSpacing::new(style).with_allow_loose_continuation(allow_loose_continuation);
        rule.check(&ctx).unwrap()
    }

    fn fix(content: &str, style: ListItemSpacingStyle) -> String {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD076ListItemSpacing::new(style);
        rule.fix(&ctx).unwrap()
    }

    fn fix_with_continuation(content: &str, style: ListItemSpacingStyle, allow_loose_continuation: bool) -> String {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD076ListItemSpacing::new(style).with_allow_loose_continuation(allow_loose_continuation);
        rule.fix(&ctx).unwrap()
    }

    // ── Basic style detection ──────────────────────────────────────────

    #[test]
    fn tight_list_tight_style_no_warnings() {
        let content = "- Item 1\n- Item 2\n- Item 3\n";
        assert!(check(content, ListItemSpacingStyle::Tight).is_empty());
    }

    #[test]
    fn loose_list_loose_style_no_warnings() {
        let content = "- Item 1\n\n- Item 2\n\n- Item 3\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
    }

    #[test]
    fn tight_list_loose_style_warns() {
        let content = "- Item 1\n- Item 2\n- Item 3\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|w| w.message.contains("Missing")));
    }

    #[test]
    fn loose_list_tight_style_warns() {
        let content = "- Item 1\n\n- Item 2\n\n- Item 3\n";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|w| w.message.contains("Unexpected")));
    }

    // ── Consistent mode ────────────────────────────────────────────────

    #[test]
    fn consistent_all_tight_no_warnings() {
        let content = "- Item 1\n- Item 2\n- Item 3\n";
        assert!(check(content, ListItemSpacingStyle::Consistent).is_empty());
    }

    #[test]
    fn consistent_all_loose_no_warnings() {
        let content = "- Item 1\n\n- Item 2\n\n- Item 3\n";
        assert!(check(content, ListItemSpacingStyle::Consistent).is_empty());
    }

    #[test]
    fn consistent_mixed_majority_loose_warns_tight() {
        // 2 loose gaps, 1 tight gap → tight is minority → warn on tight
        let content = "- Item 1\n\n- Item 2\n- Item 3\n\n- Item 4\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Missing"));
    }

    #[test]
    fn consistent_mixed_majority_tight_warns_loose() {
        // 1 loose gap, 2 tight gaps → loose is minority → warn on loose blank line
        let content = "- Item 1\n\n- Item 2\n- Item 3\n- Item 4\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Unexpected"));
    }

    #[test]
    fn consistent_tie_prefers_tight() {
        // 1 loose + 1 tight gap → tied. Prefer tight: warn on the loose gap
        // ("Unexpected blank line") so fmt removes the blank rather than
        // inserting one. See `analyze_block` for the rationale.
        let content = "- Item 1\n\n- Item 2\n- Item 3\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Unexpected"));
    }

    // ── Edge cases ─────────────────────────────────────────────────────

    #[test]
    fn single_item_list_no_warnings() {
        let content = "- Only item\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
        assert!(check(content, ListItemSpacingStyle::Tight).is_empty());
        assert!(check(content, ListItemSpacingStyle::Consistent).is_empty());
    }

    #[test]
    fn empty_content_no_warnings() {
        assert!(check("", ListItemSpacingStyle::Consistent).is_empty());
    }

    #[test]
    fn ordered_list_tight_gaps_loose_style_warns() {
        let content = "1. First\n2. Second\n3. Third\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn task_list_works() {
        let content = "- [x] Task 1\n- [ ] Task 2\n- [x] Task 3\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 2);
        let fixed = fix(content, ListItemSpacingStyle::Loose);
        assert_eq!(fixed, "- [x] Task 1\n\n- [ ] Task 2\n\n- [x] Task 3\n");
    }

    #[test]
    fn no_trailing_newline() {
        let content = "- Item 1\n- Item 2";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 1);
        let fixed = fix(content, ListItemSpacingStyle::Loose);
        assert_eq!(fixed, "- Item 1\n\n- Item 2");
    }

    #[test]
    fn two_separate_lists() {
        let content = "- A\n- B\n\nText\n\n1. One\n2. Two\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 2);
        let fixed = fix(content, ListItemSpacingStyle::Loose);
        assert_eq!(fixed, "- A\n\n- B\n\nText\n\n1. One\n\n2. Two\n");
    }

    #[test]
    fn no_list_content() {
        let content = "Just a paragraph.\n\nAnother paragraph.\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
        assert!(check(content, ListItemSpacingStyle::Tight).is_empty());
    }

    // ── Multi-line and continuation items ──────────────────────────────

    #[test]
    fn continuation_lines_tight_detected() {
        let content = "- Item 1\n  continuation\n- Item 2\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Missing"));
    }

    #[test]
    fn continuation_lines_loose_detected() {
        let content = "- Item 1\n  continuation\n\n- Item 2\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Unexpected"));
    }

    #[test]
    fn multi_paragraph_item_not_treated_as_inter_item_gap() {
        // Blank line between paragraphs within Item 1 must NOT trigger a warning.
        // Only the blank line immediately before Item 2 is an inter-item separator.
        let content = "- Item 1\n\n  Second paragraph\n\n- Item 2\n";
        // Both gaps are loose (blank before Item 2), so tight should warn once
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert_eq!(
            warnings.len(),
            1,
            "Should warn only on the inter-item blank, not the intra-item blank"
        );
        // The fix should remove only the inter-item blank (line 4), preserving the
        // multi-paragraph structure
        let fixed = fix(content, ListItemSpacingStyle::Tight);
        assert_eq!(fixed, "- Item 1\n\n  Second paragraph\n- Item 2\n");
    }

    #[test]
    fn multi_paragraph_item_loose_style_no_warnings() {
        // A loose list with multi-paragraph items is already loose — no warnings
        let content = "- Item 1\n\n  Second paragraph\n\n- Item 2\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
    }

    // ── Blockquote lists ───────────────────────────────────────────────

    #[test]
    fn blockquote_tight_list_loose_style_warns() {
        let content = "> - Item 1\n> - Item 2\n> - Item 3\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn blockquote_loose_list_detected() {
        // A line with only `>` is effectively blank in blockquote context
        let content = "> - Item 1\n>\n> - Item 2\n";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert_eq!(warnings.len(), 1, "Blockquote-only line should be detected as blank");
        assert!(warnings[0].message.contains("Unexpected"));
    }

    #[test]
    fn blockquote_loose_list_no_warnings_when_loose() {
        let content = "> - Item 1\n>\n> - Item 2\n";
        assert!(check(content, ListItemSpacingStyle::Loose).is_empty());
    }

    // ── Multiple blank lines ───────────────────────────────────────────

    #[test]
    fn multiple_blanks_all_removed() {
        let content = "- Item 1\n\n\n- Item 2\n";
        let fixed = fix(content, ListItemSpacingStyle::Tight);
        assert_eq!(fixed, "- Item 1\n- Item 2\n");
    }

    #[test]
    fn multiple_blanks_fix_is_idempotent() {
        let content = "- Item 1\n\n\n\n- Item 2\n";
        let fixed_once = fix(content, ListItemSpacingStyle::Tight);
        let fixed_twice = fix(&fixed_once, ListItemSpacingStyle::Tight);
        assert_eq!(fixed_once, fixed_twice);
        assert_eq!(fixed_once, "- Item 1\n- Item 2\n");
    }

    // ── Fix correctness ────────────────────────────────────────────────

    #[test]
    fn fix_adds_blank_lines() {
        let content = "- Item 1\n- Item 2\n- Item 3\n";
        let fixed = fix(content, ListItemSpacingStyle::Loose);
        assert_eq!(fixed, "- Item 1\n\n- Item 2\n\n- Item 3\n");
    }

    #[test]
    fn fix_removes_blank_lines() {
        let content = "- Item 1\n\n- Item 2\n\n- Item 3\n";
        let fixed = fix(content, ListItemSpacingStyle::Tight);
        assert_eq!(fixed, "- Item 1\n- Item 2\n- Item 3\n");
    }

    #[test]
    fn fix_consistent_adds_blank() {
        // 2 loose gaps, 1 tight gap → add blank before Item 3
        let content = "- Item 1\n\n- Item 2\n- Item 3\n\n- Item 4\n";
        let fixed = fix(content, ListItemSpacingStyle::Consistent);
        assert_eq!(fixed, "- Item 1\n\n- Item 2\n\n- Item 3\n\n- Item 4\n");
    }

    #[test]
    fn fix_idempotent_loose() {
        let content = "- Item 1\n- Item 2\n";
        let fixed_once = fix(content, ListItemSpacingStyle::Loose);
        let fixed_twice = fix(&fixed_once, ListItemSpacingStyle::Loose);
        assert_eq!(fixed_once, fixed_twice);
    }

    #[test]
    fn fix_idempotent_tight() {
        let content = "- Item 1\n\n- Item 2\n";
        let fixed_once = fix(content, ListItemSpacingStyle::Tight);
        let fixed_twice = fix(&fixed_once, ListItemSpacingStyle::Tight);
        assert_eq!(fixed_once, fixed_twice);
    }

    // ── Nested lists ───────────────────────────────────────────────────

    #[test]
    fn nested_list_does_not_affect_parent() {
        // Nested items should not trigger warnings for the parent list
        let content = "- Item 1\n  - Nested A\n  - Nested B\n- Item 2\n";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert!(
            warnings.is_empty(),
            "Nested items should not cause parent-level warnings"
        );
    }

    #[test]
    fn tab_nested_child_is_not_a_sibling() {
        // A tab before the child's marker puts it at column 4, one level below
        // the parent items, so the parent list is `parent` and `next` with no
        // blank line between them and nothing to report. Measuring the child
        // in bytes puts it at level 0, the parent's own, and reads the blank
        // line as a loose gap between siblings.
        let content = "* parent\n\n\t1. child\n* next\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            warnings.is_empty(),
            "a tab-nested child is not a sibling of the parent items: {warnings:?}"
        );
        assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);

        // Positive control: the same shape with the child at the parent's
        // level is a real spacing inconsistency.
        let sibling = "* parent\n\n* child\n* next\n";
        let warnings = check(sibling, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 2);
    }

    #[test]
    fn nested_list_is_analysed_at_its_own_level() {
        // The nested list mixes a loose gap and a tight gap while the parent
        // list is tight, so the inconsistency is the nested list's own: the
        // tie resolves to tight and the blank line between its first two
        // items is reported and removed. Space and tab indentation nest the
        // same way.
        for (label, content, fixed) in [
            (
                "spaces",
                "- parent\n  - a\n\n  - b\n  - c\n- next\n",
                "- parent\n  - a\n  - b\n  - c\n- next\n",
            ),
            (
                "tab",
                "* parent\n\t1. child A\n\n\t2. child B\n\t3. child C\n",
                "* parent\n\t1. child A\n\t2. child B\n\t3. child C\n",
            ),
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert_eq!(warnings.len(), 1, "{label}: {warnings:?}");
            assert_eq!(warnings[0].line, 3, "{label}: {warnings:?}");
            assert_eq!(
                warnings[0].message, "Unexpected blank line between list items",
                "{label}"
            );
            assert_eq!(fix(content, ListItemSpacingStyle::Consistent), fixed, "{label}");
        }

        // Negative control: a nested list that is uniformly loose under a
        // tight parent is consistent at both levels.
        let content = "- parent\n  - a\n\n  - b\n- next\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);
    }

    #[test]
    fn nested_lists_under_different_parents_are_separate_lists() {
        // `a1`/`a2` and `b1`/`b2` sit at the same nesting level but belong to
        // different parent items, so each pair is judged on its own: the
        // first is consistently tight, the second consistently loose, and a
        // per-level view that ran them together would call the whole set
        // inconsistent.
        let content = "- a\n  - a1\n  - a2\n- b\n  - b1\n\n  - b2\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);

        // Positive control: the same two pairs under one parent are one list
        // and its gaps do disagree.
        let content = "- a\n  - a1\n  - a2\n  - b1\n\n  - b2\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn every_warning_carries_the_edit_the_fix_applies() {
        // A warning without an edit reads as unfixable to `check`, counts as
        // not fixed after `fmt`, and offers no quick fix in an editor. Each
        // MD076 warning carries its own edit, and applying the edits alone
        // produces what the document-level fix produces: a removed gap loses
        // its whole run of blank lines, an inserted blank line carries the
        // item's blockquote prefix, and line endings are kept.
        let cases = [
            ("- a\n\n\n- b\n- c\n", ListItemSpacingStyle::Consistent),
            ("- a\n- b\n\n- c\n", ListItemSpacingStyle::Loose),
            ("> - a\n>\n> - b\n> - c\n", ListItemSpacingStyle::Consistent),
            ("> - a\n> - b\n>\n> - c\n", ListItemSpacingStyle::Loose),
            ("- p\n  - a\n\n  - b\n  - c\n", ListItemSpacingStyle::Consistent),
        ];
        for (content, style) in cases {
            let warnings = check(content, style.clone());
            assert!(!warnings.is_empty(), "{content:?}");
            assert!(warnings.iter().all(|w| w.fix.is_some()), "{content:?}: {warnings:?}");
            let applied = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
            let fixed = fix(content, style);
            assert_eq!(applied, fixed, "{content:?}");
            assert_ne!(applied, content, "{content:?}");
        }

        // The edits are byte ranges into the document as written, and an
        // inserted line ends the way the document's lines do, so a CRLF
        // document keeps its line endings. The editor applies each edit as
        // given, so the replacement itself is checked, not only the result of
        // `apply_warning_fixes`, which restores the document's endings.
        let content = "- a\r\n\r\n- b\r\n- c\r\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(
            crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap(),
            "- a\r\n- b\r\n- c\r\n"
        );
        for (content, replacement) in [("- a\r\n- b\r\n\r\n- c\r\n", "\r\n"), ("> - a\r\n> - b\r\n", ">\r\n")] {
            let warnings = check(content, ListItemSpacingStyle::Loose);
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            let fix = warnings[0].fix.as_ref().expect("the warning carries its edit");
            assert_eq!(fix.replacement, replacement, "{content:?}");
            assert_eq!(fix.range.start, fix.range.end, "{content:?}: an insertion");
        }
        let content = "- a\r\n- b\r\n\r\n- c\r\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(
            crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap(),
            "- a\r\n\r\n- b\r\n\r\n- c\r\n"
        );
    }

    #[test]
    fn nested_lists_separated_by_parent_content_are_separate_lists() {
        // A paragraph belonging to the parent item ends the nested list, so
        // the tight pair before it and the loose pair after it are two lists,
        // each consistent on its own; the gap around the paragraph is nobody's
        // inter-item gap.
        // The same holds for parent content that interrupts a paragraph
        // without a blank line before it: an HTML comment, or a blockquote
        // (a bare `>` opens one; under an unquoted list it is not a blank
        // line, and inside a quoted list a deeper `>` is not one either).
        for content in [
            "- p\n  - a\n  - b\n\n  With:\n\n  - c\n\n  - d\n",
            "- p\n  - a\n  - b\n  <!-- parent comment -->\n  - c\n\n  - d\n",
            "- p\n  - a\n  - b\n  >\n  - c\n\n  - d\n",
            "> - p\n>   - a\n>   - b\n>   >\n>   - c\n>\n>   - d\n",
            "- p\n  - a\n    >\n  parent\n  - c\n\n  - d\n",
            "- p\n  - a\n  - ```\n  more\n  - c\n\n  - d\n",
            "- p\n  - a\n  - | h |\n    | --- |\n  more\n  - c\n\n  - d\n",
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
            assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content, "{content:?}");
        }

        // Positive controls: without the paragraph the four items are one
        // list whose gaps disagree, and so are they when the dedented line
        // continues the paragraph an item's text opened (a backtick fence
        // whose info string holds a backtick is text, not a fence).
        for (content, line, message) in [
            (
                "- p\n  - a\n  - b\n\n  - c\n\n  - d\n",
                3,
                "Missing blank line between list items",
            ),
            (
                "- p\n  - a\n  - ```lang`bad\n  more\n  - c\n\n  - d\n",
                6,
                "Unexpected blank line between list items",
            ),
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            assert_eq!(warnings[0].line, line, "{content:?}");
            assert_eq!(warnings[0].message, message, "{content:?}");
        }
    }

    #[test]
    fn lists_of_different_marker_types_are_separate_lists() {
        // A bullet list followed by an ordered list, or one bullet character
        // followed by another, is two lists at that level, so a tight pair
        // and a loose pair next to each other are each consistent and the
        // blank line between the second pair stays. Nested or not.
        for content in [
            "- parent\n  - bullet a\n  - bullet b\n  1. ordered a\n\n  2. ordered b\n- next\n",
            "- parent\n  - dash a\n  - dash b\n  * star a\n\n  * star b\n- next\n",
            "- parent\n  1. dot a\n  2. dot b\n  1) paren a\n\n  2) paren b\n- next\n",
            "- dash a\n- dash b\n* star a\n\n* star b\n",
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
            assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content, "{content:?}");
        }

        // Positive controls: with one marker type throughout, the four items
        // are one list whose gaps disagree, and the blank line goes.
        for (content, line) in [
            ("- parent\n  - a\n  - b\n  - c\n\n  - d\n- next\n", 5),
            ("- parent\n  1. a\n  2. b\n  3. c\n\n  4. d\n- next\n", 5),
            ("- a\n- b\n- c\n\n- d\n", 4),
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            assert_eq!(warnings[0].line, line, "{content:?}");
            assert_eq!(
                warnings[0].message, "Unexpected blank line between list items",
                "{content:?}"
            );
        }
    }

    #[test]
    fn siblings_at_a_different_indent_are_not_the_nested_list() {
        // The siblings at column 2 sit left of the parent's content column, so
        // they continue the outer list, which is loose throughout; the child
        // list at column 3 is tight throughout. Neither is reported, and the
        // fix leaves the child list tight.
        let content = " - parent\n   - child a\n   - child b\n\n  - sibling a\n\n  - sibling b\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);

        // Inside a blockquote the columns count from the quote's content, so
        // an indent before the `>` does not make the child list a sibling of
        // its parent: the loose child list is consistent on its own.
        let content = " > - parent\n>   - child a\n>\n>   - child b\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);

        // Positive controls: an inconsistent gap in either list is reported
        // against that list, on the blank line that makes it loose.
        for (content, line, message) in [
            (
                " - parent\n   - child a\n   - child b\n\n  - sibling a\n  - sibling b\n",
                4,
                "Unexpected blank line between list items",
            ),
            (
                " - parent\n   - child a\n\n   - child b\n   - child c\n  - sibling\n",
                3,
                "Unexpected blank line between list items",
            ),
            (
                " > - parent\n>   - child a\n>\n>   - child b\n>   - child c\n",
                3,
                "Unexpected blank line between list items",
            ),
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            assert_eq!(warnings[0].line, line, "{content:?}");
            assert_eq!(warnings[0].message, message, "{content:?}");
        }
    }

    #[test]
    fn lists_in_different_blockquotes_are_different_lists() {
        // A blockquote inside a list item holds its own list; a `>` left of
        // the item's content starts another blockquote outside the item, and
        // a blank line ends a blockquote, so the items after either are a
        // different list. Each list here is consistent on its own, and the
        // fix must not remove a blank line that separates two blockquotes.
        for content in [
            "- p\n  >- b1\n  >\n  >- b2\n>- c\n>- d\n",
            "- p\n  > - b1\n  > - b2\n\n  > - b3\n",
            "> - a\n>   - b\n>   - c\n\n>   - d\n",
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
            assert_eq!(fix(content, ListItemSpacingStyle::Consistent), content);
        }

        // Positive controls: a bare `>` at the list's own depth is the gap
        // between its items, whether or not another blockquote follows.
        for (content, line, message) in [
            (
                "- p\n  > - b1\n  > - b2\n  >\n  > - b3\n",
                4,
                "Unexpected blank line between list items",
            ),
            (
                "- p\n  >- b1\n  >- b2\n  >\n  >- b3\n>- c\n>- d\n",
                4,
                "Unexpected blank line between list items",
            ),
        ] {
            let warnings = check(content, ListItemSpacingStyle::Consistent);
            assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
            assert_eq!(warnings[0].line, line, "{content:?}");
            assert_eq!(warnings[0].message, message, "{content:?}");
        }
    }

    #[test]
    fn a_line_that_ends_the_top_level_list_starts_another_after_it() {
        // A fence or HTML block at column 0, or a blank line that ends a
        // blockquote, closes the top-level list as well as a nested one. The
        // items after it are another list, so there is no gap to judge
        // between the two, and a blank line between two blockquotes is not
        // list spacing the fix may remove.
        for (content, style) in [
            ("- p\n```\n```\n- q\n", ListItemSpacingStyle::Loose),
            ("- p\n<!-- x -->\n- q\n", ListItemSpacingStyle::Loose),
            ("> - a\n> - b\n\n> - c\n", ListItemSpacingStyle::Consistent),
            ("> - a\n> - b\n\n> - c\n", ListItemSpacingStyle::Tight),
        ] {
            let warnings = check(content, style.clone());
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
            assert_eq!(fix(content, style), content);
        }

        // The blockquote after the fence is not inside `p`, so the quoted
        // items are one list and `loose` wants a blank line between them.
        let content = "- p\n```\n```\n  > - b\n  lazy\n> - c\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 6);
        assert_eq!(warnings[0].message, "Missing blank line between list items");
        assert_eq!(
            fix(content, ListItemSpacingStyle::Loose),
            "- p\n```\n```\n  > - b\n  lazy\n>\n> - c\n"
        );

        // Positive control: a bare `>` inside one blockquote is list spacing.
        let content = "> - a\n>\n> - b\n> - c\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].message, "Unexpected blank line between list items");
    }

    #[test]
    fn explicit_style_applies_to_nested_lists() {
        // `loose` wants a blank line between the nested items too, and the
        // fix inserts it there; the parent gap is already loose.
        let content = "- a\n\n- b\n  - b1\n  - b2\n";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 5);
        assert_eq!(warnings[0].message, "Missing blank line between list items");
        assert_eq!(
            fix(content, ListItemSpacingStyle::Loose),
            "- a\n\n- b\n  - b1\n\n  - b2\n"
        );

        // `tight` removes the blank line between nested items and leaves a
        // tight parent alone.
        let content = "- a\n- b\n  - b1\n\n  - b2\n";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 4);
        assert_eq!(fix(content, ListItemSpacingStyle::Tight), "- a\n- b\n  - b1\n  - b2\n");
    }

    // ── Structural blank lines (code blocks, tables, HTML) ──────────

    #[test]
    fn code_block_in_tight_list_no_false_positive() {
        // Blank line after closing fence is structural (required by MD031), not a separator
        let content = "\
- Item 1 with code:

  ```python
  print('hello')
  ```

- Item 2 simple.
- Item 3 simple.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank after code block should not make item 1 appear loose"
        );
    }

    #[test]
    fn table_in_tight_list_no_false_positive() {
        // Blank line after table is structural (required by MD058), not a separator
        let content = "\
- Item 1 with table:

  | Col 1 | Col 2 |
  |-------|-------|
  | A     | B     |

- Item 2 simple.
- Item 3 simple.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank after table should not make item 1 appear loose"
        );
    }

    #[test]
    fn html_block_in_tight_list_no_false_positive() {
        let content = "\
- Item 1 with HTML:

  <details>
  <summary>Click</summary>
  Content
  </details>

- Item 2 simple.
- Item 3 simple.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank after HTML block should not make item 1 appear loose"
        );
    }

    #[test]
    fn blockquote_in_tight_list_no_false_positive() {
        // Blank line around a blockquote in a list item is structural, not a separator
        let content = "\
- Item 1 with quote:

  > This is a blockquote
  > with multiple lines.

- Item 2 simple.
- Item 3 simple.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank around blockquote should not make item 1 appear loose"
        );
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Blockquote in tight list should not trigger a violation"
        );
    }

    #[test]
    fn blockquote_multiple_items_with_quotes_tight() {
        // Multiple items with blockquotes should all be treated as structural
        let content = "\
- Item 1:

  > Quote A

- Item 2:

  > Quote B

- Item 3 plain.
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Multiple items with blockquotes should remain tight"
        );
    }

    #[test]
    fn blockquote_mixed_with_genuine_loose_gap() {
        // A blockquote item followed by a genuine loose gap should still be detected
        let content = "\
- Item 1:

  > Quote

- Item 2 plain.

- Item 3 plain.
";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert!(
            !warnings.is_empty(),
            "Genuine loose gap between Item 2 and Item 3 should be flagged"
        );
    }

    #[test]
    fn blockquote_single_line_in_tight_list() {
        let content = "\
- Item 1:

  > Single line quote.

- Item 2.
- Item 3.
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Single-line blockquote should be structural"
        );
    }

    #[test]
    fn blockquote_in_ordered_list_tight() {
        let content = "\
1. Item 1:

   > Quoted text in ordered list.

1. Item 2.
1. Item 3.
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Blockquote in ordered list should be structural"
        );
    }

    #[test]
    fn nested_blockquote_in_tight_list() {
        let content = "\
- Item 1:

  > Outer quote
  > > Nested quote

- Item 2.
- Item 3.
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Nested blockquote in tight list should be structural"
        );
    }

    #[test]
    fn blockquote_as_entire_item_is_loose() {
        // When a blockquote IS the item content (not nested within text),
        // a trailing blank line is a genuine loose gap, not structural.
        let content = "\
- > Quote is the entire item content.

- Item 2.
- Item 3.
";
        let warnings = check(content, ListItemSpacingStyle::Tight);
        assert!(
            !warnings.is_empty(),
            "Blank after blockquote-only item is a genuine loose gap"
        );
    }

    #[test]
    fn mixed_code_and_table_in_tight_list() {
        let content = "\
1. Item with code:

   ```markdown
   This is some Markdown
   ```

1. Simple item.
1. Item with table:

   | Col 1 | Col 2 |
   |:------|:------|
   | Row 1 | Row 1 |
   | Row 2 | Row 2 |
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Mix of code blocks and tables should not cause false positives"
        );
    }

    #[test]
    fn code_block_with_genuinely_loose_gaps_still_warns() {
        // Item 1 has structural blank (code block), items 2-3 have genuine blank separator
        // Items 2-3 are genuinely loose, item 3-4 is tight → inconsistent
        let content = "\
- Item 1:

  ```bash
  echo hi
  ```

- Item 2

- Item 3
- Item 4
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Genuine inconsistency with code blocks should still be flagged"
        );
    }

    #[test]
    fn all_items_have_code_blocks_no_warnings() {
        let content = "\
- Item 1:

  ```python
  print(1)
  ```

- Item 2:

  ```python
  print(2)
  ```

- Item 3:

  ```python
  print(3)
  ```
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "All items with code blocks should be consistently tight"
        );
    }

    #[test]
    fn tilde_fence_code_block_in_list() {
        let content = "\
- Item 1:

  ~~~
  code here
  ~~~

- Item 2 simple.
- Item 3 simple.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Tilde fences should be recognized as structural content"
        );
    }

    #[test]
    fn nested_list_with_code_block() {
        let content = "\
- Item 1
  - Nested with code:

    ```
    nested code
    ```

  - Nested simple.
- Item 2
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Nested list with code block should not cause false positives"
        );
    }

    #[test]
    fn tight_style_with_code_block_no_warnings() {
        let content = "\
- Item 1:

  ```
  code
  ```

- Item 2.
- Item 3.
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Tight style should not warn about structural blanks around code blocks"
        );
    }

    #[test]
    fn loose_style_with_code_block_missing_separator() {
        // Loose style requires blank line between every pair of items.
        // Items 2-3 have no blank → should warn
        let content = "\
- Item 1:

  ```
  code
  ```

- Item 2.
- Item 3.
";
        let warnings = check(content, ListItemSpacingStyle::Loose);
        assert_eq!(
            warnings.len(),
            1,
            "Loose style should still require blank between simple items"
        );
        assert!(warnings[0].message.contains("Missing"));
    }

    #[test]
    fn blockquote_list_with_code_block() {
        let content = "\
> - Item 1:
>
>   ```
>   code
>   ```
>
> - Item 2.
> - Item 3.
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Blockquote-prefixed list with code block should not cause false positives"
        );
    }

    // ── Indented code block (not fenced) in list item ─────────────────

    #[test]
    fn indented_code_block_in_list_no_false_positive() {
        // A 4-space indented code block inside a list item should be treated
        // as structural content, not trigger a loose gap detection.
        let content = "\
1. Item with indented code:

       some code here
       more code

1. Simple item
1. Another item
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank after indented code block should not make item 1 appear loose"
        );
    }

    // ── Issue #787: the marker-line exemption ends where MD031 does ───

    #[test]
    fn fence_on_marker_line_keeps_its_structural_blank() {
        // A fence opened on the marker line itself needs the blank line above it
        // (MD031), so tight mode must not remove it. One to four spaces after the
        // marker all leave the fence at the item's content column, so all four are
        // genuine fenced blocks.
        for spaces in 1..=4 {
            let pad = " ".repeat(spaces);
            let indent = " ".repeat(spaces + 1);
            let content = format!("- a\n\n-{pad}```\n{indent}code\n{indent}```\n- c\n");
            assert!(
                check(&content, ListItemSpacingStyle::Tight).is_empty(),
                "a fence on the marker line with {spaces} space(s) opens a fenced block, so its blank is structural"
            );
            assert_eq!(
                fix(&content, ListItemSpacingStyle::Tight),
                content,
                "tight fix must keep the blank MD031 requires ({spaces} space(s))"
            );
        }
    }

    #[test]
    fn over_indented_fence_on_marker_line_is_an_indented_block_not_an_exemption() {
        // Five spaces after the marker put the content column at 2, leaving the
        // fence at a relative indent of 4: an *indented* code block, which MD031
        // says nothing about. The blank above it is an ordinary loose separator and
        // tight mode must still remove it.
        for fence in ["```", "~~~"] {
            let content = format!("- a\n\n-     {fence}\n      code\n      {fence}\n- c\n");
            let warnings = check(&content, ListItemSpacingStyle::Tight);
            assert_eq!(
                warnings.len(),
                1,
                "no fenced block starts here, so the blank is a loose gap ({fence}): {warnings:?}"
            );
            assert_eq!(
                fix(&content, ListItemSpacingStyle::Tight),
                format!("- a\n-     {fence}\n      code\n      {fence}\n- c\n"),
                "tight fix must remove a blank that MD031 does not require ({fence})"
            );
        }
    }

    // ── Code block in middle of item with text after ────────────────

    #[test]
    fn code_block_in_middle_of_item_text_after_is_genuinely_loose() {
        // When a code block is in the middle of an item and there's regular text
        // after it, a blank line before the next item IS a genuine separator (loose),
        // not structural. The last non-blank line before item 2 is "Some text after
        // the code block." which is NOT structural content.
        let content = "\
1. Item with code in middle:

   ```
   code
   ```

   Some text after the code block.

1. Simple item
1. Another item
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Blank line after regular text (not structural content) is a genuine loose gap"
        );
    }

    // ── Fix: tight mode preserves structural blanks ──────────────────

    #[test]
    fn tight_fix_preserves_structural_blanks_around_code_blocks() {
        // When style is tight, the fix should NOT remove structural blank lines
        // around code blocks inside list items. Those blanks are required by MD031.
        let content = "\
- Item 1:

  ```
  code
  ```

- Item 2.
- Item 3.
";
        let fixed = fix(content, ListItemSpacingStyle::Tight);
        assert_eq!(
            fixed, content,
            "Tight fix should not remove structural blanks around code blocks"
        );
    }

    // ── Issue #461: 4-space indented code block in loose list ──────────

    #[test]
    fn four_space_indented_fence_in_loose_list_no_false_positive() {
        // Reproduction case from issue #461 comment by @sisp.
        // The fenced code block uses 4-space indentation inside an ordered list.
        // The blank line after the closing fence is structural (required by MD031)
        // and must not create a false "Missing blank line" warning.
        let content = "\
1. First item

1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "Structural blank after 4-space indented code block should not cause false positive"
        );
    }

    #[test]
    fn four_space_indented_fence_tight_style_no_warnings() {
        let content = "\
1. First item
1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        assert!(
            check(content, ListItemSpacingStyle::Tight).is_empty(),
            "Tight style should not warn about structural blanks with 4-space fences"
        );
    }

    #[test]
    fn four_space_indented_fence_loose_style_no_warnings() {
        // All non-structural gaps are loose, structural gaps are excluded.
        let content = "\
1. First item

1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        assert!(
            check(content, ListItemSpacingStyle::Loose).is_empty(),
            "Loose style should not warn when structural gaps are the only non-loose gaps"
        );
    }

    #[test]
    fn structural_gap_with_genuine_inconsistency_still_warns() {
        // Item 1 has a structural code block. Items 2-3 are genuinely loose,
        // but items 3-4 are tight → genuine inconsistency should still warn.
        let content = "\
1. First item with code:

    ```json
    {\"key\": \"value\"}
    ```

1. Second item

1. Third item
1. Fourth item
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Genuine loose/tight inconsistency should still warn even with structural gaps"
        );
    }

    #[test]
    fn four_space_fence_fix_is_idempotent() {
        // Fix should not modify a list that has only structural gaps and
        // genuine loose gaps — it's already consistent.
        let content = "\
1. First item

1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        let fixed = fix(content, ListItemSpacingStyle::Consistent);
        assert_eq!(fixed, content, "Fix should be a no-op for lists with structural gaps");
        let fixed_twice = fix(&fixed, ListItemSpacingStyle::Consistent);
        assert_eq!(fixed, fixed_twice, "Fix should be idempotent");
    }

    #[test]
    fn four_space_fence_fix_does_not_insert_duplicate_blank() {
        // When tight style tries to fix, it should not insert a blank line
        // before item 3 when one already exists (structural).
        let content = "\
1. First item
1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        let fixed = fix(content, ListItemSpacingStyle::Tight);
        assert_eq!(fixed, content, "Tight fix should not modify structural blanks");
    }

    #[test]
    fn mkdocs_flavor_code_block_in_list_no_false_positive() {
        // MkDocs flavor with code block inside a list item.
        // Reported by @sisp in issue #461 comment.
        let content = "\
1. First item

1. Second item with code block:

    ```json
    {\"key\": \"value\"}
    ```

1. Third item
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let rule = MD076ListItemSpacing::new(ListItemSpacingStyle::Consistent);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MkDocs flavor with structural code block blank should not produce false positive, got: {warnings:?}"
        );
    }

    // ── Issue #500: code block inside list item splits list blocks ─────

    #[test]
    fn code_block_in_second_item_detects_inconsistency() {
        // A code block inside item 2 must not split the list into separate blocks.
        // Items 1-2 are tight, items 3-4 are loose → inconsistent.
        let content = "\
# Test

- Lorem ipsum dolor sit amet.
- Lorem ipsum dolor sit amet.

    ```yaml
    hello: world
    ```

- Lorem ipsum dolor sit amet.

- Lorem ipsum dolor sit amet.
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Should detect inconsistent spacing when code block is inside a list item"
        );
    }

    #[test]
    fn code_block_in_item_all_tight_no_warnings() {
        // All non-structural gaps are tight → consistent, no warnings.
        let content = "\
- Item 1
- Item 2

    ```yaml
    hello: world
    ```

- Item 3
- Item 4
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "All tight gaps with structural code block should not warn"
        );
    }

    #[test]
    fn code_block_in_item_all_loose_no_warnings() {
        // All non-structural gaps are loose → consistent, no warnings.
        let content = "\
- Item 1

- Item 2

    ```yaml
    hello: world
    ```

- Item 3

- Item 4
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "All loose gaps with structural code block should not warn"
        );
    }

    #[test]
    fn code_block_in_ordered_list_detects_inconsistency() {
        let content = "\
1. First item
1. Second item

    ```json
    {\"key\": \"value\"}
    ```

1. Third item

1. Fourth item
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Ordered list with code block should still detect inconsistency"
        );
    }

    #[test]
    fn code_block_in_item_fix_removes_loose_outlier_on_tie() {
        // Gap classification: 1→2 tight, 2→3 structural (excluded — fenced
        // code block in the body of item 2), 3→4 loose. After excluding the
        // structural gap, that's a 1 tight / 1 loose tie. The tight
        // tie-breaker (analyze_block) warns the loose gap, so fix removes the
        // blank between items 3 and 4 rather than adding one between 1 and 2.
        let content = "\
- Item 1
- Item 2

    ```yaml
    code: here
    ```

- Item 3

- Item 4
";
        let fixed = fix(content, ListItemSpacingStyle::Consistent);
        assert!(
            fixed.contains("- Item 3\n- Item 4"),
            "Fix should remove blank line between items 3 and 4. Got:\n{fixed}"
        );
        assert!(
            !fixed.contains("- Item 1\n\n- Item 2"),
            "Fix should not insert a blank between items 1 and 2. Got:\n{fixed}"
        );
    }

    #[test]
    fn tilde_code_block_in_item_detects_inconsistency() {
        let content = "\
- Item 1
- Item 2

    ~~~
    code
    ~~~

- Item 3

- Item 4
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Tilde code block inside item should not prevent inconsistency detection"
        );
    }

    #[test]
    fn multiple_code_blocks_all_tight_no_warnings() {
        // All non-structural gaps are tight → consistent.
        let content = "\
- Item 1

    ```
    code1
    ```

- Item 2

    ```
    code2
    ```

- Item 3
- Item 4
";
        assert!(
            check(content, ListItemSpacingStyle::Consistent).is_empty(),
            "All non-structural gaps are tight, so list is consistent"
        );
    }

    #[test]
    fn code_block_with_mixed_genuine_gaps_warns() {
        // Items 1-2 structural, 2-3 loose, 3-4 tight → genuine inconsistency
        let content = "\
- Item 1

    ```
    code1
    ```

- Item 2

- Item 3
- Item 4
";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert!(
            !warnings.is_empty(),
            "Mixed genuine gaps (loose + tight) with structural code block should still warn"
        );
    }

    // ── allow-loose-continuation ─────────────────────────────────────

    #[test]
    fn continuation_loose_tight_style_default_warns() {
        // Default (allow_loose_continuation=false): blank lines around
        // continuation paragraphs are treated as loose gaps → violation
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.

  Continuation paragraph.

- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, false);
        assert!(
            !warnings.is_empty(),
            "Should warn about loose gaps when allow_loose_continuation is false"
        );
    }

    #[test]
    fn continuation_loose_tight_style_allowed_no_warnings() {
        // With allow_loose_continuation=true: blank lines around continuation
        // paragraphs are permitted even in tight mode
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.

  Continuation paragraph.

- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            warnings.is_empty(),
            "Should not warn when allow_loose_continuation is true, got: {warnings:?}"
        );
    }

    #[test]
    fn continuation_loose_mixed_items_warns() {
        // Even with allow_loose_continuation, genuinely loose inter-item gaps
        // (blank line between items that have no continuation) should still warn
        let content = "\
- Item 1.

- Item 2.
- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            !warnings.is_empty(),
            "Genuine loose gaps should still warn even with allow_loose_continuation"
        );
    }

    #[test]
    fn continuation_loose_consistent_mode() {
        // In consistent mode with allow_loose_continuation, continuation gaps
        // should not count toward loose/tight consistency
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.
- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Consistent, true);
        assert!(
            warnings.is_empty(),
            "Continuation gaps should not affect consistency when allowed, got: {warnings:?}"
        );
    }

    #[test]
    fn continuation_loose_fix_preserves_continuation_blanks() {
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.

  Continuation paragraph.

- Item 3.
";
        let fixed = fix_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert_eq!(fixed, content, "Fix should preserve continuation blank lines");
    }

    #[test]
    fn continuation_loose_fix_removes_genuine_loose_gaps() {
        let input = "\
- Item 1.

- Item 2.

- Item 3.
";
        let expected = "\
- Item 1.
- Item 2.
- Item 3.
";
        let fixed = fix_with_continuation(input, ListItemSpacingStyle::Tight, true);
        assert_eq!(fixed, expected);
    }

    #[test]
    fn continuation_loose_ordered_list() {
        let content = "\
1. Item 1.

   Continuation paragraph.

2. Item 2.

   Continuation paragraph.

3. Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            warnings.is_empty(),
            "Ordered list continuation should work too, got: {warnings:?}"
        );
    }

    #[test]
    fn continuation_loose_disabled_by_default() {
        // Verify the constructor defaults to false
        let rule = MD076ListItemSpacing::new(ListItemSpacingStyle::Tight);
        assert!(!rule.config.allow_loose_continuation);
    }

    #[test]
    fn continuation_loose_ordered_under_indented_ends_the_list() {
        // "1. " puts the item's content at column 3, so text at column 2
        // after a blank line is not a continuation of the item: it ends the
        // list, and the items after it are a list of their own, tight and
        // consistent. Nothing to report, in either style, whether or not
        // continuation gaps are allowed. Text at column 3 continues the item,
        // and its gap is a continuation gap that the default rejects, reported
        // at the blank line the fix removes, the one before the next item.
        let content = "\
1. Item 1.

  Under-indented text.

1. Item 2.
1. Item 3.
";
        for (style, allow) in [
            (ListItemSpacingStyle::Tight, true),
            (ListItemSpacingStyle::Tight, false),
            (ListItemSpacingStyle::Consistent, false),
        ] {
            let warnings = check_with_continuation(content, style, allow);
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
        }
        let content = "\
1. Item 1.

   Continuation text.

1. Item 2.
1. Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, false);
        assert_eq!(warnings.len(), 1, "{content:?}: {warnings:?}");
        assert_eq!(warnings[0].line, 4);
        assert_eq!(warnings[0].message, "Unexpected blank line between list items");
    }

    #[test]
    fn continuation_loose_mix_continuation_and_genuine_gaps() {
        // Some items have continuation (allowed), one gap is genuinely loose (not allowed)
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.

- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            !warnings.is_empty(),
            "Genuine loose gap between items 2-3 should warn even with continuation allowed"
        );
        // Only the genuine loose gap should warn, not the continuation gap
        assert_eq!(
            warnings.len(),
            1,
            "Expected exactly one warning for the genuine loose gap"
        );
    }

    #[test]
    fn continuation_loose_fix_mixed_preserves_continuation_removes_genuine() {
        // Fix should preserve continuation blanks but remove genuine loose gaps
        let input = "\
- Item 1.

  Continuation paragraph.

- Item 2.

- Item 3.
";
        let expected = "\
- Item 1.

  Continuation paragraph.

- Item 2.
- Item 3.
";
        let fixed = fix_with_continuation(input, ListItemSpacingStyle::Tight, true);
        assert_eq!(fixed, expected);
    }

    #[test]
    fn continuation_loose_after_code_block() {
        // Code block is structural, continuation after code block should also work
        let content = "\
- Item 1.

  ```python
  code
  ```

  Continuation after code.

- Item 2.
- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            warnings.is_empty(),
            "Code block + continuation should both be exempt, got: {warnings:?}"
        );
    }

    #[test]
    fn continuation_loose_style_does_not_interfere() {
        // With style=loose, allow-loose-continuation shouldn't change behavior —
        // loose style already requires blank lines everywhere
        let content = "\
- Item 1.

  Continuation paragraph.

- Item 2.

  Continuation paragraph.

- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Loose, true);
        assert!(
            warnings.is_empty(),
            "Loose style with continuation should not warn, got: {warnings:?}"
        );
    }

    #[test]
    fn continuation_loose_tight_no_continuation_content() {
        // All items are simple (no continuation), tight style should work normally
        let content = "\
- Item 1.
- Item 2.
- Item 3.
";
        let warnings = check_with_continuation(content, ListItemSpacingStyle::Tight, true);
        assert!(
            warnings.is_empty(),
            "Simple tight list should pass with allow_loose_continuation, got: {warnings:?}"
        );
    }

    // ── Config schema ──────────────────────────────────────────────────

    #[test]
    fn default_config_section_provides_style_key() {
        let rule = MD076ListItemSpacing::new(ListItemSpacingStyle::Consistent);
        let section = rule.default_config_section();
        assert!(section.is_some());
        let (name, value) = section.unwrap();
        assert_eq!(name, "MD076");
        if let toml::Value::Table(map) = value {
            assert!(map.contains_key("style"));
            assert!(map.contains_key("allow-loose-continuation"));
        } else {
            panic!("Expected Table value from default_config_section");
        }
    }
}
