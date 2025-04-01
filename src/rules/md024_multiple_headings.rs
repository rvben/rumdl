use std::collections::HashMap;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::markdown_elements::{MarkdownElements, ElementType};

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
}
