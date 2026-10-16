/// Rule MD069: No duplicate list markers
///
/// See [docs/md069.md](../../docs/md069.md) for full documentation, configuration, and examples.
///
/// This rule detects duplicate list markers that typically occur from copy-paste
/// with editor auto-list-continuation. For example:
///
/// ```markdown
/// - - duplicate marker (accidental)
/// ```
///
/// Should be:
///
/// ```markdown
/// - duplicate marker
/// ```
///
/// CommonMark parses `- - text` as a valid nested list, but this pattern is
/// never intentionally used in practice. Intentional nested lists use indentation:
///
/// ```markdown
/// - Parent
///   - Child
/// ```
use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::lint_context::types::is_horizontal_rule_content;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;

#[derive(Clone, Default)]
pub struct MD069NoDuplicateListMarkers;

/// A run of two or more list markers at the start of a line.
struct MarkerRun {
    /// Byte offset of the first marker.
    start: usize,
    /// Byte offset of the first character after the run and its whitespace.
    rest_start: usize,
    /// The first marker character.
    first: char,
    /// The second marker character.
    second: char,
}

fn is_marker(b: u8) -> bool {
    matches!(b, b'*' | b'+' | b'-')
}

/// Parse the marker run at the start of `line`, or `None` when the line holds
/// no duplicate that is safe to talk about.
///
/// Each marker is a bullet character followed by a space or tab. The run stops
/// before a marker whose tail is a thematic break (that marker starts a break,
/// not an item), and the line is not a duplicate at all when its first marker
/// already starts a thematic break. A remainder that is empty is not reported
/// because collapsing the run would leave an empty item, which cannot interrupt
/// a paragraph. A remainder that is an indented code block of the innermost item
/// is on a line the caller skips as code.
fn parse_marker_run(line: &str) -> Option<MarkerRun> {
    let bytes = line.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    let start = pos;
    let mut markers: Vec<char> = Vec::new();
    loop {
        if pos >= bytes.len() || !is_marker(bytes[pos]) || !matches!(bytes.get(pos + 1), Some(b' ' | b'\t')) {
            break;
        }
        if is_horizontal_rule_content(line[pos..].trim_matches([' ', '\t'])) {
            if markers.is_empty() {
                return None;
            }
            break;
        }
        markers.push(bytes[pos] as char);
        pos += 1;
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        if pos >= bytes.len() {
            // Nothing follows the last marker.
            return None;
        }
    }
    if markers.len() < 2 {
        return None;
    }
    Some(MarkerRun {
        start,
        rest_start: pos,
        first: markers[0],
        second: markers[1],
    })
}

impl MD069NoDuplicateListMarkers {
    pub fn new() -> Self {
        Self
    }

    /// Whether collapsing the run on `line_num` leaves every later line meaning
    /// what it meant before.
    ///
    /// The collapse moves the content column of the item to `T = indent + 2`.
    /// A later non-blank line at or above `T` can change which item it belongs
    /// to, turn into indented code, or change what a setext underline closes,
    /// so the fix is withheld. A line below `T` that follows a blank line, or
    /// that starts a block which closes the item, ends the scan. Any other line
    /// below `T` may be lazy paragraph continuation, which does not close the
    /// item, so the scan continues past it.
    fn collapse_is_safe(ctx: &LintContext, line_num: usize) -> bool {
        let threshold = ctx.lines[line_num - 1].visual_indent + 2;
        let mut previous_blank = false;
        for info in &ctx.lines[line_num..] {
            if info.is_blank {
                previous_blank = true;
                continue;
            }
            if info.visual_indent >= threshold {
                return false;
            }
            let closes_item = previous_blank
                || info.list_item.is_some()
                || info.heading.is_some()
                || info.is_horizontal_rule
                || info.blockquote.is_some()
                || info.in_code_block
                || info.in_html_block;
            if closes_item {
                return true;
            }
            previous_blank = false;
        }
        true
    }
}

