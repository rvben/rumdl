use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use crate::utils::document_structure::DocumentStructure;

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
                return format!("{}{}{}{}{}", indentation, hashes, space, fixed_content, trailing.as_str());
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
        format!("{}{}", self.remove_trailing_punctuation(trimmed, re), whitespace)
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

    // Add method to describe the configured punctuation characters
    fn get_punctuation_description(&self) -> String {
        let mut desc = "punctuation".to_string();
        if self.punctuation.len() <= 10 {
            desc = format!("punctuation '{}'", self.punctuation);
        }
        desc
    }
    
    // Count the number of trailing punctuation characters in a string
    fn count_trailing_punctuation(&self, text: &str, re: &Regex) -> usize {
        if let Some(captures) = re.captures(text.trim()) {
            if let Some(m) = captures.get(1) {
                return m.as_str().len();
            }
        }
        0
    }

    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if content is empty or no headings
        if content.is_empty() || structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let re = match self.get_punctuation_regex() {
            Ok(re) => re,
            Err(e) => return Err(LintError::FixFailed(format!("Invalid regex pattern: {}", e))),
        };

        let mut warnings = Vec::new();
        let _line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();
        
        // Process each heading using heading line numbers from the document structure
        for line_num in structure.heading_lines.iter() {
            // Line numbers in the structure are 1-indexed
            let line_idx = line_num - 1;
            
            if line_idx >= lines.len() {
                continue;
            }
            
            let line = lines[line_idx];
            let heading_text = line.trim();
            
            // Skip empty headings
            if heading_text.is_empty() {
                continue;
            }
            
            // Check for ATX headings
            if heading_text.starts_with('#') {
                // Get the heading text (after the # characters)
                let mut heading_text = heading_text.trim_start_matches('#').trim_start();
                
                // Handle closed ATX headings by removing trailing #
                if heading_text.ends_with('#') {
                    heading_text = heading_text.trim_end_matches('#').trim_end();
                }
                
                // Check for punctuation at the end of the heading
                if self.has_trailing_punctuation(heading_text, &re) {
                    let range = self.get_line_byte_range(content, *line_num);
                    let trailing_punct_len = self.count_trailing_punctuation(line, &re);
                    
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_idx + 1,
                        column: 1,
                        message: format!("Heading should not end with {}", 
                                        self.get_punctuation_description()),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: Range { 
                                start: range.end - trailing_punct_len,
                                end: range.end
                            },
                            replacement: self.fix_atx_heading(line, &re),
                        }),
                    });
                }
            } 
            // Check for Setext headings
            else {
                // Only process first line of setext headings (the text, not the underline)
                if line_idx + 1 < lines.len() && self.is_setext_underline(lines[line_idx + 1]) && self.has_trailing_punctuation(line, &re) {
                    let range = self.get_line_byte_range(content, *line_num);
                    let trailing_punct_len = self.count_trailing_punctuation(line, &re);
                    
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_idx + 1,
                        column: 1,
                        message: format!("Heading should not end with {}", 
                                        self.get_punctuation_description()),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: Range { 
                                start: range.end - trailing_punct_len,
                                end: range.end
                            },
                            replacement: self.fix_setext_heading(line, &re),
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
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let re = match self.get_punctuation_regex() {
            Ok(re) => re,
            Err(e) => return Err(LintError::FixFailed(format!("Invalid regex pattern: {}", e))),
        };

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());
        let lines: Vec<&str> = content.lines().collect();
        
        // Track setext heading underline lines to skip them when processing
        let mut setext_underlines = Vec::new();
        
        // First identify all setext underlines to avoid processing them as content
        for (i, line) in lines.iter().enumerate() {
            if i > 0 && self.is_setext_underline(line) {
                setext_underlines.push(i);
            }
        }
        
        // Process each line for headings
        for (line_num, line) in lines.iter().enumerate() {
            // Skip setext underlines - we only want to process the content line
            if setext_underlines.contains(&line_num) {
                continue;
            }
            
            // Skip front matter content
            if self.is_in_front_matter(&lines, line_num) {
                continue;
            }
            
            // Skip deeply indented headings (4+ spaces) as they are considered code blocks
            if self.is_deeply_indented_heading(line) {
                continue;
            }
            
            // Check if it's a code block, but don't skip lightly indented headings
            if line_index.is_code_block(line_num + 1) && !INDENTED_HEADING_RE.is_match(line) {
                continue;
            }
            
            // Process indented ATX headings (1-3 spaces)
            if INDENTED_HEADING_RE.is_match(line) {
                if let Some(heading_text) = self.extract_atx_heading_text(line) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        let last_char = heading_text.trim().chars().last().unwrap_or(' ');
                        
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: 1,
                            message: format!(
                                "Heading '{}' should not end with punctuation '{}'",
                                heading_text.trim(),
                                last_char
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: self.get_line_byte_range(content, line_num + 1),
                                replacement: self.fix_atx_heading(line, &re),
                            }),
                        });
                    }
                }
            }
            // Process regular ATX headings
            else if ATX_HEADING_RE.is_match(line) {
                if let Some(heading_text) = self.extract_atx_heading_text(line) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        let last_char = heading_text.trim().chars().last().unwrap_or(' ');
                        
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: 1,
                            message: format!(
                                "Heading '{}' should not end with punctuation '{}'",
                                heading_text.trim(),
                                last_char
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: self.get_line_byte_range(content, line_num + 1),
                                replacement: self.fix_atx_heading(line, &re),
                            }),
                        });
                    }
                }
            }
            
            // Process setext headings (if the next line is a setext underline)
            else if line_num + 1 < lines.len() && self.is_setext_underline(lines[line_num + 1]) && self.has_trailing_punctuation(line, &re) {
                let last_char = line.trim().chars().last().unwrap_or(' ');
                
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num + 1,
                    column: 1,
                    message: format!(
                        "Heading '{}' should not end with punctuation '{}'",
                        line.trim(),
                        last_char
                    ),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: self.get_line_byte_range(content, line_num + 1),
                        replacement: self.fix_setext_heading(line, &re),
                    }),
                });
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(content.to_string());
        }
        
        let re = match self.get_punctuation_regex() {
            Ok(re) => re,
            Err(e) => return Err(LintError::FixFailed(format!("Invalid regex pattern: {}", e))),
        };
        
        let lines: Vec<&str> = content.lines().collect();
        let mut output_lines = Vec::new();
        
        // Track setext heading underline lines
        let mut setext_underlines = Vec::new();
        
        // First identify all setext underlines
        for (i, line) in lines.iter().enumerate() {
            if i > 0 && self.is_setext_underline(line) {
                setext_underlines.push(i);
            }
        }
        
        // Track if we're in front matter
        let mut in_front_matter = false;
        let mut front_matter_marker_count = 0;
        
        // Process each line
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            
            // Handle front matter markers
            if line.trim() == "---" {
                front_matter_marker_count += 1;
                in_front_matter = front_matter_marker_count == 1; // After second marker, we're out of front matter
                output_lines.push(line.to_string());
                i += 1;
                continue;
            }
            
            // Preserve front matter lines unchanged
            if in_front_matter {
                output_lines.push(line.to_string());
                i += 1;
                continue;
            }
            
            // Skip deeply indented headings - they're treated as code blocks
            if self.is_deeply_indented_heading(line) {
                output_lines.push(line.to_string());
                i += 1;
                continue;
            }
            
            // Process indented ATX headings (1-3 spaces)
            if INDENTED_HEADING_RE.is_match(line) {
                if let Some(heading_text) = self.extract_atx_heading_text(line) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        output_lines.push(self.fix_atx_heading(line, &re));
                    } else {
                        output_lines.push(line.to_string());
                    }
                } else {
                    output_lines.push(line.to_string());
                }
                i += 1;
            }
            // Process regular ATX headings
            else if ATX_HEADING_RE.is_match(line) {
                if let Some(heading_text) = self.extract_atx_heading_text(line) {
                    if self.has_trailing_punctuation(&heading_text, &re) {
                        output_lines.push(self.fix_atx_heading(line, &re));
                    } else {
                        output_lines.push(line.to_string());
                    }
                } else {
                    output_lines.push(line.to_string());
                }
                i += 1;
            }
            // Process setext headings
            else if i + 1 < lines.len() && self.is_setext_underline(lines[i + 1]) {
                if self.has_trailing_punctuation(line, &re) {
                    output_lines.push(self.fix_setext_heading(line, &re));
                } else {
                    output_lines.push(line.to_string());
                }
                
                // Add the underline
                output_lines.push(lines[i + 1].to_string());
                i += 2;
            }
            // Skip handling of setext underlines - they're handled with their content line
            else if setext_underlines.contains(&i) {
                i += 1;
            }
            // Not a heading - preserve as is
            else {
                output_lines.push(line.to_string());
                i += 1;
            }
        }
        
        // Preserve trailing newline if original content had it
        let result = output_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            return Ok(format!("{}\n", result));
        }
        
        Ok(result)
    }
}
