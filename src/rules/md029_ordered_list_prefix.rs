/// Rule MD029: Ordered list item prefix
///
/// See [docs/md029.md](../../docs/md029.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

lazy_static! {
    static ref ORDERED_LIST_ITEM_REGEX: Regex = Regex::new(r"^(\s*)\d+\.\s").unwrap();
    static ref LIST_NUMBER_REGEX: Regex = Regex::new(r"^\s*(\d+)\.\s").unwrap();
    static ref FIX_LINE_REGEX: Regex = Regex::new(r"^(\s*)\d+(\.\s.*)$").unwrap();
}

/// Represents the style for ordered lists
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ListStyle {
    One,      // Use '1.' for all items
    OneOne,   // All ones (1. 1. 1.)
    Ordered,  // Sequential (1. 2. 3.)
    Ordered0, // Zero-based (0. 1. 2.)
}

#[derive(Debug, Clone)]
pub struct MD029OrderedListPrefix {
    pub style: ListStyle,
}

impl Default for MD029OrderedListPrefix {
    fn default() -> Self {
        Self {
            style: ListStyle::Ordered,
        }
    }
}

impl MD029OrderedListPrefix {
    pub fn new(style: ListStyle) -> Self {
        Self { style }
    }

    #[inline]
    fn get_list_number(line: &str) -> Option<usize> {
        LIST_NUMBER_REGEX
            .captures(line)
            .and_then(|cap| cap[1].parse::<usize>().ok())
    }

    #[inline]
    fn parse_marker_number(marker: &str) -> Option<usize> {
        // Handle marker format like "1." or "1"
        let num_part = if let Some(stripped) = marker.strip_suffix('.') {
            stripped
        } else {
            marker
        };
        num_part.parse::<usize>().ok()
    }

    #[inline]
    fn get_expected_number(&self, index: usize) -> usize {
        match self.style {
            ListStyle::One => 1,
            ListStyle::OneOne => 1,
            ListStyle::Ordered => index + 1,
            ListStyle::Ordered0 => index,
        }
    }

    #[inline]
    fn fix_line(&self, line: &str, expected_num: usize) -> String {
        FIX_LINE_REGEX
            .replace(line, format!("${{1}}{}{}", expected_num, "$2"))
            .to_string()
    }
}

