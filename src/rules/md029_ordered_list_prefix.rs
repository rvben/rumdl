/// Rule MD029: Ordered list item prefix
///
/// See [docs/md029.md](../../docs/md029.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

lazy_static! {
    static ref ORDERED_LIST_ITEM_REGEX: Regex = Regex::new(r"^(\s*)\d+\.\s").unwrap();
    static ref LIST_NUMBER_REGEX: Regex = Regex::new(r"^\s*(\d+)\.\s").unwrap();
    static ref FIX_LINE_REGEX: Regex = Regex::new(r"^(\s*)\d+(\.\s.*)$").unwrap();
}

/// Represents the style for ordered lists
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ListStyle {
    One,        // Use '1.' for all items
    OneOne,     // All ones (1. 1. 1.)
    Ordered,    // Sequential (1. 2. 3.)
    Ordered0,   // Zero-based (0. 1. 2.)
}

#[derive(Debug, Clone)]
pub struct MD029OrderedListPrefix {
    pub style: ListStyle,
}

impl Default for MD029OrderedListPrefix {
    fn default() -> Self {
        Self {
            style: ListStyle::Ordered,
        }
    }
}

impl MD029OrderedListPrefix {
    pub fn new(style: ListStyle) -> Self {
        Self { style }
    }

    fn get_list_number(line: &str) -> Option<usize> {
        LIST_NUMBER_REGEX
            .captures(line)
            .and_then(|cap| cap[1].parse::<usize>().ok())
    }

    fn get_expected_number(&self, index: usize) -> usize {
        match self.style {
            ListStyle::One => 1,
            ListStyle::OneOne => 1,
            ListStyle::Ordered => index + 1,
            ListStyle::Ordered0 => index,
        }
    }

    fn fix_line(&self, line: &str, expected_num: usize) -> String {
        FIX_LINE_REGEX
            .replace(line, format!("${{1}}{}{}", expected_num, "$2"))
            .to_string()
    }
}

impl Rule for MD029OrderedListPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list marker value"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;
        let mut indent_stack: Vec<(usize, usize)> = Vec::new(); // (indent, index)
        let lines: Vec<&str> = content.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }
            if Self::get_list_number(line).is_some() {
                let indent = line.chars().take_while(|c| c.is_whitespace()).count();
                // Pop stack if current indent is less than stack top
                while let Some(&(top_indent, _)) = indent_stack.last() {
                    if indent < top_indent {
                        indent_stack.pop();
                    } else {
                        break;
                    }
                }
                // If indent matches stack top, increment index
                if let Some(&mut (top_indent, ref mut idx)) = indent_stack.last_mut() {
                    if indent == top_indent {
                        let expected_num = self.get_expected_number(*idx);
                        if Self::get_list_number(line).unwrap() != expected_num {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                message: format!(
                                    "Ordered list item number {} does not match style (expected {})",
                                    Self::get_list_number(line).unwrap(), expected_num
                                ),
                                line: line_num + 1,
                                column: line.find(char::is_numeric).unwrap_or(0) + 1,
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: 0..0, // TODO: Replace with correct byte range if available
                                    replacement: self.fix_line(line, expected_num),
                                }),
                            });
                        }
                        *idx += 1;
                        continue;
                    }
                }
                // New deeper indent or first item
                indent_stack.push((indent, 0));
                let expected_num = self.get_expected_number(0);
                if Self::get_list_number(line).unwrap() != expected_num {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Ordered list item number {} does not match style (expected {})",
                            Self::get_list_number(line).unwrap(),
                            expected_num
                        ),
                        line: line_num + 1,
                        column: line.find(char::is_numeric).unwrap_or(0) + 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: 0..0, // TODO: Replace with correct byte range if available
                            replacement: self.fix_line(line, expected_num),
                        }),
                    });
                }
                // Increment the new top
                if let Some(&mut (_, ref mut idx)) = indent_stack.last_mut() {
                    *idx += 1;
                }
            } else if !line.trim().is_empty() {
                // Non-list, non-blank line breaks the list
                indent_stack.clear();
            } else {
                // Blank line breaks the list
                indent_stack.clear();
            }
        }
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut in_code_block = false;
        let mut indent_stack: Vec<(usize, usize)> = Vec::new(); // (indent, index)
        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if Self::get_list_number(line).is_some() {
                let indent = line.chars().take_while(|c| c.is_whitespace()).count();
                // Pop stack if current indent is less than stack top
                while let Some(&(top_indent, _)) = indent_stack.last() {
                    if indent < top_indent {
                        indent_stack.pop();
                    } else {
                        break;
                    }
                }
                // If indent matches stack top, increment index
                if let Some(&mut (top_indent, ref mut idx)) = indent_stack.last_mut() {
                    if indent == top_indent {
                        let expected_num = self.get_expected_number(*idx);
                        let fixed_line = self.fix_line(line, expected_num);
                        result.push_str(&fixed_line);
                        result.push('\n');
                        *idx += 1;
                        continue;
                    }
                }
                // New deeper indent or first item
                indent_stack.push((indent, 0));
                let expected_num = self.get_expected_number(0);
                let fixed_line = self.fix_line(line, expected_num);
                result.push_str(&fixed_line);
                result.push('\n');
                // Increment the new top
                if let Some(&mut (_, ref mut idx)) = indent_stack.last_mut() {
                    *idx += 1;
                }
            } else if !line.trim().is_empty() {
                // Non-list, non-blank line breaks the list
                indent_stack.clear();
                result.push_str(line);
                result.push('\n');
            } else {
                // Blank line breaks the list
                indent_stack.clear();
                result.push_str(line);
                result.push('\n');
            }
        }
        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }
        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no lists
        if structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check if there are no ordered lists
        if !content.contains('1')
            || (!content.contains("1.") && !content.contains("2.") && !content.contains("0."))
        {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut list_items = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Create a set of list line indices for faster lookup
        let mut list_line_set = std::collections::HashSet::new();
        for &line_num in &structure.list_lines {
            list_line_set.insert(line_num); // Keep as 1-indexed for easier comparison
        }

        // Group ordered list items into sections
        let mut in_list = false;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed

            // Skip lines in code blocks
            if structure.is_in_code_block(line_num) {
                // If we were in a list, check it before continuing
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                    in_list = false;
                }
                continue;
            }

            if list_line_set.contains(&line_num) {
                if Self::get_list_number(line).is_some() {
                    // If this is the first item of a new list, record the list start
                    if !in_list {
                        in_list = true;
                    }

                    list_items.push((line_idx, line.to_string()));
                }
            } else if !line.trim().is_empty() {
                // Non-empty, non-list line breaks the list
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                    in_list = false;
                }
            }
        }

        // Check last section if it exists
        if !list_items.is_empty() {
            self.check_list_section(&list_items, &mut warnings);
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty()
            || !content.contains('1')
            || (!content.contains("1.") && !content.contains("2.") && !content.contains("0."))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style_str = crate::config::get_rule_config_value::<String>(config, "MD029", "style").unwrap_or_else(|| "ordered".to_string());
        let style = match style_str.as_str() {
            "one" => ListStyle::One,
            "one_one" => ListStyle::OneOne,
            "ordered0" => ListStyle::Ordered0,
            _ => ListStyle::Ordered,
        };
        Box::new(MD029OrderedListPrefix::new(style))
    }
}

