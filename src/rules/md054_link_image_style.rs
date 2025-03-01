use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    // Updated regex patterns that work with Unicode characters
    static ref AUTOLINK_RE: Regex = Regex::new(r"<(https?://[^>]+)>").unwrap();
    static ref INLINE_RE: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref URL_INLINE_RE: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref SHORTCUT_RE: Regex = Regex::new(r"\[([^\]]+)\]").unwrap();
    static ref COLLAPSED_RE: Regex = Regex::new(r"\[([^\]]+)\]\[\]").unwrap();
    static ref FULL_RE: Regex = Regex::new(r"\[([^\]]+)\]\[([^\]]+)\]").unwrap();
    static ref CODE_BLOCK_DELIMITER: Regex = Regex::new(r"^(```|~~~)").unwrap();
}

/// Configuration for link and image styles
#[derive(Debug)]
pub struct MD054LinkImageStyle {
    pub autolink: bool,
    pub collapsed: bool,
    pub full: bool,
    pub inline: bool,
    pub shortcut: bool,
    pub url_inline: bool,
}

impl Default for MD054LinkImageStyle {
    fn default() -> Self {
        Self {
            autolink: true,
            collapsed: true,
            full: true,
            inline: true,
            shortcut: true,
            url_inline: true,
        }
    }
}

impl MD054LinkImageStyle {
    pub fn new(
        autolink: bool,
        collapsed: bool,
        full: bool,
        inline: bool,
        shortcut: bool,
        url_inline: bool,
    ) -> Self {
        Self {
            autolink,
            collapsed,
            full,
            inline,
            shortcut,
            url_inline,
        }
    }

    /// Check if a style is allowed based on configuration
    fn is_style_allowed(&self, style: &str) -> bool {
        match style {
            "autolink" => self.autolink,
            "collapsed" => self.collapsed,
            "full" => self.full,
            "inline" => self.inline,
            "shortcut" => self.shortcut,
            "url_inline" => self.url_inline,
            _ => false,
        }
    }

    /// Determine the style of a link using regex to safely handle Unicode
    fn determine_link_style(&self, line: &str, start_idx: usize) -> Option<(String, usize, usize)> {
        // Safe substring with Unicode awareness
        let safe_substring = if start_idx < line.len() {
            let char_indices: Vec<(usize, char)> = line.char_indices().collect();
            let start_char_idx = char_indices.iter()
                .position(|(byte_idx, _)| *byte_idx >= start_idx)
                .unwrap_or(char_indices.len());
                
            if start_char_idx < char_indices.len() {
                let start_byte_idx = char_indices[start_char_idx].0;
                &line[start_byte_idx..]
            } else {
                ""
            }
        } else {
            ""
        };
        
        if safe_substring.is_empty() {
            return None;
        }

        // Autolink: <https://example.com>
        if let Some(cap) = AUTOLINK_RE.captures(safe_substring) {
            let match_text = cap.get(0).unwrap().as_str();
            let match_start = line.find(match_text).unwrap_or(0) + start_idx;
            let match_end = match_start + match_text.len();
            return Some(("autolink".to_string(), match_start, match_end));
        }

        // Inline: [text](url)
        if let Some(cap) = INLINE_RE.captures(safe_substring) {
            let match_text = cap.get(0).unwrap().as_str();
            let match_start = line.find(match_text).unwrap_or(0) + start_idx;
            let match_end = match_start + match_text.len();
            
            // Check if it's a URL inline (where text and URL are the same)
            let text = cap.get(1).unwrap().as_str();
            let url = cap.get(2).unwrap().as_str();
            if text == url {
                return Some(("url_inline".to_string(), match_start, match_end));
            }
            
            return Some(("inline".to_string(), match_start, match_end));
        }

        // Collapsed: [text][]
        if let Some(cap) = COLLAPSED_RE.captures(safe_substring) {
            let match_text = cap.get(0).unwrap().as_str();
            let match_start = line.find(match_text).unwrap_or(0) + start_idx;
            let match_end = match_start + match_text.len();
            return Some(("collapsed".to_string(), match_start, match_end));
        }

        // Full: [text][reference]
        if let Some(cap) = FULL_RE.captures(safe_substring) {
            let match_text = cap.get(0).unwrap().as_str();
            let match_start = line.find(match_text).unwrap_or(0) + start_idx;
            let match_end = match_start + match_text.len();
            return Some(("full".to_string(), match_start, match_end));
        }

        // Shortcut: [text]
        if let Some(cap) = SHORTCUT_RE.captures(safe_substring) {
            let match_text = cap.get(0).unwrap().as_str();
            let match_start = line.find(match_text).unwrap_or(0) + start_idx;
            let match_end = match_start + match_text.len();
            
            // Check that it's not followed by an opening parenthesis or square bracket
            // Use a safe approach to check the next character
            let next_char = line[match_end..].chars().next();
            if next_char.is_none() || (next_char.unwrap() != '(' && next_char.unwrap() != '[') {
                return Some(("shortcut".to_string(), match_start, match_end));
            }
        }

        None
    }

