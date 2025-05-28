//!
//! Rule MD036: No emphasis used as a heading
//!
//! See [docs/md036.md](../../docs/md036.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::{calculate_emphasis_range, LineIndex};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Optimize regex patterns with compilation once at startup
    static ref RE_ASTERISK_SINGLE: Regex = Regex::new(r"^\s*\*([^*\n]+)\*\s*$").unwrap();
    static ref RE_UNDERSCORE_SINGLE: Regex = Regex::new(r"^\s*_([^_\n]+)_\s*$").unwrap();
    static ref RE_ASTERISK_DOUBLE: Regex = Regex::new(r"^\s*\*\*([^*\n]+)\*\*\s*$").unwrap();
    static ref RE_UNDERSCORE_DOUBLE: Regex = Regex::new(r"^\s*__([^_\n]+)__\s*$").unwrap();
    static ref LIST_MARKER: Regex = Regex::new(r"^\s*(?:[*+-]|\d+\.)\s+").unwrap();
    static ref BLOCKQUOTE_MARKER: Regex = Regex::new(r"^\s*>").unwrap();
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
    static ref HEADING_MARKER: Regex = Regex::new(r"^#+\s").unwrap();
    static ref HEADING_WITH_EMPHASIS: Regex = Regex::new(r"^(#+\s+).*(?:\*\*|\*|__|_)").unwrap();
    // Pattern to match common Table of Contents labels that should not be converted to headings
    static ref TOC_LABEL_PATTERN: Regex = Regex::new(r"^\s*(?:\*\*|\*|__|_)(?:Table of Contents|Contents|TOC|Index)(?:\*\*|\*|__|_)\s*$").unwrap();
}

/// Rule MD036: Emphasis used instead of a heading
#[derive(Clone)]
pub struct MD036NoEmphasisAsHeading {
    /// Punctuation characters to remove from the end of headings when converting from emphasis
    /// Default: ".,;:!?" - removes common trailing punctuation
    /// Set to empty string to preserve all punctuation
    punctuation: String,
}

impl MD036NoEmphasisAsHeading {
    pub fn new(punctuation: String) -> Self {
        Self { punctuation }
    }

    fn is_entire_line_emphasized(
        line: &str,
        doc_structure: &DocumentStructure,
        line_num: usize,
    ) -> Option<(usize, String, usize, usize)> {
        let original_line = line;
        let line = line.trim();

        // Fast path for empty lines and lines that don't contain emphasis markers
        if line.is_empty() || (!line.contains('*') && !line.contains('_')) {
            return None;
        }

        // Skip if line is already a heading (but not a heading with emphasis)
        if HEADING_MARKER.is_match(line) && !HEADING_WITH_EMPHASIS.is_match(line) {
            return None;
        }

        // Skip if line is a Table of Contents label (common legitimate use of bold text)
        if TOC_LABEL_PATTERN.is_match(line) {
            return None;
        }

        // Skip if line is in a list, blockquote, or code block using DocumentStructure
        if LIST_MARKER.is_match(line)
            || BLOCKQUOTE_MARKER.is_match(line)
            || doc_structure.is_in_code_block(line_num + 1)
        // line_num is 0-based, but DocumentStructure expects 1-based
        {
            return None;
        }

        // Check specific patterns directly without additional requirements
        // Check for *emphasis* pattern (entire line)
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(line) {
            let full_match = caps.get(0).unwrap();
            let start_pos = original_line.find(full_match.as_str()).unwrap_or(0);
            let end_pos = start_pos + full_match.len();
            return Some((
                1,
                caps.get(1).unwrap().as_str().to_string(),
                start_pos,
                end_pos,
            ));
        }

        // Check for _emphasis_ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(line) {
            let full_match = caps.get(0).unwrap();
            let start_pos = original_line.find(full_match.as_str()).unwrap_or(0);
            let end_pos = start_pos + full_match.len();
            return Some((
                1,
                caps.get(1).unwrap().as_str().to_string(),
                start_pos,
                end_pos,
            ));
        }

        // Check for **strong** pattern (entire line)
        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(line) {
            let full_match = caps.get(0).unwrap();
            let start_pos = original_line.find(full_match.as_str()).unwrap_or(0);
            let end_pos = start_pos + full_match.len();
            return Some((
                2,
                caps.get(1).unwrap().as_str().to_string(),
                start_pos,
                end_pos,
            ));
        }

        // Check for __strong__ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(line) {
            let full_match = caps.get(0).unwrap();
            let start_pos = original_line.find(full_match.as_str()).unwrap_or(0);
            let end_pos = start_pos + full_match.len();
            return Some((
                2,
                caps.get(1).unwrap().as_str().to_string(),
                start_pos,
                end_pos,
            ));
        }

        None
    }

    fn get_heading_for_emphasis(&self, level: usize, text: &str) -> String {
        let prefix = "#".repeat(level);
        // Remove trailing punctuation based on configuration
        let text = if self.punctuation.is_empty() {
            text.trim()
        } else {
            let chars_to_remove: Vec<char> = self.punctuation.chars().collect();
            text.trim().trim_end_matches(&chars_to_remove[..])
        };

        // Split long text into multiple lines if needed
        if text.len() > 80 {
            let words = text.split_whitespace();
            let mut current_line = String::new();
            let mut result = String::new();
            let mut first_line = true;

            for word in words {
                if current_line.len() + word.len() + 1 > 80 {
                    if first_line {
                        result.push_str(&format!("{} {}\n", prefix, current_line.trim()));
                        first_line = false;
                    } else {
                        result.push_str(&format!("{}\n", current_line.trim()));
                    }
                    current_line = word.to_string();
                } else {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
            }

            if first_line {
                result.push_str(&format!("{} {}", prefix, current_line.trim()));
            } else {
                result.push_str(current_line.trim());
            }
            result
        } else {
            format!("{} {}", prefix, text)
        }
    }
}

impl Rule for MD036NoEmphasisAsHeading {
    fn name(&self) -> &'static str {
        "MD036"
    }

    fn description(&self) -> &'static str {
        "Emphasis should not be used instead of a heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Fast path for empty content or content without emphasis markers
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(Vec::new());
        }

        // Use the optimized document structure approach
        let doc_structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &doc_structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        // Fast path for empty content or content without emphasis markers
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        for (i, line) in content.lines().enumerate() {
            // Skip obvious non-matches quickly
            if line.trim().is_empty() || (!line.contains('*') && !line.contains('_')) {
                continue;
            }

            if let Some((level, text, start_pos, end_pos)) =
                Self::is_entire_line_emphasized(line, doc_structure, i)
            {
                let (start_line, start_col, end_line, end_col) =
                    calculate_emphasis_range(i + 1, line, start_pos, end_pos);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Emphasis used instead of a heading: '{}'", text),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(i + 1, 1),
                        replacement: self.get_heading_for_emphasis(level, &text),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // Fast path for empty content or content without emphasis markers
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');
        let doc_structure = DocumentStructure::new(content);

        for i in 0..lines.len() {
            let line = lines[i];
            if let Some((level, text, _start_pos, _end_pos)) =
                Self::is_entire_line_emphasized(line, &doc_structure, i)
            {
                result.push_str(&self.get_heading_for_emphasis(level, &text));
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            } else if ends_with_newline {
                // Preserve newline at end of file
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
            crate::config::get_rule_config_value::<String>(config, "MD036", "punctuation")
                .unwrap_or_else(|| ".,;:!?".to_string());

        Box::new(MD036NoEmphasisAsHeading::new(punctuation))
    }
}
