/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::{ElementCache, ListMarkerType};
use crate::utils::range_utils::LineIndex;
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

    /// Detect code blocks, including those inside blockquotes
    fn compute_code_block_lines(content: &str) -> std::collections::HashSet<usize> {
        let mut code_block_lines = std::collections::HashSet::new();
        let mut in_code_block = false;
        let mut fence: Option<String> = None;
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if !in_code_block {
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    in_code_block = true;
                    fence = Some(trimmed[..3].to_string());
                    code_block_lines.insert(i + 1);
                    continue;
                }
            } else {
                code_block_lines.insert(i + 1);
                if let Some(ref f) = fence {
                    if trimmed.starts_with(f) {
                        in_code_block = false;
                        fence = None;
                    }
                }
                continue;
            }
        }
        code_block_lines
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
                println!(
                    "MD007 DEBUG: line {} | indent={} | nesting={} | expected={} | emit_warning={}",
                    item.line_number,
                    item.indentation,
                    item.nesting_level,
                    expected_indent,
                    item.indentation != expected_indent
                );
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
        let element_cache = ElementCache::new(content);
        let mut lines: Vec<&str> = content.lines().collect();
        for item in element_cache.get_list_items() {
            if element_cache.is_in_code_block(item.line_number) {
                continue;
            }
            if matches!(item.marker_type, ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus) {
                let expected_indent = item.nesting_level * self.indent;
                if item.indentation != expected_indent {
                    // Reconstruct the line: blockquote_prefix + correct_indent + marker + spaces_after_marker + content
                    let correct_indent = " ".repeat(expected_indent);
                    let marker = &item.marker;
                    let after_marker = " ".repeat(item.spaces_after_marker.max(1));
                    let new_line = format!(
                        "{}{}{}{}{}",
                        item.blockquote_prefix,
                        correct_indent,
                        marker,
                        after_marker,
                        item.content
                    );
                    lines[item.line_number - 1] = Box::leak(new_line.into_boxed_str());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    // Remove test_with_document_structure, as the rule no longer uses DocumentStructure for its logic
}
