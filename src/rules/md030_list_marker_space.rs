//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rule::{LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::rules::list_utils::ListType;
use crate::utils::blockquote::effective_indent_in_blockquote;
use crate::utils::element_cache::ElementCache;
use crate::utils::range_utils::calculate_match_range;
use toml;

mod md030_config;
use md030_config::MD030Config;

#[derive(Clone, Default)]
pub struct MD030ListMarkerSpace {
    config: MD030Config,
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            config: MD030Config {
                ul_single: crate::types::PositiveUsize::new(ul_single)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ul_multi: crate::types::PositiveUsize::new(ul_multi)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ol_single: crate::types::PositiveUsize::new(ol_single)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
                ol_multi: crate::types::PositiveUsize::new(ol_multi)
                    .unwrap_or(crate::types::PositiveUsize::from_const(1)),
            },
        }
    }

    pub fn from_config_struct(config: MD030Config) -> Self {
        Self { config }
    }

    pub fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.config.ul_single.get(),
            (ListType::Unordered, true) => self.config.ul_multi.get(),
            (ListType::Ordered, false) => self.config.ol_single.get(),
            (ListType::Ordered, true) => self.config.ol_multi.get(),
        }
    }
}

impl Rule for MD030ListMarkerSpace {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Early return if no list content
        if self.should_skip(ctx) {
            return Ok(warnings);
        }

        // Collect lines once
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Track which lines we've already processed (to avoid duplicates)
        let mut processed_lines = std::collections::HashSet::new();

        // First pass: Check parser-recognized list items
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            // Skip code blocks and math blocks - content inside these is not markdown
            if line_info.list_item.is_some() && !line_info.in_code_block && !line_info.in_math_block {
                let line_num_1based = line_num + 1;
                processed_lines.insert(line_num_1based);

                let line = lines[line_num];

                // Skip indented code blocks (4+ columns accounting for tab expansion)
                if ElementCache::calculate_indentation_width_default(line) >= 4 {
                    continue;
                }

                if let Some(list_info) = &line_info.list_item {
                    let list_type = if list_info.is_ordered {
                        ListType::Ordered
                    } else {
                        ListType::Unordered
                    };

                    // Calculate actual spacing after marker
                    let marker_end = list_info.marker_column + list_info.marker.len();

                    // Skip if there's no content on this line after the marker
                    // MD030 only applies when there IS content after the marker
                    if !Self::has_content_after_marker(line, marker_end) {
                        continue;
                    }

                    let actual_spaces = list_info.content_column.saturating_sub(marker_end);

                    // Determine if this is a multi-line list item
                    let is_multi_line = self.is_multi_line_list_item(ctx, line_num_1based, &lines);
                    let expected_spaces = self.get_expected_spaces(list_type, is_multi_line);

                    if actual_spaces != expected_spaces {
                        let whitespace_start_pos = marker_end;
                        let whitespace_len = actual_spaces;

                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num_1based, line, whitespace_start_pos, whitespace_len);

                        let correct_spaces = " ".repeat(expected_spaces);
                        let line_start_byte = ctx.line_offsets.get(line_num).copied().unwrap_or(0);
                        let whitespace_start_byte = line_start_byte + whitespace_start_pos;
                        let whitespace_end_byte = whitespace_start_byte + whitespace_len;

                        let fix = Some(crate::rule::Fix {
                            range: whitespace_start_byte..whitespace_end_byte,
                            replacement: correct_spaces,
                        });

                        let message =
                            format!("Spaces after list markers (Expected: {expected_spaces}; Actual: {actual_spaces})");

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            severity: Severity::Warning,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message,
                            fix,
                        });
                    }
                }
            }
        }

        // Second pass: Detect list-like patterns the parser didn't recognize
        // This handles cases like "1.Text" where there's no space after the marker
        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip if already processed or in code block/front matter/math block
            if processed_lines.contains(&line_num) {
                continue;
            }
            if let Some(line_info) = ctx.lines.get(line_idx)
                && (line_info.in_code_block
                    || line_info.in_front_matter
                    || line_info.in_html_comment
                    || line_info.in_math_block)
            {
                continue;
            }

            // Skip indented code blocks
            if self.is_indented_code_block(line, line_idx, &lines) {
                continue;
            }

            // Try to detect list-like patterns using regex-based detection
            if let Some(warning) = self.check_unrecognized_list_marker(ctx, line, line_num, &lines) {
                warnings.push(warning);
            }
        }

        Ok(warnings)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if ctx.content.is_empty() {
            return true;
        }

        // Fast byte-level check for list markers (including ordered lists)
        let bytes = ctx.content.as_bytes();
        !bytes.contains(&b'*')
            && !bytes.contains(&b'-')
            && !bytes.contains(&b'+')
            && !bytes.iter().any(|&b| b.is_ascii_digit())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD030Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD030Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD030Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        let content = ctx.content;

        // Early return if no fixes needed
        if self.should_skip(ctx) {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip lines in code blocks, front matter, or HTML comments
            if let Some(line_info) = ctx.lines.get(line_idx)
                && (line_info.in_code_block || line_info.in_front_matter || line_info.in_html_comment)
            {
                result_lines.push(line.to_string());
                continue;
            }

            // Skip if this is an indented code block (4+ spaces with blank line before)
            if self.is_indented_code_block(line, line_idx, &lines) {
                result_lines.push(line.to_string());
                continue;
            }

            // Use regex-based detection to find list markers, not parser detection.
            // This ensures we fix spacing on ALL lines that look like list items,
            // even if the parser doesn't recognize them due to strict nesting rules.
            // User intention matters: if it looks like a list item, fix it.
            let is_multi_line = self.is_multi_line_list_item(ctx, line_num, &lines);
            if let Some(fixed_line) = self.try_fix_list_marker_spacing_with_context(line, is_multi_line) {
                result_lines.push(fixed_line);
            } else {
                result_lines.push(line.to_string());
            }
        }

        // Preserve trailing newline if original content had one
        let result = result_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }
}

