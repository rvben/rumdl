/// Rule MD028: No blank lines inside blockquotes
///
/// This rule flags blank lines that appear to be inside a blockquote but lack the > marker.
/// It uses heuristics to distinguish between paragraph breaks within a blockquote
/// and intentional separators between distinct blockquotes.
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_line_range};

#[derive(Clone)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Check if a line is a blockquote line (has > markers)
    #[inline]
    fn is_blockquote_line(line: &str) -> bool {
        // Fast path: check for '>' character before doing any string operations
        if !line.as_bytes().contains(&b'>') {
            return false;
        }
        line.trim_start().starts_with('>')
    }

    /// Get the blockquote level (number of > markers) and leading whitespace
    /// Returns (level, whitespace_end_idx)
    fn get_blockquote_info(line: &str) -> (usize, usize) {
        let bytes = line.as_bytes();
        let mut i = 0;

        // Skip leading whitespace
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }

        let whitespace_end = i;
        let mut level = 0;

        // Count '>' markers
        while i < bytes.len() {
            if bytes[i] == b'>' {
                level += 1;
                i += 1;
            } else if bytes[i] == b' ' || bytes[i] == b'\t' {
                i += 1;
            } else {
                break;
            }
        }

        (level, whitespace_end)
    }

    /// Check if there's substantive content between two blockquote sections
    /// This helps distinguish between paragraph breaks and separate blockquotes
    fn has_content_between(lines: &[&str], start: usize, end: usize) -> bool {
        for line in lines.iter().take(end).skip(start) {
            let trimmed = line.trim();
            // If there's any non-blank, non-blockquote content, these are separate quotes
            if !trimmed.is_empty() && !trimmed.starts_with('>') {
                return true;
            }
        }
        false
    }

    /// Analyze context to determine if quotes are likely the same or different
    fn are_likely_same_blockquote(lines: &[&str], blank_idx: usize) -> bool {
        // Look for patterns that suggest these are the same blockquote:
        // 1. Only one blank line between them (multiple blanks suggest separation)
        // 2. Same indentation level
        // 3. No content between them
        // 4. Similar blockquote levels

        // Note: We flag ALL blank lines between blockquotes, matching markdownlint behavior.
        // Even multiple consecutive blank lines are flagged as they can be ambiguous
        // (some parsers treat them as one blockquote, others as separate blockquotes).

        // Find previous and next blockquote lines using fast byte scanning
        let mut prev_quote_idx = None;
        let mut next_quote_idx = None;

        // Scan backwards for previous blockquote
        for i in (0..blank_idx).rev() {
            let line = lines[i];
            // Fast check: if no '>' character, skip
            if line.as_bytes().contains(&b'>') && Self::is_blockquote_line(line) {
                prev_quote_idx = Some(i);
                break;
            }
        }

        // Scan forwards for next blockquote
        for (i, line) in lines.iter().enumerate().skip(blank_idx + 1) {
            // Fast check: if no '>' character, skip
            if line.as_bytes().contains(&b'>') && Self::is_blockquote_line(line) {
                next_quote_idx = Some(i);
                break;
            }
        }

        let (prev_idx, next_idx) = match (prev_quote_idx, next_quote_idx) {
            (Some(p), Some(n)) => (p, n),
            _ => return false,
        };

        // Check for content between blockquotes
        if Self::has_content_between(lines, prev_idx + 1, next_idx) {
            return false;
        }

        // Get blockquote info once per line to avoid repeated parsing
        let (prev_level, prev_whitespace_end) = Self::get_blockquote_info(lines[prev_idx]);
        let (next_level, next_whitespace_end) = Self::get_blockquote_info(lines[next_idx]);

        // Different levels suggest different contexts
        // But next_level > prev_level could be nested continuation
        if next_level < prev_level {
            return false;
        }

        // Check indentation consistency using byte indices
        let prev_line = lines[prev_idx];
        let next_line = lines[next_idx];
        let prev_indent = &prev_line[..prev_whitespace_end];
        let next_indent = &next_line[..next_whitespace_end];

        // Different indentation indicates separate blockquote contexts
        // Same indentation with no content between = same blockquote (blank line inside)
        prev_indent == next_indent
    }

    /// Check if a blank line is problematic (inside a blockquote)
    fn is_problematic_blank_line(lines: &[&str], index: usize) -> Option<(usize, String)> {
        let current_line = lines[index];

        // Must be a blank line (no content, no > markers)
        if !current_line.trim().is_empty() || Self::is_blockquote_line(current_line) {
            return None;
        }

        // Use heuristics to determine if this blank line is inside a blockquote
        // or if it's an intentional separator between blockquotes
        if !Self::are_likely_same_blockquote(lines, index) {
            return None;
        }

        // This blank line appears to be inside a blockquote
        // Find the appropriate fix using optimized parsing
        for i in (0..index).rev() {
            let line = lines[i];
            // Fast check: if no '>' character, skip
            if line.as_bytes().contains(&b'>') && Self::is_blockquote_line(line) {
                let (level, whitespace_end) = Self::get_blockquote_info(line);
                let indent = &line[..whitespace_end];
                let mut fix = String::with_capacity(indent.len() + level);
                fix.push_str(indent);
                for _ in 0..level {
                    fix.push('>');
                }
                return Some((level, fix));
            }
        }

        None
    }
}

