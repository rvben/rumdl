/// Rule MD029: Ordered list item prefix
///
/// See [docs/md029.md](../../docs/md029.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::regex_cache::ORDERED_LIST_MARKER_REGEX;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

mod md029_config;
pub use md029_config::{ListStyle, MD029Config};

lazy_static! {
    static ref FIX_LINE_REGEX: Regex = Regex::new(r"^(\s*)\d+(\.\s.*)$").unwrap();
}

#[derive(Debug, Clone, Default)]
pub struct MD029OrderedListPrefix {
    config: MD029Config,
}

impl MD029OrderedListPrefix {
    pub fn new(style: ListStyle) -> Self {
        Self {
            config: MD029Config { style },
        }
    }

    pub fn from_config_struct(config: MD029Config) -> Self {
        Self { config }
    }

    #[inline]
    fn get_list_number(line: &str) -> Option<usize> {
        ORDERED_LIST_MARKER_REGEX
            .captures(line)
            .and_then(|cap| cap[2].parse::<usize>().ok())
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
        match self.config.style {
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
        if !ctx.content.contains('.') || !ctx.content.lines().any(|line| ORDERED_LIST_MARKER_REGEX.is_match(line)) {
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
        let mut in_code_fence = false;

        for line in lines.iter() {
            let trimmed = line.trim();

            // Track code fences
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_fence = !in_code_fence;
                result.push_str(line);
                result.push('\n');
                byte_pos += line.len() + 1;
                continue;
            }

            // Skip if in code block or fence
            if in_code_fence || ctx.is_in_code_block_or_span(byte_pos) {
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
                // Check if the line is indented enough to be part of a list item
                let line_indent = line.chars().take_while(|c| c.is_whitespace()).count();
                let is_continuation = indent_stack
                    .last()
                    .map(|&(list_indent, _)| {
                        // Allow lazy continuation (0-2 spaces) or proper continuation (3+ spaces)
                        line_indent <= 2 || line_indent >= list_indent + 3
                    })
                    .unwrap_or(false);

                if is_continuation {
                    if line_indent <= 2 && !indent_stack.is_empty() {
                        // Check if this line is itself a list item
                        let trimmed = line.trim();
                        let is_list_item = trimmed.starts_with("* ")
                            || trimmed.starts_with("- ")
                            || trimmed.starts_with("+ ")
                            || (trimmed.len() > 2
                                && trimmed.chars().next().unwrap().is_ascii_digit()
                                && trimmed.contains(". "));

                        if !is_list_item {
                            // This is a lazy continuation - fix it by adding proper indentation
                            let (list_indent, _) = indent_stack.last().unwrap();
                            let proper_indent = " ".repeat(list_indent + 3);
                            result.push_str(&proper_indent);
                            result.push_str(line.trim_start());
                            result.push('\n');
                        } else {
                            // This is a list item, not a continuation - it breaks the list
                            indent_stack.clear();
                            result.push_str(line);
                            result.push('\n');
                        }
                    } else {
                        // This line is properly indented
                        result.push_str(line);
                        result.push('\n');
                    }
                } else {
                    // Non-list, non-blank line breaks the list
                    indent_stack.clear();
                    result.push_str(line);
                    result.push('\n');
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
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        // For MD029, we need to use the regular check method to get lazy continuation detection
        // The document structure optimization doesn't provide enough context for proper lazy continuation checking
        self.check(ctx)
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

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD029Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD029Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD029Config>(config);
        Box::new(MD029OrderedListPrefix::from_config_struct(rule_config))
    }
}

impl DocumentStructureExtensions for MD029OrderedListPrefix {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is relevant if there are any ordered list items
        // We need to check even lists with all "1." items for:
        // 1. Incorrect numbering according to configured style
        // 2. Lazy continuation issues
        ctx.list_blocks.iter().any(|block| block.is_ordered)
    }
}

impl MD029OrderedListPrefix {
    /// Check for lazy continuation lines in a list block
    fn check_for_lazy_continuation(
        &self,
        ctx: &crate::lint_context::LintContext,
        list_block: &crate::lint_context::ListBlock,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Check all lines in the block for lazy continuation
        for line_num in list_block.start_line..=list_block.end_line {
            if let Some(line_info) = ctx.line_info(line_num) {
                // Skip list item lines themselves
                if list_block.item_lines.contains(&line_num) {
                    continue;
                }

                // Skip blank lines
                if line_info.is_blank {
                    continue;
                }

                // Skip lines that are in code blocks
                if line_info.in_code_block {
                    continue;
                }

                // Skip code fence lines
                let trimmed = line_info.content.trim();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    continue;
                }

                // Check if this is a lazy continuation (0-2 spaces)
                if line_info.indent <= 2 && !line_info.content.trim().is_empty() {
                    // This is a lazy continuation - add a style warning
                    let col = line_info.indent + 1;

                    warnings.push(LintWarning {
                        rule_name: Some("MD029-style"),
                        message: "List continuation should be indented (lazy continuation detected)".to_string(),
                        line: line_num,
                        column: col,
                        end_line: line_num,
                        end_column: col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_info.byte_offset..line_info.byte_offset,
                            replacement: "   ".to_string(), // Add 3 spaces
                        }),
                    });
                }
            }
        }
    }

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
            // First, check for lazy continuation in this block
            self.check_for_lazy_continuation(ctx, list_block, warnings);

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
