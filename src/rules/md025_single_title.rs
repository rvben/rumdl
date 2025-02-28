use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug)]
pub struct MD025SingleTitle {
    level: usize,
    front_matter_title: String,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: "title:".to_string(),
        }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, front_matter_title: &str) -> Self {
        Self {
            level,
            front_matter_title: front_matter_title.to_string(),
        }
    }

    fn has_front_matter_title(&self, content: &str) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() >= 3 && lines[0] == "---" {
            let end_index = lines.iter().skip(1).position(|&line| line == "---");
            if let Some(end_index) = end_index {
                for i in 1..=end_index {
                    if lines[i].trim().starts_with(&self.front_matter_title) {
                        return true;
                    }
                }
            }
        }
        false
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
        let mut found_title = self.has_front_matter_title(content);

        for (i, line) in content.lines().enumerate() {
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
                                replacement: format!("{} {}", "#".repeat(level + 1), &line[level..]),
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
        let mut found_title = self.has_front_matter_title(content);
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
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
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 