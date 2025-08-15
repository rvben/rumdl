/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::{ElementCache, ListMarkerType};
use crate::utils::regex_cache::UNORDERED_LIST_MARKER_REGEX;
use toml;

mod md007_config;
use md007_config::MD007Config;

#[derive(Debug, Clone, Default)]
pub struct MD007ULIndent {
    config: MD007Config,
}

impl MD007ULIndent {
    pub fn new(indent: usize) -> Self {
        Self {
            config: MD007Config {
                indent,
                start_indented: false,
                start_indent: 2,
            },
        }
    }

    pub fn from_config_struct(config: MD007Config) -> Self {
        Self { config }
    }

    /// Get parent info for any list item to determine proper text alignment
    /// Returns (has_parent, expected_indent_position)
    fn get_parent_info(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_number: usize,
        indentation: usize,
    ) -> (bool, Option<usize>) {
        // Look backward from current line to find parent item
        for line_idx in (1..line_number).rev() {
            if let Some(line_info) = ctx.line_info(line_idx) {
                if let Some(list_item) = &line_info.list_item {
                    // Found a list item - check if it's at a lower indentation (parent level)
                    if list_item.marker_column < indentation {
                        // This is a parent item - calculate where child content should align
                        if list_item.is_ordered {
                            // For ordered lists, calculate the position where text starts
                            // e.g., "1. Text" -> text starts at position 3
                            // e.g., "10. Text" -> text starts at position 4
                            // e.g., "100. Text" -> text starts at position 5
                            let text_start_pos = list_item.marker_column + list_item.marker.len() + 1; // +1 for space after marker
                            return (true, Some(text_start_pos));
                        } else {
                            // For unordered lists, calculate where text starts
                            // e.g., "  * Text" -> text starts at position 4 (2 spaces + "* ")
                            let text_start_pos = list_item.marker_column + 2; // "* " or "- " or "+ "
                            return (true, Some(text_start_pos));
                        }
                    }
                }
                // If we encounter non-blank, non-list content at column 0, stop looking
                else if !line_info.is_blank && line_info.indent == 0 {
                    break;
                }
            }
        }
        (false, None)
    }
}