impl DocumentStructureExtensions for MD029OrderedListPrefix {
    fn has_relevant_elements(&self, content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are list items AND they might be ordered lists
        !doc_structure.list_lines.is_empty()
            && (content.contains("1.") || content.contains("2.") || content.contains("0."))
    }
}

impl MD029OrderedListPrefix {
    fn check_list_section(&self, items: &[(usize, String)], warnings: &mut Vec<LintWarning>) {
        // Improved grouping: start a new group when indentation decreases or stays the same after a break
        let mut groups: Vec<Vec<(usize, String)>> = Vec::new();
        let mut current_group: Vec<(usize, String)> = Vec::new();
        let mut last_indent: Option<usize> = None;
        for (line_num, _line) in items.iter() {
            let indent = _line.chars().take_while(|c| c.is_whitespace()).count();
            if current_group.is_empty() {
                current_group.push((*line_num, _line.clone()));
                last_indent = Some(indent);
            } else if indent > last_indent.unwrap() {
                // Nested list: start a new group
                groups.push(std::mem::take(&mut current_group));
                current_group.push((*line_num, _line.clone()));
                last_indent = Some(indent);
            } else if indent < last_indent.unwrap() {
                // Outdent: start a new group
                groups.push(std::mem::take(&mut current_group));
                current_group.push((*line_num, _line.clone()));
                last_indent = Some(indent);
            } else {
                // Same level: continue current group
                current_group.push((*line_num, _line.clone()));
                last_indent = Some(indent);
            }
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }
        // Check each group independently
        for group in groups {
            for (idx, (line_num, line)) in group.iter().enumerate() {
                if Self::get_list_number(line).is_some() {
                    let expected_num = self.get_expected_number(idx);
                    if Self::get_list_number(line).unwrap() != expected_num {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!(
                                "Ordered list item number {} does not match style (expected {})",
                                Self::get_list_number(line).unwrap(),
                                expected_num
                            ),
                            line: line_num + 1,
                            column: line.find(char::is_numeric).unwrap_or(0) + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: 0..0, // TODO: Replace with correct byte range if available
                                replacement: self.fix_line(line, expected_num),
                            }),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        // Test with default style (ordered)
        let rule = MD029OrderedListPrefix::default();

        // Test with correctly ordered list
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with incorrectly ordered list
        let content = "1. First item\n3. Third item\n5. Fifth item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 5

        // Test with one-one style
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 2 and 3

        // Test with ordered0 style
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. First item\n1. Second item\n2. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());
    }
}