impl MD030ListMarkerSpace {
    /// Check if a list item line has content after the marker
    /// Returns false if the line ends after the marker (with optional whitespace)
    /// MD030 only applies when there IS content on the same line as the marker
    #[inline]
    fn has_content_after_marker(line: &str, marker_end: usize) -> bool {
        if marker_end >= line.len() {
            return false;
        }
        !line[marker_end..].trim().is_empty()
    }

    /// Check if a list item is multi-line (spans multiple lines or contains nested content)
    fn is_multi_line_list_item(&self, ctx: &crate::lint_context::LintContext, line_num: usize, lines: &[&str]) -> bool {
        // Get the current list item info
        let current_line_info = match ctx.line_info(line_num) {
            Some(info) if info.list_item.is_some() => info,
            _ => return false,
        };

        let current_list = current_line_info.list_item.as_ref().unwrap();

        // Check subsequent lines to see if they are continuation of this list item
        for next_line_num in (line_num + 1)..=lines.len() {
            if let Some(next_line_info) = ctx.line_info(next_line_num) {
                // If we encounter another list item at the same or higher level, this item is done
                if let Some(next_list) = &next_line_info.list_item {
                    if next_list.marker_column <= current_list.marker_column {
                        break; // Found the next list item at same/higher level
                    }
                    // If there's a nested list item, this is multi-line
                    return true;
                }

                // If we encounter a non-empty line that's not indented enough to be part of this list item,
                // this list item is done
                let line_content = lines.get(next_line_num - 1).unwrap_or(&"");
                if !line_content.trim().is_empty() {
                    // Get blockquote level from the current list item's line
                    let bq_level = current_line_info
                        .blockquote
                        .as_ref()
                        .map(|bq| bq.nesting_level)
                        .unwrap_or(0);

                    // For blockquote lists, min continuation indent is just the marker width
                    // (not the full content_column which includes blockquote prefix)
                    let min_continuation_indent = if bq_level > 0 {
                        // For lists in blockquotes, use marker width (2 for "* " or "- ")
                        // content_column includes blockquote prefix, so subtract that
                        current_list.content_column.saturating_sub(
                            current_line_info
                                .blockquote
                                .as_ref()
                                .map(|bq| bq.prefix.len())
                                .unwrap_or(0),
                        )
                    } else {
                        current_list.content_column
                    };

                    // Calculate effective indent (blockquote-aware)
                    let raw_indent = line_content.len() - line_content.trim_start().len();
                    let actual_indent = effective_indent_in_blockquote(line_content, bq_level, raw_indent);

                    if actual_indent < min_continuation_indent {
                        break; // Line is not indented enough to be part of this list item
                    }

                    // If we find a continuation line, this is multi-line
                    if actual_indent >= min_continuation_indent {
                        return true;
                    }
                }

                // Empty lines don't affect the multi-line status by themselves
            }
        }

        false
    }

