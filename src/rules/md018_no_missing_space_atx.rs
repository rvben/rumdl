/// Rule MD018: No missing space after ATX heading marker
///
/// See [docs/md018.md](../../docs/md018.md) for full documentation, configuration, and examples.
use crate::config::MarkdownFlavor;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;
use crate::utils::regex_cache::get_cached_regex;

// Emoji and Unicode hashtag patterns
const EMOJI_HASHTAG_PATTERN_STR: &str = r"^#️⃣|^#⃣";
const UNICODE_HASHTAG_PATTERN_STR: &str = r"^#[\u{FE0F}\u{20E3}]";

// MagicLink issue/PR reference pattern: #123, #10, etc.
// Matches # followed by one or more digits, then either end of string,
// whitespace, or punctuation (not alphanumeric continuation)
const MAGICLINK_REF_PATTERN_STR: &str = r"^#\d+(?:\s|[^a-zA-Z0-9]|$)";

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

    /// Check if a line is a MagicLink-style issue/PR reference (e.g., #123, #10)
    /// Used by MkDocs flavor to skip PyMdown MagicLink patterns
    fn is_magiclink_ref(line: &str) -> bool {
        get_cached_regex(MAGICLINK_REF_PATTERN_STR).is_ok_and(|re| re.is_match(line.trim_start()))
    }

    /// Check if an ATX heading line is missing space after the marker
    fn check_atx_heading_line(&self, line: &str, flavor: MarkdownFlavor) -> Option<(usize, String)> {
        // Look for ATX marker at start of line (with optional indentation)
        let trimmed_line = line.trim_start();
        let indent = line.len() - trimmed_line.len();

        if !trimmed_line.starts_with('#') {
            return None;
        }

        // Only flag patterns at column 1 (no indentation) to match markdownlint behavior
        // Indented patterns are likely:
        // - Multi-line link continuations (e.g., "  #sig-contribex](url)")
        // - List item content
        // - Other continuation contexts
        if indent > 0 {
            return None;
        }

        // Skip emoji hashtags and Unicode hashtag patterns
        let is_emoji = get_cached_regex(EMOJI_HASHTAG_PATTERN_STR)
            .map(|re| re.is_match(trimmed_line))
            .unwrap_or(false);
        let is_unicode = get_cached_regex(UNICODE_HASHTAG_PATTERN_STR)
            .map(|re| re.is_match(trimmed_line))
            .unwrap_or(false);
        if is_emoji || is_unicode {
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

            // MkDocs flavor: skip MagicLink-style issue/PR refs (#123, #10, etc.)
            // MagicLink only uses single #, so check hash_count == 1
            if flavor == MarkdownFlavor::MkDocs && hash_count == 1 && Self::is_magiclink_ref(line) {
                return None;
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
            // Skip lines inside HTML blocks or HTML comments (e.g., CSS selectors like #id)
            if line_info.in_html_block || line_info.in_html_comment {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                // Only check ATX headings
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    // Skip indented headings to match markdownlint behavior
                    // Markdownlint only flags patterns at column 1
                    if line_info.indent > 0 {
                        continue;
                    }

                    // Check if there's a space after the marker
                    let line = line_info.content(ctx.content);
                    let trimmed = line.trim_start();

                    // Skip emoji hashtags and Unicode hashtag patterns
                    let is_emoji = get_cached_regex(EMOJI_HASHTAG_PATTERN_STR)
                        .map(|re| re.is_match(trimmed))
                        .unwrap_or(false);
                    let is_unicode = get_cached_regex(UNICODE_HASHTAG_PATTERN_STR)
                        .map(|re| re.is_match(trimmed))
                        .unwrap_or(false);
                    if is_emoji || is_unicode {
                        continue;
                    }

                    // MkDocs flavor: skip MagicLink-style issue/PR refs (#123, #10, etc.)
                    if ctx.flavor == MarkdownFlavor::MkDocs && heading.level == 1 && Self::is_magiclink_ref(line) {
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
                                rule_name: Some(self.name().to_string()),
                                message: format!("No space after {} in heading", "#".repeat(heading.level as usize)),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: self.get_line_byte_range(ctx.content, line_num + 1),
                                    replacement: {
                                        // Preserve original indentation (including tabs)
                                        let line = line_info.content(ctx.content);
                                        let original_indent = &line[..line_info.indent];
                                        format!("{original_indent}{} {after_marker}", heading.marker)
                                    },
                                }),
                            });
                        }
                    }
                }
            } else if !line_info.in_code_block
                && !line_info.in_front_matter
                && !line_info.in_html_comment
                && !line_info.is_blank
            {
                // Check for malformed headings that weren't detected as proper headings
                if let Some((hash_end_pos, fixed_line)) =
                    self.check_atx_heading_line(line_info.content(ctx.content), ctx.flavor)
                {
                    let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                        line_num + 1,     // Convert to 1-indexed
                        hash_end_pos + 1, // 1-indexed column
                        0,                // Zero-width to indicate missing space
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
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
                    let line = line_info.content(ctx.content);
                    let trimmed = line.trim_start();

                    // Skip emoji hashtags and Unicode hashtag patterns
                    let is_emoji = get_cached_regex(EMOJI_HASHTAG_PATTERN_STR)
                        .map(|re| re.is_match(trimmed))
                        .unwrap_or(false);
                    let is_unicode = get_cached_regex(UNICODE_HASHTAG_PATTERN_STR)
                        .map(|re| re.is_match(trimmed))
                        .unwrap_or(false);

                    // MkDocs flavor: skip MagicLink-style issue/PR refs (#123, #10, etc.)
                    let is_magiclink =
                        ctx.flavor == MarkdownFlavor::MkDocs && heading.level == 1 && Self::is_magiclink_ref(line);

                    // Only attempt fix if not a special pattern
                    if !is_emoji && !is_unicode && !is_magiclink && trimmed.len() > heading.marker.len() {
                        let after_marker = &trimmed[heading.marker.len()..];
                        if !after_marker.is_empty() && !after_marker.starts_with(' ') && !after_marker.starts_with('\t')
                        {
                            // Add space after marker, preserving original indentation (including tabs)
                            let line = line_info.content(ctx.content);
                            let original_indent = &line[..line_info.indent];
                            lines.push(format!("{original_indent}{} {after_marker}", heading.marker));
                            fixed = true;
                        }
                    }
                }
            } else if !line_info.in_code_block
                && !line_info.in_front_matter
                && !line_info.in_html_comment
                && !line_info.is_blank
            {
                // Fix malformed headings
                if let Some((_, fixed_line)) = self.check_atx_heading_line(line_info.content(ctx.content), ctx.flavor) {
                    lines.push(fixed_line);
                    fixed = true;
                }
            }

            if !fixed {
                lines.push(line_info.content(ctx.content).to_string());
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
        // Fast path: check if document likely has headings
        !ctx.likely_has_headings()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with missing space
        let content = "#Heading 1\n## Heading 2\n###Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_malformed_heading_detection() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Test the check_atx_heading_line method
        assert!(
            rule.check_atx_heading_line("##Introduction", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("###Background", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("####Details", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("#Summary", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("######Conclusion", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("##Table of Contents", MarkdownFlavor::Standard)
                .is_some()
        );

        // Should NOT detect these
        assert!(rule.check_atx_heading_line("###", MarkdownFlavor::Standard).is_none()); // Just hashes
        assert!(rule.check_atx_heading_line("#", MarkdownFlavor::Standard).is_none()); // Single hash
        assert!(rule.check_atx_heading_line("##a", MarkdownFlavor::Standard).is_none()); // Too short
        assert!(
            rule.check_atx_heading_line("#*emphasis", MarkdownFlavor::Standard)
                .is_none()
        ); // Emphasis marker
        assert!(
            rule.check_atx_heading_line("#######TooBig", MarkdownFlavor::Standard)
                .is_none()
        ); // More than 6 hashes
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only detect the malformed ones
        assert_eq!(result.len(), 2);
        let detected_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert!(detected_lines.contains(&3)); // ##Malformed Heading
        assert!(detected_lines.contains(&7)); // ###Another Malformed
    }

    #[test]
    fn test_css_selectors_in_html_blocks() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Test CSS selectors inside <style> tags should not trigger MD018
        // This is a common pattern in Quarto/RMarkdown files
        let content = r#"# Proper Heading

<style>
#slide-1 ol li {
    margin-top: 0;
}

#special-slide ol li {
    margin-top: 2em;
}
</style>

## Another Heading
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not detect CSS selectors as malformed headings
        assert_eq!(
            result.len(),
            0,
            "CSS selectors in <style> blocks should not be flagged as malformed headings"
        );
    }

    #[test]
    fn test_js_code_in_script_blocks() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Test that patterns like #element in <script> tags don't trigger MD018
        let content = r#"# Heading

<script>
const element = document.querySelector('#main-content');
#another-comment
</script>

## Another Heading
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not detect JS code as malformed headings
        assert_eq!(
            result.len(),
            0,
            "JavaScript code in <script> blocks should not be flagged as malformed headings"
        );
    }

    #[test]
    fn test_all_malformed_headings_detected() {
        let rule = MD018NoMissingSpaceAtx::new();

        // All patterns at line start should be detected as malformed headings
        // (matching markdownlint behavior)

        // Lowercase single-hash - should be detected
        assert!(
            rule.check_atx_heading_line("#hello", MarkdownFlavor::Standard)
                .is_some(),
            "#hello SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#tag", MarkdownFlavor::Standard).is_some(),
            "#tag SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#hashtag", MarkdownFlavor::Standard)
                .is_some(),
            "#hashtag SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#javascript", MarkdownFlavor::Standard)
                .is_some(),
            "#javascript SHOULD be detected as malformed heading"
        );

        // Numeric patterns - should be detected (could be headings like "# 123")
        assert!(
            rule.check_atx_heading_line("#123", MarkdownFlavor::Standard).is_some(),
            "#123 SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#12345", MarkdownFlavor::Standard)
                .is_some(),
            "#12345 SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#29039)", MarkdownFlavor::Standard)
                .is_some(),
            "#29039) SHOULD be detected as malformed heading"
        );

        // Uppercase single-hash - should be detected
        assert!(
            rule.check_atx_heading_line("#Summary", MarkdownFlavor::Standard)
                .is_some(),
            "#Summary SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#Introduction", MarkdownFlavor::Standard)
                .is_some(),
            "#Introduction SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#API", MarkdownFlavor::Standard).is_some(),
            "#API SHOULD be detected as malformed heading"
        );

        // Multi-hash patterns - should be detected
        assert!(
            rule.check_atx_heading_line("##introduction", MarkdownFlavor::Standard)
                .is_some(),
            "##introduction SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("###section", MarkdownFlavor::Standard)
                .is_some(),
            "###section SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("###fer", MarkdownFlavor::Standard)
                .is_some(),
            "###fer SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("##123", MarkdownFlavor::Standard).is_some(),
            "##123 SHOULD be detected as malformed heading"
        );
    }

    #[test]
    fn test_patterns_that_should_not_be_flagged() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Just hashes (horizontal rule or empty)
        assert!(rule.check_atx_heading_line("###", MarkdownFlavor::Standard).is_none());
        assert!(rule.check_atx_heading_line("#", MarkdownFlavor::Standard).is_none());

        // Content too short
        assert!(rule.check_atx_heading_line("##a", MarkdownFlavor::Standard).is_none());

        // Emphasis markers
        assert!(
            rule.check_atx_heading_line("#*emphasis", MarkdownFlavor::Standard)
                .is_none()
        );

        // More than 6 hashes
        assert!(
            rule.check_atx_heading_line("#######TooBig", MarkdownFlavor::Standard)
                .is_none()
        );

        // Proper headings with space
        assert!(
            rule.check_atx_heading_line("# Hello", MarkdownFlavor::Standard)
                .is_none()
        );
        assert!(
            rule.check_atx_heading_line("## World", MarkdownFlavor::Standard)
                .is_none()
        );
        assert!(
            rule.check_atx_heading_line("### Section", MarkdownFlavor::Standard)
                .is_none()
        );
    }

    #[test]
    fn test_inline_issue_refs_not_at_line_start() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Inline patterns (not at line start) are not checked by check_atx_heading_line
        // because that function only checks lines that START with #

        // These should return None because they don't start with #
        assert!(
            rule.check_atx_heading_line("See issue #123", MarkdownFlavor::Standard)
                .is_none()
        );
        assert!(
            rule.check_atx_heading_line("Check #trending on Twitter", MarkdownFlavor::Standard)
                .is_none()
        );
        assert!(
            rule.check_atx_heading_line("- fix: issue #29039", MarkdownFlavor::Standard)
                .is_none()
        );
    }

    #[test]
    fn test_lowercase_patterns_full_check() {
        // Integration test: verify lowercase patterns are flagged through full check() flow
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#hello\n\n#world\n\n#tag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "All three lowercase patterns should be flagged");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
        assert_eq!(result[2].line, 5);
    }

    #[test]
    fn test_numeric_patterns_full_check() {
        // Integration test: verify numeric patterns are flagged through full check() flow
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#123\n\n#456\n\n#29039";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "All three numeric patterns should be flagged");
    }

    #[test]
    fn test_fix_lowercase_patterns() {
        // Verify fix() correctly handles lowercase patterns
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#hello\nSome text.\n\n#world";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "# hello\nSome text.\n\n# world";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_numeric_patterns() {
        // Verify fix() correctly handles numeric patterns
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#123\nContent.\n\n##456";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "# 123\nContent.\n\n## 456";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_indented_malformed_headings() {
        // Indented patterns are skipped to match markdownlint behavior.
        // Markdownlint only flags patterns at column 1 (no indentation).
        // Indented patterns are often multi-line link continuations or list content.
        let rule = MD018NoMissingSpaceAtx::new();

        // Indented patterns should NOT be flagged (matches markdownlint)
        assert!(
            rule.check_atx_heading_line(" #hello", MarkdownFlavor::Standard)
                .is_none(),
            "1-space indented #hello should be skipped"
        );
        assert!(
            rule.check_atx_heading_line("  #hello", MarkdownFlavor::Standard)
                .is_none(),
            "2-space indented #hello should be skipped"
        );
        assert!(
            rule.check_atx_heading_line("   #hello", MarkdownFlavor::Standard)
                .is_none(),
            "3-space indented #hello should be skipped"
        );

        // 4+ spaces is a code block, not checked by this function
        // (code block detection happens at LintContext level)

        // BUT patterns at column 1 (no indentation) ARE flagged
        assert!(
            rule.check_atx_heading_line("#hello", MarkdownFlavor::Standard)
                .is_some(),
            "Non-indented #hello should be detected"
        );
    }

    #[test]
    fn test_tab_after_hash_is_valid() {
        // Tab after hash is valid (acts like space)
        let rule = MD018NoMissingSpaceAtx::new();

        assert!(
            rule.check_atx_heading_line("#\tHello", MarkdownFlavor::Standard)
                .is_none(),
            "Tab after # should be valid"
        );
        assert!(
            rule.check_atx_heading_line("##\tWorld", MarkdownFlavor::Standard)
                .is_none(),
            "Tab after ## should be valid"
        );
    }

    #[test]
    fn test_mixed_case_patterns() {
        let rule = MD018NoMissingSpaceAtx::new();

        // All should be detected regardless of case
        assert!(
            rule.check_atx_heading_line("#hELLO", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("#Hello", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("#HELLO", MarkdownFlavor::Standard)
                .is_some()
        );
        assert!(
            rule.check_atx_heading_line("#hello", MarkdownFlavor::Standard)
                .is_some()
        );
    }

    #[test]
    fn test_unicode_lowercase() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Unicode lowercase should be detected
        assert!(
            rule.check_atx_heading_line("#über", MarkdownFlavor::Standard).is_some(),
            "Unicode lowercase #über should be detected"
        );
        assert!(
            rule.check_atx_heading_line("#café", MarkdownFlavor::Standard).is_some(),
            "Unicode lowercase #café should be detected"
        );
        assert!(
            rule.check_atx_heading_line("#日本語", MarkdownFlavor::Standard)
                .is_some(),
            "Japanese #日本語 should be detected"
        );
    }

    #[test]
    fn test_matches_markdownlint_behavior() {
        // Comprehensive test matching markdownlint's expected behavior
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"#hello

## world

###fer

#123

#Tag
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // markdownlint flags: #hello (line 1), ###fer (line 5), #123 (line 7), #Tag (line 9)
        // ## world is correct (has space)
        let flagged_lines: Vec<usize> = result.iter().map(|w| w.line).collect();

        assert!(flagged_lines.contains(&1), "#hello should be flagged");
        assert!(!flagged_lines.contains(&3), "## world should NOT be flagged");
        assert!(flagged_lines.contains(&5), "###fer should be flagged");
        assert!(flagged_lines.contains(&7), "#123 should be flagged");
        assert!(flagged_lines.contains(&9), "#Tag should be flagged");

        assert_eq!(result.len(), 4, "Should have exactly 4 warnings");
    }

    #[test]
    fn test_skip_frontmatter_yaml_comments() {
        // YAML comments in frontmatter should NOT be flagged as missing space in headings
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"---
#reviewers:
#- sig-api-machinery
#another_comment: value
title: Test Document
---

# Valid heading

#invalid heading without space
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag line 10 (#invalid heading without space)
        // Lines 2-4 are YAML comments in frontmatter and should be skipped
        assert_eq!(
            result.len(),
            1,
            "Should only flag the malformed heading outside frontmatter"
        );
        assert_eq!(result[0].line, 10, "Should flag line 10");
    }

    #[test]
    fn test_skip_html_comments() {
        // Content inside HTML comments should NOT be flagged
        // This includes Jupyter cell markers like #%% in commented-out code blocks
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"# Real Heading

Some text.

<!--
```
#%% Cell marker
import matplotlib.pyplot as plt

#%% Another cell
data = [1, 2, 3]
```
-->

More content.
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should find no issues - the #%% markers are inside HTML comments
        assert!(
            result.is_empty(),
            "Should not flag content inside HTML comments, found {} issues",
            result.len()
        );
    }

    #[test]
    fn test_mkdocs_magiclink_skips_numeric_refs() {
        // MkDocs flavor should skip MagicLink-style issue/PR refs (#123, #10, etc.)
        let rule = MD018NoMissingSpaceAtx::new();

        // These numeric patterns should be SKIPPED in MkDocs flavor
        assert!(
            rule.check_atx_heading_line("#10", MarkdownFlavor::MkDocs).is_none(),
            "#10 should be skipped in MkDocs flavor (MagicLink issue ref)"
        );
        assert!(
            rule.check_atx_heading_line("#123", MarkdownFlavor::MkDocs).is_none(),
            "#123 should be skipped in MkDocs flavor (MagicLink issue ref)"
        );
        assert!(
            rule.check_atx_heading_line("#10 discusses the issue", MarkdownFlavor::MkDocs)
                .is_none(),
            "#10 followed by text should be skipped in MkDocs flavor"
        );
        assert!(
            rule.check_atx_heading_line("#37.", MarkdownFlavor::MkDocs).is_none(),
            "#37 followed by punctuation should be skipped in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_still_flags_non_numeric() {
        // MkDocs flavor should still flag non-numeric patterns
        let rule = MD018NoMissingSpaceAtx::new();

        // Non-numeric patterns should still be flagged even in MkDocs flavor
        assert!(
            rule.check_atx_heading_line("#Summary", MarkdownFlavor::MkDocs)
                .is_some(),
            "#Summary should still be flagged in MkDocs flavor"
        );
        assert!(
            rule.check_atx_heading_line("#hello", MarkdownFlavor::MkDocs).is_some(),
            "#hello should still be flagged in MkDocs flavor"
        );
        assert!(
            rule.check_atx_heading_line("#10abc", MarkdownFlavor::MkDocs).is_some(),
            "#10abc (mixed) should still be flagged in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_only_single_hash() {
        // MagicLink only uses single #, so ##10 should still be flagged
        let rule = MD018NoMissingSpaceAtx::new();

        assert!(
            rule.check_atx_heading_line("##10", MarkdownFlavor::MkDocs).is_some(),
            "##10 should be flagged in MkDocs flavor (only single # is MagicLink)"
        );
        assert!(
            rule.check_atx_heading_line("###123", MarkdownFlavor::MkDocs).is_some(),
            "###123 should be flagged in MkDocs flavor"
        );
    }

    #[test]
    fn test_standard_flavor_flags_numeric_refs() {
        // Standard flavor should still flag numeric patterns (no MagicLink awareness)
        let rule = MD018NoMissingSpaceAtx::new();

        assert!(
            rule.check_atx_heading_line("#10", MarkdownFlavor::Standard).is_some(),
            "#10 should be flagged in Standard flavor"
        );
        assert!(
            rule.check_atx_heading_line("#123", MarkdownFlavor::Standard).is_some(),
            "#123 should be flagged in Standard flavor"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_full_check() {
        // Integration test: verify MkDocs flavor skips MagicLink refs through full check() flow
        let rule = MD018NoMissingSpaceAtx::new();

        let content = r#"# PRs that are helpful for context

#10 discusses the philosophy behind the project, and #37 shows a good example.

#Summary

##Introduction
"#;

        // MkDocs flavor - should skip #10 and #37, but flag #Summary and ##Introduction
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        let flagged_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
        assert!(
            !flagged_lines.contains(&3),
            "#10 should NOT be flagged in MkDocs flavor"
        );
        assert!(
            flagged_lines.contains(&5),
            "#Summary SHOULD be flagged in MkDocs flavor"
        );
        assert!(
            flagged_lines.contains(&7),
            "##Introduction SHOULD be flagged in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_fix_exact_output() {
        // Verify fix() produces exact expected output
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#10 discusses the issue.\n\n#Summary";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Exact expected output: #10 preserved, #Summary fixed
        let expected = "#10 discusses the issue.\n\n# Summary";
        assert_eq!(
            fixed, expected,
            "MkDocs fix should preserve MagicLink refs and fix non-numeric headings"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_edge_cases() {
        // Test various edge cases for MagicLink pattern matching
        let rule = MD018NoMissingSpaceAtx::new();

        // These should all be SKIPPED in MkDocs flavor (valid MagicLink refs)
        // Note: #1 alone is skipped due to content length < 2, not MagicLink
        let valid_refs = [
            "#10",             // Two digits
            "#999999",         // Large number
            "#10 text after",  // Space then text
            "#10\ttext after", // Tab then text
            "#10.",            // Period after
            "#10,",            // Comma after
            "#10!",            // Exclamation after
            "#10?",            // Question mark after
            "#10)",            // Close paren after
            "#10]",            // Close bracket after
            "#10;",            // Semicolon after
            "#10:",            // Colon after
        ];

        for ref_str in valid_refs {
            assert!(
                rule.check_atx_heading_line(ref_str, MarkdownFlavor::MkDocs).is_none(),
                "{ref_str:?} should be skipped as MagicLink ref in MkDocs flavor"
            );
        }

        // These should still be FLAGGED in MkDocs flavor (not valid MagicLink refs)
        let invalid_refs = [
            "#10abc",   // Alphanumeric continuation
            "#10a",     // Single alpha continuation
            "#abc10",   // Alpha prefix
            "#10ABC",   // Uppercase continuation
            "#Summary", // Pure text
            "#hello",   // Lowercase text
        ];

        for ref_str in invalid_refs {
            assert!(
                rule.check_atx_heading_line(ref_str, MarkdownFlavor::MkDocs).is_some(),
                "{ref_str:?} should be flagged in MkDocs flavor (not a valid MagicLink ref)"
            );
        }
    }

    #[test]
    fn test_mkdocs_magiclink_hyphenated_continuation() {
        // Hyphenated patterns like #10-related should still be flagged
        // because they're likely malformed headings, not MagicLink refs
        let rule = MD018NoMissingSpaceAtx::new();

        // Hyphen is not alphanumeric, so #10- would match as MagicLink
        // But #10-related has alphanumeric after the hyphen
        // The regex ^#\d+(?:\s|[^a-zA-Z0-9]|$) would match #10- but not consume -related
        // So #10-related would match (the -r part is after the match)
        assert!(
            rule.check_atx_heading_line("#10-", MarkdownFlavor::MkDocs).is_none(),
            "#10- should be skipped (hyphen is non-alphanumeric terminator)"
        );
    }

    #[test]
    fn test_mkdocs_magiclink_standalone_number() {
        // #10 alone on a line (common in changelogs)
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "See issue:\n\n#10\n\nFor details.";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // #10 alone should not be flagged in MkDocs flavor
        assert!(
            result.is_empty(),
            "Standalone #10 should not be flagged in MkDocs flavor"
        );

        // Verify fix doesn't modify it
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "fix() should not modify standalone MagicLink ref");
    }

    #[test]
    fn test_standard_flavor_flags_all_numeric() {
        // Standard flavor should flag ALL numeric patterns (no MagicLink awareness)
        // Note: #1 is skipped because content length < 2 (existing behavior)
        let rule = MD018NoMissingSpaceAtx::new();

        let numeric_patterns = ["#10", "#123", "#999999", "#10 text"];

        for pattern in numeric_patterns {
            assert!(
                rule.check_atx_heading_line(pattern, MarkdownFlavor::Standard).is_some(),
                "{pattern:?} should be flagged in Standard flavor"
            );
        }

        // #1 is skipped due to content length < 2 rule (not MagicLink related)
        assert!(
            rule.check_atx_heading_line("#1", MarkdownFlavor::Standard).is_none(),
            "#1 should be skipped (content too short, existing behavior)"
        );
    }

    #[test]
    fn test_mkdocs_vs_standard_fix_comparison() {
        // Compare fix output between MkDocs and Standard flavors
        let rule = MD018NoMissingSpaceAtx::new();

        let content = "#10 is an issue\n#Summary";

        // MkDocs: preserves #10, fixes #Summary
        let ctx_mkdocs = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed_mkdocs = rule.fix(&ctx_mkdocs).unwrap();
        assert_eq!(fixed_mkdocs, "#10 is an issue\n# Summary");

        // Standard: fixes both
        let ctx_standard = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed_standard = rule.fix(&ctx_standard).unwrap();
        assert_eq!(fixed_standard, "# 10 is an issue\n# Summary");
    }
}
