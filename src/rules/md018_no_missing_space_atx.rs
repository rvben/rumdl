/// Rule MD018: No missing space after ATX heading marker
///
/// See [docs/md018.md](../../docs/md018.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;
use crate::utils::regex_cache::get_cached_regex;

// Emoji and Unicode hashtag patterns
const EMOJI_HASHTAG_PATTERN_STR: &str = r"^#️⃣|^#⃣";
const UNICODE_HASHTAG_PATTERN_STR: &str = r"^#[\u{FE0F}\u{20E3}]";

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
            // Skip lines inside HTML blocks (e.g., CSS selectors like #id)
            if line_info.in_html_block {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                // Only check ATX headings
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
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

                    if trimmed.len() > heading.marker.len() {
                        let after_marker = &trimmed[heading.marker.len()..];
                        if !after_marker.is_empty() && !after_marker.starts_with(' ') && !after_marker.starts_with('\t')
                        {
                            // Skip hashtag-like patterns (e.g., #tag, #123, #29039)
                            // But only for single-hash patterns to avoid skipping ##Heading
                            // This prevents false positives on GitHub issue refs and social hashtags
                            if heading.level == 1 {
                                let content = after_marker.trim();
                                // Get first "word" (up to space, comma, or closing paren)
                                let first_word: String = content
                                    .chars()
                                    .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ')')
                                    .collect();
                                if let Some(first_char) = first_word.chars().next() {
                                    // Skip if first word starts with lowercase or number (hashtag/issue ref)
                                    // Don't skip uppercase as those are likely intended headings
                                    if first_char.is_lowercase() || first_char.is_numeric() {
                                        continue;
                                    }
                                }
                            }

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
            } else if !line_info.in_code_block && !line_info.is_blank {
                // Check for malformed headings that weren't detected as proper headings
                if let Some((hash_end_pos, fixed_line)) = self.check_atx_heading_line(line_info.content(ctx.content)) {
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
                    if is_emoji || is_unicode {
                        continue;
                    }

                    if trimmed.len() > heading.marker.len() {
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
            } else if !line_info.in_code_block && !line_info.is_blank {
                // Fix malformed headings
                if let Some((_, fixed_line)) = self.check_atx_heading_line(line_info.content(ctx.content)) {
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
    fn test_github_issue_refs_and_hashtags_skipped() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Issue refs like #29039 should NOT be detected (starts with number)
        assert!(
            rule.check_atx_heading_line("#29039)").is_none(),
            "#29039) should not be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#123").is_none(),
            "#123 should not be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#12345").is_none(),
            "#12345 should not be detected as malformed heading"
        );

        // Hashtags starting with lowercase should NOT be detected
        assert!(
            rule.check_atx_heading_line("#tag").is_none(),
            "#tag should not be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#hashtag").is_none(),
            "#hashtag should not be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#javascript").is_none(),
            "#javascript should not be detected as malformed heading"
        );

        // Uppercase single-hash SHOULD be detected (likely intended heading)
        assert!(
            rule.check_atx_heading_line("#Summary").is_some(),
            "#Summary SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#Introduction").is_some(),
            "#Introduction SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("#API").is_some(),
            "#API SHOULD be detected as malformed heading"
        );

        // Multi-hash patterns SHOULD always be detected (not social hashtags)
        assert!(
            rule.check_atx_heading_line("##introduction").is_some(),
            "##introduction SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("###section").is_some(),
            "###section SHOULD be detected as malformed heading"
        );
        assert!(
            rule.check_atx_heading_line("##123").is_some(),
            "##123 SHOULD be detected as malformed heading"
        );
    }

    #[test]
    fn test_issue_refs_in_list_continuations() {
        let rule = MD018NoMissingSpaceAtx::new();

        // Real-world example from Deno Releases.md
        // Issue refs in continuation lines should NOT be flagged
        let content = "- fix(compile): temporary fallback\n  #29039)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#29039) in list continuation should not be flagged. Got: {result:?}"
        );

        // Multiple issue refs
        let content = "- fix: issue (#28986, #29005,\n  #29024, #29039)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue refs in list should not be flagged. Got: {result:?}"
        );
    }
}
