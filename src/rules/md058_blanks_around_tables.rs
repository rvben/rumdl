use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;

/// Rule MD058: Blanks around tables
///
/// See [docs/md058.md](../../docs/md058.md) for full documentation, configuration, and examples.
///
/// Ensures tables have blank lines before and after them

#[derive(Clone)]
pub struct MD058BlanksAroundTables;

impl Default for MD058BlanksAroundTables {
    fn default() -> Self {
        MD058BlanksAroundTables
    }
}

impl MD058BlanksAroundTables {
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
                    code_fence = if trimmed.starts_with("```") {
                        "```"
                    } else {
                        "~~~"
                    };
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

    /// Check if a line is a table row
    fn is_table_row(&self, line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains('|')
    }

    /// Check if a line is a delimiter row (separates header from body)
    fn is_delimiter_row(&self, line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains('|')
            && trimmed
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
    }

    /// Check if a line is blank
    fn is_blank_line(&self, line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Identify table sections (groups of lines that form a table)
    fn identify_tables(&self, lines: &[&str]) -> Vec<(usize, usize)> {
        let mut tables = Vec::new();

        let mut current_table_start: Option<usize> = None;

        let mut found_delimiter = false;

        for (i, line) in lines.iter().enumerate() {
            if self.is_in_code_block(lines, i) {
                continue;
            }

            let is_table_row = self.is_table_row(line);
            let is_delimiter = self.is_delimiter_row(line);

            // Track delimiter row to ensure we have a valid table
            if is_delimiter {
                found_delimiter = true;
            }

            // Possible table row
            if is_table_row {
                if current_table_start.is_none() {
                    current_table_start = Some(i);
                }
            } else if current_table_start.is_some() && !is_table_row {
                // End of table
                if let Some(start) = current_table_start {
                    if found_delimiter {
                        tables.push((start, i - 1));
                    }
                }
                current_table_start = None;
                found_delimiter = false;
            }
        }

        // Handle case where table ends at EOF
        if let Some(start) = current_table_start {
            if found_delimiter {
                tables.push((start, lines.len() - 1));
            }
        }

        tables
    }
}

impl Rule for MD058BlanksAroundTables {
    fn name(&self) -> &'static str {
        "MD058"
    }

    fn description(&self) -> &'static str {
        "Tables should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let tables = self.identify_tables(&lines);

        for (table_start, table_end) in tables {
            // Check for blank line before table
            if table_start > 0 && !self.is_blank_line(lines[table_start - 1]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line before table".to_string(),
                    line: table_start + 1,
                    column: 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(table_start + 1, 1),
                        replacement: format!("\n{}", lines[table_start]),
                    }),
                });
            }

            // Check for blank line after table
            if table_end < lines.len() - 1 && !self.is_blank_line(lines[table_end + 1]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line after table".to_string(),
                    line: table_end + 1,
                    column: lines[table_end].len() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index
                            .line_col_to_byte_range(table_end + 1, lines[table_end].len() + 1),
                        replacement: format!("{}\n", lines[table_end]),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::new();

        let mut i = 0;

        while i < lines.len() {
            let warning_before = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message == "Missing blank line before table");

            if let Some(idx) = warning_before {
                result.push("".to_string());
                warnings.remove(idx);
            }

            result.push(lines[i].to_string());

            let warning_after = warnings
                .iter()
                .position(|w| w.line == i + 1 && w.message == "Missing blank line after table");

            if let Some(idx) = warning_after {
                result.push("".to_string());
                warnings.remove(idx);
            }

            i += 1;
        }

        Ok(result.join("\n"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD058BlanksAroundTables)
    }
}
