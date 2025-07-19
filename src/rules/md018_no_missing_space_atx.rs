/// Rule MD018: No missing space after ATX heading marker
///
/// See [docs/md018.md](../../docs/md018.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to detect emoji hashtags like #️⃣
    static ref EMOJI_HASHTAG_PATTERN: Regex = Regex::new(r"^#️⃣|^#⃣").unwrap();

    // Pattern to detect Unicode hashtag symbols that shouldn't be treated as headings
    static ref UNICODE_HASHTAG_PATTERN: Regex = Regex::new(r"^#[\u{FE0F}\u{20E3}]").unwrap();
}

#[derive(Clone)]
pub struct MD018NoMissingSpaceAtx;

impl Default for MD018NoMissingSpaceAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD018NoMissingSpaceAtx {
    pub fn new() -> Self {
        Self
    }

    /// Check if an ATX heading line is missing space after the marker
    fn check_atx_heading_line(&self, line: &str) -> Option<(usize, String)> {
        // Look for ATX marker at start of line (with optional indentation)
        let trimmed_line = line.trim_start();
        let indent = line.len() - trimmed_line.len();

        if !trimmed_line.starts_with('#') {
            return None;
        }

        // Skip emoji hashtags and Unicode hashtag patterns
        if EMOJI_HASHTAG_PATTERN.is_match(trimmed_line) || UNICODE_HASHTAG_PATTERN.is_match(trimmed_line) {
            return None;
        }

        // Count the number of hashes
        let hash_count = trimmed_line.chars().take_while(|&c| c == '#').count();
        if hash_count == 0 || hash_count > 6 {
            return None;
        }

        // Check what comes after the hashes
        let after_hashes = &trimmed_line[hash_count..];

        // Skip if what follows the hashes is an emoji modifier or variant selector
        if after_hashes
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, '\u{FE0F}' | '\u{20E3}' | '\u{FE0E}'))
        {
            return None;
        }

        // If there's content immediately after hashes (no space), it needs fixing
        if !after_hashes.is_empty() && !after_hashes.starts_with(' ') && !after_hashes.starts_with('\t') {
            // Additional checks to avoid false positives
            let content = after_hashes.trim();

            // Skip if it's just more hashes (horizontal rule)
            if content.chars().all(|c| c == '#') {
                return None;
            }

            // Skip if content is too short to be meaningful
            if content.len() < 2 {
                return None;
            }

            // Skip if it starts with emphasis markers
            if content.starts_with('*') || content.starts_with('_') {
                return None;
            }

            // Skip if it looks like a hashtag (e.g., #tag, #123)
            // But only skip if it's lowercase or a number to avoid skipping headings like #Summary
            if hash_count == 1 && !content.is_empty() {
                let first_char = content.chars().next();
                if let Some(ch) = first_char {
                    // Skip if it's a lowercase letter or number (common hashtag pattern)
                    // Don't skip uppercase as those are likely headings
                    if (ch.is_lowercase() || ch.is_numeric()) && !content.contains(' ') {
                        return None;
                    }
                }
            }

            // This looks like a malformed heading that needs a space
            let fixed = format!("{}{} {}", " ".repeat(indent), "#".repeat(hash_count), after_hashes);
            return Some((indent + hash_count, fixed));
        }

        None
    }

    // Calculate the byte range for a specific line in the content
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> std::ops::Range<usize> {
        let mut current_line = 1;
        let mut start_byte = 0;

        for (i, c) in content.char_indices() {
            if current_line == line_num && c == '\n' {
                return start_byte..i;
            } else if c == '\n' {
                current_line += 1;
                if current_line == line_num {
                    start_byte = i + 1;
                }
            }
        }

        // If we're looking for the last line and it doesn't end with a newline
        if current_line == line_num {
            return start_byte..content.len();
        }

        // Fallback if line not found (shouldn't happen)
        0..0
    }
}

