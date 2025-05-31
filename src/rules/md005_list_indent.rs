//!
//! Rule MD005: Inconsistent indentation for list items at the same level
//!
//! See [docs/md005.md](../../docs/md005.md) for full documentation, configuration, and examples.

use crate::utils::range_utils::{calculate_match_range, LineIndex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructure;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use toml;

lazy_static! {
    static ref LIST_MARKER_REGEX: Regex = Regex::new(r"^\d+[.)]").unwrap();
}

/// Rule MD005: Inconsistent indentation for list items at the same level
#[derive(Clone)]
pub struct MD005ListIndent;

impl MD005ListIndent {
    #[inline]
    fn get_list_marker_info(line: &str) -> Option<(usize, char, usize)> {
        // Early return for empty or whitespace-only lines
        if line.is_empty() || line.trim().is_empty() {
            return None;
        }

        let indentation = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();

        // Fast path check for unordered list markers
        if !trimmed.is_empty() {
            let first_char = trimmed.chars().next().unwrap();

            // Check for unordered list markers (* - +)
            if (first_char == '*' || first_char == '-' || first_char == '+')
                && trimmed.len() > 1
                && trimmed.chars().nth(1).map_or(false, |c| c.is_whitespace())
            {
                return Some((indentation, first_char, 1)); // 1 char marker
            }

            // Fast path check for ordered list markers (digits followed by . or ))
            if first_char.is_ascii_digit() {
                if let Some(marker_match) = LIST_MARKER_REGEX.find(trimmed) {
                    let marker_char = trimmed.chars().nth(marker_match.end() - 1).unwrap();
                    if trimmed.len() > marker_match.end()
                        && trimmed
                            .chars()
                            .nth(marker_match.end())
                            .map_or(false, |c| c.is_whitespace())
                    {
                        return Some((indentation, marker_char, marker_match.end()));
                    }
                }
            }
        }

        None
    }

    #[inline]
    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    // Determine the expected indentation for a list item at a specific level
    #[inline]
    fn get_expected_indent(level: usize) -> usize {
        if level == 1 {
            0 // Top level items should be at the start of the line
        } else {
            2 * (level - 1) // Nested items should be indented by 2 spaces per level
        }
    }

    // Determine if a line is a continuation of a list item
    #[inline]
    fn is_list_continuation(prev_line: &str, current_line: &str) -> bool {
        // Early return for empty lines
        if current_line.trim().is_empty() {
            return false;
        }

        // If the previous line is a list item and the current line has more indentation
        // but is not a list item itself, it's a continuation
        if let Some((prev_indent, _, _)) = Self::get_list_marker_info(prev_line) {
            let current_indent = current_line.len() - current_line.trim_start().len();
            return current_indent > prev_indent
                && Self::get_list_marker_info(current_line).is_none();
        }
        false
    }
}

impl Default for MD005ListIndent {
    fn default() -> Self {
        Self
    }
}

impl Rule for MD005ListIndent {
    fn name(&self) -> &'static str {
        "MD005"
    }

    fn description(&self) -> &'static str {
        "List indentation should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Early returns for common cases
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check to avoid processing files without list markers
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();

        // Early return if there are no lines
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Pre-compute code blocks to avoid repeated checks
        let mut in_code_blocks = vec![false; lines.len()];
        let mut in_block = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_block = !in_block;
            }
            in_code_blocks[i] = in_block;
        }

        // Maps to store indentation by level for each list
        let mut current_list_id = 0;
        let mut in_list = false;

        // First pass: collect all list items and their indentation
        let mut list_items = Vec::new();
        for (line_num, line) in lines.iter().enumerate() {
            // Skip blank lines and code blocks
            if Self::is_blank_line(line) || in_code_blocks[line_num] {
                continue;
            }

            // Check if this is a list item
            if let Some((indent, _marker, _)) = Self::get_list_marker_info(line) {
                // If the indent is 0, or this is the first list item, or much less indented
                // than the previous list item, consider it the start of a new list
                let is_new_list = !in_list
                    || indent == 0
                    || (list_items.last().map_or(false, |(_, prev_indent, _)| {
                        prev_indent > &0 && indent < prev_indent / 2
                    }));

                if is_new_list {
                    current_list_id += 1;
                    in_list = true;
                }

                list_items.push((line_num, indent, current_list_id));
            } else {
                // Not a list item - check if it's a continuation or something else
                if list_items.is_empty() || !in_list {
                    continue;
                }

                let (prev_line_num, _, _) = list_items.last().unwrap();
                if !Self::is_list_continuation(lines[*prev_line_num], line) {
                    in_list = false;
                }
            }
        }

        // Early return if no list items were found
        if list_items.is_empty() {
            return Ok(Vec::new());
        }

        // Second pass: determine levels for each list
        let mut list_level_map: HashMap<usize, HashMap<usize, usize>> = HashMap::new(); // list_id -> { indent -> level }
        let mut list_item_levels: Vec<(usize, usize, usize)> = Vec::new(); // (line_num, indent, level)

        for (line_num, indent, list_id) in &list_items {
            // Skip items in code blocks
            if in_code_blocks[*line_num] {
                continue;
            }

            // Get or create the indent->level map for this list
            let level_map = list_level_map.entry(*list_id).or_default();

            // If it's the first item in this list, it's level 1
            if level_map.is_empty() {
                level_map.insert(*indent, 1);
                list_item_levels.push((*line_num, *indent, 1));
                continue;
            }

            // Find the deepest previous level with an indentation less than this item
            let mut level = 1; // Default to top level
            let mut parent_indent = 0;

            for (prev_indent, prev_level) in level_map.iter() {
                if prev_indent < indent && (*prev_level >= level || *prev_indent > parent_indent) {
                    level = *prev_level + 1;
                    parent_indent = *prev_indent;
                } else if prev_indent == indent {
                    // Same indentation means same level
                    level = *prev_level;
                    break;
                }
            }

            level_map.insert(*indent, level);
            list_item_levels.push((*line_num, *indent, level));
        }

        // Third pass: check if indentation matches the expected level for each item
        for (line_num, indent, _list_id) in &list_items {
            // Skip items in code blocks
            if in_code_blocks[*line_num] {
                continue;
            }

            // Find level for this item
            let level = list_item_levels
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, lvl)| *lvl)
                .unwrap_or(1);

            let expected_indent = Self::get_expected_indent(level);

            if *indent != expected_indent {
                let inconsistent_message = format!(
                    "List item indentation is {} {}, expected {} for level {}",
                    indent,
                    if *indent == 1 { "space" } else { "spaces" },
                    expected_indent,
                    level
                );

                // Create a fixed version of the line with proper indentation
                let line = lines[*line_num];
                let trimmed = line.trim_start();
                let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);

                // Calculate precise character range for the incorrect indentation
                let (start_line, start_col, end_line, end_col) = if *indent > 0 {
                    // Highlight the incorrect indentation spaces
                    calculate_match_range(*line_num + 1, line, 1, *indent + 1)
                } else {
                    // No indentation, highlight position where indentation should be
                    calculate_match_range(*line_num + 1, line, 1, 1)
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: inconsistent_message,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement,
                    }),
                });
            }
        }

        // Check for consistency among items in the same level of the same list
        // Group list items by list_id and level
        let mut level_groups: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new(); // (list_id, level) -> [(line_num, indent)]

        for (line_num, indent, level) in &list_item_levels {
            let list_id = list_items
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);

            level_groups
                .entry((list_id, *level))
                .or_default()
                .push((*line_num, *indent));
        }

        // For each group, check for indentation consistency
        for ((_list_id, _level), items) in &level_groups {
            if items.len() <= 1 {
                continue; // Need at least 2 items to check consistency
            }

            // Get first item's indentation as the reference
            let reference_indent = items[0].1;

            // Check that all other items match this indentation
            for &(line_num, indent) in &items[1..] {
                if indent != reference_indent {
                    // Found inconsistent indentation
                    let inconsistent_message = format!(
                        "List item indentation is inconsistent with other items at the same level (found: {}, expected: {})",
                        indent, reference_indent
                    );

                    // Create a fixed version
                    let line = lines[line_num];
                    let trimmed = line.trim_start();
                    let replacement = format!("{}{}", " ".repeat(reference_indent), trimmed);

                    // Only add if we don't already have a warning for this line
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        // Calculate precise character range for the incorrect indentation
                        let (start_line, start_col, end_line, end_col) = if indent > 0 {
                            // Highlight the incorrect indentation spaces
                            calculate_match_range(line_num + 1, line, 1, indent + 1)
                        } else {
                            // No indentation, highlight position where indentation should be
                            calculate_match_range(line_num + 1, line, 1, 1)
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: inconsistent_message,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement,
                            }),
                        });
                    }
                }
            }
        }

        // Check for nested items with insufficient indentation
        for (line_num, indent, level) in &list_item_levels {
            if *level <= 1 {
                continue; // Not nested
            }

            // Find parent level items
            let list_id = list_items
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);

            // Get items at the parent level
            let parent_items = level_groups.get(&(list_id, level - 1));

            if let Some(parent_items) = parent_items {
                if parent_items.is_empty() {
                    continue;
                }

                // Find parent indentation
                let parent_indent = parent_items[0].1;

                // Child should be more indented
                if *indent <= parent_indent {
                    let message = format!(
                        "Nested list item should be more indented than parent (parent: {}, child: {})",
                        parent_indent, indent
                    );

                    // Create fix
                    let line = lines[*line_num];
                    let trimmed = line.trim_start();
                    let expected_indent = parent_indent + 2;
                    let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);

                    // Only add if we don't already have a warning
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        // Calculate precise character range for the incorrect indentation
                        let (start_line, start_col, end_line, end_col) = if *indent > 0 {
                            // Highlight the incorrect indentation spaces
                            calculate_match_range(*line_num + 1, line, 1, *indent + 1)
                        } else {
                            // No indentation, highlight position where indentation should be
                            calculate_match_range(*line_num + 1, line, 1, 1)
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement,
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // Early returns for common cases
        if content.is_empty() {
            return Ok(String::new());
        }

        // Quick check to avoid processing files without list markers
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(content.to_string());
        }

        // Get warnings from the check method
        let warnings = self.check(ctx)?;

        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Create a map of line numbers to replacements
        let mut line_replacements: HashMap<usize, String> = HashMap::new();
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                // Line number is 1-based in warnings but we need 0-based for array indexing
                let line_idx = warning.line - 1;
                line_replacements.insert(line_idx, fix.replacement.clone());
            }
        }

        // Apply replacements line by line
        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = Vec::with_capacity(lines.len());

        for (i, line) in lines.iter().enumerate() {
            if let Some(replacement) = line_replacements.get(&i) {
                fixed_lines.push(replacement.clone());
            } else {
                fixed_lines.push((*line).to_string());
            }
        }

        // Join the fixed lines, preserving the original ending
        let result = fixed_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty()
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        // Early return if no lists
        if structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check to avoid processing files with no list markers
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();

        // Early return if there are no lines
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Maps to store indentation by level for each list
        let mut current_list_id = 0;
        let mut in_list = false;

        // First pass: collect all list items and their indentation
        let mut list_items = Vec::new();

        // Process only list lines using structure.list_lines
        for &line_num in &structure.list_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Skip blank lines and code blocks
            if Self::is_blank_line(line) || structure.is_in_code_block(line_num) {
                continue;
            }

            // Check if this is a list item
            if let Some((indent, _marker, _)) = Self::get_list_marker_info(line) {
                // If the indent is 0, or this is the first list item, or much less indented
                // than the previous list item, consider it the start of a new list
                let is_new_list = !in_list
                    || indent == 0
                    || (list_items.last().map_or(false, |(_, prev_indent, _)| {
                        prev_indent > &0 && indent < prev_indent / 2
                    }));

                if is_new_list {
                    current_list_id += 1;
                    in_list = true;
                }

                list_items.push((line_idx, indent, current_list_id));
            } else {
                // Not a list item - check if it's a continuation or something else
                if list_items.is_empty() || !in_list {
                    continue;
                }

                let (prev_line_num, _, _) = list_items.last().unwrap();
                if !Self::is_list_continuation(lines[*prev_line_num], line) {
                    in_list = false;
                }
            }
        }

        // Early return if no list items were found
        if list_items.is_empty() {
            return Ok(Vec::new());
        }

        // Second pass: determine levels for each list
        let mut list_level_map: HashMap<usize, HashMap<usize, usize>> = HashMap::new(); // list_id -> { indent -> level }
        let mut list_item_levels: Vec<(usize, usize, usize)> = Vec::new(); // (line_num, indent, level)

        for (line_num, indent, list_id) in &list_items {
            // Skip items in code blocks
            if structure.is_in_code_block(*line_num + 1) {
                continue;
            }

            // Get or create the indent->level map for this list
            let level_map = list_level_map.entry(*list_id).or_default();

            // If it's the first item in this list, it's level 1
            if level_map.is_empty() {
                level_map.insert(*indent, 1);
                list_item_levels.push((*line_num, *indent, 1));
                continue;
            }

            // Find the deepest previous level with an indentation less than this item
            let mut level = 1; // Default to top level
            let mut parent_indent = 0;

            for (prev_indent, prev_level) in level_map.iter() {
                if prev_indent < indent && (*prev_level >= level || *prev_indent > parent_indent) {
                    level = *prev_level + 1;
                    parent_indent = *prev_indent;
                } else if prev_indent == indent {
                    // Same indentation means same level
                    level = *prev_level;
                    break;
                }
            }

            level_map.insert(*indent, level);
            list_item_levels.push((*line_num, *indent, level));
        }

        // Third pass: check if indentation matches the expected level for each item
        for (line_num, indent, _list_id) in &list_items {
            // Skip items in code blocks
            if structure.is_in_code_block(*line_num + 1) {
                continue;
            }

            // Find level for this item
            let level = list_item_levels
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, lvl)| *lvl)
                .unwrap_or(1);

            let expected_indent = Self::get_expected_indent(level);

            if *indent != expected_indent {
                let inconsistent_message = format!(
                    "List item indentation is {} {}, expected {} for level {}",
                    indent,
                    if *indent == 1 { "space" } else { "spaces" },
                    expected_indent,
                    level
                );

                // Create a fixed version of the line with proper indentation
                let line = lines[*line_num];
                let trimmed = line.trim_start();
                let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num + 1,
                    column: 1,
                    end_line: line_num + 1,
                    end_column: 1 + 1,
                    message: inconsistent_message,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement,
                    }),
                });
            }
        }

        // Check for consistency among items in the same level of the same list
        // Group list items by list_id and level
        let mut level_groups: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new(); // (list_id, level) -> [(line_num, indent)]

        for (line_num, indent, level) in &list_item_levels {
            let list_id = list_items
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);

            level_groups
                .entry((list_id, *level))
                .or_default()
                .push((*line_num, *indent));
        }

        // For each group, check for indentation consistency
        for ((_list_id, _level), items) in &level_groups {
            if items.len() <= 1 {
                continue; // Need at least 2 items to check consistency
            }

            // Get first item's indentation as the reference
            let reference_indent = items[0].1;

            // Check that all other items match this indentation
            for &(line_num, indent) in &items[1..] {
                if indent != reference_indent {
                    // Found inconsistent indentation
                    let inconsistent_message = format!(
                        "List item indentation is inconsistent with other items at the same level (found: {}, expected: {})",
                        indent, reference_indent
                    );

                    // Create a fixed version
                    let line = lines[line_num];
                    let trimmed = line.trim_start();
                    let replacement = format!("{}{}", " ".repeat(reference_indent), trimmed);

                    // Only add if we don't already have a warning for this line
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: 1,
                            end_line: line_num + 1,
                            end_column: 1 + 1,
                            message: inconsistent_message,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement,
                            }),
                        });
                    }
                }
            }
        }

        // Check for nested items with insufficient indentation
        for (line_num, indent, level) in &list_item_levels {
            if *level <= 1 {
                continue; // Not nested
            }

            // Find parent level items
            let list_id = list_items
                .iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);

            // Get items at the parent level
            let parent_items = level_groups.get(&(list_id, level - 1));

            if let Some(parent_items) = parent_items {
                if parent_items.is_empty() {
                    continue;
                }

                // Find parent indentation
                let parent_indent = parent_items[0].1;

                // Child should be more indented
                if *indent <= parent_indent {
                    let message = format!(
                        "Nested list item should be more indented than parent (parent: {}, child: {})",
                        parent_indent, indent
                    );

                    // Create fix
                    let line = lines[*line_num];
                    let trimmed = line.trim_start();
                    let expected_indent = parent_indent + 2;
                    let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);

                    // Only add if we don't already have a warning
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: 1,
                            end_line: line_num + 1,
                            end_column: 1 + 1,
                            message,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement,
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        None
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD005ListIndent)
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD005ListIndent {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    // ... existing tests ...

    #[test]
    fn test_with_document_structure() {
        let rule = MD005ListIndent;

        // Test with consistent list indentation
        let content = "* Item 1\n* Item 2\n  * Nested item\n  * Another nested item";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());

        // Test with inconsistent list indentation
        let content = "* Item 1\n* Item 2\n * Nested item\n  * Another nested item";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(!result.is_empty()); // Should have at least one warning

        // Test with different level indentation issues
        let content = "* Item 1\n  * Nested item\n * Another nested item with wrong indent";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(!result.is_empty()); // Should have at least one warning
    }
}
