use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use toml;
use crate::lint_context::LintContext;
use markdown::mdast::{Node, List, ListItem, Blockquote, Paragraph};

lazy_static! {
    // Updated regex to handle blockquote markers at the beginning of lines
    // This matches: optional blockquote markers (>), whitespace, list marker, space, and content
    static ref LIST_ITEM_RE: Regex = Regex::new(r"^((?:\s*>\s*)*\s*)([-*+])\s+(.*)$").unwrap();
    static ref CODE_BLOCK_MARKER: Regex = Regex::new(r"^(```|~~~)").unwrap();

    // Regex for finding the first list marker in content
    static ref FIRST_LIST_MARKER_RE: Regex = Regex::new(r"(?m)^(\s*)([*+-])(\s+[^*+\-\s]|\s*$)").unwrap();
}

/// Style mode for list markers
#[derive(Debug, Clone, PartialEq)]
pub enum StyleMode {
    /// Enforce a specific marker style
    Specific(String),
    /// Enforce consistency based on the first marker found
    Consistent,
}

/// Rule MD008: Unordered list style
///
/// See [docs/md008.md](../../docs/md008.md) for full documentation, configuration, and examples.
///
/// This rule enforces a specific marker character (* or - or +) for unordered lists
#[derive(Debug, Clone)]
pub struct MD008ULStyle {
    pub style_mode: StyleMode,
}

impl Default for MD008ULStyle {
    fn default() -> Self {
        Self {
            style_mode: StyleMode::Consistent,
        }
    }
}

impl MD008ULStyle {
    pub fn new(style_mode: StyleMode) -> Self {
        Self { style_mode }
    }

    /// Create a new instance with specific style mode
    pub fn with_mode(style: char, style_mode: StyleMode) -> Self {
        match style_mode {
            StyleMode::Specific(_) => Self {
                style_mode: StyleMode::Specific(style.to_string()),
            },
            StyleMode::Consistent => Self {
                style_mode: StyleMode::Consistent,
            },
        }
    }

    /// Parse a list item line, returning (indentation, marker, content_length)
    fn parse_list_item(line: &str) -> Option<(usize, char, usize)> {
        LIST_ITEM_RE.captures(line).map(|caps| {
            let whitespace = caps.get(1).map_or("", |m| m.as_str());
            let marker = caps
                .get(2)
                .map_or("", |m| m.as_str())
                .chars()
                .next()
                .unwrap();
            let content = caps.get(3).map_or("", |m| m.as_str());

            (whitespace.len(), marker, content.len())
        })
    }

