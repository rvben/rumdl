/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref URL_PATTERN: Regex = Regex::new(r"^https?://\S+$").unwrap();
    static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^!\[.*?\]\[.*?\]$" ).unwrap();
    static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\[.*?\]:\s*https?://\S+$").unwrap();
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
            tables: true,
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

        // Only skip if the entire line is a URL
        if URL_PATTERN.is_match(line) {
            return true;
        }
        // Only skip if the entire line is an image reference
        if IMAGE_REF_PATTERN.is_match(line) {
            return true;
        }
        // Only skip if the entire line is a link reference
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Create a quick lookup set for heading lines
        let heading_lines_set: std::collections::HashSet<usize> =
            structure.heading_lines.iter().cloned().collect();

        // Create a quick lookup for setext headings (where start_line != end_line in regions)
        let _setext_lines_set: std::collections::HashSet<usize> = structure
            .heading_regions
            .iter()
            .filter(|(start, end)| start != end)
            .flat_map(|(start, end)| (*start..=*end).collect::<Vec<usize>>())
            .collect();

        // Create a quick lookup set for list item lines (including continuations)
        let _list_lines_set: std::collections::HashSet<usize> =
            structure.list_lines.iter().cloned().collect();

        for (line_num, &line) in lines.iter().enumerate() {
            let line_number = line_num + 1; // 1-based

            if !self.strict {
                // Skip setext underline lines (=== or ---)
                if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                    continue;
                }

                // Skip block elements according to config flags
                let mut is_block = false;
                if self.headings && heading_lines_set.contains(&line_number) {
                    is_block = true;
                }
                if !self.code_blocks && structure.is_in_code_block(line_number) {
                    is_block = true;
                }
                if self.tables && Self::is_in_table(&lines, line_num) {
                    is_block = true;
                }
                if structure.is_in_blockquote(line_number) {
                    is_block = true;
                }
                if structure.is_in_html_block(line_number) {
                    is_block = true;
                }
                if is_block {
                    continue;
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, &lines, line_num, structure) {
                    continue;
                }
            }

            // Check line length
            let effective_length = line.len();
            if effective_length > self.line_length {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Line length {} exceeds {} characters",
                        effective_length, self.line_length
                    ),
                    line: line_number,
                    column: self.line_length + 1,
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "line_length".to_string(),
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

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // get_rule_config_value now automatically tries both underscore and kebab-case variants
        let line_length = crate::config::get_rule_config_value::<usize>(config, "MD013", "line_length")
            .unwrap_or(80);

        let code_blocks = crate::config::get_rule_config_value::<bool>(config, "MD013", "code_blocks")
            .unwrap_or(true);

        let tables = crate::config::get_rule_config_value::<bool>(config, "MD013", "tables")
            .unwrap_or(false);

        let headings = crate::config::get_rule_config_value::<bool>(config, "MD013", "headings")
            .unwrap_or(true);

        let strict = crate::config::get_rule_config_value::<bool>(config, "MD013", "strict")
            .unwrap_or(false);

        Box::new(MD013LineLength::new(
            line_length,
            code_blocks,
            tables,
            headings,
            strict,
        ))
    }
}

impl DocumentStructureExtensions for MD013LineLength {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule always applies unless content is empty
        !ctx.content.is_empty()
    }
}
