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
    // Match ATX headings (with or without closing hashes)
    static ref ATX_HEADING_RE: Regex = Regex::new(r"^(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Match closed ATX headings specifically
    static ref CLOSED_ATX_HEADING_RE: Regex = Regex::new(r"^(#{1,6})(\s+)(.+?)(\s+#{1,6})$").unwrap();

    // Match indented headings with up to 3 spaces (these are valid headings in Markdown)
    static ref INDENTED_HEADING_RE: Regex = Regex::new(r"^( {1,3})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Match deeply indented headings (4+ spaces) - these are considered code blocks in Markdown
    static ref DEEPLY_INDENTED_HEADING_RE: Regex = Regex::new(r"^(\s{4,})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Pattern for setext heading underlines (= or -)
    static ref SETEXT_UNDERLINE_RE: Regex = Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();

    // Regex cache for punctuation patterns
    static ref PUNCTUATION_REGEX_CACHE: RwLock<HashMap<String, Regex>> = RwLock::new(HashMap::new());

    // Optimized regex patterns for fix operations
    // Only match ATX headings with 0-3 spaces indentation (4+ spaces are code blocks)
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

    // Extract the heading text from an ATX heading
    #[inline]
    fn extract_atx_heading_text(&self, line: &str) -> Option<String> {
        // Check for indented headings first (1-3 spaces)
        if let Some(captures) = INDENTED_HEADING_RE.captures(line) {
            return Some(captures.get(4).unwrap().as_str().to_string());
        } else if let Some(captures) = CLOSED_ATX_HEADING_RE.captures(line) {
            return Some(captures.get(3).unwrap().as_str().to_string());
        } else if let Some(captures) = ATX_HEADING_RE.captures(line) {
            return Some(captures.get(3).unwrap().as_str().to_string());
        }
        None
    }

    // Remove trailing punctuation from text
    fn remove_trailing_punctuation(&self, text: &str, re: &Regex) -> String {
        re.replace_all(text.trim(), "").to_string()
    }

    // Fix an ATX heading by removing trailing punctuation
    fn fix_atx_heading(&self, line: &str, re: &Regex) -> String {
        // Check for indented headings first (1-3 spaces)
        if let Some(captures) = INDENTED_HEADING_RE.captures(line) {
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

        if let Some(captures) = CLOSED_ATX_HEADING_RE.captures(line) {
            // Handle closed ATX heading (# Heading #)
            let hashes = captures.get(1).unwrap().as_str();
            let space = captures.get(2).unwrap().as_str();
            let content = captures.get(3).unwrap().as_str();
            let closing = captures.get(4).unwrap().as_str();

            let fixed_content = self.remove_trailing_punctuation(content, re);
            return format!("{}{}{}{}", hashes, space, fixed_content, closing);
        }

        if let Some(captures) = ATX_HEADING_RE.captures(line) {
            // Handle regular ATX heading (# Heading)
            let hashes = captures.get(1).unwrap().as_str();
            let space = captures.get(2).unwrap().as_str();
            let content = captures.get(3).unwrap().as_str();

            let fixed_content = self.remove_trailing_punctuation(content, re);

            // Preserve any trailing hashes if present
            if let Some(trailing) = captures.get(4) {
                return format!("{}{}{}{}", hashes, space, fixed_content, trailing.as_str());
            }

            return format!("{}{}{}", hashes, space, fixed_content);
        }

        // Fallback if no regex matches
        line.to_string()
    }

    // Fix a setext heading by removing trailing punctuation from the content line
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

    // Check if we're in front matter (between --- markers)
    fn is_in_front_matter(&self, lines: &[&str], line_idx: usize) -> bool {
        if line_idx == 0 || lines.is_empty() {
            return false;
        }

        let mut start_marker = false;

        for (i, &line) in lines.iter().enumerate() {
            if i >= line_idx {
                break;
            }

            let trimmed = line.trim();
            if trimmed == "---" {
                if !start_marker {
                    start_marker = true;
                } else {
                    // Found end marker, we're past front matter
                    return false;
                }
            }
        }

        // If we found a start marker but no end marker, we're still in front matter
        start_marker
    }

    // Check if a line is a deeply indented heading (4+ spaces) which should be treated as code
    fn is_deeply_indented_heading(&self, line: &str) -> bool {
        DEEPLY_INDENTED_HEADING_RE.is_match(line)
    }

    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
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

            // Skip if we're in front matter
            if self.is_in_front_matter(&lines, heading_line - 1) {
                continue;
            }

            // Skip deeply indented headings (they're code blocks)
            if self.is_deeply_indented_heading(lines[heading_line - 1]) {
                continue;
            }

            // Use heading_regions to determine heading type
            let region = structure.heading_regions[idx];

            if region.0 == region.1 {
                // ATX heading (single line)
                if let Some(heading_text) = self.extract_atx_heading_text(lines[heading_line - 1]) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        // Find the trailing punctuation in the ATX heading
                        if let Some(punctuation_match) = re.find(heading_text.trim()) {
                            let line_content = lines[heading_line - 1];

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
                                    replacement: self.fix_atx_heading(lines[heading_line - 1], &re),
                                }),
                            });
                        }
                    }
                }
            } else {
                // Setext heading: check the content line for trailing punctuation
                if self.has_trailing_punctuation(lines[heading_line - 1], &re) {
                    // Find the trailing punctuation in the setext heading
                    if let Some(punctuation_match) = re.find(lines[heading_line - 1].trim()) {
                        let line_content = lines[heading_line - 1];

                        // For setext headings, find the punctuation position in the line
                        let text_start_in_line = line_content
                            .find(lines[heading_line - 1].trim())
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

                        let last_char =
                            lines[heading_line - 1].trim().chars().last().unwrap_or(' ');
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!(
                                "Heading '{
                }' ends with punctuation '{}'",
                                lines[heading_line - 1].trim(),
                                last_char
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: self.get_line_byte_range(content, heading_line),
                                replacement: self.fix_setext_heading(lines[heading_line - 1], &re),
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

        // Quick check for ATX headings (#) or potential setext headings (lines followed by = or -)
        let has_headings = content.contains('#') || {
            let lines: Vec<&str> = content.lines().collect();
            lines.windows(2).any(|pair| {
                !pair[0].trim().is_empty()
                    && (pair[1].trim().chars().all(|c| c == '=' || c == '-')
                        && !pair[1].trim().is_empty())
            })
        };

        if !has_headings {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        if !content.chars().any(|c| self.punctuation.contains(c)) {
            return Ok(Vec::new());
        }

        // Use fallback pattern - try to use shared structure if available
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Fast path: if no headings or punctuation, return unchanged
        if !content.contains('#')
            && !content.lines().any(|line| {
                let lines: Vec<&str> = content.lines().collect();
                let idx = lines.iter().position(|&l| l == line).unwrap_or(0);
                idx + 1 < lines.len() && SETEXT_UNDERLINE_RE.is_match(lines[idx + 1])
            })
        {
            return Ok(content.to_string());
        }

        if !content.chars().any(|c| self.punctuation.contains(c)) {
            return Ok(content.to_string());
        }

        // For default punctuation, use fast regex approach
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

        for (idx, &_line_num) in structure.heading_lines.iter().enumerate() {
            // Use heading_regions to get line indices
            let region = structure.heading_regions[idx];
            let start_line_idx = region.0.saturating_sub(1); // 1-based to 0-based

            // Check bounds
            if start_line_idx >= fixed_lines.len() {
                continue;
            }

            // Fix based on heading type identified by region span
            if region.0 == region.1 {
                // ATX heading (single line)
                let fixed = self.fix_atx_heading(&fixed_lines[start_line_idx], &re);
                fixed_lines[start_line_idx] = fixed;
            } else {
                // Setext heading (content is on start_line_idx)
                let fixed = self.fix_setext_heading(&fixed_lines[start_line_idx], &re);
                fixed_lines[start_line_idx] = fixed;
            }
        }

        // Join lines back with original newline separators
        let mut result = String::new();
        let mut original_lines = content.lines().peekable();
        let fixed_lines_iter = fixed_lines.into_iter();

        for fixed_line in fixed_lines_iter {
            result.push_str(&fixed_line);
            // Append original newline sequence if possible
            if original_lines.next().is_some() && original_lines.peek().is_some() {
                #[cfg(windows)]
                result.push_str("\r\n");
                #[cfg(not(windows))]
                result.push('\n');
            }
        }

        // Handle case where content originally ended with a newline
        if content.ends_with('\n') && !result.ends_with('\n') {
            #[cfg(windows)]
            result.push_str("\r\n");
            #[cfg(not(windows))]
            result.push('\n');
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
            && content.chars().any(|c| self.punctuation.contains(c))
    }
}