    /// Precompute code blocks for faster checking
    fn precompute_code_blocks(content: &str) -> Vec<bool> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut code_blocks = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            if CODE_BLOCK_MARKER.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
            }
            code_blocks[i] = in_code_block;
        }

        code_blocks
    }

    /// Check if content contains any list items (for fast skipping)
    #[inline]
    fn contains_potential_list_items(content: &str) -> bool {
        content.contains('*') || content.contains('-') || content.contains('+')
    }

    /// Helper method to find the first list marker in content
    fn find_first_list_marker(content: &str) -> Option<String> {
        if let Some(captures) = FIRST_LIST_MARKER_RE.captures(content) {
            if let Some(marker) = captures.get(2) {
                return Some(marker.as_str().to_string());
            }
        }

        None
    }

    /// Get the style from StyleMode for checking list items
    fn get_style_from_mode(&self, content: &str) -> String {
        match &self.style_mode {
            StyleMode::Specific(style) => style.clone(),
            StyleMode::Consistent => {
                // Find the first list marker to determine style
                Self::find_first_list_marker(content).unwrap_or_else(|| "*".to_string())
            }
        }
    }

    /// AST-based traversal: collect all unordered list items, grouped by blockquote context
    fn collect_unordered_list_items<'a>(
        node: &'a Node,
        blockquote_depth: usize,
        items: &mut Vec<(usize, &'a ListItem, char, usize, usize)>,
        ctx: &LintContext,
    ) {
        match node {
            Node::Blockquote(Blockquote { children, .. }) => {
                for child in children {
                    Self::collect_unordered_list_items(child, blockquote_depth + 1, items, ctx);
                }
            }
            Node::List(List { children, ordered: false, .. }) => {
                for item in children {
                    if let Node::ListItem(li) = item {
                        // Find the marker and line number
                        if let Some((marker, line, col)) = Self::get_list_marker_info(li, ctx) {
                            items.push((blockquote_depth, li, marker, line, col));
                        }
                        // Recurse into nested lists
                        for child in &li.children {
                            Self::collect_unordered_list_items(child, blockquote_depth, items, ctx);
                        }
                    }
                }
            }
            Node::List(List { children, ordered: true, .. }) => {
                // Skip ordered lists
                for item in children {
                    if let Node::ListItem(li) = item {
                        for child in &li.children {
                            Self::collect_unordered_list_items(child, blockquote_depth, items, ctx);
                        }
                    }
                }
            }
            Node::Root(root) => {
                for child in &root.children {
                    Self::collect_unordered_list_items(child, blockquote_depth, items, ctx);
                }
            }
            _ => {
                // Recurse into children if any
                if let Some(children) = node.children() {
                    for child in children {
                        Self::collect_unordered_list_items(child, blockquote_depth, items, ctx);
                    }
                }
            }
        }
    }

    /// Extract the marker character, line, and column for a ListItem
    fn get_list_marker_info<'a>(li: &'a ListItem, ctx: &LintContext) -> Option<(char, usize, usize)> {
        // Try to find the marker in the source text
        if let Some(pos) = li.position.as_ref() {
            let start = pos.start.offset;
            let (line, col) = ctx.offset_to_line_col(start);
            // Get the line from the source
            let line_str = ctx.content.lines().nth(line - 1)?;
            // Find the first non-whitespace character
            let marker = line_str.chars().find(|c| *c == '*' || *c == '-' || *c == '+')?;
            Some((marker, line, col))
        } else {
            None
        }
    }

    /// AST-based: get all unordered list items grouped by blockquote depth
    fn group_items_by_blockquote<'a>(
        ctx: &'a LintContext,
    ) -> std::collections::HashMap<usize, Vec<(char, usize, usize)>> {
        let mut items = Vec::new();
        Self::collect_unordered_list_items(&ctx.ast, 0, &mut items, ctx);
        let mut groups: std::collections::HashMap<usize, Vec<(char, usize, usize)>> = std::collections::HashMap::new();
        for (depth, _li, marker, line, col) in items {
            groups.entry(depth).or_default().push((marker, line, col));
        }
        groups
    }

    /// AST-based: get the expected style for a group
    fn get_expected_style_for_group(&self, group: &[(char, usize, usize)]) -> char {
        match &self.style_mode {
            StyleMode::Specific(style) => style.chars().next().unwrap_or('*'),
            StyleMode::Consistent => group.first().map(|(m, _, _)| *m).unwrap_or('*'),
        }
    }
}

