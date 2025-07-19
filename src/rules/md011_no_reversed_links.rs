/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref REVERSED_LINK_REGEX: Regex =
        Regex::new(r"\[([^\]]+)\]\(([^)]+)\)|(\([^)]+\))\[([^\]]+)\]").unwrap();
    // Pattern to match reversed links: (URL)[text]
    // The URL pattern allows for nested parentheses using a simple approach
    static ref REVERSED_LINK_CHECK_REGEX: Regex = Regex::new(
        r"\(([^)]*(?:\([^)]*\)[^)]*)*)\)\[([^\]]+)\]"
    ).unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)").unwrap();

    // Pattern to detect escaped brackets and parentheses
    static ref ESCAPED_CHARS: Regex = Regex::new(r"\\[\[\]()]").unwrap();

    // New patterns for detecting malformed link attempts where user intent is clear
    static ref MALFORMED_LINK_PATTERNS: Vec<(Regex, &'static str)> = vec![
        // Missing closing bracket: (URL)[text  or  [text](URL
        (Regex::new(r"\(([^)]+)\)\[([^\]]*$)").unwrap(), "missing closing bracket"),
        (Regex::new(r"\[([^\]]+)\]\(([^)]*$)").unwrap(), "missing closing parenthesis"),

        // Wrong bracket types: {URL}[text] or [text]{URL}
        (Regex::new(r"\{([^}]+)\}\[([^\]]+)\]").unwrap(), "wrong bracket type (curly instead of parentheses)"),
        (Regex::new(r"\[([^\]]+)\]\{([^}]+)\}").unwrap(), "wrong bracket type (curly instead of parentheses)"),

        // URL and text swapped in correct syntax: [URL](text) where URL is clearly a URL
        (Regex::new(r"\[(https?://[^\]]+)\]\(([^)]+)\)").unwrap(), "URL and text appear to be swapped"),
        (Regex::new(r"\[(www\.[^\]]+)\]\(([^)]+)\)").unwrap(), "URL and text appear to be swapped"),
        (Regex::new(r"\[([^\]]*\.[a-z]{2,4}[^\]]*)\]\(([^)]+)\)").unwrap(), "URL and text appear to be swapped"),
    ];
}

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    /// Check if a character at position is escaped (preceded by odd number of backslashes)
    fn is_escaped(content: &str, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }

        let mut backslash_count = 0;
        let mut check_pos = pos - 1;

        loop {
            if content.chars().nth(check_pos) == Some('\\') {
                backslash_count += 1;
                if check_pos == 0 {
                    break;
                }
                check_pos -= 1;
            } else {
                break;
            }
        }

        backslash_count % 2 == 1
    }

    fn find_reversed_links(content: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        let mut line_start = 0;
        let mut current_line = 1;

        for line in content.lines() {
            // Skip processing if we can't possibly have a reversed link
            if !line.contains('(') || !line.contains('[') || !line.contains(']') || !line.contains(')') {
                line_start += line.len() + 1;
                current_line += 1;
                continue;
            }

            for cap in REVERSED_LINK_CHECK_REGEX.captures_iter(line) {
                // Extract URL and text
                let url = &cap[1];
                let text = &cap[2];

                let start = line_start + cap.get(0).unwrap().start();
                results.push((current_line, start - line_start + 1, text.to_string(), url.to_string()));
            }
            line_start += line.len() + 1; // +1 for newline
            current_line += 1;
        }

        results
    }

    /// Detect malformed link attempts where user intent is clear
    fn detect_malformed_link_attempts(&self, line: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        let mut processed_ranges = Vec::new(); // Track processed character ranges to avoid duplicates

        for (pattern, issue_type) in MALFORMED_LINK_PATTERNS.iter() {
            for cap in pattern.captures_iter(line) {
                let match_obj = cap.get(0).unwrap();
                let start = match_obj.start();
                let len = match_obj.len();
                let end = start + len;

                // Skip if this range overlaps with already processed ranges
                if processed_ranges
                    .iter()
                    .any(|(proc_start, proc_end)| (start < *proc_end && end > *proc_start))
                {
                    continue;
                }

                // Extract potential URL and text based on the pattern
                if let Some((url, text)) = self.extract_url_and_text_from_match(&cap, issue_type) {
                    // Only proceed if this looks like a genuine link attempt
                    if self.looks_like_link_attempt(&url, &text) {
                        results.push((start, len, url, text));
                        processed_ranges.push((start, end));
                    }
                }
            }
        }

        results
    }

    /// Extract URL and text from regex match based on the issue type
    fn extract_url_and_text_from_match(&self, cap: &regex::Captures, issue_type: &str) -> Option<(String, String)> {
        match issue_type {
            "missing closing bracket" => {
                // (URL)[text -> cap[1] = URL, cap[2] = incomplete text
                Some((cap[1].to_string(), format!("{}]", &cap[2])))
            }
            "missing closing parenthesis" => {
                // [text](URL -> cap[1] = text, cap[2] = incomplete URL
                Some((format!("{})", &cap[2]), cap[1].to_string()))
            }
            "wrong bracket type (curly instead of parentheses)" => {
                // {URL}[text] or [text]{URL} -> cap[1] and cap[2]
                if cap.get(0).unwrap().as_str().starts_with('{') {
                    // {URL}[text] -> swap and fix brackets
                    Some((cap[1].to_string(), cap[2].to_string()))
                } else {
                    // [text]{URL} -> already in correct order, fix brackets
                    Some((cap[2].to_string(), cap[1].to_string()))
                }
            }
            "URL and text appear to be swapped" => {
                // [URL](text) -> cap[1] = URL, cap[2] = text, need to swap
                Some((cap[1].to_string(), cap[2].to_string()))
            }
            _ => None,
        }
    }

    /// Check if the extracted URL and text look like a genuine link attempt
    fn looks_like_link_attempt(&self, url: &str, text: &str) -> bool {
        // URL should look like a URL
        let url_indicators = [
            "http://", "https://", "www.", "ftp://", ".com", ".org", ".net", ".edu", ".gov", ".io", ".co",
        ];

        let has_url_indicator = url_indicators
            .iter()
            .any(|indicator| url.to_lowercase().contains(indicator));

        // Text should be reasonable length and not look like a URL
        let text_looks_reasonable = text.len() >= 3
            && text.len() <= 50
            && !url_indicators
                .iter()
                .any(|indicator| text.to_lowercase().contains(indicator))
            && !text.to_lowercase().starts_with("http")
            && text.chars().any(|c| c.is_alphabetic()); // Must contain at least one letter

        // URL should not be too short or contain only non-URL characters
        let url_looks_reasonable =
            url.len() >= 4 && (has_url_indicator || url.contains('.')) && !url.chars().all(|c| c.is_alphabetic()); // Shouldn't be just letters

        // Both URL and text should look reasonable for this to be a link attempt
        has_url_indicator && text_looks_reasonable && url_looks_reasonable
    }
}