impl Rule for MD018NoMissingSpaceAtx {
    fn name(&self) -> &'static str {
        "MD018"
    }

    fn description(&self) -> &'static str {
        "No space after hash in heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Check all lines that have ATX headings from cached info
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Only check ATX headings
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    // Check if there's a space after the marker
                    let line = &line_info.content;
                    let trimmed = line.trim_start();

                    // Skip emoji hashtags and Unicode hashtag patterns
                    if EMOJI_HASHTAG_PATTERN.is_match(trimmed) || UNICODE_HASHTAG_PATTERN.is_match(trimmed) {
                        continue;
                    }

                    if trimmed.len() > heading.marker.len() {
                        let after_marker = &trimmed[heading.marker.len()..];
                        if !after_marker.is_empty() && !after_marker.starts_with(' ') && !after_marker.starts_with('\t')
                        {
                            // Missing space after ATX marker
                            let hash_end_col = line_info.indent + heading.marker.len() + 1; // 1-indexed
                            let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                                line_num + 1, // Convert to 1-indexed
                                hash_end_col,
                                0, // Zero-width to indicate missing space
                            );

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                message: format!("No space after {} in heading", "#".repeat(heading.level as usize)),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: self.get_line_byte_range(ctx.content, line_num + 1),
                                    replacement: format!(
                                        "{}{} {}",
                                        " ".repeat(line_info.indent),
                                        heading.marker,
                                        after_marker
                                    ),
                                }),
                            });
                        }
                    }
                }
            } else if !line_info.in_code_block && !line_info.is_blank {
                // Check for malformed headings that weren't detected as proper headings
                if let Some((hash_end_pos, fixed_line)) = self.check_atx_heading_line(&line_info.content) {
                    let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                        line_num + 1,     // Convert to 1-indexed
                        hash_end_pos + 1, // 1-indexed column
                        0,                // Zero-width to indicate missing space
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "No space after hash in heading".to_string(),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: self.get_line_byte_range(ctx.content, line_num + 1),
                            replacement: fixed_line,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut lines = Vec::new();

        for line_info in ctx.lines.iter() {
            let mut fixed = false;

            if let Some(heading) = &line_info.heading {
                // Fix ATX headings missing space
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    let line = &line_info.content;
                    let trimmed = line.trim_start();

                    // Skip emoji hashtags and Unicode hashtag patterns
                    if EMOJI_HASHTAG_PATTERN.is_match(trimmed) || UNICODE_HASHTAG_PATTERN.is_match(trimmed) {
                        continue;
                    }

                    if trimmed.len() > heading.marker.len() {
                        let after_marker = &trimmed[heading.marker.len()..];
                        if !after_marker.is_empty() && !after_marker.starts_with(' ') && !after_marker.starts_with('\t')
                        {
                            // Add space after marker
                            lines.push(format!(
                                "{}{} {}",
                                " ".repeat(line_info.indent),
                                heading.marker,
                                after_marker
                            ));
                            fixed = true;
                        }
                    }
                }
            } else if !line_info.in_code_block && !line_info.is_blank {
                // Fix malformed headings
                if let Some((_, fixed_line)) = self.check_atx_heading_line(&line_info.content) {
                    lines.push(fixed_line);
                    fixed = true;
                }
            }

            if !fixed {
                lines.push(line_info.content.clone());
            }
        }

        // Reconstruct content preserving line endings
        let mut result = lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no lines contain hash symbols
        !ctx.lines.iter().any(|line| line.content.contains('#'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD018NoMissingSpaceAtx::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD018NoMissingSpaceAtx;

        // Test with correct space
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with missing space
        let content = "#Heading 1\n## Heading 2\n###Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_malformed_heading_detection() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Test the check_atx_heading_line method
        assert!(rule.check_atx_heading_line("##Introduction").is_some());
        assert!(rule.check_atx_heading_line("###Background").is_some());
        assert!(rule.check_atx_heading_line("####Details").is_some());
        assert!(rule.check_atx_heading_line("#Summary").is_some());
        assert!(rule.check_atx_heading_line("######Conclusion").is_some());
        assert!(rule.check_atx_heading_line("##Table of Contents").is_some());

        // Should NOT detect these
        assert!(rule.check_atx_heading_line("###").is_none()); // Just hashes
        assert!(rule.check_atx_heading_line("#").is_none()); // Single hash
        assert!(rule.check_atx_heading_line("##a").is_none()); // Too short
        assert!(rule.check_atx_heading_line("#*emphasis").is_none()); // Emphasis marker
        assert!(rule.check_atx_heading_line("#######TooBig").is_none()); // More than 6 hashes
    }

    #[test]
    fn test_malformed_heading_with_context() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Test with full content that includes code blocks
        let content = r#"# Test Document

##Introduction
This should be detected.

    ##CodeBlock
This should NOT be detected (indented code block).

```
##FencedCodeBlock
This should NOT be detected (fenced code block).
```

##Conclusion
This should be detected.
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should detect malformed headings but ignore code blocks
        let detected_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert!(detected_lines.contains(&3)); // ##Introduction
        assert!(detected_lines.contains(&14)); // ##Conclusion (updated line number)
        assert!(!detected_lines.contains(&6)); // ##CodeBlock (should be ignored)
        assert!(!detected_lines.contains(&10)); // ##FencedCodeBlock (should be ignored)
    }

    #[test]
    fn test_malformed_heading_fix() {
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"##Introduction
This is a test.

###Background
More content."#;

        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = r#"## Introduction
This is a test.

### Background
More content."#;

        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_mixed_proper_and_malformed_headings() {
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"# Proper Heading

##Malformed Heading

## Another Proper Heading

###Another Malformed

#### Proper with space
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should only detect the malformed ones
        assert_eq!(result.len(), 2);
        let detected_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert!(detected_lines.contains(&3)); // ##Malformed Heading
        assert!(detected_lines.contains(&7)); // ###Another Malformed
    }
}
