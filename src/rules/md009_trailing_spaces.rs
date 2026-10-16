use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::hard_break::br_produces_useful_break;
use crate::utils::range_utils::calculate_trailing_range;
use crate::utils::regex_cache::{ORDERED_LIST_MARKER_REGEX, UNORDERED_LIST_MARKER_REGEX};

mod md009_config;
use md009_config::MD009Config;

#[derive(Debug, Clone, Default)]
pub struct MD009TrailingSpaces {
    config: MD009Config,
}

impl MD009TrailingSpaces {
    pub fn new(br_spaces: usize, strict: bool) -> Self {
        Self {
            config: MD009Config {
                br_spaces: crate::types::BrSpaces::new(br_spaces),
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

    /// Count the trailing spaces and tabs.
    ///
    /// CommonMark strips only spaces and tabs at the end of a line, so any other
    /// Unicode space (U+00A0, U+2000..U+200A, U+3000, ...) is text. Counting it would
    /// make the fix rewrite the document's content, and would turn a line holding only
    /// such a character into a "blank" line that splits its paragraph.
    fn count_trailing_whitespace(line: &str) -> usize {
        line.chars().rev().take_while(|&c| Self::is_trailing_char(c)).count()
    }

    fn is_trailing_char(c: char) -> bool {
        c == ' ' || c == '\t'
    }

    fn trimmed_len_ascii_whitespace(line: &str) -> usize {
        line.as_bytes()
            .iter()
            .rposition(|&b| b != b' ' && b != b'\t')
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
        if !line.trim_matches(Self::is_trailing_char).is_empty() {
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
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(9))
    }

    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces should be removed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        let mut warnings = Vec::new();

        // Use pre-computed lines (needed for looking back at prev_line)
        let lines = ctx.raw_lines();

        let mut filtered = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_pymdown_blocks()
            .skip_esm_blocks();
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
            // The run the fix removes. On a non-ASCII line that is every trailing space
            // and tab; an ASCII line keeps the spaces-only count its hard-break
            // comparison below is written against.
            let trailing_all_whitespace = if line_is_ascii {
                trailing_ascii_spaces
            } else {
                Self::count_trailing_whitespace(line)
            };

            // A line is reported only when it ends in a space: a bare trailing tab is
            // MD010's. The run the fix removes is every space and tab at the end.
            if trailing_ascii_spaces == 0 {
                continue;
            }

            // Handle empty lines
            let trimmed_len = if line_is_ascii {
                Self::trimmed_len_ascii_whitespace(line)
            } else {
                line.trim_end_matches(Self::is_trailing_char).len()
            };
            // In MDX and templates, trailing whitespace may be string data.
            // Protect the actual run so whitespace after a closing expression
            // remains lintable, including in strict mode and on blank lines.
            let whitespace_start = ctx.line_offsets[line_num] + trimmed_len;
            let whitespace_end = ctx.line_offsets[line_num] + line.len();
            // A run that ends a line inside a multi-line code span is part of the
            // span's text, which a renderer keeps, so it is neither trailing space
            // nor removable. The run is judged by its first byte: a span ends with
            // a backtick, so a run cannot begin before the span and end inside it,
            // and a run after the closing fence starts outside the span. The
            // parser's own spans are not enough: MkDocs container bodies are
            // supplemented in `code_spans()`, and a reader without math finds
            // spans that rumdl's math reading hides.
            if ctx.is_code_span_text_byte(whitespace_start) {
                continue;
            }
            if ctx.overlaps_mdx_inline_code(whitespace_start, whitespace_end)
                || (ctx.flavor.supports_jsx()
                    && (ctx.is_in_html_tag(whitespace_start) || ctx.is_in_jsx_component_tag(whitespace_start)))
                || ctx.is_in_jinja_string(whitespace_start)
                || ctx.is_in_shortcode(whitespace_start)
            {
                continue;
            }

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
                        ctx.line_column_byte_range_with_length(line_num + 1, 1, line.chars().count())
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
            // The br_spaces exception applies when the trailing whitespace produces a `<br>`;
            // a br_spaces below 2 turns the exception off, so nothing matches it.
            // In non-strict mode we accept br_spaces anywhere that isn't the literal final
            // line of the document. In strict mode (markdownlint parity) we additionally
            // require the line to be a non-last line of a paragraph: structural lines
            // (headings, fences, setext underlines, horizontal rules, ...) and last lines
            // of paragraphs (single-line paragraphs, lines before blanks, list-item ends,
            // ...) all get flagged because the `<br>` they would emit is unobservable.
            let is_truly_last_line = line_num == lines.len() - 1 && !content.ends_with('\n');
            let has_only_ascii_trailing = trailing_ascii_spaces == trailing_all_whitespace;
            let matches_br_spaces = self.config.br_spaces.line_break() == Some(trailing_ascii_spaces);
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
                line.trim_end_matches(Self::is_trailing_char)
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
                ctx.line_column_byte_range_with_length(
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty.
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
    fn test_mdx_module_trailing_spaces_preserve_template_string_values() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(0, strict);
            let source = "export const value = `left   \n   \nright`\n\nVisible prose   \n";
            let expected = "export const value = `left   \n   \nright`\n\nVisible prose\n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_mdx_expression_trailing_spaces_preserve_values() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(0, strict);
            for expression in ["{`left   \n   \nright`}", "{/* c */ `内容\u{2000}\nvalue`}"] {
                let source = format!("export const unused = 1\n\n{expression}   \n");
                let expected = format!("export const unused = 1\n\n{expression}\n");
                let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::MDX, None);
                assert_eq!(
                    rule.check(&ctx).unwrap().len(),
                    1,
                    "only prose whitespace is lintable: {source}"
                );
                assert_eq!(rule.fix(&ctx).unwrap(), expected);
            }
        }
    }

    #[test]
    fn test_mdx_attribute_trailing_spaces_preserve_string_data() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(0, strict);
            let source = "Before <span title={`left   \nright`}>text</span> after   \n";
            let expected = "Before <span title={`left   \nright`}>text</span> after\n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_jinja_quoted_trailing_spaces_preserve_string_data() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(0, strict);
            let source = "Before {{ 'left   \n   \nright' }} after   \n";
            let expected = "Before {{ 'left   \n   \nright' }} after\n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_hugo_raw_argument_trailing_spaces_preserve_values() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(0, strict);
            let source = "Before {{< note title=`left   \n   \nright` >}} after   \n";
            let expected = "Before {{< note title=`left   \n   \nright` >}} after\n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Hugo, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

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
        // Only the ASCII space after the U+2000 is trailing whitespace. The U+2000
        // is text, so it stays and the fix reaches its fixed point in one pass.
        let rule = MD009TrailingSpaces::default(); // br_spaces=2

        let content = "> 0\u{2000} ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "The ASCII space after the Unicode space is trailing");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> 0\u{2000}", "Only the ASCII space is removed");

        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed2, "Fix must be idempotent");
    }

    #[test]
    fn test_unicode_whitespace_is_text_not_trailing_whitespace() {
        // CommonMark trims only spaces and tabs at the end of a line, so every other
        // Unicode space is content: reporting or removing it changes the document.
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(2, strict);
            for ws in ['\u{a0}', '\u{2000}', '\u{2001}', '\u{2003}', '\u{3000}'] {
                for content in [
                    format!("text{ws}\n"),
                    format!("text{ws}\t\n"),
                    format!("# head{ws}\n"),
                    format!("> quote{ws}\n"),
                    format!("- item{ws}\nnext\n"),
                    format!("a\n{ws}\nb\n"),
                    format!("{ws}\n"),
                ] {
                    let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
                    assert!(
                        rule.check(&ctx).unwrap().is_empty(),
                        "strict={strict}: flagged text {content:?}"
                    );
                    assert_eq!(rule.fix(&ctx).unwrap(), content);
                }
            }
        }
    }

    #[test]
    fn test_ascii_space_after_unicode_whitespace_is_still_trailing() {
        let rule = MD009TrailingSpaces::default();
        // The Unicode space stays, the ASCII run after it is removed. The run
        // of 2 would be a hard break, so use 3 to make the fix observable.
        for (content, expected) in [
            ("text\u{2000}   \n", "text\u{2000}\n"),
            ("text\u{a0}\t \n", "text\u{a0}\n"),
            ("text\u{3000}\t   \n", "text\u{3000}\n"),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{content:?}");
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected);
            let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx2).unwrap(), fixed, "idempotent: {content:?}");
        }
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
        assert_fix_roundtrip(&rule, "text\u{2000}   \n");
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

    /// The rule configurations a code span must survive: the default, strict
    /// mode, and `br-spaces = 0`, which turns the hard-break exemption off.
    fn span_configs() -> [MD009TrailingSpaces; 3] {
        [
            MD009TrailingSpaces::default(),
            MD009TrailingSpaces::new(2, true),
            MD009TrailingSpaces::new(0, false),
        ]
    }

    fn assert_kept_whole(source: &str, flavor: crate::config::MarkdownFlavor) {
        for rule in span_configs() {
            let ctx = LintContext::new(source, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                warnings.is_empty(),
                "whitespace inside a code span is its text, not trailing space: {source:?} {:?} {warnings:?}",
                rule.config
            );
            assert_eq!(rule.fix(&ctx).unwrap(), source, "fix changed a code span: {source:?}");
        }
    }

    #[test]
    fn trailing_spaces_inside_a_multi_line_code_span_are_kept() {
        assert_kept_whole("`a   \nb`\n", crate::config::MarkdownFlavor::Standard);
    }

    #[test]
    fn code_span_whitespace_is_kept_in_every_shape_a_span_can_take() {
        for source in [
            // The count of spaces does not matter, including the hard-break count.
            "`a \nb`\n",
            "`a  \nb`\n",
            "`a    \nb`\n",
            // A run right after the opening fence.
            "` \n`\n",
            "``a   \nb``\n",
            "``a`  \nb``\n",
            // Three lines: every inner run is text, whatever its length.
            "`a   \nmid  \nb`\n",
            "`a   \nmid    \nb`\n",
            // Tabs and Unicode spaces are whitespace the rule would also remove.
            "`a \t \nb`\n",
            "`a\u{2000}\nb`\n",
            "`\u{e9}   \nb`\n",
            // Containers keep their prefix on the continuation line.
            "- `a   \n  b`\n",
            "1. `a   \n   b`\n",
            "- x\n  - `a   \n    b`\n",
            "> `a   \n> b`\n",
            "> > `a   \n> > b`\n",
            "- > `a   \n  > b`\n",
            "> - `a   \n>   b`\n",
            "> `a   \nb`\n",
            // CRLF documents.
            "`a   \r\nb`\r\n",
            // Spans that share a paragraph with other text, spans, or markup.
            "text `a   \nb` more\n",
            "`a   \nb` and `c   \nd`\n",
            "*`a   \nb`*\n",
            "[`a   \nb`](u)\n",
            "<span>`a   \nb`</span>\n",
            // No newline after the closing fence.
            "`a   \nb`",
            // Math: a backtick inside `$...$` opens no span for rumdl but does
            // for a reader without math, and the text must hold under both.
            "$x$ `a   \nb`\n",
            "A $x `y$   \nz` end\n",
        ] {
            assert_kept_whole(source, crate::config::MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn code_span_whitespace_is_kept_in_a_mkdocs_container() {
        // pulldown-cmark reads the indented admonition body as indented code, so
        // the span is only known through the container supplement.
        assert_kept_whole("!!! note\n\n    `a   \n    b`\n", crate::config::MarkdownFlavor::MkDocs);
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(2, strict);
            let source = "!!! note\n\n    `a   \n    b`\n\nouter   \n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MkDocs, None);
            assert_eq!(rule.fix(&ctx).unwrap(), "!!! note\n\n    `a   \n    b`\n\nouter\n");
        }
    }

    #[test]
    fn spaces_after_a_span_on_its_closing_line_are_still_trailing() {
        for strict in [false, true] {
            let rule = MD009TrailingSpaces::new(2, strict);
            let source = "`a   \nb` tail   \nnext\n";
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "only the run after the span: {warnings:?}");
            assert_eq!(warnings[0].line, 2);
            assert_eq!(rule.fix(&ctx).unwrap(), "`a   \nb` tail\nnext\n");
        }
    }

    #[test]
    fn trailing_whitespace_outside_a_code_span_is_still_removed() {
        for (source, expected) in [
            // Right after the closing fence, on the line that closes the span.
            ("`a`   \nnext\n", "`a`\nnext\n"),
            ("text   \nnext\n", "text\nnext\n"),
            // A backtick that never closes opens no span.
            ("`a   \nb\n", "`a\nb\n"),
            // The span would have to cross a blank line, so there is none.
            ("`a   \n\nb`\n", "`a\n\nb`\n"),
            // Raw HTML is not a code span.
            ("<code>a   \nb</code>\n", "<code>a\nb</code>\n"),
            // A span that ends before the run is over.
            ("`a` and `b`   \nnext\n", "`a` and `b`\nnext\n"),
        ] {
            for strict in [false, true] {
                let rule = MD009TrailingSpaces::new(2, strict);
                let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
                assert_eq!(rule.fix(&ctx).unwrap(), expected, "strict={strict}: {source:?}");
            }
        }
    }

    #[test]
    fn a_hard_break_next_to_a_code_span_is_kept() {
        let rule = MD009TrailingSpaces::default();
        let source = "`a`  \nnext `b`  \nlast\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), source);
    }

    #[test]
    fn br_spaces_zero_still_removes_a_two_space_run_outside_a_code_span() {
        let rule = MD009TrailingSpaces::new(0, false);
        let source = "text  \nnext\n\n`a  \nb`\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "text\nnext\n\n`a  \nb`\n");
    }

    #[test]
    fn fenced_code_is_skipped_unless_strict_and_a_span_in_it_is_not_special() {
        let source = "```\n`a   \nb`\n```\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(MD009TrailingSpaces::default().fix(&ctx).unwrap(), source);
    }

    #[test]
    fn test_strict_hard_break_survives_a_unicode_space_line() {
        // A line holding only U+00A0 is a paragraph line, so the break before it
        // (and a break on it) still renders and must be kept.
        let rule = MD009TrailingSpaces::new(2, true);
        for content in ["a  \n\u{a0}\nb\n", "\u{a0}  \nb\n"] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }
        // Positive control: a real blank line after the break makes it useless.
        let content = "a  \n\nb\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "a\n\nb\n");
    }
}
