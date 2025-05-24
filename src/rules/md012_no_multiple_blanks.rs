use crate::utils::range_utils::LineIndex;
use toml;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD012: No multiple consecutive blank lines
///
/// See [docs/md012.md](../../docs/md012.md) for full documentation, configuration, and examples.

#[derive(Debug, Clone)]
pub struct MD012NoMultipleBlanks {
    pub maximum: usize,
}

impl Default for MD012NoMultipleBlanks {
    fn default() -> Self {
        Self { maximum: 1 }
    }
}

impl MD012NoMultipleBlanks {
    pub fn new(maximum: usize) -> Self {
        Self { maximum }
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

    fn is_in_front_matter(lines: &[&str], current_line: usize) -> bool {
        if current_line == 0 {
            return lines[0].trim() == "---";
        }

        let mut dashes = 0;
        for (i, line) in lines.iter().take(current_line + 1).enumerate() {
            if line.trim() == "---" {
                dashes += 1;
            }
            if i == current_line && dashes == 1 {
                return true;
            }
        }
        false
    }
}

impl Rule for MD012NoMultipleBlanks {
    fn name(&self) -> &'static str {
        "MD012"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive blank lines"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for consecutive newlines or potential whitespace-only lines before processing
        // Look for multiple consecutive lines that could be blank (empty or whitespace-only)
        let lines: Vec<&str> = content.lines().collect();
        let has_potential_blanks = lines.windows(2).any(|pair| {
            pair[0].trim().is_empty() && pair[1].trim().is_empty()
        });

        if !has_potential_blanks {
            return Ok(Vec::new());
        }

        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut blank_count = 0;
        let mut blank_start = 0;

        for (line_num, &line) in lines.iter().enumerate() {
            // Skip code blocks and front matter
            if Self::is_in_code_block(&lines, line_num)
                || Self::is_in_front_matter(&lines, line_num)
            {
                continue;
            }

            if line.trim().is_empty() {
                if blank_count == 0 {
                    blank_start = line_num;
                }
                blank_count += 1;
            } else {
                if blank_count > self.maximum {
                    // Generate warnings for each excess blank line to match markdownlint behavior
                    let location = if blank_start == 0 {
                        "at start of file"
                    } else {
                        "between content"
                    };

                    // Report warnings starting from the (maximum+1)th blank line
                    for i in self.maximum..blank_count {
                        let excess_line = blank_start + i + 1; // +1 for 1-indexed lines
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            severity: Severity::Warning,
                            message: format!(
                                "Multiple consecutive blank lines {} (Expected: {}; Actual: {})",
                                location, self.maximum, blank_count
                            ),
                            line: excess_line,
                            column: 1,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(excess_line, 1),
                                replacement: String::new(), // Remove the excess line
                            }),
                        });
                    }
                }
                blank_count = 0;
            }
        }

        // Check for trailing blank lines
        if blank_count > self.maximum {
            let location = "at end of file";
            for i in self.maximum..blank_count {
                let excess_line = blank_start + i + 1;
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    severity: Severity::Warning,
                    message: format!(
                        "Multiple consecutive blank lines {} (Expected: {}; Actual: {})",
                        location, self.maximum, blank_count
                    ),
                    line: excess_line,
                    column: 1,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(excess_line, 1),
                        replacement: String::new(),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = Vec::new();

        let mut blank_count = 0;

        let lines: Vec<&str> = content.lines().collect();

        let mut in_code_block = false;

        let mut in_front_matter = false;

        let mut code_block_blanks = Vec::new();

        for &line in lines.iter() {
            // Track code blocks and front matter
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                // Handle accumulated blank lines before code block
                if !in_code_block {
                    let allowed_blanks = blank_count.min(self.maximum);
                    if allowed_blanks > 0 {
                        result.extend(vec![""; allowed_blanks]);
                    }
                    blank_count = 0;
                } else {
                    // Add accumulated blank lines inside code block
                    result.append(&mut code_block_blanks);
                }
                in_code_block = !in_code_block;
                result.push(line);
                continue;
            }

            if line.trim() == "---" {
                in_front_matter = !in_front_matter;
                if blank_count > 0 {
                    result.extend(vec![""; blank_count]);
                    blank_count = 0;
                }
                result.push(line);
                continue;
            }

            if in_code_block {
                if line.trim().is_empty() {
                    code_block_blanks.push(line);
                } else {
                    result.append(&mut code_block_blanks);
                    result.push(line);
                }
            } else if in_front_matter {
                if blank_count > 0 {
                    result.extend(vec![""; blank_count]);
                    blank_count = 0;
                }
                result.push(line);
            } else if line.trim().is_empty() {
                blank_count += 1;
            } else {
                // Add allowed blank lines before content
                let allowed_blanks = blank_count.min(self.maximum);
                if allowed_blanks > 0 {
                    result.extend(vec![""; allowed_blanks]);
                }
                blank_count = 0;
                result.push(line);
            }
        }

        // Handle trailing blank lines
        if !in_code_block {
            let allowed_blanks = blank_count.min(self.maximum);
            if allowed_blanks > 0 {
                result.extend(vec![""; allowed_blanks]);
            }
        }

        // Join lines and handle final newline

        let mut output = result.join("\n");
        if content.ends_with('\n') {
            output.push('\n');
        }

        Ok(output)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "maximum".to_string(),
            toml::Value::Integer(self.maximum as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let maximum =
            crate::config::get_rule_config_value::<usize>(config, "MD012", "maximum").unwrap_or(1);
        Box::new(MD012NoMultipleBlanks::new(maximum))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD012NoMultipleBlanks {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // MD012 checks for consecutive blank lines, so it's relevant for any non-empty content
        !ctx.content.is_empty()
    }
}
