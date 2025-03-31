use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    static ref LIST_MARKER_REGEX: Regex = Regex::new(r"^\s*([*+-])(\s+|$)").unwrap();
}

#[derive(Debug)]
pub struct MD008ULStyle {
    style: char,
    use_consistent: bool,
}

impl Default for MD008ULStyle {
    fn default() -> Self {
        Self {
            style: '-',
            use_consistent: true,
        }
    }
}

impl MD008ULStyle {
    pub fn new(style: char) -> Self {
        Self {
            style,
            use_consistent: false,
        }
    }

    #[inline]
    fn get_list_marker(line: &str) -> Option<char> {
        // Skip empty lines
        if line.trim().is_empty() {
            return None;
        }

        // Use regex for faster matching
        if let Some(caps) = LIST_MARKER_REGEX.captures(line) {
            if let Some(marker_match) = caps.get(1) {
                let marker = marker_match.as_str().chars().next().unwrap();
                return Some(marker);
            }
        }
        None
    }

    fn detect_first_marker_style(&self, content: &str) -> Option<char> {
        // Early return for empty content
        if content.is_empty() {
            return None;
        }
        
        // Pre-compute code blocks to avoid repeated checks
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_blocks = vec![false; lines.len()];
        let mut in_front_matter = vec![false; lines.len()];
        
        // Fast pre-check for list markers
        let has_markers = content.contains('*') || content.contains('+') || content.contains('-');
        if !has_markers {
            return None;
        }
        
        // Pre-compute code blocks
        let mut in_block = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_block = !in_block;
            }
            in_code_blocks[i] = in_block;
        }
        
        // Pre-compute front matter
        let mut in_fm = false;
        let mut is_first_line = true;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            if is_first_line && trimmed == "---" {
                in_fm = true;
                is_first_line = false;
            } else if in_fm && trimmed == "---" {
                in_fm = false;
            }
            
            in_front_matter[i] = in_fm;
            if !is_first_line {
                is_first_line = false;
            }
        }
        
        // Find first marker
        for (i, line) in lines.iter().enumerate() {
            // Skip front matter and code blocks
            if in_front_matter[i] || in_code_blocks[i] {
                continue;
            }

            // Look for a list marker
            if let Some(marker) = Self::get_list_marker(line) {
                return Some(marker);
            }
        }
        None
    }
}

impl Rule for MD008ULStyle {
    fn name(&self) -> &'static str {
        "MD008"
    }

    fn description(&self) -> &'static str {
        "Unordered list style"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early returns for common cases
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        // Quick check if any list markers exist
        if !content.contains('*') && !content.contains('+') && !content.contains('-') {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Pre-compute code blocks and front matter
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_blocks = vec![false; lines.len()];
        let mut in_front_matter = vec![false; lines.len()];
        
        // Pre-compute code blocks
        let mut in_block = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_block = !in_block;
            }
            in_code_blocks[i] = in_block;
        }
        
        // Pre-compute front matter
        let mut in_fm = false;
        let mut is_first_line = true;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            if is_first_line && trimmed == "---" {
                in_fm = true;
                is_first_line = false;
            } else if in_fm && trimmed == "---" {
                in_fm = false;
            }
            
            in_front_matter[i] = in_fm;
            if !is_first_line {
                is_first_line = false;
            }
        }

        // Determine the target style - use the first marker found or fall back to default
        let target_style = if self.use_consistent {
            self.detect_first_marker_style(content)
                .unwrap_or(self.style)
        } else {
            self.style
        };

        // Keep track of markers we've seen to avoid duplicate warnings
        let mut processed_markers = HashSet::new();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip front matter and code blocks
            if in_front_matter[line_num] || in_code_blocks[line_num] {
                continue;
            }

            if let Some(marker) = Self::get_list_marker(line) {
                if marker != target_style {
                    // Create a unique key for this marker to avoid duplicate warnings
                    let marker_key = (line_num, marker);
                    
                    if !processed_markers.contains(&marker_key) {
                        processed_markers.insert(marker_key);
                        
                        let message = if self.use_consistent {
                            format!(
                                "Unordered list item marker '{}' should be '{}' to match first marker style",
                                marker, target_style
                            )
                        } else {
                            format!(
                                "Unordered list item marker '{}' should be '{}' (configured style)",
                                marker, target_style
                            )
                        };

                        warnings.push(LintWarning {
                            message,
                            line: line_num + 1,
                            column: line.find(marker).unwrap_or(0) + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(
                                    line_num + 1,
                                    line.find(marker).unwrap_or(0) + 1,
                                ),
                                replacement: if let Some(r) = LIST_MARKER_REGEX.replace(line, |caps: &regex::Captures| {
                                    let ws_after = caps.get(2).map_or("", |m| m.as_str());
                                    format!("{}{}{}", &line[0..caps.get(1).unwrap().start()], target_style, ws_after)
                                }).to_string().into() {
                                    r
                                } else {
                                    line.replacen(marker, &target_style.to_string(), 1)
                                },
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early returns for common cases
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Quick check if any list markers exist
        if !content.contains('*') && !content.contains('+') && !content.contains('-') {
            return Ok(content.to_string());
        }

        // Pre-compute code blocks and front matter
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_blocks = vec![false; lines.len()];
        let mut in_front_matter = vec![false; lines.len()];
        
        // Pre-compute code blocks
        let mut in_block = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_block = !in_block;
            }
            in_code_blocks[i] = in_block;
        }
        
        // Pre-compute front matter
        let mut in_fm = false;
        let mut is_first_line = true;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            if is_first_line && trimmed == "---" {
                in_fm = true;
                is_first_line = false;
            } else if in_fm && trimmed == "---" {
                in_fm = false;
            }
            
            in_front_matter[i] = in_fm;
            if !is_first_line {
                is_first_line = false;
            }
        }

        // Determine the target style - use the first marker found or fall back to default
        let target_style = if self.use_consistent {
            self.detect_first_marker_style(content)
                .unwrap_or(self.style)
        } else {
            self.style
        };

        // Pre-allocate for better performance
        let mut result = String::with_capacity(content.len());

        for (i, line) in lines.iter().enumerate() {
            // Skip modifying front matter and code blocks
            if in_front_matter[i] || in_code_blocks[i] {
                result.push_str(line);
            } else if let Some(marker) = Self::get_list_marker(line) {
                if marker != target_style {
                    if let Some(replaced) = LIST_MARKER_REGEX.replace(line, |caps: &regex::Captures| {
                        let ws_after = caps.get(2).map_or("", |m| m.as_str());
                        format!("{}{}{}", &line[0..caps.get(1).unwrap().start()], target_style, ws_after)
                    }).to_string().into() {
                        result.push_str(&replaced);
                    } else {
                        result.push_str(&line.replacen(marker, &target_style.to_string(), 1));
                    }
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve the original trailing newline state
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
