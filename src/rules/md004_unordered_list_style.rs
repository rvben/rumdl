use crate::LintContext;
/// Rule MD004: Use consistent style for unordered list markers
///
/// See [docs/md004.md](../../docs/md004.md) for full documentation, configuration, and examples.
///
/// Enforces that all unordered list items in a Markdown document use the same marker style ("*", "+", or "-") or are consistent with the first marker used, depending on configuration.
///
/// ## Purpose
///
/// Ensures visual and stylistic consistency for unordered lists, making documents easier to read and maintain.
///
/// ## Configuration Options
///
/// The rule supports configuring the required marker style:
/// ```yaml
/// MD004:
///   style: dash      # Options: "dash", "asterisk", "plus", or "consistent" (default)
/// ```
///
/// ## Examples
///
/// ### Correct (with style: dash)
/// ```markdown
/// - Item 1
/// - Item 2
///   - Nested item
/// - Item 3
/// ```
///
/// ### Incorrect (with style: dash)
/// ```markdown
/// * Item 1
/// - Item 2
/// + Item 3
/// ```
///
/// ## Behavior
///
/// - Checks each unordered list item for its marker character.
/// - In "consistent" mode, the first marker sets the style for the document.
/// - Skips code blocks and front matter.
/// - Reports a warning if a list item uses a different marker than the configured or detected style.
///
/// ## Fix Behavior
///
/// - Rewrites all unordered list markers to match the configured or detected style.
/// - Preserves indentation and content after the marker.
///
/// ## Rationale
///
/// Consistent list markers improve readability and reduce distraction, especially in large documents or when collaborating with others. This rule helps enforce a uniform style across all unordered lists.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructureExtensions;
use toml;

mod md004_config;
use md004_config::MD004Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnorderedListStyle {
    Asterisk,   // "*"
    Plus,       // "+"
    Dash,       // "-"
    Consistent, // Use the first marker in a file consistently
    Sublist,    // Each nesting level uses a different marker (*, +, -, cycling)
}

impl Default for UnorderedListStyle {
    fn default() -> Self {
        Self::Consistent
    }
}

/// Rule MD004: Unordered list style
#[derive(Clone, Default)]
pub struct MD004UnorderedListStyle {
    config: MD004Config,
}

impl MD004UnorderedListStyle {
    pub fn new(style: UnorderedListStyle) -> Self {
        Self {
            config: MD004Config { style, after_marker: 1 },
        }
    }

    pub fn from_config_struct(config: MD004Config) -> Self {
        Self { config }
    }
}

