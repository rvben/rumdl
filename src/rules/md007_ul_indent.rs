use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

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
        // Fast path - if content doesn't contain list markers, no list items exist
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut list_levels: Vec<(usize, usize)> = Vec::new(); // (indent, nesting level)

        for (i, line) in lines.iter().enumerate() {
            if Self::is_in_code_block(content, i) {
                continue;
            }

            if let Some((indent, _marker, _content_len)) = Self::parse_list_item(line) {
                // Determine the nesting level of this item
                let nesting_level = if indent == 0 {
                    // Top level item
                    0
                } else {
                    // Find the appropriate nesting level based on previous items
                    let mut level = 0;
                    for &(prev_indent, prev_level) in list_levels.iter().rev() {
                        if indent > prev_indent {
                            level = prev_level + 1;
                            break;
                        } else if indent == prev_indent {
                            level = prev_level;
                            break;
                        } else if indent < prev_indent {
                            // Continue searching for a matching level
                            continue;
                        }
                    }
                    level
                };

                // Update list level tracking
                list_levels.push((indent, nesting_level));

                // Calculate expected indentation: level * indent spaces
                let expected_indent = nesting_level * self.indent;

                // If indentation doesn't match expected value
                if indent != expected_indent {
                    // Get the correct indentation
                    let correct_indent = " ".repeat(expected_indent);
                    let trimmed = line.trim_start();
                    let replacement = format!("{}{}", correct_indent, trimmed);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: indent + 1,
                        message: format!(
                            "Unordered list indentation should be {} spaces (level {}), found {}",
                            expected_indent, nesting_level, indent
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement,
                        }),
                    });
                }
            } else {
                // Not a list item - clear levels if line is not blank
                // This ensures that separate lists are treated independently
                if !line.trim().is_empty() {
                    list_levels.clear();
                }
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if the document has no lists
        if structure.list_lines.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut list_levels: Vec<(usize, usize)> = Vec::new(); // (indent, nesting level)

        // Process only lines with list items, using the pre-computed list_lines
        for &line_num in &structure.list_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Skip lines in code blocks
            if structure.is_in_code_block(line_num) {
                continue;
            }

            if let Some((indent, _marker, _content_len)) = Self::parse_list_item(line) {
                // Determine the nesting level of this item
                let nesting_level = if indent == 0 {
                    // Top level item
                    0
                } else {
                    // Find the appropriate nesting level based on previous items
                    let mut level = 0;
                    for &(prev_indent, prev_level) in list_levels.iter().rev() {
                        if indent > prev_indent {
                            level = prev_level + 1;
                            break;
                        } else if indent == prev_indent {
                            level = prev_level;
                            break;
                        } else if indent < prev_indent {
                            // Continue searching for a matching level
                            continue;
                        }
                    }
                    level
                };

                // Update list level tracking
                list_levels.push((indent, nesting_level));

                // Calculate expected indentation: level * indent spaces
                let expected_indent = nesting_level * self.indent;

                // If indentation doesn't match expected value
                if indent != expected_indent {
                    // Get the correct indentation
                    let correct_indent = " ".repeat(expected_indent);
                    let trimmed = line.trim_start();
                    let replacement = format!("{}{}", correct_indent, trimmed);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num,
                        column: indent + 1,
                        message: format!(
                            "Unordered list indentation should be {} spaces (level {}), found {}",
                            expected_indent, nesting_level, indent
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement,
                        }),
                    });
                }
            } else {
                // Check if this line is not part of a list and not empty
                // If so, we clear our list levels to ensure separate lists are treated independently
                if !line.trim().is_empty() && !structure.list_lines.contains(&line_num) {
                    list_levels.clear();
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let warnings = self.check(content)?;

        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        // Create a map of line numbers with fixes
        let mut line_fixes: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                line_fixes.insert(warning.line, fix.replacement.clone());
            }
        }

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if let Some(fixed_line) = line_fixes.get(&line_num) {
                result.push_str(fixed_line);
            } else {
                result.push_str(line);
            }

            // Add newline for all lines except the last one, unless the original content ends with newline
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if present
        if content.ends_with('\n') && !result.ends_with('\n') {
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

impl DocumentStructureExtensions for MD007ULIndent {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are list items
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
