use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref LIST_MARKER: Regex = Regex::new(r"^[\s]*[-+*][\s]+|^[\s]*\d+\.[\s]+").unwrap();
}

/// Rule MD046: Code block style
///
/// This rule is triggered when code blocks do not use a consistent style (either fenced or indented).
pub struct MD046CodeBlockStyle {
    style: CodeBlockStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodeBlockStyle {
    Consistent,
    Fenced,
    Indented,
}

impl MD046CodeBlockStyle {
    pub fn new(style: CodeBlockStyle) -> Self {
        Self { style }
    }

    fn is_fenced_code_block_start(&self, line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }

    fn is_list_item(&self, line: &str) -> bool {
        LIST_MARKER.is_match(line)
    }

    fn is_indented_code_block(&self, lines: &[&str], current_line: usize) -> bool {
        let line = lines[current_line];
        
        // Must start with exactly 4 spaces (not tabs)
        if !line.starts_with("    ") || line.starts_with("\t") {
            return false;
        }

        // Not a fenced code block
        if self.is_fenced_code_block_start(line) {
            return false;
        }

        // Check if we're in a list context
        let mut list_level = 0;
        let mut i = current_line;
        while i > 0 {
            i -= 1;
            let prev_line = lines[i].trim_start();
            
            // Empty lines don't affect list context
            if prev_line.is_empty() {
                continue;
            }
            
            // Found a list marker
            if self.is_list_item(prev_line) {
                list_level = lines[i].chars().take_while(|c| c.is_whitespace()).count() / 2;
                break;
            }
            
            // Found non-empty, non-list line
            break;
        }

        // If we're in a list, the indentation must be more than the list level
        if list_level > 0 {
            let current_indent = line.chars().take_while(|c| c.is_whitespace()).count();
            return current_indent > list_level * 2 + 4;
        }

        // Not in a list context, standard 4-space rule applies
        true
    }

    fn detect_style(&self, content: &str) -> Option<CodeBlockStyle> {
        let lines: Vec<&str> = content.lines().collect();
        let mut fenced_found = false;
        let mut indented_found = false;
        let mut fenced_line = usize::MAX;
        let mut indented_line = usize::MAX;

        // First scan through all lines to find code blocks
        for (i, line) in lines.iter().enumerate() {
            if self.is_fenced_code_block_start(line) {
                fenced_found = true;
                fenced_line = fenced_line.min(i);
            } else if self.is_indented_code_block(&lines, i) {
                indented_found = true;
                indented_line = indented_line.min(i);
            }
        }

        if !fenced_found && !indented_found {
            // No code blocks found
            return None;
        } else if fenced_found && !indented_found {
            // Only fenced blocks found
            return Some(CodeBlockStyle::Fenced);
        } else if !fenced_found && indented_found {
            // Only indented blocks found
            return Some(CodeBlockStyle::Indented);
        } else {
            // Both types found - use the first one encountered
            if indented_line < fenced_line {
                return Some(CodeBlockStyle::Indented);
            } else {
                return Some(CodeBlockStyle::Fenced);
            }
        }
    }
}

impl Rule for MD046CodeBlockStyle {
    fn name(&self) -> &'static str {
        "MD046"
    }

    fn description(&self) -> &'static str {
        "Code blocks should use a consistent style"
    }

    fn check(&self, content: &str) -> LintResult {
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Determine target style
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.style.clone(),
        };

        // Track code block states for proper detection
        let mut in_fenced_block = false;
        let mut fenced_fence_type = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            
            // Handle fenced code blocks
            if !in_fenced_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                in_fenced_block = true;
                fenced_fence_type = Some(if trimmed.starts_with("```") { "```" } else { "~~~" });
                
                if target_style == CodeBlockStyle::Indented {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Code block style should be indented".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: String::new(), // Remove the opening fence
                        }),
                    });
                }
            } else if in_fenced_block && fenced_fence_type.is_some() {
                let fence = fenced_fence_type.unwrap();
                if trimmed.starts_with(fence) {
                    in_fenced_block = false;
                    fenced_fence_type = None;
                    
                    if target_style == CodeBlockStyle::Indented {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: "Code block style should be indented".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(i + 1, 1),
                                replacement: String::new(), // Remove the closing fence
                            }),
                        });
                    }
                } else if target_style == CodeBlockStyle::Indented {
                    // This is content within a fenced block that should be indented
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Code block style should be indented".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: "    ".to_string() + trimmed, // Add indentation
                        }),
                    });
                }
            } else if self.is_indented_code_block(&lines, i) && target_style == CodeBlockStyle::Fenced {
                // This is an indented code block that should be fenced
                
                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);
                let _next_line_is_indented = i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);
                
                if !prev_line_is_indented {
                    // Start of a new indented block
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Code block style should be fenced".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: "```\n".to_string() + line.trim_start(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }

        let lines: Vec<&str> = content.lines().collect();
        
        // Determine target style
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.style.clone(),
        };

        let mut result = String::with_capacity(content.len());
        let mut in_fenced_block = false;
        let mut fenced_fence_type = None;
        let mut in_indented_block = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            
            // Handle fenced code blocks
            if !in_fenced_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                in_fenced_block = true;
                fenced_fence_type = Some(if trimmed.starts_with("```") { "```" } else { "~~~" });
                
                if target_style == CodeBlockStyle::Indented {
                    // Skip the opening fence
                    in_indented_block = true;
                } else {
                    // Keep the fenced block
                    result.push_str(line);
                    result.push('\n');
                }
            } else if in_fenced_block && fenced_fence_type.is_some() {
                let fence = fenced_fence_type.unwrap();
                if trimmed.starts_with(fence) {
                    in_fenced_block = false;
                    fenced_fence_type = None;
                    in_indented_block = false;
                    
                    if target_style == CodeBlockStyle::Indented {
                        // Skip the closing fence
                    } else {
                        // Keep the fenced block
                        result.push_str(line);
                        result.push('\n');
                    }
                } else if target_style == CodeBlockStyle::Indented {
                    // Convert content inside fenced block to indented
                    result.push_str("    ");
                    result.push_str(trimmed);
                    result.push('\n');
                } else {
                    // Keep fenced block content as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if self.is_indented_code_block(&lines, i) {
                // This is an indented code block
                
                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);
                
                if target_style == CodeBlockStyle::Fenced {
                    if !prev_line_is_indented && !in_indented_block {
                        // Start of a new indented block that should be fenced
                        result.push_str("```\n");
                        result.push_str(line.trim_start());
                        result.push('\n');
                        in_indented_block = true;
                    } else {
                        // Inside an indented block
                        result.push_str(line.trim_start());
                        result.push('\n');
                    }
                    
                    // Check if this is the end of the indented block
                    let _next_line_is_indented = i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);
                    if !_next_line_is_indented && in_indented_block {
                        result.push_str("```\n");
                        in_indented_block = false;
                    }
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                // Regular line
                if in_indented_block && target_style == CodeBlockStyle::Fenced {
                    result.push_str("```\n");
                    in_indented_block = false;
                }
                
                result.push_str(line);
                result.push('\n');
            }
        }

        // Close any remaining blocks
        if in_indented_block && target_style == CodeBlockStyle::Fenced {
            result.push_str("```\n");
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