    /// Helper to fix marker spacing for both ordered and unordered lists
    fn fix_marker_spacing(
        &self,
        marker: &str,
        after_marker: &str,
        indent: &str,
        is_multi_line: bool,
        is_ordered: bool,
    ) -> Option<String> {
        // MD030 only fixes multiple spaces, not tabs
        // Tabs are handled by MD010 (no-hard-tabs), matching markdownlint behavior
        // Skip if the spacing starts with a tab
        if after_marker.starts_with('\t') {
            return None;
        }

        // Calculate expected spacing based on list type and context
        let expected_spaces = if is_ordered {
            if is_multi_line {
                self.config.ol_multi.get()
            } else {
                self.config.ol_single.get()
            }
        } else if is_multi_line {
            self.config.ul_multi.get()
        } else {
            self.config.ul_single.get()
        };

        // Case 1: No space after marker (content directly follows marker)
        // User intention: they meant to write a list item but forgot the space
        if !after_marker.is_empty() && !after_marker.starts_with(' ') {
            let spaces = " ".repeat(expected_spaces);
            return Some(format!("{indent}{marker}{spaces}{after_marker}"));
        }

        // Case 2: Multiple spaces after marker
        if after_marker.starts_with("  ") {
            let content = after_marker.trim_start_matches(' ');
            if !content.is_empty() {
                let spaces = " ".repeat(expected_spaces);
                return Some(format!("{indent}{marker}{spaces}{content}"));
            }
        }

        // Case 3: Single space after marker but expected spacing differs
        // This handles custom configurations like ul_single = 3
        if after_marker.starts_with(' ') && !after_marker.starts_with("  ") && expected_spaces != 1 {
            let content = &after_marker[1..]; // Skip the single space
            if !content.is_empty() {
                let spaces = " ".repeat(expected_spaces);
                return Some(format!("{indent}{marker}{spaces}{content}"));
            }
        }

        None
    }

    /// Fix list marker spacing with context - handles tabs, multiple spaces, and mixed whitespace
    fn try_fix_list_marker_spacing_with_context(&self, line: &str, is_multi_line: bool) -> Option<String> {
        // Extract blockquote prefix if present
        let (blockquote_prefix, content) = Self::strip_blockquote_prefix(line);

        let trimmed = content.trim_start();
        let indent = &content[..content.len() - trimmed.len()];

        // Check for unordered list markers - only fix multiple-space issues, not missing-space
        // Unordered markers (*, -, +) have too many non-list uses to apply heuristic fixing
        for marker in &["*", "-", "+"] {
            if let Some(after_marker) = trimmed.strip_prefix(marker) {
                // Skip emphasis patterns (**, --, ++)
                if after_marker.starts_with(*marker) {
                    break;
                }

                // Skip if this looks like emphasis: *text* or _text_
                if *marker == "*" && after_marker.contains('*') {
                    break;
                }

                // Fix if there's a space (handling both multiple spaces and single space with non-default config)
                // Don't add spaces where there are none - too ambiguous for unordered markers
                if after_marker.starts_with(' ')
                    && let Some(fixed) = self.fix_marker_spacing(marker, after_marker, indent, is_multi_line, false)
                {
                    return Some(format!("{blockquote_prefix}{fixed}"));
                }
                break; // Found a marker, don't check others
            }
        }

        // Check for ordered list markers
        if let Some(dot_pos) = trimmed.find('.') {
            let before_dot = &trimmed[..dot_pos];
            if before_dot.chars().all(|c| c.is_ascii_digit()) && !before_dot.is_empty() {
                let after_dot = &trimmed[dot_pos + 1..];

                // Skip empty items
                if after_dot.is_empty() {
                    return None;
                }

                // For NO-SPACE case (content directly after dot), apply "clear user intent" filter
                if !after_dot.starts_with(' ') && !after_dot.starts_with('\t') {
                    let first_char = after_dot.chars().next().unwrap_or(' ');

                    // Skip decimal numbers: 3.14, 2.5, etc.
                    if first_char.is_ascii_digit() {
                        return None;
                    }

                    // For CLEAR user intent, only fix if:
                    // 1. Starts with uppercase letter (strong list indicator), OR
                    // 2. Starts with [ or ( (link/paren content)
                    let is_clear_intent = first_char.is_ascii_uppercase() || first_char == '[' || first_char == '(';

                    if !is_clear_intent {
                        return None;
                    }
                }
                // For items with spaces (including multiple spaces), always let fix_marker_spacing handle it

                let marker = format!("{before_dot}.");
                if let Some(fixed) = self.fix_marker_spacing(&marker, after_dot, indent, is_multi_line, true) {
                    return Some(format!("{blockquote_prefix}{fixed}"));
                }
            }
        }

        None
    }

