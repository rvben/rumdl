use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_block_utils::CodeBlockUtils;
use crate::rules::heading_utils_new::HeadingUtilsNew;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Front matter delimiter pattern
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
}

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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Track code blocks

        let mut in_code_block = false;

        let mut in_front_matter = false;

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // Skip front matter
            if i == 0 && FRONT_MATTER_DELIMITER.is_match(line) {
                in_front_matter = true;
                i += 1;
                continue;
            }

            if in_front_matter {
                if FRONT_MATTER_DELIMITER.is_match(line) {
                    in_front_matter = false;
                }
                i += 1;
                continue;
            }

            // Skip code blocks
            if CodeBlockUtils::is_code_block_delimiter(line) {
                in_code_block = !in_code_block;
                i += 1;
                continue;
            }

            if in_code_block {
                i += 1;
                continue;
            }

            // Check for ATX headings with indentation
            if HeadingUtilsNew::is_atx_heading(line) {
                if let Some(heading) = HeadingUtilsNew::parse_atx_heading(line) {
                    if heading.indentation > 0 {
                        // Create a fixed version without indentation
                        let fixed_heading = if heading.style
                            == crate::rules::heading_utils_new::HeadingStyle::AtxClosed
                        {
                            HeadingUtilsNew::to_closed_atx_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            )
                        } else {
                            HeadingUtilsNew::to_atx_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            )
                        };

                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            severity: Severity::Warning,
                            message: format!(
                                "Heading should not be indented by {} spaces",
                                heading.indentation
                            ),
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(i + 1, 1),
                                replacement: fixed_heading,
                            }),
                        });
                    }
                }
                i += 1;
                continue;
            }

            // Check for Setext heading underlines
            if i + 1 < lines.len() {
                let next_line = lines[i + 1];
                if HeadingUtilsNew::is_setext_heading_underline(next_line) {
                    if let Some(heading) = HeadingUtilsNew::parse_setext_heading(line, Some(next_line))
                    {
                        if heading.indentation > 0 {
                            // We need to fix both the heading text and the underline
                            let (fixed_text, fixed_underline) = HeadingUtilsNew::to_setext_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            );

                            // Add warning for the heading text line
                            warnings.push(LintWarning {
                                line: i + 1,
                                column: 1,
                                severity: Severity::Warning,
                                message: format!(
                                    "Setext heading should not be indented by {} spaces",
                                    heading.indentation
                                ),
                                fix: Some(Fix {
                                    range: _line_index.line_col_to_byte_range(i + 1, 1),
                                    replacement: fixed_text,
                                }),
                            });

                            // Add warning for the underline
                            warnings.push(LintWarning {
                                line: i + 2,
                                column: 1,
                                severity: Severity::Warning,
                                message: "Setext heading underline should not be indented".to_string(),
                                fix: Some(Fix {
                                    range: _line_index.line_col_to_byte_range(i + 2, 1),
                                    replacement: fixed_underline,
                                }),
                            });
                        }
                        i += 2; // Skip both the heading and the underline
                        continue;
                    }
                }
            }

            i += 1;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Track code blocks

        let mut in_code_block = false;

        let mut in_front_matter = false;

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // Handle front matter
            if i == 0 && FRONT_MATTER_DELIMITER.is_match(line) {
                in_front_matter = true;
                i += 1;
                continue;
            }

            if in_front_matter {
                if FRONT_MATTER_DELIMITER.is_match(line) {
                    in_front_matter = false;
                }
                result.push(line.to_string());
                i += 1;
                continue;
            }

            // Handle code blocks
            if CodeBlockUtils::is_code_block_delimiter(line) {
                in_code_block = !in_code_block;
                result.push(line.to_string());
                i += 1;
                continue;
            }

            if in_code_block {
                result.push(line.to_string());
                i += 1;
                continue;
            }

            // Handle ATX headings
            if HeadingUtilsNew::is_atx_heading(line) {
                if let Some(heading) = HeadingUtilsNew::parse_atx_heading(line) {
                    if heading.indentation > 0 {
                        // Create a fixed version without indentation
                        let fixed_heading = if heading.style
                            == crate::rules::heading_utils_new::HeadingStyle::AtxClosed
                        {
                            HeadingUtilsNew::to_closed_atx_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            )
                        } else {
                            HeadingUtilsNew::to_atx_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            )
                        };

                        result.push(fixed_heading);
                    } else {
                        result.push(line.to_string());
                    }
                    i += 1;
                    continue;
                }
            }

            // Handle Setext headings
            if i + 1 < lines.len() {
                let next_line = lines[i + 1];
                if HeadingUtilsNew::is_setext_heading_underline(next_line) {
                    if let Some(heading) =
                        HeadingUtilsNew::parse_setext_heading(line, Some(next_line))
                    {
                        if heading.indentation > 0 {
                            // Fix both the heading text and the underline
                            let (fixed_text, fixed_underline) = HeadingUtilsNew::to_setext_style(
                                &crate::rules::heading_utils_new::Heading {
                                    indentation: 0,
                                    ..heading
                                },
                            );

                            result.push(fixed_text);
                            result.push(fixed_underline);
                        } else {
                            result.push(line.to_string());
                            result.push(next_line.to_string());
                        }
                        i += 2; // Skip both the heading and the underline
                        continue;
                    }
                }
            }

            // Regular line
            result.push(line.to_string());
            i += 1;
        }

        Ok(result.join("\n"))
    }
}
