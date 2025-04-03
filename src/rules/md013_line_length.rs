use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref URL_PATTERN: Regex = Regex::new(r"^https?://\S+$").unwrap();
    static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^\s*!\[.*?\]\[.*?\]\s*$").unwrap();
    static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\s*\[.*?\]:\s*https?://\S+\s*$").unwrap();
}

#[derive(Debug)]
pub struct MD013LineLength {
    pub line_length: usize,
    pub code_blocks: bool,
    pub tables: bool,
    pub headings: bool,
    pub strict: bool,
}

impl Default for MD013LineLength {
    fn default() -> Self {
        Self {
            line_length: 80,
            code_blocks: true,
            tables: false,
            headings: true,
            strict: false,
        }
    }
}

impl MD013LineLength {
    pub fn new(
        line_length: usize,
        code_blocks: bool,
        tables: bool,
        headings: bool,
        strict: bool,
    ) -> Self {
        Self {
            line_length,
            code_blocks,
            tables,
            headings,
            strict,
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

    fn is_in_table(lines: &[&str], current_line: usize) -> bool {
        // Check if current line is part of a table
        let current = lines[current_line].trim();
        if current.starts_with('|') || current.starts_with("|-") {
            return true;
        }

        // Check if line is between table markers
        if current_line > 0 && current_line + 1 < lines.len() {
            let prev = lines[current_line - 1].trim();
            let next = lines[current_line + 1].trim();
            if (prev.starts_with('|') || prev.starts_with("|-"))
                && (next.starts_with('|') || next.starts_with("|-"))
            {
                return true;
            }
        }
        false
    }

    fn is_heading(&self, lines: &[&str], current_line: usize) -> bool {
        let line = lines[current_line];

        // ATX headings
        if line.trim_start().starts_with('#') {
            return true;
        }

        // Setext headings (check for underline on next line)
        if current_line + 1 < lines.len() {
            let next = lines[current_line + 1].trim();
            if !next.is_empty() && next.chars().all(|c| c == '=' || c == '-') {
                return true;
            }
        }

        // Check if current line is a setext underline
        if !line.trim().is_empty()
            && line.trim().chars().all(|c| c == '=' || c == '-')
            && current_line > 0
            && !lines[current_line - 1].trim().is_empty()
        {
            return true;
        }

        false
    }

    fn should_ignore_line(&self, line: &str, lines: &[&str], current_line: usize) -> bool {
        if self.strict {
            return false;
        }

        // URLs on their own line
        if URL_PATTERN.is_match(line.trim()) {
            return true;
        }

        // Image references
        if IMAGE_REF_PATTERN.is_match(line) {
            return true;
        }

        // Link references
        if LINK_REF_PATTERN.is_match(line) {
            return true;
        }

        // Code blocks with long strings
        if Self::is_in_code_block(lines, current_line)
            && !line.trim().is_empty()
            && !line.contains(' ')
            && !line.contains('\t')
        {
            return true;
        }

        false
    }
}

impl Rule for MD013LineLength {
    fn name(&self) -> &'static str {
        "MD013"
    }

    fn description(&self) -> &'static str {
        "Line length should not be excessive"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, &line) in lines.iter().enumerate() {
            if line.len() > self.line_length {
                // Check if line should be skipped based on configuration
                let skip = (!self.code_blocks && Self::is_in_code_block(&lines, line_num))
                    || (!self.tables && Self::is_in_table(&lines, line_num))
                    || (!self.headings && self.is_heading(&lines, line_num))
                    || self.should_ignore_line(line, &lines, line_num);

                if !skip {
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: format!(
                            "Line length {} exceeds {} characters",
                            line.len(),
                            self.line_length
                        ),
                        line: line_num + 1,
                        column: self.line_length + 1,
                        severity: Severity::Warning,
                        fix: None, // Line wrapping requires manual intervention
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Line wrapping requires manual intervention as it needs to consider:
        // - Markdown syntax
        // - Word boundaries
        // - Indentation
        // - Lists and blockquotes
        // - Code blocks and tables
        Ok(content.to_string())
    }
}
