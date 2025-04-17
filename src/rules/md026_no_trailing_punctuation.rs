use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Range;

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
}

#[derive(Debug, Clone)]
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
        let pattern = format!(r"([{}]+)$", regex::escape(&self.punctuation));
        Regex::new(&pattern)
    }

    fn has_trailing_punctuation(&self, text: &str, re: &Regex) -> bool {
        re.is_match(text.trim())
    }

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

    // Detect if a line is a setext heading underline
    fn is_setext_underline(&self, line: &str) -> bool {
        SETEXT_UNDERLINE_RE.is_match(line)
    }

    // Check if we're in front matter (between --- markers)
    fn is_in_front_matter(&self, lines: &[&str], line_idx: usize) -> bool {
        if line_idx == 0 || lines.is_empty() {
            return false;
        }

        let mut start_marker = false;
        let mut end_marker = false;

        // Find front matter markers before this line
        for i in 0..line_idx {
            if i == 0 && lines[i] == "---" {
                start_marker = true;
                continue;
            }

            if start_marker && lines[i] == "---" && i < line_idx {
                end_marker = true;
                break;
            }
        }

        // Check if we're between markers
        start_marker && !end_marker
    }

    // Check if a line is a deeply indented heading (4+ spaces)
    // These are treated as code blocks in Markdown
    fn is_deeply_indented_heading(&self, line: &str) -> bool {
        line.starts_with("    ") && line.trim_start().starts_with('#')
    }

    fn check_with_structure(&self, content: &str, structure: &crate::utils::document_structure::DocumentStructure) -> LintResult {
        if content.is_empty() {
            return Ok(Vec::new());
        }

        let re = match self.get_punctuation_regex() {
            Ok(re) => re,
            Err(e) => {
                return Err(LintError::FixFailed(format!(
                    "Invalid regex pattern: {}",
                    e
                )))
            }
        };

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();

        for (idx, &line_num) in structure.heading_lines.iter().enumerate() {
            let region = structure.heading_regions[idx];
            let heading_line = region.0; // Always the content line for both ATX and setext

            // Skip front matter content
            if self.is_in_front_matter(&lines, heading_line) {
                continue;
            }

            // Skip deeply indented headings (4+ spaces) as they are considered code blocks
            if self.is_deeply_indented_heading(lines[heading_line - 1]) {
                continue;
            }

            // Check if it's a code block, but don't skip lightly indented headings
            if line_index.is_code_block(heading_line + 1) && !INDENTED_HEADING_RE.is_match(lines[heading_line - 1]) {
                continue;
            }

            // For ATX headings
            if region.0 == region.1 {
                if INDENTED_HEADING_RE.is_match(lines[heading_line - 1]) {
                    if let Some(heading_text) = self.extract_atx_heading_text(lines[heading_line - 1]) {
                        if self.has_trailing_punctuation(&heading_text, &re) {
                            let last_char = heading_text.trim().chars().last().unwrap_or(' ');

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: heading_line,
                                column: 1,
                                message: format!(
                                    "Heading '{}' should not end with punctuation '{}'",
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
                } else if ATX_HEADING_RE.is_match(lines[heading_line - 1]) {
                    if let Some(heading_text) = self.extract_atx_heading_text(lines[heading_line - 1]) {
                        if self.has_trailing_punctuation(&heading_text, &re) {
                            let last_char = heading_text.trim().chars().last().unwrap_or(' ');

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: heading_line,
                                column: 1,
                                message: format!(
                                    "Heading '{}' should not end with punctuation '{}'",
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
                    let last_char = lines[heading_line - 1].trim().chars().last().unwrap_or(' ');
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: heading_line,
                        column: 1,
                        message: format!(
                            "Heading '{}' should not end with punctuation '{}'",
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

    fn check(&self, content: &str) -> LintResult {
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        let mut fixed_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let re = self.get_punctuation_regex().unwrap();

        for (idx, &line_num) in structure.heading_lines.iter().enumerate() {
            let region = structure.heading_regions[idx];
            let start = region.0 - 1;
            let _end = region.1 - 1;
            // Only fix if the heading line exists
            if start < lines.len() {
                // Remove trailing punctuation from the heading text
                let line = lines[start];
                // ATX or Setext
                if region.0 == region.1 {
                    // ATX
                    let fixed = self.fix_atx_heading(line, &re);
                    fixed_lines[start] = fixed;
                } else {
                    // Setext (multi-line)
                    let fixed = self.fix_setext_heading(line, &re);
                    fixed_lines[start] = fixed;
                }
            }
        }

        let mut result = fixed_lines.join("\n");
        if ends_with_newline {
            result.push('\n');
        }
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
