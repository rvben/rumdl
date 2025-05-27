use crate::utils::range_utils::{LineIndex, calculate_match_range};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::blockquote_utils::BlockquoteUtils;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to match blockquote lines with multiple spaces after >
    static ref BLOCKQUOTE_MULTIPLE_SPACES: Regex = Regex::new(r"^(\s*)>(\s{2,})(.*)$").unwrap();
}

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
                // Find the extra spaces after the > marker
                if let Some(captures) = BLOCKQUOTE_MULTIPLE_SPACES.captures(line) {
                    let indentation = captures.get(1).map_or("", |m| m.as_str());
                    let extra_spaces = captures.get(2).map_or("", |m| m.as_str());

                    // Calculate the position of the extra spaces
                    let marker_pos = indentation.len(); // Position of '>'
                    let extra_spaces_start = marker_pos + 1; // Position after '>'
                    let extra_spaces_len = extra_spaces.len() - 1; // All spaces except the first one (which is correct)

                    let (start_line, start_col, end_line, end_col) = calculate_match_range(
                        i + 1,
                        line,
                        extra_spaces_start + 1, // Skip the first space (which is correct)
                        extra_spaces_len
                    );

                    let actual_content = BlockquoteUtils::get_blockquote_content(line);
                    warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line: end_line,
                    end_column: end_col,
                    message: "Multiple spaces after blockquote symbol".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                    range: _line_index.line_col_to_byte_range(i + 1, indentation.len() + 1),
                    replacement: format!("> {}", actual_content.trim_start()),
                        }),
                    });
                }
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
