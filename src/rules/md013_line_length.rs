/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_excess_range;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref URL_PATTERN: Regex = Regex::new(r"^https?://\S+$").unwrap();
    static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^!\[.*?\]\[.*?\]$" ).unwrap();
    static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\[.*?\]:\s*https?://\S+$").unwrap();

    // Sentence splitting patterns
    static ref SENTENCE_END: Regex = Regex::new(r"[.!?]\s+[A-Z]").unwrap();
    static ref ABBREVIATION: Regex = Regex::new(r"\b(?:Mr|Mrs|Ms|Dr|Prof|Sr|Jr|vs|etc|i\.e|e\.g|Inc|Corp|Ltd|Co|St|Ave|Blvd|Rd|Ph\.D|M\.D|B\.A|M\.A|Ph\.D|U\.S|U\.K|U\.N|N\.Y|L\.A|D\.C)\.\s+[A-Z]").unwrap();
    static ref DECIMAL_NUMBER: Regex = Regex::new(r"\d+\.\s*\d+").unwrap();
    static ref LIST_ITEM: Regex = Regex::new(r"^\s*\d+\.\s+").unwrap();

    // Link detection patterns
    static ref INLINE_LINK: Regex = Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap();
    static ref REFERENCE_LINK: Regex = Regex::new(r"\[([^\]]*)\]\[([^\]]*)\]").unwrap();
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

        // Quick check for common patterns before expensive regex
        let trimmed = line.trim();

        // Only skip if the entire line is a URL (quick check first)
        if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && URL_PATTERN.is_match(trimmed)
        {
            return true;
        }

        // Only skip if the entire line is an image reference (quick check first)
        if trimmed.starts_with("![")
            && trimmed.ends_with(']')
            && IMAGE_REF_PATTERN.is_match(trimmed)
        {
            return true;
        }

        // Only skip if the entire line is a link reference (quick check first)
        if trimmed.starts_with('[') && trimmed.contains("]:") && LINK_REF_PATTERN.is_match(trimmed)
        {
            return true;
        }

        // Code blocks with long strings (only check if in code block)
        if structure.is_in_code_block(current_line + 1)
            && !trimmed.is_empty()
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

        // Early return if no lines could possibly exceed the limit
        if lines.iter().all(|line| line.len() <= self.line_length) {
            return Ok(Vec::new());
        }

        // Pre-compute LineIndex for efficient byte range calculations
        let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());

        // Create a quick lookup set for heading lines
        let heading_lines_set: std::collections::HashSet<usize> =
            structure.heading_lines.iter().cloned().collect();

        // Pre-compute table lines for efficiency instead of calling is_in_table for each line
        let table_lines_set: std::collections::HashSet<usize> = if self.tables {
            let mut table_lines = std::collections::HashSet::new();
            let mut in_table = false;
            for (i, line) in lines.iter().enumerate() {
                let line_number = i + 1;
                if line.contains('|') && !structure.is_in_code_block(line_number) {
                    in_table = true;
                    table_lines.insert(line_number);
                } else if in_table && line.trim().is_empty() {
                    in_table = false;
                } else if in_table {
                    table_lines.insert(line_number);
                }
            }
            table_lines
        } else {
            std::collections::HashSet::new()
        };

        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;
            let effective_length = line.chars().count();

            // Skip short lines immediately
            if effective_length <= self.line_length {
                continue;
            }

            // Skip various block types efficiently
            if !self.strict {
                // Skip setext heading underlines
                if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                    continue;
                }

                // Skip block elements according to config flags (optimized checks)
                if (self.headings && heading_lines_set.contains(&line_number))
                    || (!self.code_blocks && structure.is_in_code_block(line_number))
                    || (self.tables && table_lines_set.contains(&line_number))
                    || structure.is_in_blockquote(line_number)
                    || structure.is_in_html_block(line_number)
                {
                    continue;
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, &lines, line_num, structure) {
                    continue;
                }
            }

            // Generate simplified fix (avoid expensive sentence splitting for now)
            let fix = if !self.should_skip_line_for_fix(line, line_num, structure) {
                // First try trimming trailing whitespace
                let trimmed = line.trim_end();
                if trimmed.len() <= self.line_length && trimmed != *line {
                    let line_start = line_index.line_col_to_byte_range(line_number, 1).start;
                    let line_end = if line_number < lines.len() {
                        line_index.line_col_to_byte_range(line_number + 1, 1).start - 1
                    } else {
                        content.len()
                    };
                    Some(crate::rule::Fix {
                        range: line_start..line_end,
                        replacement: trimmed.to_string(),
                    })
                } else {
                    None // Skip expensive sentence splitting for performance
                }
            } else {
                None
            };

            let message = if let Some(ref _fix_obj) = fix {
                format!(
                    "Line length {} exceeds {} characters (can trim whitespace)",
                    effective_length, self.line_length
                )
            } else {
                format!(
                    "Line length {} exceeds {} characters",
                    effective_length, self.line_length
                )
            };

            // Calculate precise character range for the excess portion
            let (start_line, start_col, end_line, end_col) =
                calculate_excess_range(line_number, line, self.line_length);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix,
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| {
                w.fix
                    .as_ref()
                    .map(|f| (f.range.start, f.range.end, &f.replacement))
            })
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
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
        let line_length =
            crate::config::get_rule_config_value::<usize>(config, "MD013", "line_length")
                .unwrap_or(80);

        let code_blocks =
            crate::config::get_rule_config_value::<bool>(config, "MD013", "code_blocks")
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

impl MD013LineLength {
    /// Check if a line should be skipped for fixing
    fn should_skip_line_for_fix(
        &self,
        line: &str,
        line_num: usize,
        structure: &DocumentStructure,
    ) -> bool {
        let line_number = line_num + 1; // 1-based

        // Skip code blocks
        if structure.is_in_code_block(line_number) {
            return true;
        }

        // Skip HTML blocks
        if structure.is_in_html_block(line_number) {
            return true;
        }

        // Skip tables (they have complex formatting)
        if Self::is_in_table(&[line], 0) {
            return true;
        }

        // Skip lines that are only URLs (can't be wrapped)
        if line.trim().starts_with("http://") || line.trim().starts_with("https://") {
            return true;
        }

        // Skip setext heading underlines
        if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
            return true;
        }

        false
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
