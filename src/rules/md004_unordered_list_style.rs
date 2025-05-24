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
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructureExtensions;
use crate::LintContext;
use lazy_static::lazy_static;
use markdown::mdast::{ListItem, Node};
use regex::Regex;
use toml;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnorderedListStyle {
    Asterisk,   // "*"
    Plus,       // "+"
    Dash,       // "-"
    Consistent, // Use the first marker in a file consistently
}

impl Default for UnorderedListStyle {
    fn default() -> Self {
        Self::Consistent
    }
}

lazy_static! {
    static ref UNORDERED_LIST_REGEX: Regex = Regex::new(
        // Match unordered list items: optional whitespace, marker, space, then content or end of line
        r"^(?P<indent>\s*)(?P<marker>[*+-])(?P<after>\s+)(?P<content>.*?)$"
    ).unwrap();
    static ref CODE_BLOCK_START: Regex = Regex::new(r"^\s*(```|~~~)").unwrap();
    static ref CODE_BLOCK_END: Regex = Regex::new(r"^\s*(```|~~~)\s*$").unwrap();
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();
}

/// Rule MD004: Unordered list style
#[derive(Clone)]
pub struct MD004UnorderedListStyle {
    pub style: UnorderedListStyle,
    pub after_marker: usize,
}

impl MD004UnorderedListStyle {
    pub fn new(style: UnorderedListStyle) -> Self {
        Self {
            style,
            after_marker: 1,
        }
    }

    // Helper: extract marker, indentation, and blockquote prefix for a ListItem
    fn extract_marker_indent_bq(li: &ListItem, ctx: &LintContext) -> Option<(usize, char, String, usize)> {
        let pos = li.position.as_ref()?;
        let line_num = pos.start.line;
        if line_num == 0 {
            return None;
        }
        let mut line_start_offset = 0usize;
        let mut lines = ctx.content.lines();
        for _ in 1..line_num {
            line_start_offset += lines.next()?.len();
            line_start_offset += 1;
        }
        let line = lines.next()?;
        let mut indent = String::new();
        let mut bq_prefix = 0;
        let mut marker = None;
        let mut i = 0;
        let chars: Vec<_> = line.chars().collect();
        while i < chars.len() {
            if chars[i] == ' ' || chars[i] == '\t' {
                indent.push(chars[i]);
                i += 1;
            } else if chars[i] == '>' {
                bq_prefix += 1;
                i += 1;
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
            } else {
                break;
            }
        }
        if i < chars.len() && (chars[i] == '*' || chars[i] == '-' || chars[i] == '+') {
            marker = Some((line_start_offset + i, chars[i]));
        }
        marker.map(|(offset, marker)| (offset, marker, indent, bq_prefix))
    }

