//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rules::list_utils::ListType;
use crate::utils::range_utils::LineIndex;

use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use fancy_regex::Regex;
use lazy_static::lazy_static;
use markdown::mdast::{List, ListItem, Node};
use toml;
use crate::regex_lazy;
use crate::rules::list_utils::ListUtils;
use crate::utils::element_cache::get_element_cache;
use std::collections::HashMap;

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

    fn precompute_states(&self, lines: &[&str]) -> (Vec<bool>, Vec<bool>) {
        let mut is_list_line = vec![false; lines.len()];
        let mut multi_line = vec![false; lines.len()];
        let mut in_code_block = false;

        // First pass: mark code blocks and list items
        for (i, &line) in lines.iter().enumerate() {
            if CODE_BLOCK_REGEX.is_match(line).unwrap_or(false) {
                in_code_block = !in_code_block;
                continue;
            }
            if !in_code_block {
                // Check both patterns
                if let Ok(Some(_)) = LIST_REGEX.captures(line) {
                    is_list_line[i] = true;
                } else if let Ok(Some(_)) = LIST_NO_SPACE_REGEX.captures(line) {
                    is_list_line[i] = true;
                }
            }
        }

        // Second pass: compute multi-line states
        for i in 0..lines.len() {
            if is_list_line[i] {
                let mut found_continuation = false;
                let curr_indent = lines[i].chars().take_while(|c| c.is_whitespace()).count();
                let j = i + 1;
                while j < lines.len() {
                    let next_line = &lines[j][..];
                    if next_line.trim().is_empty() {
                        found_continuation = true; // blank line always makes multi-line
                        break;
                    }
                    let next_indent = next_line.chars().take_while(|c| c.is_whitespace()).count();
                    let is_next_list = LIST_REGEX.captures(next_line).is_ok()
                        && LIST_REGEX.captures(next_line).as_ref().unwrap().is_some()
                        || LIST_NO_SPACE_REGEX.captures(next_line).is_ok()
                            && LIST_NO_SPACE_REGEX
                                .captures(next_line)
                                .as_ref()
                                .unwrap()
                                .is_some();
                    if is_next_list {
                        if next_indent > curr_indent {
                            found_continuation = true; // parent of nested list is multi-line
                        }
                        break;
                    }
                    if next_indent > curr_indent {
                        found_continuation = true; // paragraph continuation
                        break;
                    } else {
                        break;
                    }
                }
                multi_line[i] = found_continuation;
            }
        }

        (is_list_line, multi_line)
    }
}

// Utility: Strip all leading blockquote prefixes (inspired by MD007)
fn strip_blockquote_prefix(line: &str) -> (String, &str) {
    let mut rest = line;
    let mut prefix = String::new();
    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('>') {
            let after = &trimmed[1..];
            let mut chars = after.chars();
            let mut space_count = 0;
            if let Some(' ') = chars.next() {
                space_count = 1;
            }
            let (spaces, after_marker) = after.split_at(space_count);
            prefix.push('>');
            prefix.push_str(spaces);
            rest = after_marker;
        } else {
            break;
        }
    }
    (prefix, rest)
}