    /// Check if a position is inside a code block or code span
    fn is_in_code(&self, lines: &[&str], line_num: usize, col: usize) -> bool {
        // Check if in code block
        let mut in_code_block = false;
        let mut code_fence = "";

        for (i, line) in lines.iter().enumerate().take(line_num + 1) {
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    in_code_block = true;
                    code_fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                } else if trimmed.starts_with(code_fence) {
                    in_code_block = false;
                }
            }

            if i == line_num && in_code_block {
                return true;
            }
        }

        // Check if in inline code span
        if line_num < lines.len() {
            let line = lines[line_num];
            let mut in_code_span = false;
            let mut last_backtick_pos = 0;

            let char_indices: Vec<(usize, char)> = line.char_indices().collect();
            for (i, (byte_pos, c)) in char_indices.iter().enumerate() {
                if *c == '`' {
                    if !in_code_span {
                        in_code_span = true;
                        last_backtick_pos = i;
                    } else {
                        in_code_span = false;
                    }
                }

                if *byte_pos == col && in_code_span && i > last_backtick_pos {
                    return true;
                }
            }
        }

        false
    }
}

impl Rule for MD054LinkImageStyle {
    fn name(&self) -> &'static str {
        "MD054"
    }

    fn description(&self) -> &'static str {
        "Link and image style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut found_styles = HashSet::new();
        let lines: Vec<&str> = content.lines().collect();

        // First pass: determine which styles are used
        for (line_num, line) in lines.iter().enumerate() {
            let mut start_idx = 0;
            
            // Use a safer approach to iterate through the line
            while start_idx < line.len() {
                if let Some((style, match_start, end_idx)) = self.determine_link_style(line, start_idx) {
                    if !self.is_in_code(&lines, line_num, match_start) {
                        found_styles.insert(style);
                    }
                    // Ensure we're making progress to avoid infinite loops
                    if end_idx > start_idx {
                        start_idx = end_idx;
                    } else {
                        start_idx += 1;
                    }
                } else {
                    // Make sure we advance by at least one character
                    let next_char_width = line[start_idx..].chars().next().map_or(1, |c| c.len_utf8());
                    start_idx += next_char_width;
                }
            }
        }

        // Determine which styles are allowed based on configuration and found styles
        let allowed_styles: Vec<String> = found_styles
            .iter()
            .filter(|style| self.is_style_allowed(style))
            .cloned()
            .collect();

        // If all found styles are allowed, no warnings
        if allowed_styles.len() == found_styles.len() {
            return Ok(warnings);
        }

        // Second pass: flag links that don't use the allowed styles
        for (line_num, line) in lines.iter().enumerate() {
            let mut start_idx = 0;
            
            // Use a safer approach to iterate through the line
            while start_idx < line.len() {
                if let Some((style, match_start, match_end)) = self.determine_link_style(line, start_idx) {
                    if !self.is_in_code(&lines, line_num, match_start) && !allowed_styles.contains(&style) {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: match_start + 1,
                            message: format!("Link/image style '{}' is not consistent with document", style),
                            fix: None, // Automatic fixing is complex, so we leave it manual for now
                        });
                    }
                    // Ensure we're making progress to avoid infinite loops
                    if match_end > start_idx {
                        start_idx = match_end;
                    } else {
                        start_idx += 1;
                    }
                } else {
                    // Make sure we advance by at least one character
                    let next_char_width = line[start_idx..].chars().next().map_or(1, |c| c.len_utf8());
                    start_idx += next_char_width;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, _content: &str) -> Result<String, LintError> {
        // Automatic fixing for link styles is complex and could break content
        // For now, we'll return the original content with a message
        Err(LintError::FixFailed(
            "Automatic fixing of link styles is not implemented. Please fix manually.".to_string(),
        ))
    }
}