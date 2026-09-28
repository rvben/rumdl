//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rule::{LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::blockquote::parse_blockquote_prefix;
use crate::utils::calculate_indentation_width_default;
use crate::utils::list_fix_guard::{Allowed, drop_structure_changing_fixes};
use crate::utils::list_indent_shift::{
    Continuation, EndedItems, Nesting, OwnerFrame, classify_continuation, close_ended_items, continuation_params,
    keep_ended_items_closed, move_owned_line,
};
use crate::utils::range_utils::calculate_match_range;
use toml;

mod md030_config;
pub use md030_config::MD030Config;

#[derive(Clone, Default)]
pub struct MD030ListMarkerSpace {
    config: MD030Config,
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            config: MD030Config {
                ul_single: crate::types::PositiveUsize::new(ul_single)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ul_multi: crate::types::PositiveUsize::new(ul_multi)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ol_single: crate::types::PositiveUsize::new(ol_single)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ol_multi: crate::types::PositiveUsize::new(ol_multi)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ol_align_column: crate::types::OlAlignColumn::default(),
            },
        }
    }

    fn from_config_struct(config: MD030Config) -> Self {
        Self { config }
    }

    /// Set the ordered-list alignment column. Intended for tests; production code
    /// configures this via `MD030.ol-align-column`. Panics on an out-of-range value
    /// (the config path rejects those with a diagnostic instead).
    #[cfg(test)]
    fn with_ol_align_column(mut self, column: usize) -> Self {
        self.config.ol_align_column =
            crate::types::OlAlignColumn::new(column).expect("test ol-align-column out of range");
        self
    }
}

impl Rule for MD030ListMarkerSpace {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Early return if no list content
        if self.should_skip(ctx) {
            return Ok(warnings);
        }

        let lines = ctx.raw_lines();

        // Track which lines we've already processed (to avoid duplicates)
        let mut processed_lines = std::collections::HashSet::new();

        // Re-spacing a marker moves its item's content column, and every line the
        // item owns has to move with it or the document changes meaning: a wider
        // `1.` marker leaves its `   1. inner` child under-indented and flattened,
        // and a narrower one leaves a nested fence four columns past the new
        // content column, where it turns into an indented code block. The moves
        // ride on the re-spacing warning's own fix (see `list_indent_shift`).
        //
        // Active list items (and inline bullets) whose content the current line may
        // continue. Because the loop walks top to bottom, every owning item is
        // already on the stack by the time we reach its content.
        let mut stack: Vec<OwnerFrame> = Vec::new();
        // The first warning of the outermost list being walked, and whether a
        // narrowed item in it would take in a line that cannot move out of it.
        // Its items shift only against each other, so a list that cannot be fixed
        // leaves the lists around it fixable.
        let mut list_warnings = 0;
        let mut unfixable = false;

        // Main pass: move each continuation/nested line with the items that own it
        // as the loop reaches it, and check parser-recognized list items.
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            let line_num_1based = line_num + 1;
            let line = lines[line_num];

            // Drop the items this line ends. A lazy continuation line ends nothing
            // and stays where it is.
            let before = stack.clone();
            let lazy = close_ended_items(ctx, line_num_1based, lines, &mut stack);

            // Skip code blocks, math blocks, PyMdown blocks, and MkDocs markdown HTML divs (grid cards use custom spacing)
            let is_list_item = line_info.list_item.is_some()
                && !line_info.in_code_block
                && !line_info.in_math_block
                && !line_info.in_pymdown_block
                && !line_info.in_mkdocs_html_markdown
                && !line_info.in_footnote_definition;

            if !is_list_item {
                // A continuation/nested line moves with the items that own it, and a
                // line that ends items keeps out of them once they narrow.
                if !lazy && !line.trim().is_empty() {
                    let mut owners = stack.clone();
                    match keep_ended_items_closed(ctx, line_num, &before, stack.len(), Nesting::Relative) {
                        EndedItems::StayClosed => {}
                        EndedItems::ShiftOut(frame) => owners.push(frame),
                        EndedItems::Unfixable => unfixable = true,
                    }
                    move_owned_line(ctx, line, line_num, &owners, &mut warnings);
                }
                continue;
            }

            processed_lines.insert(line_num_1based);
            let Some(list_info) = &line_info.list_item else {
                continue;
            };

            if stack.is_empty() {
                if std::mem::take(&mut unfixable) {
                    Self::decline_fixes(&mut warnings[list_warnings..]);
                }
                list_warnings = warnings.len();
            }

            // The item is content of its parent, so its leading indent moves too.
            move_owned_line(ctx, line, line_num, &stack, &mut warnings);

