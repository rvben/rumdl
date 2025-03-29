use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::code_block_utils::CodeBlockUtils;
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
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        // First check for fenced blocks
        for (start, end) in &code_blocks {
            let block_content = &content[*start..*end];
            if block_content.trim_start().starts_with("```") || block_content.trim_start().starts_with("~~~") {
                return Some(CodeBlockStyle::Fenced);
            }
        }

        // Then check for indented blocks
        for (i, _) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };
            
            // Skip if this line is part of a fenced code block
            if code_blocks.iter().any(|(start, end)| line_start >= *start && line_start < *end) {
                continue;
            }

            if self.is_indented_code_block(&lines, i) {
                return Some(CodeBlockStyle::Indented);
            }
        }
        None
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
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);

        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.style.clone(),
        };

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Skip if this line is part of a code span
            if code_blocks.iter().any(|(start, end)| {
                line_start >= *start && line_start < *end && !content[*start..*end].contains('\n')
            }) {
                continue;
            }

            if self.is_fenced_code_block_start(line) {
                if target_style == CodeBlockStyle::Indented {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Code block style should be indented".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: "    ".to_string() + line.trim_start(),
                        }),
                    });
                }
            } else if self.is_indented_code_block(&lines, i)
                && !code_blocks.iter().any(|(start, end)| line_start >= *start && line_start < *end)
                && target_style == CodeBlockStyle::Fenced
            {
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

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.style.clone(),
        };

        let mut result = String::new();
        let mut in_indented_block = false;

        for (i, line) in lines.iter().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };

            // Skip if this line is part of a code span
            if code_blocks.iter().any(|(start, end)| {
                line_start >= *start && line_start < *end && !content[*start..*end].contains('\n')
            }) {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if self.is_fenced_code_block_start(line) {
                if target_style == CodeBlockStyle::Indented {
                    // Convert fenced block to indented
                    result.push_str("    ");
                    result.push_str(line.trim_start());
                    result.push('\n');
                } else {
                    // Keep fenced block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if self.is_indented_code_block(&lines, i)
                && !code_blocks.iter().any(|(start, end)| line_start >= *start && line_start < *end)
            {
                if target_style == CodeBlockStyle::Fenced {
                    // Convert indented block to fenced
                    if !in_indented_block {
                        result.push_str("```\n");
                        in_indented_block = true;
                    }
                    result.push_str(line.trim_start());
                    result.push('\n');
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
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

        Ok(result)
    }
}
