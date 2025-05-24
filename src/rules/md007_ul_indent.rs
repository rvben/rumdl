/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::{ElementCache, ListMarkerType};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

#[derive(Debug, Clone)]
pub struct MD007ULIndent {
    pub indent: usize,
}

impl Default for MD007ULIndent {
    fn default() -> Self {
        Self { indent: 2 }
    }
}

impl MD007ULIndent {
    pub fn new(indent: usize) -> Self {
        Self { indent }
    }

    #[allow(dead_code)]
    fn is_in_code_block(content: &str, line_idx: usize) -> bool {
        lazy_static! {
            static ref CODE_BLOCK_MARKER: Regex = Regex::new(r"^(```|~~~)").unwrap();
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            if i > line_idx {
                break;
            }

            if CODE_BLOCK_MARKER.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
            }

            if i == line_idx {
                return in_code_block;
            }
        }

        false
    }
}

impl Rule for MD007ULIndent {
    fn name(&self) -> &'static str {
        "MD007"
    }

    fn description(&self) -> &'static str {
        "Unordered list indentation"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();
        for item in element_cache.get_list_items() {
            // Only unordered list items
            // Skip list items inside code blocks (including YAML/front matter)
            if element_cache.is_in_code_block(item.line_number) {
                continue;
            }
            if matches!(item.marker_type, ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus) {
                let expected_indent = item.nesting_level * self.indent;
                if item.indentation != expected_indent {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Incorrect indentation: expected {} spaces for nesting level {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + item.indentation + 1, // correct column for marker
                        severity: Severity::Warning,
                        fix: None,
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
        _doc_structure: &DocumentStructure,
    ) -> LintResult {
        self.check(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let tab_str = " ".repeat(self.indent);
        let mut lines: Vec<String> = content
            .lines()
            .map(|l| {
                // Normalize leading tabs to spaces
                let mut norm = String::new();
                let mut chars = l.chars().peekable();
                while let Some(&c) = chars.peek() {
                    if c == '\t' {
                        norm.push_str(&tab_str);
                        chars.next();
                    } else if c == ' ' {
                        norm.push(' ');
                        chars.next();
                    } else {
                        break;
                    }
                }
                norm.extend(chars);
                norm
            })
            .collect();

        // Recompute logical nesting for each unordered list item
        let mut prev_items: Vec<(usize, usize, usize)> = Vec::new(); // (blockquote_depth, indent, nesting_level)
        for (_, line) in lines.iter_mut().enumerate() {
            let orig_line = line.clone();
            // Inline blockquote prefix parsing (since parse_blockquote_prefix is private)
            let mut rest = orig_line.as_str();
            let mut blockquote_prefix = String::new();
            let mut blockquote_depth = 0;
            loop {
                let trimmed = rest.trim_start();
                if trimmed.starts_with('>') {
                    // Find the '>' and a single optional space
                    let after = &trimmed[1..];
                    let mut chars = after.chars();
                    let mut space_count = 0;
                    if let Some(' ') = chars.next() {
                        space_count = 1;
                    }
                    let (spaces, after_marker) = after.split_at(space_count);
                    blockquote_prefix.push('>');
                    blockquote_prefix.push_str(spaces);
                    rest = after_marker;
                    blockquote_depth += 1;
                } else {
                    break;
                }
            }
            // Only process unordered list items outside code blocks
            if rest.trim().is_empty() || rest.starts_with("```") || rest.starts_with("~~~") {
                // Do NOT clear prev_items on blank lines; only skip processing
                continue;
            }
            // Use the same regex as element_cache
            let re = regex::Regex::new(r"^(?P<indent>[ ]*)(?P<marker>[*+-])(?P<after>[ ]+)(?P<content>.*)$").unwrap();
            if let Some(caps) = re.captures(rest) {
                let indent_str = caps.name("indent").map_or("", |m| m.as_str());
                let marker = caps.name("marker").unwrap().as_str();
                let after = caps.name("after").map_or(" ", |m| m.as_str());
                let content = caps.name("content").map_or("", |m| m.as_str());
                let indent = indent_str.len();
                // Compute logical nesting level
                let mut nesting_level = 0;
                if let Some(&(_last_bq, last_indent, last_level)) = prev_items.iter().rev().find(|(bq, _, _)| *bq == blockquote_depth) {
                    if indent > last_indent {
                        nesting_level = last_level + 1;
                    } else {
                        for &(prev_bq, prev_indent, prev_level) in prev_items.iter().rev() {
                            if prev_bq == blockquote_depth && prev_indent <= indent {
                                nesting_level = prev_level;
                                break;
                            }
                        }
                    }
                }
                // Remove stack entries with indent >= current indent and same blockquote depth
                while let Some(&(prev_bq, prev_indent, _)) = prev_items.last() {
                    if prev_bq != blockquote_depth || prev_indent < indent {
                        break;
                    }
                    prev_items.pop();
                }
                prev_items.push((blockquote_depth, indent, nesting_level));
                // Reconstruct line with correct indentation
                let correct_indent = " ".repeat(nesting_level * self.indent);
                *line = format!("{}{}{}{}{}", blockquote_prefix, correct_indent, marker, after, content);
            } else {
                // Only clear prev_items if the line is not blank and not a list item
                if !rest.trim().is_empty() {
                    prev_items.clear();
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
                && !ctx.content.contains('+'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "indent".to_string(),
            toml::Value::Integer(self.indent as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let indent =
            crate::config::get_rule_config_value::<usize>(config, "MD007", "indent").unwrap_or(2);
        Box::new(MD007ULIndent::new(indent))
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
