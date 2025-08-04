use crate::utils::range_utils::{LineIndex, calculate_match_range};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to match quote lines with multiple spaces after >
    static ref BLOCKQUOTE_MULTIPLE_SPACES: Regex = Regex::new(r"^(\s*)>(\s{2,})(.*)$").unwrap();

    // New patterns for detecting malformed blockquote attempts where user intent is clear
    static ref MALFORMED_BLOCKQUOTE_PATTERNS: Vec<(Regex, &'static str)> = vec![
        // Double > without space: >>text (looks like nested but missing spaces)
        (Regex::new(r"^(\s*)>>([^\s>].*|$)").unwrap(), "missing spaces in nested blockquote"),

        // Triple > without space: >>>text
        (Regex::new(r"^(\s*)>>>([^\s>].*|$)").unwrap(), "missing spaces in deeply nested blockquote"),

        // Space then > then text: > >text (extra > by mistake)
        (Regex::new(r"^(\s*)>\s+>([^\s>].*|$)").unwrap(), "extra blockquote marker"),

        // Multiple spaces then >: (spaces)>text (indented blockquote without space)
        (Regex::new(r"^(\s{4,})>([^\s].*|$)").unwrap(), "indented blockquote missing space"),
    ];
}

/// Rule MD027: No multiple spaces after blockquote symbol
///
/// See [docs/md027.md](../../docs/md027.md) for full documentation, configuration, and examples.

#[derive(Debug, Default, Clone)]
pub struct MD027MultipleSpacesBlockquote;

impl Rule for MD027MultipleSpacesBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after quote marker (>)"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip lines in code blocks
            if line_info.in_code_block {
                continue;
            }

            // Check if this line is a blockquote using cached info
            if let Some(blockquote) = &line_info.blockquote {
                // Part 1: Check for multiple spaces after the blockquote marker
                if blockquote.has_multiple_spaces_after_marker {
                    // Find where the extra spaces start in the line
                    // We need to find the position after the markers and first space/tab
                    let mut byte_pos = 0;
                    let mut found_markers = 0;
                    let mut found_first_space = false;

                    for (i, ch) in line_info.content.char_indices() {
                        if found_markers < blockquote.nesting_level {
                            if ch == '>' {
                                found_markers += 1;
                            }
                        } else if !found_first_space && (ch == ' ' || ch == '\t') {
                            // This is the first space/tab after markers
                            found_first_space = true;
                        } else if found_first_space && (ch == ' ' || ch == '\t') {
                            // This is where extra spaces start
                            byte_pos = i;
                            break;
                        }
                    }

                    // Count how many extra spaces/tabs there are
                    let extra_spaces_bytes = line_info.content[byte_pos..]
                        .chars()
                        .take_while(|&c| c == ' ' || c == '\t')
                        .fold(0, |acc, ch| acc + ch.len_utf8());

                    if extra_spaces_bytes > 0 {
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num, &line_info.content, byte_pos, extra_spaces_bytes);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Multiple spaces after quote marker (>)".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: {
                                    let line_index = LineIndex::new(ctx.content.to_string());
                                    let start_byte = line_index.line_col_to_byte_range(line_num, start_col).start;
                                    let end_byte = line_index.line_col_to_byte_range(line_num, end_col).start;
                                    start_byte..end_byte
                                },
                                replacement: "".to_string(), // Remove the extra spaces
                            }),
                        });
                    }
                }
            } else {
                // Part 2: Check for malformed blockquote attempts on non-blockquote lines
                let malformed_attempts = self.detect_malformed_blockquote_attempts(&line_info.content);
                for (start, len, fixed_line, description) in malformed_attempts {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num, &line_info.content, start, len);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Malformed quote: {description}"),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: {
                                let line_index = LineIndex::new(ctx.content.to_string());
                                line_index.line_col_to_byte_range(line_num, 1)
                            },
                            replacement: fixed_line,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = Vec::with_capacity(ctx.lines.len());

        for line_info in &ctx.lines {
            if let Some(blockquote) = &line_info.blockquote {
                // Fix blockquotes with multiple spaces after the marker
                if blockquote.has_multiple_spaces_after_marker {
                    // Rebuild the line with exactly one space after the markers
                    let fixed_line = format!(
                        "{}{} {}",
                        blockquote.indent,
                        ">".repeat(blockquote.nesting_level),
                        blockquote.content
                    );
                    result.push(fixed_line);
                } else {
                    result.push(line_info.content.clone());
                }
            } else {
                // Check for malformed blockquote attempts
                let malformed_attempts = self.detect_malformed_blockquote_attempts(&line_info.content);
                if !malformed_attempts.is_empty() {
                    // Use the first fix (there should only be one per line)
                    let (_, _, fixed_line, _) = &malformed_attempts[0];
                    result.push(fixed_line.clone());
                } else {
                    result.push(line_info.content.clone());
                }
            }
        }

        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if ctx.content.ends_with('\n') { "\n" } else { "" })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD027MultipleSpacesBlockquote)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains('>')
    }
}

