use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
use lazy_static::lazy_static;
use regex::Regex;
use fancy_regex::Regex as FancyRegex;

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
    static ref DUPLICATE_TEXT: FancyRegex = FancyRegex::new(r"(\b\w+\b)(?:\s+\1\b)+").unwrap();
}

#[derive(Debug, Default)]
pub struct MD017NoEmphasisAsHeading;

impl MD017NoEmphasisAsHeading {
    fn is_heading_with_emphasis(line: &str, content: &str, line_num: usize) -> Option<(String, String)> {
        let line = line.trim();

        // Fast path for empty lines and lines that don't contain emphasis markers
        if line.is_empty() || (!line.contains('*') && !line.contains('_')) {
            return None;
        }

        // Skip if line is not a heading or doesn't contain emphasis
        if !HEADING_WITH_EMPHASIS.is_match(line) {
            return None;
        }

        // Skip if line is in a list, blockquote, or code block
        if LIST_MARKER.is_match(line) || BLOCKQUOTE_MARKER.is_match(line) || 
           HeadingUtils::is_in_code_block(content, line_num) {
            return None;
        }

        // Extract heading level and text
        let mut parts = line.splitn(2, ' ');
        let heading_marker = parts.next().unwrap_or("");
        let text = parts.next().unwrap_or("").trim();

        // Check for emphasis patterns and extract text
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(text) {
            return Some((heading_marker.to_string(), caps.get(1).unwrap().as_str().trim().to_string()));
        }

        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(text) {
            return Some((heading_marker.to_string(), caps.get(1).unwrap().as_str().trim().to_string()));
        }

        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(text) {
            return Some((heading_marker.to_string(), caps.get(1).unwrap().as_str().trim().to_string()));
        }

        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(text) {
            return Some((heading_marker.to_string(), caps.get(1).unwrap().as_str().trim().to_string()));
        }

        None
    }

    fn get_clean_heading(marker: &str, text: &str) -> String {
        // First remove any duplicate text
        let clean_text = if let Ok(Some(cap)) = DUPLICATE_TEXT.captures(text) {
            cap.get(1).map_or(text, |m| m.as_str())
        } else {
            text
        };

        // Then format as a proper heading
        format!("{} {}", marker, clean_text.trim())
    }
}

impl Rule for MD017NoEmphasisAsHeading {
    fn name(&self) -> &'static str {
        "MD017"
    }

    fn description(&self) -> &'static str {
        "Double emphasis should not be used as a heading"
    }

    fn check(&self, content: &str) -> LintResult {
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

            if let Some((marker, text)) = Self::is_heading_with_emphasis(line, content, i) {
                warnings.push(LintWarning {
                    line: i + 1,
                    column: 1,
                    message: format!("Double emphasis should not be used as a heading: '{}'", text),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(i + 1, 1),
                        replacement: Self::get_clean_heading(&marker, &text),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fast path for empty content or content without emphasis markers
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(content.to_string());
        }

        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();

        for i in 0..lines.len() {
            let line = lines[i];
            if let Some((marker, text)) = Self::is_heading_with_emphasis(line, content, i) {
                result.push_str(&Self::get_clean_heading(&marker, &text));
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
}
