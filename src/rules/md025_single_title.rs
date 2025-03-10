use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::HeadingUtils;
use crate::rules::front_matter_utils::FrontMatterUtils;

#[derive(Debug)]
pub struct MD025SingleTitle {
    level: usize,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
        }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, _front_matter_title: &str) -> Self {
        Self {
            level,
        }
    }
}

impl Rule for MD025SingleTitle {
    fn name(&self) -> &'static str {
        "MD025"
    }

    fn description(&self) -> &'static str {
        "Multiple top-level headings in the same document"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut found_title = false;
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip processing if line is in a code block or front matter
            if HeadingUtils::is_in_code_block(content, i) || FrontMatterUtils::is_in_front_matter(content, i) {
                continue;
            }
            
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count();
                if level == self.level {
                    if found_title {
                        warnings.push(LintWarning {
                            message: format!("Multiple top-level headings (level {}) in the same document", self.level),
                            line: i + 1,
                            column: line.find('#').unwrap_or(0) + 1,
                            fix: Some(Fix {
                                line: i + 1,
                                column: line.find('#').unwrap_or(0) + 1,
                                replacement: format!("{} {}", "#".repeat(level + 1), &line[level + line.find('#').unwrap_or(0)..]),
                            }),
                        });
                    }
                    found_title = true;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut found_title = false;
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip processing if line is in a code block or front matter
            if HeadingUtils::is_in_code_block(&content, i) || FrontMatterUtils::is_in_front_matter(&content, i) {
                result.push_str(line);
            } else {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    let level = trimmed.chars().take_while(|&c| c == '#').count();
                    if level == self.level && found_title {
                        // Increase heading level by 1
                        let spaces = line.chars().take_while(|&c| c.is_whitespace()).count();
                        result.push_str(&" ".repeat(spaces));
                        result.push_str(&"#".repeat(level + 1));
                        result.push_str(&line[spaces + level..]);
                    } else {
                        result.push_str(line);
                    }
                    if level == self.level {
                        found_title = true;
                    }
                } else {
                    result.push_str(line);
                }
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
} 