            let marker_end = list_info.marker_column + list_info.marker.len();

            // MD030 only applies when there is content after the marker.
            if !Self::has_content_after_marker(line, marker_end) {
                continue;
            }

            let actual_spaces = list_info.content_column.saturating_sub(marker_end);

            // Spacing comes from the shared MD030 config logic: the ol-align-column
            // override for ordered lists, otherwise the fixed single-/multi-line value.
            let is_multi_line = self.is_multi_line_list_item(ctx, line_num_1based, lines);
            let expected_spaces =
                self.config
                    .expected_spaces(list_info.is_ordered, is_multi_line, list_info.marker.len());

            let warning = (actual_spaces != expected_spaces).then(|| {
                warnings.push(self.spacing_fix_warning(
                    ctx,
                    line,
                    line_num,
                    marker_end..marker_end + actual_spaces,
                    expected_spaces,
                    format!("Spaces after list markers (Expected: {expected_spaces}; Actual: {actual_spaces})"),
                ));
                warnings.len() - 1
            });

            // Push this item's frame so the lines it owns move with it, and space any
            // inline nested bullet (`1. - x`), which gets its own frame.
            let own = expected_spaces as isize - actual_spaces as isize;
            if let Some(frame) = OwnerFrame::for_item(ctx, line_num_1based, own, warning) {
                stack.push(frame);
                if list_info.is_ordered {
                    self.align_inline_bullet(
                        ctx,
                        line_num_1based,
                        lines,
                        list_info.content_column,
                        &mut stack,
                        &mut warnings,
                    );
                }
            }
        }

        if unfixable {
            Self::decline_fixes(&mut warnings[list_warnings..]);
        }
        // Re-spacing must not change the structure; the pass below exists to
        // create lists, so it is not held to that.
        drop_structure_changing_fixes(ctx, &mut warnings, Allowed::Nothing);

        // Second pass: Detect list-like patterns the parser didn't recognize
        // This handles cases like "1.Text" where there's no space after the marker
        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip if already processed or in code block/front matter/math block
            if processed_lines.contains(&line_num) {
                continue;
            }
            if let Some(line_info) = ctx.lines.get(line_idx)
                && (line_info.in_code_block
                    || line_info.in_front_matter
                    || line_info.in_html_comment
                    || line_info.in_mdx_comment
                    || line_info.in_math_block
                    || line_info.in_pymdown_block
                    || line_info.in_mkdocs_html_markdown
                    || line_info.in_footnote_definition)
            {
                continue;
            }

            // Skip indented code blocks
            if self.is_indented_code_block(line, line_idx, lines) {
                continue;
            }

            // Try to detect list-like patterns using regex-based detection
            if let Some(warning) = self.check_unrecognized_list_marker(ctx, line, line_num, lines) {
                warnings.push(warning);
            }
        }

        Ok(warnings)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if ctx.content.is_empty() {
            return true;
        }

        // Fast byte-level check for list markers (including ordered lists)
        let bytes = ctx.content.as_bytes();
        !bytes.contains(&b'*')
            && !bytes.contains(&b'-')
            && !bytes.contains(&b'+')
            && !bytes.iter().any(|&b| b.is_ascii_digit())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD030Config);

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        // Derive fixes directly from check() so detection and fixing share one code path.
        // This guarantees that every violation check() reports is also fixed, with no
        // possibility of the two paths diverging due to mismatched skip conditions.
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }
}

impl MD030ListMarkerSpace {
    /// Check if a list item line has content after the marker
    /// Returns false if the line ends after the marker (with optional whitespace)
    /// MD030 only applies when there IS content on the same line as the marker
    #[inline]
    /// Drop the fixes of one list's warnings. Re-spacing only some of its items
    /// would shift them against the rest and change the nesting, so the list's
    /// spacing fixes are kept or dropped together.
    fn decline_fixes(warnings: &mut [LintWarning]) {
        for warning in warnings {
            warning.fix = None;
        }
    }

    fn has_content_after_marker(line: &str, marker_end: usize) -> bool {
        if marker_end >= line.len() {
            return false;
        }
        !line[marker_end..].trim().is_empty()
    }

