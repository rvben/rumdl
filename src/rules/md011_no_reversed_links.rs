/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref REVERSED_LINK_REGEX: Regex =
        Regex::new(r"\[([^\]]+)\]\(([^)]+)\)|(\([^)]+\))\[([^\]]+)\]").unwrap();
    static ref REVERSED_LINK_CHECK_REGEX: Regex = Regex::new(r"\(([^)]+)\)\[([^\]]+)\]").unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)").unwrap();
}

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    fn find_reversed_links(content: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        let mut line_start = 0;
        let mut current_line = 1;

        for line in content.lines() {
            for cap in REVERSED_LINK_REGEX.captures_iter(line) {
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
            if CODE_FENCE_REGEX.is_match(line) {
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

        for (line_num, line) in content.lines().enumerate() {
            for cap in REVERSED_LINK_CHECK_REGEX.captures_iter(line) {
                let match_obj = cap.get(0).unwrap();

                // Calculate precise character range for the reversed syntax
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, match_obj.start(), match_obj.len());

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Reversed link syntax".to_string(),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: (0..0), // TODO: Replace with correct byte range if available
                        replacement: format!("[{}]({})", &cap[2], &cap[1]),
                    }),
                });
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_capture_group_order_fix() {
        // This test confirms that the capture group order bug is fixed
        // The regex pattern \(([^)]+)\)\[([^\]]+)\] captures:
        // cap[1] = URL (inside parentheses)
        // cap[2] = text (inside brackets)
        // So (URL)[text] should become [text](URL)

        let rule = MD011NoReversedLinks;

        // Test with reversed link syntax
        let content = "Check out (https://example.com)[this link] for more info.";
        let ctx = LintContext::new(content);

        // This should detect the reversed syntax
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reversed link syntax"));

        // Verify the fix produces correct output
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[this link](https://example.com)");
    }

    #[test]
    fn test_multiple_reversed_links() {
        // Test multiple reversed links in the same content
        let rule = MD011NoReversedLinks;

        let content = "Visit (https://example.com)[Example] and (https://test.com)[Test Site].";
        let ctx = LintContext::new(content);

        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);

        // Verify both fixes are correct
        assert_eq!(result[0].fix.as_ref().unwrap().replacement, "[Example](https://example.com)");
        assert_eq!(result[1].fix.as_ref().unwrap().replacement, "[Test Site](https://test.com)");
    }

    #[test]
    fn test_normal_links_not_flagged() {
        // Test that normal link syntax is not flagged
        let rule = MD011NoReversedLinks;

        let content = "This is a normal [link](https://example.com) and another [link](https://test.com).";
        let ctx = LintContext::new(content);

        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn debug_capture_groups() {
        // Debug test to understand capture group behavior
        let pattern = r"\(([^)]+)\)\[([^\]]+)\]";
        let regex = Regex::new(pattern).unwrap();

        let test_text = "(https://example.com)[Click here]";

        if let Some(cap) = regex.captures(test_text) {
            println!("Full match: {}", &cap[0]);
            println!("cap[1] (first group): {}", &cap[1]);
            println!("cap[2] (second group): {}", &cap[2]);

            // Current fix format
            let current_fix = format!("[{}]({})", &cap[2], &cap[1]);
            println!("Current fix produces: {}", current_fix);

            // Test what the actual rule produces
            let rule = MD011NoReversedLinks;
            let ctx = LintContext::new(test_text);
            let result = rule.check(&ctx).unwrap();
            if !result.is_empty() {
                println!("Rule fix produces: {}", result[0].fix.as_ref().unwrap().replacement);
            }
        }
    }
}
