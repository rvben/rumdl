use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::lint_context::types::HeadingStyle;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_trailing_range;
use crate::utils::regex_cache::{ORDERED_LIST_MARKER_REGEX, UNORDERED_LIST_MARKER_REGEX};

mod md009_config;
use md009_config::MD009Config;

/// Whether the line at `line_num` (0-indexed) is a setext heading underline (`===` or `---`).
///
/// rumdl marks `LineInfo::heading` only on the setext text line, not the underline; the
/// underline still parses as `is_paragraph_context` per its own flags. Detect it by looking
/// back to the previous line's heading style.
fn is_setext_underline(ctx: &LintContext, line_num: usize) -> bool {
    if line_num == 0 {
        return false;
    }
    ctx.line_info(line_num).is_some_and(|prev| {
        prev.heading
            .as_ref()
            .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
    })
}

/// Whether a `<br>` produced by trailing spaces on the line at `line_num` (0-indexed)
/// would be meaningful — i.e. the line is paragraph-context AND the next line continues
/// the same paragraph.
///
/// Mirrors markdownlint's MD009 strict logic, which only allows the `br_spaces` exception
/// on lines covered by `[paragraph.startLine, paragraph.endLine - 1]`. The last line of a
/// paragraph (single-line paragraph, line before a blank, line before a heading, separated
/// list items, etc.) gets no useful break and is flagged.
fn br_produces_useful_break(ctx: &LintContext, line_num: usize) -> bool {
    let lines = ctx.raw_lines();
    let Some(current) = ctx.line_info(line_num + 1) else {
        return false;
    };
    if !current.is_paragraph_context() || is_setext_underline(ctx, line_num) {
        return false;
    }
    let next_idx = line_num + 1;
    if next_idx >= lines.len() {
        return false;
    }
    let Some(next) = ctx.line_info(next_idx + 1) else {
        return false;
    };
    if next.is_blank || !next.is_paragraph_context() || next.list_item.is_some() || is_setext_underline(ctx, next_idx) {
        return false;
    }
    true
}

#[derive(Debug, Clone, Default)]
pub struct MD009TrailingSpaces {
    config: MD009Config,
}

impl MD009TrailingSpaces {
    pub fn new(br_spaces: usize, strict: bool) -> Self {
        Self {
            config: MD009Config {
                br_spaces: crate::types::BrSpaces::from_const(br_spaces),
                strict,
                list_item_empty_lines: false,
            },
        }
    }

    pub const fn from_config_struct(config: MD009Config) -> Self {
        Self { config }
    }

    fn count_trailing_spaces(line: &str) -> usize {
        line.chars().rev().take_while(|&c| c == ' ').count()
    }

    fn count_trailing_spaces_ascii(line: &str) -> usize {
        line.as_bytes().iter().rev().take_while(|&&b| b == b' ').count()
    }

    /// Count all trailing whitespace characters (ASCII and Unicode).
    /// This includes U+2000..U+200A (various Unicode spaces), ASCII space, tab, etc.
    fn count_trailing_whitespace(line: &str) -> usize {
        line.chars().rev().take_while(|c| c.is_whitespace()).count()
    }

    fn trimmed_len_ascii_whitespace(line: &str) -> usize {
        line.as_bytes()
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map_or(0, |idx| idx + 1)
    }

    fn calculate_trailing_range_ascii(
        line: usize,
        line_len: usize,
        content_end: usize,
    ) -> (usize, usize, usize, usize) {
        // Return 1-indexed columns to match calculate_trailing_range behavior
        (line, content_end + 1, line, line_len + 1)
    }

    fn is_empty_list_item_line(line: &str, prev_line: Option<&str>) -> bool {
        // A line is an empty list item line if:
        // 1. It's blank or only contains spaces
        // 2. The previous line is a list item
        if !line.trim().is_empty() {
            return false;
        }

        if let Some(prev) = prev_line {
            // Check for unordered list markers (*, -, +) with proper formatting
            UNORDERED_LIST_MARKER_REGEX.is_match(prev) || ORDERED_LIST_MARKER_REGEX.is_match(prev)
        } else {
            false
        }
    }
}

