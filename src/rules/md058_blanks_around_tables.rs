use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use crate::utils::table_utils::TableUtils;

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
    /// Check if a line is blank
    fn is_blank_line(&self, line: &str) -> bool {
        line.trim().is_empty()
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

        // Early return for empty content or content without tables
        if content.is_empty() || !content.contains('|') {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();

        // Use shared table detection for better performance
        let table_blocks = TableUtils::find_table_blocks(content);

        for table_block in table_blocks {
            // Check for blank line before table
            if table_block.start_line > 0 && !self.is_blank_line(lines[table_block.start_line - 1])
            {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line before table".to_string(),
                    line: table_block.start_line + 1,
                    column: 1,
                    end_line: table_block.start_line + 1,
                    end_column: 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(table_block.start_line + 1, 1),
                        replacement: format!("\n{}", lines[table_block.start_line]),
                    }),
                });
            }

            // Check for blank line after table
            if table_block.end_line < lines.len() - 1
                && !self.is_blank_line(lines[table_block.end_line + 1])
            {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Missing blank line after table".to_string(),
                    line: table_block.end_line + 1,
                    column: lines[table_block.end_line].len() + 1,
                    end_line: table_block.end_line + 1,
                    end_column: lines[table_block.end_line].len() + 2,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(
                            table_block.end_line + 1,
                            lines[table_block.end_line].len() + 1,
                        ),
                        replacement: format!("{}\n", lines[table_block.end_line]),
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
