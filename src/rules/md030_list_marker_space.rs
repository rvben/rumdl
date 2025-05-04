//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::utils::range_utils::LineIndex;
use crate::utils::element_cache::{ElementCache, ListMarkerType};
use crate::rules::list_utils::{is_list_item, is_multi_line_item, ListType};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;
use toml;
use crate::lint_context::LintContext;
use markdown::mdast::{Node, List, ListItem};
use std::collections::HashMap;

lazy_static! {
    // Regex to capture list markers and the spaces *after* them
    // Allows ZERO or more spaces after marker now using \s*
    static ref LIST_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)").unwrap();
    // Regex used for fixing - ensures exactly the required number of spaces
    // Note: Captures slightly differently to handle replacement efficiently
    static ref LIST_FIX_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)").unwrap();
    static ref CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
}

#[derive(Clone)]
pub struct MD030ListMarkerSpace {
    ul_single: usize,
    ul_multi: usize,
    ol_single: usize,
    ol_multi: usize,
}

impl Default for MD030ListMarkerSpace {
    fn default() -> Self {
        Self {
            ul_single: 1,
            ul_multi: 1,
            ol_single: 1,
            ol_multi: 1,
        }
    }
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            ul_single,
            ul_multi,
            ol_single,
            ol_multi,
        }
    }

    pub fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.ul_single,
            (ListType::Unordered, true) => self.ul_multi,
            (ListType::Ordered, false) => self.ol_single,
            (ListType::Ordered, true) => self.ol_multi,
        }
    }

    fn fix_line(&self, line: &str, list_type: ListType, is_multi: bool) -> String {
        let expected_spaces = self.get_expected_spaces(list_type, is_multi);
        // Use the LIST_FIX_REGEX for replacement
        LIST_FIX_REGEX
            .replace(line, |caps: &regex::Captures| {
                // Reconstruct the start: indentation + marker + correct spaces
                format!("{}{}{}", &caps[1], &caps[2], " ".repeat(expected_spaces))
            })
            .to_string()
    }

    fn precompute_states(&self, lines: &[&str]) -> (Vec<bool>, Vec<bool>) {
        let mut is_list_line = vec![false; lines.len()];
        let mut multi_line = vec![false; lines.len()];
        let mut in_code_block = false;

        // First pass: mark code blocks
        for (i, &line) in lines.iter().enumerate() {
            if CODE_BLOCK_REGEX.is_match(line) {
                in_code_block = !in_code_block;
            }
            if !in_code_block && is_list_item(line).is_some() {
                is_list_line[i] = true;
            }
        }

        // Second pass: compute multi-line states
        for i in 0..lines.len() {
            if is_list_line[i] {
                multi_line[i] = is_multi_line_item(lines, i);
            }
        }

        (is_list_line, multi_line)
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
        let line_index = LineIndex::new(ctx.content.to_string());
        let rule = self;
        // Move process_list_block here so it's in scope for visit_lists
        fn process_list_block(
            list_node: &List,
            parent_is_ordered: Option<bool>,
            ctx: &LintContext,
            rule: &MD030ListMarkerSpace,
            line_index: &LineIndex,
            warnings: &mut Vec<LintWarning>,
            nesting_level: usize,
            parent_is_code_or_blockquote: bool,
        ) {
            use markdown::mdast::Node;
            struct ItemInfo<'a> {
                li: &'a ListItem,
                line: usize,
                col: usize,
                line_str: &'a str,
                marker: String,
                spaces: usize,
                curr_indent: usize,
                is_multi: bool,
                list_type: ListType,
            }
            let mut items = Vec::new();
            let lines: Vec<&str> = ctx.content.lines().collect();
            for item in &list_node.children {
                if let Node::ListItem(li) = item {
                    // Skip if parent is code block, blockquote, or HTML block
                    if parent_is_code_or_blockquote {
                        continue;
                    }
                    if let Some(pos) = li.position.as_ref() {
                        let start = pos.start.offset;
                        let (line, col) = ctx.offset_to_line_col(start);
                        let line_str = ctx.content.lines().nth(line - 1).unwrap_or("");
                        // Only check the line at ListItem.position.start.line
                        // Defensive: ensure this is the first line of the list item, and parent is a List
                        // (We are already in process_list_block, so parent is List)
                        if let Some(cap) = LIST_REGEX.captures(line_str) {
                            let marker = cap[2].to_string();
                            let spaces = cap[3].len();
                            let curr_indent = line_str.chars().take_while(|c| c.is_whitespace()).count();
                            let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                ListType::Ordered
                            } else {
                                ListType::Unordered
                            };
                            // Hybrid source-based multi-line detection: scan forward from the marker line
                            let is_multi = {
                                let lines: Vec<&str> = ctx.content.lines().collect();
                                let curr_line_idx = line - 1; // 0-based
                                let mut found_continuation = false;
                                let mut i = curr_line_idx + 1;
                                while i < lines.len() {
                                    let next_line = lines[i];
                                    if next_line.trim().is_empty() {
                                        break;
                                    }
                                    // If next line is a new list item, stop
                                    if is_list_item(next_line).is_some() {
                                        break;
                                    }
                                    let next_indent = next_line.chars().take_while(|c| c.is_whitespace()).count();
                                    if next_indent > curr_indent {
                                        found_continuation = true;
                                        break;
                                    } else {
                                        break;
                                    }
                                    i += 1;
                                }
                                found_continuation
                            };
                            items.push(ItemInfo {
                                li,
                                line,
                                col,
                                line_str,
                                marker,
                                spaces,
                                curr_indent,
                                is_multi,
                                list_type,
                            });
                        }
                    }
                }
            }
            let list_type = if parent_is_ordered.unwrap_or(false) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            // Consistency logic: group items by (is_multi, nesting_level)
            // For each group, determine the expected value (config or most common)
            let mut groups: HashMap<(bool, usize), Vec<&ItemInfo>> = HashMap::new();
            for item in &items {
                let use_multi = nesting_level == 0 && item.is_multi;
                let group_key = (use_multi, nesting_level);
                groups.entry(group_key).or_default().push(item);
            }
            for ((use_multi, nesting_level), group_items) in groups {
                let config_value = if nesting_level > 0 {
                    rule.get_expected_spaces(list_type, false)
                } else {
                    rule.get_expected_spaces(list_type, use_multi)
                };
                // If config_value == 1, use consistency logic
                if config_value == 1 {
                    // Find the most common number of spaces in this group
                    let mut counts = HashMap::new();
                    for item in group_items.iter() {
                        *counts.entry(item.spaces).or_insert(0) += 1;
                    }
                    let (&most_common, _) = counts.iter().max_by_key(|&(_, v)| v).unwrap();
                    for item in group_items {
                        if item.spaces != most_common {
                            warnings.push(LintWarning {
                                rule_name: Some(rule.name()),
                                severity: Severity::Warning,
                                line: item.line,
                                column: item.col + item.curr_indent + item.marker.len(),
                                message: format!(
                                    "Inconsistent spacing after list marker: expected {} space(s) (most common), found {}",
                                    most_common, item.spaces
                                ),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(item.line, 1),
                                    replacement: rule.fix_line(item.line_str, item.list_type, use_multi),
                                }),
                            });
                        }
                    }
                } else {
                    // Config requires a specific value: flag any deviation
                    for item in group_items {
                        let expected_spaces = rule.get_expected_spaces(item.list_type, item.is_multi);
                        if item.spaces != expected_spaces {
                            warnings.push(LintWarning {
                                rule_name: Some(rule.name()),
                                severity: Severity::Warning,
                                line: item.line,
                                column: item.col + item.curr_indent + item.marker.len(),
                                message: format!(
                                    "Expected {} space(s) after list marker, found {}",
                                    expected_spaces, item.spaces
                                ),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(item.line, 1),
                                    replacement: rule.fix_line(item.line_str, item.list_type, item.is_multi),
                                }),
                            });
                        }
                    }
                }
            }
        }
        fn visit_lists(
            node: &Node,
            parent_is_ordered: Option<bool>,
            ctx: &LintContext,
            rule: &MD030ListMarkerSpace,
            line_index: &LineIndex,
            warnings: &mut Vec<LintWarning>,
            nesting_level: usize,
            parent_is_code_or_blockquote: bool,
        ) {
            use markdown::mdast::Node;
            match node {
                Node::List(list) => {
                    process_list_block(list, parent_is_ordered, ctx, rule, line_index, warnings, nesting_level, parent_is_code_or_blockquote);
                    for item in &list.children {
                        visit_lists(item, Some(list.ordered), ctx, rule, line_index, warnings, nesting_level + 1, parent_is_code_or_blockquote);
                    }
                }
                Node::Blockquote(_) | Node::Code(_) | Node::Html(_) => {
                    // Set parent_is_code_or_blockquote = true for children
                    if let Some(children) = node.children() {
                        for child in children {
                            visit_lists(child, parent_is_ordered, ctx, rule, line_index, warnings, nesting_level, true);
                        }
                    }
                }
                Node::Root(root) => {
                    for child in &root.children {
                        visit_lists(child, None, ctx, rule, line_index, warnings, 0, false);
                    }
                }
                _ => {
                    if let Some(children) = node.children() {
                        for child in children {
                            visit_lists(child, parent_is_ordered, ctx, rule, line_index, warnings, nesting_level, parent_is_code_or_blockquote);
                        }
                    }
                }
            }
        }
        visit_lists(&ctx.ast, None, ctx, rule, &line_index, &mut warnings, 0, false);
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if ctx.content.is_empty() {
            return Ok(String::new());
        }
        if !ctx.content.contains('*')
            && !ctx.content.contains('-')
            && !ctx.content.contains('+')
            && !ctx.content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(ctx.content.to_string());
        }
        let lines: Vec<&str> = ctx.content.lines().collect();
        let (is_list_line, multi_line) = self.precompute_states(&lines);
        let mut result_lines = Vec::with_capacity(lines.len());
        for (i, &line) in lines.iter().enumerate() {
            if is_list_line[i] {
                if let Some((list_type, _line_start_match, spaces)) = is_list_item(line) {
                    let expected_spaces = self.get_expected_spaces(list_type, multi_line[i]);
                    if spaces != expected_spaces {
                        result_lines.push(self.fix_line(line, list_type, multi_line[i]));
                    } else {
                        result_lines.push(line.to_string());
                    }
                } else {
                    result_lines.push(line.to_string());
                }
            } else {
                result_lines.push(line.to_string());
            }
        }
        let mut result = result_lines.join("\n");
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
        ctx.content.is_empty()
            || (!ctx.content.contains('*')
                && !ctx.content.contains('-')
                && !ctx.content.contains('+')
                && !ctx.content.contains(|c: char| c.is_ascii_digit()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("ul-single".to_string(), toml::Value::Integer(self.ul_single as i64));
        map.insert("ul-multi".to_string(), toml::Value::Integer(self.ul_multi as i64));
        map.insert("ol-single".to_string(), toml::Value::Integer(self.ol_single as i64));
        map.insert("ol-multi".to_string(), toml::Value::Integer(self.ol_multi as i64));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        let ul_single = crate::config::get_rule_config_value::<usize>(config, "MD030", "ul-single").unwrap_or(1);
        let ul_multi  = crate::config::get_rule_config_value::<usize>(config, "MD030", "ul-multi").unwrap_or(1);
        let ol_single = crate::config::get_rule_config_value::<usize>(config, "MD030", "ol-single").unwrap_or(1);
        let ol_multi  = crate::config::get_rule_config_value::<usize>(config, "MD030", "ol-multi").unwrap_or(1);
        Box::new(MD030ListMarkerSpace::new(ul_single, ul_multi, ol_single, ol_multi))
    }
}

impl DocumentStructureExtensions for MD030ListMarkerSpace {
    fn has_relevant_elements(&self, _ctx: &crate::lint_context::LintContext, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have warnings for both items with incorrect spacing"
        );
        let content = "* Item 1\n  continued on next line\n* Item 2";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Default spacing for single and multiline is 1"
        );
        let custom_rule = MD030ListMarkerSpace::new(1, 2, 1, 2);
        let content = "* Item 1\n  continued on next line\n*  Item 2 with 2 spaces";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = custom_rule
            .check_with_structure(&ctx, &structure)
            .unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have warnings for both items (wrong spacing)"
        );
    }
}
