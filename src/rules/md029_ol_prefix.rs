
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug)]
pub struct MD029OLPrefix {
    pub style: String,
}

impl Default for MD029OLPrefix {
    fn default() -> Self {
        Self {
            style: "ordered".to_string(),
        }
    }
}

impl MD029OLPrefix {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    fn get_list_number(line: &str) -> Option<usize> {
        let re = Regex::new(r"^\s*(\d+)\.\s").unwrap();
        re.captures(line)
            .and_then(|cap| cap[1].parse().ok())
    }

    fn should_be_ordered(&self, current: usize, index: usize) -> bool {
        match self.style.as_str() {
            "ordered" => current != index + 1,
            "one" => current != 1,
            "zero" => current != 0,
            _ => false,
        }
    }

    fn get_expected_number(&self, index: usize) -> usize {
        match self.style.as_str() {
            "ordered" => index + 1,
            "one" => 1,
            "zero" => 0,
            _ => index + 1,
        }
    }
}

impl Rule for MD029OLPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list item prefix should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut list_index = 0;

        for (line_num, line) in content.lines().enumerate() {
            if let Some(number) = Self::get_list_number(line) {
                if self.should_be_ordered(number, list_index) {
                    let indentation = line.len() - line.trim_start().len();
                    let expected = self.get_expected_number(list_index);
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        line: line_num + 1,
                        column: indentation + 1,
                        message: format!("List item prefix should be {} for style '{}'", expected, self.style),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: indentation + 1,
                            replacement: format!("{}{}.{}", 
                                " ".repeat(indentation),
                                expected,
                                line.trim_start().split('.').skip(1).collect::<String>()
                            ),
                        }),
                    });
                }
                list_index += 1;
            } else if line.trim().is_empty() {
                list_index = 0;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut list_index = 0;

        for line in content.lines() {
            if let Some(_) = Self::get_list_number(line) {
                let indentation = line.len() - line.trim_start().len();
                let expected = self.get_expected_number(list_index);
                let fixed_line = format!("{}{}.{}", 
                    " ".repeat(indentation),
                    expected,
                    line.trim_start().split('.').skip(1).collect::<String>()
                );
                result.push_str(&fixed_line);
                list_index += 1;
            } else {
                result.push_str(line);
                if line.trim().is_empty() {
                    list_index = 0;
                }
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 