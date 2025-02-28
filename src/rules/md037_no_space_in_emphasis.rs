use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD037NoSpaceInEmphasis;

impl MD037NoSpaceInEmphasis {
    fn find_emphasis_issues(content: &str) -> Vec<(usize, usize, String)> {
        let mut issues = Vec::new();
        let re = Regex::new(r"(\*\s|\s\*|\*{2}\s|\s\*{2}|_\s|\s_|_{2}\s|\s_{2})([^*_]+?)(\*\s|\s\*|\*{2}\s|\s\*{2}|_\s|\s_|_{2}\s|\s_{2})").unwrap();
        
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                let start = cap.get(0).unwrap().start();
                let end = cap.get(0).unwrap().end();
                let text = cap[2].trim().to_string();
                let is_double = cap[1].len() > 2;
                let fixed = if is_double {
                    format!("**{}**", text)
                } else {
                    format!("*{}*", text)
                };
                issues.push((start, end, fixed));
            }
        }
        issues
    }
}

impl Rule for MD037NoSpaceInEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers should be removed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let issues = Self::find_emphasis_issues(line);
            for (start, _, fixed) in issues {
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start + 1,
                    message: "Spaces inside emphasis markers should be removed".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start + 1,
                        replacement: fixed,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        
        for line in content.lines() {
            let mut fixed_line = line.to_string();
            let issues = Self::find_emphasis_issues(line);
            
            for (start, end, fixed) in issues.iter().rev() {
                fixed_line.replace_range(*start..*end, fixed);
            }
            
            result.push_str(&fixed_line);
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 