/// Rule MD021: No multiple spaces inside closed ATX heading
///
/// See [docs/md021.md](../../docs/md021.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_line_range;
use regex::Regex;
use std::sync::LazyLock;

// Regex patterns
// CommonMark separates ATX markers from the heading text with spaces and tabs only;
// any other Unicode space (U+00A0, U+2003, ...) is part of the text.
const CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR: &str = r"^([ \t]*)(#+)([ \t]+)(.*?)([ \t]+)(#+)[ \t]*$";
static CLOSED_ATX_MULTIPLE_SPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR).unwrap());

#[derive(Clone)]
pub struct MD021NoMultipleSpaceClosedAtx;

impl Default for MD021NoMultipleSpaceClosedAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD021NoMultipleSpaceClosedAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_closed_atx_heading_with_multiple_spaces(&self, line: &str) -> bool {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            start_spaces > 1 || end_spaces > 1
        } else {
            false
        }
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[4];
            let closing_hashes = &captures[6];
            format!(
                "{}{} {} {}",
                indentation,
                opening_hashes,
                content.trim_matches([' ', '\t']),
                closing_hashes
            )
        } else {
            line.to_string()
        }
    }

    fn count_spaces(&self, line: &str) -> (usize, usize) {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            (start_spaces, end_spaces)
        } else {
            (0, 0)
        }
    }
}

impl Rule for MD021NoMultipleSpaceClosedAtx {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(21))
    }

    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        let edges = crate::utils::blockquote::container_edges(ctx);

        // Check all closed ATX headings from cached info, quoted ones included
        for parsed in ctx.headings() {
            let line_num = parsed.line_num - 1;
            let line_info = parsed.line_info;
            let heading = parsed.heading;
            // A quoted heading's `#`s follow the quote prefix, not the line's indent
            let marker_byte = line_info.byte_offset
                + if parsed.is_blockquote() {
                    heading.marker_column
                } else {
                    line_info.indent
                };
            if ctx.overlaps_template_code(marker_byte, marker_byte + 1) {
                continue;
            }
            // Skip headings indented 4+ spaces (they're code blocks); a quoted
            // heading was recognized by the parser and may sit in a wide list item
            if !parsed.is_blockquote()
                && crate::utils::blockquote::is_indented_past_edge(&edges, line_num, line_info.content(ctx.content))
            {
                continue;
            }

            // Only check closed ATX headings
            if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) && heading.has_closing_sequence {
                let full_line = line_info.content(ctx.content);
                // The heading starts after any indentation and, when it
                // opens a list item, after the list marker, which is kept
                let (kept_prefix, line) = full_line.split_at(heading.marker_column);

                // Check if line matches closed ATX pattern with multiple spaces
                if self.is_closed_atx_heading_with_multiple_spaces(line) {
                    let captures = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();
                    let _indentation = captures.get(1).unwrap();
                    let opening_hashes = captures.get(2).unwrap();
                    let (start_spaces, end_spaces) = self.count_spaces(line);

                    let message = if start_spaces > 1 && end_spaces > 1 {
                        format!(
                            "Multiple spaces ({} at start, {} at end) inside hashes on closed heading (with {} at start and end)",
                            start_spaces,
                            end_spaces,
                            "#".repeat(opening_hashes.as_str().len())
                        )
                    } else if start_spaces > 1 {
                        format!(
                            "Multiple spaces ({}) after {} at start of closed heading",
                            start_spaces,
                            "#".repeat(opening_hashes.as_str().len())
                        )
                    } else {
                        format!(
                            "Multiple spaces ({}) before {} at end of closed heading",
                            end_spaces,
                            "#".repeat(opening_hashes.as_str().len())
                        )
                    };

                    // Replace the entire line with the fixed version
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num + 1, full_line);
                    let replacement = format!("{kept_prefix}{}", self.fix_closed_atx_heading(line));

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message,
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(start_line, 1, full_line.len()),
                            replacement,
                        )),
                    });
                }
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
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
        Box::new(MD021NoMultipleSpaceClosedAtx::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD021NoMultipleSpaceClosedAtx;

        // Test with correct spacing
        let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with multiple spaces
        let content = "#  Heading 1 #\n## Heading 2 ##\n### Heading 3  ###";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with multiple spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_unicode_space_in_closed_atx_is_content() {
        let rule = MD021NoMultipleSpaceClosedAtx;
        let src = "#\u{a0}\u{a0}Title\u{a0}\u{a0}#\n";
        let ctx = LintContext::new(src, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), src);
    }

    #[test]
    fn test_ascii_spaces_around_unicode_space_are_still_collapsed() {
        let rule = MD021NoMultipleSpaceClosedAtx;
        let ctx = LintContext::new("#  \u{a0}Title  #\n", crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), "# \u{a0}Title #\n");
    }
}
