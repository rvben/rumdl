use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
/// Rule MD010: No tabs
///
/// See [docs/md010.md](../../docs/md010.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::calculate_match_range;

pub mod md010_config;
pub use md010_config::MD010Config;

/// Rule MD010: Hard tabs
#[derive(Clone, Default)]
pub struct MD010NoHardTabs {
    config: MD010Config,
}

/// What the structure around a code block's text line fixes about its tabs.
struct CodeLine {
    /// Blockquote markers that belong to the containers around the block.
    quotes: usize,
    /// For the first line of an indented block, the byte where its code begins.
    prefix_end: Option<usize>,
}

impl MD010NoHardTabs {
    pub fn new(spaces_per_tab: usize) -> Self {
        Self {
            config: MD010Config {
                spaces_per_tab: crate::types::PositiveUsize::from_const(spaces_per_tab),
                code_blocks: false,
                ..Default::default()
            },
        }
    }

    pub const fn from_config_struct(config: MD010Config) -> Self {
        Self { config }
    }

    /// Byte offset where a line's structural prefix ends, and how many blockquote
    /// markers it holds.
    ///
    /// The prefix is the run of indentation, blockquote markers (`>`) and list
    /// markers (`-`, `*`, `+`, or up to nine digits followed by `.` or `)`, when
    /// followed by whitespace or the end of the line) that opens the line. Its
    /// whitespace decides where list content starts, how deep a quote is, and
    /// whether a line is indented code. A whitespace-only line has no structure
    /// and returns 0.
    ///
    /// `code_quotes` is set for the text of a code block. Its markers belong to
    /// the code unless they are the containers' own, so list markers are never
    /// read and at most that many `>` markers are.
    fn structural_prefix(line: &str, code_quotes: Option<usize>) -> (usize, usize) {
        let bytes = line.as_bytes();
        if bytes.iter().all(|b| matches!(b, b' ' | b'\t')) {
            return (0, 0);
        }
        let mut pos = 0;
        let mut quotes = 0;
        loop {
            while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
                pos += 1;
            }
            let Some(&first) = bytes.get(pos) else {
                return (pos, quotes);
            };
            let marker_len = match first {
                b'>' if code_quotes.is_none_or(|limit| quotes < limit) => 1,
                b'-' | b'*' | b'+' if code_quotes.is_none() => 1,
                b'0'..=b'9' if code_quotes.is_none() => {
                    let digits = bytes[pos..].iter().take_while(|b| b.is_ascii_digit()).count();
                    match bytes.get(pos + digits) {
                        Some(b'.' | b')') if digits <= 9 => digits + 1,
                        _ => return (pos, quotes),
                    }
                }
                b'[' if code_quotes.is_none() => match Self::footnote_label_len(&line[pos..]) {
                    Some(len) => len,
                    None => return (pos, quotes),
                },
                _ => return (pos, quotes),
            };
            if first != b'>' && !matches!(bytes.get(pos + marker_len), None | Some(b' ' | b'\t')) {
                return (pos, quotes);
            }
            quotes += usize::from(first == b'>');
            pos += marker_len;
        }
    }

    /// Length of a footnote definition label (`[^id]:`) at the start of `text`.
    ///
    /// The label may hold spaces and non-ASCII characters but no brackets or
    /// tabs, and must be non-empty. Like a list marker it counts only when
    /// whitespace or the end of the line follows, and that whitespace decides
    /// whether the footnote's first block is a paragraph or indented code.
    fn footnote_label_len(text: &str) -> Option<usize> {
        let label = text.strip_prefix("[^")?;
        let end = label.find([']', '[', '\t'])?;
        (end > 0 && label[end..].starts_with("]:")).then_some(2 + end + 2)
    }

    /// For the text lines of a code block, the number of blockquote markers that
    /// belong to the containers around it, and for the first line of an indented
    /// block, where its code text begins.
    ///
    /// A fence's opener is Markdown and is not listed. The first line of an
    /// indented block is listed because it can open the containers with markers
    /// that share the line with code: its code begins four columns past the
    /// enclosing container's content column, and everything before that is
    /// structure.
    fn code_quote_markers(ctx: &crate::lint_context::LintContext) -> std::collections::HashMap<usize, CodeLine> {
        let mut lines = std::collections::HashMap::new();
        for detail in &ctx.code_block_details {
            let line_of = |byte: usize| ctx.line_offsets.partition_point(|&off| off <= byte).saturating_sub(1);
            let first = line_of(detail.start);
            let last = line_of(detail.end.saturating_sub(1)).max(first);
            let opener = &ctx.content[ctx.line_content_byte_range(first + 1)];
            let quotes = match detail.content_column {
                Some(content_column) => {
                    let code_start = Self::offset_at_column(opener, content_column + 4);
                    let quotes = opener[..code_start].bytes().filter(|&b| b == b'>').count();
                    lines.insert(
                        first + 1,
                        CodeLine {
                            quotes,
                            prefix_end: Some(code_start),
                        },
                    );
                    quotes
                }
                None => Self::structural_prefix(opener, None).1,
            };
            for line in first + 2..=last + 1 {
                lines.insert(
                    line,
                    CodeLine {
                        quotes,
                        prefix_end: None,
                    },
                );
            }
        }
        lines
    }

    /// Byte offset of the first position on `line` at or past visual `column`,
    /// with a tab advancing to the next multiple of four. The line length when
    /// it ends first.
    fn offset_at_column(line: &str, column: usize) -> usize {
        let mut col = 0;
        for (i, b) in line.bytes().enumerate() {
            if col >= column {
                return i;
            }
            col += if b == b'\t' { 4 - col % 4 } else { 1 };
        }
        line.len()
    }

    /// Columns the whitespace in `line[start..end]` spans when the line starts at
    /// column 0 and a tab advances to the next multiple of four. Every character
    /// is one column, which holds inside the structural prefix (a footnote label
    /// may hold wide characters, but they are counted per character).
    fn columns_spanned(line: &str, start: usize, end: usize) -> usize {
        let mut col = 0;
        let mut start_col = 0;
        for (i, c) in line[..end].char_indices() {
            if i == start {
                start_col = col;
            }
            col += if c == '\t' { 4 - col % 4 } else { 1 };
        }
        col - start_col
    }

    /// Start of the trailing run of spaces and tabs when it must be removed.
    ///
    /// A tab followed by at most one space is a soft line break, but the same run
    /// written as four spaces is a hard break. A run that already ends in two
    /// spaces is a hard break and is left to the ordinary conversion.
    fn trailing_tab_run(line: &str) -> Option<usize> {
        let start = line.trim_end_matches([' ', '\t']).len();
        let run = &line[start..];
        (run.contains('\t') && !run.ends_with("  ")).then_some(start)
    }

    /// The language an info string declares, or `""` when it declares none.
    ///
    /// Normally that is the first whitespace-separated word, so
    /// ```` ```makefile title="Makefile" ```` gives `makefile`. Two flavors write
    /// a language inside braces instead, and each is read only where it is real
    /// syntax: a Pandoc code-attribute block names it as the first `.class`
    /// (`{#id .makefile}` gives `makefile`), and a Quarto executable chunk names
    /// its engine (`{r, echo=FALSE}` gives `r`).
    fn fence_language(info_string: &str, flavor: crate::config::MarkdownFlavor) -> std::borrow::Cow<'_, str> {
        let info = info_string.trim();

        if flavor.is_pandoc_compatible()
            && let Some(class) = crate::utils::pandoc::pandoc_code_class_lang(info)
        {
            return std::borrow::Cow::Borrowed(class);
        }

        if flavor == crate::config::MarkdownFlavor::Quarto
            && crate::utils::quarto_chunks::is_executable_chunk(info)
            && let Some(header) = crate::utils::quarto_chunks::parse_inline_chunk_header(info)
        {
            return std::borrow::Cow::Owned(header.engine);
        }

        std::borrow::Cow::Borrowed(info.split_whitespace().next().unwrap_or(""))
    }

    /// Lines (1-indexed) belonging to a code block whose language is listed in
    /// `ignore-code-languages`.
    ///
    /// Covers CommonMark fences plus the Azure DevOps colon fences the parser
    /// never sees. An indented code block has no info string and can never match.
    fn ignored_language_lines(&self, ctx: &crate::lint_context::LintContext) -> std::collections::HashSet<usize> {
        let mut ignored = std::collections::HashSet::new();

        for detail in ctx.code_block_details.iter().chain(ctx.colon_fence_details()) {
            let label = Self::fence_language(&detail.info_string, ctx.flavor);
            if label.is_empty()
                || !self
                    .config
                    .ignore_code_languages
                    .iter()
                    .any(|listed| listed.eq_ignore_ascii_case(label.as_ref()))
            {
                continue;
            }

            let start_line = ctx
                .line_offsets
                .partition_point(|&off| off <= detail.start)
                .saturating_sub(1);
            let end_byte = detail.end.saturating_sub(1);
            let end_line = ctx
                .line_offsets
                .partition_point(|&off| off <= end_byte)
                .saturating_sub(1);
            for line in start_line..=end_line {
                ignored.insert(line + 1);
            }
        }

        ignored
    }

    /// Runs of consecutive tabs as `(start, end)` byte offsets into `line`.
    fn find_and_group_tabs(line: &str) -> Vec<(usize, usize)> {
        let mut groups = Vec::new();
        let mut current_group_start: Option<usize> = None;
        let mut last_tab_pos = 0;

        for (i, byte) in line.bytes().enumerate() {
            if byte == b'\t' {
                if let Some(start) = current_group_start {
                    // We're in a group - check if this tab is consecutive
                    if i == last_tab_pos + 1 {
                        // Consecutive tab, continue the group
                        last_tab_pos = i;
                    } else {
                        // Gap found, save current group and start new one
                        groups.push((start, last_tab_pos + 1));
                        current_group_start = Some(i);
                        last_tab_pos = i;
                    }
                } else {
                    // Start a new group
                    current_group_start = Some(i);
                    last_tab_pos = i;
                }
            }
        }

        // Add the last group if there is one
        if let Some(start) = current_group_start {
            groups.push((start, last_tab_pos + 1));
        }

        groups
    }
}

