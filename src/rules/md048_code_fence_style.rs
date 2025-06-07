use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::utils::range_utils::{calculate_match_range, LineIndex};
use crate::utils::code_block_utils::CodeBlockUtils;
use toml;

/// Rule MD048: Code fence style
///
/// See [docs/md048.md](../../docs/md048.md) for full documentation, configuration, and examples.
#[derive(Clone)]
pub struct MD048CodeFenceStyle {
    style: CodeFenceStyle,
}

impl MD048CodeFenceStyle {
    pub fn new(style: CodeFenceStyle) -> Self {
        Self { style }
    }

    fn detect_style(&self, content: &str) -> Option<CodeFenceStyle> {
        // Detect all code blocks to skip nested ones
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        let mut byte_pos = 0;
        
        for line in content.lines() {
            let trimmed = line.trim_start();
            let fence_start = byte_pos + (line.len() - trimmed.len());
            
            // Skip if this fence is inside a code block
            if !CodeBlockUtils::is_in_code_block_or_span(&code_blocks, fence_start) {
                if trimmed.starts_with("```") {
                    return Some(CodeFenceStyle::Backtick);
                } else if trimmed.starts_with("~~~") {
                    return Some(CodeFenceStyle::Tilde);
                }
            }
            
            byte_pos += line.len() + 1; // +1 for newline
        }
        None
    }
}

impl Rule for MD048CodeFenceStyle {
    fn name(&self) -> &'static str {
        "MD048"
    }

    fn description(&self) -> &'static str {
        "Code fence style should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let target_style = match self.style {
            CodeFenceStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(CodeFenceStyle::Backtick),
            _ => self.style,
        };
        
        // Track if we're inside a code block
        let mut in_code_block = false;
        let mut code_block_fence = String::new();

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            
            // Check for code fence markers
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
                let fence_length = trimmed.chars().take_while(|&c| c == fence_char).count();
                let current_fence = fence_char.to_string().repeat(fence_length);
                
                if !in_code_block {
                    // Entering a code block
                    in_code_block = true;
                    code_block_fence = current_fence.clone();
                    
                    // Check this opening fence
                    if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                        // Find the position and length of the backtick fence
                        let fence_start = line.len() - trimmed.len();
                        let fence_end =
                            fence_start + trimmed.find(|c: char| c != '`').unwrap_or(trimmed.len());

                        // Calculate precise character range for the entire fence
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: "Code fence style: use ~~~ instead of ```".to_string(),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement: line.replace("```", "~~~"),
                            }),
                        });
                    } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                        // Find the position and length of the tilde fence
                        let fence_start = line.len() - trimmed.len();
                        let fence_end =
                            fence_start + trimmed.find(|c: char| c != '~').unwrap_or(trimmed.len());

                        // Calculate precise character range for the entire fence
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: "Code fence style: use ``` instead of ~~~".to_string(),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement: line.replace("~~~", "```"),
                            }),
                        });
                    }
                } else if trimmed.starts_with(&code_block_fence) && 
                          trimmed[code_block_fence.len()..].trim().is_empty() {
                    // Exiting the code block - check this closing fence too
                    if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                        // Find the position and length of the backtick fence
                        let fence_start = line.len() - trimmed.len();
                        let fence_end =
                            fence_start + trimmed.find(|c: char| c != '`').unwrap_or(trimmed.len());

                        // Calculate precise character range for the entire fence
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: "Code fence style: use ~~~ instead of ```".to_string(),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement: line.replace("```", "~~~"),
                            }),
                        });
                    } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                        // Find the position and length of the tilde fence
                        let fence_start = line.len() - trimmed.len();
                        let fence_end =
                            fence_start + trimmed.find(|c: char| c != '~').unwrap_or(trimmed.len());

                        // Calculate precise character range for the entire fence
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: "Code fence style: use ``` instead of ~~~".to_string(),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement: line.replace("~~~", "```"),
                            }),
                        });
                    }
                    
                    in_code_block = false;
                    code_block_fence.clear();
                }
                // If it's a fence inside a code block, skip it
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let target_style = match self.style {
            CodeFenceStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(CodeFenceStyle::Backtick),
            _ => self.style,
        };

        let mut result = String::new();
        let mut in_code_block = false;
        let mut code_block_fence = String::new();
        
        for line in content.lines() {
            let trimmed = line.trim_start();
            
            // Check for code fence markers
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
                let fence_length = trimmed.chars().take_while(|&c| c == fence_char).count();
                let current_fence = fence_char.to_string().repeat(fence_length);
                
                if !in_code_block {
                    // Entering a code block
                    in_code_block = true;
                    code_block_fence = current_fence.clone();
                    
                    // Fix this opening fence
                    if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                        // Replace all backticks with tildes, preserving the count
                        let prefix = &line[..line.len() - trimmed.len()];
                        let rest = &trimmed[fence_length..];
                        result.push_str(prefix);
                        result.push_str(&"~".repeat(fence_length));
                        result.push_str(rest);
                    } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                        // Replace all tildes with backticks, preserving the count
                        let prefix = &line[..line.len() - trimmed.len()];
                        let rest = &trimmed[fence_length..];
                        result.push_str(prefix);
                        result.push_str(&"`".repeat(fence_length));
                        result.push_str(rest);
                    } else {
                        result.push_str(line);
                    }
                } else if trimmed.starts_with(&code_block_fence) && 
                          trimmed[code_block_fence.len()..].trim().is_empty() {
                    // Exiting the code block - fix this closing fence too
                    if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                        // Replace all backticks with tildes, preserving the count
                        let prefix = &line[..line.len() - trimmed.len()];
                        let fence_length = trimmed.chars().take_while(|&c| c == '`').count();
                        let rest = &trimmed[fence_length..];
                        result.push_str(prefix);
                        result.push_str(&"~".repeat(fence_length));
                        result.push_str(rest);
                    } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                        // Replace all tildes with backticks, preserving the count
                        let prefix = &line[..line.len() - trimmed.len()];
                        let fence_length = trimmed.chars().take_while(|&c| c == '~').count();
                        let rest = &trimmed[fence_length..];
                        result.push_str(prefix);
                        result.push_str(&"`".repeat(fence_length));
                        result.push_str(rest);
                    } else {
                        result.push_str(line);
                    }
                    
                    in_code_block = false;
                    code_block_fence.clear();
                } else {
                    // Inside a code block - don't fix nested fences
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
        
        // Remove the last newline if the original content didn't end with one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(self.style.to_string()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD048", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "backtick" => CodeFenceStyle::Backtick,
            "tilde" => CodeFenceStyle::Tilde,
            "consistent" => CodeFenceStyle::Consistent,
            _ => CodeFenceStyle::Consistent,
        };
        Box::new(MD048CodeFenceStyle::new(style))
    }
}