    // Helper: check if a line is inside a code block
    fn is_in_code_block(line_num: usize, ctx: &LintContext) -> bool {
        let content = ctx.content;
        let mut in_code_block = false;
        let mut in_front_matter = false;
        for (i, line) in content.lines().enumerate() {
            if FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = !in_front_matter;
                continue;
            }
            if in_front_matter && i + 1 == line_num {
                return true;
            }
            if line.starts_with("```") || line.starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            if i + 1 == line_num {
                return in_code_block || in_front_matter;
            }
        }
        false
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
        if !ctx.content.contains(|c: char| c == '*' || c == '-' || c == '+') {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let content = &ctx.content;
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_marker: Option<char> = None;

        for (i, line) in content.lines().enumerate() {
            // Handle front matter
            if FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = !in_front_matter;
                continue;
            }
            if in_front_matter {
                continue;
            }

            // Handle code blocks
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            // Skip ordered list items
            if Regex::new(r"^\s*\d+[.)]").unwrap().is_match(line) {
                continue;
            }

            // Check for unordered list items
            if let Some(caps) = UNORDERED_LIST_REGEX.captures(line) {
                let indent = caps.name("indent").map(|m| m.as_str().to_string()).unwrap_or_default();
                let marker = caps.name("marker").unwrap().as_str().chars().next().unwrap();
                let offset = content[..content.lines().take(i).map(|l| l.len() + 1).sum::<usize>()].len() + indent.len();

                match self.style {
                    UnorderedListStyle::Consistent => {
                        if let Some(first) = first_marker {
                            // Check if current marker matches the first marker found
                            if marker != first {
                                let (line, col) = ctx.offset_to_line_col(offset);
                                warnings.push(LintWarning {
                                    line,
                                    column: col,
                                    message: format!("marker '{}' does not match expected style '{}'", marker, first),
                                    severity: Severity::Warning,
                                    rule_name: Some(self.name()),
                                    fix: None,
                                });
                            }
                        } else {
                            // This is the first marker we've found - set the style
                            first_marker = Some(marker);
                        }
                    },
                    _ => {
                        // Handle specific style requirements (asterisk, dash, plus)
                        let target_marker = match self.style {
                            UnorderedListStyle::Asterisk => '*',
                            UnorderedListStyle::Dash => '-',
                            UnorderedListStyle::Plus => '+',
                            _ => unreachable!(),
                        };
                        if marker != target_marker {
                            let (line, col) = ctx.offset_to_line_col(offset);
                            warnings.push(LintWarning {
                                line,
                                column: col,
                                message: format!("marker '{}' does not match expected style '{}'", marker, target_marker),
                                severity: Severity::Warning,
                                rule_name: Some(self.name()),
                                fix: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let ast = &ctx.ast;
        let mut edits = vec![];
        fn walk_fix(node: &Node, rule: &MD004UnorderedListStyle, ctx: &LintContext, edits: &mut Vec<(usize, char)>) {
            if let Node::List(list) = node {
                if list.ordered { return; }
                let mut item_info = vec![];
                let front_matter_lines = {
                    let mut in_front_matter = false;
                    let mut skip_lines = std::collections::HashSet::new();
                    for (i, line) in ctx.content.lines().enumerate() {
                        if FRONT_MATTER_DELIM.is_match(line) {
                            in_front_matter = !in_front_matter;
                            skip_lines.insert(i + 1);
                            continue;
                        }
                        if in_front_matter {
                            skip_lines.insert(i + 1);
                        }
                    }
                    skip_lines
                };
                for item in &list.children {
                    if let Node::ListItem(li) = item {
                        if let Some(pos) = li.position.as_ref() {
                            let line_num = pos.start.line;
                            if front_matter_lines.contains(&line_num) {
                                continue;
                            }
                            if MD004UnorderedListStyle::is_in_code_block(line_num, ctx) {
                                continue;
                            }
                        }
                        if let Some((offset, marker, indent, bq_prefix)) = MD004UnorderedListStyle::extract_marker_indent_bq(li, ctx) {
                            item_info.push((offset, marker, indent, bq_prefix, li));
                        }
                    }
                }
                if item_info.is_empty() { return; }
                match rule.style {
                    UnorderedListStyle::Consistent => {
                        // Group into contiguous runs by indent and bq_prefix (not marker)
                        let mut run_start = 0;
                        while run_start < item_info.len() {
                            let (_, _, start_indent, start_bq, _) = &item_info[run_start];
                            let mut run_end = run_start + 1;
                            while run_end < item_info.len() {
                                let (_, _, indent, bq, _) = &item_info[run_end];
                                if indent != start_indent || bq != start_bq {
                                    break;
                                }
                                run_end += 1;
                            }
                            // Determine most prevalent marker in the run
                            let mut counts = std::collections::HashMap::new();
                            for i in run_start..run_end {
                                let (_, marker, _, _, _) = &item_info[i];
                                *counts.entry(marker).or_insert(0) += 1;
                            }
                            // Find the marker with the highest count; tie-breaker: first in run
                            let mut max_count = 0;
                            let mut target_marker = item_info[run_start].1;
                            let first_marker = item_info[run_start].1;
                            for (_, (_, marker, _, _, _)) in item_info[run_start..run_end].iter().enumerate() {
                                let count = *counts.get(marker).unwrap_or(&0);
                                if count > max_count {
                                    max_count = count;
                                    target_marker = *marker;
                                }
                            }
                            // If there is a tie, use the first marker in the run
                            let mut tie_count = 0;
                            for count in counts.values() {
                                if *count == max_count {
                                    tie_count += 1;
                                }
                            }
                            if tie_count > 1 {
                                target_marker = first_marker;
                            }
                            for i in run_start..run_end {
                                let (offset, marker, _, _, _) = &item_info[i];
                                if *marker != target_marker {
                                    edits.push((*offset, target_marker));
                                }
                            }
                            run_start = run_end;
                        }
                    }
                    // Explicit style: fix every unordered list item
                    UnorderedListStyle::Asterisk | UnorderedListStyle::Dash | UnorderedListStyle::Plus => {
                        let target_marker = match rule.style {
                            UnorderedListStyle::Asterisk => '*',
                            UnorderedListStyle::Dash => '-',
                            UnorderedListStyle::Plus => '+',
                            _ => unreachable!(),
                        };
                        for (offset, marker, _indent, _bq_prefix, _li) in &item_info {
                            if *marker != target_marker {
                                edits.push((*offset, target_marker));
                            }
                        }
                    }
                }
            }
            if let Some(children) = node.children() {
                for child in children {
                    walk_fix(child, rule, ctx, edits);
                }
            }
        }
        walk_fix(ast, self, ctx, &mut edits);
        let mut result = ctx.content.to_string();
        edits.sort_by(|a, b| b.0.cmp(&a.0));
        for (offset, marker) in edits {
            if offset < result.len() {
                result.replace_range(offset..offset + 1, &marker.to_string());
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
        ctx.content.is_empty() || !ctx.content.contains(|c: char| c == '*' || c == '-' || c == '+')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(match self.style {
                UnorderedListStyle::Asterisk => "asterisk".to_string(),
                UnorderedListStyle::Dash => "dash".to_string(),
                UnorderedListStyle::Plus => "plus".to_string(),
                UnorderedListStyle::Consistent => "consistent".to_string(),
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
            _ => UnorderedListStyle::Consistent,
        };
        Box::new(MD004UnorderedListStyle::new(style))
    }
}

impl DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        !doc_structure.list_items.is_empty()
    }
}
