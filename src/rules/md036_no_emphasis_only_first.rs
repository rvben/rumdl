//!
//! Rule MD036: No emphasis used as a heading
//!
//! See [docs/md036.md](../../docs/md036.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::range_utils::LineIndex;
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
}

/// Rule MD036: Emphasis used instead of a heading
#[derive(Clone)]
pub struct MD036NoEmphasisAsHeading;

impl MD036NoEmphasisAsHeading {
    fn is_entire_line_emphasized(
        line: &str,
        content: &str,
        line_num: usize,
    ) -> Option<(usize, String)> {
        let line = line.trim();

        // Fast path for empty lines and lines that don't contain emphasis markers
        if line.is_empty() || (!line.contains('*') && !line.contains('_')) {
            return None;
        }

        // Skip if line is already a heading (but not a heading with emphasis)
        if HEADING_MARKER.is_match(line) && !HEADING_WITH_EMPHASIS.is_match(line) {
            return None;
        }

        // Skip if line is in a list, blockquote, or code block
        if LIST_MARKER.is_match(line)
            || BLOCKQUOTE_MARKER.is_match(line)
            || HeadingUtils::is_in_code_block(content, line_num)
        {
            return None;
        }

        // Check specific patterns directly without additional requirements
        // Check for *emphasis* pattern (entire line)
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().to_string()));
        }

        // Check for _emphasis_ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().to_string()));
        }

        // Check for **strong** pattern (entire line)
        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().to_string()));
        }

        // Check for __strong__ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().to_string()));
        }

        None
    }

    fn get_heading_for_emphasis(level: usize, text: &str) -> String {
        let prefix = "#".repeat(level);
        // Split long text into multiple lines if needed
        let text = text.trim();
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

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        for (i, line) in content.lines().enumerate() {
            // Skip obvious non-matches quickly
            if line.trim().is_empty() || (!line.contains('*') && !line.contains('_')) {
                continue;
            }

            if let Some((level, text)) = Self::is_entire_line_emphasized(line, content, i) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: i + 1,
                    column: 1,
                    message: format!("Emphasis used instead of a heading: '{}'", text),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(i + 1, 1),
                        replacement: Self::get_heading_for_emphasis(level, &text),
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

        for i in 0..lines.len() {
            let line = lines[i];
            if let Some((level, text)) = Self::is_entire_line_emphasized(line, content, i) {
                result.push_str(&Self::get_heading_for_emphasis(level, &text));
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

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD036NoEmphasisAsHeading)
    }
}
