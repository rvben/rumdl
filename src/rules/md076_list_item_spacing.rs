use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
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
pub struct MD076Config {
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
    /// Blank line required by another rule (MD031, MD058) after structural content.
    /// Excluded from consistency analysis — neither loose nor tight.
    Structural,
    /// Blank line after continuation content within a list item.
    /// Treated as `Structural` when `allow_loose_continuation` is enabled,
    /// or as `Loose` when disabled (default).
    ContinuationLoose,
}

/// Per-block analysis result shared by check() and fix().
struct BlockAnalysis {
    /// 1-indexed line numbers of items at this block's nesting level.
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

    /// Check whether a non-blank line is continuation content within a list item
    /// (indented text that is not itself a list marker).
    fn is_continuation_content(ctx: &LintContext, line_num: usize) -> bool {
        if let Some(info) = ctx.line_info(line_num) {
            // If this line has a list marker, it's an item, not continuation
            if info.list_item.is_some() {
                return false;
            }
            // Must have non-empty indented content
            let content = info.content(ctx.content);
            !content.trim().is_empty() && content.starts_with(' ')
        } else {
            false
        }
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
        // Check if the last non-blank line is continuation content
        if scan > first && Self::is_continuation_content(ctx, scan) {
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

    /// Analyze a single list block to determine which gaps need fixing.
    ///
    /// Returns `None` if the block has fewer than 2 items at its nesting level
    /// or if no gaps violate the configured style.
    fn analyze_block(
        ctx: &LintContext,
        block: &crate::lint_context::types::ListBlock,
        style: &ListItemSpacingStyle,
        allow_loose_continuation: bool,
    ) -> Option<BlockAnalysis> {
        // Only compare items at this block's own nesting level.
        // item_lines may include nested list items (higher marker_column) that belong
        // to a child list — those must not affect spacing analysis.
        let items: Vec<usize> = block
            .item_lines
            .iter()
            .copied()
            .filter(|&line_num| {
                ctx.line_info(line_num)
                    .and_then(|li| li.list_item.as_ref())
                    .map(|item| item.marker_column / 2 == block.nesting_level)
                    .unwrap_or(false)
            })
            .collect();

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
                // Majority wins; on a tie, prefer loose (warn tight).
                if loose_count >= tight_count {
                    (false, true)
                } else {
                    (true, false)
                }
            }
        };

        Some(BlockAnalysis {
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

        for block in &ctx.list_blocks {
            let Some(analysis) = Self::analyze_block(ctx, block, &self.config.style, allow_cont) else {
                continue;
            };

            for (i, &gap) in analysis.gaps.iter().enumerate() {
                let is_loose_violation = match gap {
                    GapKind::Loose => analysis.warn_loose_gaps,
                    GapKind::ContinuationLoose => !allow_cont && analysis.warn_loose_gaps,
                    _ => false,
                };

                if is_loose_violation {
                    let blanks = Self::inter_item_blanks(ctx, analysis.items[i], analysis.items[i + 1]);
                    if let Some(&blank_line) = blanks.first() {
                        let line_content = ctx
                            .line_info(blank_line)
                            .map(|li| li.content(ctx.content))
                            .unwrap_or("");
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: blank_line,
                            column: 1,
                            end_line: blank_line,
                            end_column: line_content.len() + 1,
                            message: "Unexpected blank line between list items".to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                } else if gap == GapKind::Tight && analysis.warn_tight_gaps {
                    let next_item = analysis.items[i + 1];
                    let line_content = ctx.line_info(next_item).map(|li| li.content(ctx.content)).unwrap_or("");
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: next_item,
                        column: 1,
                        end_line: next_item,
                        end_column: line_content.len() + 1,
                        message: "Missing blank line between list items".to_string(),
                        severity: Severity::Warning,
                        fix: None,
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

        for block in &ctx.list_blocks {
            let Some(analysis) = Self::analyze_block(ctx, block, &self.config.style, allow_cont) else {
                continue;
            };

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
    fn consistent_tie_prefers_loose() {
        let content = "- Item 1\n\n- Item 2\n- Item 3\n";
        let warnings = check(content, ListItemSpacingStyle::Consistent);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Missing"));
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
    fn code_block_in_item_fix_adds_missing_blanks() {
        // Items 1-2 are tight, items 3-4 are loose → majority loose → fix adds blank before item 2
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
            fixed.contains("- Item 1\n\n- Item 2"),
            "Fix should add blank line between items 1 and 2"
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
