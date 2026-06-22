use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::emphasis_style::EmphasisStyle;
use crate::utils::emphasis_utils::{find_emphasis_markers, find_single_emphasis_spans, replace_inline_code};
use crate::utils::skip_context::is_in_mkdocs_markup;

mod md049_config;
use md049_config::MD049Config;

/// Rule MD049: Emphasis style
///
/// See [docs/md049.md](../../docs/md049.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when the style for emphasis is inconsistent:
/// - Asterisks: `*text*`
/// - Underscores: `_text_`
///
/// This rule is focused on regular emphasis, not strong emphasis.
#[derive(Debug, Default, Clone)]
pub struct MD049EmphasisStyle {
    config: MD049Config,
}

impl MD049EmphasisStyle {
    /// Create a new instance of MD049EmphasisStyle
    pub fn new(style: EmphasisStyle) -> Self {
        MD049EmphasisStyle {
            config: MD049Config { style },
        }
    }

    pub fn from_config_struct(config: MD049Config) -> Self {
        Self { config }
    }

    /// Check if a byte position is within a link (inline links, reference links, or reference definitions).
    /// Delegates to LintContext::is_in_link which uses O(log n) binary search.
    fn is_in_link(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        ctx.is_in_link(byte_pos)
    }

    // Collect emphasis from a single line
    fn collect_emphasis_from_line(
        &self,
        line: &str,
        line_num: usize,
        line_start_pos: usize,
        emphasis_info: &mut Vec<(usize, usize, usize, char, String)>, // (line, col, abs_pos, marker, content)
    ) {
        // Replace inline code to avoid false positives. `replace_inline_code`
        // substitutes each inline-code span with an equal-length run of 'X', so
        // every byte offset (and thus every emphasis marker position) is
        // identical between `line` and `line_no_code`.
        let line_no_code = replace_inline_code(line);

        // Find all emphasis markers
        let markers = find_emphasis_markers(&line_no_code);
        if markers.is_empty() {
            return;
        }

        // Find single emphasis spans (not strong emphasis)
        let spans = find_single_emphasis_spans(&line_no_code, &markers);

        for span in spans {
            let marker_char = span.opening.as_char();
            let col = span.opening.start_pos + 1; // Convert to 1-based
            let abs_pos = line_start_pos + span.opening.start_pos;

            // Use the content from the *original* line, not the code-masked copy.
            // The span content from `line_no_code` would contain the 'X'
            // placeholders, which must never leak into the generated fix.
            // Marker positions are byte offsets that are valid in `line` because
            // masking preserves byte length and span boundaries.
            let content_start = span.opening.end_pos();
            let content_end = span.closing.start_pos;
            let original_content = line[content_start..content_end].to_string();

            emphasis_info.push((line_num, col, abs_pos, marker_char, original_content));
        }
    }
}

