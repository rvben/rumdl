/// Rule MD028: No blank lines inside blockquotes
///
/// This rule flags blank lines that appear to be inside a blockquote but lack the > marker.
/// It uses heuristics to distinguish between paragraph breaks within a blockquote
/// and intentional separators between distinct blockquotes.
///
/// GFM Alerts (GitHub Flavored Markdown) are automatically detected and excluded:
/// - `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`
///   These alerts MUST be separated by blank lines to render correctly on GitHub.
///
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_line_range;

/// GFM Alert types supported by GitHub
/// Reference: https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts
const GFM_ALERT_TYPES: &[&str] = &["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

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

    /// Check if a blockquote line is a GFM alert start
    /// GFM alerts have the format: `> [!TYPE]` where TYPE is NOTE, TIP, IMPORTANT, WARNING, or CAUTION
    /// Reference: https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts
    #[inline]
    fn is_gfm_alert_line(line: &str) -> bool {
        // Fast path: must contain '[!' pattern
        if !line.contains("[!") {
            return false;
        }

        // Extract content after the > marker(s)
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') {
            return false;
        }

        // Skip all > markers and whitespace to get to content
        let content = trimmed
            .trim_start_matches('>')
            .trim_start_matches([' ', '\t'])
            .trim_start_matches('>')
            .trim_start();

        // Check for GFM alert pattern: [!TYPE]
        if !content.starts_with("[!") {
            return false;
        }

        // Extract the alert type
        if let Some(end_bracket) = content.find(']') {
            let alert_type = &content[2..end_bracket];
            return GFM_ALERT_TYPES.iter().any(|&t| t.eq_ignore_ascii_case(alert_type));
        }

        false
    }

    /// Find the first line of a blockquote block starting from a given line
    /// Scans backwards to find where this blockquote block begins
    fn find_blockquote_start(lines: &[&str], from_idx: usize) -> Option<usize> {
        if from_idx >= lines.len() {
            return None;
        }

        // Start from the given line and scan backwards
        let mut start_idx = from_idx;

        for i in (0..=from_idx).rev() {
            let line = lines[i];

            // If it's a blockquote line, update start
            if Self::is_blockquote_line(line) {
                start_idx = i;
            } else if line.trim().is_empty() {
                // Blank line - check if previous content was blockquote
                // If we haven't found any blockquote yet, continue
                if start_idx == from_idx && !Self::is_blockquote_line(lines[from_idx]) {
                    continue;
                }
                // Otherwise, blank line ends this blockquote block
                break;
            } else {
                // Non-blockquote, non-blank line - this ends the blockquote block
                break;
            }
        }

        // Return start only if it's actually a blockquote line
        if Self::is_blockquote_line(lines[start_idx]) {
            Some(start_idx)
        } else {
            None
        }
    }

    /// Check if a blockquote block (starting at given index) is a GFM alert
    fn is_gfm_alert_block(lines: &[&str], blockquote_line_idx: usize) -> bool {
        // Find the start of this blockquote block
        if let Some(start_idx) = Self::find_blockquote_start(lines, blockquote_line_idx) {
            // Check if the first line of the block is a GFM alert
            return Self::is_gfm_alert_line(lines[start_idx]);
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

        // GFM Alert check: If either blockquote is a GFM alert (> [!NOTE], > [!TIP], etc.),
        // treat them as intentionally separate blockquotes. GFM alerts MUST be separated
        // by blank lines to render correctly on GitHub.
        let prev_is_alert = Self::is_gfm_alert_block(lines, prev_idx);
        let next_is_alert = Self::is_gfm_alert_block(lines, next_idx);
        if prev_is_alert || next_is_alert {
            return false;
        }

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
                    rule_name: Some(self.name().to_string()),
                    message: format!("Blank line inside blockquote (level {level})"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: ctx
                            .line_index
                            .line_col_to_byte_range_with_length(line_num, 1, line.len()),
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag content without blockquotes");
    }

    #[test]
    fn test_valid_blockquote_no_blanks() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> This is a blockquote\n> With multiple lines\n> But no blank lines";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blockquotes without blank lines");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with just > are valid and should NOT be flagged
        let content = "> First line\n>\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with just > marker");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker_and_space() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with > and space are also valid
        let content = "> First line\n> \n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with > and space");
    }

    #[test]
    fn test_blank_line_in_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        // Truly blank line (no >) inside blockquote should be flagged
        let content = "> First line\n\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag truly blank line inside blockquote");
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Blank line inside blockquote"));
    }

    #[test]
    fn test_multiple_blank_lines() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n\n> Fourth";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_nested_blockquote_with_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with >> are valid
        let content = ">> Nested quote\n>>\n>> More nested";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >> marker");
    }

    #[test]
    fn test_fix_single_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n> Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> First\n>\n> Third");
    }

    #[test]
    fn test_fix_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested\n\n>> More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">> Nested\n>>\n>> More");
    }

    #[test]
    fn test_fix_with_indentation() {
        let rule = MD028NoBlanksBlockquote;
        let content = "  > Indented quote\n\n  > More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented quote\n  >\n  > More");
    }

    #[test]
    fn test_mixed_levels() {
        let rule = MD028NoBlanksBlockquote;
        // Blank lines between different levels
        let content = "> Level 1\n\n>> Level 2\n\n> Level 1 again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx1 = LintContext::new("No blockquotes here", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx1));

        let ctx2 = LintContext::new("> Has blockquote", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx2));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD028NoBlanksBlockquote;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blank_after_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\nNot a quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line after blockquote ends is valid");
    }

    #[test]
    fn test_blank_before_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "Not a quote\n\n> Quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line before blockquote starts is valid");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\n> More\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with('\n'));

        let content_no_newline = "> Quote\n\n> More";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert!(!fixed2.ends_with('\n'));
    }

    #[test]
    fn test_document_structure_extension() {
        let rule = MD028NoBlanksBlockquote;
        let ctx = LintContext::new("> test", crate::config::MarkdownFlavor::Standard, None);
        // Test that the rule works correctly with blockquotes
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag valid blockquote");

        // Test that rule skips content without blockquotes
        let ctx2 = LintContext::new("no blockquote", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx2), "Should skip content without blockquotes");
    }

    #[test]
    fn test_deeply_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">>> Deep nest\n\n>>> More deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >>> marker");
    }

    #[test]
    fn test_complex_blockquote_structure() {
        let rule = MD028NoBlanksBlockquote;
        // Line with > is valid, not a blank line
        let content = "> Level 1\n> > Nested properly\n>\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag line with > marker");
    }

    #[test]
    fn test_complex_with_blank() {
        let rule = MD028NoBlanksBlockquote;
        // Blank line between different nesting levels is not flagged
        // (going from >> back to > is a context change)
        let content = "> Level 1\n> > Nested\n\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Blank between different nesting levels is not inside blockquote"
        );
    }

    // ==================== GFM Alert Tests ====================
    // GitHub Flavored Markdown alerts use the syntax > [!TYPE] where TYPE is
    // NOTE, TIP, IMPORTANT, WARNING, or CAUTION. These alerts MUST be separated
    // by blank lines to render correctly on GitHub.
    // Reference: https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts

    #[test]
    fn test_gfm_alert_detection_note() {
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!NOTE]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!NOTE] Additional text"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line(">  [!NOTE]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!note]")); // case insensitive
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!Note]")); // mixed case
    }

    #[test]
    fn test_gfm_alert_detection_all_types() {
        // All five GFM alert types
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!NOTE]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!TIP]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!IMPORTANT]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!WARNING]"));
        assert!(MD028NoBlanksBlockquote::is_gfm_alert_line("> [!CAUTION]"));
    }

    #[test]
    fn test_gfm_alert_detection_not_alert() {
        // These should NOT be detected as GFM alerts
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("> Regular blockquote"));
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("> [!INVALID]"));
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("> [NOTE]")); // missing !
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("> [!]")); // empty type
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("Regular text [!NOTE]")); // not blockquote
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("")); // empty
        assert!(!MD028NoBlanksBlockquote::is_gfm_alert_line("> ")); // empty blockquote
    }

    #[test]
    fn test_gfm_alerts_separated_by_blank_line() {
        // Issue #126 use case: Two GFM alerts separated by blank line should NOT be flagged
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!TIP]\n> Here's a github tip\n\n> [!NOTE]\n> Here's a github note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blank line between GFM alerts");
    }

    #[test]
    fn test_gfm_alerts_all_five_types_separated() {
        // All five alert types in sequence, each separated by blank lines
        let rule = MD028NoBlanksBlockquote;
        let content = r#"> [!NOTE]
> Note content

> [!TIP]
> Tip content

> [!IMPORTANT]
> Important content

> [!WARNING]
> Warning content

> [!CAUTION]
> Caution content"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag blank lines between any GFM alert types"
        );
    }

    #[test]
    fn test_gfm_alert_with_multiple_lines() {
        // GFM alert with multiple content lines, then another alert
        let rule = MD028NoBlanksBlockquote;
        let content = r#"> [!WARNING]
> This is a warning
> with multiple lines
> of content

> [!NOTE]
> This is a note"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag blank line between multi-line GFM alerts"
        );
    }

    #[test]
    fn test_gfm_alert_followed_by_regular_blockquote() {
        // GFM alert followed by regular blockquote - should NOT flag
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!TIP]\n> A helpful tip\n\n> Regular blockquote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blank line after GFM alert");
    }

    #[test]
    fn test_regular_blockquote_followed_by_gfm_alert() {
        // Regular blockquote followed by GFM alert - should NOT flag
        let rule = MD028NoBlanksBlockquote;
        let content = "> Regular blockquote\n\n> [!NOTE]\n> Important note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blank line before GFM alert");
    }

    #[test]
    fn test_regular_blockquotes_still_flagged() {
        // Regular blockquotes (not GFM alerts) should still be flagged
        let rule = MD028NoBlanksBlockquote;
        let content = "> First blockquote\n\n> Second blockquote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should still flag blank line between regular blockquotes"
        );
    }

    #[test]
    fn test_gfm_alert_blank_line_within_same_alert() {
        // Blank line WITHIN a single GFM alert should still be flagged
        // (this is a missing > marker inside the alert)
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!NOTE]\n> First paragraph\n\n> Second paragraph of same note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The second > line is NOT a new alert, so this is a blank within the same blockquote
        // However, since the first blockquote is a GFM alert, and the second is just continuation,
        // this could be ambiguous. Current implementation: if first is alert, don't flag.
        // This is acceptable - user can use > marker on blank line if they want continuation.
        assert!(
            result.is_empty(),
            "GFM alert status propagates to subsequent blockquote lines"
        );
    }

    #[test]
    fn test_gfm_alert_case_insensitive() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!note]\n> lowercase\n\n> [!TIP]\n> uppercase\n\n> [!Warning]\n> mixed";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "GFM alert detection should be case insensitive");
    }

    #[test]
    fn test_gfm_alert_with_nested_blockquote() {
        // GFM alert doesn't support nesting, but test behavior
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!NOTE]\n> > Nested quote inside alert\n\n> [!TIP]\n> Tip";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag blank between alerts even with nested content"
        );
    }

    #[test]
    fn test_gfm_alert_indented() {
        let rule = MD028NoBlanksBlockquote;
        // Indented GFM alerts (e.g., in a list context)
        let content = "  > [!NOTE]\n  > Indented note\n\n  > [!TIP]\n  > Indented tip";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blank between indented GFM alerts");
    }

    #[test]
    fn test_gfm_alert_mixed_with_regular_content() {
        // Mixed document with GFM alerts and regular content
        let rule = MD028NoBlanksBlockquote;
        let content = r#"# Heading

Some paragraph.

> [!NOTE]
> Important note

More paragraph text.

> [!WARNING]
> Be careful!

Final text."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "GFM alerts in mixed document should not trigger warnings"
        );
    }

    #[test]
    fn test_gfm_alert_fix_not_applied() {
        // When we have GFM alerts, fix should not modify the blank lines
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!TIP]\n> Tip\n\n> [!NOTE]\n> Note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should not modify blank lines between GFM alerts");
    }

    #[test]
    fn test_gfm_alert_multiple_blank_lines_between() {
        // Multiple blank lines between GFM alerts should not be flagged
        let rule = MD028NoBlanksBlockquote;
        let content = "> [!NOTE]\n> Note\n\n\n> [!TIP]\n> Tip";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag multiple blank lines between GFM alerts"
        );
    }
}