impl Rule for MD029OrderedListPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list marker value"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early returns for performance
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any ordered list markers before processing
        if !ctx.content.contains('.') || !ctx.content.lines().any(|line| ORDERED_LIST_ITEM_REGEX.is_match(line)) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Group ordered list blocks that are only separated by code blocks
        // This handles cases where the centralized system splits lists that should be continuous
        let ordered_blocks: Vec<_> = ctx.list_blocks.iter().filter(|block| block.is_ordered).collect();

        if ordered_blocks.is_empty() {
            return Ok(Vec::new());
        }

        // Group consecutive list blocks that should be treated as continuous
        let mut block_groups = Vec::new();
        let mut current_group = vec![ordered_blocks[0]];

        for i in 1..ordered_blocks.len() {
            let prev_block = ordered_blocks[i - 1];
            let current_block = ordered_blocks[i];

            // Check if there are only code blocks/fences between these list blocks
            let between_content_is_code_only =
                self.is_only_code_between_blocks(ctx, prev_block.end_line, current_block.start_line);

            if between_content_is_code_only {
                // Treat as continuation of the same logical list
                current_group.push(current_block);
            } else {
                // Start a new list group
                block_groups.push(current_group);
                current_group = vec![current_block];
            }
        }
        block_groups.push(current_group);

        // Process each group of blocks as a continuous list
        for group in block_groups {
            self.check_ordered_list_group(ctx, &group, &mut warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut result = String::new();
        let mut indent_stack: Vec<(usize, usize)> = Vec::new(); // (indent, index)
        let lines: Vec<&str> = content.lines().collect();
        let mut byte_pos = 0;

        for line in lines.iter() {
            // Skip if in code block
            if ctx.is_in_code_block_or_span(byte_pos) {
                result.push_str(line);
                result.push('\n');
                byte_pos += line.len() + 1;
                continue;
            }
            if Self::get_list_number(line).is_some() {
                let indent = line.chars().take_while(|c| c.is_whitespace()).count();
                // Pop stack if current indent is less than stack top
                while let Some(&(top_indent, _)) = indent_stack.last() {
                    if indent < top_indent {
                        indent_stack.pop();
                    } else {
                        break;
                    }
                }
                // If indent matches stack top, increment index
                if let Some(&mut (top_indent, ref mut idx)) = indent_stack.last_mut() {
                    if indent == top_indent {
                        let expected_num = self.get_expected_number(*idx);
                        let fixed_line = self.fix_line(line, expected_num);
                        result.push_str(&fixed_line);
                        result.push('\n');
                        *idx += 1;
                        continue;
                    }
                }
                // New deeper indent or first item
                indent_stack.push((indent, 0));
                let expected_num = self.get_expected_number(0);
                let fixed_line = self.fix_line(line, expected_num);
                result.push_str(&fixed_line);
                result.push('\n');
                // Increment the new top
                if let Some(&mut (_, ref mut idx)) = indent_stack.last_mut() {
                    *idx += 1;
                }
            } else if !line.trim().is_empty() {
                // Check if this is a code fence line - don't break the list for these
                let trimmed = line.trim();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    // Code fence lines don't break the list
                    result.push_str(line);
                    result.push('\n');
                } else {
                    // Check if the line is indented enough to be part of a list item
                    let line_indent = line.chars().take_while(|c| c.is_whitespace()).count();
                    let is_continuation = indent_stack
                        .last()
                        .map(|&(list_indent, _)| line_indent >= list_indent + 3)
                        .unwrap_or(false);

                    if is_continuation {
                        // This line is part of the list item (indented continuation)
                        result.push_str(line);
                        result.push('\n');
                    } else {
                        // Non-list, non-blank line breaks the list
                        indent_stack.clear();
                        result.push_str(line);
                        result.push('\n');
                    }
                }
            } else {
                // Blank line - don't clear the stack, as lists can have blank lines within them
                result.push_str(line);
                result.push('\n');
            }

            byte_pos += line.len() + 1;
        }
        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }
        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return if no lists
        if structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check if there are no ordered lists
        if !content.contains('1') || (!content.contains("1.") && !content.contains("2.") && !content.contains("0.")) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut list_items = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Create a set of list line indices for faster lookup
        let mut list_line_set = std::collections::HashSet::new();
        for &line_num in &structure.list_lines {
            list_line_set.insert(line_num); // Keep as 1-indexed for easier comparison
        }

        // Group ordered list items into sections
        let mut in_list = false;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed

            // Skip lines in code blocks
            if structure.is_in_code_block(line_num) {
                // Code blocks don't break the list - just skip them
                continue;
            }

            if list_line_set.contains(&line_num) {
                if Self::get_list_number(line).is_some() {
                    // If this is the first item of a new list, record the list start
                    if !in_list {
                        in_list = true;
                    }

                    list_items.push((line_idx, line.to_string()));
                }
            } else if !line.trim().is_empty() {
                // Check if this is a code fence line - don't break the list for these
                let trimmed = line.trim();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    // Code fence lines don't break the list - just skip them
                    continue;
                }

                // Non-empty, non-list line breaks the list
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings, content);
                    list_items.clear();
                    in_list = false;
                }
            }
        }

        // Check last section if it exists
        if !list_items.is_empty() {
            self.check_list_section(&list_items, &mut warnings, content);
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty()
            || !content.contains('1')
            || (!content.contains("1.") && !content.contains("2.") && !content.contains("0."))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style_str = crate::config::get_rule_config_value::<String>(config, "MD029", "style")
            .unwrap_or_else(|| "ordered".to_string());
        let style = match style_str.as_str() {
            "one" => ListStyle::One,
            "one_one" => ListStyle::OneOne,
            "ordered0" => ListStyle::Ordered0,
            _ => ListStyle::Ordered,
        };
        Box::new(MD029OrderedListPrefix::new(style))
    }
}

impl DocumentStructureExtensions for MD029OrderedListPrefix {
    fn has_relevant_elements(&self, ctx: &crate::lint_context::LintContext, doc_structure: &DocumentStructure) -> bool {
        let content = ctx.content;
        // This rule is only relevant if there are list items AND they might be ordered lists
        !doc_structure.list_lines.is_empty()
            && (content.contains("1.") || content.contains("2.") || content.contains("0."))
    }
}

impl MD029OrderedListPrefix {
    /// Check if there's only code blocks/fences between two list blocks
    fn is_only_code_between_blocks(
        &self,
        ctx: &crate::lint_context::LintContext,
        end_line: usize,
        start_line: usize,
    ) -> bool {
        if end_line >= start_line {
            return false;
        }

        for line_num in (end_line + 1)..start_line {
            if let Some(line_info) = ctx.line_info(line_num) {
                let trimmed = line_info.content.trim();

                // Skip empty lines
                if trimmed.is_empty() {
                    continue;
                }

                // If in code block, it's fine
                if line_info.in_code_block {
                    continue;
                }

                // If this is a code fence line, it's fine
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    continue;
                }

                // Any other non-empty content means lists are truly separated
                return false;
            }
        }

