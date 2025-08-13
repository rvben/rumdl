/// Rule MD020: No missing space inside closed ATX heading
///
/// See [docs/md020.md](../../docs/md020.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_single_line_range};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Updated patterns to handle optional custom IDs like {#custom-id} after closing hashes
    static ref CLOSED_ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)(#+)([^#\s].*?)([^#\s])(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_START_PATTERN: Regex = Regex::new(r"^(\s*)(#+)([^#\s].*?)\s(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_END_PATTERN: Regex = Regex::new(r"^(\s*)(#+)\s(.*?)([^#\s])(#+)(\s*(?:\{#[^}]+\})?\s*)$").unwrap();
}

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
                if line_info.indent >= 4 {
                    continue;
                }

                // Check all ATX headings (both properly closed and malformed)
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    let line = &line_info.content;

                    // Check if line matches closed ATX pattern without space
                    // This will detect both properly closed headings with missing space
                    // and malformed attempts at closed headings like "# Heading#"
                    if self.is_closed_atx_heading_without_space(line) {
                        let line_index = LineIndex::new(ctx.content.to_string());
                        let line_range = line_index.line_content_range(line_num + 1);

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
                            start_col = opening_hashes.end() + 1;
                            length = 1;
                        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
                            // Missing space at start: #Heading #
                            let opening_hashes = captures.get(2).unwrap();
                            message = format!(
                                "Missing space after {} at start of closed heading",
                                "#".repeat(opening_hashes.as_str().len())
                            );
                            // Highlight the position right after the opening hashes
                            start_col = opening_hashes.end() + 1;
                            length = 1;
                        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
                            // Missing space at end: # Heading#
                            let content = captures.get(3).unwrap();
                            let closing_hashes = captures.get(5).unwrap();
                            message = format!(
                                "Missing space before {} at end of closed heading",
                                "#".repeat(closing_hashes.as_str().len())
                            );
                            // Highlight the position right before the closing hashes
                            start_col = content.end() + 1;
                            length = 1;
                        }

                        let (start_line, start_col_calc, end_line, end_col) =
                            calculate_single_line_range(line_num + 1, start_col, length);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
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

        for line_info in ctx.lines.iter() {
            let mut fixed = false;

            if let Some(heading) = &line_info.heading {
                // Skip headings indented 4+ spaces (they're code blocks)
                if line_info.indent >= 4 {
                    lines.push(line_info.content.clone());
                    continue;
                }

                // Fix ATX headings without space (both properly closed and malformed)
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX)
                    && self.is_closed_atx_heading_without_space(&line_info.content)
                {
                    lines.push(self.fix_closed_atx_heading(&line_info.content));
                    fixed = true;
                }
            }

            if !fixed {
                lines.push(line_info.content.clone());
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
        ctx.content.is_empty() || !ctx.content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with missing spaces
        let content = "# Heading 1#\n## Heading 2 ##\n### Heading 3###";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
}
