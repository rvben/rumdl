use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

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

        for (line_num, &line) in lines.iter().enumerate() {
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

        for (i, line) in lines.iter().enumerate() {
            if !self.code_blocks && Self::is_in_code_block(&lines, i) {
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
