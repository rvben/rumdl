use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::MarkdownElements;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
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
            rule_name: Some(self.name()),
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

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }
        
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        
        // Check for front matter title if configured
        let mut found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() && structure.has_front_matter {
            if let Some((start, end)) = structure.front_matter_range {
                // Extract front matter content
                let front_matter_content: String = content.lines()
                    .skip(start - 1)  // Convert from 1-indexed to 0-indexed
                    .take(end - start + 1)
                    .collect::<Vec<&str>>()
                    .join("\n");
                    
                // Check if it contains a title field
                found_title_in_front_matter = front_matter_content
                    .lines()
                    .any(|line| line.trim().starts_with(&format!("{}:", self.front_matter_title)));
            }
        }
        
        // Find all level-1 headings using the structure
        let mut target_level_headings = Vec::new();
        
        // Process only heading lines using structure
        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Check if this is a level-1 heading
            if i < structure.heading_levels.len() && structure.heading_levels[i] == self.level {
                // Line is 1-indexed in structure, convert to 0-indexed
                target_level_headings.push(line_num - 1);
            }
        }
        
        // If we already found a title in front matter, all level-1 headings should trigger warnings
        let start_index = if found_title_in_front_matter { 0 } else { 1 };
        
        // If we have any target level headings after accounting for front matter, warn as needed
        if target_level_headings.len() > start_index {
            let lines: Vec<&str> = content.lines().collect();
            
            for &line in &target_level_headings[start_index..] {
                // Skip if out of bounds
                if line >= lines.len() {
                    continue;
                }
                
                let line_content = lines[line];
                let col = line_content.find('#').unwrap_or(0);
                
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Multiple top-level headings (level {}) in the same document",
                        self.level
                    ),
                    line: line + 1, // Convert back to 1-indexed
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
    
    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }
    
    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !content.contains('#')
    }
}

impl DocumentStructureExtensions for MD025SingleTitle {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_with_document_structure() {
        let rule = MD025SingleTitle::default();
        
        // Test with only one level-1 heading
        let content = "# Title\n\n## Section 1\n\n## Section 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());
        
        // Test with multiple level-1 headings
        let content = "# Title 1\n\n## Section 1\n\n# Title 2\n\n## Section 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1); // Should flag the second level-1 heading
        assert_eq!(result[0].line, 5);
        
        // Test with front matter title and a level-1 heading
        let content = "---\ntitle: Document Title\n---\n\n# Main Heading\n\n## Section 1";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1); // Should flag the level-1 heading since there's already a title in front matter
        assert_eq!(result[0].line, 5);
    }
}