// Returns (nesting_levels, is_multi_line) for each line
fn calculate_nesting_and_multiline(lines: &[&str]) -> (Vec<usize>, Vec<bool>) {
    let mut nesting_levels = vec![0; lines.len()];
    let mut is_multi_line = vec![false; lines.len()];
    let mut stack: Vec<(usize, String)> = Vec::new(); // (indent, marker)
    let mut in_code_block = false;
    let mut code_fence: Option<String> = None;
    for (i, &line) in lines.iter().enumerate() {
        let (bq_prefix, rest) = strip_blockquote_prefix(line);
        let trimmed = rest.trim_start();
        // Code block detection
        if !in_code_block {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_block = true;
                code_fence = Some(trimmed[..3].to_string());
                continue;
            }
        } else {
            if let Some(ref fence) = code_fence {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    code_fence = None;
                }
            }
            continue;
        }
        // List marker detection
        let marker_re = regex_lazy!(r"^([ \t]*)([*+-]|\d+\.)(\s+)(.*)$");
        if let Some(caps) = marker_re.captures(rest) {
            let indent = caps.get(1).map_or("", |m| m.as_str()).len();
            let marker = caps.get(2).map_or("", |m| m.as_str());
            // Stack logic: pop until we find a parent or root
            while let Some(&(last_indent, _)) = stack.last() {
                if indent > last_indent {
                    break;
                } else if indent < last_indent {
                    stack.pop();
                } else {
                    break;
                }
            }
            stack.push((indent, marker.to_string()));
            nesting_levels[i] = stack.len() - 1;
            // Multi-line: look ahead for indented continuation or blank
            let mut j = i + 1;
            while j < lines.len() {
                let (next_bq, next_rest) = strip_blockquote_prefix(lines[j]);
                let next_trim = next_rest.trim_start();
                if next_trim.is_empty() {
                    break;
                }
                let next_indent = next_rest.chars().take_while(|c| c.is_whitespace()).count();
                if marker_re.is_match(next_rest) {
                    break;
                }
                if next_indent > indent {
                    is_multi_line[i] = true;
                    break;
                } else {
                    break;
                }
            }
        } else {
            // Not a list item, keep previous level
            if let Some(&(last_indent, _)) = stack.last() {
                if rest.chars().take_while(|c| c.is_whitespace()).count() > last_indent {
                    // Continuation of previous list item
                    nesting_levels[i] = stack.len() - 1;
                    is_multi_line[i] = true;
                } else {
                    // Not a list or continuation
                    nesting_levels[i] = 0;
                }
            } else {
                nesting_levels[i] = 0;
            }
        }
    }
    (nesting_levels, is_multi_line)
}

// Refactored to group lists by indentation/nesting, not just marker type. For each contiguous block of list items at the same indentation level (including nested lists), determine if any item (including nested) is multi-line. Apply the parent list's multi-line status to all items in the block, including nested ones. This matches markdownlint's behavior for nested/multi-line lists.

// Helper struct to represent a list block and its items
struct ListBlock<'a> {
    lines: Vec<(usize, &'a str, usize)>, // (original line index, line content, indent level)
    list_type: ListType,
    is_multi: bool,
}

fn parse_lists_by_indent<'a>(lines: &'a [&str]) -> Vec<ListBlock<'a>> {
    let mut lists = Vec::new();
    let mut in_code_block = false;
    let mut code_fence: Option<String> = None;
    let mut current_list: Option<ListBlock> = None;
    let marker_re = regex_lazy!(r"^([ \t]*)([*+-]|\d+\.)(\s+)(.*)$");
    for (i, &line) in lines.iter().enumerate() {
        let (_bq_prefix, rest) = strip_blockquote_prefix(line);
        let trimmed = rest.trim_start();
        // Code block detection
        if !in_code_block {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_block = true;
                code_fence = Some(trimmed[..3].to_string());
                if let Some(list) = current_list.take() {
                    lists.push(list);
                }
                continue;
            }
        } else {
            if let Some(ref fence) = code_fence {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    code_fence = None;
                }
            }
            if let Some(list) = current_list.take() {
                lists.push(list);
            }
            continue;
        }
        // List marker detection
        if let Some(caps) = marker_re.captures(rest) {
            let indent = caps.get(1).map_or("", |m| m.as_str()).len();
            let marker = caps.get(2).map_or("", |m| m.as_str());
            let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            // Start new list if needed (indentation change)
            let mut start_new = false;
            if let Some(ref mut list) = current_list {
                if list.lines.last().map_or(false, |&(_, _, last_indent)| last_indent != indent) {
                    start_new = true;
                }
            }
            if start_new {
                if let Some(list) = current_list.take() {
                    lists.push(list);
                }
            }
            if current_list.is_none() {
                current_list = Some(ListBlock {
                    lines: Vec::new(),
                    list_type,
                    is_multi: false,
                });
            }
            if let Some(ref mut list) = current_list {
                list.lines.push((i, line, indent));
            }
        } else {
            // Not a list item
            if let Some(ref mut list) = current_list {
                // If indented or blank, treat as possible continuation
                if rest.trim().is_empty() || rest.chars().take_while(|c| c.is_whitespace()).count() > 0 {
                    let indent = rest.chars().take_while(|c| c.is_whitespace()).count();
                    list.lines.push((i, line, indent));
                } else {
                    // End current list
                    if let Some(list) = current_list.take() {
                        lists.push(list);
                    }
                }
            }
        }
    }
    if let Some(list) = current_list.take() {
        lists.push(list);
    }
    lists
}

