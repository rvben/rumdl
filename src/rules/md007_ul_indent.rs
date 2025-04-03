use crate::utils::range_utils::LineIndex;
use crate::utils::document_structure::DocumentStructure;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug)]
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

    fn is_list_item(line: &str) -> Option<(usize, char)> {
        let indentation = line.len() - line.trim_start().len();

        let trimmed = line.trim_start();

        if let Some(c) = trimmed.chars().next() {
            if (c == '*' || c == '-' || c == '+')
                && (trimmed.len() == 1
                    || trimmed.chars().nth(1).map_or(false, |c| c.is_whitespace()))
            {
                return Some((indentation, c));
            }
        }
        None
    }

    fn is_list_continuation(line: &str) -> bool {
        let indent = line.len() - line.trim_start().len();
        indent > 0 && !line.trim().is_empty() && Self::is_list_item(line).is_none()
    }

    fn get_expected_indent(&self, level: usize) -> usize {
        level * self.indent
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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut current_level = 0;

        let mut level_indents = vec![0];

        let mut in_list = false;

        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            if let Some((indent, marker)) = Self::is_list_item(line) {
                if indent > level_indents[current_level] {
                    // Going deeper
                    current_level += 1;
                    let expected_indent = self.get_expected_indent(current_level);
                    if current_level >= level_indents.len() {
                        level_indents.push(expected_indent);
                    }
                    if indent != expected_indent {
                        warnings.push(LintWarning {
            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indent + 1,
                            message: format!(
                                "List item with marker '{}' should be indented {} spaces (found {})",
                                marker, expected_indent, indent
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement: format!("{}{}", " ".repeat(expected_indent), line.trim_start()),
                            }),
                        });
                    }
                } else {
                    // Same level or going back
                    while current_level > 0 && indent <= level_indents[current_level - 1] {
                        current_level -= 1;
                    }
                    let expected_indent = self.get_expected_indent(current_level);
                    if indent != expected_indent {
                        warnings.push(LintWarning {
            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indent + 1,
                            message: format!(
                                "List item with marker '{}' should be indented {} spaces (found {})",
                                marker, expected_indent, indent
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement: format!("{}{}", " ".repeat(expected_indent), line.trim_start()),
                            }),
                        });
                    }
                }
                in_list = true;
            } else if Self::is_list_continuation(line) {
                if in_list {
                    let indent = line.len() - line.trim_start().len();
                    let expected_indent = self.get_expected_indent(current_level);
                    if indent != expected_indent {
                        warnings.push(LintWarning {
            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indent + 1,
                            message: format!(
                                "List continuation should be indented {} spaces (found {})",
                                expected_indent, indent
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                                replacement: format!(
                                    "{}{}",
                                    " ".repeat(expected_indent),
                                    line.trim_start()
                                ),
                            }),
                        });
                    }
                }
            } else {
                in_list = false;
                current_level = 0;
                level_indents.truncate(1);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let mut current_level = 0;

        let mut level_indents = vec![0];

        let mut in_list = false;

        for line in content.lines() {
            if line.trim().is_empty() {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if let Some((indent, _)) = Self::is_list_item(line) {
                if indent > level_indents[current_level] {
                    // Going deeper
                    current_level += 1;
                    let expected_indent = self.get_expected_indent(current_level);
                    if current_level >= level_indents.len() {
                        level_indents.push(expected_indent);
                    }
                    result.push_str(&format!(
                        "{}{}\n",
                        " ".repeat(expected_indent),
                        line.trim_start()
                    ));
                } else {
                    // Same level or going back
                    while current_level > 0 && indent <= level_indents[current_level - 1] {
                        current_level -= 1;
                    }
                    let expected_indent = self.get_expected_indent(current_level);
                    result.push_str(&format!(
                        "{}{}\n",
                        " ".repeat(expected_indent),
                        line.trim_start()
                    ));
                }
                in_list = true;
            } else if Self::is_list_continuation(line) && in_list {
                let expected_indent = self.get_expected_indent(current_level);
                result.push_str(&format!(
                    "{}{}\n",
                    " ".repeat(expected_indent),
                    line.trim_start()
                ));
            } else {
                in_list = false;
                current_level = 0;
                level_indents.truncate(1);
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_with_document_structure() {
        println!("=====================");
        println!("RUNNING MD007 TEST WITH DOCUMENT STRUCTURE");
        println!("=====================");
        
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        
        // Debug output
        println!("Number of warnings: {}", result.len());
        for (i, warning) in result.iter().enumerate() {
            println!("Warning {}: line={}, column={}, message={}", 
                     i, warning.line, warning.column, warning.message);
        }
        
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].column, 4);
    }
}
