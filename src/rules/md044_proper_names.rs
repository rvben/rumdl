use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref CODE_BLOCK_FENCE: Regex = Regex::new(r"^```").unwrap();
    static ref INDENTED_CODE_BLOCK: Regex = Regex::new(r"^    ").unwrap();
}

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

    // Helper method for checking code blocks
    fn is_code_block(&self, line: &str, in_code_block: bool) -> bool {
        in_code_block || INDENTED_CODE_BLOCK.is_match(line)
    }
    
    // Create a regex-safe version of the name for word boundary matches
    fn create_safe_pattern(&self, name: &str) -> String {
        // Create variations of the name with and without dots
        let variations = vec![
            name.to_lowercase(),
            name.to_lowercase().replace(".", "")
        ];
        
        // Create a pattern that matches any of the variations with word boundaries
        let pattern = variations
            .iter()
            .map(|v| regex::escape(v))
            .collect::<Vec<_>>()
            .join("|");
            
        format!(r"(?i)\b({})\b", pattern)
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
        if content.is_empty() || self.names.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            // Handle code blocks
            if CODE_BLOCK_FENCE.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
                continue;
            }

            if self.code_blocks_excluded && self.is_code_block(line, in_code_block) {
                continue;
            }

            for name in &self.names {
                // Create a pattern that handles variations of the name
                let pattern = self.create_safe_pattern(name);
                let re = Regex::new(&pattern).unwrap();

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
        if content.is_empty() || self.names.is_empty() {
            return Ok(content.to_string());
        }
        
        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines = Vec::with_capacity(lines.len());
        let mut in_code_block = false;

        for line in lines {
            let mut current_line = line.to_string();
            
            // Handle code blocks
            if CODE_BLOCK_FENCE.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
                new_lines.push(current_line);
                continue;
            }

            if self.code_blocks_excluded && self.is_code_block(line, in_code_block) {
                new_lines.push(current_line);
                continue;
            }

            // Apply all name replacements to this line
            for name in &self.names {
                let pattern = self.create_safe_pattern(name);
                let re = Regex::new(&pattern).unwrap();
                current_line = re.replace_all(&current_line, name.as_str()).to_string();
            }
            
            new_lines.push(current_line);
        }

        Ok(new_lines.join("\n"))
    }
} 