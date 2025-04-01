use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

/// Rule that ensures bullet lists start at the beginning of the line
///
/// In standard Markdown:
/// - Top-level bullet items should start at column 0 (no indentation)
/// - Nested bullet items should be indented under their parent
/// - A bullet item following non-list content should start a new list at column 0
#[derive(Default)]
pub struct MD006StartBullets;

lazy_static! {
    // Pattern to match bullet list items: captures indentation, marker, and space after marker
    static ref BULLET_PATTERN: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();

    // Pattern to match code fence markers
    static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(\s*)(```|~~~)").unwrap();
}

impl MD006StartBullets {
    /// Checks if a line is a bullet list item and returns its indentation level
    fn is_bullet_list_item(line: &str) -> Option<usize> {
        if let Some(captures) = BULLET_PATTERN.captures(line) {
            if let Some(indent) = captures.get(1) {
                return Some(indent.as_str().len());
            }
        }
        None
    }

    /// Checks if a line is blank (empty or whitespace only)
    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// According to Markdown standards, determines if a bullet item is properly nested
    /// A properly nested item:
    /// 1. Has indentation greater than its parent
    /// 2. Follows (directly or after blank lines) a parent bullet item with less indentation
    fn is_properly_nested(&self, lines: &[&str], line_idx: usize) -> bool {
        // Get current item's indentation

        let current_indent = match Self::is_bullet_list_item(lines[line_idx]) {
            Some(indent) => indent,
            None => return false, // Not a bullet item
        };

        // If not indented, it's automatically a top-level item (not nested)
        if current_indent == 0 {
            return false;
        }

        // Look backwards to find a parent item or non-list content

        let mut i = line_idx;
        while i > 0 {
            i -= 1;

            // Skip blank lines (empty lines don't break nesting)
            if Self::is_blank_line(lines[i]) {
                continue;
            }

            // Found a list item, check its indentation
            if let Some(prev_indent) = Self::is_bullet_list_item(lines[i]) {
                // If previous item has less indentation, it's a parent of this item
                // In standard Markdown, any item with greater indentation than a previous item
                // is considered properly nested
                if prev_indent < current_indent {
                    return true;
                }

                // If same indentation, items are siblings; keep looking for parent
                if prev_indent == current_indent {
                    continue;
                }

                // In rare edge cases where previous item has more indentation than current
                // (usually indicates a formatting issue), continue looking for parent
                continue;
            }

            // If we hit non-list content, this is a new list that should start at col 0
            return false;
        }

        // If we reach the start of the document without finding a parent, this item
        // should not be indented (should be a top-level item)
        false
    }
}

impl Rule for MD006StartBullets {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());
        let mut result = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Track if we're in a code block

        let mut in_code_block = false;

        for (line_idx, line) in lines.iter().enumerate() {
            // Toggle code block state
            if CODE_FENCE_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            // Check if this line is a bullet list item
            if let Some(indent) = Self::is_bullet_list_item(line) {
                // Skip items with no indentation (already at the beginning of the line)
                if indent == 0 {
                    continue;
                }

                // Skip properly nested items according to Markdown standards
                // A nested item should have a parent item with less indentation
                if self.is_properly_nested(&lines, line_idx) {
                    continue;
                }

                // If we get here, we have an improperly indented bullet item:
                // Either it's indented but has no parent, or it follows non-list content
                let fixed_line = line.trim_start();

                // Check if this should have a blank line before it
                let needs_blank_line = line_idx > 0
                    && !Self::is_blank_line(lines[line_idx - 1])
                    && Self::is_bullet_list_item(lines[line_idx - 1]).is_none();

                let replacement = if needs_blank_line {
                    format!("\n{}", fixed_line)
                } else {
                    fixed_line.to_string()
                };

                result.push(LintWarning {
                    severity: Severity::Warning,
                    line: line_idx + 1, // 1-indexed line number
                    column: 1,
                    message: "Consider starting bulleted lists at the beginning of the line"
                        .to_string(),
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_idx + 1, 1),
                        replacement,
                    }),
                });
            }
        }

        Ok(result)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let warnings = self.check(content)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut fixed_lines: Vec<String> = Vec::with_capacity(lines.len());

        // Create a map of line numbers to replacements

        let mut line_replacements = std::collections::HashMap::new();
        for warning in warnings {
            if let Some(fix) = warning.fix {
                // Line number is 1-based in warnings but we need 0-based for indexing
                let line_idx = warning.line - 1;
                line_replacements.insert(line_idx, fix.replacement);
            }
        }

        // Apply replacements line by line

        let mut i = 0;
        while i < lines.len() {
            if let Some(replacement) = line_replacements.get(&i) {
                // Check if this replacement includes a blank line
                if let Some(stripped) = replacement.strip_prefix('\n') {
                    // Add a blank line
                    fixed_lines.push(String::new());
                    // Then add the actual content (without the leading newline)
                    fixed_lines.push(stripped.to_string());
                } else {
                    fixed_lines.push(replacement.clone());
                }
            } else {
                fixed_lines.push(lines[i].to_string());
            }

            i += 1;
        }

        // Join the lines with newlines

        let result = fixed_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }
}
