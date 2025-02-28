use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;

/// Rule MD044: Proper names should have the correct capitalization
///
/// This rule is triggered when proper names are not capitalized correctly.
pub struct MD044ProperNames {
    names: HashSet<String>,
    code_blocks_excluded: bool,
}

impl MD044ProperNames {
    pub fn new(names: Vec<String>, code_blocks_excluded: bool) -> Self {
        Self {
            names: names.into_iter().collect(),
            code_blocks_excluded,
        }
    }

    fn is_in_code_block(&self, line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("    ")
    }
}

impl Rule for MD044ProperNames {
    fn name(&self) -> &'static str {
        "MD044"
    }

    fn description(&self) -> &'static str {
        "Proper names should have the correct capitalization"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            // Handle code blocks
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if self.code_blocks_excluded && (in_code_block || self.is_in_code_block(line)) {
                continue;
            }

            for name in &self.names {
                let name_pattern = format!(r"\b{}\b", regex::escape(name));
                let re = Regex::new(&name_pattern).unwrap();

                for cap in re.find_iter(line) {
                    let found_name = &line[cap.start()..cap.end()];
                    if found_name != name {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: cap.start() + 1,
                            message: format!("Proper name '{}' should be '{}'", found_name, name),
                            fix: Some(Fix {
                                line: line_num + 1,
                                column: cap.start() + 1,
                                replacement: name.to_string(),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = content.to_string();
        let mut in_code_block = false;

        for name in &self.names {
            let name_pattern = format!(r"\b{}\b", regex::escape(&name.to_lowercase()));
            let re = Regex::new(&name_pattern).unwrap();

            // Split content into lines to handle code blocks
            let lines: Vec<String> = result
                .lines()
                .map(|line| {
                    if line.trim_start().starts_with("```") {
                        in_code_block = !in_code_block;
                        line.to_string()
                    } else if self.code_blocks_excluded && (in_code_block || self.is_in_code_block(line)) {
                        line.to_string()
                    } else {
                        re.replace_all(line, name).to_string()
                    }
                })
                .collect();

            result = lines.join("\n");
        }

        Ok(result)
    }
} 