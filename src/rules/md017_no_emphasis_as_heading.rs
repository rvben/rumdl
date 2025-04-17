use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::range_utils::LineIndex;
use fancy_regex::Regex as FancyRegex;
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
    static ref DUPLICATE_TEXT: FancyRegex = FancyRegex::new(r"(\b\w+\b)(?:\s+\1\b)+").unwrap();
}

#[derive(Debug, Default)]
pub struct MD017NoEmphasisAsHeading;

impl MD017NoEmphasisAsHeading {
    /// Creates a new instance of the MD017 rule.
    ///
    /// This rule enforces that emphasis markers (`*`, `_`, `**`, `__`) are not used as headings.
    /// According to the CommonMark spec, emphasis markers are for inline text emphasis,
    /// while headings should use either ATX style (`#`) or Setext style (`===`, `---`).
    pub fn new() -> Self {
        Self
    }

    pub fn with_allow_emphasis_headings(_allow_emphasis: bool) -> Self {
        // For now, this parameter is not used in the implementation
        // but we'll keep the constructor for compatibility with tests
        Self
    }

    fn is_in_code_block(&self, content: &str, line_number: usize) -> bool {
        let mut in_code_block = false;
        let mut fence_char = None;

        for (i, line) in content.lines().enumerate() {
            if i >= line_number {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if Some(&trimmed[..3]) == fence_char {
                    in_code_block = false;
                }
            }
        }

        in_code_block
    }
}

impl Rule for MD017NoEmphasisAsHeading {
    fn name(&self) -> &'static str {
        "MD017"
    }

    fn description(&self) -> &'static str {
        "No emphasis used instead of headings"
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

            // Skip lines in code blocks
            if self.is_in_code_block(content, i) || HeadingUtils::is_in_code_block(content, i) {
                continue;
            }

            // Skip lines that are list items or blockquotes
            if LIST_MARKER.is_match(line) || BLOCKQUOTE_MARKER.is_match(line) {
                continue;
            }

            // Check for single-line emphasis patterns
            let trimmed = line.trim();
            let level = if RE_ASTERISK_SINGLE.is_match(trimmed)
                || RE_UNDERSCORE_SINGLE.is_match(trimmed)
            {
                Some(1)
            } else if RE_ASTERISK_DOUBLE.is_match(trimmed) || RE_UNDERSCORE_DOUBLE.is_match(trimmed)
            {
                Some(2)
            } else {
                None
            };

            if let Some(level) = level {
                let message = if level == 1 {
                    "Single emphasis should not be used as a heading"
                } else {
                    "Double emphasis should not be used as a heading"
                };

                if let Some(replacement) = HeadingUtils::convert_emphasis_to_heading(line) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
                        message: message.to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement,
                        }),
                    });
                }
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

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if self.is_in_code_block(content, i) || HeadingUtils::is_in_code_block(content, i) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Skip lines that are list items or blockquotes
            if LIST_MARKER.is_match(line) || BLOCKQUOTE_MARKER.is_match(line) {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            if let Some(replacement) = HeadingUtils::convert_emphasis_to_heading(line) {
                result.push_str(&replacement);
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
