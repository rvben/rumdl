use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
    static ref FRONT_MATTER_PATTERN: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref SETEXT_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();
    static ref ATX_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*).*$").unwrap();
}

#[derive(Debug)]
pub struct MD022BlanksAroundHeadings {
    lines_above: usize,
    lines_below: usize,
}

impl Default for MD022BlanksAroundHeadings {
    fn default() -> Self {
        Self {
            lines_above: 1,
            lines_below: 1,
        }
    }
}

impl MD022BlanksAroundHeadings {
    pub fn new(lines_above: usize, lines_below: usize) -> Self {
        Self {
            lines_above,
            lines_below,
        }
    }

    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    fn count_blank_lines_above(&self, lines: &[&str], current_line: usize) -> usize {
        let mut count = 0;
        let mut line_idx = current_line;
        while line_idx > 0 {
            line_idx -= 1;
            if Self::is_blank_line(lines[line_idx]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn count_blank_lines_below(&self, lines: &[&str], current_line: usize) -> usize {
        let mut count = 0;
        let mut line_idx = current_line;
        while line_idx < lines.len() - 1 {
            line_idx += 1;
            if Self::is_blank_line(lines[line_idx]) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn is_setext_heading_underline(&self, line: &str) -> bool {
        SETEXT_HEADING_PATTERN.is_match(line)
    }

    fn check_heading(&self, line_num: usize, lines: &[&str], first_heading: bool, warnings: &mut Vec<LintWarning>) {
        let blank_lines_above = self.count_blank_lines_above(lines, line_num);
        let is_setext = line_num + 1 < lines.len() && self.is_setext_heading_underline(lines[line_num + 1]);
        let blank_lines_below = if is_setext {
            self.count_blank_lines_below(lines, line_num + 1)
        } else {
            self.count_blank_lines_below(lines, line_num)
        };
        let required_lines_above = if first_heading && line_num == 0 { 0 } else { self.lines_above };

        // Check blank lines above
        if blank_lines_above < required_lines_above {
            warnings.push(LintWarning {
                line: line_num + 1,
                column: 1,
                message: format!(
                    "Heading should have {} blank line{} above",
                    required_lines_above,
                    if required_lines_above == 1 { "" } else { "s" }
                ),
                fix: Some(Fix {
                    line: line_num + 1,
                    column: 1,
                    replacement: "\n".repeat(required_lines_above - blank_lines_above) + lines[line_num],
                }),
            });
        }

        // Check blank lines below
        if blank_lines_below < self.lines_below {
            let warning_line = if is_setext { line_num + 2 } else { line_num + 1 };
            let next_line = if is_setext { line_num + 2 } else { line_num + 1 };
            let is_next_heading = next_line < lines.len() && self.is_heading(lines[next_line]);

            if is_next_heading {
                if blank_lines_below < 1 {
                    warnings.push(LintWarning {
                        line: warning_line,
                        column: 1,
                        message: "Consecutive headings should be separated by a blank line".to_string(),
                        fix: Some(Fix {
                            line: warning_line,
                            column: 1,
                            replacement: format!("{}\n{}", lines[line_num], lines[next_line]),
                        }),
                    });
                }
            } else {
                warnings.push(LintWarning {
                    line: warning_line,
                    column: 1,
                    message: format!(
                        "Heading should have {} blank line{} below",
                        self.lines_below,
                        if self.lines_below == 1 { "" } else { "s" }
                    ),
                    fix: Some(Fix {
                        line: warning_line,
                        column: 1,
                        replacement: if is_setext {
                            format!("{}\n{}\n{}", lines[line_num], lines[line_num + 1], "\n".repeat(self.lines_below - blank_lines_below))
                        } else {
                            format!("{}\n{}", lines[line_num], "\n".repeat(self.lines_below - blank_lines_below))
                        },
                    }),
                });
            }
        }
    }

    fn is_heading(&self, line: &str) -> bool {
        ATX_HEADING_PATTERN.is_match(line)
    }

    fn is_setext_heading(&self, line: &str, next_line: Option<&str>) -> bool {
        if let Some(next) = next_line {
            !Self::is_blank_line(line) && self.is_setext_heading_underline(next)
        } else {
            false
        }
    }

    fn fix_content(&self, lines: &[&str], first_heading: bool, line_num: usize, result: &mut Vec<String>) {
        let required_lines_above = if first_heading && line_num == 0 { 0 } else { self.lines_above };
        let blank_lines_above = self.count_blank_lines_above(lines, line_num);
        let is_setext = line_num + 1 < lines.len() && self.is_setext_heading_underline(lines[line_num + 1]);
        let blank_lines_below = if is_setext {
            self.count_blank_lines_below(lines, line_num + 1)
        } else {
            self.count_blank_lines_below(lines, line_num)
        };

        // Add missing blank lines above if needed
        if !first_heading && blank_lines_above < required_lines_above {
            for _ in 0..(required_lines_above - blank_lines_above) {
                result.push(String::new());
            }
        }

        // Add the heading line(s)
        result.push(lines[line_num].to_string());
        if is_setext {
            result.push(lines[line_num + 1].to_string());
        }

        // Add missing blank lines below if needed
        let next_line = if is_setext { line_num + 2 } else { line_num + 1 };
        let is_next_heading = next_line < lines.len() && (
            self.is_heading(lines[next_line]) ||
            (next_line + 1 < lines.len() && self.is_setext_heading(lines[next_line], Some(lines[next_line + 1])))
        );

        if is_next_heading {
            // For consecutive headings, ensure exactly one blank line
            if blank_lines_below < 1 {
                result.push(String::new());
            }
        } else {
            for _ in 0..(self.lines_below - blank_lines_below) {
                result.push(String::new());
            }
        }
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let lines: Vec<&str> = content.lines().collect();
        let mut warnings = Vec::new();
        let mut first_heading = true;
        let mut in_code_block = false;
        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            // Handle code blocks
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            // Handle front matter
            if i == 0 && FRONT_MATTER_PATTERN.is_match(line) {
                in_front_matter = true;
                continue;
            }
            if in_front_matter && FRONT_MATTER_PATTERN.is_match(line) {
                in_front_matter = false;
                continue;
            }

            if in_code_block || in_front_matter {
                continue;
            }

            if self.is_heading(line) || self.is_setext_heading(line, lines.get(i + 1).map(|v| &**v)) {
                self.check_heading(i, &lines, first_heading, &mut warnings);
                first_heading = false;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();
        let mut first_heading = true;
        let mut in_code_block = false;
        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            // Handle code blocks
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push(line.to_string());
                continue;
            }

            // Handle front matter
            if i == 0 && FRONT_MATTER_PATTERN.is_match(line) {
                in_front_matter = true;
                result.push(line.to_string());
                continue;
            }
            if in_front_matter && FRONT_MATTER_PATTERN.is_match(line) {
                in_front_matter = false;
                result.push(line.to_string());
                continue;
            }

            if in_code_block || in_front_matter {
                result.push(line.to_string());
                continue;
            }

            if self.is_heading(line) || self.is_setext_heading(line, lines.get(i + 1).map(|v| &**v)) {
                self.fix_content(&lines, first_heading, i, &mut result);
                first_heading = false;
            } else {
                result.push(line.to_string());
            }
        }

        Ok(result.join("\n"))
    }
} 