use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::blockquote_utils::BlockquoteUtils;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;

#[derive(Debug, Default)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Checks if a line is completely empty (just whitespace)
    fn is_completely_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Generates the replacement for a blank blockquote line
    fn get_replacement(indent: &str, level: usize) -> String {
        let mut result = indent.to_string();

        // For nested blockquotes: ">>" or ">" based on level
        for _ in 0..level {
            result.push('>');
        }
        // Add a single space after the last '>'
        result.push(' ');

        result
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, content: &str) -> LintResult {
        let line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let mut in_blockquote = false;

        for (i, &line) in lines.iter().enumerate() {
            if Self::is_completely_empty_line(line) {
                // A completely empty line separates blockquotes
                in_blockquote = false;
                continue;
            }

            if BlockquoteUtils::is_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);

                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }

                // Check if this is an empty blockquote line
                if BlockquoteUtils::is_empty_blockquote(line) {
                    let indent = BlockquoteUtils::extract_indentation(line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "Blank line inside blockquote".to_string(),
                        line: i + 1,
                        column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: Self::get_replacement(&indent, level),
                        }),
                    });
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if there are no blockquotes
        if structure.blockquotes.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Process each blockquote region
        for blockquote in &structure.blockquotes {
            // Check for blank lines within this blockquote
            for line_num in blockquote.start_line..=blockquote.end_line {
                // Skip if out of bounds
                if line_num == 0 || line_num > lines.len() {
                    continue;
                }

                let line_idx = line_num - 1; // Convert to 0-indexed
                let line = lines[line_idx];

                // Check if this is an empty blockquote line
                if BlockquoteUtils::is_blockquote(line)
                    && BlockquoteUtils::is_empty_blockquote(line)
                {
                    let level = BlockquoteUtils::get_nesting_level(line);
                    let indent = BlockquoteUtils::extract_indentation(line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "Blank line inside blockquote".to_string(),
                        line: line_num,
                        column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement: Self::get_replacement(&indent, level),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::with_capacity(lines.len());

        let mut in_blockquote = false;

        for line in lines {
            if Self::is_completely_empty_line(line) {
                // Add empty lines as-is
                in_blockquote = false;
                result.push(line.to_string());
                continue;
            }

            if BlockquoteUtils::is_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);

                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                }

                // Handle empty blockquote lines
                if BlockquoteUtils::is_empty_blockquote(line) {
                    let indent = BlockquoteUtils::extract_indentation(line);
                    result.push(Self::get_replacement(&indent, level));
                } else {
                    // Add the line as is
                    result.push(line.to_string());
                }
            } else {
                // Non-blockquote line
                in_blockquote = false;
                result.push(line.to_string());
            }
        }

        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Blockquote
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        !content.contains('>')
    }
}

impl DocumentStructureExtensions for MD028NoBlanksBlockquote {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Only run if the document has blockquotes
        !doc_structure.blockquotes.is_empty()
    }
}
