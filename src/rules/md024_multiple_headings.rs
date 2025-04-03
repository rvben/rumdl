use std::collections::HashMap;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::markdown_elements::{MarkdownElements, ElementType};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};

/// A rule that checks for multiple headings with the same content
#[derive(Default)]
pub struct MD024MultipleHeadings {
    allow_different_nesting: bool,
}

impl MD024MultipleHeadings {
    /// Create a new instance with configuration
    pub fn new(allow_different_nesting: bool) -> Self {
        MD024MultipleHeadings {
            allow_different_nesting,
        }
    }

    /// Gets a unique signature for a heading based on its text and level
    fn get_heading_signature(&self, text: &str, level: u32) -> String {
        // If we're allowing different nesting levels, convert to lowercase for case-insensitive comparison
        // Otherwise, preserve case as per the original implementation
        let text = if self.allow_different_nesting {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        // If we're allowing different nesting levels, ignore the level
        let level = if self.allow_different_nesting {
            1
        } else {
            level
        };

        format!("{}:{}", level, text)
    }
}

impl Rule for MD024MultipleHeadings {
    fn name(&self) -> &'static str {
        "MD024"
    }

    fn description(&self) -> &'static str {
        "Multiple headings with the same content"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        
        // Track headings by their signature
        let mut headings = HashMap::new();

        // Detect all headings using the MarkdownElements utility
        let detected_headings = MarkdownElements::detect_headings(content);

        for heading in detected_headings {
            // Skip non-heading elements (shouldn't happen) and empty headings
            if heading.element_type != ElementType::Heading || heading.text.trim().is_empty() {
                continue;
            }

            // Get the heading level from metadata
            if let Some(level_str) = &heading.metadata {
                if let Ok(level) = level_str.parse::<u32>() {
                    let signature = self.get_heading_signature(&heading.text, level);

                    // Check if we've seen this heading before
                    if let Some(first_occurrence) = headings.get(&signature) {
                        warnings.push(LintWarning {
            rule_name: Some(self.name()),
                            line: heading.start_line + 1,  // Convert 0-indexed to 1-indexed
                            column: 1,
                            message: format!("Multiple headings with the same content (first occurrence at line {})", first_occurrence),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    } else {
                        // First occurrence
                        headings.insert(signature, heading.start_line + 1);  // Convert to 1-indexed
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // No automatic fix for multiple headings with the same content
        // The user needs to decide how to make each heading unique
        Ok(content.to_string())
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings or only one heading
        if structure.heading_lines.len() <= 1 {
            return Ok(Vec::new());
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Track headings by their signature
        let mut headings = HashMap::new();
        
        // Process only heading lines using structure.heading_lines and structure.heading_levels
        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Get the line index (0-based)
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed
            
            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }
            
            // Get the heading text
            let line = lines[line_idx];
            
            // Extract the text content (strip hashes and whitespace for ATX headings)
            let text = if line.trim().starts_with('#') {
                // This is an ATX heading
                let mut chars = line.trim().chars();
                // Skip the hash symbols at the beginning
                while chars.next() == Some('#') {}
                chars.as_str().trim()
            } else if i + 1 < structure.heading_lines.len() && line_idx + 1 < lines.len() {
                // This could be a setext heading - check if the next line has = or -
                let next_line = lines[line_idx + 1];
                if next_line.trim().starts_with('=') || next_line.trim().starts_with('-') {
                    line.trim()
                } else {
                    continue; // Not a heading we can process
                }
            } else {
                continue; // Not a heading we can process
            };
            
            // Get the heading level
            let level = if i < structure.heading_levels.len() {
                structure.heading_levels[i] as u32
            } else {
                // Fallback if the structure doesn't have the level
                1
            };
            
            // Get the signature
            let signature = self.get_heading_signature(text, level);
            
            // Check if we've seen this heading before
            if let Some(first_occurrence) = headings.get(&signature) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,  // Already 1-indexed from structure
                    column: 1,
                    message: format!("Multiple headings with the same content (first occurrence at line {})", first_occurrence),
                    severity: Severity::Warning,
                    fix: None,
                });
            } else {
                // First occurrence
                headings.insert(signature, line_num);  // Already 1-indexed
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

impl DocumentStructureExtensions for MD024MultipleHeadings {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are at least two headings
        doc_structure.heading_lines.len() > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_heading_signature() {
        let rule_without_nesting = MD024MultipleHeadings::new(false);
        let rule_with_nesting = MD024MultipleHeadings::new(true);
        
        // Test without allowing different nesting levels - should preserve case
        assert_eq!(
            rule_without_nesting.get_heading_signature("Test Heading", 2),
            "2:Test Heading"
        );
        
        // Test with allowing different nesting levels - should convert to lowercase
        assert_eq!(
            rule_with_nesting.get_heading_signature("Test Heading", 2),
            "1:test heading"  // Level is normalized to 1
        );
        
        // Test that the same heading at different levels produces different signatures
        let heading = "Same Heading";
        let sig1 = rule_without_nesting.get_heading_signature(heading, 1);
        let sig2 = rule_without_nesting.get_heading_signature(heading, 2);
        assert_ne!(sig1, sig2);
        
        // Test that with allow_different_nesting, levels are ignored
        let sig3 = rule_with_nesting.get_heading_signature(heading, 1);
        let sig4 = rule_with_nesting.get_heading_signature(heading, 2);
        assert_eq!(sig3, sig4);
    }

    #[test]
    fn test_with_document_structure() {
        let rule = MD024MultipleHeadings::default();
        
        // Test with unique headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());
        
        // Test with duplicate headings
        let content = "# Heading\n## Subheading\n# Heading";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1); // Should flag the duplicate heading
        assert_eq!(result[0].line, 3);
        
        // Test with allow_different_nesting=true
        let rule_with_nesting = MD024MultipleHeadings::new(true);
        
        // Duplicate headings at different levels should be flagged
        let content = "# Heading\n## Heading\n### Heading";
        let structure = DocumentStructure::new(content);
        let result = rule_with_nesting.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag both duplicate headings
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }
}
