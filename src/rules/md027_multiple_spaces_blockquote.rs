use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::blockquote_utils::BlockquoteUtils;

/// Rule MD027: No multiple spaces after blockquote symbol
///
/// See [docs/md027.md](../../docs/md027.md) for full documentation, configuration, and examples.

#[derive(Debug, Default, Clone)]
pub struct MD027MultipleSpacesBlockquote;

impl Rule for MD027MultipleSpacesBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after blockquote symbol"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        for (i, &line) in lines.iter().enumerate() {
            if BlockquoteUtils::is_blockquote(line)
                && BlockquoteUtils::has_multiple_spaces_after_marker(line)
            {
                let start_col = BlockquoteUtils::get_blockquote_start_col(line);
                let actual_content = BlockquoteUtils::get_blockquote_content(line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: i + 1,
                    column: start_col,
                    message: "Multiple spaces after blockquote symbol".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(i + 1, start_col),
                        replacement: format!("> {}", actual_content.trim_start()),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::with_capacity(lines.len());

        for line in lines {
            if BlockquoteUtils::is_blockquote(line)
                && BlockquoteUtils::has_multiple_spaces_after_marker(line)
            {
                result.push(BlockquoteUtils::fix_blockquote_spacing(line));
            } else {
                result.push(line.to_string());
            }
        }

        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD027MultipleSpacesBlockquote)
    }
}