impl Default for MD028NoBlanksBlockquote {
    fn default() -> Self {
        Self
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for content without blockquotes
        if !ctx.content.contains('>') {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Get all lines
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Pre-scan to find blank lines and blockquote lines for faster processing
        let mut blank_line_indices = Vec::new();
        let mut has_blockquotes = false;

        for (line_idx, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_idx < ctx.lines.len() && ctx.lines[line_idx].in_code_block {
                continue;
            }

            if line.trim().is_empty() {
                blank_line_indices.push(line_idx);
            } else if Self::is_blockquote_line(line) {
                has_blockquotes = true;
            }
        }

        // If no blockquotes found, no need to check blank lines
        if !has_blockquotes {
            return Ok(Vec::new());
        }

        // Only check blank lines that could be problematic
        for &line_idx in &blank_line_indices {
            let line_num = line_idx + 1;

            // Check if this is a problematic blank line inside a blockquote
            if let Some((level, fix_content)) = Self::is_problematic_blank_line(&lines, line_idx) {
                let line = lines[line_idx];
                let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Blank line inside blockquote (level {level})"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(line_num, 1, line.len()),
                        replacement: fix_content,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = Vec::with_capacity(ctx.lines.len());
        let lines: Vec<&str> = ctx.content.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            // Check if this blank line needs fixing
            if let Some((_, fix_content)) = Self::is_problematic_blank_line(&lines, line_idx) {
                result.push(fix_content);
            } else {
                result.push(line.to_string());
            }
        }

        Ok(result.join("\n") + if ctx.content.ends_with('\n') { "\n" } else { "" })
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Blockquote
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.likely_has_blockquotes()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD028NoBlanksBlockquote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_no_blockquotes() {
        let rule = MD028NoBlanksBlockquote;
        let content = "This is regular text\n\nWith blank lines\n\nBut no blockquotes";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag content without blockquotes");
    }

    #[test]
    fn test_valid_blockquote_no_blanks() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> This is a blockquote\n> With multiple lines\n> But no blank lines";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blockquotes without blank lines");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with just > are valid and should NOT be flagged
        let content = "> First line\n>\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with just > marker");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker_and_space() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with > and space are also valid
        let content = "> First line\n> \n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with > and space");
    }

    #[test]
    fn test_blank_line_in_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        // Truly blank line (no >) inside blockquote should be flagged
        let content = "> First line\n\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag truly blank line inside blockquote");
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Blank line inside blockquote"));
    }

    #[test]
    fn test_multiple_blank_lines() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n\n> Fourth";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With proper indentation checking, both blank lines are flagged as they're within the same blockquote
        assert_eq!(result.len(), 2, "Should flag each blank line within the blockquote");
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_nested_blockquote_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested quote\n\n>> More nested";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_nested_blockquote_with_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with >> are valid
        let content = ">> Nested quote\n>>\n>> More nested";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >> marker");
    }

    #[test]
    fn test_fix_single_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n> Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> First\n>\n> Third");
    }

    #[test]
    fn test_fix_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested\n\n>> More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">> Nested\n>>\n>> More");
    }

    #[test]
    fn test_fix_with_indentation() {
        let rule = MD028NoBlanksBlockquote;
        let content = "  > Indented quote\n\n  > More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented quote\n  >\n  > More");
    }

    #[test]
    fn test_mixed_levels() {
        let rule = MD028NoBlanksBlockquote;
        // Blank lines between different levels
        let content = "> Level 1\n\n>> Level 2\n\n> Level 1 again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Line 2 is a blank between > and >>, level 1 to level 2, considered inside level 1
        // Line 4 is a blank between >> and >, level 2 to level 1, NOT inside blockquote
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_blockquote_with_code_block() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote with code:\n> ```\n> code\n> ```\n>\n> More quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Line 5 has > marker, so it's not a blank line
        assert!(result.is_empty(), "Should not flag line with > marker");
    }

    #[test]
    fn test_category() {
        let rule = MD028NoBlanksBlockquote;
        assert_eq!(rule.category(), RuleCategory::Blockquote);
    }

    #[test]
    fn test_should_skip() {
        let rule = MD028NoBlanksBlockquote;
        let ctx1 = LintContext::new("No blockquotes here", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx1));

        let ctx2 = LintContext::new("> Has blockquote", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx2));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD028NoBlanksBlockquote;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blank_after_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\nNot a quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line after blockquote ends is valid");
    }

    #[test]
    fn test_blank_before_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "Not a quote\n\n> Quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line before blockquote starts is valid");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\n> More\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with('\n'));

        let content_no_newline = "> Quote\n\n> More";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert!(!fixed2.ends_with('\n'));
    }

    #[test]
    fn test_document_structure_extension() {
        let rule = MD028NoBlanksBlockquote;
        let ctx = LintContext::new("> test", crate::config::MarkdownFlavor::Standard);
        // Test that the rule works correctly with blockquotes
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag valid blockquote");

        // Test that rule skips content without blockquotes
        let ctx2 = LintContext::new("no blockquote", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx2), "Should skip content without blockquotes");
    }

    #[test]
    fn test_deeply_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">>> Deep nest\n\n>>> More deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">>> Deep nest\n>>>\n>>> More deep");
    }

    #[test]
    fn test_deeply_nested_with_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with >>> are valid
        let content = ">>> Deep nest\n>>>\n>>> More deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >>> marker");
    }

    #[test]
    fn test_complex_blockquote_structure() {
        let rule = MD028NoBlanksBlockquote;
        // Line with > is valid, not a blank line
        let content = "> Level 1\n> > Nested properly\n>\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag line with > marker");
    }

    #[test]
    fn test_complex_with_blank() {
        let rule = MD028NoBlanksBlockquote;
        // Blank line between different nesting levels is not flagged
        // (going from >> back to > is a context change)
        let content = "> Level 1\n> > Nested\n\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Blank between different nesting levels is not inside blockquote"
        );
    }
}
