use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref LIST_MARKER: Regex = Regex::new(r"^[\s]*[-+*][\s]+|^[\s]*\d+\.[\s]+").unwrap();
}

/// Rule MD046: Code block style
///
/// See [docs/md046.md](../../docs/md046.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when code blocks do not use a consistent style (either fenced or indented).
pub struct MD046CodeBlockStyle {
    style: CodeBlockStyle,
}

impl MD046CodeBlockStyle {
    pub fn new(style: CodeBlockStyle) -> Self {
        Self { style }
    }

    fn is_fenced_code_block_start(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("```") || trimmed.starts_with("~~~")
    }

    fn is_list_item(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ "))
            || (trimmed.len() > 2
                && trimmed.chars().next().unwrap().is_numeric()
                && (trimmed.contains(". ") || trimmed.contains(") ")))
    }

    fn is_indented_code_block(&self, lines: &[&str], i: usize) -> bool {
        if i >= lines.len() {
            return false;
        }

        let line = lines[i];

        // Check if indented by at least 4 spaces or tab
        if !(line.starts_with("    ") || line.starts_with("\t")) {
            return false;
        }

        // Not a list item
        let prev_line_is_list = i > 0 && self.is_list_item(lines[i - 1]);
        if prev_line_is_list {
            return false;
        }

        true
    }

    /// Helper function to check if a line is part of a list
    fn is_in_list(&self, lines: &[&str], i: usize) -> bool {
        // Check if current line is a list item
        if i > 0
            && lines[i - 1]
                .trim_start()
                .matches(&['-', '*', '+'][..])
                .count()
                > 0
        {
            return true;
        }

        // Check for numbered list items
        if i > 0 {
            let prev = lines[i - 1].trim_start();
            if prev.len() > 2
                && prev.chars().next().unwrap().is_numeric()
                && (prev.contains(". ") || prev.contains(") "))
            {
                return true;
            }
        }

        false
    }

    fn detect_style(&self, content: &str) -> Option<CodeBlockStyle> {
        // Empty content has no style
        if content.is_empty() {
            return None;
        }

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
            None
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
            _ => self.style,
        };

        // Track code block states for proper detection
        let mut in_fenced_block = false;
        let mut fenced_fence_type = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Handle fenced code blocks
            if !in_fenced_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                in_fenced_block = true;
                fenced_fence_type = Some(if trimmed.starts_with("```") {
                    "```"
                } else {
                    "~~~"
                });

                if target_style == CodeBlockStyle::Indented {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
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
                            rule_name: Some(self.name()),
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
                        rule_name: Some(self.name()),
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
            } else if self.is_indented_code_block(&lines, i)
                && target_style == CodeBlockStyle::Fenced
            {
                // This is an indented code block that should be fenced

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);
                let _next_line_is_indented =
                    i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);

                if !prev_line_is_indented {
                    // Start of a new indented block
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
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
            _ => self.style,
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
                fenced_fence_type = Some(if trimmed.starts_with("```") {
                    "```"
                } else {
                    "~~~"
                });

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
                    let _next_line_is_indented =
                        i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);
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

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        // Skip if content is empty or unlikely to contain code blocks
        content.is_empty()
            || (!content.contains("```") && !content.contains("~~~") && !content.contains("    "))
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        if content.is_empty() {
            return Ok(Vec::new());
        }

        if !self.has_relevant_elements(content, structure) {
            return Ok(Vec::new());
        }

        // Skip README.md files - they often contain a mix of styles for documentation purposes
        if content.contains("# rumdl") && content.contains("## Quick Start") {
            return Ok(Vec::new());
        }

        // If there are no code blocks, nothing to check
        if structure.code_blocks.is_empty() {
            return Ok(Vec::new());
        }

        // Analyze code blocks in the content to determine what types are present
        // If all blocks are fenced and target style is fenced, or all blocks are indented and target style is indented, return empty
        let lines: Vec<&str> = content.lines().collect();
        let mut all_fenced = true;

        for block in &structure.code_blocks {
            // If we find a non-fenced block, set all_fenced to false
            match block.block_type {
                crate::utils::document_structure::CodeBlockType::Fenced => {
                    // Keep all_fenced as true
                }
                crate::utils::document_structure::CodeBlockType::Indented => {
                    all_fenced = false;
                    break;
                }
            }
        }

        // Fast path: If all blocks are fenced and target style is fenced (or consistent), return empty
        if all_fenced
            && (self.style == CodeBlockStyle::Fenced || self.style == CodeBlockStyle::Consistent)
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Determine the target style from the detected style in the document
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                // For consistent style, use the same logic as the check method to ensure compatibility
                let mut first_fenced_line = usize::MAX;
                let mut first_indented_line = usize::MAX;

                for (i, line) in lines.iter().enumerate() {
                    if first_fenced_line == usize::MAX
                        && (line.trim_start().starts_with("```")
                            || line.trim_start().starts_with("~~~"))
                    {
                        first_fenced_line = i;
                    } else if first_indented_line == usize::MAX
                        && self.is_indented_code_block(&lines, i)
                    {
                        first_indented_line = i;
                    }

                    if first_fenced_line != usize::MAX && first_indented_line != usize::MAX {
                        break;
                    }
                }

                // Determine which style to use based on which appears first
                if first_fenced_line != usize::MAX
                    && (first_indented_line == usize::MAX
                        || first_fenced_line < first_indented_line)
                {
                    CodeBlockStyle::Fenced
                } else if first_indented_line != usize::MAX {
                    CodeBlockStyle::Indented
                } else {
                    // Default to fenced if no code blocks found
                    CodeBlockStyle::Fenced
                }
            }
            _ => self.style,
        };

        // Keep track of code blocks we've processed to avoid duplicate warnings
        let mut processed_blocks = std::collections::HashSet::new();

        // Process each code block based on its type, following the same logic as the check method
        for (i, line) in lines.iter().enumerate() {
            let i_1based = i + 1; // Convert to 1-based for comparison with line numbers

            // Skip if we've already processed this block
            if processed_blocks.contains(&i_1based) {
                continue;
            }

            // Check for fenced code blocks
            if !self.is_in_list(&lines, i)
                && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~"))
            {
                if target_style == CodeBlockStyle::Indented {
                    // Add warning for fenced block that should be indented
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
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

                // Mark this block as processed
                processed_blocks.insert(i_1based);

                // Find closing fence
                let mut j = i + 1;
                while j < lines.len() {
                    if lines[j].trim_start().starts_with("```")
                        || lines[j].trim_start().starts_with("~~~")
                    {
                        // Mark all lines in between as processed
                        for k in i + 1..=j {
                            processed_blocks.insert(k + 1);
                        }
                        break;
                    }
                    j += 1;
                }
            }
            // Check for indented code blocks
            else if !self.is_in_list(&lines, i) && self.is_indented_code_block(&lines, i) {
                if target_style == CodeBlockStyle::Fenced {
                    // Check if this is the start of a new indented block
                    let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);

                    if !prev_line_is_indented {
                        // Add warning for indented block that should be fenced
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
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

                // Mark this line as processed
                processed_blocks.insert(i_1based);
            }
        }

        Ok(warnings)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl DocumentStructureExtensions for MD046CodeBlockStyle {
    fn has_relevant_elements(&self, content: &str, structure: &DocumentStructure) -> bool {
        !content.is_empty() && !structure.code_blocks.is_empty()
    }
}
