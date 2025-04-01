use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::MarkdownElements;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern for quick check if content has any headings at all
    static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();
}

#[derive(Debug)]
pub struct MD025SingleTitle {
    level: usize,
    front_matter_title: String,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
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
}

impl Rule for MD025SingleTitle {
    fn name(&self) -> &'static str {
        "MD025"
    }

    fn description(&self) -> &'static str {
        "Multiple top-level headings in the same document"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        // Quick check if there are any headings at all
        if !HEADING_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        
        // Check for front matter title if configured
        let mut found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() {
            if let Some(front_matter) = MarkdownElements::detect_front_matter(content) {
                // Extract front matter content
                let front_matter_content = &content[front_matter.start_line..=front_matter.end_line];
                // Check if it contains a title field
                found_title_in_front_matter = front_matter_content
                    .lines()
                    .any(|line| line.trim().starts_with(&format!("{}:", self.front_matter_title)));
            }
        }
        
        // Use unified heading detection
        let headings = MarkdownElements::detect_headings(content);
        
        // Filter headings to only include those of the target level
        let target_level_headings: Vec<_> = headings.iter()
            .filter(|h| {
                if let Some(level_str) = &h.metadata {
                    if let Ok(level) = level_str.parse::<usize>() {
                        return level == self.level;
                    }
                }
                false
            })
            .collect();
        
        // If we already found a title in front matter, all level-1 headings should trigger warnings
        let start_index = if found_title_in_front_matter { 0 } else { 1 };
        
        // If we have any target level headings after accounting for front matter, warn as needed
        if target_level_headings.len() > start_index {
            for heading in &target_level_headings[start_index..] {
                let line = heading.start_line;
                let line_content = content.lines().nth(line).unwrap_or("");
                let col = line_content.find('#').unwrap_or(0);
                
                warnings.push(LintWarning {
                    message: format!(
                        "Multiple top-level headings (level {}) in the same document",
                        self.level
                    ),
                    line: line + 1,
                    column: col + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line + 1, col + 1),
                        replacement: format!(
                            "{} {}",
                            "#".repeat(self.level + 1),
                            &line_content[(col + self.level)..]
                        ),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Quick check if there are any headings at all
        if !HEADING_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();
        
        // Check for front matter title if configured
        let mut found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() {
            if let Some(front_matter) = MarkdownElements::detect_front_matter(content) {
                // Extract front matter content
                let front_matter_content = &content[front_matter.start_line..=front_matter.end_line];
                // Check if it contains a title field
                found_title_in_front_matter = front_matter_content
                    .lines()
                    .any(|line| line.trim().starts_with(&format!("{}:", self.front_matter_title)));
            }
        }
        
        // Use unified heading detection
        let headings = MarkdownElements::detect_headings(content);
        
        // Filter headings to only include those of the target level
        let target_level_headings: Vec<_> = headings.iter()
            .filter(|h| {
                if let Some(level_str) = &h.metadata {
                    if let Ok(level) = level_str.parse::<usize>() {
                        return level == self.level;
                    }
                }
                false
            })
            .collect();
        
        // If we already found a title in front matter, all level-1 headings should be fixed
        let start_index = if found_title_in_front_matter { 0 } else { 1 };
        
        // No headings to fix or only acceptable number
        if target_level_headings.len() <= start_index {
            return Ok(content.to_string());
        }
        
        // Create a set of line numbers for headings that need to be fixed
        let lines_to_fix: std::collections::HashSet<usize> = target_level_headings[start_index..]
            .iter()
            .map(|h| h.start_line)
            .collect();
        
        // Process each line
        for (i, line) in lines.iter().enumerate() {
            if lines_to_fix.contains(&i) {
                // This is a line that needs fixing
                let col = line.find('#').unwrap_or(0);
                let indentation = &line[..col];
                let content_start = col + self.level;
                
                // Add one more # to increase the heading level
                result.push_str(indentation);
                result.push_str(&"#".repeat(self.level + 1));
                result.push_str(&line[content_start..]);
            } else {
                // Leave this line unchanged
                result.push_str(line);
            }
            
            // Add newline between lines (except after the last line)
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
