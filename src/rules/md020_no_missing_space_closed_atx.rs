/// Rule MD020: No missing space inside closed ATX heading
///
/// See [docs/md020.md](../../docs/md020.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;
use regex::Regex;
use std::sync::LazyLock;

// Closed ATX heading patterns
// [^#\s\\] before closing hashes prevents matching escaped hashes like C\# (C-sharp)
static CLOSED_ATX_NO_SPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(#+)([^#\s].*?)([^#\s\\])(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap());
static CLOSED_ATX_NO_SPACE_START_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(#+)([^#\s].*?)\s(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap());
static CLOSED_ATX_NO_SPACE_END_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(#+)\s(.*?)([^#\s\\])(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap());

#[derive(Clone)]
pub struct MD020NoMissingSpaceClosedAtx;

impl Default for MD020NoMissingSpaceClosedAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD020NoMissingSpaceClosedAtx {
    pub fn new() -> Self {
        Self
    }

    /// Whether a line opens with an ATX marker: a heading, or a `#Heading#`
    /// line that is paragraph text only because its opening space is missing.
    fn opens_with_atx_marker(line_info: &crate::lint_context::LineInfo) -> bool {
        line_info.atx_missing_space.is_some()
            || line_info
                .heading
                .as_deref()
                .is_some_and(|heading| matches!(heading.style, crate::lint_context::HeadingStyle::ATX))
    }

    /// Whether a quoted line opens its content with one to six `#` under four
    /// columns of indentation, so it can be an ATX heading rather than quoted code. The
    /// indentation counts from the column after the optional space (or tab)
    /// following the last quote marker, or from `edge` when a list item inside the
    /// quote starts its content further right; tabs advance to multiples of four.
    fn quoted_atx_candidate(line: &str, edge: usize) -> bool {
        let mut column: usize = 0;
        let mut content_start: usize = 0;
        for (pos, c) in line.char_indices() {
            match c {
                ' ' => column += 1,
                '\t' => column += 4 - column % 4,
                '>' => {
                    column += 1;
                    content_start = column + 1;
                }
                '#' => {
                    let hashes = line[pos..].len() - line[pos..].trim_start_matches('#').len();
                    return hashes <= 6 && column.saturating_sub(content_start.max(edge)) < 4;
                }
                _ => return false,
            }
        }
        false
    }

    fn is_closed_atx_heading_without_space(&self, line: &str) -> bool {
        CLOSED_ATX_NO_SPACE_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_START_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_END_PATTERN.is_match(line)
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            let custom_id = &captures[6];
            format!("{indentation}{opening_hashes} {content}{last_char} {closing_hashes}{custom_id}")
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let closing_hashes = &captures[4];
            let custom_id = &captures[5];
            format!("{indentation}{opening_hashes} {content} {closing_hashes}{custom_id}")
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            let custom_id = &captures[6];
            format!("{indentation}{opening_hashes} {content}{last_char} {closing_hashes}{custom_id}")
        } else {
            line.to_string()
        }
    }
}

impl Rule for MD020NoMissingSpaceClosedAtx {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(20))
    }

    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // A quoted setext heading's text lines are heading text, not ATX candidates.
        let quoted_setext_lines: std::collections::HashSet<usize> = ctx
            .headings()
            .filter(|h| h.is_blockquote() && h.is_setext())
            .flat_map(|h| (h.first_line_num() - 1)..h.line_num)
            .collect();

        let edges = crate::utils::blockquote::container_edges(ctx);

        // Check all closed ATX headings from cached info

        let source = ctx
            .candidate_line_indices(crate::lint_context::LineCandidateKind::Hash)
            .iter()
            .map(|&index| (index, &ctx.lines[index]));

        for (line_num, line_info) in source {
            let full_line = line_info.content(ctx.content);
            // Inside a blockquote the heading candidate is the quoted content,
            // and the quote prefix stays in the reported columns and the fix.
            let (prefix_len, marker_offset) = if line_info.blockquote.is_some() {
                if quoted_setext_lines.contains(&line_num)
                    || line_info.in_code_block
                    || line_info.in_html_block
                    || line_info.in_html_comment
                    || line_info.in_mdx_comment
                    || line_info.in_esm_block
                    || line_info.in_pymdown_block
                    || line_info.in_front_matter
                {
                    continue;
                }
                match crate::utils::blockquote::parse_blockquote_prefix(full_line) {
                    Some(parsed)
                        if crate::utils::blockquote::quote_markers_are_real(full_line)
                            && !crate::utils::blockquote::quote_markers_are_paragraph_text(
                                ctx, &edges, line_num, full_line,
                            )
                            && Self::quoted_atx_candidate(full_line, edges[line_num]) =>
                    {
                        let prefix_len = full_line.len() - parsed.content.len();
                        (
                            prefix_len,
                            prefix_len + parsed.content.len() - parsed.content.trim_start().len(),
                        )
                    }
                    _ => continue,
                }
            } else {
                (0, line_info.indent)
            };
            let marker_byte = line_info.byte_offset + marker_offset;
            if ctx.overlaps_template_code(marker_byte, marker_byte + 1)
                || ctx.is_in_html_tag(marker_byte)
                || ctx.is_in_code_span_byte(marker_byte)
                || ctx.is_in_parsed_link(marker_byte)
                || ctx.overlaps_mdx_inline_code(marker_byte, marker_byte + 1)
                || ctx.is_in_jsx_component_tag(marker_byte)
            {
                continue;
            }
            // Check ATX headings, both properly closed and malformed, skipping
            // ones indented 4+ spaces (they're code blocks)
            if prefix_len == 0
                && (!Self::opens_with_atx_marker(line_info)
                    || crate::utils::blockquote::is_indented_past_edge(&edges, line_num, full_line))
            {
                continue;
            }

            let line = &full_line[prefix_len..];
            let start_offset = full_line[..prefix_len].chars().count();

            // Check if line matches closed ATX pattern without space
            // This will detect both properly closed headings with missing space
            // and malformed attempts at closed headings like "# Heading#"
            if self.is_closed_atx_heading_without_space(line) {
                let line_range = ctx.line_content_byte_range(line_num + 1);

                let mut start_col = 1;
                let mut length = 1;
                let mut message = String::new();

                if let Some(captures) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
                    // Missing space at both start and end: #Heading#
                    let opening_hashes = captures.get(2).unwrap();
                    message = format!(
                        "Missing space inside hashes on closed heading (with {} at start and end)",
                        "#".repeat(opening_hashes.as_str().len())
                    );
                    // Highlight the position right after the opening hashes
                    // Convert byte offset to character count for correct Unicode handling
                    start_col = start_offset + line[..opening_hashes.end()].chars().count() + 1;
                    length = 1;
                } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
                    // Missing space at start: #Heading #
                    let opening_hashes = captures.get(2).unwrap();
                    message = format!(
                        "Missing space after {} at start of closed heading",
                        "#".repeat(opening_hashes.as_str().len())
                    );
                    // Highlight the position right after the opening hashes
                    // Convert byte offset to character count for correct Unicode handling
                    start_col = start_offset + line[..opening_hashes.end()].chars().count() + 1;
                    length = 1;
                } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
                    // Missing space at end: # Heading#
                    let content = captures.get(3).unwrap();
                    let closing_hashes = captures.get(5).unwrap();
                    message = format!(
                        "Missing space before {} at end of closed heading",
                        "#".repeat(closing_hashes.as_str().len())
                    );
                    // Highlight the last character before the closing hashes
                    // Convert byte offset to character count for correct Unicode handling
                    start_col = start_offset + line[..content.end()].chars().count() + 1;
                    length = 1;
                }

                let (start_line, start_col_calc, end_line, end_col) =
                    calculate_single_line_range(line_num + 1, start_col, length);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: start_line,
                    column: start_col_calc,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix::new(
                        line_range,
                        format!("{}{}", &full_line[..prefix_len], self.fix_closed_atx_heading(line)),
                    )),
                });
            }
        }

        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_headings()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD020NoMissingSpaceClosedAtx::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD020NoMissingSpaceClosedAtx;

        // Test with correct spacing
        let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with missing spaces
        let content = "# Heading 1#\n## Heading 2 ##\n### Heading 3###";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_multibyte_char_column_position() {
        let rule = MD020NoMissingSpaceClosedAtx;

        // Multi-byte characters before the content should not affect column calculation
        // "Ü" is 2 bytes in UTF-8 but 1 character
        // "##Ünited##" has ## at byte 0-1, content starts at byte 2
        // Column should be 3 (character position), not 3 (byte position) here they match
        // But "##über##" tests that column after ## reflects character count
        let content = "##Ünited##";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // Column should be based on character position, not byte offset
        // "##" is 2 chars, so the position after ## is char position 3
        // The byte offset of .end() for the opening hashes is 2, so start_col = 2 + 1 = 3
        // For ASCII this is the same, but let's verify with a more complex case

        // Content with multi-byte chars BEFORE closing hashes
        // "##Ü test##" - Ü is 2 bytes, test starts at byte 4, char 3
        // Content ends and closing hashes start after "Ü test" = 7 chars / 8 bytes
        let content = "## Ü test##";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // "## Ü test##" - regex group 3 (content) ends at byte 9 (after "Ü tes")
        // line[..9] = "## Ü tes" = 8 characters, so start_col = 8 + 1 = 9
        // Without the fix, byte offset 9 + 1 = 10 (wrong for non-ASCII)
        assert_eq!(
            result[0].column, 9,
            "Column should use character position, not byte offset"
        );
    }
}
