use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to detect HTML comments (start and end tags separately)
    static ref HTML_COMMENT_START: Regex = Regex::new(r"<!--").unwrap();
    static ref HTML_COMMENT_END: Regex = Regex::new(r"-->").unwrap();
}

#[derive(Debug)]
pub struct MD010NoHardTabs {
    pub spaces_per_tab: usize,
    pub code_blocks: bool,
}

impl Default for MD010NoHardTabs {
    fn default() -> Self {
        Self {
            spaces_per_tab: 4,
            code_blocks: true,
        }
    }
}

impl MD010NoHardTabs {
    pub fn new(spaces_per_tab: usize, code_blocks: bool) -> Self {
        Self {
            spaces_per_tab,
            code_blocks,
        }
    }

    fn is_in_code_block(lines: &[&str], current_line: usize) -> bool {
        let mut fence_count = 0;
        for (i, line) in lines.iter().take(current_line + 1).enumerate() {
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                fence_count += 1;
            }
            if i == current_line && fence_count % 2 == 1 {
                return true;
            }
        }
        false
    }

    // Identify lines that are part of HTML comments
    fn find_html_comment_lines(lines: &[&str]) -> Vec<bool> {
        let mut in_html_comment = false;
        let mut html_comment_lines = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            // Check if this line has a comment start
            let has_comment_start = HTML_COMMENT_START.is_match(line);
            // Check if this line has a comment end
            let has_comment_end = HTML_COMMENT_END.is_match(line);

            if has_comment_start && !has_comment_end && !in_html_comment {
                // Comment starts on this line and doesn't end
                in_html_comment = true;
                html_comment_lines[i] = true;
            } else if has_comment_end && in_html_comment {
                // Comment ends on this line
                html_comment_lines[i] = true;
                in_html_comment = false;
            } else if has_comment_start && has_comment_end {
                // Both start and end on the same line
                html_comment_lines[i] = true;
            } else if in_html_comment {
                // We're inside a multi-line comment
                html_comment_lines[i] = true;
            }
        }

        html_comment_lines
    }

    fn count_leading_tabs(line: &str) -> usize {
        let mut count = 0;
        for c in line.chars() {
            if c == '\t' {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn find_tab_positions(line: &str) -> Vec<usize> {
        line.chars()
            .enumerate()
            .filter(|(_, c)| *c == '\t')
            .map(|(i, _)| i)
            .collect()
    }
}

impl Rule for MD010NoHardTabs {
    fn name(&self) -> &'static str {
        "MD010"
    }

    fn description(&self) -> &'static str {
        "No hard tabs"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        for (line_num, &line) in lines.iter().enumerate() {
            // Skip if in HTML comment
            if html_comment_lines[line_num] {
                continue;
            }
            
            // Skip if in code block and code_blocks is false
            if !self.code_blocks && Self::is_in_code_block(&lines, line_num) {
                continue;
            }

            let tab_positions = Self::find_tab_positions(line);
            if tab_positions.is_empty() {
                continue;
            }

            let leading_tabs = Self::count_leading_tabs(line);
            let non_leading_tabs = tab_positions.len() - leading_tabs;

            // Generate warning for each tab
            for &pos in &tab_positions {
                let is_leading = pos < leading_tabs;
                let message = if line.trim().is_empty() {
                    "Empty line contains hard tabs".to_string()
                } else if is_leading {
                    format!(
                        "Found {} leading hard tab(s), use {} spaces instead",
                        leading_tabs,
                        leading_tabs * self.spaces_per_tab
                    )
                } else {
                    format!(
                        "Found {} hard tab(s) for alignment, use spaces instead",
                        non_leading_tabs
                    )
                };

                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: pos + 1,
                    message,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, pos + 1),
                        replacement: line.replace('\t', &" ".repeat(self.spaces_per_tab)),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        for (i, line) in lines.iter().enumerate() {
            if html_comment_lines[i] {
                // Preserve HTML comments as they are
                result.push_str(line);
            } else if !self.code_blocks && Self::is_in_code_block(&lines, i) {
                result.push_str(line);
            } else {
                result.push_str(&line.replace('\t', &" ".repeat(self.spaces_per_tab)));
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
