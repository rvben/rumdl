/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::ElementCache;
use crate::utils::element_cache::ListMarkerType;
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

    #[allow(dead_code)]
    fn parse_list_item(line: &str) -> Option<(usize, char, usize)> {
        lazy_static! {
            static ref LIST_ITEM_RE: Regex = Regex::new(r"^(\s*)([-*+])\s+(.*)$").unwrap();
        }

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

    fn check(&self, content: &str) -> LintResult {
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let element_cache = ElementCache::new(content);

        // Track which lines have already been flagged due to a parent
        let mut flagged_lines = std::collections::HashSet::new();

        for list_item in element_cache.get_list_items() {
            if let ListMarkerType::Ordered = list_item.marker_type {
                continue;
            }

            // Skip check if the line is inside a code block
            if element_cache.is_in_code_block(list_item.line_number) {
                continue;
            }

            // Calculate expected indentation: level * indent spaces
            let expected_indent = list_item.nesting_level * self.indent;

            if list_item.indentation != expected_indent {
                let correct_indent = " ".repeat(expected_indent);
                let trimmed = content
                    .lines()
                    .nth(list_item.line_number - 1)
                    .map(|line| line.trim_start())
                    .unwrap_or("");
                let replacement = format!("{}{}", correct_indent, trimmed);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: list_item.line_number,
                    column: list_item.indentation + 1,
                    message: format!(
                        "Unordered list indentation should be {} spaces (level {}), found {}",
                        expected_indent, list_item.nesting_level, list_item.indentation
                    ),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(list_item.line_number, 1),
                        replacement,
                    }),
                });
                flagged_lines.insert(list_item.line_number);
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, _structure: &DocumentStructure) -> LintResult {
        // Simply call the normal check method since we aren't using the structure yet
        self.check(content)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        let element_cache = ElementCache::new(content);
        let mut flagged_lines = std::collections::HashSet::new();

        for (i, &line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if let Some(list_item) = element_cache.get_list_item(line_num) {
                if let ListMarkerType::Ordered = list_item.marker_type {
                    result.push(line.to_string());
                    continue;
                }
                let expected_indent = list_item.nesting_level * self.indent;
                if list_item.indentation != expected_indent {
                    let correct_indent = " ".repeat(expected_indent);
                    let trimmed = line.trim_start();
                    result.push(format!("{}{}", correct_indent, trimmed));
                    flagged_lines.insert(line_num);
                } else {
                    result.push(line.to_string());
                }
            } else {
                result.push(line.to_string());
            }
        }
        let result_str = result.join("\n");
        if content.ends_with('\n') && !result_str.ends_with('\n') {
            Ok(result_str + "\n")
        } else {
            Ok(result_str)
        }
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty()
            || (!content.contains('*') && !content.contains('-') && !content.contains('+'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("indent".to_string(), toml::Value::Integer(self.indent as i64));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let indent = crate::config::get_rule_config_value::<usize>(config, "MD007", "indent").unwrap_or(2);
        Box::new(MD007ULIndent::new(indent))
    }
}

impl DocumentStructureExtensions for MD007ULIndent {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Use the document structure to check if there are any unordered list elements
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        // Test with default indentation (2 spaces)
        let rule = MD007ULIndent::default();

        // Test with valid indentation
        let content = "* Item 1\n  * Nested item 1\n  * Nested item 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for correct indentation"
        );

        // Test with invalid indentation
        let content = "* Item 1\n * Nested item 1\n * Nested item 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2, "Expected warnings for 1-space indentation");

        // Test with custom indentation
        let rule = MD007ULIndent::new(4);
        let content = "* Item 1\n * Nested item 1";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Expected warning for 1-space indentation with 4-space rule"
        );
    }
}