impl Rule for MD004UnorderedListStyle {
    fn name(&self) -> &'static str {
        "MD004"
    }

    fn description(&self) -> &'static str {
        "Use consistent style for unordered list markers"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        // Early returns for performance
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list markers before processing
        if !ctx.content.contains(['*', '-', '+']) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut first_marker: Option<char> = None;

        // Use centralized list blocks for better performance and accuracy
        for list_block in &ctx.list_blocks {
            // Check each list item in this block
            // We need to check individual items even in mixed lists (ordered with nested unordered)
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line)
                    && let Some(list_item) = &line_info.list_item
                {
                    // Skip ordered list items - we only care about unordered ones
                    if list_item.is_ordered {
                        continue;
                    }

                    // Get the marker character
                    let marker = list_item.marker.chars().next().unwrap();

                    // Calculate offset for the marker position
                    let offset = line_info.byte_offset + list_item.marker_column;

                    match self.config.style {
                        UnorderedListStyle::Consistent => {
                            // For consistent mode, we check consistency across the entire document
                            if let Some(first) = first_marker {
                                // Check if current marker matches the first marker found
                                if marker != first {
                                    let (line, col) = ctx.offset_to_line_col(offset);
                                    warnings.push(LintWarning {
                                        line,
                                        column: col,
                                        end_line: line,
                                        end_column: col + 1,
                                        message: format!(
                                            "List marker '{marker}' does not match expected style '{first}'"
                                        ),
                                        severity: Severity::Warning,
                                        rule_name: Some(self.name()),
                                        fix: Some(Fix {
                                            range: offset..offset + 1,
                                            replacement: first.to_string(),
                                        }),
                                    });
                                }
                            } else {
                                // This is the first marker we've found - set the style
                                first_marker = Some(marker);
                            }
                        }
                        UnorderedListStyle::Sublist => {
                            // Calculate expected marker based on indentation level
                            // Each 2 spaces of indentation represents a nesting level
                            let nesting_level = list_item.marker_column / 2;
                            let expected_marker = match nesting_level % 3 {
                                0 => '*',
                                1 => '+',
                                2 => '-',
                                _ => {
                                    // This should never happen as % 3 only returns 0, 1, or 2
                                    // but fallback to asterisk for safety
                                    '*'
                                }
                            };
                            if marker != expected_marker {
                                let (line, col) = ctx.offset_to_line_col(offset);
                                warnings.push(LintWarning {
                                        line,
                                        column: col,
                                        end_line: line,
                                        end_column: col + 1,
                                        message: format!(
                                            "List marker '{marker}' does not match expected style '{expected_marker}' for nesting level {nesting_level}"
                                        ),
                                        severity: Severity::Warning,
                                        rule_name: Some(self.name()),
                                        fix: Some(Fix {
                                            range: offset..offset + 1,
                                            replacement: expected_marker.to_string(),
                                        }),
                                    });
                            }
                        }
                        _ => {
                            // Handle specific style requirements (asterisk, dash, plus)
                            let target_marker = match self.config.style {
                                UnorderedListStyle::Asterisk => '*',
                                UnorderedListStyle::Dash => '-',
                                UnorderedListStyle::Plus => '+',
                                UnorderedListStyle::Consistent | UnorderedListStyle::Sublist => {
                                    // These cases are handled separately above
                                    // but fallback to asterisk for safety
                                    '*'
                                }
                            };
                            if marker != target_marker {
                                let (line, col) = ctx.offset_to_line_col(offset);
                                warnings.push(LintWarning {
                                    line,
                                    column: col,
                                    end_line: line,
                                    end_column: col + 1,
                                    message: format!(
                                        "List marker '{marker}' does not match expected style '{target_marker}'"
                                    ),
                                    severity: Severity::Warning,
                                    rule_name: Some(self.name()),
                                    fix: Some(Fix {
                                        range: offset..offset + 1,
                                        replacement: target_marker.to_string(),
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let mut lines: Vec<String> = ctx.content.lines().map(String::from).collect();
        let mut first_marker: Option<char> = None;

        // Use centralized list blocks
        for list_block in &ctx.list_blocks {
            // Process each list item in this block
            // We need to check individual items even in mixed lists
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line)
                    && let Some(list_item) = &line_info.list_item
                {
                    // Skip ordered list items - we only care about unordered ones
                    if list_item.is_ordered {
                        continue;
                    }

                    let line_idx = item_line - 1;
                    if line_idx >= lines.len() {
                        continue;
                    }

                    let line = &lines[line_idx];
                    let marker = list_item.marker.chars().next().unwrap();

                    // Determine the target marker
                    let target_marker = match self.config.style {
                        UnorderedListStyle::Consistent => {
                            if let Some(first) = first_marker {
                                first
                            } else {
                                first_marker = Some(marker);
                                marker
                            }
                        }
                        UnorderedListStyle::Sublist => {
                            // Calculate expected marker based on indentation level
                            // Each 2 spaces of indentation represents a nesting level
                            let nesting_level = list_item.marker_column / 2;
                            match nesting_level % 3 {
                                0 => '*',
                                1 => '+',
                                2 => '-',
                                _ => {
                                    // This should never happen as % 3 only returns 0, 1, or 2
                                    // but fallback to asterisk for safety
                                    '*'
                                }
                            }
                        }
                        UnorderedListStyle::Asterisk => '*',
                        UnorderedListStyle::Dash => '-',
                        UnorderedListStyle::Plus => '+',
                    };

                    // Replace the marker if needed
                    if marker != target_marker {
                        let marker_pos = list_item.marker_column;
                        if marker_pos < line.len() {
                            let mut new_line = String::new();
                            new_line.push_str(&line[..marker_pos]);
                            new_line.push(target_marker);
                            new_line.push_str(&line[marker_pos + 1..]);
                            lines[line_idx] = new_line;
                        }
                    }
                }
            }
        }

        let mut result = lines.join("\n");
        if ctx.content.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains(['*', '-', '+'])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(match self.config.style {
                UnorderedListStyle::Asterisk => "asterisk".to_string(),
                UnorderedListStyle::Dash => "dash".to_string(),
                UnorderedListStyle::Plus => "plus".to_string(),
                UnorderedListStyle::Consistent => "consistent".to_string(),
                UnorderedListStyle::Sublist => "sublist".to_string(),
            }),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD004", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "asterisk" => UnorderedListStyle::Asterisk,
            "dash" => UnorderedListStyle::Dash,
            "plus" => UnorderedListStyle::Plus,
            "sublist" => UnorderedListStyle::Sublist,
            _ => UnorderedListStyle::Consistent,
        };
        Box::new(MD004UnorderedListStyle::new(style))
    }
}

impl DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // Quick check for any list markers and unordered list blocks
        ctx.content.contains(['*', '-', '+']) && ctx.list_blocks.iter().any(|block| !block.is_ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_consistent_asterisk_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "* Item 1\n* Item 2\n  * Nested\n* Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_consistent_dash_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "- Item 1\n- Item 2\n  - Nested\n- Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_consistent_plus_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "+ Item 1\n+ Item 2\n  + Nested\n+ Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_inconsistent_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_asterisk_style_enforced() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "List marker '-' does not match expected style '*'");
        assert_eq!(result[1].message, "List marker '+' does not match expected style '*'");
    }

    #[test]
    fn test_dash_style_enforced() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "List marker '*' does not match expected style '-'");
        assert_eq!(result[1].message, "List marker '+' does not match expected style '-'");
    }

    #[test]
    fn test_plus_style_enforced() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "List marker '*' does not match expected style '+'");
        assert_eq!(result[1].message, "List marker '-' does not match expected style '+'");
    }

    #[test]
    fn test_fix_consistent_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3");
    }

    #[test]
    fn test_fix_asterisk_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "- Item 1\n+ Item 2\n- Item 3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3");
    }

    #[test]
    fn test_fix_dash_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let content = "* Item 1\n+ Item 2\n* Item 3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "- Item 1\n- Item 2\n- Item 3");
    }

    #[test]
    fn test_fix_plus_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
        let content = "* Item 1\n- Item 2\n* Item 3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "+ Item 1\n+ Item 2\n+ Item 3");
    }

    #[test]
    fn test_nested_lists() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "* Item 1\n  * Nested 1\n    * Double nested\n  - Wrong marker\n* Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_fix_nested_lists() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "* Item 1\n  - Nested 1\n    + Double nested\n  - Nested 2\n* Item 2";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            "* Item 1\n  * Nested 1\n    * Double nested\n  * Nested 2\n* Item 2"
        );
    }

    #[test]
    fn test_with_code_blocks() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "* Item 1\n\n```\n- This is in code\n+ Not a list\n```\n\n- Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 8);
    }

    #[test]
    fn test_with_blockquotes() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "> * Item 1\n> - Item 2\n\n* Regular item\n+ Different marker";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should detect inconsistencies both in blockquote and regular content
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_empty_document() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_lists() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "This is a paragraph.\n\nAnother paragraph.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_ordered_lists_ignored() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "1. Item 1\n2. Item 2\n   1. Nested\n3. Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_mixed_ordered_unordered() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "1. Ordered\n   * Unordered nested\n   - Wrong marker\n2. Another ordered";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let content = "* Item with **bold** and *italic*\n+ Item with `code`\n* Item with [link](url)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            "- Item with **bold** and *italic*\n- Item with `code`\n- Item with [link](url)"
        );
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let content = "  - Indented item\n    + Nested item\n  - Another indented";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  * Indented item\n    * Nested item\n  * Another indented");
    }

    #[test]
    fn test_multiple_spaces_after_marker() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "*   Item 1\n-   Item 2\n+   Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "*   Item 1\n*   Item 2\n*   Item 3");
    }

    #[test]
    fn test_tab_after_marker() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let content = "*\tItem 1\n-\tItem 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "*\tItem 1\n*\tItem 2");
    }

    #[test]
    fn test_edge_case_marker_at_end() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        // These are valid list items with minimal content (just a space)
        let content = "* \n- \n+ ";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag - and + as wrong markers
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* \n* \n* ");
    }

    #[test]
    fn test_from_config() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("style".to_string(), toml::Value::String("plus".to_string()));
        config.rules.insert("MD004".to_string(), rule_config);

        let rule = MD004UnorderedListStyle::from_config(&config);
        let content = "* Item 1\n- Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let config = rule.default_config_section();
        assert!(config.is_some());
        let (name, value) = config.unwrap();
        assert_eq!(name, "MD004");
        if let toml::Value::Table(table) = value {
            assert_eq!(table.get("style").and_then(|v| v.as_str()), Some("dash"));
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_sublist_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Sublist);
        // Level 0 should use *, level 1 should use +, level 2 should use -
        let content = "* Item 1\n  + Item 2\n    - Item 3\n      * Item 4\n  + Item 5";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Sublist style should accept cycling markers");
    }

    #[test]
    fn test_sublist_style_incorrect() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Sublist);
        // Wrong markers for each level
        let content = "- Item 1\n  * Item 2\n    + Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0].message,
            "List marker '-' does not match expected style '*' for nesting level 0"
        );
        assert_eq!(
            result[1].message,
            "List marker '*' does not match expected style '+' for nesting level 1"
        );
        assert_eq!(
            result[2].message,
            "List marker '+' does not match expected style '-' for nesting level 2"
        );
    }

    #[test]
    fn test_fix_sublist_style() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Sublist);
        let content = "- Item 1\n  - Item 2\n    - Item 3\n      - Item 4";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  + Item 2\n    - Item 3\n      * Item 4");
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!(
                "{}Item {}\n",
                if i % 3 == 0 {
                    "* "
                } else if i % 3 == 1 {
                    "- "
                } else {
                    "+ "
                },
                i
            ));
        }
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();
        // Should detect all non-asterisk markers
        assert!(result.len() > 600);
    }
}
