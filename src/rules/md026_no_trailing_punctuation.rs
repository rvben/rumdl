/// Rule MD026: No trailing punctuation in headings
///
/// See [docs/md026.md](../../docs/md026.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::RwLock;

lazy_static! {
    // Optimized single regex for all ATX heading types (normal, closed, indented 1-3 spaces)
    static ref ATX_HEADING_UNIFIED: Regex = Regex::new(r"^( {0,3})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Fast check patterns for early returns
    static ref QUICK_HEADING_CHECK: Regex = Regex::new(r"^( {0,3}#|\s*[=\-]+\s*$)").unwrap();
    static ref QUICK_PUNCTUATION_CHECK: Regex = Regex::new(r"[.,;:!?]").unwrap();

    // Deeply indented headings (4+ spaces) - these are code blocks
    static ref DEEPLY_INDENTED_HEADING_RE: Regex = Regex::new(r"^(\s{4,})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Pattern for setext heading underlines (= or -)
    static ref SETEXT_UNDERLINE_RE: Regex = Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();

    // Regex cache for punctuation patterns
    static ref PUNCTUATION_REGEX_CACHE: RwLock<HashMap<String, Regex>> = RwLock::new(HashMap::new());

    // Optimized regex patterns for fix operations with multiline flag
    static ref FAST_ATX_PUNCTUATION_RE: Regex = Regex::new(r"(?m)^( {0,3}#{1,6}\s+.+?)[.,;:!?]+(\s*(?:#{1,6})?)$").unwrap();
    static ref FAST_SETEXT_PUNCTUATION_RE: Regex = Regex::new(r"(?m)^(.+?)[.,;:!?]+(\s*)$").unwrap();
}

/// Rule MD026: Trailing punctuation in heading
#[derive(Clone)]
pub struct MD026NoTrailingPunctuation {
    punctuation: String,
}

impl Default for MD026NoTrailingPunctuation {
    fn default() -> Self {
        Self {
            punctuation: ".,;:!?".to_string(),
        }
    }
}

impl MD026NoTrailingPunctuation {
    pub fn new(punctuation: Option<String>) -> Self {
        Self {
            punctuation: punctuation.unwrap_or_else(|| ".,;:!?".to_string()),
        }
    }

    #[inline]
    fn get_punctuation_regex(&self) -> Result<Regex, regex::Error> {
        // Check cache first
        {
            let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
            if let Some(cached_regex) = cache.get(&self.punctuation) {
                return Ok(cached_regex.clone());
            }
        }

        // Compile and cache the regex
        let pattern = format!(r"([{}]+)$", regex::escape(&self.punctuation));
        let regex = Regex::new(&pattern)?;

        {
            let mut cache = PUNCTUATION_REGEX_CACHE.write().unwrap();
            cache.insert(self.punctuation.clone(), regex.clone());
        }

        Ok(regex)
    }

    #[inline]
    fn has_trailing_punctuation(&self, text: &str, re: &Regex) -> bool {
        re.is_match(text.trim())
    }

    #[inline]
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> Range<usize> {
        let mut start_pos = 0;

        for (idx, line) in content.lines().enumerate() {
            if idx + 1 == line_num {
                return Range {
                    start: start_pos,
                    end: start_pos + line.len(),
                };
            }
            // +1 for the newline character
            start_pos += line.len() + 1;
        }

        Range {
            start: content.len(),
            end: content.len(),
        }
    }

    // Optimized heading text extraction using unified regex
    #[inline]
    fn extract_atx_heading_text(&self, line: &str) -> Option<String> {
        if let Some(captures) = ATX_HEADING_UNIFIED.captures(line) {
            return Some(captures.get(4).unwrap().as_str().to_string());
        }
        None
    }

    // Remove trailing punctuation from text
    #[inline]
    fn remove_trailing_punctuation(&self, text: &str, re: &Regex) -> String {
        re.replace_all(text.trim(), "").to_string()
    }

    // Optimized ATX heading fix using unified regex
    #[inline]
    fn fix_atx_heading(&self, line: &str, re: &Regex) -> String {
        if let Some(captures) = ATX_HEADING_UNIFIED.captures(line) {
            let indentation = captures.get(1).unwrap().as_str();
            let hashes = captures.get(2).unwrap().as_str();
            let space = captures.get(3).unwrap().as_str();
            let content = captures.get(4).unwrap().as_str();

            let fixed_content = self.remove_trailing_punctuation(content, re);

            // Preserve any trailing hashes if present
            if let Some(trailing) = captures.get(5) {
                return format!(
                    "{}{}{}{}{}",
                    indentation,
                    hashes,
                    space,
                    fixed_content,
                    trailing.as_str()
                );
            }

            return format!("{}{}{}{}", indentation, hashes, space, fixed_content);
        }

        // Fallback if no regex matches
        line.to_string()
    }

    // Fix a setext heading by removing trailing punctuation from the content line
    #[inline]
    fn fix_setext_heading(&self, content_line: &str, re: &Regex) -> String {
        let trimmed = content_line.trim_end();
        let mut whitespace = "";

        // Preserve trailing whitespace
        if content_line.len() > trimmed.len() {
            whitespace = &content_line[trimmed.len()..];
        }

        // Remove punctuation and preserve whitespace
        format!(
            "{}{}",
            self.remove_trailing_punctuation(trimmed, re),
            whitespace
        )
    }

    // Optimized front matter detection
    #[inline]
    fn is_in_front_matter(&self, lines: &[&str], line_index: usize) -> bool {
        if lines.is_empty() {
            return false;
        }

        // Quick check: if first line is not front matter start, no front matter exists
        if !lines[0].trim().starts_with("---") {
            return false;
        }

        // Find front matter bounds
        let mut in_front_matter = false;
        let mut front_matter_end = 0;

        for (i, &line) in lines.iter().enumerate() {
            if line.trim() == "---" {
                if i == 0 {
                    in_front_matter = true;
                } else if in_front_matter {
                    front_matter_end = i;
                    break;
                }
            }
        }

        line_index < front_matter_end
    }

    // Fast check for deeply indented headings (4+ spaces = code block)
    #[inline]
    fn is_deeply_indented_heading(&self, line: &str) -> bool {
        DEEPLY_INDENTED_HEADING_RE.is_match(line)
    }

    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return optimizations
        if content.is_empty() || structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        if !QUICK_PUNCTUATION_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut warnings = Vec::new();

        let re = match self.get_punctuation_regex() {
            Ok(regex) => regex,
            Err(_) => return Ok(warnings),
        };

        for (idx, &heading_line) in structure.heading_lines.iter().enumerate() {
            // Check bounds
            if heading_line == 0 || heading_line > lines.len() {
                continue;
            }

            let line_content = lines[heading_line - 1];

            // Skip if we're in front matter
            if self.is_in_front_matter(&lines, heading_line - 1) {
                continue;
            }

            // Skip deeply indented headings (they're code blocks)
            if self.is_deeply_indented_heading(line_content) {
                continue;
            }

            // Use heading_regions to determine heading type
            let region = structure.heading_regions[idx];

            if region.0 == region.1 {
                // ATX heading (single line)
                if let Some(heading_text) = self.extract_atx_heading_text(line_content) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        // Find the trailing punctuation in the ATX heading
                        if let Some(punctuation_match) = re.find(heading_text.trim()) {
                            // For ATX headings, find the punctuation position in the line
                            let heading_start_in_line =
                                line_content.find(&heading_text).unwrap_or(0);
                            let punctuation_start_in_line =
                                heading_start_in_line + punctuation_match.start();
                            let punctuation_len = punctuation_match.len();

                            let (start_line, start_col, end_line, end_col) = calculate_match_range(
                                heading_line,
                                line_content,
                                punctuation_start_in_line,
                                punctuation_len,
                            );

                            let last_char = heading_text.trim().chars().last().unwrap_or(' ');
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                message: format!(
                                    "Heading '{}' ends with punctuation '{}'",
                                    heading_text.trim(),
                                    last_char
                                ),
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: self.get_line_byte_range(content, heading_line),
                                    replacement: self.fix_atx_heading(line_content, &re),
                                }),
                            });
                        }
                    }
                }
            } else {
                // Setext heading: check the content line for trailing punctuation
                if self.has_trailing_punctuation(line_content, &re) {
                    // Find the trailing punctuation in the setext heading
                    if let Some(punctuation_match) = re.find(line_content.trim()) {
                        // For setext headings, find the punctuation position in the line
                        let text_start_in_line = line_content
                            .find(line_content.trim())
                            .unwrap_or(0);
                        let punctuation_start_in_line =
                            text_start_in_line + punctuation_match.start();
                        let punctuation_len = punctuation_match.len();

                        let (start_line, start_col, end_line, end_col) = calculate_match_range(
                            heading_line,
                            line_content,
                            punctuation_start_in_line,
                            punctuation_len,
                        );

                        let last_char = line_content.trim().chars().last().unwrap_or(' ');
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!(
                                "Heading '{}' ends with punctuation '{}'",
                                line_content.trim(),
                                last_char
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: self.get_line_byte_range(content, heading_line),
                                replacement: self.fix_setext_heading(line_content, &re),
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }
}

impl Rule for MD026NoTrailingPunctuation {
    fn name(&self) -> &'static str {
        "MD026"
    }

    fn description(&self) -> &'static str {
        "Trailing punctuation in heading"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        if !QUICK_PUNCTUATION_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        // Check if we have potential headings (ATX # or setext underlines)
        let has_headings = content.lines().any(|line|
            line.trim_start().starts_with('#') ||
            SETEXT_UNDERLINE_RE.is_match(line)
        );

        if !has_headings {
            return Ok(Vec::new());
        }

        // Use shared structure if available, otherwise create one
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Fast path optimizations
        if content.is_empty() {
            return Ok(content.to_string());
        }

        // Quick check for punctuation
        if !QUICK_PUNCTUATION_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        // Check if we have potential headings (ATX # or setext underlines)
        let has_headings = content.lines().any(|line|
            line.trim_start().starts_with('#') ||
            SETEXT_UNDERLINE_RE.is_match(line)
        );

        if !has_headings {
            return Ok(content.to_string());
        }

        // For default punctuation, use optimized fast regex approach
        if self.punctuation == ".,;:!?" {
            let mut result = content.to_string();

            // Fix ATX headings with fast regex
            result = FAST_ATX_PUNCTUATION_RE
                .replace_all(&result, "$1$2")
                .to_string();

            // Fix setext headings - need to be more careful here
            let lines: Vec<&str> = result.lines().collect();
            let mut fixed_lines = Vec::with_capacity(lines.len());

            for (i, &line) in lines.iter().enumerate() {
                if i + 1 < lines.len() && SETEXT_UNDERLINE_RE.is_match(lines[i + 1]) {
                    // This is a setext heading content line
                    fixed_lines.push(FAST_SETEXT_PUNCTUATION_RE.replace(line, "$1$2").to_string());
                } else {
                    fixed_lines.push(line.to_string());
                }
            }

            // Preserve original line endings
            let mut final_result = String::with_capacity(result.len());
            for (i, line) in fixed_lines.iter().enumerate() {
                final_result.push_str(line);
                if i < fixed_lines.len() - 1 || content.ends_with('\n') {
                    final_result.push('\n');
                }
            }

            return Ok(final_result);
        }

        // Fallback for custom punctuation: use document structure approach
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();
        let re = self.get_punctuation_regex().unwrap();

        for (idx, &heading_line) in structure.heading_lines.iter().enumerate() {
            if heading_line == 0 || heading_line > lines.len() {
                continue;
            }

            // Skip if we're in front matter
            if self.is_in_front_matter(&lines, heading_line - 1) {
                continue;
            }

            // Skip deeply indented headings (they're code blocks)
            if self.is_deeply_indented_heading(lines[heading_line - 1]) {
                continue;
            }

            let region = structure.heading_regions[idx];

            if region.0 == region.1 {
                // ATX heading
                if let Some(heading_text) = self.extract_atx_heading_text(lines[heading_line - 1]) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        fixed_lines[heading_line - 1] = self.fix_atx_heading(lines[heading_line - 1], &re);
                    }
                }
            } else {
                // Setext heading
                if self.has_trailing_punctuation(lines[heading_line - 1], &re) {
                    fixed_lines[heading_line - 1] = self.fix_setext_heading(lines[heading_line - 1], &re);
                }
            }
        }

        // Reconstruct content preserving line endings
        let mut result = String::with_capacity(content.len());
        for (i, line) in fixed_lines.iter().enumerate() {
            result.push_str(line);
            if i < fixed_lines.len() - 1 || content.ends_with('\n') {
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "punctuation".to_string(),
            toml::Value::String(self.punctuation.clone()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let punctuation =
            crate::config::get_rule_config_value::<String>(config, "MD026", "punctuation")
                .unwrap_or_else(|| ".,;:!?".to_string());
        Box::new(MD026NoTrailingPunctuation::new(Some(punctuation)))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD026NoTrailingPunctuation {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty()
            && !structure.heading_lines.is_empty()
            && QUICK_PUNCTUATION_CHECK.is_match(content)
    }
}
