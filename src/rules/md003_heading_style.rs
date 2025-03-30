use lazy_static::lazy_static;
use regex::Regex;
use fancy_regex::Regex as FancyRegex;

use crate::{
    rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity},
    rules::heading_utils::{HeadingStyle, HeadingUtils},
    utils::range_utils::LineIndex,
};

lazy_static! {
    static ref HEADING_PATTERN: Regex = Regex::new(r"^(#{1,6})(?:\s+(.+?))?(?:\s+#*)?$").unwrap();
    
    // Improved patterns for heading duplication detection
    // This catches cases like: ## Heading## Heading, ## Heading **Heading**, etc.
    static ref DUPLICATED_HEADING: FancyRegex = FancyRegex::new(
        r"^(#{1,6}\s+)([^#\n.]+?)(?:\s*#{1,6}\s+\2|\s*\*\*\2\*\*|\s*\*\2\*|\s*__\2__|\s*_\2_)"
    ).unwrap();
    
    // This handles duplications with punctuation: ## Heading.## Heading
    static ref DUPLICATED_WITH_PUNCTUATION: FancyRegex = FancyRegex::new(
        r"^(#{1,6}\s+)([^#\n]+?)(?:\.\s*#{1,6}\s+\2|\.\s*\*\*\2\*\*|\.\s*\*\2\*|\.\s*__\2__|\.\s*_\2_)"
    ).unwrap();
    
    // This handles cases where multiple headings are chained together
    static ref CHAINED_HEADINGS: FancyRegex = FancyRegex::new(
        r"^(#{1,6}\s+)([^#\n]+?)(?:(#{1,6})\s+([^#\n]+?))+$"
    ).unwrap();
    
    static ref EMPHASIS_PATTERN: Regex = Regex::new(r"\*\*[^*\n]+\*\*|\*[^*\n]+\*|__[^_\n]+__|_[^_\n]+_").unwrap();
    static ref DUPLICATE_WORDS: FancyRegex = FancyRegex::new(r"(\b\w+\b)(?:\s+\1\b)+").unwrap();
    static ref HEADING_WITH_EMPHASIS: Regex = Regex::new(r"^(#+\s+).*?(\*\*[^*]+\*\*|__[^_]+__)").unwrap();
}

pub struct MD003HeadingStyle {
    style: HeadingStyle,
}

impl Default for MD003HeadingStyle {
    fn default() -> Self {
        Self {
            style: HeadingStyle::Atx,
        }
    }
}

impl Rule for MD003HeadingStyle {
    fn name(&self) -> &'static str {
        "MD003"
    }

    fn description(&self) -> &'static str {
        "Heading style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip if we're in a code block
            if HeadingUtils::is_in_code_block(content, i) {
                continue;
            }

            // Check for various types of duplicated headings
            if let Some(fixed_line) = self.detect_and_fix_duplicated_heading(line) {
                warnings.push(LintWarning {
                    line: i + 1,
                    column: 1,
                    message: "Duplicated heading text detected".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: LineIndex::new(content.to_string())
                            .line_col_to_byte_range(i + 1, 1),
                        replacement: fixed_line,
                    }),
                });
                continue;
            }

            // Check if this line is a heading with inconsistent style
            if let Some(heading) = HeadingUtils::parse_heading(content, i) {
                if heading.style != self.style {
                    let fixed_line = HeadingUtils::convert_heading_style(&heading, &self.style);
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: format!("Heading style should be {}", match self.style {
                            HeadingStyle::Atx => "atx",
                            HeadingStyle::AtxClosed => "atx_closed",
                            HeadingStyle::Setext1 => "setext_1",
                            HeadingStyle::Setext2 => "setext_2",
                        }),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: LineIndex::new(content.to_string())
                                .line_col_to_byte_range(i + 1, 1),
                            replacement: fixed_line,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if HeadingUtils::is_in_code_block(content, i) {
                fixed_lines.push(line.to_string());
                continue;
            }

            // First check for duplicated headings and fix them
            if let Some(fixed_line) = self.detect_and_fix_duplicated_heading(line) {
                fixed_lines.push(fixed_line);
                continue;
            }

            // Then handle style consistency
            if let Some(heading) = HeadingUtils::parse_heading(content, i) {
                if heading.style != self.style {
                    fixed_lines.push(HeadingUtils::convert_heading_style(&heading, &self.style));
                } else {
                    fixed_lines.push(line.to_string());
                }
            } else {
                fixed_lines.push(line.to_string());
            }
        }

        Ok(fixed_lines.join("\n"))
    }
}