    /// Strip blockquote prefix from a line, returning (prefix, content)
    fn strip_blockquote_prefix(line: &str) -> (String, &str) {
        let mut prefix = String::new();
        let mut remaining = line;

        loop {
            let trimmed = remaining.trim_start();
            if !trimmed.starts_with('>') {
                break;
            }
            // Add leading spaces to prefix
            let leading_spaces = remaining.len() - trimmed.len();
            prefix.push_str(&remaining[..leading_spaces]);
            prefix.push('>');
            remaining = &trimmed[1..];

            // Handle optional space after >
            if remaining.starts_with(' ') {
                prefix.push(' ');
                remaining = &remaining[1..];
            }
        }

        (prefix, remaining)
    }

    /// Detect list-like patterns that the parser didn't recognize (e.g., "1.Text" with no space)
    /// This implements user-intention-based detection: if it looks like a list item, flag it
    fn check_unrecognized_list_marker(
        &self,
        ctx: &crate::lint_context::LintContext,
        line: &str,
        line_num: usize,
        lines: &[&str],
    ) -> Option<LintWarning> {
        // Strip blockquote prefix to analyze the content
        let (_blockquote_prefix, content) = Self::strip_blockquote_prefix(line);

        let trimmed = content.trim_start();
        let indent_len = content.len() - trimmed.len();

        // Note: We intentionally do NOT apply heuristic detection to unordered list markers
        // (*, -, +) because they have too many non-list uses: emphasis, globs, diffs, etc.
        // The parser handles valid unordered list items; we only do heuristic detection
        // for ordered lists where "1.Text" is almost always a list item with missing space.

        // Check for ordered list markers (digits followed by .) without proper spacing
        if let Some(dot_pos) = trimmed.find('.') {
            let before_dot = &trimmed[..dot_pos];
            if before_dot.chars().all(|c| c.is_ascii_digit()) && !before_dot.is_empty() {
                let after_dot = &trimmed[dot_pos + 1..];
                // Only flag if there's content directly after the marker (no space, no tab)
                if !after_dot.is_empty() && !after_dot.starts_with(' ') && !after_dot.starts_with('\t') {
                    let first_char = after_dot.chars().next().unwrap_or(' ');

                    // For CLEAR user intent, only flag if:
                    // 1. Starts with uppercase letter (strong list indicator), OR
                    // 2. Starts with [ or ( (link/paren content)
                    // Lowercase and digits are ambiguous (could be decimal, version, etc.)
                    let is_clear_intent = first_char.is_ascii_uppercase() || first_char == '[' || first_char == '(';

                    if is_clear_intent {
                        let is_multi_line = self.is_multi_line_for_unrecognized(line_num, lines);
                        let expected_spaces = self.get_expected_spaces(ListType::Ordered, is_multi_line);

                        let marker = format!("{before_dot}.");
                        let marker_pos = indent_len;
                        let marker_end = marker_pos + marker.len();

                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num, line, marker_end, 0);

                        let correct_spaces = " ".repeat(expected_spaces);
                        let line_start_byte = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                        let fix_position = line_start_byte + marker_end;

                        return Some(LintWarning {
                            rule_name: Some("MD030".to_string()),
                            severity: Severity::Warning,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Spaces after list markers (Expected: {expected_spaces}; Actual: 0)"),
                            fix: Some(crate::rule::Fix {
                                range: fix_position..fix_position,
                                replacement: correct_spaces,
                            }),
                        });
                    }
                }
            }
        }