impl Rule for MD069NoDuplicateListMarkers {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(69))
    }

    fn name(&self) -> &'static str {
        "MD069"
    }

    fn description(&self) -> &'static str {
        "Duplicate list markers"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for filtered_line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .skip_html_comments()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .skip_obsidian_comments()
        {
            let line_num = filtered_line.line_num;
            let line = filtered_line.content;

            let Some(run) = parse_marker_run(line) else {
                continue;
            };

            // The parser must agree that a list item starts here; otherwise the
            // line is paragraph text or part of another block.
            if ctx.list_item_on_line(line_num).is_none() {
                continue;
            }

            let rest = &line[run.rest_start..];
            let collapsed_is_break =
                is_horizontal_rule_content(format!("{} {rest}", run.first).trim_matches([' ', '\t']));
            let fix = if collapsed_is_break || !Self::collapse_is_safe(ctx, line_num) {
                None
            } else {
                let line_start = ctx.line_start_byte(line_num).unwrap_or(0);
                Some(Fix::new(
                    line_start + run.start..line_start + run.rest_start,
                    format!("{} ", run.first),
                ))
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!(
                    "Duplicate list marker '{} {}' - likely copy-paste error",
                    run.first, run.second
                ),
                line: line_num,
                column: byte_to_char_count(line, run.start),
                end_line: line_num,
                end_column: byte_to_char_count(line, run.rest_start),
                severity: Severity::Warning,
                fix,
            });
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast check: if no list markers exist at all, skip
        !ctx.has_char('-') && !ctx.has_char('*') && !ctx.has_char('+')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn check(content: &str) -> Vec<LintWarning> {
        let rule = MD069NoDuplicateListMarkers::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    fn fix(content: &str) -> String {
        let rule = MD069NoDuplicateListMarkers::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.fix(&ctx).unwrap()
    }

    // === Detection tests ===

    #[test]
    fn test_duplicate_dash_markers() {
        let warnings = check("- - duplicate text");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Duplicate list marker"));
    }

    #[test]
    fn test_duplicate_asterisk_markers() {
        let warnings = check("* * duplicate text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_duplicate_plus_markers() {
        let warnings = check("+ + duplicate text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_mixed_markers() {
        let warnings = check("- * mixed markers");
        assert_eq!(warnings.len(), 1);

        let warnings = check("* - mixed markers");
        assert_eq!(warnings.len(), 1);

        let warnings = check("+ - mixed markers");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_indented_duplicate() {
        let warnings = check("  - - indented duplicate");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 3); // After 2 spaces of indent
    }

    #[test]
    fn test_multiple_duplicates_in_document() {
        let content = "- - first\n- - second\n- normal";
        let warnings = check(content);
        assert_eq!(warnings.len(), 2);
    }

    // === False positive prevention ===

    #[test]
    fn test_cli_flag_no_false_positive() {
        // - --flag should NOT be flagged (second - not followed by space)
        let warnings = check("- --verbose flag");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_short_flag_no_false_positive() {
        // - -n should NOT be flagged (second - not followed by space)
        let warnings = check("- -n option");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_emphasis_no_false_positive() {
        // * *emphasis* should NOT be flagged (second * not followed by space)
        let warnings = check("* *emphasis* text");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_bold_no_false_positive() {
        let warnings = check("- **bold** text");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normal_list_no_false_positive() {
        let warnings = check("- normal list item");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_indented_nested_list_no_false_positive() {
        // Valid nested list structure
        let content = "- Parent\n  - Child";
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    // === Context skipping ===

    #[test]
    fn test_skip_code_block() {
        let content = "```\n- - in code block\n```";
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_skip_indented_code_block() {
        let content = "    - - indented code";
        let warnings = check(content);
        // Indented code blocks are treated as code, not lists
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_skip_frontmatter() {
        let content = "---\n- - in frontmatter\n---\n- - actual duplicate";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4); // Only the one outside frontmatter
    }

    #[test]
    fn test_skip_html_comment() {
        let content = "<!-- - - in comment -->\n- - duplicate";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
    }

    // === Fix tests ===

    #[test]
    fn test_fix_duplicate_dash() {
        let fixed = fix("- - duplicate text");
        assert_eq!(fixed, "- duplicate text");
    }

    #[test]
    fn test_fix_duplicate_asterisk() {
        let fixed = fix("* * duplicate text");
        assert_eq!(fixed, "* duplicate text");
    }

    #[test]
    fn test_fix_mixed_markers() {
        // The outer marker stays, so the item stays in the list it is already in
        let fixed = fix("- * mixed");
        assert_eq!(fixed, "- mixed");
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let fixed = fix("  - - indented");
        assert_eq!(fixed, "  - indented");
    }

    #[test]
    fn test_fix_multiple_duplicates() {
        let content = "- - first\n- - second";
        let fixed = fix(content);
        assert_eq!(fixed, "- first\n- second");
    }

    #[test]
    fn test_fix_preserves_normal_lines() {
        let content = "# Heading\n- - duplicate\n- normal\nParagraph";
        let fixed = fix(content);
        assert_eq!(fixed, "# Heading\n- duplicate\n- normal\nParagraph");
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let fixed = fix("- - duplicate\n");
        assert_eq!(fixed, "- duplicate\n");
    }

    #[test]
    fn test_fix_no_trailing_newline() {
        let fixed = fix("- - duplicate");
        assert_eq!(fixed, "- duplicate");
    }

    // === Structure preservation ===

    fn unchanged(content: &str) {
        assert!(
            check(content).iter().all(|w| w.fix.is_none()),
            "fix offered for {content:?}"
        );
        assert_eq!(fix(content), content, "document changed: {content:?}");
    }

    fn not_reported(content: &str) {
        assert!(check(content).is_empty(), "reported: {content:?}");
        assert_eq!(fix(content), content);
    }

    #[test]
    fn test_thematic_break_shapes_not_reported() {
        for c in ["* * *\n", "- - -\n", "_ _ _\n", "- - - -\n", "-  -  -\n"] {
            not_reported(c);
        }
        // "+ + +" is not a thematic break, so it is a duplicate
        assert_eq!(fix("+ + +\n"), "+ +\n");
    }

    #[test]
    fn test_thematic_break_inside_list_item_not_reported() {
        // 4 columns of indent, but only 1 relative to the item content
        not_reported("1.\n    - - -\n");
        not_reported("- a\n  - - -\n");
        // an item that holds a thematic break
        not_reported("- Foo\n- * * *\n");
        not_reported("- Foo\n- * * *\n- Bar\n");
    }

    #[test]
    fn test_marker_run_before_thematic_break_collapses_to_item_with_break() {
        assert_eq!(fix("- Foo\n- - * * *\n"), "- Foo\n- * * *\n");
    }

    #[test]
    fn test_unicode_whitespace_does_not_hide_a_duplicate() {
        // U+00A0 is text, not whitespace that a thematic break may contain
        let content = "- * * *\u{a0}\n";
        assert_eq!(check(content).len(), 1);
        assert_eq!(fix(content), "- *\u{a0}\n");
    }

    #[test]
    fn test_crlf_thematic_break_not_reported() {
        not_reported("- - -\r\n");
        not_reported("1.\r\n    - - -\r\n");
    }

    #[test]
    fn test_paragraph_continuation_not_reported() {
        not_reported("a\n    - - b\n");
    }

    #[test]
    fn test_empty_remainder_not_reported() {
        not_reported("- - \n");
        not_reported("+ + + \n");
    }

    #[test]
    fn test_code_remainder_not_reported() {
        not_reported("- -     x\n");
        not_reported("-     - x\n");
        // a tab can also reach the fifth column
        not_reported("- -\t\tx\n");
        // four columns or fewer is still a paragraph
        assert_eq!(fix("- -\t x\n"), "- x\n");
    }

    #[test]
    fn test_fix_keeps_first_marker() {
        assert_eq!(fix("- * mixed"), "- mixed");
        assert_eq!(fix("* - mixed"), "* mixed");
        assert_eq!(fix("+ - mixed"), "+ mixed");
        // The list stays one list
        assert_eq!(fix("- a\n- * b\n- c\n"), "- a\n- b\n- c\n");
    }

    #[test]
    fn test_fix_collapses_whole_run_in_one_pass() {
        assert_eq!(fix("- - - x\n"), "- x\n");
        assert_eq!(fix("- * + x\n"), "- x\n");
        let once = fix("- - - x\n");
        assert_eq!(fix(&once), once);
    }

    #[test]
    fn test_fix_tabs_between_markers() {
        assert_eq!(fix("-\t-\tx\n"), "- x\n");
    }

    #[test]
    fn test_fix_preserves_crlf() {
        assert_eq!(fix("- - a\r\n- b\r\n"), "- a\r\n- b\r\n");
    }

    #[test]
    fn test_fix_touches_only_the_marker_run() {
        let w = check("  - - a\n");
        let fix = w[0].fix.as_ref().unwrap();
        assert_eq!(fix.range, 2..6);
        assert_eq!(fix.replacement, "- ");
    }

    #[test]
    fn test_collapse_that_is_a_thematic_break_is_reported_without_fix() {
        let content = "- * - - -\n";
        let w = check(content);
        assert_eq!(w.len(), 1);
        assert!(w[0].fix.is_none());
        assert_eq!(fix(content), content);
    }

    #[test]
    fn test_dependent_lines_withhold_the_fix() {
        for c in [
            "- - a\n\n      b\n",
            "- - a\n    - b\n",
            "- - a\n    ---\n",
            "- - a\n  b\n",
            "- - a\nb\n\n      c\n",
            "- - a\nb\n  - c\n",
        ] {
            let w = check(c);
            assert_eq!(w.len(), 1, "warning kept for {c:?}");
            unchanged(c);
        }
    }

    #[test]
    fn test_independent_following_lines_allow_the_fix() {
        assert_eq!(fix("- - a\nb\n"), "- a\nb\n");
        assert_eq!(fix("- - a\n\nb\n"), "- a\n\nb\n");
        assert_eq!(fix("- - a\n- b\n"), "- a\n- b\n");
        assert_eq!(fix("- - a\n# h\n\n      c\n"), "- a\n# h\n\n      c\n");
        assert_eq!(fix("- - a\n\n> q\n"), "- a\n\n> q\n");
    }

    #[test]
    fn test_nested_and_ordered_containers() {
        assert_eq!(fix("1. - a\n"), "1. - a\n");
        assert_eq!(fix("- p\n  - - a\n"), "- p\n  - a\n");
    }

    #[test]
    fn test_blockquote_lines_not_reported() {
        not_reported("> - - a\n");
    }

    // === Edge cases ===

    #[test]
    fn test_multiple_spaces_between_markers() {
        // - -   text (multiple spaces after first marker)
        let warnings = check("-   - text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_tab_indentation() {
        // Tab at start of line (4 spaces equivalent) creates an indented code block
        // So this should NOT be flagged - it's treated as code, not a list
        let warnings = check("\t- - duplicate");
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_empty_content_after_markers() {
        // An empty inner item: collapsing it would leave an empty item, which cannot
        // interrupt a paragraph, so it is not reported (like "- -")
        assert!(check("- - ").is_empty());
        assert!(check("o\n- - \n").is_empty());
    }

    #[test]
    fn test_three_markers() {
        // - - - text could be either:
        // 1. HR (---)
        // 2. Triple nested list
        // Our regex will match the first two, leaving "- text"
        let content = "- - - text";
        let warnings = check(content);
        // Should match - - and leave "- text"
        assert_eq!(warnings.len(), 1);
    }

    // === Horizontal rule tests ===

    #[test]
    fn test_horizontal_rule_dash_no_false_positive() {
        // - - - is a valid horizontal rule (thematic break), not duplicate markers
        let warnings = check("- - -");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_horizontal_rule_asterisk_no_false_positive() {
        // * * * is a valid horizontal rule
        let warnings = check("* * *");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_horizontal_rule_many_dashes_no_false_positive() {
        // - - - - - is still a valid horizontal rule
        let warnings = check("- - - - -");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_horizontal_rule_with_spaces_no_false_positive() {
        // Horizontal rules with extra spaces are still valid
        let warnings = check("-  -  -");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_fix_preserves_horizontal_rules() {
        // Horizontal rules should not be "fixed"
        let content = "- - -\n* * *\n- - duplicate";
        let fixed = fix(content);
        assert_eq!(fixed, "- - -\n* * *\n- duplicate");
    }

    // === Inline config tests ===

    #[test]
    fn test_fix_respects_disable_line_comment() {
        // rumdl-disable-line should prevent fix on that line
        let content = "- - Item <!-- rumdl-disable-line MD069 -->";
        let fixed = fix(content);
        assert_eq!(fixed, content); // No change - rule disabled
    }

    #[test]
    fn test_fix_respects_markdownlint_disable_line_comment() {
        // markdownlint-disable-line should also work
        let content = "- - Item <!-- markdownlint-disable-line MD069 -->";
        let fixed = fix(content);
        assert_eq!(fixed, content); // No change - rule disabled
    }

    #[test]
    fn test_fix_respects_disable_next_line_comment() {
        // rumdl-disable-next-line should prevent fix on the next line
        let content = "<!-- rumdl-disable-next-line MD069 -->\n- - Item";
        let fixed = fix(content);
        assert_eq!(fixed, content); // No change - rule disabled for next line
    }

    #[test]
    fn test_fix_respects_disable_block() {
        // rumdl-disable block should prevent fix for all lines in range
        let content = "<!-- rumdl-disable MD069 -->\n- - Item1\n- - Item2\n<!-- rumdl-enable MD069 -->\n- - Item3";
        let fixed = fix(content);
        // Only Item3 should be fixed
        assert_eq!(
            fixed,
            "<!-- rumdl-disable MD069 -->\n- - Item1\n- - Item2\n<!-- rumdl-enable MD069 -->\n- Item3"
        );
    }

    #[test]
    fn test_check_respects_disable_line_comment() {
        // Verify check() also respects inline disable (it uses filtered_lines which handles this)
        let content = "- - Item <!-- rumdl-disable-line MD069 -->";
        let warnings = check(content);
        // The check() still reports the warning; inline config filtering happens in lib.rs
        // But we're testing fix() doesn't modify the line
        // Note: check() itself doesn't filter - that's done at a higher level in lib.rs
        // What we care about is that fix() respects the inline config
        assert_eq!(warnings.len(), 1); // check() still sees it
    }

    // === Roundtrip safety tests ===

    fn assert_fix_roundtrip(content: &str) {
        let rule = MD069NoDuplicateListMarkers::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx2).unwrap();
        assert!(
            remaining.is_empty(),
            "After fix(), check() should find 0 violations.\nOriginal: {content:?}\nFixed: {fixed:?}\nRemaining: {remaining:?}"
        );
    }

    #[test]
    fn test_roundtrip_basic_duplicate() {
        assert_fix_roundtrip("- - duplicate text");
    }

    #[test]
    fn test_roundtrip_multiple_duplicates() {
        assert_fix_roundtrip("- - first\n- - second\n- normal");
    }

    #[test]
    fn test_roundtrip_mixed_markers() {
        assert_fix_roundtrip("- * mixed\n* - other\n+ + plus");
    }

    #[test]
    fn test_roundtrip_indented() {
        assert_fix_roundtrip("  - - indented\n  * * more");
    }

    #[test]
    fn test_roundtrip_with_trailing_newline() {
        assert_fix_roundtrip("- - duplicate\n");
    }

    #[test]
    fn test_roundtrip_no_trailing_newline() {
        assert_fix_roundtrip("- - duplicate");
    }

    #[test]
    fn test_roundtrip_with_horizontal_rules() {
        assert_fix_roundtrip("- - -\n* * *\n- - duplicate");
    }

    #[test]
    fn test_roundtrip_with_code_blocks() {
        assert_fix_roundtrip("```\n- - in code\n```\n- - outside");
    }

    #[test]
    fn test_roundtrip_clean_content() {
        assert_fix_roundtrip("- normal list\n- another item");
    }
}