impl Rule for MD009TrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces should be removed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        // Use pre-computed lines (needed for looking back at prev_line)
        let lines = ctx.raw_lines();

        let mut filtered = ctx.filtered_lines().skip_front_matter().skip_pymdown_blocks();
        if !self.config.strict {
            filtered = filtered.skip_code_blocks();
        }

        for filtered_line in filtered {
            let line_num = filtered_line.line_num - 1;
            let line = filtered_line.content;

            let line_is_ascii = line.is_ascii();
            // Count ASCII trailing spaces for br_spaces comparison
            let trailing_ascii_spaces = if line_is_ascii {
                Self::count_trailing_spaces_ascii(line)
            } else {
                Self::count_trailing_spaces(line)
            };
            // For non-ASCII lines, also count all trailing whitespace (including Unicode)
            // to ensure the fix range covers everything that trim_end() removes
            let trailing_all_whitespace = if line_is_ascii {
                trailing_ascii_spaces
            } else {
                Self::count_trailing_whitespace(line)
            };

            // Skip if no trailing whitespace
            if trailing_all_whitespace == 0 {
                continue;
            }

            // Handle empty lines
            let trimmed_len = if line_is_ascii {
                Self::trimmed_len_ascii_whitespace(line)
            } else {
                line.trim_end().len()
            };
            if trimmed_len == 0 {
                if trailing_all_whitespace > 0 {
                    // Check if this is an empty list item line and config allows it
                    let prev_line = if line_num > 0 { Some(lines[line_num - 1]) } else { None };
                    if self.config.list_item_empty_lines && Self::is_empty_list_item_line(line, prev_line) {
                        continue;
                    }

                    // Calculate precise character range for all trailing whitespace on empty line
                    let (start_line, start_col, end_line, end_col) = if line_is_ascii {
                        Self::calculate_trailing_range_ascii(line_num + 1, line.len(), 0)
                    } else {
                        calculate_trailing_range(line_num + 1, line, 0)
                    };
                    let line_start = *ctx.line_offsets.get(line_num).unwrap_or(&0);
                    let fix_range = if line_is_ascii {
                        line_start..line_start + line.len()
                    } else {
                        line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.chars().count())
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Empty line has trailing spaces".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(fix_range, String::new())),
                    });
                }
                continue;
            }

            // Check if it's a valid line break (only ASCII spaces count for br_spaces).
            // The br_spaces exception applies when the trailing whitespace produces a `<br>`.
            // In non-strict mode we accept br_spaces anywhere that isn't the literal final
            // line of the document. In strict mode (markdownlint parity) we additionally
            // require the line to be a non-last line of a paragraph: structural lines
            // (headings, fences, setext underlines, horizontal rules, ...) and last lines
            // of paragraphs (single-line paragraphs, lines before blanks, list-item ends,
            // ...) all get flagged because the `<br>` they would emit is unobservable.
            let is_truly_last_line = line_num == lines.len() - 1 && !content.ends_with('\n');
            let has_only_ascii_trailing = trailing_ascii_spaces == trailing_all_whitespace;
            let matches_br_spaces = trailing_ascii_spaces == self.config.br_spaces.get();
            if !is_truly_last_line && has_only_ascii_trailing && matches_br_spaces {
                let allow = if self.config.strict {
                    br_produces_useful_break(ctx, line_num)
                } else {
                    true
                };
                if allow {
                    continue;
                }
            }

            // Check if this is an empty blockquote line ("> " or ">> " etc)
            // These are allowed by MD028 to have a single trailing ASCII space
            let trimmed = if line_is_ascii {
                &line[..trimmed_len]
            } else {
                line.trim_end()
            };
            let is_empty_blockquote_with_space = trimmed.chars().all(|c| c == '>' || c == ' ' || c == '\t')
                && trimmed.contains('>')
                && has_only_ascii_trailing
                && trailing_ascii_spaces == 1;

            if is_empty_blockquote_with_space {
                continue; // Allow single trailing ASCII space for empty blockquote lines
            }
            // Calculate precise character range for all trailing whitespace
            let (start_line, start_col, end_line, end_col) = if line_is_ascii {
                Self::calculate_trailing_range_ascii(line_num + 1, line.len(), trimmed.len())
            } else {
                calculate_trailing_range(line_num + 1, line, trimmed.len())
            };
            let line_start = *ctx.line_offsets.get(line_num).unwrap_or(&0);
            let fix_range = if line_is_ascii {
                let start = line_start + trimmed.len();
                let end = start + trailing_all_whitespace;
                start..end
            } else {
                line_index.line_col_to_byte_range_with_length(
                    line_num + 1,
                    trimmed.chars().count() + 1,
                    trailing_all_whitespace,
                )
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: if trailing_all_whitespace == 1 {
                    "Trailing space found".to_string()
                } else {
                    format!("{trailing_all_whitespace} trailing spaces found")
                },
                severity: Severity::Warning,
                fix: Some(Fix::new(fix_range, String::new())),
            });
        }

        Ok(warnings)
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty.
        // We cannot skip based on ASCII-space-only check because Unicode whitespace
        // characters (e.g., U+2000 EN QUAD) also count as trailing whitespace.
        // The per-line is_ascii fast path in check()/fix() handles performance.
        ctx.content.is_empty()
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    crate::impl_rule_config_methods!(MD009Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_no_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "This is a line\nAnother line\nNo trailing spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_basic_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with spaces   \nAnother line  \nClean line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Default br_spaces=2, so line with 2 spaces is OK
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "3 trailing spaces found");
    }

    #[test]
    fn test_md009_front_matter() {
        let rule = MD009TrailingSpaces::default();
        let content = "---\ntitle: Test   \n---\nBody   ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should only flag the one in the body (line 4), not the one in front-matter (line 2)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_fix_basic_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with spaces   \nAnother line  \nClean line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: 3 spaces -> removed (doesn't match br_spaces=2)
        // Line 2: 2 spaces -> kept (matches br_spaces=2)
        // Line 3: no spaces -> unchanged
        assert_eq!(fixed, "Line with spaces\nAnother line  \nClean line");
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD009TrailingSpaces::new(2, true);
        // Strict mode keeps the br_spaces exception only on non-last paragraph lines:
        //   - line 1 ("Line with spaces  ") has paragraph continuation on line 2 -> allowed
        //   - line 2 ("Code block:  ") is followed by a code fence (non-paragraph next)
        //     so its `<br>` is wasted -> flagged
        //   - lines 3 and 5 are fence boundaries (non-paragraph) -> flagged
        //   - line 4 is inside the code block; rumdl's strict mode is intentionally
        //     more thorough than markdownlint and flags trailing whitespace there too
        let content = "Line with spaces  \nCode block:  \n```  \nCode with spaces  \n```  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        let lines_flagged: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert_eq!(lines_flagged, vec![2, 3, 4, 5], "got: {result:?}");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with spaces  \nCode block:\n```\nCode with spaces\n```");
    }

    #[test]
    fn test_strict_mode_allows_br_spaces_on_paragraph_lines() {
        // markdownlint parity: when `strict = true`, the br_spaces (2-space) line break
        // is still allowed on paragraph-context lines because the trailing spaces
        // produce a real <br>. Strict only flags trailing spaces on lines that can't
        // produce a useful line break (headings, code blocks, last line, etc.).
        //
        // Reproduction from issue #593: blockquote prose with a 2-space line break.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "> Note:  \n> This is in a new line due to 2 spaces behind \"Note:\".\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "strict mode should allow br_spaces on paragraph-context lines, got: {result:?}"
        );

        // The fix() must not strip those spaces either, since the rule didn't flag them.
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_on_heading() {
        // Headings don't produce a <br> from trailing spaces, so strict mode flags them.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "# Heading  \nFollow-up paragraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "strict should flag heading br_spaces, got: {result:?}");
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_on_last_paragraph_line() {
        // markdownlint parity: a `<br>` from trailing spaces on the last line of a
        // paragraph is unobservable (the paragraph already ends at the next blank
        // line), so strict mode flags it.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Paragraph  \n\nNext paragraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![1],
            "strict should flag br_spaces on a single-line paragraph, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_between_list_items() {
        // markdownlint parity: each top-level list item is its own paragraph block.
        // Trailing br_spaces on item 1 don't bridge into item 2; strict flags them.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "- item 1  \n- item 2\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![1],
            "strict should flag br_spaces at the end of a list item, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_allows_br_spaces_in_list_item_continuation() {
        // markdownlint parity: a list item with a continuation line is a single
        // paragraph; a trailing 2-space line break inside it is meaningful.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "- first line  \n  second line of same item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "strict should allow br_spaces between a list item and its continuation, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_before_heading() {
        // markdownlint parity: an ATX heading interrupts the paragraph above it,
        // so the line before it is a paragraph end — strict flags trailing br_spaces.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Paragraph  \n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![1],
            "strict should flag br_spaces on the line before a heading, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_on_setext_heading_text() {
        // Setext heading text line is a heading, not a paragraph, so trailing spaces
        // on it can't produce a <br>. Strict mode flags it.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Setext heading  \n===\n\nFollow-up paragraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![1],
            "strict should flag setext heading text trailing spaces, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_on_setext_underline() {
        // The setext underline is part of the heading block, not a paragraph.
        // rumdl marks `heading` only on the text line; the underline is detected by
        // looking back. Strict mode flags trailing spaces on it.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Setext heading\n===  \n\nFollow-up paragraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![2],
            "strict should flag setext underline trailing spaces, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_flags_br_spaces_in_indented_code_block() {
        // Indented code blocks aren't paragraphs. rumdl's strict mode is intentionally
        // stricter than markdownlint here (markdownlint excludes code blocks entirely
        // unless `code_blocks: true`); we surface the trailing whitespace as a warning.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Paragraph above.\n\n    code line  \n    another code  \n\nParagraph below.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![3, 4],
            "strict should flag indented code block trailing spaces, got: {result:?}"
        );
    }

    #[test]
    fn test_strict_mode_allows_br_spaces_in_table_row() {
        // GFM table rows close with `|`; the trailing whitespace before that pipe is
        // inside the cell, not at the line end. This test guards against future
        // refactors mistakenly treating table rows as a special non-paragraph context
        // when there is in fact no end-of-line whitespace on the row.
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "| col |\n| --- |\n| cell  |\n\nParagraph.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "rows that don't actually have trailing whitespace shouldn't trigger MD009, got: {result:?}"
        );
    }

    #[test]
    fn test_non_strict_mode_with_code_blocks() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with spaces  \n```\nCode with spaces  \n```\nOutside code  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // In non-strict mode, code blocks are not checked
        // Line 1 has 2 spaces (= br_spaces), so it's OK
        // Line 5 is last line without newline, so trailing spaces are flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_br_spaces_preservation() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with two spaces  \nLine with three spaces   \nLine with one space ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // br_spaces=2, so lines with exactly 2 spaces are OK
        // Line 2 has 3 spaces (should be removed, not normalized)
        // Line 3 has 1 space and is last line without newline (will be removed)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);

        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: keeps 2 spaces (exact match with br_spaces)
        // Line 2: removes all 3 spaces (doesn't match br_spaces)
        // Line 3: last line without newline, spaces removed
        assert_eq!(
            fixed,
            "Line with two spaces  \nLine with three spaces\nLine with one space"
        );
    }

    #[test]
    fn test_empty_lines_with_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Normal line\n   \n  \nAnother line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Empty line has trailing spaces");
        assert_eq!(result[1].message, "Empty line has trailing spaces");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Normal line\n\n\nAnother line");
    }

    #[test]
    fn test_empty_blockquote_lines() {
        let rule = MD009TrailingSpaces::default();
        let content = "> Quote\n>   \n> More quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].message, "3 trailing spaces found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Quote\n>\n> More quote"); // All trailing spaces removed
    }

    #[test]
    fn test_last_line_handling() {
        let rule = MD009TrailingSpaces::new(2, false);

        // Content without final newline
        let content = "First line  \nLast line  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Last line without newline should have trailing spaces removed
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "First line  \nLast line");

        // Content with final newline
        let content_with_newline = "First line  \nLast line  \n";
        let ctx = LintContext::new(content_with_newline, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Both lines should preserve br_spaces
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_trailing_space() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with one space ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Trailing space found");
    }

    #[test]
    fn test_tabs_not_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with tab\t\nLine with spaces  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only spaces are checked, not tabs
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_mixed_content() {
        let rule = MD009TrailingSpaces::new(2, false);
        // Construct content with actual trailing spaces using string concatenation
        let mut content = String::new();
        content.push_str("# Heading");
        content.push_str("   "); // Add 3 trailing spaces (more than br_spaces=2)
        content.push('\n');
        content.push_str("Normal paragraph\n> Blockquote\n>\n```\nCode block\n```\n- List item\n");

        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the line with trailing spaces
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("trailing spaces"));
    }

    #[test]
    fn test_column_positions() {
        let rule = MD009TrailingSpaces::default();
        let content = "Text   ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 5); // After "Text"
        assert_eq!(result[0].end_column, 8); // After all spaces
    }

    #[test]
    fn test_default_config() {
        let rule = MD009TrailingSpaces::default();
        let config = rule.default_config_section();
        assert!(config.is_some());
        let (name, _value) = config.unwrap();
        assert_eq!(name, "MD009");
    }

    #[test]
    fn test_from_config() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("br_spaces".to_string(), toml::Value::Integer(3));
        rule_config
            .values
            .insert("strict".to_string(), toml::Value::Boolean(true));
        config.rules.insert("MD009".to_string(), rule_config);

        let rule = MD009TrailingSpaces::from_config(&config);
        let content = "Line   ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // In strict mode, should remove all spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line");
    }

    #[test]
    fn test_list_item_empty_lines() {
        // Create rule with list_item_empty_lines enabled
        let config = MD009Config {
            list_item_empty_lines: true,
            ..Default::default()
        };
        let rule = MD009TrailingSpaces::from_config_struct(config);

        // Test unordered list with empty line
        let content = "- First item\n  \n- Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should not flag the empty line with spaces after list item
        assert!(result.is_empty());

        // Test ordered list with empty line
        let content = "1. First item\n  \n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test that non-list empty lines are still flagged
        let content = "Normal paragraph\n  \nAnother paragraph";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_list_item_empty_lines_disabled() {
        // Default config has list_item_empty_lines disabled
        let rule = MD009TrailingSpaces::default();

        let content = "- First item\n  \n- Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the empty line with spaces
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD009TrailingSpaces::default();
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("Line {i} with spaces  \n"));
        }
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Default br_spaces=2, so all lines with 2 spaces are OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_content_after_fix() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "**Bold** text  \n*Italic* text  \n[Link](url)  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "**Bold** text  \n*Italic* text  \n[Link](url)");
    }

    #[test]
    fn test_nested_blockquotes() {
        let rule = MD009TrailingSpaces::default();
        let content = "> > Nested  \n> >   \n> Normal  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 2 has empty blockquote with 3 spaces, line 3 is last line without newline
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);

        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: Keeps 2 spaces (exact match with br_spaces)
        // Line 2: Empty blockquote with 3 spaces -> removes all (doesn't match br_spaces)
        // Line 3: Last line without newline -> removes all spaces
        assert_eq!(fixed, "> > Nested  \n> >\n> Normal");
    }

    #[test]
    fn test_normalized_line_endings() {
        let rule = MD009TrailingSpaces::default();
        // In production, content is normalized to LF at I/O boundary
        let content = "Line with spaces  \nAnother line  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 1 has 2 spaces (= br_spaces) so it's OK
        // Line 2 is last line without newline, so it's flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_issue_80_no_space_normalization() {
        // Test for GitHub issue #80 - MD009 should not add spaces when removing trailing spaces
        let rule = MD009TrailingSpaces::new(2, false); // br_spaces=2

        // Test that 1 trailing space is removed, not normalized to 2
        let content = "Line with one space \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "Trailing space found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with one space\nNext line");

        // Test that 3 trailing spaces are removed, not normalized to 2
        let content = "Line with three spaces   \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "3 trailing spaces found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with three spaces\nNext line");

        // Test that exactly 2 trailing spaces are preserved
        let content = "Line with two spaces  \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0); // Should not flag lines with exact br_spaces

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with two spaces  \nNext line");
    }

    #[test]
    fn test_unicode_whitespace_idempotent_fix() {
        // Verify that mixed Unicode (U+2000 EN QUAD) and ASCII trailing whitespace
        // is stripped in a single idempotent pass.
        let rule = MD009TrailingSpaces::default(); // br_spaces=2

        // Case from proptest: blockquote with U+2000 and ASCII space
        let content = "> 0\u{2000} ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should detect trailing Unicode+ASCII whitespace");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> 0", "Should strip all trailing whitespace in one pass");

        // Verify idempotency: fixing again should produce same result
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed2, "Fix must be idempotent");
    }

    #[test]
    fn test_unicode_whitespace_variants() {
        let rule = MD009TrailingSpaces::default();

        // U+2000 EN QUAD
        let content = "text\u{2000}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "text\n");

        // U+2001 EM QUAD
        let content = "text\u{2001}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "text\n");

        // U+3000 IDEOGRAPHIC SPACE
        let content = "text\u{3000}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "text\n");

        // Mixed: Unicode space + ASCII spaces
        // The trailing 2 ASCII spaces match br_spaces, so they are preserved.
        // The U+2000 between content and the spaces is removed.
        let content = "text\u{2000}  \n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Unicode+ASCII mix should be flagged");
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "text\n",
            "All trailing whitespace should be stripped when mix includes Unicode"
        );
        // Verify idempotency
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed2, "Fix must be idempotent");

        // Pure ASCII 2 spaces should still be preserved as br_spaces
        let content = "text  \nnext\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Pure ASCII br_spaces should still be preserved");
    }

    #[test]
    fn test_unicode_whitespace_strict_mode() {
        let rule = MD009TrailingSpaces::new(2, true);

        // Strict mode should remove all Unicode whitespace too
        let content = "text\u{2000}\nmore\u{3000}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "text\nmore\n");
    }

    /// Helper: after fix(), run check() on the result and assert zero violations remain.
    fn assert_fix_roundtrip(rule: &MD009TrailingSpaces, content: &str) {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx2).unwrap();
        assert!(
            remaining.is_empty(),
            "After fix(), check() should find 0 violations.\nOriginal: {content:?}\nFixed: {fixed:?}\nRemaining: {remaining:?}"
        );
    }

    #[test]
    fn test_roundtrip_basic_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(&rule, "Line with spaces   \nAnother line  \nClean line");
    }

    #[test]
    fn test_roundtrip_strict_mode() {
        let rule = MD009TrailingSpaces::new(2, true);
        assert_fix_roundtrip(
            &rule,
            "Line with spaces  \nCode block:  \n```  \nCode with spaces  \n```  ",
        );
    }

    #[test]
    fn test_roundtrip_empty_lines() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(&rule, "Normal line\n   \n  \nAnother line");
    }

    #[test]
    fn test_roundtrip_br_spaces_preservation() {
        let rule = MD009TrailingSpaces::new(2, false);
        assert_fix_roundtrip(
            &rule,
            "Line with two spaces  \nLine with three spaces   \nLine with one space ",
        );
    }

    #[test]
    fn test_roundtrip_last_line_no_newline() {
        let rule = MD009TrailingSpaces::new(2, false);
        assert_fix_roundtrip(&rule, "First line  \nLast line  ");
    }

    #[test]
    fn test_roundtrip_last_line_with_newline() {
        let rule = MD009TrailingSpaces::new(2, false);
        assert_fix_roundtrip(&rule, "First line  \nLast line  \n");
    }

    #[test]
    fn test_roundtrip_unicode_whitespace() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(&rule, "> 0\u{2000} ");
        assert_fix_roundtrip(&rule, "text\u{2000}\n");
        assert_fix_roundtrip(&rule, "text\u{3000}\n");
        assert_fix_roundtrip(&rule, "text\u{2000}  \n");
    }

    #[test]
    fn test_roundtrip_code_blocks_non_strict() {
        let rule = MD009TrailingSpaces::new(2, false);
        assert_fix_roundtrip(
            &rule,
            "Line with spaces  \n```\nCode with spaces  \n```\nOutside code  ",
        );
    }

    #[test]
    fn test_roundtrip_blockquotes() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(&rule, "> Quote\n>   \n> More quote");
        assert_fix_roundtrip(&rule, "> > Nested  \n> >   \n> Normal  ");
    }

    #[test]
    fn test_roundtrip_list_item_empty_lines() {
        let config = MD009Config {
            list_item_empty_lines: true,
            ..Default::default()
        };
        let rule = MD009TrailingSpaces::from_config_struct(config);
        assert_fix_roundtrip(&rule, "- First item\n  \n- Second item");
        assert_fix_roundtrip(&rule, "Normal paragraph\n  \nAnother paragraph");
    }

    #[test]
    fn test_roundtrip_complex_document() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(
            &rule,
            "# Title   \n\nParagraph  \n\n- List   \n  - Nested  \n\n```\ncode   \n```\n\n> Quote   \n>    \n\nEnd  ",
        );
    }

    #[test]
    fn test_roundtrip_multibyte() {
        let rule = MD009TrailingSpaces::new(2, true);
        assert_fix_roundtrip(&rule, "- 1€ expenses \n");
        assert_fix_roundtrip(&rule, "€100 + €50 = €150   \n");
        assert_fix_roundtrip(&rule, "Hello 你好世界   \n");
        assert_fix_roundtrip(&rule, "Party 🎉🎉🎉   \n");
        assert_fix_roundtrip(&rule, "안녕하세요   \n");
    }

    #[test]
    fn test_roundtrip_mixed_tabs_and_spaces() {
        let rule = MD009TrailingSpaces::default();
        assert_fix_roundtrip(&rule, "Line with tab\t\nLine with spaces  ");
        assert_fix_roundtrip(&rule, "Line\t  \nAnother\n");
    }

    #[test]
    fn test_roundtrip_heading_with_br_spaces() {
        // Headings with exactly br_spaces trailing spaces: check() does not flag them,
        // so fix() should not remove them. This tests consistency.
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "# Heading  \nParagraph\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // check() allows br_spaces on headings (does not flag)
        assert!(
            warnings.is_empty(),
            "check() should not flag heading with exactly br_spaces trailing spaces"
        );
        assert_fix_roundtrip(&rule, content);
    }

    #[test]
    fn test_fix_replacement_always_removes_trailing_spaces() {
        // The fix replacement must always be an empty string, fully removing
        // trailing spaces that do not match the br_spaces allowance.
        let rule = MD009TrailingSpaces::new(2, false);

        // 3 trailing spaces (not matching br_spaces=2) should produce a warning
        // with an empty replacement that removes them entirely
        let content = "Hello   \nWorld\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fix = result[0].fix.as_ref().expect("Should have a fix");
        assert_eq!(
            fix.replacement, "",
            "Fix replacement should always be empty string (remove trailing spaces)"
        );

        // Also verify via fix() method
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Hello\nWorld\n");
    }
}
