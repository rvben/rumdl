use regex::Regex;
use lazy_static::lazy_static;
use crate::rule::{Rule, LintWarning, LintResult, LintError, Fix};

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
        "MD006StartBullets"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
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
                result.push(LintWarning {
                    line: line_idx + 1, // 1-indexed line number
                    column: 1,
                    message: "Consider starting bulleted lists at the beginning of the line".to_string(),
                    fix: Some(Fix {
                        line: line_idx + 1,
                        column: 1,
                        replacement: fixed_line.to_string(),
                    }),
                });
            }
        }
        
        Ok(result)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let warnings = self.check(content)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }
        
        let mut fixed_content = String::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Create a map of fixes by line number
        let mut fix_map = std::collections::HashMap::new();
        for warning in warnings {
            if let Some(fix) = warning.fix {
                fix_map.insert(fix.line, fix.replacement);
            }
        }
        
        // Apply fixes line by line
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            
            if let Some(replacement) = fix_map.get(&line_num) {
                fixed_content.push_str(replacement);
            } else {
                fixed_content.push_str(line);
            }
            
            // Add newline unless it's the last line and the original doesn't end with newline
            if i < lines.len() - 1 || content.ends_with('\n') {
                fixed_content.push('\n');
            }
        }
        
        Ok(fixed_content)
    }
}




