use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;

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

    /// Determine the style of a link
    fn determine_link_style(&self, line: &str, start_idx: usize) -> Option<(String, usize, usize)> {
        // Autolink: <https://example.com>
        let autolink_re = Regex::new(r"<(https?://[^>]+)>").unwrap();
        if let Some(cap) = autolink_re.captures(&line[start_idx..]) {
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("autolink".to_string(), match_start, match_end));
        }

        // Inline: [text](url)
        let inline_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
        if let Some(cap) = inline_re.captures(&line[start_idx..]) {
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("inline".to_string(), match_start, match_end));
        }

        // URL inline: [url](url) (where text and URL are the same)
        let url_inline_re = Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
        if let Some(cap) = url_inline_re.captures(&line[start_idx..]) {
            let text = cap.get(1).unwrap().as_str();
            let full_match = cap.get(0).unwrap().as_str();
            // Check if text is same as URL by looking for pattern [text](text)
            if full_match == format!("[{}]({})", text, text) {
                let match_start = start_idx + cap.get(0).unwrap().start();
                let match_end = start_idx + cap.get(0).unwrap().end();
                return Some(("url_inline".to_string(), match_start, match_end));
            }
        }

        // Shortcut: [text]
        let shortcut_re = Regex::new(r"\[([^\]]+)\]").unwrap();
        if let Some(cap) = shortcut_re.captures(&line[start_idx..]) {
            let full_match = cap.get(0).unwrap();
            let match_start = start_idx + full_match.start();
            let match_end = start_idx + full_match.end();
            
            // Check that it's not followed by an opening parenthesis
            // which would make it an inline link, not a shortcut reference
            if match_end >= line.len() || line.chars().nth(match_end) != Some('(') {
                return Some(("shortcut".to_string(), match_start, match_end));
            }
        }

        // Collapsed: [text][]
        let collapsed_re = Regex::new(r"\[([^\]]+)\]\[\]").unwrap();
        if let Some(cap) = collapsed_re.captures(&line[start_idx..]) {
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("collapsed".to_string(), match_start, match_end));
        }

        // Full: [text][reference]
        let full_re = Regex::new(r"\[([^\]]+)\]\[([^\]]+)\]").unwrap();
        if let Some(cap) = full_re.captures(&line[start_idx..]) {
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("full".to_string(), match_start, match_end));
        }

        None
    }

    /// Check if a position is inside a code block or code span
    fn is_in_code(&self, content: &str, line_num: usize, col: usize) -> bool {
        // Check if in code block
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut code_fence = "";

        for (i, line) in lines.iter().enumerate() {
            if i > line_num {
                break;
            }

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

            for (i, c) in line.chars().enumerate() {
                if c == '`' {
                    if !in_code_span {
                        in_code_span = true;
                        last_backtick_pos = i;
                    } else {
                        in_code_span = false;
                    }
                }

                if i == col && in_code_span && i > last_backtick_pos {
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

        // First pass: determine which styles are used
        for (line_num, line) in content.lines().enumerate() {
            let mut start_idx = 0;
            while start_idx < line.len() {
                if let Some((style, _, end_idx)) = self.determine_link_style(line, start_idx) {
                    if !self.is_in_code(content, line_num, start_idx) {
                        found_styles.insert(style);
                    }
                    start_idx = end_idx;
                } else {
                    start_idx += 1;
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
        for (line_num, line) in content.lines().enumerate() {
            let mut start_idx = 0;
            while start_idx < line.len() {
                if let Some((style, match_start, match_end)) = self.determine_link_style(line, start_idx) {
                    if !self.is_in_code(content, line_num, start_idx) && !allowed_styles.contains(&style) {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: match_start + 1,
                            message: format!("Link/image style '{}' is not consistent with document", style),
                            fix: None, // Automatic fixing is complex, so we leave it manual for now
                        });
                    }
                    start_idx = match_end;
                } else {
                    start_idx += 1;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Automatic fixing for link styles is complex and could break content
        // For now, we'll return the original content with a message
        Err(LintError::FixFailed(
            "Automatic fixing of link styles is not implemented. Please fix manually.".to_string(),
        ))
    }
} 