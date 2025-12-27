/// Rule MD021: No multiple spaces inside closed ATX heading
///
/// See [docs/md021.md](../../docs/md021.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::regex_cache::get_cached_regex;

// Regex patterns
const CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR: &str = r"^(\s*)(#+)(\s+)(.*?)(\s+)(#+)\s*$";

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
        if let Some(captures) = get_cached_regex(CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line))
        {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            start_spaces > 1 || end_spaces > 1
        } else {
            false
        }
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = get_cached_regex(CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line))
        {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[4];
            let closing_hashes = &captures[6];
            format!(
                "{}{} {} {}",
                indentation,
                opening_hashes,
                content.trim(),
                closing_hashes
            )
        } else {
            line.to_string()
        }
    }

    fn count_spaces(&self, line: &str) -> (usize, usize) {
        if let Some(captures) = get_cached_regex(CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR)
            .ok()
            .and_then(|re| re.captures(line))
        {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            (start_spaces, end_spaces)
        } else {
            (0, 0)
        }
    }
}

impl Rule for MD021NoMultipleSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed heading"
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

                // Only check closed ATX headings
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) && heading.has_closing_sequence {
                    let line = line_info.content(ctx.content);

                    // Check if line matches closed ATX pattern with multiple spaces
                    if self.is_closed_atx_heading_with_multiple_spaces(line) {
                        let captures = get_cached_regex(CLOSED_ATX_MULTIPLE_SPACE_PATTERN_STR)
                            .ok()
                            .and_then(|re| re.captures(line))
                            .unwrap();
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
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num + 1, line);
                        let replacement = self.fix_closed_atx_heading(line);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: ctx
                                    .line_index
                                    .line_col_to_byte_range_with_length(start_line, 1, line.len()),
                                replacement,
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

        for line_info in ctx.lines.iter() {
            let mut fixed = false;

            if let Some(heading) = &line_info.heading {
                // Skip headings indented 4+ spaces (they're code blocks)
                if line_info.visual_indent >= 4 {
                    lines.push(line_info.content(ctx.content).to_string());
                    continue;
                }

                // Fix closed ATX headings with multiple spaces
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX)
                    && heading.has_closing_sequence
                    && self.is_closed_atx_heading_with_multiple_spaces(line_info.content(ctx.content))
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
}