impl MD027MultipleSpacesBlockquote {
    /// Detect malformed blockquote attempts where user intent is clear
    fn detect_malformed_blockquote_attempts(&self, line: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();

        for (pattern, issue_type) in MALFORMED_BLOCKQUOTE_PATTERNS.iter() {
            if let Some(cap) = pattern.captures(line) {
                let match_obj = cap.get(0).unwrap();
                let start = match_obj.start();
                let len = match_obj.len();

                // Extract potential blockquote components
                if let Some((fixed_line, description)) = self.extract_blockquote_fix_from_match(&cap, issue_type, line)
                {
                    // Only proceed if this looks like a genuine blockquote attempt
                    if self.looks_like_blockquote_attempt(line, &fixed_line) {
                        results.push((start, len, fixed_line, description));
                    }
                }
            }
        }

        results
    }

    /// Extract the proper blockquote format from a malformed match
    fn extract_blockquote_fix_from_match(
        &self,
        cap: &regex::Captures,
        issue_type: &str,
        _original_line: &str,
    ) -> Option<(String, String)> {
        match issue_type {
            "missing spaces in nested blockquote" => {
                // >>text -> > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> > {}", indent, content.trim()),
                    "Missing spaces in nested blockquote".to_string(),
                ))
            }
            "missing spaces in deeply nested blockquote" => {
                // >>>text -> > > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> > > {}", indent, content.trim()),
                    "Missing spaces in deeply nested blockquote".to_string(),
                ))
            }
            "extra blockquote marker" => {
                // > >text -> > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> {}", indent, content.trim()),
                    "Extra blockquote marker".to_string(),
                ))
            }
            "indented blockquote missing space" => {
                // (spaces)>text -> (spaces)> text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> {}", indent, content.trim()),
                    "Indented blockquote missing space".to_string(),
                ))
            }
            _ => None,
        }
    }

    /// Check if the pattern looks like a genuine blockquote attempt
    fn looks_like_blockquote_attempt(&self, original: &str, fixed: &str) -> bool {
        // Basic heuristics to avoid false positives

        // 1. Content should not be too short (avoid flagging things like ">>>" alone)
        let trimmed_original = original.trim();
        if trimmed_original.len() < 5 {
            // More restrictive
            return false;
        }

        // 2. Should contain some text content after the markers
        let content_after_markers = trimmed_original.trim_start_matches('>').trim_start_matches(' ');
        if content_after_markers.is_empty() || content_after_markers.len() < 3 {
            // More restrictive
            return false;
        }

        // 3. Content should contain some alphabetic characters (not just symbols)
        if !content_after_markers.chars().any(|c| c.is_alphabetic()) {
            return false;
        }

        // 4. Fixed version should actually be a valid blockquote
        // Check if it starts with optional whitespace followed by >
        let blockquote_pattern = regex::Regex::new(r"^\s*>").unwrap();
        if !blockquote_pattern.is_match(fixed) {
            return false;
        }

        // 5. Avoid flagging things that might be code or special syntax
        if content_after_markers.starts_with('#') // Headers
            || content_after_markers.starts_with('[') // Links
            || content_after_markers.starts_with('`') // Code
            || content_after_markers.starts_with("http") // URLs
            || content_after_markers.starts_with("www.") // URLs
            || content_after_markers.starts_with("ftp")
        // URLs
        {
            return false;
        }

        // 6. Content should look like prose, not code or markup
        let word_count = content_after_markers.split_whitespace().count();
        if word_count < 3 {
            // Should be at least 3 words to look like prose
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_blockquote() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = "> This is a blockquote\n> > Nested quote";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid blockquotes should not be flagged");
    }

    #[test]
    fn test_multiple_spaces_after_marker() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">  This has two spaces\n>   This has three spaces";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 3); // Points to the extra space (after > and first space)
        assert_eq!(result[0].message, "Multiple spaces after quote marker (>)");
        assert_eq!(result[1].line, 2);
        assert_eq!(result[1].column, 3);
    }

    #[test]
    fn test_nested_multiple_spaces() {
        let rule = MD027MultipleSpacesBlockquote;
        // LintContext sees these as single-level blockquotes because of the space between markers
        let content = ">  Two spaces after marker\n>>  Two spaces in nested blockquote";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Multiple spaces"));
        assert!(result[1].message.contains("Multiple spaces"));
    }

    #[test]
    fn test_malformed_nested_quote() {
        let rule = MD027MultipleSpacesBlockquote;
        // LintContext sees >>text as a valid nested blockquote with no space after marker
        // MD027 doesn't flag this as malformed, only as missing space after marker
        let content = ">>This is a nested blockquote without space after markers";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // This should not be flagged at all since >>text is valid CommonMark
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_malformed_deeply_nested() {
        let rule = MD027MultipleSpacesBlockquote;
        // LintContext sees >>>text as a valid triple-nested blockquote
        let content = ">>>This is deeply nested without spaces after markers";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // This should not be flagged - >>>text is valid CommonMark
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_extra_quote_marker() {
        let rule = MD027MultipleSpacesBlockquote;
        // "> >text" is parsed as single-level blockquote with ">text" as content
        // This is valid CommonMark and not detected as malformed
        let content = "> >This looks like nested but is actually single level with >This as content";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_indented_missing_space() {
        let rule = MD027MultipleSpacesBlockquote;
        // 4+ spaces makes this a code block, not a blockquote
        let content = "   >This has 3 spaces indent and no space after marker";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // LintContext sees this as a blockquote with no space after marker
        // MD027 doesn't flag this as malformed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_multiple_spaces() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">  Two spaces\n>   Three spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Two spaces\n> Three spaces");
    }

    #[test]
    fn test_fix_malformed_quotes() {
        let rule = MD027MultipleSpacesBlockquote;
        // These are valid nested blockquotes, not malformed
        let content = ">>Nested without spaces\n>>>Deeply nested without spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // No fix needed - these are valid
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_extra_marker() {
        let rule = MD027MultipleSpacesBlockquote;
        // This is valid - single blockquote with >Extra as content
        let content = "> >Extra marker here";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // No fix needed
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_code_block_ignored() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = "```\n>  This is in a code block\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Code blocks should be ignored");
    }

    #[test]
    fn test_short_content_not_flagged() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">>>\n>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Very short content should not be flagged");
    }

    #[test]
    fn test_non_prose_not_flagged() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">>#header\n>>[link]\n>>`code`\n>>http://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Non-prose content should not be flagged");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">  Two spaces\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Two spaces\n");

        let content_no_newline = ">  Two spaces";
        let ctx2 = LintContext::new(content_no_newline);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "> Two spaces");
    }

    #[test]
    fn test_mixed_issues() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">  Multiple spaces here\n>>Normal nested quote\n> Normal quote";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should only flag the multiple spaces");
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_looks_like_blockquote_attempt() {
        let rule = MD027MultipleSpacesBlockquote;

        // Should return true for genuine attempts
        assert!(rule.looks_like_blockquote_attempt(
            ">>This is a real blockquote attempt with text",
            "> > This is a real blockquote attempt with text"
        ));

        // Should return false for too short
        assert!(!rule.looks_like_blockquote_attempt(">>>", "> > >"));

        // Should return false for no alphabetic content
        assert!(!rule.looks_like_blockquote_attempt(">>123", "> > 123"));

        // Should return false for code-like content
        assert!(!rule.looks_like_blockquote_attempt(">>#header", "> > #header"));
    }

    #[test]
    fn test_extract_blockquote_fix() {
        let rule = MD027MultipleSpacesBlockquote;
        let regex = Regex::new(r"^(\s*)>>([^\s>].*|$)").unwrap();
        let cap = regex.captures(">>content").unwrap();

        let result = rule.extract_blockquote_fix_from_match(&cap, "missing spaces in nested blockquote", ">>content");
        assert!(result.is_some());
        let (fixed, desc) = result.unwrap();
        assert_eq!(fixed, "> > content");
        assert!(desc.contains("Missing spaces"));
    }

    #[test]
    fn test_empty_blockquote() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">\n>  \n> content";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Empty blockquotes with multiple spaces should still be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = "  >  Indented with multiple spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented with multiple spaces");
    }

    #[test]
    fn test_tabs_after_marker() {
        let rule = MD027MultipleSpacesBlockquote;
        // Tab after marker - should be flagged as multiple spaces
        let content = ">\tTab after marker";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Tab after marker should be flagged");
        assert_eq!(result[0].message, "Multiple spaces after quote marker (>)");

        // Tab and space after marker
        let content2 = ">\t Space then tab";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 1, "Tab and space should be flagged");

        // Two tabs after marker
        let content3 = ">\t\tTwo tabs";
        let ctx3 = LintContext::new(content3);
        let result3 = rule.check(&ctx3).unwrap();
        assert_eq!(result3.len(), 1, "Two tabs should be flagged");
    }

    #[test]
    fn test_mixed_spaces_and_tabs() {
        let rule = MD027MultipleSpacesBlockquote;
        // Space then tab
        let content = "> \tSpace then tab";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 3); // Points to the tab

        // Tab then space
        let content2 = ">\t Tab then space";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 1);
        assert_eq!(result2[0].column, 3); // Points to the space after tab
    }

    #[test]
    fn test_fix_tabs() {
        let rule = MD027MultipleSpacesBlockquote;
        // Fix should remove extra tabs
        let content = ">\t\tTwo tabs";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Two tabs");

        // Fix mixed spaces and tabs
        let content2 = "> \t Mixed";
        let ctx2 = LintContext::new(content2);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "> Mixed");
    }
}
