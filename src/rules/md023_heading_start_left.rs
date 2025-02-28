use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::HeadingUtils;

#[derive(Debug, Default)]
pub struct MD023HeadingStartLeft;

impl Rule for MD023HeadingStartLeft {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(line, line_num + 1) {
                let indentation = HeadingUtils::get_indentation(line);
                if indentation > 0 {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: format!("Heading should not be indented by {} spaces", indentation),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: HeadingUtils::convert_heading_style(&heading, &heading.style),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();

        for line in content.lines() {
            if let Some(heading) = HeadingUtils::parse_heading(line, 0) {
                let indentation = HeadingUtils::get_indentation(line);
                if indentation > 0 {
                    result.push_str(&HeadingUtils::convert_heading_style(&heading, &heading.style));
                    result.push('\n');
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 