fn is_list_block_multi_line(list: &ListBlock) -> bool {
    // If any item in the list block has a continuation (next line is indented or blank), it's multi-line
    let marker_re = regex_lazy!(r"^([ \t]*)([*+-]|\d+\.)(\s+)(.*)$");
    let mut prev_item_idx: Option<usize> = None;
    for (idx, &(_i, line, _indent)) in list.lines.iter().enumerate() {
        let (_bq_prefix, rest) = strip_blockquote_prefix(line);
        if let Some(_caps) = marker_re.captures(rest) {
            if let Some(prev) = prev_item_idx {
                // Check lines between prev and current for continuation
                for j in (prev+1)..idx {
                    let (_bq, between) = strip_blockquote_prefix(list.lines[j].1);
                    if between.trim().is_empty() || between.chars().take_while(|c| c.is_whitespace()).count() > 0 {
                        return true;
                    }
                }
            }
            prev_item_idx = Some(idx);
        }
    }
    false
}

// Refined: robust code block detection and stack clearing on interruption, matching markdownlint's implementation.

struct ListStackItem {
    list_type: ListType,
    indent: usize,
    is_multi: bool,
    start_idx: usize,
}

fn analyze_lists_stack<'a>(lines: &'a [&str]) -> Vec<(usize, &'a str, ListType, bool)> {
    let marker_re = regex_lazy!(r"^([ \t]*)([*+-]|\d+\.)(\s+)(.*)$");
    let mut stack: Vec<ListStackItem> = Vec::new();
    let mut in_code_block = false;
    let mut code_fence: Option<String> = None;
    let mut result = Vec::new();
    let mut code_block_lines = vec![false; lines.len()];
    // First pass: mark code block lines
    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !in_code_block {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_block = true;
                code_fence = Some(trimmed[..3].to_string());
                code_block_lines[i] = true;
                continue;
            }
        } else {
            code_block_lines[i] = true;
            if let Some(ref fence) = code_fence {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    code_fence = None;
                }
            }
            continue;
        }
    }
    // Second pass: stack-based list analysis, skipping code block lines
    let mut stack: Vec<ListStackItem> = Vec::new();
    for (i, &line) in lines.iter().enumerate() {
        if code_block_lines[i] {
            stack.clear();
            continue;
        }
        let (_bq_prefix, rest) = strip_blockquote_prefix(line);
        if let Some(caps) = marker_re.captures(rest) {
            let indent = caps.get(1).map_or("", |m| m.as_str()).len();
            let marker = caps.get(2).map_or("", |m| m.as_str());
            let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            while let Some(top) = stack.last() {
                if indent > top.indent {
                    break;
                }
                stack.pop();
            }
            stack.push(ListStackItem {
                list_type,
                indent,
                is_multi: false,
                start_idx: i,
            });
            result.push((i, line, list_type, false));
        } else {
            if let Some(top) = stack.last_mut() {
                let curr_indent = rest.chars().take_while(|c| c.is_whitespace()).count();
                if rest.trim().is_empty() || curr_indent > top.indent {
                    for item in stack.iter_mut() {
                        item.is_multi = true;
                    }
                } else {
                    stack.clear();
                }
            }
        }
    }
    // Third pass: propagate multi-line status to all items in the same list stack
    let mut stack: Vec<ListStackItem> = Vec::new();
    let mut final_result = Vec::new();
    for (i, &line) in lines.iter().enumerate() {
        if code_block_lines[i] {
            stack.clear();
            continue;
        }
        let (_bq_prefix, rest) = strip_blockquote_prefix(line);
        if let Some(caps) = marker_re.captures(rest) {
            let indent = caps.get(1).map_or("", |m| m.as_str()).len();
            let marker = caps.get(2).map_or("", |m| m.as_str());
            let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            while let Some(top) = stack.last() {
                if indent > top.indent {
                    break;
                }
                stack.pop();
            }
            let is_multi = stack.iter().any(|item| item.is_multi);
            stack.push(ListStackItem {
                list_type,
                indent,
                is_multi,
                start_idx: i,
            });
            final_result.push((i, line, list_type, is_multi));
        } else {
            if let Some(top) = stack.last_mut() {
                let curr_indent = rest.chars().take_while(|c| c.is_whitespace()).count();
                if rest.trim().is_empty() || curr_indent > top.indent {
                    for item in stack.iter_mut() {
                        item.is_multi = true;
                    }
                } else {
                    stack.clear();
                }
            }
        }
    }
    final_result
}