impl Rule for MD049EmphasisStyle {
    fn name(&self) -> &'static str {
        "MD049"
    }

    fn description(&self) -> &'static str {
        "Emphasis style should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Emphasis
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = vec![];

        // Early return if no emphasis markers
        if !ctx.likely_has_emphasis() {
            return Ok(warnings);
        }

        // Use LintContext to skip code blocks
        // Create LineIndex for correct byte position calculations across all line ending types
        let line_index = &ctx.line_index;

        // Collect all emphasis from the document
        let mut emphasis_info = vec![];

        // Process content lines, automatically skipping front matter, code blocks, HTML comments,
        // MDX constructs, math blocks, and Obsidian comments
        // Math blocks contain LaTeX syntax where _ and * have special meaning
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_comments()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .skip_math_blocks()
            .skip_obsidian_comments()
            .skip_mkdocstrings()
        {
            // Skip if the line doesn't contain any emphasis markers
            if !line.content.contains('*') && !line.content.contains('_') {
                continue;
            }

            // Get absolute position for this line
            let line_start = line_index.get_line_start_byte(line.line_num).unwrap_or(0);
            self.collect_emphasis_from_line(line.content, line.line_num, line_start, &mut emphasis_info);
        }

        // Filter out emphasis markers that are inside links or MkDocs markup
        let lines = ctx.raw_lines();
        // Math byte ranges, computed once for the whole document. The
        // line-level `skip_math_blocks` filter drops whole math-only lines,
        // but a line that mixes a display span with lintable prose (e.g.
        // `$$ _x_ $$ $$ _y_ $$`) stays lintable so trailing prose is checked;
        // this byte-level guard then exempts only the underscores that fall
        // inside the line-start `$$...$$` span, matching MD037/MD050.
        // `math_byte_ranges` has no code-block awareness, so a `$$` inside a
        // fenced code block would wrongly open a span that swallows real
        // prose up to the next `$$`. Neutralize `$` bytes inside code-block
        // ranges first (replacing only the ASCII `$` keeps every byte offset
        // and UTF-8 validity intact) so the byte model agrees with the
        // code-block-aware line-level math map.
        // Sort and merge so membership is a binary search rather than a
        // per-span linear scan: a math-heavy document (many `$x$` spans
        // alternating with emphasis) would otherwise be O(spans x ranges).
        // Ranges may overlap (e.g. a `$b$` inside a `$$...$$` block), so the
        // merge collapses them into disjoint, ascending intervals.
        let math_ranges: Vec<(usize, usize)> = {
            // A `$` inside fenced code or an inline code span is never a math
            // delimiter, but `math_byte_ranges` does not know that. Neutralize
            // those `$` first so the byte model agrees with the code-block-
            // aware line-level math map and inline code cannot synthesize a
            // span around real emphasis.
            let code_spans = ctx.code_spans();
            let math_source: std::borrow::Cow<'_, str> = if ctx.code_blocks.is_empty() && code_spans.is_empty() {
                std::borrow::Cow::Borrowed(ctx.content)
            } else {
                let mut bytes = ctx.content.as_bytes().to_vec();
                let len = bytes.len();
                let mut mask = |start: usize, end: usize| {
                    for b in &mut bytes[start.min(len)..end.min(len)] {
                        if *b == b'$' {
                            *b = b' ';
                        }
                    }
                };
                for &(start, end) in &ctx.code_blocks {
                    mask(start, end);
                }
                for span in code_spans.iter() {
                    mask(span.byte_offset, span.byte_end);
                }
                // Only ASCII `$` was replaced with ASCII space, so the
                // buffer is still valid UTF-8 and the same length.
                std::borrow::Cow::Owned(String::from_utf8(bytes).expect("ASCII-only substitution"))
            };
            let mut r = crate::utils::skip_context::math_byte_ranges(&math_source);
            r.sort_unstable_by_key(|&(start, _)| start);
            let mut merged: Vec<(usize, usize)> = Vec::with_capacity(r.len());
            for (start, end) in r {
                match merged.last_mut() {
                    Some(last) if start <= last.1 => last.1 = last.1.max(end),
                    _ => merged.push((start, end)),
                }
            }
            merged
        };
        emphasis_info.retain(|(line_num, col, abs_pos, _, _)| {
            // Skip emphasis inside math. `math_ranges` is disjoint and sorted
            // by start, so the only interval that can contain `abs_pos` is
            // the last one whose start is <= `abs_pos`.
            let idx = math_ranges.partition_point(|&(start, _)| start <= *abs_pos);
            if idx > 0 && *abs_pos < math_ranges[idx - 1].1 {
                return false;
            }
            // Skip emphasis inside Obsidian comments
            if ctx.is_in_obsidian_comment(*abs_pos) {
                return false;
            }
            // Skip if inside a link
            if Self::is_in_link(ctx, *abs_pos) {
                return false;
            }
            // Skip if inside MkDocs markup (Keys, Caret, Mark, icon shortcodes)
            if let Some(line) = lines.get(*line_num - 1) {
                let line_pos = col.saturating_sub(1); // Convert 1-indexed col to 0-indexed position
                if is_in_mkdocs_markup(line, line_pos, ctx.flavor) {
                    return false;
                }
            }
            true
        });

        match self.config.style {
            EmphasisStyle::Consistent => {
                // If we have less than 2 emphasis nodes, no need to check consistency
                if emphasis_info.len() < 2 {
                    return Ok(warnings);
                }

                // Count how many times each marker appears (prevalence-based approach)
                let asterisk_count = emphasis_info.iter().filter(|(_, _, _, m, _)| *m == '*').count();
                let underscore_count = emphasis_info.iter().filter(|(_, _, _, m, _)| *m == '_').count();

                // Use the most prevalent marker as the target style
                // In case of a tie, prefer asterisk (matches CommonMark recommendation)
                let target_marker = if asterisk_count >= underscore_count { '*' } else { '_' };

                // Check all emphasis nodes for consistency with the prevalent style
                for (line_num, _col, abs_pos, marker, content) in &emphasis_info {
                    if *marker != target_marker {
                        // Calculate emphasis length (marker + content + marker).
                        // The byte length drives the Fix range; the character length
                        // drives the displayed end column.
                        let emphasis_len = 1 + content.len() + 1;
                        let (_, char_col) = ctx.offset_to_line_col(*abs_pos);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: *line_num,
                            column: char_col,
                            end_line: *line_num,
                            end_column: char_col + content.chars().count() + 2,
                            message: format!("Emphasis should use {target_marker} instead of {marker}"),
                            fix: Some(Fix::new(
                                *abs_pos..*abs_pos + emphasis_len,
                                format!("{target_marker}{content}{target_marker}"),
                            )),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            EmphasisStyle::Asterisk | EmphasisStyle::Underscore => {
                let (wrong_marker, correct_marker) = match self.config.style {
                    EmphasisStyle::Asterisk => ('_', '*'),
                    EmphasisStyle::Underscore => ('*', '_'),
                    EmphasisStyle::Consistent => {
                        // This case is handled separately above
                        // but fallback to asterisk style for safety
                        ('_', '*')
                    }
                };

                for (line_num, _col, abs_pos, marker, content) in &emphasis_info {
                    if *marker == wrong_marker {
                        // Calculate emphasis length (marker + content + marker).
                        // The byte length drives the Fix range; the character length
                        // drives the displayed end column.
                        let emphasis_len = 1 + content.len() + 1;
                        let (_, char_col) = ctx.offset_to_line_col(*abs_pos);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: *line_num,
                            column: char_col,
                            end_line: *line_num,
                            end_column: char_col + content.chars().count() + 2,
                            message: format!("Emphasis should use {correct_marker} instead of {wrong_marker}"),
                            fix: Some(Fix::new(
                                *abs_pos..*abs_pos + emphasis_len,
                                format!("{correct_marker}{content}{correct_marker}"),
                            )),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_emphasis()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD049Config);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let rule = MD049EmphasisStyle::default();
        assert_eq!(rule.name(), "MD049");
    }

    #[test]
    fn test_style_from_str() {
        assert_eq!(EmphasisStyle::from("asterisk"), EmphasisStyle::Asterisk);
        assert_eq!(EmphasisStyle::from("underscore"), EmphasisStyle::Underscore);
        assert_eq!(EmphasisStyle::from("other"), EmphasisStyle::Consistent);
    }

    #[test]
    fn test_emphasis_in_links_not_flagged() {
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = r#"Check this [*asterisk*](https://example.com/*pattern*) link and [_underscore_](https://example.com/_private_).

Also see the [`__init__`][__init__] reference.

This should be _flagged_ since we're using asterisk style.

[__init__]: https://example.com/__init__.py"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the real emphasis outside links should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Emphasis should use * instead of _"));
        // Should flag "_flagged_" but not emphasis patterns inside links
        assert!(result[0].line == 5); // Line with "_flagged_"
    }

    #[test]
    fn test_emphasis_in_links_vs_outside_links() {
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
        let content = r#"Check [*emphasis*](https://example.com/*test*) and inline *real emphasis* text.

[*link*]: https://example.com/*path*"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the actual emphasis outside links should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Emphasis should use _ instead of *"));
        // Should be the "real emphasis" text on line 1
        assert!(result[0].line == 1);
    }

    #[test]
    fn test_mkdocs_keys_notation_not_flagged() {
        // Keys notation uses ++ which shouldn't be confused with emphasis
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "Press ++ctrl+alt+del++ to restart.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Keys notation should not be flagged as emphasis
        assert!(
            result.is_empty(),
            "Keys notation should not be flagged as emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_caret_notation_not_flagged() {
        // Caret notation (^superscript^ and ^^insert^^) should not be flagged
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "This is ^superscript^ and ^^inserted^^ text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Caret notation should not be flagged as emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_mark_notation_not_flagged() {
        // Mark notation (==highlight==) should not be flagged
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "This is ==highlighted== text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Mark notation should not be flagged as emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_mixed_content_with_real_emphasis() {
        // Mixed content: MkDocs markup + real emphasis that should be flagged
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "Press ++ctrl++ and _underscore emphasis_ here.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Only the real underscore emphasis should be flagged (not Keys notation)
        assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
        assert!(result[0].message.contains("Emphasis should use * instead of _"));
    }

    #[test]
    fn test_mkdocs_icon_shortcode_not_flagged() {
        // Icon shortcodes like :material-star: should not affect emphasis detection
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "Click :material-check: and _this should be flagged_.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // The underscore emphasis should still be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Emphasis should use * instead of _"));
    }

    #[test]
    fn test_mkdocstrings_block_not_flagged() {
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "# Example\n\n::: my_module.MyClass\n    options:\n      members:\n        - _private_method\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "_private_method_ inside mkdocstrings block should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocstrings_block_with_emphasis_outside() {
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "::: my_module.MyClass\n    options:\n      members:\n        - _init\n\nThis _should be flagged_ outside.\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Only emphasis outside mkdocstrings should be flagged. Got: {result:?}"
        );
        assert_eq!(result[0].line, 6);
    }

    #[test]
    fn test_inline_code_inside_emphasis_preserved_on_fix() {
        // Regression test: inline code inside an emphasis span must survive the
        // style conversion. Previously the 'X' masking placeholder leaked into
        // the fix output, destroying the code span.
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "- _An item with `inline code` inside._ Trailing text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "- *An item with `inline code` inside.* Trailing text.");
        assert!(!fixed.contains('X'), "masking placeholder leaked into fix: {fixed}");

        // Idempotent: fixing the already-fixed content changes nothing.
        let ctx2 = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx2).unwrap(), fixed);
    }

    #[test]
    fn test_inline_code_inside_emphasis_underscore_style() {
        // Same bug in the opposite direction (* -> _).
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
        let content = "See *the `id` field* below.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "See _the `id` field_ below.");
    }

    #[test]
    fn test_obsidian_inline_comment_emphasis_ignored() {
        // Emphasis inside Obsidian comments should be ignored
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        let content = "Visible %%_hidden_%% text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should ignore emphasis inside Obsidian comments. Got: {result:?}"
        );
    }
}