        true
    }

    /// Check a group of ordered list blocks that should be treated as continuous
    fn check_ordered_list_group(
        &self,
        ctx: &crate::lint_context::LintContext,
        group: &[&crate::lint_context::ListBlock],
        warnings: &mut Vec<LintWarning>,
    ) {
        // Collect all items from all blocks in the group
        let mut all_items = Vec::new();

        for list_block in group {
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line) {
                    if let Some(list_item) = &line_info.list_item {
                        // Skip unordered lists (safety check)
                        if !list_item.is_ordered {
                            continue;
                        }
                        all_items.push((item_line, line_info, list_item));
                    }
                }
            }
        }

        // Sort by line number to ensure correct order
        all_items.sort_by_key(|(line_num, _, _)| *line_num);

        // Group items by indentation level and process each level independently
        let mut level_groups: std::collections::HashMap<
            usize,
            Vec<(
                usize,
                &crate::lint_context::LineInfo,
                &crate::lint_context::ListItemInfo,
            )>,
        > = std::collections::HashMap::new();

        for (line_num, line_info, list_item) in all_items {
            // Group by marker column (indentation level)
            level_groups
                .entry(list_item.marker_column)
                .or_default()
                .push((line_num, line_info, list_item));
        }

        // Process each indentation level separately
        for (_indent, mut group) in level_groups {
            // Sort by line number to ensure correct order
            group.sort_by_key(|(line_num, _, _)| *line_num);

            // Check each item in the group for correct sequence
            for (idx, (line_num, line_info, list_item)) in group.iter().enumerate() {
                // Parse the actual number from the marker (e.g., "1." -> 1)
                if let Some(actual_num) = Self::parse_marker_number(&list_item.marker) {
                    let expected_num = self.get_expected_number(idx);

                    if actual_num != expected_num {
                        // Calculate byte position for the fix
                        let marker_start = line_info.byte_offset + list_item.marker_column;
                        let number_len = actual_num.to_string().len();

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!(
                                "Ordered list item number {actual_num} does not match style (expected {expected_num})"
                            ),
                            line: *line_num,
                            column: list_item.marker_column + 1,
                            end_line: *line_num,
                            end_column: list_item.marker_column + number_len + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: marker_start..marker_start + number_len,
                                replacement: expected_num.to_string(),
                            }),
                        });
                    }
                }
            }
        }
    }

    fn check_list_section(&self, items: &[(usize, String)], warnings: &mut Vec<LintWarning>, content: &str) {
        // Group items by indentation level and process each level independently
        let mut level_groups: std::collections::HashMap<usize, Vec<(usize, String)>> = std::collections::HashMap::new();

        for (line_num, line) in items {
            let indent = line.chars().take_while(|c| c.is_whitespace()).count();
            level_groups.entry(indent).or_default().push((*line_num, line.clone()));
        }

        // Process each indentation level separately
        for (_indent, mut group) in level_groups {
            // Sort by line number to ensure correct order
            group.sort_by_key(|(line_num, _)| *line_num);

            // Check each item in the group for correct sequence
            for (idx, (line_num, line)) in group.iter().enumerate() {
                if let Some(actual_num) = Self::get_list_number(line) {
                    let expected_num = self.get_expected_number(idx);

                    if actual_num != expected_num {
                        // Create a LineIndex for the actual content
                        let line_index = LineIndex::new(content.to_string());

                        // Find the number position in the line for precise replacement
                        let number_start = line.find(char::is_numeric).unwrap_or(0);
                        let number_len = actual_num.to_string().len();

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!(
                                "Ordered list item number {actual_num} does not match style (expected {expected_num})"
                            ),
                            line: line_num + 1,
                            column: number_start + 1,
                            end_line: line_num + 1,
                            end_column: number_start + number_len + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(
                                    line_num + 1,
                                    number_start + 1,
                                    number_len,
                                ),
                                replacement: expected_num.to_string(),
                            }),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_with_document_structure() {
        // Test with default style (ordered)
        let rule = MD029OrderedListPrefix::default();

        // Test with correctly ordered list
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());

        // Test with incorrectly ordered list
        let content = "1. First item\n3. Third item\n5. Fifth item";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 5

        // Test with one-one style
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 2 and 3

        // Test with ordered0 style
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. First item\n1. Second item\n2. Third item";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_redundant_computation_fix() {
        // This test confirms that the redundant computation bug is fixed
        // Previously: get_list_number() was called twice (once for is_some(), once for unwrap())
        // Now: get_list_number() is called once with if let pattern

        let rule = MD029OrderedListPrefix::default();

        // Test with mixed valid and edge case content
        let content = "1. First item\n3. Wrong number\n2. Another wrong number";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);

        // This should not panic and should produce warnings for incorrect numbering
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 2

        // Verify the warnings have correct content
        assert!(result[0].message.contains("3 does not match style (expected 2)"));
        assert!(result[1].message.contains("2 does not match style (expected 3)"));
    }

    #[test]
    fn test_performance_improvement() {
        // This test verifies that the fix improves performance by avoiding redundant calls
        let rule = MD029OrderedListPrefix::default();

        // Create a larger list to test performance
        let mut content = String::new();
        for i in 1..=100 {
            content.push_str(&format!("{}. Item {}\n", i + 1, i)); // All wrong numbers
        }

        let structure = DocumentStructure::new(&content);
        let ctx = crate::lint_context::LintContext::new(&content);

        // This should complete without issues and produce warnings for all items
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 100); // Should have warnings for all 100 items

        // Verify first and last warnings
        assert!(result[0].message.contains("2 does not match style (expected 1)"));
        assert!(result[99].message.contains("101 does not match style (expected 100)"));
    }
}