impl Rule for MD010NoHardTabs {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(10))
    }

    fn name(&self) -> &'static str {
        "MD010"
    }

    fn description(&self) -> &'static str {
        "No tabs"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        let mut filtered = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_html_comments()
            .skip_mdx_comments()
            .skip_html_blocks()
            .skip_pymdown_blocks()
            .skip_mkdocstrings()
            .skip_esm_blocks();

        if !self.config.code_blocks {
            filtered = filtered.skip_code_blocks();
        }

        // Testing code-blocks here only avoids building a set that cannot matter:
        // with it false the walk has already dropped every code block line.
        // Only a code block that is checked has text whose prefix needs telling
        // apart from its containers' own.
        let code_quotes = if self.config.code_blocks {
            Self::code_quote_markers(ctx)
        } else {
            std::collections::HashMap::new()
        };

        let ignored_lines = if self.config.code_blocks && !self.config.ignore_code_languages.is_empty() {
            self.ignored_language_lines(ctx)
        } else {
            std::collections::HashSet::new()
        };

        for filtered_line in filtered {
            if ignored_lines.contains(&filtered_line.line_num) {
                continue;
            }
            let line_num = filtered_line.line_num - 1;
            let line = filtered_line.content;

            // Process tabs directly without intermediate collection
            let tab_groups = Self::find_and_group_tabs(line);
            if tab_groups.is_empty() {
                continue;
            }

            let code_line = code_quotes.get(&filtered_line.line_num);
            let prefix_end = match code_line {
                Some(CodeLine {
                    prefix_end: Some(end), ..
                }) => *end,
                _ => Self::structural_prefix(line, code_line.map(|c| c.quotes)).0,
            };
            let line_start = ctx.line_content_byte_range(line_num + 1).start;

            // Outside code, a trailing run holding a tab is removed whole, so it
            // cannot turn into a hard break.
            // Inside code, a tab is text and keeps the configured width.
            let trailing_start = if filtered_line.line_info.in_code_block || line.trim().is_empty() {
                None
            } else {
                Self::trailing_tab_run(line)
            };
            let mut tab_groups = tab_groups;
            if let Some(trailing) = trailing_start {
                tab_groups.retain(|&(start, _)| start < trailing);
                tab_groups.push((trailing, line.len()));
            }

            // Generate warning for each group of consecutive tabs
            for (start_pos, end_pos) in tab_groups {
                let is_trailing_run = trailing_start == Some(start_pos);
                // Tabs in executable/template code can be string data, rather
                // than Markdown alignment. Keep these values and attributes
                // intact while checking prose beside them on the same line.
                if ctx.overlaps_mdx_inline_code(line_start + start_pos, line_start + end_pos)
                    || (ctx.flavor.supports_jsx()
                        && (ctx.is_in_html_tag(line_start + start_pos)
                            || ctx.is_in_jsx_component_tag(line_start + start_pos)))
                    || ctx.is_in_jinja_string(line_start + start_pos)
                    || ctx.is_in_shortcode(line_start + start_pos)
                {
                    continue;
                }

                let tab_count = line[start_pos..end_pos].matches('\t').count();

                // A tab between a code span's backticks is part of the span's text,
                // and a renderer shows it. That includes the leading whitespace of a
                // line that continues the span: a line that matches all its
                // containers has it stripped, a lazy one keeps it, and nothing here
                // tells them apart. Like a fenced block, a span is skipped unless
                // `code-blocks` asks for every tab, and then its tab is reported
                // without a fix. A group is judged by its first byte: a span's
                // boundaries are backticks, so a run of tabs or of trailing
                // whitespace cannot start before a span and end inside it.
                if ctx.is_code_span_text_byte(line_start + start_pos) {
                    if self.config.code_blocks {
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, start_pos, end_pos - start_pos);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Found tab inside inline code, left as is because it is part of the code text"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                    continue;
                }

                let is_structural = !is_trailing_run && start_pos < prefix_end;

                // Whitespace inside the structural prefix is written as the columns
                // the tabs span, because that is what the parser reads from it. Any
                // other tab is alignment and uses the configured width.
                //
                // A backslash before a trailing run is a literal backslash while the
                // run is there and a hard break once it is gone, and the one space
                // that would keep it literal is itself trailing whitespace. That
                // warning has no fix.
                let replacement = if is_trailing_run {
                    (!line[..start_pos].ends_with('\\')).then(String::new)
                } else if is_structural {
                    Some(" ".repeat(Self::columns_spanned(line, start_pos, end_pos)))
                } else {
                    Some(" ".repeat(tab_count * self.config.spaces_per_tab.get()))
                };

                // Calculate precise character range for the tab group
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, start_pos, end_pos - start_pos);

                let message = if is_trailing_run {
                    "Found trailing tab, remove it".to_string()
                } else if line.trim().is_empty() {
                    if tab_count == 1 {
                        "Empty line contains tab".to_string()
                    } else {
                        format!("Empty line contains {tab_count} tabs")
                    }
                } else if is_structural {
                    if tab_count == 1 {
                        format!(
                            "Found leading tab, use {} spaces instead",
                            replacement.as_deref().map_or(0, str::len)
                        )
                    } else {
                        format!(
                            "Found {} leading tabs, use {} spaces instead",
                            tab_count,
                            replacement.as_deref().map_or(0, str::len)
                        )
                    }
                } else if tab_count == 1 {
                    "Found tab for alignment, use spaces instead".to_string()
                } else {
                    format!("Found {tab_count} tabs for alignment, use spaces instead")
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message,
                    severity: Severity::Warning,
                    fix: replacement
                        .map(|replacement| Fix::new(line_start + start_pos..line_start + end_pos, replacement)),
                });
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no tabs
        ctx.content.is_empty() || !ctx.has_char('\t')
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    crate::impl_rule_config_methods!(MD010Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_mdx_expression_tabs_preserve_string_values() {
        let rule = MD010NoHardTabs::default();
        for source in [
            "export const unused = 1\n\n{ \"left\tright\" }\n",
            "Before { \"left\t\tright\" } after\n",
            "# heading { \"内容\tvalue\" }\n",
            "export const unused = 1\n\n{`left\n\tright`}\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "MDX code is literal: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_mdx_attribute_tabs_preserve_values_and_lint_visible_children() {
        let rule = MD010NoHardTabs::default();
        for attribute in ["title=\"left\tright\"", "title={\"left\tright\"}"] {
            let source = format!("Before <span {attribute}>left\tright</span> after\n");
            let expected = format!("Before <span {attribute}>left    right</span> after\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_tabs_beside_mdx_expressions_are_still_fixed() {
        let rule = MD010NoHardTabs::default();
        let source = "Before\t{ \"left\tright\" }\tafter\n";
        let expected = "Before    { \"left\tright\" }    after\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }

    #[test]
    fn test_quoted_jinja_tabs_are_literal_but_directive_spacing_is_lintable() {
        let rule = MD010NoHardTabs::default();
        for template in [
            "{{ 'left\tright' }}",
            "{% set value = 'left\tright' %}{{ value }}",
            "{{ {\"text\": 'left\tright'}[\"text\"] }}",
        ] {
            let source = format!("Before {template}\tafter\n");
            let expected = format!("Before {template}    after\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
        let source = "Before {{ value\t}} after\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "Before {{ value    }} after\n");
    }

    #[test]
    fn test_hugo_shortcode_tabs_preserve_argument_values() {
        let rule = MD010NoHardTabs::default();
        for template in ["{{< note title=\"left\tright\" >}}", "{{% note title=`left\tright` %}}"] {
            let source = format!("Before {template}\tafter\n");
            let expected = format!("Before {template}    after\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Hugo, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_no_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "This is a line\nAnother line\nNo tabs here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_tab() {
        let rule = MD010NoHardTabs::default();
        let content = "Line with\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 10);
        assert_eq!(result[0].message, "Found tab for alignment, use spaces instead");
    }

    #[test]
    fn test_leading_tabs_skipped_in_indented_code_by_default() {
        // Both lines start with a tab at column 0: parsed as an indented code block.
        // Default code_blocks=false skips tabs in indented code blocks.
        let content = "\tIndented line\n\t\tDouble indented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert!(
            result_off.is_empty(),
            "indented code block skipped by default, got {result_off:?}"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented line\n\t\tDouble indented",
            "fix must preserve indented code block content"
        );

        // code_blocks=true: tabs inside indented code blocks are flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(result_on.len(), 2, "got {result_on:?}");
        assert_eq!(result_on[0].line, 1);
        assert_eq!(result_on[0].message, "Found leading tab, use 4 spaces instead");
        assert_eq!(result_on[1].line, 2);
        assert_eq!(result_on[1].message, "Found 2 leading tabs, use 8 spaces instead");
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented line\n        Double indented");
    }

    #[test]
    fn test_fix_tabs() {
        // Line 1 starts with a tab at column 0 -> indented code block, skipped by default.
        // Line 2 has a mid-line tab (alignment) -> flagged and fixed.
        let content = "\tIndented\nNormal\tline\nNo tabs";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let warnings_off = rule_off.check(&ctx).unwrap();
        assert_eq!(warnings_off.len(), 1, "got {warnings_off:?}");
        assert_eq!(warnings_off[0].line, 2);
        assert_eq!(warnings_off[0].message, "Found tab for alignment, use spaces instead");
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented\nNormal    line\nNo tabs",
            "indented code block line preserved; alignment tab fixed"
        );

        // code_blocks=true: line 1 is also flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let warnings_on = rule_on.check(&ctx).unwrap();
        assert_eq!(warnings_on.len(), 2, "got {warnings_on:?}");
        assert_eq!(warnings_on[0].line, 1);
        assert_eq!(warnings_on[1].line, 2);
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented\nNormal    line\nNo tabs");
    }

    #[test]
    fn test_custom_spaces_per_tab() {
        // Single tab at column 0 -> indented code block, skipped by default.
        let content = "\tIndented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::new(4);
        assert!(
            rule_off.check(&ctx).unwrap().is_empty(),
            "indented code block skipped by default"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented",
            "indented code block preserved by default"
        );

        // code_blocks=true: tab is flagged and fixed.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        assert_eq!(rule_on.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented");
    }

    #[test]
    fn test_fenced_code_block_tabs_skipped_by_default() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // By default (code_blocks=false) tabs inside code blocks are skipped
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Normal    line\n```\nCode\twith\ttab\n```\nAnother    line");
    }

    #[test]
    fn test_fenced_only_content_skipped_by_default() {
        let rule = MD010NoHardTabs::default();
        let content = "```\nCode\twith\ttab\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // By default (code_blocks=false) tabs in fenced code blocks are skipped
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_html_comments_ignored() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n<!-- HTML\twith\ttab -->\nAnother\tline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should not flag tabs in HTML comments
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_multiline_html_comments() {
        let rule = MD010NoHardTabs::default();
        let content = "Before\n<!--\nMultiline\twith\ttabs\ncomment\t-->\nAfter\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should only flag the tab after the comment
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_empty_lines_with_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal line\n\t\t\n\t\nAnother line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Empty line contains 2 tabs");
        assert_eq!(result[1].message, "Empty line contains tab");
    }

    #[test]
    fn test_mixed_tabs_and_spaces() {
        // " \t..." (space then tab) and "\t ..." (tab then space): both parsed as
        // indented code blocks by the shared spec-compliant flag.
        // Default code_blocks=false skips them.
        let content = " \tMixed indentation\n\t Mixed again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert!(
            result_off.is_empty(),
            "indented code block lines skipped, got {result_off:?}"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            " \tMixed indentation\n\t Mixed again",
            "content preserved unchanged"
        );

        // code_blocks=true: both lines flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(result_on.len(), 2, "got {result_on:?}");
        // Both indentation tabs reach column four, which is what the parser reads.
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Mixed indentation\n     Mixed again");
    }

    #[test]
    fn test_consecutive_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Text\t\t\tthree tabs\tand\tanother";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should group consecutive tabs
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].message, "Found 3 tabs for alignment, use spaces instead");
    }

    #[test]
    fn test_find_and_group_tabs() {
        // Test finding and grouping tabs in one pass
        let groups = MD010NoHardTabs::find_and_group_tabs("a\tb\tc");
        assert_eq!(groups, vec![(1, 2), (3, 4)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("\t\tabc");
        assert_eq!(groups, vec![(0, 2)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("no tabs");
        assert!(groups.is_empty());

        // Test with consecutive and non-consecutive tabs
        let groups = MD010NoHardTabs::find_and_group_tabs("\t\t\ta\t\tb");
        assert_eq!(groups, vec![(0, 3), (4, 6)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("\ta\tb\tc");
        assert_eq!(groups, vec![(0, 1), (2, 3), (4, 5)]);
    }

    #[test]
    fn tab_after_multibyte_characters_reports_its_character_column() {
        let rule = MD010NoHardTabs::default();
        let content = "a\u{e9}\u{e9}b\tdef\n\u{1f600}\t\tx\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let columns: Vec<_> = warnings.iter().map(|w| (w.line, w.column, w.end_column)).collect();
        assert_eq!(columns, vec![(1, 5, 6), (2, 2, 4)]);

        for warning in &warnings {
            let range = warning.fix.as_ref().unwrap().range.clone();
            assert!(content[range].chars().all(|c| c == '\t'), "fix must replace only tabs");
        }
        assert_eq!(rule.fix(&ctx).unwrap(), "a\u{e9}\u{e9}b    def\n\u{1f600}        x\n");
    }

    fn structural_prefix_end(line: &str) -> usize {
        MD010NoHardTabs::structural_prefix(line, None).0
    }

    #[test]
    fn test_structural_prefix_end() {
        assert_eq!(structural_prefix_end("\t\tcode"), 2);
        assert_eq!(structural_prefix_end(" \tcode"), 2);
        assert_eq!(structural_prefix_end("no tabs"), 0);
        assert_eq!(structural_prefix_end("\t"), 0);
        assert_eq!(structural_prefix_end("10.\tx"), 4);
        assert_eq!(structural_prefix_end("-\t> x"), 4);
        assert_eq!(structural_prefix_end("-x\ty"), 0);
        assert_eq!(structural_prefix_end("1234567890.\tx"), 0);
        assert_eq!(structural_prefix_end("12\tx"), 0);
        // In code text only the containers' own quote markers are structure.
        assert_eq!(MD010NoHardTabs::structural_prefix("-\tx", Some(0)), (0, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix("1.\tx", Some(0)), (0, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix(">\tx", Some(0)), (0, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix(">\t>\tx", Some(1)), (2, 1));
        assert_eq!(MD010NoHardTabs::structural_prefix("  \t-\tx", Some(0)), (3, 0));
    }

    #[test]
    fn test_default_config() {
        let rule = MD010NoHardTabs::default();
        let config = rule.default_config_section();
        assert!(config.is_some());
        let (name, _value) = config.unwrap();
        assert_eq!(name, "MD010");
    }

    #[test]
    fn test_from_config() {
        // "\tTab" at column 0 -> indented code block, skipped by default (code_blocks=false).
        let content_plain = "\tTab";
        let ctx_plain = LintContext::new(content_plain, crate::config::MarkdownFlavor::Standard, None);
        let rule_8_off = MD010NoHardTabs::new(8); // spaces_per_tab=8, code_blocks=false
        assert!(
            rule_8_off.check(&ctx_plain).unwrap().is_empty(),
            "indented code block skipped"
        );
        assert_eq!(
            rule_8_off.fix(&ctx_plain).unwrap(),
            "\tTab",
            "content preserved unchanged"
        );

        // code_blocks=true: the leading tab is flagged and replaced with the four
        // columns it spans, whatever spaces_per_tab is.
        let rule_8_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(8),
            code_blocks: true,
            ..Default::default()
        });
        assert_eq!(rule_8_on.check(&ctx_plain).unwrap().len(), 1);
        assert_eq!(rule_8_on.fix(&ctx_plain).unwrap(), "    Tab");

        // Fenced code block: tab skipped by default.
        let content_fenced = "```\n\tTab in code\n```";
        let ctx_fenced = LintContext::new(content_fenced, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule_8_off.check(&ctx_fenced).unwrap().is_empty(),
            "fenced code block skipped"
        );
        assert_eq!(rule_8_off.fix(&ctx_fenced).unwrap(), "```\n\tTab in code\n```");

        // code_blocks=true: tab inside fence is flagged.
        let result_on = rule_8_on.check(&ctx_fenced).unwrap();
        assert_eq!(result_on.len(), 1, "got {result_on:?}");
        assert_eq!(result_on[0].line, 2);
        assert_eq!(rule_8_on.fix(&ctx_fenced).unwrap(), "```\n    Tab in code\n```");
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD010NoHardTabs::default();
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("Line {i}\twith\ttabs\n"));
        }
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2000);
    }

    #[test]
    fn test_preserve_content() {
        let rule = MD010NoHardTabs::default();
        let content = "**Bold**\ttext\n*Italic*\ttext\n[Link](url)\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "**Bold**    text\n*Italic*    text\n[Link](url)    tab");
    }

    #[test]
    fn test_edge_cases() {
        let rule = MD010NoHardTabs::default();

        // Tab at end of line
        let content = "Text\t";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // Only tabs
        let content = "\t\t\t";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty line contains 3 tabs");
    }

    #[test]
    fn test_fenced_code_block_tabs_preserved_in_fix_by_default() {
        let rule = MD010NoHardTabs::default();

        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore\ttabs";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // By default (code_blocks=false) tabs in fenced code blocks are preserved
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
        let expected = "Text    with    tab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore    tabs";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_tilde_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        // 5-tilde fenced code block should be recognized and tabs inside should be skipped
        let content = "~~~~~\ncode\twith\ttab\n~~~~~\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only tabs on line 4 (outside the code block) should be flagged
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings but got {}: {:?}",
            result.len(),
            result
        );
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_backtick_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        // 5-backtick fenced code block
        let content = "`````\ncode\twith\ttab\n`````\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings but got {}: {:?}",
            result.len(),
            result
        );
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_indented_code_block_tabs_skipped_by_default() {
        // "    code\twith\ttab" is indented with 4 spaces -> indented code block.
        // Default code_blocks=false skips it; only the tab on the normal line is flagged.
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert_eq!(
            result_off.len(),
            1,
            "expected 1 warning (only normal-text tab), got {}: {:?}",
            result_off.len(),
            result_off
        );
        assert_eq!(result_off[0].line, 3);
        assert_eq!(result_off[0].message, "Found tab for alignment, use spaces instead");

        // code_blocks=true: all 3 tabs flagged (2 on line 1, 1 on line 3).
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(
            result_on.len(),
            3,
            "expected 3 warnings with code_blocks=true, got {}: {:?}",
            result_on.len(),
            result_on
        );
        assert_eq!(result_on[0].line, 1);
        assert_eq!(result_on[1].line, 1);
        assert_eq!(result_on[2].line, 3);
    }

    #[test]
    fn test_html_comment_end_then_start_same_line() {
        let rule = MD010NoHardTabs::default();
        // Tabs inside consecutive HTML comments should not be flagged
        let content =
            "<!-- first comment\nend --> text <!-- second comment\n\ttabbed content inside second comment\n-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected 0 warnings but got {}: {:?}",
            result.len(),
            result
        );
    }

    #[test]
    fn test_fix_tilde_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        let content = "~~~~~\ncode\twith\ttab\n~~~~~\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Tabs inside code block preserved, tabs outside replaced
        assert_eq!(fixed, "~~~~~\ncode\twith\ttab\n~~~~~\ntext    with    tab");
    }

    #[test]
    fn test_fix_indented_code_block_tabs_replaced() {
        // Default code_blocks=false: indented code block tabs preserved, normal-text tab fixed.
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "    code\twith\ttab\n\nNormal    text",
            "indented code block preserved; only normal-text tab fixed"
        );

        // code_blocks=true: all tabs replaced including those in the indented code block.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        assert_eq!(
            rule_on.fix(&ctx).unwrap(),
            "    code    with    tab\n\nNormal    text",
            "all tabs replaced with code_blocks=true"
        );
    }

    #[test]
    fn test_issue_630_default_skips_both_code_blocks() {
        // Default code_blocks = false: tabs skipped in BOTH block types.
        let rule = MD010NoHardTabs::default();
        let content = "Foo bar\n\n    for range 100 {\n    \tfoo()\n    }\n\nThis is a fenced\n\n```\nfor range 100 {\n\tfoo()\n}\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "both code blocks skipped, got {result:?}");
    }

    #[test]
    fn test_issue_630_code_blocks_true_flags_both() {
        // code_blocks = true: tabs flagged in BOTH block types.
        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let content = "Foo bar\n\n    for range 100 {\n    \tfoo()\n    }\n\nThis is a fenced\n\n```\nfor range 100 {\n\tfoo()\n}\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 4 "    \tfoo()": one alignment tab group inside the indented block.
        // Line 11 "\tfoo()": one leading tab group inside the fenced block.
        assert_eq!(result.len(), 2, "got {result:?}");
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 11);
    }

    #[test]
    fn test_code_blocks_toggle_fenced() {
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";

        // Default false: only the two tab groups outside the fence.
        let off = MD010NoHardTabs::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let r_off = off.check(&ctx).unwrap();
        assert_eq!(r_off.len(), 2, "got {r_off:?}");
        assert_eq!(r_off[0].line, 1);
        assert_eq!(r_off[1].line, 5);
        assert_eq!(
            off.fix(&ctx).unwrap(),
            "Normal    line\n```\nCode\twith\ttab\n```\nAnother    line"
        );

        // true: also the two groups on the fenced content line.
        let on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ..Default::default()
        });
        let r_on = on.check(&ctx).unwrap();
        assert_eq!(r_on.len(), 4, "got {r_on:?}");
        assert_eq!(r_on[0].line, 1);
        assert_eq!(r_on[1].line, 3);
        assert_eq!(r_on[2].line, 3);
        assert_eq!(r_on[3].line, 5);
        assert_eq!(
            on.fix(&ctx).unwrap(),
            "Normal    line\n```\nCode    with    tab\n```\nAnother    line"
        );
    }

    #[test]
    fn test_code_blocks_toggle_makefile_fence_preserved_by_default() {
        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n```\nMore\ttabs";
        let off = MD010NoHardTabs::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // Default preserves the Makefile recipe tab; only prose tabs fixed.
        assert_eq!(
            off.fix(&ctx).unwrap(),
            "Text    with    tab\n```makefile\ntarget:\n\tcommand\n```\nMore    tabs"
        );
    }

    #[test]
    fn test_ignore_code_languages_skips_a_listed_fence() {
        // code-blocks = true opts into checking tabs inside fences, but a tab in a
        // Makefile recipe is required syntax rather than a formatting mistake.
        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n```\n```sh\necho\thello\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
        });

        let result = rule.check(&ctx).unwrap();
        let lines: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert_eq!(
            lines,
            vec![1, 1, 7],
            "the makefile recipe tab on line 4 must be skipped, got {result:?}"
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Text    with    tab\n```makefile\ntarget:\n\tcommand\n```\n```sh\necho    hello\n```"
        );
    }

    #[test]
    fn test_ignore_code_languages_matches_the_label_case_insensitively() {
        // ```Makefile and ```makefile name the same language.
        let content = "```Makefile\ntarget:\n\tcommand\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "an uppercase fence label must match a lowercase configured language"
        );
    }

    #[test]
    fn test_ignore_code_languages_matches_only_the_first_info_string_word() {
        // The label is the first word, so attributes after it do not defeat the match.
        let labelled = "```makefile title=\"Makefile\"\ntarget:\n\tcommand\n```";
        let ctx = LintContext::new(labelled, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "a fence carrying attributes after its language must still match"
        );

        // A different language on the same shape is still reported, so the match is
        // not simply accepting every labelled fence.
        let other = "```shell title=\"Makefile\"\ntarget:\n\tcommand\n```";
        let other_ctx = LintContext::new(other, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.check(&other_ctx).unwrap().len(),
            1,
            "an unlisted language must still be reported"
        );
    }

    #[test]
    fn test_ignore_code_languages_cannot_match_an_indented_code_block() {
        // An indented code block has no info string, so it has no language to list.
        // Documented in docs/md010.md as a real gap.
        let content = "Text.\n\n    target:\n    \tcommand\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert_eq!(
            rule.check(&ctx).unwrap().len(),
            1,
            "an indented block carries no language, so the list cannot exempt it"
        );
    }

    #[test]
    fn test_ignore_code_languages_is_inert_without_code_blocks() {
        // With code-blocks at its default the whole block is already skipped, so the
        // list changes nothing in either direction.
        let content = "```makefile\ntarget:\n\tcommand\n```\n```sh\necho\thello\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "listing a language must not start checking blocks that code-blocks skips"
        );
    }

    #[test]
    fn test_ignore_code_languages_matches_a_pandoc_class_attribute() {
        // Pandoc declares a fence's language as the first `.class` of its attribute
        // block, so ```{.makefile} names the same language as ```makefile.
        let content = "```{.makefile}\ntarget:\n\tcommand\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "the first .class of a Pandoc attribute block is the fence language"
        );

        // An unlisted class on the same shape is still reported.
        let other = "```{.shell}\ntarget:\n\tcommand\n```";
        let other_ctx = LintContext::new(other, crate::config::MarkdownFlavor::Pandoc, None);
        assert_eq!(
            rule.check(&other_ctx).unwrap().len(),
            1,
            "an unlisted class must still be reported"
        );
    }

    #[test]
    fn test_ignore_code_languages_class_attribute_needs_a_pandoc_compatible_flavor() {
        // Under `standard` braces are not attribute syntax, so the label stays raw
        // and cannot match a bare language name.
        let content = "```{.makefile}\ntarget:\n\tcommand\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        assert_eq!(
            rule.check(&ctx).unwrap().len(),
            1,
            "attribute syntax must only be read under a Pandoc-compatible flavor"
        );
    }

    #[test]
    fn test_ignore_code_languages_matches_a_quarto_exec_chunk() {
        // A Quarto executable chunk names its engine inside the braces.
        let content = "```{r}\nx <- 1\n\tindented\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);

        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["r".to_string()],
            ..Default::default()
        });

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "a `{{r}}` chunk declares language r"
        );

        // A chunk for a different engine is still reported.
        let python = "```{python}\nx = 1\n\tindented\n```";
        let python_ctx = LintContext::new(python, crate::config::MarkdownFlavor::Quarto, None);
        assert_eq!(
            rule.check(&python_ctx).unwrap().len(),
            1,
            "an unlisted engine must still be reported"
        );
    }

    #[test]
    fn test_ignore_code_languages_matches_an_azure_colon_fence() {
        // Azure DevOps colon fences carry their language on the opener.
        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks: true,
            ignore_code_languages: vec!["makefile".to_string()],
            ..Default::default()
        });

        for content in [
            ":::makefile\ntarget:\n\tcommand\n:::",
            "::: makefile\ntarget:\n\tcommand\n:::",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::AzureDevOps, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "a colon fence's language must be honoured, got {:?} for {content:?}",
                rule.check(&ctx).unwrap()
            );
        }

        // An unlisted colon fence language is still reported.
        let other = ":::mermaid\nflowchart LR\n\tA --> B\n:::";
        let other_ctx = LintContext::new(other, crate::config::MarkdownFlavor::AzureDevOps, None);
        assert_eq!(
            rule.check(&other_ctx).unwrap().len(),
            1,
            "an unlisted colon fence language must still be reported"
        );
    }

    #[test]
    fn test_tabs_in_front_matter_are_not_flagged() {
        // Hard tabs inside YAML front matter are metadata, not Markdown body,
        // and must not be reported.
        let rule = MD010NoHardTabs::default();
        let content = "---\ntitle:\t\"Tabbed value\"\n---\n\n# Heading\n\nBody text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "tabs inside front matter must not be flagged, got: {result:?}"
        );
    }

    fn fixed_with(spaces_per_tab: usize, code_blocks: bool, content: &str) -> String {
        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(spaces_per_tab),
            code_blocks,
            ..Default::default()
        });
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx).unwrap()
    }

    fn fixed(content: &str) -> String {
        fixed_with(4, false, content)
    }

    /// Leading whitespace: tabs expand to the columns CommonMark gives them, which
    /// depend on where they start, so the document keeps its structure.
    const LEADING_ROWS: &[(&str, &str)] = &[
        // After a list marker the tab reaches the next multiple of four.
        ("-\tx\n\n    ## a\n", "-   x\n\n    ## a\n"),
        ("*\tx\n", "*   x\n"),
        ("+\tx\n", "+   x\n"),
        ("- \tx\n", "-   x\n"),
        ("1.\tx\n", "1.  x\n"),
        ("1)\tx\n", "1)  x\n"),
        ("10.\tx\n", "10. x\n"),
        ("100.\tx\n", "100.    x\n"),
        ("-\t-\tx\n", "-   -   x\n"),
        ("1.\t-\tx\n", "1.  -   x\n"),
        // Space-tab mixes all reach column four.
        ("- x\n\n \tcont\n", "- x\n\n    cont\n"),
        ("- x\n\n  \tcont\n", "- x\n\n    cont\n"),
        ("- x\n\n   \tcont\n", "- x\n\n    cont\n"),
        ("- x\n\n\t cont\n", "- x\n\n     cont\n"),
        ("- x\n\n\tcont\n", "- x\n\n    cont\n"),
        ("- x\n\t- y\n\t\t- z\n", "- x\n    - y\n        - z\n"),
        // After a blockquote marker.
        (">\tfoo\n", ">   foo\n"),
        ("> \tfoo\n", ">   foo\n"),
        (">\t> foo\n", ">   > foo\n"),
        ("-\t> foo\n", "-   > foo\n"),
        ("> -\tx\n", "> - x\n"),
    ];

    #[test]
    fn leading_tabs_expand_to_the_tab_stop() {
        for (content, expected) in LEADING_ROWS {
            assert_eq!(&fixed(content), expected, "{content:?}");
        }
    }

    #[test]
    fn leading_tabs_ignore_spaces_per_tab() {
        for spaces_per_tab in [1, 2, 3, 8] {
            for (content, expected) in LEADING_ROWS {
                assert_eq!(
                    &fixed_with(spaces_per_tab, false, content),
                    expected,
                    "{content:?} with spaces-per-tab = {spaces_per_tab}"
                );
            }
        }
    }

    #[test]
    fn tabs_after_text_still_use_spaces_per_tab() {
        for (content, width) in [
            ("a\tb\n", 2),
            ("a\tb\n", 4),
            ("a\tb\n", 8),
            ("- a\tb\n", 2),
            ("|a\t|b|\n", 8),
        ] {
            let expected = content.replace('\t', &" ".repeat(width));
            assert_eq!(fixed_with(width, false, content), expected, "{content:?}");
        }
        // The tab after the marker is leading, the one inside the text is not.
        assert_eq!(fixed_with(2, false, "-\tx\ty\n"), "-   x  y\n");
    }

    #[test]
    fn a_leading_tab_message_names_the_width_that_is_written() {
        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(2),
            code_blocks: false,
            ..Default::default()
        });
        let ctx = LintContext::new("- x\n\n\tcont\n", crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Found leading tab, use 4 spaces instead");
        assert_eq!(warnings[0].fix.as_ref().unwrap().replacement, "    ");
    }

    #[test]
    fn a_tab_in_leading_code_whitespace_keeps_the_code_with_code_blocks_on() {
        for spaces_per_tab in [2, 8] {
            for (content, expected) in [
                ("-\t\tx\n", "-       x\n"),
                (">\t\tfoo\n", ">       foo\n"),
                ("para\n\n\tcode\n\t\tmore\n", "para\n\n    code\n        more\n"),
                ("para\n\n \tcode\n", "para\n\n    code\n"),
                ("para\n\n  \tcode\n", "para\n\n    code\n"),
                ("para\n\n\t# not heading\n", "para\n\n    # not heading\n"),
                ("```\n\tcode\n```\n", "```\n    code\n```\n"),
                // A tab line that is not a closing fence stays inside the fence.
                ("```\n\t```\nfoo\n", "```\n    ```\nfoo\n"),
                ("- ```\n\tcode\n  ```\n", "- ```\n    code\n  ```\n"),
                ("> ```\n>\tcode\n> ```\n", "> ```\n>   code\n> ```\n"),
                ("1. x\n\n   ```\n\tcode\n   ```\n", "1. x\n\n   ```\n    code\n   ```\n"),
            ] {
                assert_eq!(
                    fixed_with(spaces_per_tab, true, content),
                    expected,
                    "{content:?} with spaces-per-tab = {spaces_per_tab}"
                );
            }
        }
        // A tab inside a code line is code text and takes the configured width.
        assert_eq!(fixed_with(2, true, "para\n\n    a\tb\n"), "para\n\n    a  b\n");
        assert_eq!(
            fixed_with(8, true, "```\nfoo\tbar\n```\n"),
            "```\nfoo        bar\n```\n"
        );
    }

    #[test]
    fn code_blocks_are_left_alone_when_code_blocks_is_off() {
        for content in ["para\n\n\tcode\n", "```\n\tcode\n```\n", "```\nfoo\t\nbar\n```\n"] {
            assert_eq!(fixed(content), content);
        }
    }

    #[test]
    fn a_trailing_tab_is_removed_rather_than_widened_into_a_hard_break() {
        for (content, expected) in [
            ("foo\t\nbar\n", "foo\nbar\n"),
            ("foo \t\nbar\n", "foo\nbar\n"),
            ("foo  \t\nbar\n", "foo\nbar\n"),
            ("foo\t\t\nbar\n", "foo\nbar\n"),
            ("foo\t \nbar\n", "foo\nbar\n"),
            ("foo\t", "foo"),
            ("# h\t\n\ntext\n", "# h\n\ntext\n"),
            ("- foo\t\n  bar\n", "- foo\n  bar\n"),
            ("> foo\t\n> bar\n", "> foo\n> bar\n"),
            // A backslash before the run would become a hard break once the run is gone,
            // so that tab is reported and left alone.
            ("foo\\\t\nbar\n", "foo\\\t\nbar\n"),
            // An empty `+` or `1.` cannot interrupt a paragraph, so it is text.
            ("foo\n+\t\nbar\n", "foo\n+\nbar\n"),
            ("foo\n1.\t\nbar\n", "foo\n1.\nbar\n"),
            ("-\t\n  x\n", "-\n  x\n"),
            // Two or more trailing spaces were already a hard break; it is kept.
            ("foo\t  \nbar\n", "foo      \nbar\n"),
            // Tabs before the trailing run are still alignment.
            ("a\tb\t\nc\n", "a    b\nc\n"),
        ] {
            assert_eq!(fixed(content), expected, "{content:?}");
        }
    }

    #[test]
    fn code_text_that_looks_like_a_marker_keeps_the_configured_width() {
        for (content, expected) in [
            // Diff lines and shell prompts in a fence are text.
            (
                "```\n-\told\n+\tnew\n>\tquoted\n1.\tx\n```\n",
                "```\n-        old\n+        new\n>        quoted\n1.        x\n```\n",
            ),
            ("para\n\n    a\n    -\tx\n", "para\n\n    a\n    -        x\n"),
            // The first line of an indented block is code text as well, unless a
            // marker on it opens the container the code sits in.
            ("    -\tx\n    -\ty\n", "    -        x\n    -        y\n"),
            ("    >\tx\n    -\ty\n", "    >        x\n    -        y\n"),
            ("    >\tx\n    >\ty\n", "    >        x\n    >        y\n"),
            ("-\t\tx\n\t\ty\n", "-       x\n        y\n"),
            // The containers' own markers still decide the columns.
            ("> ```\n>\t-\tx\n> ```\n", "> ```\n>   -        x\n> ```\n"),
            ("- ```\n  -\tx\n  ```\n", "- ```\n  -        x\n  ```\n"),
            ("> ```\n> >\tx\n> ```\n", "> ```\n> >        x\n> ```\n"),
        ] {
            assert_eq!(fixed_with(8, true, content), expected, "{content:?}");
        }
    }

    fn span_rule(code_blocks: bool) -> MD010NoHardTabs {
        MD010NoHardTabs::from_config_struct(MD010Config {
            code_blocks,
            ..Default::default()
        })
    }

    /// A tab between a code span's backticks is part of its text, so no setting
    /// of the rule may rewrite it. By default it is not reported, as in a fence.
    /// With `code-blocks = true` it is reported, without a fix.
    fn assert_span_tabs_kept(source: &str, flavor: crate::config::MarkdownFlavor) {
        for code_blocks in [false, true] {
            let rule = span_rule(code_blocks);
            let ctx = LintContext::new(source, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            if code_blocks {
                assert!(!warnings.is_empty(), "code-blocks reports the tab: {source:?}");
                for warning in &warnings {
                    assert!(
                        warning.fix.is_none(),
                        "a code span tab has no fix: {source:?} {warning:?}"
                    );
                    assert_eq!(
                        warning.message,
                        "Found tab inside inline code, left as is because it is part of the code text"
                    );
                }
            } else {
                assert!(
                    warnings.is_empty(),
                    "a code span tab is not reported: {source:?} {warnings:?}"
                );
            }
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                source,
                "code-blocks={code_blocks} rewrote a code span: {source:?}"
            );
        }
    }

    #[test]
    fn a_tab_inside_an_inline_code_span_is_kept_in_every_shape_a_span_can_take() {
        for source in [
            // Inside one line.
            "`a\tb`\n",
            "`a\t\tb`\n",
            "text `a\tb` more\n",
            "`a\tb` and `c\td`\n",
            // Ending a line inside a multi-line span: a tab, tabs, mixed with spaces.
            "`a\t\nb`\n",
            "`a\t\t\nb`\n",
            "`a \t\nb`\n",
            "`a\t \nb`\n",
            "``a\t\nb``\n",
            "`a\t\nmid\t\nb`\n",
            "`a\t\nb` and `c\t\nd`\n",
            "`a\t\r\nb`\r\n",
            "`a\tb`",
            // Containers keep their prefix on the continuation line.
            "- `a\t\n  b`\n",
            "1. `a\t\n   b`\n",
            "- x\n  - `a\t\n    b`\n",
            "> `a\t\n> b`\n",
            "> > `a\t\n> > b`\n",
            "- > `a\t\n  > b`\n",
            "> - `a\t\n>   b`\n",
            // Spans inside other inline markup.
            "*`a\t\nb`*\n",
            "[`a\t\nb`](u)\n",
            "<span>`a\t\nb`</span>\n",
            // Math: a backtick inside `$...$` opens no span for rumdl but does for
            // a reader without math, and the text must hold under both.
            "$x$ `a\t\nb`\n",
            "A $x `y$\t\nz` end\n",
        ] {
            assert_span_tabs_kept(source, crate::config::MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn leading_whitespace_of_a_span_continuation_line_is_span_text_unless_proven_otherwise() {
        // A continuation line that matches all its containers has its leading
        // whitespace stripped, but a lazy one keeps it as part of the span, and
        // the rule cannot tell the two apart, so every such tab is left alone.
        for source in [
            "`a\n\tb`\n",
            "- `a\n\tb`\n",
            "- `a\n  \tb`\n",
            "> `a\n>\tb`\n",
            "> `a\n> \tb`\n",
            "- > `a\n  >\tb`\n",
            // Lazy lines: the whitespace is part of the code text.
            "> `a\n\tb`\n",
            "> para\n`a\n\tb`\n",
            "- > para\n  `a\n  \tb`\n",
            "> `a\n    >\tb`\n",
            // A marker-looking token on a continuation line is text, and so is the
            // tab after it.
            "- `a\n  2.\tb`\n",
            "`a\n2.\tb`\n",
            "> `a\n> 2.\tb`\n",
        ] {
            assert_span_tabs_kept(source, crate::config::MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn a_tab_in_a_code_span_inside_a_mkdocs_container_is_kept() {
        // pulldown-cmark reads the indented admonition body as indented code, so
        // the span is only known through the container supplement.
        for source in ["!!! note\n\n    `a\tb`\n", "!!! note\n\n    `a\t\n    b`\n"] {
            assert_span_tabs_kept(source, crate::config::MarkdownFlavor::MkDocs);
        }
        let rule = span_rule(false);
        let source = "!!! note\n\n    `a\t\n    b`\n\nouter\t\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MkDocs, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "!!! note\n\n    `a\t\n    b`\n\nouter\n");
    }

    #[test]
    fn a_code_span_tab_warning_covers_the_run_and_has_no_fix() {
        let rule = span_rule(true);
        let ctx = LintContext::new("`a\t\tb`\n", crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            (warnings[0].line, warnings[0].column, warnings[0].end_column),
            (1, 3, 5)
        );
        assert!(warnings[0].fix.is_none());
    }

    #[test]
    fn tabs_around_a_code_span_are_still_fixed() {
        for (source, expected) in [
            ("`a` x\ty\n", "`a` x    y\n"),
            ("text\t`a` and\t`b`\n", "text    `a` and    `b`\n"),
            // A tab on the line that closes the span, after the closing backtick.
            ("`a\nb`\tx\n", "`a\nb`    x\n"),
            ("`a`\t\nnext\n", "`a`\nnext\n"),
            // Not a code span: an unclosed backtick, a span across a blank line,
            // and raw HTML.
            ("`a\t\nb\n", "`a\nb\n"),
            ("`a\t\n\nb`\n", "`a\n\nb`\n"),
            ("<code>a\t\nb</code>\n", "<code>a\nb</code>\n"),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(span_rule(false).fix(&ctx).unwrap(), expected, "{source:?}");
        }
    }

    #[test]
    fn tabs_outside_a_code_span_are_fixed_while_the_tab_inside_it_is_kept() {
        for (source, expected) in [
            ("a\t`b\tc`\n", "a    `b\tc`\n"),
            ("-\t`a\tb`\n", "-   `a\tb`\n"),
            ("`a\t\nb` tail\t\nnext\n", "`a\t\nb` tail\nnext\n"),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(span_rule(false).fix(&ctx).unwrap(), expected, "{source:?}");
        }
    }

    #[test]
    fn code_blocks_true_fixes_the_text_of_a_fence_but_not_of_a_span_on_the_same_line() {
        // Backticks inside a fence are code text, not a span: the opt-in still
        // rewrites them. Beside a real span on a prose line only the span is kept.
        for (source, expected) in [
            ("```\n`a\tb`\n```\n", "```\n`a    b`\n```\n"),
            ("```\n`a\t\nb`\n```\n", "```\n`a    \nb`\n```\n"),
            ("a\t`b\tc`\n", "a    `b\tc`\n"),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(span_rule(true).fix(&ctx).unwrap(), expected, "{source:?}");
        }
    }

    #[test]
    fn fixing_around_code_spans_twice_changes_nothing_more() {
        for source in ["`a\t\nb` x\t\n", "a\t`b\tc`\t\nd\n", "`a\nb`\tx\n", "- `a\t\n  b`\t\n"] {
            let once = fixed(source);
            assert_eq!(fixed(&once), once, "{source:?}");
        }
    }

    #[test]
    fn a_trailing_tab_after_a_backslash_is_reported_without_a_fix() {
        let rule = MD010NoHardTabs::default();
        let ctx = LintContext::new("foo\\\t\nbar\n", crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].fix.is_none(), "{warnings:?}");
    }

    #[test]
    fn a_trailing_tab_in_code_text_takes_the_configured_width() {
        assert_eq!(fixed_with(2, true, "```\nfoo\t\nbar\n```\n"), "```\nfoo  \nbar\n```\n");
    }

    #[test]
    fn a_trailing_tab_is_one_warning_covering_the_whole_run() {
        let rule = MD010NoHardTabs::default();
        let ctx = LintContext::new("foo  \t\nbar\n", crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let fix = warnings[0].fix.as_ref().unwrap();
        assert_eq!(fix.range, 3..6);
        assert_eq!(fix.replacement, "");
        assert_eq!(warnings[0].message, "Found trailing tab, remove it");
    }

    #[test]
    fn a_whitespace_only_line_keeps_spaces_per_tab() {
        assert_eq!(fixed_with(2, false, "a\n\t\nb\n"), "a\n  \nb\n");
        assert_eq!(fixed_with(8, false, "a\n\t\nb\n"), format!("a\n{}\nb\n", " ".repeat(8)));
    }

    #[test]
    fn tabs_after_non_ascii_text_do_not_disturb_the_columns() {
        assert_eq!(fixed_with(2, false, "-\tcafé\tx\n"), "-   café  x\n");
    }

    #[test]
    fn fixing_twice_changes_nothing_more() {
        for (content, _) in LEADING_ROWS {
            let once = fixed(content);
            assert_eq!(fixed(&once), once, "{content:?}");
        }
        for content in ["foo\t\nbar\n", "foo\\\t\nbar\n", "foo\n+\t\nbar\n", "a\tb\t\nc\n"] {
            let once = fixed(content);
            assert_eq!(fixed(&once), once, "{content:?}");
        }
    }

    #[test]
    fn every_warning_replacement_matches_the_fixed_document() {
        let rule = MD010NoHardTabs::default();
        for (content, expected) in LEADING_ROWS {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let mut result = (*content).to_string();
            let mut warnings = rule.check(&ctx).unwrap();
            warnings.sort_by_key(|w| std::cmp::Reverse(w.fix.as_ref().unwrap().range.start));
            for warning in warnings {
                let fix = warning.fix.unwrap();
                result.replace_range(fix.range, &fix.replacement);
            }
            assert_eq!(&result, expected, "{content:?}");
        }
    }

    #[test]
    fn footnote_label_whitespace_is_the_columns_the_tab_spans() {
        for (content, expected) in [
            ("[^1]:\tNote\n", "[^1]:   Note\n"),
            ("[^ab]:\tNote\n", "[^ab]:  Note\n"),
            ("[^abc]:\tNote\n", "[^abc]: Note\n"),
            ("[^abcd]:\tNote\n", "[^abcd]:    Note\n"),
            ("[^abcde]:\tNote\n", "[^abcde]:   Note\n"),
            ("[^1]:\t\tNote\n", "[^1]:       Note\n"),
            ("[^a b]:\tNote\n", "[^a b]: Note\n"),
            ("[^\u{e9}\u{e9}]:\tNote\n", "[^\u{e9}\u{e9}]:  Note\n"),
            ("> [^1]:\tNote\n", "> [^1]: Note\n"),
            ("[^1]:\t- item\n", "[^1]:   - item\n"),
            ("[^1]:\n\n\tcont\n", "[^1]:\n\n    cont\n"),
        ] {
            for spaces_per_tab in [2, 4, 8] {
                assert_eq!(
                    fixed_with(spaces_per_tab, false, content),
                    expected,
                    "{content:?} at {spaces_per_tab}"
                );
            }
        }
    }

    #[test]
    fn a_tab_that_is_not_after_a_footnote_label_keeps_the_configured_width() {
        for (content, expected) in [
            ("[x]:\tfoo\n", "[x]:  foo\n"),
            ("a [^1]:\tb\n", "a [^1]:  b\n"),
            ("[^1]\tb\n", "[^1]  b\n"),
            ("[^]:\tb\n", "[^]:  b\n"),
            ("[^a[b]:\tb\n", "[^a[b]:  b\n"),
            ("[^1]:x\ty\n", "[^1]:x  y\n"),
            ("[^a\tb]:\tc\n", "[^a  b]:  c\n"),
        ] {
            assert_eq!(fixed_with(2, false, content), expected, "{content:?}");
        }
    }

    #[test]
    fn a_footnote_label_is_not_a_marker_in_code_text() {
        assert_eq!(MD010NoHardTabs::structural_prefix("[^1]:\tx", Some(0)), (0, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix("[^1]:\tx", None), (6, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix("[^1]:x", None), (0, 0));
        assert_eq!(MD010NoHardTabs::structural_prefix("[^1]:", None), (5, 0));
    }
}
