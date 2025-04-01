use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::markdown_elements::{MarkdownElements, ElementType};

#[derive(Debug, Default)]
pub struct MD023HeadingStartLeft;

impl Rule for MD023HeadingStartLeft {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Get all headings using the MarkdownElements utility
        let headings = MarkdownElements::detect_headings(content);

        for heading in headings {
            if heading.element_type != ElementType::Heading {
                continue;
            }

            // Get the line at this position
            let start_line = heading.start_line;
            if start_line >= lines.len() {
                continue; // Safety check
            }

            let line = lines[start_line];
            let indentation = line.len() - line.trim_start().len();

            // If the heading is indented, add a warning
            if indentation > 0 {
                // Determine if it's an ATX or Setext heading
                let is_setext = heading.end_line > heading.start_line;
                let level = if let Some(level_str) = &heading.metadata {
                    level_str.parse::<u32>().unwrap_or(1)
                } else {
                    1 // Default to level 1 if not specified
                };

                if is_setext {
                    // For Setext headings, we need to fix both the heading text and underline
                    let heading_text = lines[start_line].trim();
                    let underline_line = start_line + 1;
                    
                    if underline_line < lines.len() {
                        let underline_text = lines[underline_line].trim();
                        
                        // Add warning for the heading text line
                        warnings.push(LintWarning {
                            line: start_line + 1, // Convert to 1-indexed
                            column: 1,
                            severity: Severity::Warning,
                            message: format!(
                                "Setext heading should not be indented by {} spaces",
                                indentation
                            ),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(start_line + 1, 1),
                                replacement: heading_text.to_string(),
                            }),
                        });

                        // Add warning for the underline - only if it's indented
                        let underline_indentation = lines[underline_line].len() - lines[underline_line].trim_start().len();
                        if underline_indentation > 0 {
                            warnings.push(LintWarning {
                                line: underline_line + 1, // Convert to 1-indexed
                                column: 1,
                                severity: Severity::Warning,
                                message: "Setext heading underline should not be indented".to_string(),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(underline_line + 1, 1),
                                    replacement: underline_text.to_string(),
                                }),
                            });
                        }
                    }
                } else {
                    // For ATX headings, just fix the single line
                    let is_closed_atx = line.trim().ends_with('#');
                    let heading_content = if heading.text.trim().is_empty() {
                        String::new() // Empty heading
                    } else {
                        format!(" {}", heading.text.trim())
                    };
                    
                    // Create a fixed version without indentation
                    let fixed_heading = if is_closed_atx {
                        if heading_content.trim().is_empty() {
                            format!("{} {}", "#".repeat(level as usize), "#".repeat(level as usize))
                        } else {
                            format!("{}{} {}", "#".repeat(level as usize), heading_content, "#".repeat(level as usize))
                        }
                    } else {
                        format!("{}{}", "#".repeat(level as usize), heading_content)
                    };

                    warnings.push(LintWarning {
                        line: start_line + 1, // Convert to 1-indexed
                        column: 1,
                        severity: Severity::Warning,
                        message: format!(
                            "Heading should not be indented by {} spaces",
                            indentation
                        ),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(start_line + 1, 1),
                            replacement: fixed_heading,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        // Get all headings using the MarkdownElements utility
        let headings = MarkdownElements::detect_headings(content);
        
        // Create a map of line number to heading
        let mut heading_map = std::collections::HashMap::new();
        for heading in headings {
            if heading.element_type == ElementType::Heading {
                heading_map.insert(heading.start_line, heading);
            }
        }

        while i < lines.len() {
            // Check if this line is part of a heading
            if let Some(heading) = heading_map.get(&i) {
                let indentation = lines[i].len() - lines[i].trim_start().len();
                let is_setext = heading.end_line > heading.start_line;

                if indentation > 0 {
                    // This heading needs to be fixed
                    if is_setext {
                        // For Setext headings, add the heading text without indentation
                        fixed_lines.push(lines[i].trim().to_string());
                        // Then add the underline without indentation
                        if i + 1 < lines.len() {
                            fixed_lines.push(lines[i + 1].trim().to_string());
                        }
                        i += 2; // Skip both heading and underline
                    } else {
                        // For ATX headings, determine if it's closed
                        let is_closed_atx = lines[i].trim().ends_with('#');
                        
                        // Get the heading level
                        let level = if let Some(level_str) = &heading.metadata {
                            level_str.parse::<u32>().unwrap_or(1)
                        } else {
                            1 // Default to level 1 if not specified
                        };
                        
                        // Get heading content, handling empty headings
                        let heading_content = if heading.text.trim().is_empty() {
                            String::new() // Empty heading
                        } else {
                            format!(" {}", heading.text.trim())
                        };
                        
                        // Create a fixed version without indentation
                        let fixed_heading = if is_closed_atx {
                            if heading_content.trim().is_empty() {
                                format!("{} {}", "#".repeat(level as usize), "#".repeat(level as usize))
                            } else {
                                format!("{}{} {}", "#".repeat(level as usize), heading_content, "#".repeat(level as usize))
                            }
                        } else {
                            format!("{}{}", "#".repeat(level as usize), heading_content)
                        };
                        
                        fixed_lines.push(fixed_heading);
                        i += 1;
                    }
                } else {
                    // This heading is already at the beginning of the line
                    fixed_lines.push(lines[i].to_string());
                    if is_setext && i + 1 < lines.len() {
                        fixed_lines.push(lines[i + 1].to_string());
                        i += 2; // Skip both heading and underline
                    } else {
                        i += 1;
                    }
                }
            } else {
                // Not a heading, copy as-is
                fixed_lines.push(lines[i].to_string());
                i += 1;
            }
        }

        let result = fixed_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }
}
