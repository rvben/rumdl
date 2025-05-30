/// Rule MD018: No missing space after ATX heading marker
///
/// See [docs/md018.md](../../docs/md018.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_single_line_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"(?m)^(#+)([^#\s].*)").unwrap();
    // New pattern for detecting malformed heading attempts where user intent is clear
    static ref MALFORMED_HEADING_PATTERN: Regex = Regex::new(r"^(#{1,6})([^\s#][^\r\n]*)$").unwrap();
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

    fn is_atx_heading_without_space(&self, line: &str) -> bool {
        ATX_NO_SPACE_PATTERN.is_match(line)
    }

    /// Detect malformed heading attempts where user intent is clear
    /// Returns true if line starts with 1-6 # characters followed immediately by non-whitespace
    /// and the content suggests a heading (not emphasis, not random text)
    fn is_malformed_heading_attempt(&self, line: &str) -> bool {
        if !MALFORMED_HEADING_PATTERN.is_match(line) {
            return false;
        }

        // Extract the content after the hashes
        if let Some(caps) = MALFORMED_HEADING_PATTERN.captures(line) {
            let hashes = caps.get(1).unwrap().as_str();
            let content = caps.get(2).unwrap().as_str();

            // Safety checks to ensure this is likely a heading attempt:

            // 1. Must have 1-6 hashes (valid heading level)
            if hashes.len() > 6 {
                return false;
            }

            // 2. Content should be substantial (not just a single character)
            if content.len() < 2 {
                return false;
            }

            // 3. Content should not look like emphasis or other markdown
            // Avoid false positives for things like: ###
            if content.trim().is_empty() {
                return false;
            }

            // 4. Should not be a line of repeated characters (horizontal rule-like)
            let trimmed_content = content.trim();
            if trimmed_content.len() > 3 && trimmed_content.chars().all(|c| c == trimmed_content.chars().next().unwrap()) {
                return false;
            }

            // 5. Should not start with markdown emphasis markers immediately after hashes
            if trimmed_content.starts_with('*') || trimmed_content.starts_with('_') {
                return false;
            }

            // 6. Should not be a list item that happens to start with #
            if trimmed_content.contains("- ") || trimmed_content.contains("* ") || trimmed_content.contains("+ ") {
                return false;
            }

            // 7. Content should suggest it's meant to be a title/heading
            // Basic heuristic: if it looks like a sentence or title
            let first_char = trimmed_content.chars().next().unwrap();
            if first_char.is_alphabetic() || first_char.is_numeric() {
                return true;
            }
        }

        false
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();

        let content = &line[hashes.end()..];
        format!("{} {}", hashes.as_str(), content)
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
        "No space after hash on ATX style heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content or content without ATX headings
        if content.is_empty() || !content.contains('#') {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Fast path: if no hash symbols, return unchanged
        if content.is_empty() || !content.contains('#') {
            return Ok(content.to_string());
        }

        // Use document structure to identify code blocks and heading lines
        let structure = DocumentStructure::new(content);

        // Create a set of heading line numbers for fast lookup
        let heading_lines: std::collections::HashSet<usize> =
            structure.heading_lines.iter().cloned().collect();

        // Process line by line, checking both proper headings and malformed attempts
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed

            // Check if this is a properly formed heading that needs fixing
            if heading_lines.contains(&line_num) && self.is_atx_heading_without_space(line) {
                result_lines.push(self.fix_atx_heading(line));
            }
            // Check if this is a malformed heading attempt that needs fixing
            else if !heading_lines.contains(&line_num)
                && !structure.is_in_code_block(line_num)
                && !structure.is_in_front_matter(line_num)
                && self.is_malformed_heading_attempt(line) {
                result_lines.push(self.fix_atx_heading(line));
            } else {
                result_lines.push(line.to_string());
            }
        }

        Ok(result_lines.join("\n"))
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let mut warnings = Vec::new();
        let content = _ctx.content;
        let lines: Vec<&str> = content.lines().collect();

        // Part 1: Check properly formed headings that are missing spaces
        // (These are already detected as headings by the document structure)
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if this is an ATX heading without space
            if self.is_atx_heading_without_space(line) {
                let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();
                let hashes = captures.get(1).unwrap();
                let content_start = captures.get(2).unwrap();

                // Calculate precise range: highlight from end of hashes to start of content
                let hash_end_col = hashes.end() + 1; // 1-indexed
                let content_start_col = content_start.start() + 1; // 1-indexed
                let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                    line_num,
                    hash_end_col,
                    content_start_col - hash_end_col,
                );

                let line_range = self.get_line_byte_range(content, line_num);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "No space after {} in ATX style heading",
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_atx_heading(line),
                    }),
                });
            }
        }

        // Part 2: Check for malformed heading attempts that weren't detected by document structure
        // Create a set of already-processed heading line numbers for efficiency
        let heading_lines_set: std::collections::HashSet<usize> =
            structure.heading_lines.iter().cloned().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed

            // Skip if this line was already processed as a proper heading
            if heading_lines_set.contains(&line_num) {
                continue;
            }

            // Skip if we're in a code block or front matter
            if structure.is_in_code_block(line_num) || structure.is_in_front_matter(line_num) {
                continue;
            }

            // Check for malformed heading attempts
            if self.is_malformed_heading_attempt(line) {
                if let Some(caps) = MALFORMED_HEADING_PATTERN.captures(line) {
                    let hashes = caps.get(1).unwrap();

                    // Calculate precise range: highlight the missing space
                    let hash_end_col = hashes.end() + 1; // 1-indexed
                    let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                        line_num,
                        hash_end_col,
                        0, // Zero-width to indicate missing space
                    );

                    let line_range = self.get_line_byte_range(content, line_num);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "No space after {} in ATX style heading",
                            "#".repeat(hashes.as_str().len())
                        ),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_range,
                            replacement: self.fix_atx_heading(line),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD018NoMissingSpaceAtx::new())
    }
}

