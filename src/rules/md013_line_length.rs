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
                // Generate fix if we can safely modify the line
                let fix = if !self.should_skip_line_for_fix(line, line_num, structure) {
                    // First try trimming trailing whitespace
                    let trimmed = line.trim_end();
                    if trimmed.len() <= self.line_length && trimmed != line {
                        let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());
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
                    } else if let Some((first_part, second_part)) = self.try_split_sentences(line, self.line_length) {
                        // Try sentence splitting
                        let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());
                        let line_start = line_index.line_col_to_byte_range(line_number, 1).start;
                        let line_end = if line_number < lines.len() {
                            line_index.line_col_to_byte_range(line_number + 1, 1).start - 1
                        } else {
                            content.len()
                        };

                        // Preserve indentation from original line
                        let leading_whitespace = line.len() - line.trim_start().len();
                        let indent = &line[..leading_whitespace];

                        let replacement = format!("{}\n{}{}", first_part, indent, second_part);
                        Some(crate::rule::Fix {
                            range: line_start..line_end,
                            replacement,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                let message = if let Some(ref fix_obj) = fix {
                    if fix_obj.replacement.contains('\n') {
                        format!(
                            "Line length {} exceeds {} characters (can split sentences)",
                            effective_length, self.line_length
                        )
                    } else {
                        format!(
                            "Line length {} exceeds {} characters (can trim whitespace)",
                            effective_length, self.line_length
                        )
                    }
                } else {
                    format!(
                        "Line length {} exceeds {} characters",
                        effective_length, self.line_length
                    )
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message,
                    line: line_number,
                    column: self.line_length + 1,
                    severity: Severity::Warning,
                    fix,
                });
            }
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
        let mut fixes: Vec<_> = warnings.iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
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

impl MD013LineLength {
        /// Find sentence boundaries in a line, avoiding false positives
    fn find_sentence_boundaries(&self, line: &str) -> Vec<usize> {
        let mut boundaries = Vec::new();

        // Find all potential sentence endings
        for mat in SENTENCE_END.find_iter(line) {
            // The regex matches "[.!?]\s+[A-Z]", so we want to split after the punctuation and space
            // Find the position right before the capital letter
            let match_text = mat.as_str();
            let punct_and_space_len = match_text.len() - 1; // Everything except the capital letter
            let split_pos = mat.start() + punct_and_space_len;

            // Check if this is a false positive
            let before = &line[..mat.start() + 1];

            // Skip if it's an abbreviation
            if ABBREVIATION.is_match(&line[..mat.end()]) {
                continue;
            }

            // Skip if it's a decimal number
            if DECIMAL_NUMBER.is_match(&line[..mat.end()]) {
                continue;
            }

            // Skip if it's a numbered list item
            if LIST_ITEM.is_match(line) && before.contains('.') && before.matches('.').count() == 1 {
                continue;
            }

            // Skip if we're inside a link or code span
            if self.is_inside_markdown_construct(line, mat.start()) {
                continue;
            }

            boundaries.push(split_pos);
        }

        boundaries
    }

        /// Check if a position is inside a markdown construct (links, code spans, etc.)
    fn is_inside_markdown_construct(&self, line: &str, pos: usize) -> bool {
        let chars: Vec<char> = line.chars().collect();

        // Check for code spans
        let mut in_code = false;
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '`' {
                // Count consecutive backticks
                let mut _backtick_count = 0;
                let _start = i;
                while i < chars.len() && chars[i] == '`' {
                    _backtick_count += 1;
                    i += 1;
                }

                if in_code {
                    // Look for matching closing backticks
                    in_code = false; // Assume we found the closing
                } else {
                    // Opening backticks
                    in_code = true;
                }
            } else {
                if i == pos && in_code {
                    return true;
                }
                i += 1;
            }
        }

                // Check for links - look for complete [text](url) or [text][ref] patterns
        for mat in INLINE_LINK.find_iter(line) {
            if pos >= mat.start() && pos < mat.end() {
                return true;
            }
        }

        for mat in REFERENCE_LINK.find_iter(line) {
            if pos >= mat.start() && pos < mat.end() {
                return true;
            }
        }

        false
    }

    /// Attempt to split a line at sentence boundaries
    fn try_split_sentences(&self, line: &str, max_length: usize) -> Option<(String, String)> {
        let boundaries = self.find_sentence_boundaries(line);

        if boundaries.is_empty() {
            return None;
        }

        // Find the best split point
        for &boundary in &boundaries {
            let first_part = line[..boundary].trim_end();
            let second_part = line[boundary..].trim_start();

            // Check if both parts would be within the limit
            if first_part.len() <= max_length && second_part.len() <= max_length {
                // Ensure the second part starts with a capital letter (sentence)
                if second_part.chars().next().map_or(false, |c| c.is_uppercase()) {
                    return Some((first_part.to_string(), second_part.to_string()));
                }
            }
        }

        None
    }

    /// Check if a line should be skipped for fixing
    fn should_skip_line_for_fix(&self, line: &str, line_num: usize, structure: &DocumentStructure) -> bool {
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
