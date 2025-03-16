use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils;
use crate::utils::range_utils::LineIndex;

/// Rule MD022: Blanks around headings
#[derive(Debug)]
pub struct MD022BlanksAroundHeadings {
    lines_above: usize,
    lines_below: usize,
}

impl MD022BlanksAroundHeadings {
    pub fn new(lines_above: usize, lines_below: usize) -> Self {
        Self {
            lines_above,
            lines_below,
        }
    }

    pub fn default() -> Self {
        Self {
            lines_above: 1,
            lines_below: 1,
        }
    }

    // Count blank lines above a given line
    fn count_blank_lines_above(&self, lines: &[&str], current_line: usize) -> usize {
        let mut blank_lines = 0;
        let mut line_index = current_line;

        while line_index > 0 {
            line_index -= 1;
            if lines[line_index].trim().is_empty() {
                blank_lines += 1;
            } else {
                break;
            }
        }

        blank_lines
    }

    // Count blank lines below a given line
    fn count_blank_lines_below(&self, lines: &[&str], current_line: usize) -> usize {
        let mut blank_lines = 0;
        let mut line_index = current_line + 1;

        while line_index < lines.len() {
            if lines[line_index].trim().is_empty() {
                blank_lines += 1;
                line_index += 1;
            } else {
                break;
            }
        }

        blank_lines
    }

    // Check if line is an ATX heading (including empty headings like "#")
    fn is_atx_heading(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with('#')
            && (trimmed.len() == 1 || (trimmed.chars().nth(1).map_or(false, |c| c.is_whitespace())))
    }

    // Check if the current line is a Setext heading underline
    fn is_setext_underline(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed.chars().all(|c| c == '=' || c == '-')
    }

    // Check if current line is a setext heading content line
    fn is_setext_content(&self, lines: &[&str], i: usize, index: &LineIndex) -> bool {
        i + 1 < lines.len()
            && !lines[i].trim().is_empty()
            && self.is_setext_underline(lines[i + 1])
            && !index.is_code_block(i + 1)
    }

    fn check_internal(&self, content: &str) -> LintResult {
        let lines: Vec<&str> = content.lines().collect();
        let index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Skip YAML front matter if present
        let mut start_index = 0;
        if !lines.is_empty() && lines[0].trim() == "---" {
            for (i, line) in lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    start_index = i + 1;
                    break;
                }
            }
        }

        // Process each line
        let mut i = start_index;
        while i < lines.len() {
            // Skip if in code block
            if index.is_code_block(i) {
                i += 1;
                continue;
            }

            // Check for ATX headings
            if self.is_atx_heading(lines[i]) {
                // Check for blank lines above (except first line after front matter)
                if i > start_index {
                    let blank_lines_above = self.count_blank_lines_above(&lines, i);
                    if blank_lines_above < self.lines_above {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: format!(
                                "Heading should have at least {} blank line{} above",
                                self.lines_above,
                                if self.lines_above > 1 { "s" } else { "" }
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }

                // Check for blank lines below
                let blank_lines_below = self.count_blank_lines_below(&lines, i);
                if blank_lines_below < self.lines_below {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: format!(
                            "Heading should have at least {} blank line{} below",
                            self.lines_below,
                            if self.lines_below > 1 { "s" } else { "" }
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }

                // Check if heading is indented
                let indentation = heading_utils::get_heading_indentation(&lines, i);
                if indentation > 0 {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Headings should not be indented".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }

                i += 1;
                continue;
            }

            // Check for Setext headings
            if self.is_setext_content(&lines, i, &index) {
                // Check for blank lines above the content line (except first line)
                if i > start_index {
                    let blank_lines_above = self.count_blank_lines_above(&lines, i);
                    if blank_lines_above < self.lines_above {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: format!(
                                "Heading should have at least {} blank line{} above",
                                self.lines_above,
                                if self.lines_above > 1 { "s" } else { "" }
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }

                // Check for blank lines below the underline
                let blank_lines_below = self.count_blank_lines_below(&lines, i + 1);
                if blank_lines_below < self.lines_below {
                    warnings.push(LintWarning {
                        line: i + 2, // underline line
                        column: 1,
                        message: format!(
                            "Heading should have at least {} blank line{} below",
                            self.lines_below,
                            if self.lines_below > 1 { "s" } else { "" }
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }

                // Check indentation for content line
                let indentation = heading_utils::get_heading_indentation(&lines, i);
                if indentation > 0 {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Headings should not be indented".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }

                // Check indentation for underline
                let underline_indentation = heading_utils::get_heading_indentation(&lines, i + 1);
                if underline_indentation > 0 {
                    warnings.push(LintWarning {
                        line: i + 2, // underline line
                        column: 1,
                        message: "Heading underlines should not be indented".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }

                i += 2; // Skip the underline
                continue;
            }

            i += 1;
        }

        Ok(warnings)
    }

    fn fix_content(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let index = LineIndex::new(content.to_string());
        let mut result = Vec::new();

        // Skip YAML front matter if present
        let mut start_index = 0;
        let mut is_front_matter = false;

        if !lines.is_empty() && lines[0].trim() == "---" {
            is_front_matter = true;
            for (i, line) in lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    start_index = i + 1;
                    break;
                }
            }
        }

        // Add front matter if present
        if is_front_matter {
            for i in 0..start_index {
                result.push(lines[i].to_string());
            }
        }

        // Process each line
        let mut i = start_index;
        while i < lines.len() {
            if index.is_code_block(i) {
                // If in code block, add as-is
                result.push(lines[i].to_string());
                i += 1;
                continue;
            }

            // Detect headings
            let is_atx_heading = self.is_atx_heading(lines[i]);
            let is_setext_content = self.is_setext_content(&lines, i, &index);

            if is_atx_heading || is_setext_content {
                // Add blank lines above (unless at start of document)
                if i > start_index {
                    // First, remove trailing blanks from result
                    while !result.is_empty() && result.last().unwrap().trim().is_empty() {
                        result.pop();
                    }

                    // Add required blank lines above
                    for _ in 0..self.lines_above {
                        result.push("".to_string());
                    }
                }

                // Add the heading line
                result.push(lines[i].to_string());

                // For Setext, add the underline line too
                if is_setext_content {
                    i += 1;
                    result.push(lines[i].to_string());
                }

                // Add blank lines below
                for _ in 0..self.lines_below {
                    result.push("".to_string());
                }

                // Skip past any existing blank lines after the heading
                i += 1;
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
            } else {
                // Regular non-heading line
                result.push(lines[i].to_string());
                i += 1;
            }
        }

        // Join the lines with newlines
        let mut fixed = result.join("\n");

        // Preserve original trailing newline if it existed
        if content.ends_with('\n') && !fixed.ends_with('\n') {
            fixed.push('\n');
        }

        Ok(fixed)
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "blanks-around-headings"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        self.check_internal(content)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        self.fix_content(content)
    }
}
