use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;

/// Enforces consistent use of leading and trailing pipe characters in tables
#[derive(Debug)]
pub struct MD055TablePipeStyle {
    pub style: String,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
enum PipeStyle {
    LeadingAndTrailing,
    LeadingOnly,
    TrailingOnly,
    NoLeadingOrTrailing,
}

impl Default for MD055TablePipeStyle {
    fn default() -> Self {
        Self {
            style: "consistent".to_string(),
        }
    }
}

impl MD055TablePipeStyle {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    /// Determine the pipe style of a table row
    fn determine_pipe_style(&self, line: &str) -> Option<PipeStyle> {
        let trimmed = line.trim();
        
        // Skip empty lines or non-table lines
        if trimmed.is_empty() || !trimmed.contains('|') {
            return None;
        }

        let has_leading_pipe = trimmed.starts_with('|');
        let has_trailing_pipe = trimmed.ends_with('|');

        if has_leading_pipe && has_trailing_pipe {
            Some(PipeStyle::LeadingAndTrailing)
        } else if has_leading_pipe {
            Some(PipeStyle::LeadingOnly)
        } else if has_trailing_pipe {
            Some(PipeStyle::TrailingOnly)
        } else {
            Some(PipeStyle::NoLeadingOrTrailing)
        }
    }

    /// Check if a line is in a code block
    fn is_in_code_block(&self, lines: &[&str], line_index: usize) -> bool {
        let mut in_code_block = false;
        let mut code_fence = "";

        for (i, line) in lines.iter().enumerate() {
            if i > line_index {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    in_code_block = true;
                    code_fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                } else if trimmed.starts_with(code_fence) {
                    in_code_block = false;
                }
            }

            if i == line_index && in_code_block {
                return true;
            }
        }

        false
    }

    /// Identify table sections (groups of lines that form a table)
    fn identify_tables(&self, lines: &[&str]) -> Vec<(usize, usize)> {
        let mut tables = Vec::new();
        let mut current_table_start: Option<usize> = None;
        let mut line_has_pipe = false;
        let mut previous_line_has_pipe = false;

        for (i, line) in lines.iter().enumerate() {
            if self.is_in_code_block(lines, i) {
                continue;
            }

            let trimmed = line.trim();
            line_has_pipe = trimmed.contains('|');

            // Possible table row
            if line_has_pipe {
                if current_table_start.is_none() {
                    current_table_start = Some(i);
                }
            } else if current_table_start.is_some() && !line_has_pipe && !trimmed.is_empty() {
                // End of table
                if let Some(start) = current_table_start {
                    if i - start >= 2 { // At least header + delimiter rows
                        tables.push((start, i - 1));
                    }
                }
                current_table_start = None;
            }

            previous_line_has_pipe = line_has_pipe;
        }

        // Handle case where table ends at EOF
        if let Some(start) = current_table_start {
            if lines.len() - start >= 2 {
                tables.push((start, lines.len() - 1));
            }
        }

        tables
    }

    /// Fix a table row to match the desired style
    fn fix_table_row(&self, row: &str, target_style: &PipeStyle) -> String {
        let trimmed = row.trim();
        
        // Early return for non-table rows
        if !trimmed.contains('|') {
            return row.to_string();
        }

        let has_leading_pipe = trimmed.starts_with('|');
        let has_trailing_pipe = trimmed.ends_with('|');

        match target_style {
            PipeStyle::LeadingAndTrailing => {
                let mut result = trimmed.to_string();
                if !has_leading_pipe {
                    result = format!("|{}", result);
                }
                if !has_trailing_pipe {
                    result = format!("{}|", result);
                }
                result
            },
            PipeStyle::LeadingOnly => {
                let mut result = trimmed.to_string();
                if !has_leading_pipe {
                    result = format!("|{}", result);
                }
                if has_trailing_pipe {
                    result = result[..result.len()-1].to_string();
                }
                result
            },
            PipeStyle::TrailingOnly => {
                let mut result = trimmed.to_string();
                if has_leading_pipe {
                    result = result[1..].to_string();
                }
                if !has_trailing_pipe {
                    result = format!("{}|", result);
                }
                result
            },
            PipeStyle::NoLeadingOrTrailing => {
                let mut result = trimmed.to_string();
                if has_leading_pipe {
                    result = result[1..].to_string();
                }
                if has_trailing_pipe {
                    result = result[..result.len()-1].to_string();
                }
                result
            }
        }
    }
}

impl Rule for MD055TablePipeStyle {
    fn name(&self) -> &'static str {
        "MD055"
    }

    fn description(&self) -> &'static str {
        "Table pipe style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let tables = self.identify_tables(&lines);

        for (table_start, table_end) in tables {
            let mut styles = HashSet::new();
            
            // First pass: collect all styles used in this table
            for i in table_start..=table_end {
                if let Some(style) = self.determine_pipe_style(lines[i]) {
                    styles.insert(style);
                }
            }

            // If all rows use the same style, continue to next table
            if styles.len() <= 1 {
                continue;
            }

            // Determine target style
            let target_style = match self.style.as_str() {
                "consistent" => {
                    // Use the style of the first row
                    if let Some(style) = self.determine_pipe_style(lines[table_start]) {
                        style
                    } else {
                        continue; // Shouldn't happen if we identified a table correctly
                    }
                },
                "leading_and_trailing" => PipeStyle::LeadingAndTrailing,
                "leading_only" => PipeStyle::LeadingOnly,
                "trailing_only" => PipeStyle::TrailingOnly,
                "no_leading_or_trailing" => PipeStyle::NoLeadingOrTrailing,
                _ => continue, // Invalid style configuration
            };

            // Second pass: check if rows match the target style
            for i in table_start..=table_end {
                if let Some(style) = self.determine_pipe_style(lines[i]) {
                    if style != target_style {
                        let fixed_row = self.fix_table_row(lines[i], &target_style);
                        
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: format!(
                                "Table pipe style '{}' is not consistent with {}",
                                match style {
                                    PipeStyle::LeadingAndTrailing => "leading_and_trailing",
                                    PipeStyle::LeadingOnly => "leading_only",
                                    PipeStyle::TrailingOnly => "trailing_only",
                                    PipeStyle::NoLeadingOrTrailing => "no_leading_or_trailing",
                                },
                                match self.style.as_str() {
                                    "consistent" => "the first row",
                                    _ => "the configured style"
                                }
                            ),
                            fix: Some(Fix {
                                line: i + 1,
                                column: 1,
                                replacement: fixed_row,
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut warnings = self.check(content)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let warning_idx = warnings.iter().position(|w| w.line == i + 1);
            
            if let Some(idx) = warning_idx {
                if let Some(fix) = &warnings[idx].fix {
                    result.push(fix.replacement.clone());
                } else {
                    result.push(line.to_string());
                }
                warnings.remove(idx);
            } else {
                result.push(line.to_string());
            }
        }

        Ok(result.join("\n"))
    }
} 