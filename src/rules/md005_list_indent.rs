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
    // Pre-compiled pattern for quick list detection
    static ref QUICK_LIST_CHECK: Regex = Regex::new(r"^( *)([*+\-]|\d+[.)])").unwrap();
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

    /// Optimized check that combines all passes into one
    fn check_optimized(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for common cases
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check to avoid processing files without list markers
        if !QUICK_LIST_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();

        // Early return if there are no lines
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Pre-compute code blocks once
        let mut in_code_blocks = vec![false; lines.len()];
        let mut in_block = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_block = !in_block;
            }
            in_code_blocks[i] = in_block;
        }

        // Single pass processing with efficient data structures
        let mut list_items: Vec<(usize, usize, usize)> = Vec::new(); // (line_num, indent, list_id)
        let mut current_list_id = 0;
        let mut in_list = false;
        let mut list_level_maps: HashMap<usize, HashMap<usize, usize>> = HashMap::new(); // list_id -> { indent -> level }
        let mut level_indents: HashMap<(usize, usize), usize> = HashMap::new(); // (list_id, level) -> expected_indent

        for (line_num, line) in lines.iter().enumerate() {
            // Skip blank lines and code blocks
            if Self::is_blank_line(line) || in_code_blocks[line_num] {
                continue;
            }

            // Check if this is a list item
            if let Some((indent, _marker, _)) = Self::get_list_marker_info(line) {
                // Determine if this starts a new list
                let is_new_list = !in_list
                    || indent == 0
                    || (list_items.last().map_or(false, |(_, prev_indent, _)| {
                        prev_indent > &0 && indent < prev_indent / 2
                    }));

                if is_new_list {
                    current_list_id += 1;
                    in_list = true;
                }

                // Determine level for this item
                let level_map = list_level_maps.entry(current_list_id).or_default();
                let level = if level_map.is_empty() {
                    level_map.insert(indent, 1);
                    level_indents.insert((current_list_id, 1), indent);
                    1
                } else {
                    // Find appropriate level
                    if let Some(&existing_level) = level_map.get(&indent) {
                        existing_level
                    } else {
                        // Find parent level
                        let mut level = 1;
                        let mut parent_indent = 0;

                        for (&prev_indent, &prev_level) in level_map.iter() {
                            if prev_indent < indent && (prev_level >= level || prev_indent > parent_indent) {
                                level = prev_level + 1;
                                parent_indent = prev_indent;
                            }
                        }

                        level_map.insert(indent, level);
                        level
                    }
                };

                list_items.push((line_num, indent, current_list_id));

                // Check indentation immediately
                let expected_indent = Self::get_expected_indent(level);
                if indent != expected_indent {
                    let inconsistent_message = format!(
                        "List item indentation is {} {}, expected {} for level {}",
                        indent,
                        if indent == 1 { "space" } else { "spaces" },
                        expected_indent,
                        level
                    );

                    let (start_line, start_col, end_line, end_col) = if indent > 0 {
                        calculate_match_range(line_num + 1, line, 0, indent)
                    } else {
                        calculate_match_range(line_num + 1, line, 0, 1)
                    };

                    // Fix range should span from start of line to end of indentation
                    let fix_range = if indent > 0 {
                        // Replace the current indentation with expected indentation
                        let start_byte = line_index.line_col_to_byte_range(line_num + 1, 1).start;
                        let end_byte = line_index.line_col_to_byte_range(line_num + 1, indent + 1).start;
                        start_byte..end_byte
                    } else {
                        // For no indentation, insert at start of line
                        let byte_pos = line_index.line_col_to_byte_range(line_num + 1, 1).start;
                        byte_pos..byte_pos
                    };

                    // Replacement should be just the corrected indentation
                    let replacement = if expected_indent > 0 {
                        " ".repeat(expected_indent)
                    } else {
                        String::new()
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
                            range: fix_range,
                            replacement,
                        }),
                    });
                }

                // Track level consistency
                let key = (current_list_id, level);
                if let Some(reference_indent) = level_indents.get(&key) {
                    if indent != *reference_indent {
                        let inconsistent_message = format!(
                            "List item indentation is inconsistent with other items at the same level (found: {}, expected: {})",
                            indent, reference_indent
                        );

                        // Only add if we don't already have a warning for this line
                        if !warnings.iter().any(|w| w.line == line_num + 1) {
                            let (start_line, start_col, end_line, end_col) = if indent > 0 {
                                calculate_match_range(line_num + 1, line, 0, indent)
                            } else {
                                calculate_match_range(line_num + 1, line, 0, 1)
                            };

                            // Fix range should span from start of line to end of indentation
                            let fix_range = if indent > 0 {
                                // Replace the current indentation with expected indentation
                                let start_byte = line_index.line_col_to_byte_range(line_num + 1, 1).start;
                                let end_byte = line_index.line_col_to_byte_range(line_num + 1, indent + 1).start;
                                start_byte..end_byte
                            } else {
                                // For no indentation, insert at start of line
                                let byte_pos = line_index.line_col_to_byte_range(line_num + 1, 1).start;
                                byte_pos..byte_pos
                            };

                            // Replacement should be just the corrected indentation
                            let replacement = if *reference_indent > 0 {
                                " ".repeat(*reference_indent)
                            } else {
                                String::new()
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
                                    range: fix_range,
                                    replacement,
                                }),
                            });
                        }
                    }
                } else {
                    level_indents.insert(key, indent);
                }

            } else {
                // Check if it's a list continuation
                if list_items.is_empty() || !in_list {
                    continue;
                }

                let (prev_line_num, _, _) = list_items.last().unwrap();
                if !Self::is_list_continuation(lines[*prev_line_num], line) {
                    in_list = false;
                }
            }
        }

        Ok(warnings)
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
        // Use optimized version
        self.check_optimized(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Sort warnings by position (descending) to apply from end to start
        let mut warnings_with_fixes: Vec<_> = warnings
            .into_iter()
            .filter_map(|w| {
                if let Some(fix) = w.fix.clone() {
                    Some((w, fix))
                } else {
                    None
                }
            })
            .collect();
        warnings_with_fixes.sort_by_key(|(_, fix)| std::cmp::Reverse(fix.range.start));

        // Apply fixes to content
        let mut content = ctx.content.to_string();
        for (_, fix) in warnings_with_fixes {
            if fix.range.start <= content.len() && fix.range.end <= content.len() {
                content.replace_range(fix.range, &fix.replacement);
            }
        }

        Ok(content)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !QUICK_LIST_CHECK.is_match(content)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        // If no lists in structure, return early
        if structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Use optimized check - it's already efficient enough
        self.check_optimized(ctx)
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
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        !doc_structure.list_lines.is_empty()
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
