use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{
    DocumentStructure, DocumentStructureExtensions, ListMarkerType,
};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_BLOCK_MARKER: Regex = Regex::new(r"^(```|~~~)").unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Default)]
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

    fn get_marker_char(style: UnorderedListStyle) -> char {
        match style {
            UnorderedListStyle::Asterisk => '*',
            UnorderedListStyle::Plus => '+',
            UnorderedListStyle::Dash => '-',
            UnorderedListStyle::Consistent => '*', // Default, but will be overridden
        }
    }
}

impl Rule for MD004UnorderedListStyle {
    fn name(&self) -> &'static str {
        "MD004"
    }

    fn description(&self) -> &'static str {
        "Use consistent style for unordered list markers"
    }

    fn check(&self, content: &str) -> LintResult {
        let structure = DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if the document has no lists
        if structure.list_lines.is_empty() {
            return Ok(vec![]);
        }

        // Get only unordered list items from the structure
        let unordered_items = structure.get_list_items_by_type(ListMarkerType::Unordered);
        if unordered_items.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Track the first marker style for the "consistent" option
        let first_marker = match self.style {
            UnorderedListStyle::Consistent => {
                // Get marker from first unordered list item
                if let Some(first_item) = unordered_items.first() {
                    first_item.marker.chars().next()
                } else {
                    // Default to asterisk if no items found
                    Some('*')
                }
            }
            specific_style => Some(Self::get_marker_char(specific_style)),
        };

        // Process all unordered list items
        for item in unordered_items {
            if let Some(target_style) = first_marker {
                let item_marker = item.marker.chars().next().unwrap_or('*');

                if item_marker != target_style {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: item.line_number,
                        column: item.indentation + 1,
                        severity: Severity::Warning,
                        message: format!(
                            "Unordered list item marker '{}' does not match style '{}'",
                            item_marker, target_style
                        ),
                        fix: Some(Fix {
                            range: line_index
                                .line_col_to_byte_range(item.line_number, item.indentation + 1),
                            replacement: format!(
                                "{}{} ",
                                " ".repeat(item.indentation),
                                target_style
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');
        let structure = DocumentStructure::new(content);
        let mut fixed_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

        // Get only unordered list items from the structure
        let unordered_items = structure.get_list_items_by_type(ListMarkerType::Unordered);
        if unordered_items.is_empty() {
            let mut result = fixed_lines.join("\n");
            if ends_with_newline {
                result.push('\n');
            }
            return Ok(result);
        }

        // Determine the target marker style
        let target_marker = match self.style {
            UnorderedListStyle::Consistent => {
                if let Some(first_item) = unordered_items.first() {
                    first_item.marker.chars().next().unwrap_or('*')
                } else {
                    '*'
                }
            }
            specific_style => Self::get_marker_char(specific_style),
        };

        // Fix all unordered list items to use the target marker
        for item in unordered_items {
            let line_idx = item.line_number - 1; // 0-indexed
            let line = lines.get(line_idx).unwrap_or(&"");
            let indentation = item.indentation;
            let rest = line.trim_start().splitn(2, char::is_whitespace).nth(1).unwrap_or("");
            fixed_lines[line_idx] = format!("{}{} {}", " ".repeat(indentation), target_marker, rest.trim_start());
        }

        let mut result = fixed_lines.join("\n");
        if ends_with_newline {
            result.push('\n');
        }
        Ok(result)
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
}

impl DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Rule is only relevant if there are list items
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        // Test with consistent style
        let rule = MD004UnorderedListStyle::default();

        // Test with consistent markers
        let content = "* Item 1\n* Item 2\n* Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with inconsistent markers
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the - and + markers

        // Test specific style
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the * and + markers
    }
}