        None
    }

    /// Simplified multi-line check for unrecognized list items
    fn is_multi_line_for_unrecognized(&self, line_num: usize, lines: &[&str]) -> bool {
        // For unrecognized list items, we can't rely on parser info
        // Check if the next line exists and appears to be a continuation
        if line_num < lines.len() {
            let next_line = lines[line_num]; // line_num is 1-based, so this is the next line
            let next_trimmed = next_line.trim();
            // If next line is non-empty and indented, it might be a continuation
            if !next_trimmed.is_empty() && next_line.starts_with(' ') {
                return true;
            }
        }
        false
    }

    /// Check if a line is part of an indented code block (4+ columns with blank line before)
    fn is_indented_code_block(&self, line: &str, line_idx: usize, lines: &[&str]) -> bool {
        // Must have 4+ columns of indentation (accounting for tab expansion)
        if ElementCache::calculate_indentation_width_default(line) < 4 {
            return false;
        }

        // If it's the first line, it's not an indented code block
        if line_idx == 0 {
            return false;
        }

        // Check if there's a blank line before this line or before the start of the indented block
        if self.has_blank_line_before_indented_block(line_idx, lines) {
            return true;
        }

        false
    }

    /// Check if there's a blank line before the start of an indented block
    fn has_blank_line_before_indented_block(&self, line_idx: usize, lines: &[&str]) -> bool {
        // Walk backwards to find the start of the indented block
        let mut current_idx = line_idx;

        // Find the first line in this indented block
        while current_idx > 0 {
            let current_line = lines[current_idx];
            let prev_line = lines[current_idx - 1];

            // If current line is not indented (< 4 columns), we've gone too far
            if ElementCache::calculate_indentation_width_default(current_line) < 4 {
                break;
            }

            // If previous line is not indented, check if it's blank
            if ElementCache::calculate_indentation_width_default(prev_line) < 4 {
                return prev_line.trim().is_empty();
            }

            current_idx -= 1;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Expect warnings for lines with too many spaces after the marker
        assert_eq!(
            result.len(),
            2,
            "Should flag lines with too many spaces after list marker"
        );
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:")
                    && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_nested_emphasis_not_flagged_issue_278() {
        // Issue #278: Nested emphasis like *text **bold** more* should not trigger MD030
        let rule = MD030ListMarkerSpace::default();

        // This is emphasis with nested bold - NOT a list item
        let content = "*This text is **very** important*";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Nested emphasis should not trigger MD030, got: {result:?}"
        );

        // Simple emphasis - NOT a list item
        let content2 = "*Hello World*";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Simple emphasis should not trigger MD030, got: {result2:?}"
        );

        // Bold text - NOT a list item
        let content3 = "**bold text**";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert!(
            result3.is_empty(),
            "Bold text should not trigger MD030, got: {result3:?}"
        );

        // Bold+italic - NOT a list item
        let content4 = "***bold and italic***";
        let ctx4 = LintContext::new(content4, crate::config::MarkdownFlavor::Standard, None);
        let result4 = rule.check(&ctx4).unwrap();
        assert!(
            result4.is_empty(),
            "Bold+italic should not trigger MD030, got: {result4:?}"
        );

        // Actual list item with proper spacing - should NOT trigger
        let content5 = "* Item with space";
        let ctx5 = LintContext::new(content5, crate::config::MarkdownFlavor::Standard, None);
        let result5 = rule.check(&ctx5).unwrap();
        assert!(
            result5.is_empty(),
            "Properly spaced list item should not trigger MD030, got: {result5:?}"
        );
    }

    #[test]
    fn test_empty_marker_line_not_flagged_issue_288() {
        // Issue #288: List items with no content on the marker line should not trigger MD030
        // The space requirement only applies when there IS content after the marker
        let rule = MD030ListMarkerSpace::default();

        // Case 1: Unordered list with empty marker line followed by code block
        let content = "-\n    ```python\n    print(\"code\")\n    ```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty unordered marker line with code continuation should not trigger MD030, got: {result:?}"
        );

        // Case 2: Ordered list with empty marker line followed by code block
        let content = "1.\n    ```python\n    print(\"code\")\n    ```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty ordered marker line with code continuation should not trigger MD030, got: {result:?}"
        );

        // Case 3: Empty marker line followed by paragraph continuation
        let content = "-\n    This is a paragraph continuation\n    of the list item.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty marker line with paragraph continuation should not trigger MD030, got: {result:?}"
        );

        // Case 4: Nested list with empty marker line
        let content = "- Parent item\n  -\n      Nested content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Nested empty marker line should not trigger MD030, got: {result:?}"
        );

        // Case 5: Multiple list items, some with empty markers
        let content = "- Item with content\n-\n    Code block\n- Another item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Mixed empty/non-empty marker lines should not trigger MD030 for empty ones, got: {result:?}"
        );
    }

    #[test]
    fn test_marker_with_content_still_flagged_issue_288() {
        // Ensure we still flag markers with content but wrong spacing
        let rule = MD030ListMarkerSpace::default();

        // Two spaces before content - should flag
        let content = "-  Two spaces before content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Two spaces after unordered marker should still trigger MD030"
        );

        // Ordered list with two spaces - should flag
        let content = "1.  Two spaces\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Two spaces after ordered marker should still trigger MD030"
        );

        // Normal list item - should NOT flag
        let content = "- Normal item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Normal list item should not trigger MD030, got: {result:?}"
        );
    }

    #[test]
    fn test_has_content_after_marker() {
        // Direct unit tests for the helper function
        assert!(!MD030ListMarkerSpace::has_content_after_marker("-", 1));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("- ", 1));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("-   ", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("- item", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("-  item", 1));
        assert!(MD030ListMarkerSpace::has_content_after_marker("1. item", 2));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("1.", 2));
        assert!(!MD030ListMarkerSpace::has_content_after_marker("1. ", 2));
    }
}