impl Rule for MD007ULIndent {
    fn name(&self) -> &'static str {
        "MD007"
    }

    fn description(&self) -> &'static str {
        "Unordered list indentation"
    }

    // TODO: Consider migrating to centralized list blocks once ElementCache is deprecated
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list markers before expensive processing
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }

        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();

        for item in element_cache.get_list_items() {
            // Only unordered list items
            // Skip list items inside code blocks (including YAML/front matter)
            if element_cache.is_in_code_block(item.line_number) {
                continue;
            }
            if matches!(
                item.marker_type,
                ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus
            ) {
                // Skip first level check if start_indented is false
                if !self.config.start_indented && item.nesting_level == 0 {
                    continue;
                }

                let expected_indent = if self.config.start_indented {
                    self.config.start_indent + (item.nesting_level * self.config.indent)
                } else {
                    // For any nested item, check if it should align with parent's text content
                    if item.nesting_level > 0 {
                        let (has_parent, expected_pos) = self.get_parent_info(ctx, item.line_number, item.indentation);
                        if has_parent {
                            if let Some(pos) = expected_pos {
                                // Align with parent's text content
                                pos
                            } else {
                                // Fallback to standard indentation
                                item.nesting_level * self.config.indent
                            }
                        } else {
                            item.nesting_level * self.config.indent
                        }
                    } else {
                        item.nesting_level * self.config.indent
                    }
                };

                if item.indentation != expected_indent {
                    // Generate fix for this list item
                    let fix = {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line) = lines.get(item.line_number - 1) {
                            // Extract the marker and content
                            if UNORDERED_LIST_MARKER_REGEX.captures(line).is_some() {
                                let correct_indent = " ".repeat(expected_indent);

                                // Fix range should match warning range - only the problematic indentation
                                let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());

                                // Warning will cover the indentation area that needs to be fixed
                                let start_col = item.blockquote_prefix.len() + 1; // Start of indentation
                                let end_col = item.blockquote_prefix.len() + item.indent_str.len() + 1; // End of actual indentation string

                                let start_byte = line_index.line_col_to_byte_range(item.line_number, start_col).start;
                                let end_byte = line_index.line_col_to_byte_range(item.line_number, end_col).start;

                                // Replacement should be just the correct indentation
                                let replacement = correct_indent;

                                Some(crate::rule::Fix {
                                    range: start_byte..end_byte,
                                    replacement,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Expected {} spaces for indent depth {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + 1, // Start of indentation
                        end_line: item.line_number,
                        end_column: item.blockquote_prefix.len() + item.indent_str.len() + 1, // End of actual indentation string
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }
        }
        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return if no list items
        if doc_structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Use ElementCache for detailed list analysis (still needed for nesting levels)
        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();

        for item in element_cache.get_list_items() {
            // Only process unordered list items that are in our structure
            if !doc_structure.list_lines.contains(&item.line_number) {
                continue;
            }

            // Skip list items inside code blocks
            if doc_structure.is_in_code_block(item.line_number) {
                continue;
            }

            if matches!(
                item.marker_type,
                ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus
            ) {
                // Skip first level check if start_indented is false
                if !self.config.start_indented && item.nesting_level == 0 {
                    continue;
                }

                let expected_indent = if self.config.start_indented {
                    self.config.start_indent + (item.nesting_level * self.config.indent)
                } else {
                    // For any nested item, check if it should align with parent's text content
                    if item.nesting_level > 0 {
                        let (has_parent, expected_pos) = self.get_parent_info(ctx, item.line_number, item.indentation);
                        if has_parent {
                            if let Some(pos) = expected_pos {
                                // Align with parent's text content
                                pos
                            } else {
                                // Fallback to standard indentation
                                item.nesting_level * self.config.indent
                            }
                        } else {
                            item.nesting_level * self.config.indent
                        }
                    } else {
                        item.nesting_level * self.config.indent
                    }
                };

                if item.indentation != expected_indent {
                    // Generate fix for this list item
                    let fix = {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line) = lines.get(item.line_number - 1) {
                            // Extract the marker and content
                            if UNORDERED_LIST_MARKER_REGEX.captures(line).is_some() {
                                let correct_indent = " ".repeat(expected_indent);

                                // Fix range should match warning range - only the problematic indentation
                                let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());

                                // Warning will cover the indentation area that needs to be fixed
                                let start_col = item.blockquote_prefix.len() + 1; // Start of indentation
                                let end_col = item.blockquote_prefix.len() + item.indent_str.len() + 1; // End of actual indentation string

                                let start_byte = line_index.line_col_to_byte_range(item.line_number, start_col).start;
                                let end_byte = line_index.line_col_to_byte_range(item.line_number, end_col).start;

                                // Replacement should be just the correct indentation
                                let replacement = correct_indent;

                                Some(crate::rule::Fix {
                                    range: start_byte..end_byte,
                                    replacement,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Expected {} spaces for indent depth {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + 1, // Start of indentation
                        end_line: item.line_number,
                        end_column: item.blockquote_prefix.len() + item.indent_str.len() + 1, // End of actual indentation string
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no unordered list items
        ctx.content.is_empty()
            || !ctx
                .lines
                .iter()
                .any(|line| line.list_item.as_ref().is_some_and(|item| !item.is_ordered))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD007Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD007Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD007Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl DocumentStructureExtensions for MD007ULIndent {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // Use the document structure to check if there are any unordered list elements
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_valid_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid indentation, but got {} warnings",
            result.len()
        );
    }

    #[test]
    fn test_invalid_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].line, 3);
        assert_eq!(result[1].column, 1);
    }

    #[test]
    fn test_mixed_indentation() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n  * Item 2\n   * Item 3\n  * Item 4";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_fix_indentation() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();
        // With dynamic alignment:
        // Item 2 aligns with Item 1's text (2 spaces)
        // Item 3 aligns with Item 2's text (4 + 1 = 5 spaces)
        let expected = "* Item 1\n  * Item 2\n     * Item 3";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_md007_in_yaml_code_block() {
        let rule = MD007ULIndent::default();
        let content = r#"```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.5.0
    hooks:
    -   id: rumdl-check
```"#;
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD007 should not trigger inside a code block, but got warnings: {result:?}"
        );
    }

    #[test]
    fn test_blockquoted_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>   * Item 2\n>     * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid blockquoted list indentation, but got {result:?}"
        );
    }

    #[test]
    fn test_blockquoted_list_invalid_indent() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>    * Item 2\n>       * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings for invalid blockquoted list indentation, got {result:?}"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_nested_blockquote_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid nested blockquoted list indentation, but got {result:?}"
        );
    }

    #[test]
    fn test_blockquote_list_with_code_block() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>   * Item 2\n>   ```\n>   code\n>   ```\n>   * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD007 should not trigger inside a code block within a blockquote, but got warnings: {result:?}"
        );
    }

    #[test]
    fn test_properly_indented_lists() {
        let rule = MD007ULIndent::default();

        // Test various properly indented lists
        let test_cases = vec![
            "* Item 1\n* Item 2",
            "* Item 1\n  * Item 1.1\n    * Item 1.1.1",
            "- Item 1\n  - Item 1.1",
            "+ Item 1\n  + Item 1.1",
            "* Item 1\n  * Item 1.1\n* Item 2\n  * Item 2.1",
        ];

        for content in test_cases {
            let ctx = LintContext::new(content);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for properly indented list:\n{}\nGot {} warnings",
                content,
                result.len()
            );
        }
    }

    #[test]
    fn test_under_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n * Item 1.1", 1, 2),                   // Expected 2 spaces, got 1
            ("* Item 1\n  * Item 1.1\n   * Item 1.1.1", 1, 3), // Expected 4 spaces, got 3
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for under-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    #[test]
    fn test_over_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n   * Item 1.1", 1, 2),                   // Expected 2 spaces, got 3
            ("* Item 1\n    * Item 1.1", 1, 2),                  // Expected 2 spaces, got 4
            ("* Item 1\n  * Item 1.1\n     * Item 1.1.1", 1, 3), // Expected 4 spaces, got 5
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for over-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    #[test]
    fn test_custom_indent_2_spaces() {
        let rule = MD007ULIndent::new(2); // Default
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_3_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(3);

        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, Item 2 should align with Item 1's text (2 spaces)
        // and Item 3 should align with Item 2's text (4 spaces), not fixed increments
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test that dynamic alignment works correctly
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_4_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(4);
        let content = "* Item 1\n    * Item 2\n        * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, should expect 2 spaces and 6 spaces, not 4 and 8
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test correct dynamic alignment
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_tab_indentation() {
        let rule = MD007ULIndent::default();

        // Single tab
        let content = "* Item 1\n\t* Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Tab indentation should trigger warning");

        // Fix should convert tab to spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");

        // Multiple tabs
        let content_multi = "* Item 1\n\t* Item 2\n\t\t* Item 3";
        let ctx = LintContext::new(content_multi);
        let fixed = rule.fix(&ctx).unwrap();
        // With dynamic alignment: Item 3 aligns with Item 2 at correct position
        assert_eq!(fixed, "* Item 1\n  * Item 2\n   * Item 3");

        // Mixed tabs and spaces
        let content_mixed = "* Item 1\n \t* Item 2\n\t * Item 3";
        let ctx = LintContext::new(content_mixed);
        let fixed = rule.fix(&ctx).unwrap();
        // With dynamic alignment: Item 3 aligns with Item 2 at correct position
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");
    }

    #[test]
    fn test_mixed_ordered_unordered_lists() {
        let rule = MD007ULIndent::default();

        // MD007 only checks unordered lists, so ordered lists should be ignored
        // Note: 3 spaces is now correct for bullets under ordered items
        let content = r#"1. Ordered item
   * Unordered sub-item (correct - 3 spaces under ordered)
   2. Ordered sub-item
* Unordered item
  1. Ordered sub-item
  * Unordered sub-item"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "All unordered list indentation should be correct");

        // No fix needed as all indentation is correct
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_list_markers_variety() {
        let rule = MD007ULIndent::default();

        // Test all three unordered list markers
        let content = r#"* Asterisk
  * Nested asterisk
- Hyphen
  - Nested hyphen
+ Plus
  + Nested plus"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "All unordered list markers should work with proper indentation"
        );

        // Test with wrong indentation for each marker type
        let wrong_content = r#"* Asterisk
   * Wrong asterisk
- Hyphen
 - Wrong hyphen
+ Plus
    + Wrong plus"#;

        let ctx = LintContext::new(wrong_content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "All marker types should be checked for indentation");
    }

    #[test]
    fn test_empty_list_items() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n* \n  * Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty list items should not affect indentation checks"
        );
    }

    #[test]
    fn test_list_with_code_blocks() {
        let rule = MD007ULIndent::default();
        let content = r#"* Item 1
  ```
  code
  ```
  * Item 2
    * Item 3"#;
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_in_front_matter() {
        let rule = MD007ULIndent::default();
        let content = r#"---
tags:
  - tag1
  - tag2
---
* Item 1
  * Item 2"#;
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Lists in YAML front matter should be ignored");
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1 with **bold** and *italic*\n   * Item 2 with `code`\n     * Item 3 with [link](url)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // With dynamic alignment: Item 3 aligns with Item 2's text (2 + 2 + 1 = 5 spaces)
        let expected = "* Item 1 with **bold** and *italic*\n  * Item 2 with `code`\n     * Item 3 with [link](url)";
        assert_eq!(fixed, expected, "Fix should only change indentation, not content");
    }

    #[test]
    fn test_start_indented_config() {
        let config = MD007Config {
            start_indented: true,
            start_indent: 4,
            indent: 2,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // First level should be indented by start_indent (4 spaces)
        // Level 0: 4 spaces (start_indent)
        // Level 1: 6 spaces (start_indent + indent = 4 + 2)
        // Level 2: 8 spaces (start_indent + 2*indent = 4 + 4)
        let content = "    * Item 1\n      * Item 2\n        * Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings with start_indented config");

        // Wrong first level indentation
        let wrong_content = "  * Item 1\n    * Item 2";
        let ctx = LintContext::new(wrong_content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "Expected 4 spaces for indent depth 0, found 2");
        assert_eq!(result[1].line, 2);
        assert_eq!(result[1].message, "Expected 6 spaces for indent depth 1, found 4");

        // Fix should correct to start_indent for first level
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    * Item 1\n      * Item 2");
    }

    #[test]
    fn test_start_indented_false_allows_any_first_level() {
        let rule = MD007ULIndent::default(); // start_indented is false by default

        // When start_indented is false, first level items at any indentation are allowed
        let content = "   * Item 1"; // First level at 3 spaces
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First level at any indentation should be allowed when start_indented is false"
        );

        // Multiple first level items at different indentations should all be allowed
        let content = "* Item 1\n  * Item 2\n    * Item 3"; // All at level 0 (different indents)
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "All first-level items should be allowed at any indentation"
        );
    }

    #[test]
    fn test_deeply_nested_lists() {
        let rule = MD007ULIndent::default();
        let content = r#"* L1
  * L2
    * L3
      * L4
        * L5
          * L6"#;
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with wrong deep nesting
        let wrong_content = r#"* L1
  * L2
    * L3
      * L4
         * L5
            * L6"#;
        let ctx = LintContext::new(wrong_content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Deep nesting errors should be detected");
    }

    #[test]
    fn test_bullets_nested_under_numbered_items() {
        let rule = MD007ULIndent::default();
        let content = "\
1. **Active Directory/LDAP**
   - User authentication and directory services
   - LDAP for user information and validation

2. **Oracle Unified Directory (OUD)**
   - Extended user directory services";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - 3 spaces is correct for bullets under numbered items
        assert!(
            result.is_empty(),
            "Expected no warnings for bullets with 3 spaces under numbered items, got: {result:?}"
        );
    }

    #[test]
    fn test_bullets_nested_under_numbered_items_wrong_indent() {
        let rule = MD007ULIndent::default();
        let content = "\
1. **Active Directory/LDAP**
  - Wrong: only 2 spaces";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should flag incorrect indentation
        assert_eq!(
            result.len(),
            1,
            "Expected warning for incorrect indentation under numbered items"
        );
        assert!(
            result
                .iter()
                .any(|w| w.line == 2 && w.message.contains("Expected 3 spaces"))
        );
    }

    #[test]
    fn test_regular_bullet_nesting_still_works() {
        let rule = MD007ULIndent::default();
        let content = "\
* Top level
  * Nested bullet (2 spaces is correct)
    * Deeply nested (4 spaces)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - standard bullet nesting still uses 2-space increments
        assert!(
            result.is_empty(),
            "Expected no warnings for standard bullet nesting, got: {result:?}"
        );
    }
}