#[derive(Debug)]
struct ListContext {
    parent: Option<usize>,
    children: Vec<usize>,
    indices: Vec<usize>, // indices into list_items
    is_multi: bool,
}

#[derive(Debug)]
struct ListItemInfo {
    line_idx: usize,
    indent: usize,
    marker: String,
    list_type: ListType,
    parent_ctx: Option<usize>,
    is_multi: bool,
}

fn analyze_lists_tree<'a>(lines: &'a [&str]) -> (Vec<ListContext>, Vec<ListItemInfo>) {
    let mut contexts: Vec<ListContext> = Vec::new();
    let mut all_items: Vec<ListItemInfo> = Vec::new();
    let mut stack: Vec<usize> = Vec::new(); // indices into contexts
    let mut in_code_block = false;
    let mut code_fence: Option<String> = None;
    let mut code_block_lines = vec![false; lines.len()];
    // First pass: mark code block lines
    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !in_code_block {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_block = true;
                code_fence = Some(trimmed[..3].to_string());
                code_block_lines[i] = true;
                continue;
            }
        } else {
            code_block_lines[i] = true;
            if let Some(ref fence) = code_fence {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    code_fence = None;
                }
            }
            continue;
        }
    }
    // Second pass: build list contexts and items
    for (i, &line) in lines.iter().enumerate() {
        if code_block_lines[i] {
            stack.clear();
            continue;
        }
        let (bq_prefix, rest) = strip_blockquote_prefix(line);
        if let Some(list_item) = ListUtils::parse_list_item(rest) {
            let indent = list_item.indentation;
            let marker = list_item.marker.clone();
            let list_type = match list_item.marker_type {
                crate::rules::list_utils::ListMarkerType::Ordered => ListType::Ordered,
                _ => ListType::Unordered,
            };
            // Find parent context (by indent and marker type)
            while let Some(&ctx_idx) = stack.last() {
                let last_item_idx = *contexts[ctx_idx].indices.last().unwrap();
                let last_item = &all_items[last_item_idx];
                if indent > last_item.indent {
                    break;
                }
                stack.pop();
            }
            let parent_ctx = stack.last().copied();
            // Start new context if needed (indent or marker type change)
            let ctx_idx = if let Some(&ctx_idx) = stack.last() {
                let last_item_idx = *contexts[ctx_idx].indices.last().unwrap();
                let last_item = &all_items[last_item_idx];
                if last_item.indent == indent && last_item.marker == marker {
                    ctx_idx
                } else {
                    // New context
                    let new_idx = contexts.len();
                    contexts.push(ListContext {
                        parent: parent_ctx,
                        children: Vec::new(),
                        indices: Vec::new(),
                        is_multi: false,
                    });
                    stack.push(new_idx);
                    new_idx
                }
            } else {
                // New root context
                let new_idx = contexts.len();
                contexts.push(ListContext {
                    parent: parent_ctx,
                    children: Vec::new(),
                    indices: Vec::new(),
                    is_multi: false,
                });
                stack.push(new_idx);
                new_idx
            };
            // Determine if this item is multi-line (look ahead for indented/blank/continuation)
            let mut is_multi = false;
            let mut j = i + 1;
            while j < lines.len() {
                if code_block_lines[j] {
                    break;
                }
                let (_bq2, rest2) = strip_blockquote_prefix(lines[j]);
                if rest2.trim().is_empty() {
                    is_multi = true;
                    break;
                }
                if let Some(_) = ListUtils::parse_list_item(rest2) {
                    break;
                }
                let next_indent = rest2.chars().take_while(|c| c.is_whitespace()).count();
                if next_indent > indent {
                    is_multi = true;
                    break;
                } else {
                    break;
                }
            }
            let item_idx = all_items.len();
            all_items.push(ListItemInfo {
                line_idx: i,
                indent,
                marker: marker.clone(),
                list_type,
                parent_ctx: Some(ctx_idx),
                is_multi,
            });
            contexts[ctx_idx].indices.push(item_idx);
            if let Some(pidx) = parent_ctx {
                contexts[pidx].children.push(ctx_idx);
            }
        } else {
            // Not a list item, but could be a continuation
            if let Some(&ctx_idx) = stack.last() {
                let curr_indent = rest.chars().take_while(|c| c.is_whitespace()).count();
                let last_item_idx = *contexts[ctx_idx].indices.last().unwrap();
                let last_item = &all_items[last_item_idx];
                if rest.trim().is_empty() || curr_indent > last_item.indent {
                    // Mark all items in this context as multi-line
                    for &item_idx in &contexts[ctx_idx].indices {
                        all_items[item_idx].is_multi = true;
                    }
                    contexts[ctx_idx].is_multi = true;
                } else {
                    stack.clear();
                }
            }
        }
    }
    // For each context, if any item is multi-line, mark context as multi-line
    for ctx in &mut contexts {
        ctx.is_multi = ctx.indices.iter().any(|&idx| all_items[idx].is_multi);
    }
    // Upward propagation: if any child context is multi-line, mark parent as multi-line
    for idx in (0..contexts.len()).rev() {
        let children = contexts[idx].children.clone();
        for &child_idx in &children {
            if contexts[child_idx].is_multi {
                contexts[idx].is_multi = true;
            }
        }
    }
    // Downward propagation: if a context is multi-line, mark all children as multi-line
    fn propagate_multi_down(idx: usize, contexts: &mut [ListContext], parent_multi: bool) {
        if parent_multi {
            contexts[idx].is_multi = true;
        }
        let is_multi = contexts[idx].is_multi;
        let children = contexts[idx].children.clone();
        for child_idx in children {
            propagate_multi_down(child_idx, contexts, is_multi);
        }
    }
    let root_indices: Vec<usize> = (0..contexts.len()).filter(|&idx| contexts[idx].parent.is_none()).collect();
    let root_multis: Vec<bool> = root_indices.iter().map(|&idx| contexts[idx].is_multi).collect();
    for (&idx, &is_multi) in root_indices.iter().zip(root_multis.iter()) {
        propagate_multi_down(idx, &mut contexts, is_multi);
    }
    (contexts, all_items)
}

