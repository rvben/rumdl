use std::collections::HashMap;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{CodeSpan, DocumentStructure, DocumentStructureExtensions};

/// Rule MD038: No space inside code span markers
///
/// See [docs/md038.md](../../docs/md038.md) for full documentation, configuration, and examples.
///
/// MD038: Spaces inside code span elements
///
/// This rule is triggered when there are spaces inside code span elements.
///
/// For example:
///
/// ``` markdown
/// ` some text`
/// `some text `
/// ` some text `
/// ```
///
/// To fix this issue, remove the leading and trailing spaces within the code span markers:
///
/// ``` markdown
/// `some text`
/// ```
#[derive(Debug, Clone)]
pub struct MD038NoSpaceInCode {
    pub enabled: bool,
}

impl Default for MD038NoSpaceInCode {
    fn default() -> Self {
        Self::new()
    }
}

impl MD038NoSpaceInCode {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Extract the actual content between backticks in a code span
    fn extract_code_content<'a>(&self, code_span: &'a CodeSpan) -> &'a str {
        &code_span.content
    }

    /// Check if a code span has leading or trailing spaces and return the original and fixed versions
    fn check_space_issues(&self, code_span: &CodeSpan) -> Option<(String, String)> {
        let code_content = self.extract_code_content(code_span);

        // Check for leading or trailing spaces
        if code_content.starts_with(' ') || code_content.ends_with(' ') {
            // Only fix if there's actual content after trimming
            let trimmed = code_content.trim();
            if !trimmed.is_empty() {
                let original = format!("`{}`", code_content);
                let fixed = format!("`{}`", trimmed);
                return Some((original, fixed));
            }
        }

        None
    }

    /// Check if the document has any code spans
    fn has_code_spans(&self, structure: &DocumentStructure) -> bool {
        !structure.code_spans.is_empty()
    }
}

impl Rule for MD038NoSpaceInCode {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        if !self.enabled {
            return Ok(vec![]);
        }

        // Early return if no code spans possible
        if !content.contains('`') {
            return Ok(vec![]);
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);

        // If no code spans, return early
        if !self.has_code_spans(&structure) {
            return Ok(vec![]);
        }

        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let mut warnings = Vec::new();

        // Get lines for position mapping
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Process code spans directly from document structure
        for code_span in &structure.code_spans {
            if let Some((original, fixed)) = self.check_space_issues(code_span) {
                // Use line and column from the code span
                let line_index = code_span.line - 1; // Adjust to 0-based for array indexing
                                                     // Get the content for debugging but not required for the warning
                let _line_content = if line_index < lines.len() {
                    lines[line_index]
                } else {
                    ""
                };

                warnings.push(LintWarning {
                    message: format!("Spaces inside code span elements: {}", original),
                    line: code_span.line,
                    column: code_span.start_col,
                    end_line: code_span.line,
                    end_column: code_span.end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: (code_span.start_col - 1)..(code_span.end_col),
                        replacement: fixed,
                    }),
                    rule_name: Some(self.name()),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if !self.enabled {
            return Ok(content.to_string());
        }

        // Get warnings to identify what needs to be fixed
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Apply fixes in reverse order to avoid messing up positions
        let mut fixed_content = content.to_string();
        let mut warnings_by_line: HashMap<usize, Vec<LintWarning>> = HashMap::new();

        // Group warnings by line for more efficient processing
        for warning in warnings {
            warnings_by_line
                .entry(warning.line)
                .or_default()
                .push(warning);
        }

        // Process each line with fixes
        for (_, mut line_warnings) in warnings_by_line {
            // Sort warnings by column in reverse order (right to left)
            line_warnings.sort_by(|a, b| b.column.cmp(&a.column));

            for warning in line_warnings {
                if let Some(fix) = warning.fix {
                    // Apply the fix
                    let lines: Vec<&str> = fixed_content.lines().collect();
                    let line_idx = warning.line - 1;
                    if line_idx < lines.len() {
                        let line = lines[line_idx];
                        let start_pos = if warning.column > 0 {
                            warning.column - 1
                        } else {
                            0
                        };
                        let end_pos = fix.range.end;

                        if start_pos <= line.len() && end_pos <= line.len() {
                            let fixed_line = format!(
                                "{}{}{}",
                                &line[..start_pos],
                                fix.replacement,
                                &line[end_pos.min(line.len())..]
                            );

                            // Rebuild the content with the fixed line
                            let mut new_content = String::new();
                            for (i, l) in lines.iter().enumerate() {
                                if i == line_idx {
                                    new_content.push_str(&fixed_line);
                                } else {
                                    new_content.push_str(l);
                                }
                                if i < lines.len() - 1 {
                                    new_content.push('\n');
                                }
                            }

                            fixed_content = new_content;
                        }
                    }
                }
            }
        }

        // Ensure we maintain the final newline if it existed
        if content.ends_with('\n') && !fixed_content.ends_with('\n') {
            fixed_content.push('\n');
        }

        Ok(fixed_content)
    }

    /// Check if content is likely to have code spans
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.content.contains('`')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD038NoSpaceInCode::default())
    }
}

impl DocumentStructureExtensions for MD038NoSpaceInCode {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.code_spans.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md038_valid() {
        let rule = MD038NoSpaceInCode::new();
        let valid_cases = vec![
            "This is `code` in a sentence.",
            "This is a `longer code span` in a sentence.",
            "This is `code with internal spaces` which is fine.",
            "This is`` code with double backticks`` which is also fine.",
            "Code span at `end of line`",
            "`Start of line` code span",
            "Multiple `code spans` in `one line` are fine",
            "Code span with `symbols: !@#$%^&*()`",
            "Empty code span `` is technically valid",
        ];
        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Valid case should not have warnings: {}",
                case
            );
        }
    }

    #[test]
    fn test_md038_invalid() {
        let rule = MD038NoSpaceInCode::new();
        let invalid_cases = vec![
            "This is ` code` with leading space.",
            "This is `code ` with trailing space.",
            "This is ` code ` with both leading and trailing space.",
            "Multiple ` code ` spans with `spaces ` in one line.",
        ];
        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(
                !result.is_empty(),
                "Invalid case should have warnings: {}",
                case
            );
        }
    }

    #[test]
    fn test_md038_fix() {
        let rule = MD038NoSpaceInCode::new();
        let test_cases = vec![
            (
                "This is ` code` with leading space.",
                "This is `code` with leading space.",
            ),
            (
                "This is `code ` with trailing space.",
                "This is `code` with trailing space.",
            ),
            (
                "This is ` code ` with both spaces.",
                "This is `code` with both spaces.",
            ),
            (
                "Multiple ` code ` and `spans ` to fix.",
                "Multiple `code` and `spans` to fix.",
            ),
        ];
        for (input, expected) in test_cases {
            let ctx = crate::lint_context::LintContext::new(input);
            let result = rule.fix(&ctx).unwrap();
            assert_eq!(
                result, expected,
                "Fix did not produce expected output for: {}",
                input
            );
        }
    }

    #[test]
    fn test_check_invalid_leading_space() {
        let rule = MD038NoSpaceInCode::new();
        let input = "This has a ` leading space` in code";
        let ctx = crate::lint_context::LintContext::new(input);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].fix.is_some());
    }
}
