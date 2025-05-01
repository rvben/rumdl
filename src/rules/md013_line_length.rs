/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::DocumentStructure;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref URL_PATTERN: Regex = Regex::new(r"^https?://\S+$").unwrap();
    static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^\s*!\[.*?\]\[.*?\]\s*$").unwrap();
    static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\s*\[.*?\]:\s*https?://\S+\s*$").unwrap();
}

#[derive(Clone)]
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

    fn should_ignore_line(
        &self,
        line: &str,
        _lines: &[&str],
        current_line: usize,
        structure: &DocumentStructure,
    ) -> bool {
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
        if structure.is_in_code_block(current_line + 1)
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
        let structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();

        // Create a quick lookup set for heading lines
        let heading_lines_set: std::collections::HashSet<usize> = structure.heading_lines.iter().cloned().collect();
        // Create a quick lookup for setext headings (where start_line != end_line in regions)
        let setext_lines_set: std::collections::HashSet<usize> = structure.heading_regions.iter()
            .filter(|(start, end)| start != end)
            .flat_map(|(start, end)| (*start..=*end).collect::<Vec<usize>>())
            .collect();

        for (line_num, &line) in lines.iter().enumerate() {
            let line_number = line_num + 1; // 1-based line number

            // Skip setext underline lines (=== or ---)
            if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                continue;
            }

            // Use the precomputed setext check
            let effective_length = if self.strict && setext_lines_set.contains(&line_number) {
                line.trim_end().len() // Strict mode for setext headings
            } else {
                line.len() // Normal length check
            };

            if effective_length > self.line_length {
                // Check if line should be skipped based on configuration
                let is_heading = heading_lines_set.contains(&line_number); // Use the set
                let skip = (!self.code_blocks && structure.is_in_code_block(line_number))
                    || (!self.tables && Self::is_in_table(&lines, line_num))
                    || (!self.headings && is_heading) // Use the variable
                    || self.should_ignore_line(line, &lines, line_num, &structure);

                if !skip {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()), // Restore Option<&str>
                        message: format!(
                            "Line length {} exceeds {} characters",
                            effective_length,
                            self.line_length
                        ),
                        line: line_number,
                        column: self.line_length + 1,
                        severity: Severity::Warning,
                        fix: None, // Line wrapping requires manual intervention
                    });
                }
            }
        }
        Ok(warnings) // Restore Ok() wrapper
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "line-length".to_string(),
            toml::Value::Integer(self.line_length as i64),
        );
        map.insert(
            "code_blocks".to_string(),
            toml::Value::Boolean(self.code_blocks),
        );
        map.insert("tables".to_string(), toml::Value::Boolean(self.tables));
        map.insert("headings".to_string(), toml::Value::Boolean(self.headings));
        map.insert("strict".to_string(), toml::Value::Boolean(self.strict));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        let line_length = crate::config::get_rule_config_value::<usize>(config, "MD013", "line_length").unwrap_or(80);
        let code_blocks = crate::config::get_rule_config_value::<bool>(config, "MD013", "code_blocks").unwrap_or(true);
        let tables = crate::config::get_rule_config_value::<bool>(config, "MD013", "tables").unwrap_or(false);
        let headings = crate::config::get_rule_config_value::<bool>(config, "MD013", "headings").unwrap_or(true);
        let strict = crate::config::get_rule_config_value::<bool>(config, "MD013", "strict").unwrap_or(false);
        Box::new(MD013LineLength::new(line_length, code_blocks, tables, headings, strict))
    }
}
