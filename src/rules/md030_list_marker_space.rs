//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rules::list_utils::ListType;
use crate::utils::range_utils::LineIndex;

use crate::lint_context::LintContext;
use crate::rule::{Fix, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use fancy_regex::Regex;
use lazy_static::lazy_static;
use markdown::mdast::{List, Node};
use toml;

lazy_static! {
    // Updated regex to better handle 0 spaces case with lookahead
    static ref LIST_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)(?=\S|$)").unwrap();
    // Also match list markers without spaces
    static ref LIST_NO_SPACE_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(?!\s)").unwrap();
    // Regex used for fixing - ensures exactly the required number of spaces
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
        if let Ok(Some(caps)) = LIST_FIX_REGEX.captures(line) {
            let indent = caps.get(1).map_or("", |m| m.as_str());
            let marker = caps.get(2).map_or("", |m| m.as_str());
            format!("{}{}{}", indent, marker, " ".repeat(expected_spaces))
        } else {
            line.to_string()
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
            _nesting_level: usize,
            parent_is_code_or_blockquote: bool,
        ) {
            use markdown::mdast::Node;
            struct ItemInfo<'a> {
                line: usize,
                col: usize,
                line_str: &'a str,
                spaces: usize,
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
                        // Check both patterns
                        let (marker, spaces) = if let Ok(Some(cap)) = LIST_REGEX.captures(line_str)
                        {
                            (
                                cap.get(2).map_or("", |m| m.as_str()).to_string(),
                                cap.get(3).map_or("", |m| m.as_str()).len(),
                            )
                        } else if let Ok(Some(cap)) = LIST_NO_SPACE_REGEX.captures(line_str) {
                            (cap.get(2).map_or("", |m| m.as_str()).to_string(), 0)
                        } else {
                            continue;
                        };
                        let curr_indent =
                            line_str.chars().take_while(|c| c.is_whitespace()).count();
                        let list_type =
                            if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                ListType::Ordered
                            } else {
                                ListType::Unordered
                            };
                        // Find the next sibling's line number (at the same or lesser indentation)
                        let mut next_sibling_line = lines.len();
                        for other_item in &list_node.children {
                            if let Node::ListItem(other_li) = other_item {
                                if let Some(other_pos) = other_li.position.as_ref() {
                                    let other_start = other_pos.start.offset;
                                    let (other_line, _) = ctx.offset_to_line_col(other_start);
                                    if other_line > line && other_line < next_sibling_line {
                                        let other_line_str =
                                            ctx.content.lines().nth(other_line - 1).unwrap_or("");
                                        let other_indent = other_line_str
                                            .chars()
                                            .take_while(|c| c.is_whitespace())
                                            .count();
                                        if other_indent <= curr_indent {
                                            next_sibling_line = other_line;
                                        }
                                    }
                                }
                            }
                        }
                        let mut found_continuation = false;
                        let mut i = line;
                        while i < next_sibling_line {
                            let next_line = ctx.content.lines().nth(i).unwrap_or("");
                            let next_indent =
                                next_line.chars().take_while(|c| c.is_whitespace()).count();
                            let is_next_list = LIST_REGEX.captures(next_line).is_ok()
                                && LIST_REGEX.captures(next_line).as_ref().unwrap().is_some()
                                || LIST_NO_SPACE_REGEX.captures(next_line).is_ok()
                                    && LIST_NO_SPACE_REGEX
                                        .captures(next_line)
                                        .as_ref()
                                        .unwrap()
                                        .is_some();
                            if next_line.trim().is_empty() {
                                // Only treat as multi-line if the blank line is indented more than the marker (i.e., part of the item's content)
                                if next_indent > curr_indent {
                                    found_continuation = true;
                                }
                                break;
                            }
                            if is_next_list && next_indent > curr_indent {
                                found_continuation = true; // parent of nested list is multi-line
                                break;
                            }
                            if next_indent > curr_indent {
                                found_continuation = true; // paragraph continuation
                                break;
                            }
                            i += 1;
                        }
                        let is_multi = found_continuation;
                        items.push(ItemInfo {
                            line,
                            col,
                            line_str,
                            spaces,
                            is_multi,
                            list_type,
                        });
                    }
                }
            }
            let _list_type = if parent_is_ordered.unwrap_or(false) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            // Per-item logic: only require multi-line spacing for items that are actually multi-line
            for item in &items {
                let config_value = rule.get_expected_spaces(item.list_type, item.is_multi);
                if item.spaces < config_value {
                    warnings.push(LintWarning {
                        rule_name: Some(rule.name()),
                        severity: Severity::Warning,
                        line: item.line,
                        column: item.col + 1,
                        message: format!(
                            "Expected at least {} space{} after list marker, found {}",
                            config_value,
                            if config_value == 1 { "" } else { "s" },
                            item.spaces
                        ),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(item.line, 1),
                            replacement: rule.fix_line(
                                item.line_str,
                                item.list_type,
                                item.is_multi,
                            ),
                        }),
                    });
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
            _nesting_level: usize,
            parent_is_code_or_blockquote: bool,
        ) {
            use markdown::mdast::Node;
            match node {
                Node::List(list) => {
                    process_list_block(
                        list,
                        parent_is_ordered,
                        ctx,
                        rule,
                        line_index,
                        warnings,
                        0,
                        parent_is_code_or_blockquote,
                    );
                    for item in &list.children {
                        visit_lists(
                            item,
                            Some(list.ordered),
                            ctx,
                            rule,
                            line_index,
                            warnings,
                            0,
                            parent_is_code_or_blockquote,
                        );
                    }
                }
                Node::Blockquote(_) | Node::Code(_) | Node::Html(_) => {
                    // Set parent_is_code_or_blockquote = true for children
                    if let Some(children) = node.children() {
                        for child in children {
                            visit_lists(
                                child,
                                parent_is_ordered,
                                ctx,
                                rule,
                                line_index,
                                warnings,
                                0,
                                true,
                            );
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
                            visit_lists(
                                child,
                                parent_is_ordered,
                                ctx,
                                rule,
                                line_index,
                                warnings,
                                0,
                                parent_is_code_or_blockquote,
                            );
                        }
                    }
                }
            }
        }
        visit_lists(
            &ctx.ast,
            None,
            ctx,
            rule,
            &line_index,
            &mut warnings,
            0,
            false,
        );
        Ok(warnings)
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
        map.insert(
            "ul-single".to_string(),
            toml::Value::Integer(self.ul_single as i64),
        );
        map.insert(
            "ul-multi".to_string(),
            toml::Value::Integer(self.ul_multi as i64),
        );
        map.insert(
            "ol-single".to_string(),
            toml::Value::Integer(self.ol_single as i64),
        );
        map.insert(
            "ol-multi".to_string(),
            toml::Value::Integer(self.ol_multi as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        fn get_key(
            config: &crate::config::Config,
            rule: &str,
            dash: &str,
            underscore: &str,
        ) -> usize {
            crate::config::get_rule_config_value::<usize>(config, rule, dash)
                .or_else(|| crate::config::get_rule_config_value::<usize>(config, rule, underscore))
                .unwrap_or(1)
        }
        let ul_single = get_key(config, "MD030", "ul-single", "ul_single");
        let ul_multi = get_key(config, "MD030", "ul-multi", "ul_multi");
        let ol_single = get_key(config, "MD030", "ol-single", "ol_single");
        let ol_multi = get_key(config, "MD030", "ol-multi", "ol_multi");
        Box::new(MD030ListMarkerSpace::new(
            ul_single, ul_multi, ol_single, ol_multi,
        ))
    }

    fn fix(&self, _ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        Err(crate::rule::LintError::FixFailed("Automatic fixing is not supported for MD030. See todos/md030_fix_strategy.md for details.".to_string()))
    }
}

impl DocumentStructureExtensions for MD030ListMarkerSpace {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
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
        // Only expect warnings for too few spaces after the marker (should be empty)
        assert!(
            result.is_empty(),
            "Should not flag lines with too many spaces after list marker"
        );
    }
}