impl Default for MD011NoReversedLinks {
    fn default() -> Self {
        Self
    }
}

impl Rule for MD011NoReversedLinks {
    fn name(&self) -> &'static str {
        "MD011"
    }

    fn description(&self) -> &'static str {
        "Link syntax should not be reversed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let mut byte_pos = 0;

        for (line_num, line) in content.lines().enumerate() {
            // Skip if this line is in a code block
            if ctx.is_in_code_block_or_span(byte_pos) {
                byte_pos += line.len() + 1;
                continue;
            }

            // Part 1: Check for existing perfectly formed reversed links
            for cap in REVERSED_LINK_CHECK_REGEX.captures_iter(line) {
                let match_obj = cap.get(0).unwrap();
                let match_start = match_obj.start();
                let match_end = match_obj.end();

                // Check if the match contains escaped brackets or parentheses
                let match_text = match_obj.as_str();

                // Skip if the opening parenthesis is escaped
                if match_start > 0 && Self::is_escaped(line, byte_pos + match_start) {
                    continue;
                }

                // Check if any brackets/parentheses within the match are escaped
                let mut skip_match = false;
                for esc_match in ESCAPED_CHARS.find_iter(match_text) {
                    let esc_pos = match_start + esc_match.start();
                    if esc_pos > 0 && line.chars().nth(esc_pos.saturating_sub(1)) == Some('\\') {
                        skip_match = true;
                        break;
                    }
                }

                if skip_match {
                    continue;
                }

                // Manual check for negative lookahead: skip if followed by (url)
                // This prevents false positives like "(text)[ref](url)"
                let remaining = &line[match_end..];
                if remaining.trim_start().starts_with('(') {
                    continue;
                }

                // Extract URL and text
                let url = &cap[1];
                let text = &cap[2];

                // Calculate precise character range for the reversed syntax
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, match_obj.start(), match_obj.len());

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Reversed link syntax: use [{text}]({url}) instead"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            // Calculate proper byte range using line offsets and match position
                            let line_start_byte = ctx.line_offsets.get(line_num).copied().unwrap_or(0);
                            let match_start_byte = line_start_byte + match_obj.start();
                            let match_end_byte = match_start_byte + match_obj.len();
                            match_start_byte..match_end_byte
                        },
                        replacement: format!("[{text}]({url})"),
                    }),
                });
            }

            // Part 2: Check for malformed link attempts where user intent is clear
            let malformed_attempts = self.detect_malformed_link_attempts(line);
            for (start, len, url, text) in malformed_attempts {
                // Calculate precise character range for the malformed syntax
                let (start_line, start_col, end_line, end_col) = calculate_match_range(line_num + 1, line, start, len);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Malformed link syntax".to_string(),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            // Calculate proper byte range using line offsets and match position
                            let line_start_byte = ctx.line_offsets.get(line_num).copied().unwrap_or(0);
                            let match_start_byte = line_start_byte + start;
                            let match_end_byte = match_start_byte + len;
                            match_start_byte..match_end_byte
                        },
                        replacement: format!("[{text}]({url})"),
                    }),
                });
            }

            byte_pos += line.len() + 1; // Update byte position for next line
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut result = content.to_string();
        let mut offset: usize = 0;

        for (line_num, column, text, url) in Self::find_reversed_links(content) {
            // Calculate absolute position in original content
            let mut pos = 0;
            for (i, line) in content.lines().enumerate() {
                if i + 1 == line_num {
                    pos += column - 1;
                    break;
                }
                pos += line.len() + 1;
            }

            if !ctx.is_in_code_block_or_span(pos) {
                let adjusted_pos = pos + offset;
                let original_len = format!("({text})[{url}]").len();
                let replacement = format!("[{text}]({url})");
                result.replace_range(adjusted_pos..adjusted_pos + original_len, &replacement);
                // Update offset based on the difference in lengths
                if replacement.len() > original_len {
                    offset += replacement.len() - original_len;
                } else {
                    offset = offset.saturating_sub(original_len - replacement.len());
                }
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or doesn't have the necessary characters for links
        ctx.content.is_empty() || !ctx.content.contains('(') || !ctx.content.contains('[')
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD011NoReversedLinks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_capture_group_order_fix() {
        // This test confirms that the capture group order bug is fixed
        // The regex pattern \(([^)]+)\)\[([^\]]+)\] captures:
        // cap[1] = URL (inside parentheses)
        // cap[2] = text (inside brackets)
        // So (URL)[text] should become [text](URL)

        let rule = MD011NoReversedLinks;

        // Test with reversed link syntax
        let content = "Check out (https://example.com)[this link] for more info.";
        let ctx = LintContext::new(content);

        // This should detect the reversed syntax
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reversed link syntax"));

        // Verify the fix produces correct output
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[this link](https://example.com)");
    }

    #[test]
    fn test_multiple_reversed_links() {
        // Test multiple reversed links in the same content
        let rule = MD011NoReversedLinks;

        let content = "Visit (https://example.com)[Example] and (https://test.com)[Test Site].";
        let ctx = LintContext::new(content);

        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);

        // Verify both fixes are correct
        assert_eq!(
            result[0].fix.as_ref().unwrap().replacement,
            "[Example](https://example.com)"
        );
        assert_eq!(
            result[1].fix.as_ref().unwrap().replacement,
            "[Test Site](https://test.com)"
        );
    }

    #[test]
    fn test_normal_links_not_flagged() {
        // Test that normal link syntax is not flagged
        let rule = MD011NoReversedLinks;

        let content = "This is a normal [link](https://example.com) and another [link](https://test.com).";
        let ctx = LintContext::new(content);

        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn debug_capture_groups() {
        // Debug test to understand capture group behavior
        let pattern = r"\(([^)]+)\)\[([^\]]+)\]";
        let regex = Regex::new(pattern).unwrap();

        let test_text = "(https://example.com)[Click here]";

        if let Some(cap) = regex.captures(test_text) {
            println!("Full match: {}", &cap[0]);
            println!("cap[1] (first group): {}", &cap[1]);
            println!("cap[2] (second group): {}", &cap[2]);

            // Current fix format
            let current_fix = format!("[{}]({})", &cap[2], &cap[1]);
            println!("Current fix produces: {current_fix}");

            // Test what the actual rule produces
            let rule = MD011NoReversedLinks;
            let ctx = LintContext::new(test_text);
            let result = rule.check(&ctx).unwrap();
            if !result.is_empty() {
                println!("Rule fix produces: {}", result[0].fix.as_ref().unwrap().replacement);
            }
        }
    }

    #[test]
    fn test_malformed_link_detection() {
        let rule = MD011NoReversedLinks;

        // Test wrong bracket types
        let content = "Check out {https://example.com}[this website].";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Malformed link syntax"));

        // Test URL and text swapped
        let content = "Visit [https://example.com](Click Here).";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Malformed link syntax"));

        // Test that valid links are not flagged
        let content = "This is a [normal link](https://example.com).";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);

        // Test that non-links are not flagged
        let content = "Regular text with [brackets] and (parentheses).";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);

        // Test that risky patterns are NOT flagged (conservative approach)
        let content = "(example.com)is a test domain.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);

        let content = "(optional)parameter should not be flagged.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_malformed_link_fixes() {
        let rule = MD011NoReversedLinks;

        // Test wrong bracket types fix
        let content = "Check out {https://example.com}[this website].";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[this website](https://example.com)");

        // Test URL and text swapped fix
        let content = "Visit [https://example.com](Click Here).";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[Click Here](https://example.com)");
    }

    #[test]
    fn test_conservative_detection() {
        let rule = MD011NoReversedLinks;

        // Test that edge cases are not flagged
        let content = "This (not-a-url)text should be ignored.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);

        let content = "Also [regular text](not a url) should be ignored.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);

        let content = "And {not-url}[not-text] should be ignored.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_skip_code_blocks() {
        let rule = MD011NoReversedLinks;

        // Test that patterns inside code blocks are not flagged
        let content = r#"Here's an example:

```rust
// This regex pattern [.!?]+\s*$ should not be flagged
static ref TRAILING_PUNCTUATION: Regex = Regex::new(r"(?m)[.!?]+\s*$").unwrap();
```

But this (https://example.com)[reversed link] should be flagged."#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the reversed link outside the code block
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reversed link syntax"));
        assert_eq!(result[0].line, 8); // The line with the actual reversed link
    }

    #[test]
    fn test_negative_lookahead() {
        let rule = MD011NoReversedLinks;

        // Test that (text)[ref](url) pattern is not flagged
        let content = "This is a reference-style link: (see here)[ref](https://example.com)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should not flag (text)[ref](url) pattern");

        // Test that genuine reversed links are still caught
        let content = "This is reversed: (https://example.com)[click here]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag genuine reversed links");

        // Test with spacing before the second parentheses
        let content = "Reference with space: (text)[ref] (url)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should not flag when space before (url)");
    }

    #[test]
    fn test_escaped_characters() {
        let rule = MD011NoReversedLinks;

        // Test escaped brackets and parentheses
        let content = r"Escaped: \(not a link\)\[also not\]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should not flag escaped brackets");

        // Test with URL containing parentheses
        let content = "(https://example.com/path(with)parens)[text]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should still flag URLs with nested parentheses");
    }
}