impl MD003HeadingStyle {
    /// Detect various types of duplicated headings and fix them
    fn detect_and_fix_duplicated_heading(&self, line: &str) -> Option<String> {
        // Simple duplication: ## Heading## Heading
        if let Ok(Some(caps)) = DUPLICATED_HEADING.captures(line) {
            let prefix = caps.get(1).unwrap().as_str();
            let text = caps.get(2).unwrap().as_str().trim();
            return Some(format!("{}{}", prefix, text));
        }
        
        // Duplication with punctuation: ## Heading.## Heading
        if let Ok(Some(caps)) = DUPLICATED_WITH_PUNCTUATION.captures(line) {
            let prefix = caps.get(1).unwrap().as_str();
            let text = caps.get(2).unwrap().as_str().trim();
            return Some(format!("{}{}", prefix, text));
        }
        
        // Chained headings: ## Heading## Another Heading
        if let Ok(Some(caps)) = CHAINED_HEADINGS.captures(line) {
            let prefix = caps.get(1).unwrap().as_str();
            let text = caps.get(2).unwrap().as_str().trim();
            return Some(format!("{}{}", prefix, text));
        }
        
        // Handle complex cases with a combination of heading markers and emphasis
        if self.has_mixed_heading_markers(line) {
            return Some(self.clean_heading_markers(line));
        }
        
        None
    }
    
    /// Check if a line has mixed heading markers (combination of # and emphasis)
    fn has_mixed_heading_markers(&self, line: &str) -> bool {
        // Count # symbols after the initial heading marker
        if !line.starts_with('#') {
            return false;
        }
        
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return false;
        }
        
        let heading_text = parts[1];
        
        // Check for additional # symbols or emphasis markers
        heading_text.contains('#') || 
            heading_text.contains("**") || 
            heading_text.contains('*') || 
            heading_text.contains("__") || 
            heading_text.contains('_')
    }
    
    /// Clean a heading by removing duplicate markers and emphasis
    fn clean_heading_markers(&self, line: &str) -> String {
        if let Some(caps) = HEADING_PATTERN.captures(line) {
            let level_marker = caps.get(1).unwrap().as_str();
            let mut text = caps.get(2).map_or("", |m| m.as_str()).to_string();
            
            // Remove any emphasis markers - use a different approach to avoid borrowing issues
            let mut cleaned_text = text.clone();
            for pattern in &["**", "*", "__", "_"] {
                // Handle each type of emphasis marker separately
                while cleaned_text.contains(pattern) {
                    let start_pos = cleaned_text.find(pattern).unwrap();
                    let end_pos = cleaned_text[start_pos + pattern.len()..].find(pattern)
                        .map(|pos| start_pos + pattern.len() + pos + pattern.len())
                        .unwrap_or(cleaned_text.len());
                    
                    if end_pos <= start_pos + pattern.len() {
                        // No matching end marker, break to avoid infinite loop
                        break;
                    }
                    
                    // Extract emphasized text without markers
                    if end_pos < cleaned_text.len() {
                        let before = &cleaned_text[0..start_pos];
                        let emphasized = &cleaned_text[start_pos + pattern.len()..end_pos - pattern.len()];
                        let after = &cleaned_text[end_pos..];
                        cleaned_text = format!("{}{}{}", before, emphasized, after);
                    } else {
                        // Reached the end without finding closing marker
                        break;
                    }
                }
            }
            text = cleaned_text;
            
            // Remove any extra # markers
            text = text.replace('#', "");
            
            // Remove duplicate words
            if let Ok(Some(_)) = DUPLICATE_WORDS.captures(&text) {
                let words: Vec<&str> = text.split_whitespace().collect();
                let mut unique_words = Vec::new();
                
                for word in words {
                    if unique_words.is_empty() || unique_words.last().unwrap() != &word {
                        unique_words.push(word);
                    }
                }
                
                text = unique_words.join(" ");
            }
            
            // Clean up extra whitespace
            text = text.trim().to_string();
            
            // Return cleaned heading
            format!("{} {}", level_marker, text)
        } else {
            // If not a valid heading pattern, return unchanged
            line.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atx_heading_style() {
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\nHeading 1\n=======\nHeading 2\n-------\n";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_setext_heading_style() {
        let mut rule = MD003HeadingStyle::default();
        rule.style = HeadingStyle::Setext1;
        let content = "# Heading 1\n## Heading 2\nHeading 1\n=======\nHeading 2\n-------\n";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_fix_duplicated_headings() {
        let rule = MD003HeadingStyle::default();
        
        // Test simple duplication
        let content = "## Heading## Heading";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "## Heading");
        
        // Test duplication with period
        let content = "## Heading.## Heading";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "## Heading");
        
        // Test duplication with emphasis
        let content = "## Heading**Heading**";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "## Heading");
        
        // Test complex duplication
        let content = "## An extremely fast Markdown linter and formatter, written in Rust.## An extremely fast Markdown linter and formatter, written in Rust.**An extremely fast Markdown linter and formatter, written in Rust.**";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "## An extremely fast Markdown linter and formatter, written in Rust");
    }
    
    #[test]
    fn test_heading_duplication_in_code_blocks() {
        let rule = MD003HeadingStyle::default();
        
        // Test that duplication is not fixed in code blocks
        let content = "```\n## Heading## Heading\n```";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "```\n## Heading## Heading\n```");
    }
}