impl DocumentStructureExtensions for MD018NoMissingSpaceAtx {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD018NoMissingSpaceAtx;

        // Test with correct space
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(result.is_empty());

        // Test with missing space
        let content = "#Heading 1\n## Heading 2\n###Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_malformed_heading_detection() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Should detect clear malformed headings
        assert!(rule.is_malformed_heading_attempt("##Introduction"));
        assert!(rule.is_malformed_heading_attempt("###Background"));
        assert!(rule.is_malformed_heading_attempt("####Details"));
        assert!(rule.is_malformed_heading_attempt("#Summary"));
        assert!(rule.is_malformed_heading_attempt("######Conclusion"));
        assert!(rule.is_malformed_heading_attempt("##Table of Contents"));
        assert!(rule.is_malformed_heading_attempt("##Chapter1"));
        assert!(rule.is_malformed_heading_attempt("###Section3.1"));
        assert!(rule.is_malformed_heading_attempt("##Project Overview"));
        assert!(rule.is_malformed_heading_attempt("#TODO"));
        assert!(rule.is_malformed_heading_attempt("##FAQ"));

        // Should NOT detect ambiguous or non-heading patterns
        assert!(!rule.is_malformed_heading_attempt("###")); // Just hashes
        assert!(!rule.is_malformed_heading_attempt("#")); // Single hash
        assert!(!rule.is_malformed_heading_attempt("##a")); // Too short
        assert!(!rule.is_malformed_heading_attempt("#*emphasis")); // Emphasis marker
        assert!(!rule.is_malformed_heading_attempt("##_test")); // Underscore marker
        assert!(!rule.is_malformed_heading_attempt("#- list")); // List marker
        assert!(!rule.is_malformed_heading_attempt("##################")); // Too many hashes
        assert!(!rule.is_malformed_heading_attempt("#######TooBig")); // More than 6 hashes
        assert!(!rule.is_malformed_heading_attempt("##AAAAAAA")); // Repeated chars
        assert!(!rule.is_malformed_heading_attempt("##------")); // Repeated chars
        assert!(!rule.is_malformed_heading_attempt("#!@#$%")); // Special characters
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