    /// Build a warning that replaces the whitespace run at byte range `span` within
    /// the line at `line_idx` (0-based) with `want` spaces. Shared by the marker-
    /// spacing checks and the nested-content re-indentation.
    fn spacing_fix_warning(
        &self,
        ctx: &crate::lint_context::LintContext,
        line: &str,
        line_idx: usize,
        span: std::ops::Range<usize>,
        want: usize,
        message: String,
    ) -> LintWarning {
        let (start_line, start_col, end_line, end_col) =
            calculate_match_range(line_idx + 1, line, span.start, span.len());
        let base = ctx.line_offsets.get(line_idx).copied().unwrap_or(0);
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            message,
            fix: Some(crate::rule::Fix::new(
                base + span.start..base + span.end,
                " ".repeat(want),
            )),
        }
    }

    /// If the content at byte `content_col` of `line` is an inline unordered marker
    /// (`-`/`*`/`+` followed by spaces and then content, as in `1. - x`), return the
    /// byte offset of the spaces after that bullet and their current count. `None`
    /// for anything else (e.g. `1. -x` or `1. *emphasis*`, which aren't markers).
    fn inline_unordered_spaces(line: &str, content_col: usize) -> Option<(usize, usize)> {
        if !matches!(line.as_bytes().get(content_col), Some(b'-' | b'*' | b'+')) {
            return None;
        }
        let offset = content_col + 1;
        let rest = line.get(offset..)?;
        let spaces = rest.len() - rest.trim_start_matches(' ').len();
        if spaces == 0 || rest[spaces..].is_empty() {
            return None;
        }
        Some((offset, spaces))
    }

    /// Whether the list item on `line_num` spans multiple lines *as written* (it has
    /// continuation or nested content).
    ///
    /// Maintainer note: MD013's list reflow needs the multi-line shape of the
    /// *rewritten* item instead — plain prose continuation collapses onto the marker
    /// line during reflow — so it deliberately does not reuse this and derives its own
    /// `is_multi` from the post-reflow shape (see `md013_line_length.rs`). The two are
    /// related but technically distinct: this reflects the current text, MD013's the
    /// predicted output. The spacing *policy* they share lives in
    /// [`MD030Config::expected_spaces`]; if the meaning of "multi-line" changes here,
    /// check whether MD013's reflow prediction needs the matching change.
    fn is_multi_line_list_item(&self, ctx: &crate::lint_context::LintContext, line_num: usize, lines: &[&str]) -> bool {
        let Some((marker_column, bq_level, min_indent)) = continuation_params(ctx, line_num) else {
            return false;
        };
        Self::has_continuation(ctx, line_num, lines, marker_column, bq_level, min_indent)
    }

    /// Whether any line after `line_num` (1-based) belongs to an item with the given
    /// continuation threshold, scanning until the item ends. Shared by the multi-line
    /// check and the inline-bullet check.
    fn has_continuation(
        ctx: &crate::lint_context::LintContext,
        line_num: usize,
        lines: &[&str],
        marker_column: usize,
        bq_level: usize,
        min_indent: usize,
    ) -> bool {
        for next in (line_num + 1)..=lines.len() {
            match classify_continuation(ctx, next, lines, marker_column, bq_level, min_indent) {
                Continuation::Belongs => return true,
                Continuation::Ends => break,
                Continuation::Skip => {}
            }
        }
        false
    }

    /// The first item of a nested unordered list shares the ordered marker's line
    /// (`1. - x`), where the parser exposes only the outer marker. Space that inline
    /// bullet like a sibling bullet on its own line, and push a frame so its own
    /// continuation lines move with it.
    fn align_inline_bullet(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_num: usize,
        lines: &[&str],
        content_column: usize,
        stack: &mut Vec<OwnerFrame>,
        warnings: &mut Vec<LintWarning>,
    ) {
        let line = lines[line_num - 1];
        let Some((offset, spaces)) = Self::inline_unordered_spaces(line, content_column) else {
            return;
        };
        let bullet_content_col = offset + spaces;
        // ul-multi if the bullet itself spans lines, else ul-single, measured with a
        // raw indent (bq_level 0) like a bullet that begins its own line. The inline
        // bullet is always unordered (marker width 1).
        let multi = Self::has_continuation(ctx, line_num, lines, content_column, 0, bullet_content_col);
        let want = self.config.expected_spaces(false, multi, 1);
        if spaces == want {
            return;
        }
        warnings.push(self.spacing_fix_warning(
            ctx,
            line,
            line_num - 1,
            offset..offset + spaces,
            want,
            format!("Spaces after list markers (Expected: {want}; Actual: {spaces})"),
        ));
        stack.push(OwnerFrame {
            marker_column: content_column,
            bq_level: 0,
            min_indent: bullet_content_col,
            own: want as isize - spaces as isize,
            warning: Some(warnings.len() - 1),
        });
    }

    /// Detect list-like patterns that the parser didn't recognize (e.g., "1.Text" with no space)
    /// This implements user-intention-based detection: if it looks like a list item, flag it
    fn check_unrecognized_list_marker(
        &self,
        ctx: &crate::lint_context::LintContext,
        line: &str,
        line_num: usize,
        lines: &[&str],
    ) -> Option<LintWarning> {
        // Strip blockquote prefix to analyze the content.
        // Track the prefix length so fix positions are relative to the original line.
        let (bq_prefix_len, content) = match parse_blockquote_prefix(line) {
            Some(parsed) => (parsed.prefix.len(), parsed.content),
            None => (0, line),
        };

        let trimmed = content.trim_start();
        let indent_len = content.len() - trimmed.len();

        // Note: We intentionally do NOT apply heuristic detection to unordered list markers
        // (*, -, +) because they have too many non-list uses: emphasis, globs, diffs, etc.
        // The parser handles valid unordered list items; we only do heuristic detection
        // for ordered lists where "1.Text" is almost always a list item with missing space.

        // Check for ordered list markers (digits followed by .) without proper spacing
        if let Some(dot_pos) = trimmed.find('.') {
            let before_dot = &trimmed[..dot_pos];
            if before_dot.chars().all(|c| c.is_ascii_digit()) && !before_dot.is_empty() {
                let after_dot = &trimmed[dot_pos + 1..];
                // Only flag if there's content directly after the marker (no space, no tab)
                if !after_dot.is_empty() && !after_dot.starts_with(' ') && !after_dot.starts_with('\t') {
                    let first_char = after_dot.chars().next().unwrap_or(' ');

                    // For CLEAR user intent, only flag if:
                    // 1. Starts with uppercase letter (strong list indicator), OR
                    // 2. Starts with [ or ( (link/paren content)
                    // Lowercase and digits are ambiguous (could be decimal, version, etc.)
                    let is_clear_intent = first_char.is_ascii_uppercase() || first_char == '[' || first_char == '(';

                    if is_clear_intent {
                        let is_multi_line = self.is_multi_line_for_unrecognized(line_num, lines);
                        // Ordered marker `<digits>.` has width `before_dot.len() + 1`.
                        let expected_spaces = self.config.expected_spaces(true, is_multi_line, before_dot.len() + 1);

                        let marker = format!("{before_dot}.");
                        let marker_pos = indent_len;
                        let marker_end = marker_pos + marker.len();
                        // Offset from the start of the original line (including blockquote prefix).
                        let offset_in_line = bq_prefix_len + marker_end;

                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num, line, offset_in_line, 0);

                        let correct_spaces = " ".repeat(expected_spaces);
                        let line_start_byte = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                        let fix_position = line_start_byte + offset_in_line;

                        return Some(LintWarning {
                            rule_name: Some("MD030".to_string()),
                            severity: Severity::Warning,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Spaces after list markers (Expected: {expected_spaces}; Actual: 0)"),
                            fix: Some(crate::rule::Fix::new(fix_position..fix_position, correct_spaces)),
                        });
                    }
                }
            }
        }

        None
    }

    /// Simplified multi-line check for unrecognized list items
    fn is_multi_line_for_unrecognized(&self, line_num: usize, lines: &[&str]) -> bool {
        // For unrecognized list items, we can't rely on parser info
        // Check if the next line exists and appears to be a continuation
        if line_num < lines.len() {
            let next_line = lines[line_num]; // line_num is 1-based, so this is the next line
            let next_trimmed = next_line.trim();
            // If next line is non-empty and indented, it might be a continuation
            if !next_trimmed.is_empty() && next_line.starts_with(' ') {
                return true;
            }
        }
        false
    }

    /// Check if a line is part of an indented code block (4+ columns with blank line before)
    fn is_indented_code_block(&self, line: &str, line_idx: usize, lines: &[&str]) -> bool {
        // Must have 4+ columns of indentation (accounting for tab expansion)
        if calculate_indentation_width_default(line) < 4 {
            return false;
        }

        // If it's the first line, it's not an indented code block
        if line_idx == 0 {
            return false;
        }

        // Check if there's a blank line before this line or before the start of the indented block
        if self.has_blank_line_before_indented_block(line_idx, lines) {
            return true;
        }

        false
    }

    /// Check if there's a blank line before the start of an indented block
    fn has_blank_line_before_indented_block(&self, line_idx: usize, lines: &[&str]) -> bool {
        // Walk backwards to find the start of the indented block
        let mut current_idx = line_idx;

        // Find the first line in this indented block
        while current_idx > 0 {
            let current_line = lines[current_idx];
            let prev_line = lines[current_idx - 1];

            // If current line is not indented (< 4 columns), we've gone too far
            if calculate_indentation_width_default(current_line) < 4 {
                break;
            }

            // If previous line is not indented, check if it's blank
            if calculate_indentation_width_default(prev_line) < 4 {
                return prev_line.trim().is_empty();
            }

            current_idx -= 1;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use indoc::indoc;

    /// Assert that running `fix()` on content with violations produces output that
    /// passes `check()` with zero remaining violations.
    fn assert_fix_resolves_all_violations(rule: &MD030ListMarkerSpace, content: &str) {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let before = rule.check(&ctx).unwrap();
        assert!(
            !before.is_empty(),
            "Expected violations but check() found none in:\n{content}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let after = rule.check(&ctx_fixed).unwrap();
        assert!(
            after.is_empty(),
            "fix() left {} violation(s) unresolved:\n{:?}\nOriginal:\n{content}\nFixed:\n{fixed}",
            after.len(),
            after
        );
    }

    #[test]
    fn test_fix_keeps_a_parent_paragraph_out_of_the_shifted_child() {
        // Column 5 is below the child's content column (6) but reaches its fixed
        // one (4): the paragraph belongs to the parent and must stay there.
        let rule = MD030ListMarkerSpace::default();
        let content = "* parent\n\n  *   child\n\n     Parent paragraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "* parent\n\n  * child\n\n  Parent paragraph.\n"
        );
    }

    #[test]
    fn test_unfixable_list_leaves_other_lists_fixable() {
        // Narrowing `2)` would pull the indented code line into the item, so that
        // list stays unfixed. The list above it is independent and still fixed.
        let rule = MD030ListMarkerSpace::default();
        let content = "-   fixable\n\nParagraph.\n\n 2)   item\n\n    code\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].fix.is_some(), "{warnings:?}");
        assert!(warnings[1].fix.is_none(), "{warnings:?}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "- fixable\n\nParagraph.\n\n 2)   item\n\n    code\n"
        );
    }

    #[test]
    fn test_unfixable_list_leaves_a_later_list_fixable() {
        let rule = MD030ListMarkerSpace::default();
        let content = " 2)   item\n\n    code\n\nParagraph.\n\n-   fixable\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].fix.is_none(), "{warnings:?}");
        assert!(warnings[1].fix.is_some(), "{warnings:?}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            " 2)   item\n\n    code\n\nParagraph.\n\n- fixable\n"
        );
    }

    #[test]
    fn test_basic_functionality() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Expect warnings for lines with too many spaces after the marker
        assert_eq!(
            result.len(),
            2,
            "Should flag lines with too many spaces after list marker"
        );
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:")
                    && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_nested_emphasis_not_flagged_issue_278() {
        // Issue #278: Nested emphasis like *text **bold** more* should not trigger MD030
        let rule = MD030ListMarkerSpace::default();

        // This is emphasis with nested bold - NOT a list item
        let content = "*This text is **very** important*";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Nested emphasis should not trigger MD030, got: {result:?}"
        );

        // Simple emphasis - NOT a list item
        let content2 = "*Hello World*";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Simple emphasis should not trigger MD030, got: {result2:?}"
        );

        // Bold text - NOT a list item
        let content3 = "**bold text**";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert!(
            result3.is_empty(),
            "Bold text should not trigger MD030, got: {result3:?}"
        );

        // Bold+italic - NOT a list item
        let content4 = "***bold and italic***";
        let ctx4 = LintContext::new(content4, crate::config::MarkdownFlavor::Standard, None);
        let result4 = rule.check(&ctx4).unwrap();
        assert!(
            result4.is_empty(),
            "Bold+italic should not trigger MD030, got: {result4:?}"
        );

        // Actual list item with proper spacing - should NOT trigger
        let content5 = "* Item with space";
        let ctx5 = LintContext::new(content5, crate::config::MarkdownFlavor::Standard, None);
        let result5 = rule.check(&ctx5).unwrap();
        assert!(
            result5.is_empty(),
            "Properly spaced list item should not trigger MD030, got: {result5:?}"
        );
    }

    #[test]
    fn test_empty_marker_line_not_flagged_issue_288() {
        // Issue #288: List items with no content on the marker line should not trigger MD030
        // The space requirement only applies when there IS content after the marker
        let rule = MD030ListMarkerSpace::default();

        // Case 1: Unordered list with empty marker line followed by code block
        let content = "-\n    ```python\n    print(\"code\")\n    ```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty unordered marker line with code continuation should not trigger MD030, got: {result:?}"
        );

        // Case 2: Ordered list with empty marker line followed by code block
        let content = "1.\n    ```python\n    print(\"code\")\n    ```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty ordered marker line with code continuation should not trigger MD030, got: {result:?}"
        );

        // Case 3: Empty marker line followed by paragraph continuation
        let content = "-\n    This is a paragraph continuation\n    of the list item.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty marker line with paragraph continuation should not trigger MD030, got: {result:?}"
        );

        // Case 4: Nested list with empty marker line
        let content = "- Parent item\n  -\n      Nested content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Nested empty marker line should not trigger MD030, got: {result:?}"
        );

        // Case 5: Multiple list items, some with empty markers
        let content = "- Item with content\n-\n    Code block\n- Another item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Mixed empty/non-empty marker lines should not trigger MD030 for empty ones, got: {result:?}"
        );
    }

    #[test]
    fn test_marker_with_content_still_flagged_issue_288() {
        // Ensure we still flag markers with content but wrong spacing
        let rule = MD030ListMarkerSpace::default();

        // Two spaces before content - should flag
        let content = "-  Two spaces before content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Two spaces after unordered marker should still trigger MD030"
        );

        // Ordered list with two spaces - should flag
        let content = "1.  Two spaces\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Two spaces after ordered marker should still trigger MD030"
        );

        // Normal list item - should NOT flag
        let content = "- Normal item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Normal list item should not trigger MD030, got: {result:?}"
        );
    }

    #[test]
    fn test_nested_items_with_4space_indent_are_detected() {
        // Nested list items indented with 4 spaces should be checked for marker spacing.
        // Previously, the check skipped any line with >= 4 columns of indentation,
        // treating them as indented code blocks even when the parser identified them as
        // list items.
        let rule = MD030ListMarkerSpace::new(3, 3, 1, 1);

        // Tight nested list (no blank line): the exact scenario from issue #565.
        // ul_single=3: the nested item "    - Nested wrong" has 1 space → violation.
        let content = "-   Top-level correct\n    - Nested wrong spacing\n    -   Nested correct\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Nested item with 1 space (ul_single=3) should be flagged; got: {result:?}"
        );
        assert_eq!(result[0].line, 2, "Violation should be on line 2");
        assert!(
            result[0].message.contains("Expected: 3") && result[0].message.contains("Actual: 1"),
            "Message should state expected/actual spaces; got: {}",
            result[0].message
        );

        // fix() must produce correct output for the tight nested case.
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "-   Top-level correct\n    -   Nested wrong spacing\n    -   Nested correct\n",
            "fix() should expand 1 space to ul_single=3 on the nested item"
        );

        // Nested unordered item with correct spacing should not be flagged
        let content_ok = "-   Top-level\n    -   Nested correct\n";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let result_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            result_ok.is_empty(),
            "Nested item with correct spacing should not be flagged; got: {result_ok:?}"
        );

        // Nested ordered item: 4 spaces indent, 1 space after marker → violation (ol_single=2)
        let rule_ol = MD030ListMarkerSpace::new(1, 1, 2, 2);
        let content_ol = "1.  Top-level multi\n    1. Nested wrong\n";
        let ctx_ol = LintContext::new(content_ol, crate::config::MarkdownFlavor::Standard, None);
        let result_ol = rule_ol.check(&ctx_ol).unwrap();
        assert_eq!(
            result_ol.len(),
            1,
            "Nested ordered item with 1 space (ol_single=2) should be flagged; got: {result_ol:?}"
        );
        let fixed_ol = rule_ol.fix(&ctx_ol).unwrap();
        assert_eq!(
            fixed_ol, "1.  Top-level multi\n    1.  Nested wrong\n",
            "fix() should expand 1 space to ol_single=2 on the nested ordered item"
        );

        // Deeply nested (8+ spaces) items are also checked.
        // Only the 8-space-indented item has wrong spacing; outer levels are correct.
        let content_deep = "-   Level 1\n    -   Level 2\n        - Level 3 wrong\n        -   Level 3 correct\n";
        let ctx_deep = LintContext::new(content_deep, crate::config::MarkdownFlavor::Standard, None);
        let result_deep = rule.check(&ctx_deep).unwrap();
        assert_eq!(
            result_deep.len(),
            1,
            "Deeply nested (8-space) item with 1 space should be flagged; got: {result_deep:?}"
        );
        assert_eq!(result_deep[0].line, 3, "Violation should be on the deeply nested line");

        // Verify the full roundtrip: fix() must resolve everything check() found.
        assert_fix_resolves_all_violations(&rule, content);
        assert_fix_resolves_all_violations(&rule_ol, content_ol);
        assert_fix_resolves_all_violations(&rule, content_deep);
    }

    #[test]
    fn test_loose_nested_item_fix_matches_check() {
        // A loose nested list item (blank line between parent and child) with 4-space
        // indentation must be both detected AND fixed. check() and fix() must agree.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1);

        let content = "- parent\n\n    -  nested wrong\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // check() must detect it
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "Loose nested item with 2 spaces should be detected; got: {warnings:?}"
        );

        // fix() must fix it
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "- parent\n\n    - nested wrong\n",
            "fix() should reduce 2 spaces to 1 for loose nested item"
        );

        // Verify the full roundtrip: fix() must resolve everything check() found.
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_multi_reindents_nested_to_stay_attached() {
        // Regression: widening a multi-line marker (here ol-multi = 3) moves its
        // content right. Without re-indenting the nested list it would end up left of
        // the parent's content column and detach (the nested `1.` would flatten into a
        // sibling). The shifts accumulate down the levels so everything stays nested.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 3); // ol-multi = 3
        let content = indoc! {"
            1. outer
               1. inner
                  deep
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.   outer
                     1.   inner
                          deep
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_multi_moves_content_when_narrowing() {
        // The mirror case: removing extra spaces (narrowing) moves the continuation
        // left with the content column. Left behind, content four columns past the
        // new column would turn into an indented code block.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1); // defaults: markers narrow to 1
        let content = indoc! {"
            1.   outer
                 continuation
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1. outer
                   continuation
            "},
            "marker narrows to 1 space and the continuation moves with it"
        );
    }

    #[test]
    fn test_ol_align_column_off_by_default() {
        // Without ol-align-column (the default), a list with uniform single spaces
        // is valid even when markers differ in width.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1);
        let content = indoc! {"
            1. one
            9. nine
            10. ten
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "Default behaviour should not require column alignment"
        );
    }

    #[test]
    fn test_ol_align_column_basic() {
        // Issue #644: aligning to column 4 keeps the text column fixed across a
        // digit boundary (9. -> 10.).
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. one
            9. nine
            10. ten
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Single-digit markers should be flagged; got: {warnings:?}"
        );
        assert!(warnings.iter().all(|w| w.line == 1 || w.line == 2));
        assert!(
            warnings[0].message.contains("Expected: 2") && warnings[0].message.contains("Actual: 1"),
            "Message should report the aligned target; got: {}",
            warnings[0].message
        );
        assert_eq!(
            warnings[0].column, 3,
            "Span should start at the whitespace after the marker"
        );

        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  one
                9.  nine
                10. ten
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_wide_marker_overflows() {
        // A marker too wide for the column overflows with a single space rather
        // than pushing the narrow entries further right.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. a
            100. b
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  a
                100. b
            "},
            "narrow marker sits at column 4; wide marker overflows to column 5"
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_max_is_four_spaces() {
        // Column 6 is the maximum the config allows: a `1.` marker reaches it with
        // exactly 4 spaces, the CommonMark ceiling (5+ would start an indented code
        // block). Larger columns are rejected at the config layer, not clamped here.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(6);
        let content = indoc! {"
            1. one
            2. two
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.    one
                2.    two
            "},
            "column 6 pads `1.` to exactly 4 spaces, never more"
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_already_aligned_is_clean() {
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1.  one
            9.  nine
            10. ten
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "Already-aligned list should produce no warnings"
        );
    }

    #[test]
    fn test_ol_align_column_reindents_nested_list() {
        // A nested unordered list shifts with the ordered marker, and both bullets
        // get ul-single, including the first one, which shares the `1.` line (the
        // parser exposes only the outer `1.` marker there). With ol-align-column = 4
        // and ul-single = 3 the whole structure lands on a 4-column grid.
        let rule = MD030ListMarkerSpace::new(3, 1, 1, 1).with_ol_align_column(4); // ul-single = 3
        let content = indoc! {"
            1. - x
               - y
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  -   x
                    -   y
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_inline_non_marker_left_alone() {
        // Only a real inline bullet (marker + space) gets ul-single; content that
        // merely starts with `-`/`*` (a word, emphasis) must be left untouched, while
        // the ordered marker still aligns to column 4.
        let rule = MD030ListMarkerSpace::new(3, 1, 1, 1).with_ol_align_column(4);
        for (input, expected) in [
            ("1. -text\n", "1.  -text\n"),
            ("1. *emphasis* here\n", "1.  *emphasis* here\n"),
        ] {
            let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "input: {input:?}");
        }
    }

    #[test]
    fn test_ol_align_column_reindents_multi_level() {
        // Shifts accumulate across nesting levels: the parent's widening and the
        // nested item's widening both move the deepest line, in a single pass.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. text
               1. a
                  z
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  text
                    1.  a
                        z
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_reindents_multiline_nested_unordered() {
        // A nested unordered list whose items span multiple lines: bullets align to
        // column 4 and get ul-multi (they're multi-line), and their continuation
        // lines shift to follow. The first bullet shares the `1.` line and is spaced
        // just like `- second` on its own line.
        let rule = MD030ListMarkerSpace::new(1, 3, 1, 1).with_ol_align_column(4); // ul-single=1, ul-multi=3
        let content = indoc! {"
            1. - first
                 more first
               - second
                 more second
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  -   first
                        more first
                    -   second
                        more second
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_reindents_multiline_nested_ordered() {
        // A multi-line ordered list nested in a multi-line ordered list: every
        // marker aligns to column 4 (relative to its own start), and continuation
        // lines shift to follow, accumulating across the two levels.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. text
               more text
               1. inner
                  more inner
               2. inner2
                  more inner2
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  text
                    more text
                    1.  inner
                        more inner
                    2.  inner2
                        more inner2
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_nested_aligns_relative() {
        // A nested ordered list shifts to follow the widened parent and aligns to
        // column 4 relative to its own markers (single-digit → 2 spaces, `10.` → 1).
        // (A nested ordered list is only recognized when it starts at 1.)
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. p
               1. a
               2. b
               9. i
               10. j
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  p
                    1.  a
                    2.  b
                    9.  i
                    10. j
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_blockquote_items_in_a_list() {
        // Several blockquote items in one list: each marker reaches column 4 and
        // its blockquote continuation shifts to follow.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. > a
               > b
            2. > c
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  > a
                    > b
                2.  > c
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_detached_blockquote_left_alone() {
        // The blockquote sits at column 3 while the item's content is at column 4, so
        // the parser already treats `> y` as its own top-level block, not this item's
        // content. It must be left untouched (no re-attaching). The attached case is
        // covered by `test_ol_align_column_blockquote_items_in_a_list`.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let detached = indoc! {"
            1.  > x
               > y
        "};
        let ctx = LintContext::new(detached, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            detached,
            "a detached top-level blockquote must be left as is"
        );
    }

    #[test]
    fn test_ol_align_column_preserves_blockquote_alignment() {
        // The motivating case: ordered items wrapping blockquotes whose content
        // already sits at column 4. Aligning keeps every outer marker at column 4
        // (rather than reducing the multi-line items 1 and 3), and the blockquote
        // structure is preserved.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1.  > 1.  x
                > 2.  y

            2.  > z

            3.  > 1.  a
                > 2.  b
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Already at column 4, so nothing to change. Crucially, the multi-line
        // items 1 and 3 are not reduced to column 3.
        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "items already at column 4 must not be flagged; got: {:?}",
            rule.check(&ctx).unwrap()
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            content,
            "fix must leave the aligned input untouched"
        );
    }

    #[test]
    fn test_ol_align_column_reindents_mixed_content() {
        // An item containing a blockquote and a nested list: every kind of attached
        // content shifts together to follow the widened marker.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            1. > x
               - sub
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                1.  > x
                    - sub
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_in_blockquote() {
        // Blockquoted ordered lists align correctly (the column is measured from
        // the marker, independent of the blockquote prefix).
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            > 1. one
            > 9. nine
            > 10. ten
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                > 1.  one
                > 9.  nine
                > 10. ten
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_multiline_item_in_blockquote() {
        // A multi-line ordered item *inside* a blockquote. Its continuation indent
        // lives after the `>` prefix, not at the start of the line, so the generic
        // shift moves that, keeping `more` under `text` and the blockquote intact,
        // and the marker aligns to column 4 just like an item outside a blockquote.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            > 1. text
            >    more
            > 2. second
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                > 1.  text
                >     more
                > 2.  second
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_nested_list_in_blockquote() {
        // A nested ordered list inside a blockquoted item: shifts accumulate across
        // both levels in the blockquote's own coordinate system, so the inner marker
        // lands under the outer text and the deepest line under the inner content.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            > 1. text
            >    1. inner
            >       more
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                > 1.  text
                >     1.  inner
                >         more
            "}
        );
        assert_fix_resolves_all_violations(&rule, content);
    }

    #[test]
    fn test_ol_align_column_does_not_affect_unordered_lists() {
        // ol-align-column only governs ordered lists; unordered lists are unchanged.
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1).with_ol_align_column(4);
        let content = indoc! {"
            - a
            - b
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "Unordered lists should be unaffected by ol-align-column"
        );
    }

    #[test]
    fn test_has_content_after_marker() {
        // Direct unit tests for the helper function
        assert!(!MD030ListMarkerSpace::has_content_after_marker("-", 1));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("- ", 1));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("-   ", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("- item", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("-  item", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("1. item", 2));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("1.", 2));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("1. ", 2));
    }
}