impl Rule for MD008ULStyle {
    fn name(&self) -> &'static str {
        "MD008"
    }

    fn description(&self) -> &'static str {
        "Unordered list style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Fast path
        if ctx.content.is_empty() || !Self::contains_potential_list_items(ctx.content) {
            return Ok(Vec::new());
        }
        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();
        let groups = Self::group_items_by_blockquote(ctx);
        for (blockquote_depth, group) in groups {
            let expected = self.get_expected_style_for_group(&group);
            for (marker, line, col) in group {
                let expected_char = expected;
                if marker != expected_char {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line,
                        column: col,
                        message: format!(
                            "Unordered list item marker '{}' should be '{}'",
                            marker, expected_char
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line, col),
                            replacement: format!("{}", expected_char),
                        }),
                    });
                }
            }
        }
        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, ctx: &crate::lint_context::LintContext, doc_structure: &DocumentStructure) -> LintResult {
        // Fast path - if content is empty or no list items, return empty result
        if ctx.content.is_empty() || doc_structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = ctx.content.lines().collect();

        // Get the target style based on mode
        let expected_style = self.get_style_from_mode(ctx.content);

        let mut in_blockquote = false;

        for &line_num in &doc_structure.list_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Skip code blocks
            if doc_structure.is_in_code_block(line_num) {
                continue;
            }

            let trimmed = line.trim_start();
            // Track blockquote state
            if trimmed.starts_with('>') {
                in_blockquote = true;
            } else if !trimmed.is_empty() {
                in_blockquote = false;
            }

            if let Some((indent, marker, _content_len)) = Self::parse_list_item(line) {
                // Skip if in blockquote since those are handled separately
                if in_blockquote {
                    continue;
                }

                let expected_char = expected_style.chars().next().unwrap_or('*');
                if marker != expected_char {
                    let _trimmed_line = line.trim_start();
                    // For regular list items, just use indentation
                    let line_start = " ".repeat(indent);

                    // Find the list marker position and content after it
                    let list_marker_pos = line.find(marker).unwrap_or(0);
                    let content_after_marker = if list_marker_pos + 1 < line.len() {
                        &line[list_marker_pos + 1..]
                    } else {
                        ""
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num,
                        column: indent + 1,
                        message: format!(
                            "Unordered list item marker '{}' should be '{}'",
                            marker, expected_char
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement: format!(
                                "{}{}{}",
                                line_start, expected_char, content_after_marker
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if ctx.content.is_empty() || !Self::contains_potential_list_items(ctx.content) {
            return Ok(ctx.content.to_string());
        }
        let mut lines: Vec<String> = ctx.content.lines().map(|l| l.to_string()).collect();
        let groups = Self::group_items_by_blockquote(ctx);
        for (_blockquote_depth, group) in &groups {
            let expected = self.get_expected_style_for_group(group);
            for (marker, line, col) in group {
                let expected_char = expected;
                if *marker != expected_char {
                    // Replace the marker at the correct position in the line
                    if let Some(line_str) = lines.get_mut(line - 1) {
                        let mut chars: Vec<char> = line_str.chars().collect();
                        // Find the marker position
                        let mut idx = 0;
                        while idx < chars.len() && (chars[idx] == ' ' || chars[idx] == '\t') {
                            idx += 1;
                        }
                        if idx < chars.len() && (chars[idx] == '*' || chars[idx] == '-' || chars[idx] == '+') {
                            chars[idx] = expected_char;
                            *line_str = chars.into_iter().collect();
                        }
                    }
                }
            }
        }
        // Reconstruct the content
        let mut result = lines.join("\n");
        // Preserve trailing newlines
        let trailing_newlines_count = ctx.content.chars().rev().take_while(|&c| c == '\n').count();
        let result_trailing_newlines = result.chars().rev().take_while(|&c| c == '\n').count();
        if trailing_newlines_count > result_trailing_newlines {
            result.push_str(&"\n".repeat(trailing_newlines_count - result_trailing_newlines));
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if we should skip this rule based on content
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !Self::contains_potential_list_items(ctx.content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        let style_str = match &self.style_mode {
            StyleMode::Consistent => "consistent".to_string(),
            StyleMode::Specific(s) => s.clone(),
        };
        map.insert("style".to_string(), toml::Value::String(style_str));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD008", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style_mode = match style.as_str() {
            "*" => StyleMode::Specific("*".to_string()),
            "-" => StyleMode::Specific("-".to_string()),
            "+" => StyleMode::Specific("+".to_string()),
            "consistent" => StyleMode::Consistent,
            _ => StyleMode::Consistent, // Default to Consistent if invalid
        };
        Box::new(MD008ULStyle::new(style_mode))
    }
}

impl DocumentStructureExtensions for MD008ULStyle {
    fn has_relevant_elements(&self, ctx: &crate::lint_context::LintContext, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_with_document_structure() {
        let rule = MD008ULStyle::default();
        let content = "* Item 1\n* Item 2\n* Item 3";
        let ctx = LintContext::new(content);
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for consistent style"
        );

        let content = "- Item 1\n- Item 2\n- Item 3";
        let ctx = LintContext::new(content);
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for consistent style with dashes"
        );

        let content = "- Item 1\n- Item 2\n- Item 3";
        let ctx = LintContext::new(content);
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Expected no warnings for consistent style with dashes"
        );

        let content = "- Item 1\n* Item 2\n+ Item 3";
        let ctx = LintContext::new(content);
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected warnings for mixed list markers"
        );
    }

    #[test]
    fn test_trailing_newlines_preservation() {
        let rule = MD008ULStyle::default();
        // Test with multiple trailing newlines
        let content = "* Item 1\n* Item 2\n- Item 3\n\n\n";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();
        assert_eq!(
            result, "* Item 1\n* Item 2\n* Item 3\n\n\n",
            "Trailing newlines should be preserved"
        );
    }

    #[test]
    fn test_blockquote_handling() {
        let rule = MD008ULStyle::default();
        // Test with blockquote content
        let content = "> * Item 1\n> * Item 2\n> - Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Expected warning for mixed markers in blockquote"
        );
        // Mixed blockquote and regular list items
        let content = "> * Item 1\n* Item 2\n> - Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Expected warning for mixed markers in blockquote and regular list"
        );
    }
}