// Conversion function for ListMarkerType
fn to_list_type(marker_type: &crate::utils::element_cache::ListMarkerType) -> ListType {
    match marker_type {
        crate::utils::element_cache::ListMarkerType::Ordered => ListType::Ordered,
        _ => ListType::Unordered,
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
        let mut ast_items = Vec::new();
        let mut groups = Vec::new();
        let lines_ref: Vec<&str> = ctx.content.lines().collect();
        extract_list_items_and_groups(&ctx.ast, &mut ast_items, &mut groups, &lines_ref);
        // Compute multi-line status for each item
        let mut item_is_multi = vec![false; ast_items.len()];
        for (i, item) in ast_items.iter().enumerate() {
            let line_idx = item.line - 1;
            let curr_indent = item.indent;
            let mut j = line_idx + 1;
            while j < lines_ref.len() {
                let next_line = lines_ref[j];
                if next_line.trim().is_empty() {
                    item_is_multi[i] = true;
                    break;
                }
                let next_indent = next_line.chars().take_while(|c| c.is_whitespace()).count();
                if let Ok(Some(_)) = LIST_REGEX.captures(next_line) {
                    break;
                }
                if next_indent > curr_indent {
                    item_is_multi[i] = true;
                    break;
                } else {
                    break;
                }
            }
        }
        // Recursive multi-line propagation for groups
        let mut group_multi = vec![false; ast_items.len()];
        // Helper: recursively mark all items in a group and its descendants as multi-line
        fn propagate_group_multi(group_idx: usize, groups: &[ListGroup], ast_items: &[AstListItem], item_is_multi: &[bool], group_multi: &mut [bool]) -> bool {
            let group = &groups[group_idx];
            // Mark if any item in this group is multi-line
            let mut any_multi = group.item_indices.iter().any(|&idx| item_is_multi[idx]);
            // Find child groups (those whose min indent > this group's max indent and whose first item comes after this group's last item)
            let this_max_indent = group.item_indices.iter().map(|&idx| ast_items[idx].indent).max().unwrap_or(0);
            let this_last_idx = *group.item_indices.iter().max().unwrap_or(&0);
            for (cgidx, child_group) in groups.iter().enumerate() {
                if cgidx == group_idx { continue; }
                let child_min_indent = child_group.item_indices.iter().map(|&idx| ast_items[idx].indent).min().unwrap_or(0);
                let child_first_idx = *child_group.item_indices.iter().min().unwrap_or(&0);
                if child_min_indent > this_max_indent && child_first_idx > this_last_idx {
                    // Recursively propagate to child
                    let child_multi = propagate_group_multi(cgidx, groups, ast_items, item_is_multi, group_multi);
                    if child_multi {
                        any_multi = true;
                    }
                }
            }
            // If any item in this group or any descendant is multi-line, mark all items in this group as multi-line
            if any_multi {
                for &idx in &group.item_indices {
                    group_multi[idx] = true;
                }
            }
            any_multi
        }
        // Propagate for all groups
        for gidx in 0..groups.len() {
            propagate_group_multi(gidx, &groups, &ast_items, &item_is_multi, &mut group_multi);
        }
        // Use group_multi for check/fix
        for (i, item) in ast_items.iter().enumerate() {
            let line_idx = item.line - 1;
            if line_idx >= lines_ref.len() {
                continue;
            }
            let line = lines_ref[line_idx];
            if let Ok(Some(caps)) = LIST_REGEX.captures(line) {
                let indent = caps.get(1).map_or("", |m| m.as_str());
                let marker = caps.get(2).map_or("", |m| m.as_str());
                let after = caps.get(3).map_or("", |m| m.as_str());
                let actual_spaces = after.len();
                let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    ListType::Ordered
                } else {
                    ListType::Unordered
                };
                let expected_spaces = self.get_expected_spaces(list_type, group_multi[i]);
                if actual_spaces != expected_spaces {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "{}: Expected {} space(s) after list marker, found {}.",
                            self.name(), expected_spaces, actual_spaces
                        ),
                        line: line_idx + 1,
                        column: indent.len() + marker.len() + 1,
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        let mut ast_items = Vec::new();
        let mut groups = Vec::new();
        let lines_ref: Vec<&str> = ctx.content.lines().collect();
        extract_list_items_and_groups(&ctx.ast, &mut ast_items, &mut groups, &lines_ref);
        let mut lines: Vec<String> = ctx.content.lines().map(|s| s.to_string()).collect();
        let mut group_multi = vec![false; ast_items.len()];
        for (i, item) in ast_items.iter().enumerate() {
            let line_idx = item.line - 1;
            let curr_indent = item.indent;
            let mut j = line_idx + 1;
            while j < lines.len() {
                let next_line = lines[j].as_str();
                if next_line.trim().is_empty() {
                    group_multi[i] = true;
                    break;
                }
                let next_indent = next_line.chars().take_while(|c| c.is_whitespace()).count();
                if let Ok(Some(_)) = LIST_REGEX.captures(next_line) {
                    break;
                }
                if next_indent > curr_indent {
                    group_multi[i] = true;
                    break;
                } else {
                    break;
                }
            }
        }
        for (i, item) in ast_items.iter().enumerate() {
            let line_idx = item.line - 1;
            if line_idx >= lines.len() {
                continue;
            }
            let line = lines[line_idx].as_str();
            if let Ok(Some(caps)) = LIST_REGEX.captures(line) {
                let indent = caps.get(1).map_or("", |m| m.as_str());
                let marker = caps.get(2).map_or("", |m| m.as_str());
                let after = caps.get(3).map_or("", |m| m.as_str());
                let actual_spaces = after.len();
                let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    ListType::Ordered
                } else {
                    ListType::Unordered
                };
                let expected_spaces = self.get_expected_spaces(list_type, group_multi[i]);
                if actual_spaces != expected_spaces {
                    let before = &line[..indent.len() + marker.len()];
                    let rest = &line[indent.len() + marker.len() + after.len()..];
                    let spaces = " ".repeat(expected_spaces);
                    let new_line = format!("{}{}{}", before, spaces.as_str(), rest);
                    lines[line_idx] = new_line;
                }
            }
        }
        Ok(lines.join("\n"))
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

fn propagate_multi_down(idx: usize, contexts: &mut [ListContext], parent_multi: bool) {
    if parent_multi {
        contexts[idx].is_multi = true;
    }
    let is_multi = contexts[idx].is_multi;
    let children = contexts[idx].children.clone();
    for child_idx in children {
        propagate_multi_down(child_idx, contexts, is_multi);
    }
}

#[derive(Debug)]
struct AstListItem {
    line: usize,
    spread: bool,
    parent_list_spread: bool,
    marker: String,
    indent: usize,
}

#[derive(Debug)]
struct ListGroup {
    item_indices: Vec<usize>,
}

fn extract_list_items_and_groups(node: &Node, items: &mut Vec<AstListItem>, groups: &mut Vec<ListGroup>, lines: &[&str]) {
    match node {
        Node::Root(root) => {
            for child in &root.children {
                extract_list_items_and_groups(child, items, groups, lines);
            }
        }
        Node::List(list) => {
            let mut group_indices = Vec::new();
            for child in &list.children {
                let before = items.len();
                extract_list_items_and_groups(child, items, groups, lines);
                let after = items.len();
                if after > before {
                    group_indices.extend(before..after);
                }
            }
            if !group_indices.is_empty() {
                groups.push(ListGroup { item_indices: group_indices });
            }
        }
        Node::ListItem(item) => {
            let line = item.position.as_ref().map(|p| p.start.line).unwrap_or(1);
            let line_idx = line - 1;
            let (marker, indent) = if line_idx < lines.len() {
                if let Ok(Some(caps)) = LIST_REGEX.captures(lines[line_idx]) {
                    let indent = caps.get(1).map_or("", |m| m.as_str()).len();
                    let marker = caps.get(2).map_or("", |m| m.as_str()).to_string();
                    (marker, indent)
                } else {
                    ("".to_string(), 0)
                }
            } else {
                ("".to_string(), 0)
            };
            items.push(AstListItem {
                line,
                spread: item.spread,
                parent_list_spread: false, // unused
                marker,
                indent,
            });
            for child in &item.children {
                extract_list_items_and_groups(child, items, groups, lines);
            }
        }
        _ => {}
    }
}
