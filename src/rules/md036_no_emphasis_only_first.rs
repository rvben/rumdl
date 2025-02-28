use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD036NoEmphasisOnlyFirst;

impl MD036NoEmphasisOnlyFirst {
    fn is_emphasis_only_first_word(line: &str) -> bool {
        let line = line.trim();
        
        // Check for *emphasis* or **strong** patterns at start
        let re_asterisk = Regex::new(r"^\*{1,2}[^*\n]+\*{1,2}\s+\S.*$").unwrap();
        if re_asterisk.is_match(line) {
            return true;
        }

        // Check for _emphasis_ or __strong__ patterns at start
        let re_underscore = Regex::new(r"^_{1,2}[^_\n]+_{1,2}\s+\S.*$").unwrap();
        if re_underscore.is_match(line) {
            return true;
        }

        false
    }

    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;
        for (i, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            if i + 1 == line_num {
                break;
            }
        }
        in_code_block
    }

    fn fix_emphasis_only_first(line: &str) -> String {
        let line = line.trim();
        
        // Fix *emphasis* pattern
        let re_asterisk = Regex::new(r"^\*{1,2}([^*\n]+)\*{1,2}(\s+\S.*)$").unwrap();
        if let Some(caps) = re_asterisk.captures(line) {
            return format!("{}{}", caps.get(1).unwrap().as_str().trim(), caps.get(2).unwrap().as_str());
        }

        // Fix _emphasis_ pattern
        let re_underscore = Regex::new(r"^_{1,2}([^_\n]+)_{1,2}(\s+\S.*)$").unwrap();
        if let Some(caps) = re_underscore.captures(line) {
            return format!("{}{}", caps.get(1).unwrap().as_str().trim(), caps.get(2).unwrap().as_str());
        }

        line.to_string()
    }
}

impl Rule for MD036NoEmphasisOnlyFirst {
    fn name(&self) -> &'static str {
        "MD036"
    }

    fn description(&self) -> &'static str {
        "Emphasis should not be used for the first word only"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, i + 1) && Self::is_emphasis_only_first_word(line) {
                warnings.push(LintWarning {
                    message: "Emphasis should not be used for the first word only".to_string(),
                    line: i + 1,
                    column: 1,
                    fix: Some(Fix {
                        line: i + 1,
                        column: 1,
                        replacement: Self::fix_emphasis_only_first(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for i in 0..lines.len() {
            if !self.is_in_code_block(content, i + 1) && Self::is_emphasis_only_first_word(lines[i]) {
                result.push_str(&Self::fix_emphasis_only_first(lines[i]));
            } else {
                result.push_str(lines[i]);
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 