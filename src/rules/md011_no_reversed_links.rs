/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use regex::Regex;

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    fn find_reversed_links(content: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        let re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)|(\([^)]+\))\[([^\]]+)\]").unwrap();
        let mut line_start = 0;
        let mut current_line = 1;

        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if cap.get(3).is_some() {
                    // Found reversed link syntax (text)[url]
                    let text = cap[3].trim_matches('(').trim_matches(')');
                    let url = &cap[4];
                    let start = line_start + cap.get(0).unwrap().start();
                    results.push((
                        current_line,
                        start - line_start + 1,
                        text.to_string(),
                        url.to_string(),
                    ));
                }
            }
            line_start += line.len() + 1; // +1 for newline
            current_line += 1;
        }

        results
    }

    fn is_in_code_block(&self, content: &str, position: usize) -> bool {
        let mut in_code_block = false;
        let mut current_pos = 0;

        for line in content.lines() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            current_pos += line.len() + 1;
            if current_pos > position {
                break;
            }
        }

        in_code_block
    }
}

impl Rule for MD011NoReversedLinks {
    fn name(&self) -> &'static str {
        "MD011"
    }

    fn description(&self) -> &'static str {
        "Link syntax should not be reversed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let mut line_start = 0;

        for (line_num, line) in content.lines().enumerate() {
            let re = Regex::new(r"\(([^)]+)\)\[([^\]]+)\]").unwrap();
            for cap in re.captures_iter(line) {
                let column = line_start + cap.get(0).unwrap().start() + 1;
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Reversed link syntax".to_string(),
                    line: line_num + 1,
                    column,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: (0..0), // TODO: Replace with correct byte range if available
                        replacement: format!("[{}]({})", &cap[2], &cap[1]),
                    }),
                });
            }
            line_start += line.len() + 1;
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut result = content.to_string();
        let mut offset: usize = 0;

        for (line_num, column, text, url) in Self::find_reversed_links(content) {
            // Calculate absolute position in original content
            let mut pos = 0;
            for (i, line) in content.lines().enumerate() {
                if i + 1 == line_num {
                    pos += column - 1;
                    break;
                }
                pos += line.len() + 1;
            }

            if !self.is_in_code_block(content, pos) {
                let adjusted_pos = pos + offset;
                let original_len = format!("({})[{}]", url, text).len();
                let replacement = format!("[{}]({})", text, url);
                result.replace_range(adjusted_pos..adjusted_pos + original_len, &replacement);
                // Update offset based on the difference in lengths
                if replacement.len() > original_len {
                    offset += replacement.len() - original_len;
                } else {
                    offset = offset.saturating_sub(original_len - replacement.len());
                }
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD011NoReversedLinks)
    }
}
