/// Rule MD020: No missing space inside closed ATX heading
///
/// See [docs/md020.md](../../docs/md020.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;
use crate::utils::regex_cache::get_cached_fancy_regex;

// Closed ATX heading patterns
// Use negative lookbehind (?<!\\) to avoid matching escaped hashes like C\# (C-sharp)
const CLOSED_ATX_NO_SPACE_PATTERN_STR: &str = r"^(\s*)(#+)([^#\s].*?)([^#\s\\])(?<!\\)(#+)(\s*(?:\{#[^}]+\})?\s*)$";
const CLOSED_ATX_NO_SPACE_START_PATTERN_STR: &str = r"^(\s*)(#+)([^#\s].*?)\s(?<!\\)(#+)(\s*(?:\{#[^}]+\})?\s*)$";
const CLOSED_ATX_NO_SPACE_END_PATTERN_STR: &str = r"^(\s*)(#+)\s(.*?)([^#\s\\])(?<!\\)(#+)(\s*(?:\{#[^}]+\})?\s*)$";

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

    fn is_closed_atx_heading_without_space(&self, line: &str) -> bool {
        get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_PATTERN_STR)
            .map(|re| re.is_match(line).unwrap_or(false))
            .unwrap_or(false)
            || get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_START_PATTERN_STR)
                .map(|re| re.is_match(line).unwrap_or(false))
                .unwrap_or(false)
            || get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_END_PATTERN_STR)
                .map(|re| re.is_match(line).unwrap_or(false))
                .unwrap_or(false)
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line).ok().flatten())
        {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            let custom_id = &captures[6];
            format!("{indentation}{opening_hashes} {content}{last_char} {closing_hashes}{custom_id}")
        } else if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_START_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line).ok().flatten())
        {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let closing_hashes = &captures[4];
            let custom_id = &captures[5];
            format!("{indentation}{opening_hashes} {content} {closing_hashes}{custom_id}")
        } else if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_END_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line).ok().flatten())
        {
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
    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Check all closed ATX headings from cached info
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip headings indented 4+ spaces (they're code blocks)
                if line_info.visual_indent >= 4 {
                    continue;
                }

                // Check all ATX headings (both properly closed and malformed)
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    let line = line_info.content(ctx.content);

                    // Check if line matches closed ATX pattern without space
                    // This will detect both properly closed headings with missing space
                    // and malformed attempts at closed headings like "# Heading#"
                    if self.is_closed_atx_heading_without_space(line) {
                        let line_range = ctx.line_index.line_content_range(line_num + 1);

                        let mut start_col = 1;
                        let mut length = 1;
                        let mut message = String::new();

                        if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_PATTERN_STR)
                            .ok()
                            .and_then(|re| re.captures(line).ok().flatten())
                        {
                            // Missing space at both start and end: #Heading#
                            let opening_hashes = captures.get(2).unwrap();
                            message = format!(
                                "Missing space inside hashes on closed heading (with {} at start and end)",
                                "#".repeat(opening_hashes.as_str().len())
                            );
                            // Highlight the position right after the opening hashes
                            // Convert byte offset to character count for correct Unicode handling
                            start_col = line[..opening_hashes.end()].chars().count() + 1;
                            length = 1;
                        } else if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_START_PATTERN_STR)
                            .ok()
                            .and_then(|re| re.captures(line).ok().flatten())
                        {
                            // Missing space at start: #Heading #
                            let opening_hashes = captures.get(2).unwrap();
                            message = format!(
                                "Missing space after {} at start of closed heading",
                                "#".repeat(opening_hashes.as_str().len())
                            );
                            // Highlight the position right after the opening hashes
                            // Convert byte offset to character count for correct Unicode handling
                            start_col = line[..opening_hashes.end()].chars().count() + 1;
                            length = 1;
                        } else if let Some(captures) = get_cached_fancy_regex(CLOSED_ATX_NO_SPACE_END_PATTERN_STR)
                            .ok()
                            .and_then(|re| re.captures(line).ok().flatten())
                        {
                            // Missing space at end: # Heading#
                            let content = captures.get(3).unwrap();
                            let closing_hashes = captures.get(5).unwrap();
                            message = format!(
                                "Missing space before {} at end of closed heading",
                                "#".repeat(closing_hashes.as_str().len())
                            );
                            // Highlight the last character before the closing hashes
                            // Convert byte offset to character count for correct Unicode handling
                            start_col = line[..content.end()].chars().count() + 1;
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
                            fix: Some(Fix {
                                range: line_range,
                                replacement: self.fix_closed_atx_heading(line),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut lines = Vec::new();

        for (i, line_info) in ctx.lines.iter().enumerate() {
            let line_num = i + 1;
            // If rule is disabled for this line, keep original
            if ctx.inline_config().is_rule_disabled(self.name(), line_num) {
                lines.push(line_info.content(ctx.content).to_string());
                continue;
            }

            let mut fixed = false;

            if let Some(heading) = &line_info.heading {
                // Skip headings indented 4+ spaces (they're code blocks)
                if line_info.visual_indent >= 4 {
                    lines.push(line_info.content(ctx.content).to_string());
                    continue;
                }

                // Fix ATX headings without space (both properly closed and malformed)
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX)
                    && self.is_closed_atx_heading_without_space(line_info.content(ctx.content))
                {
                    lines.push(self.fix_closed_atx_heading(line_info.content(ctx.content)));
                    fixed = true;
                }
            }

            if !fixed {
                lines.push(line_info.content(ctx.content).to_string());
            }
        }

        // Reconstruct content preserving line endings
        let mut result = lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
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
