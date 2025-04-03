use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Updated regex patterns that work with Unicode characters
    static ref AUTOLINK_RE: Regex = Regex::new(r"<([^<>]+)>").unwrap();
    static ref INLINE_RE: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref URL_INLINE_RE: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref SHORTCUT_RE: Regex = Regex::new(r"\[([^\]]+)\]").unwrap();
    static ref COLLAPSED_RE: Regex = Regex::new(r"\[([^\]]+)\]\[\]").unwrap();
    static ref FULL_RE: Regex = Regex::new(r"\[([^\]]+)\]\[([^\]]+)\]").unwrap();
    static ref CODE_BLOCK_DELIMITER: Regex = Regex::new(r"^(```|~~~)").unwrap();
    static ref REFERENCE_DEF_RE: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap();
}

/// Rule MD054: Link and image style should be consistent
///
/// This rule is triggered when different link or image styles are used in the same document.
/// Markdown supports various styles for links and images, and this rule enforces consistency.
///
/// ## Supported Link Styles
///
/// - **Autolink**: `<https://example.com>`
/// - **Inline**: `[link text](https://example.com)`
/// - **URL Inline**: Special case of inline links where the URL itself is also the link text: `[https://example.com](https://example.com)`
/// - **Shortcut**: `[link text]` (requires a reference definition elsewhere in the document)
/// - **Collapsed**: `[link text][]` (requires a reference definition with the same name)
/// - **Full**: `[link text][reference]` (requires a reference definition for the reference)
///
/// ## Configuration Options
///
/// You can configure which link styles are allowed. By default, all styles are allowed:
///
/// ```yaml
/// MD054:
///   autolink: true    # Allow autolink style
///   inline: true      # Allow inline style
///   url_inline: true  # Allow URL inline style
///   shortcut: true    # Allow shortcut style
///   collapsed: true   # Allow collapsed style
///   full: true        # Allow full style
/// ```
///
/// To enforce a specific style, set only that style to `true` and all others to `false`.
///
/// ## Unicode Support
///
/// This rule fully supports Unicode characters in link text and URLs, including:
/// - Combining characters (e.g., café)
/// - Zero-width joiners (e.g., family emojis: 👨‍👩‍👧‍👦)
/// - Right-to-left text (e.g., Arabic, Hebrew)
/// - Emojis and other special characters
///
/// ## Rationale
///
/// Consistent link styles improve document readability and maintainability. Different link
/// styles have different advantages (e.g., inline links are self-contained, reference links
/// keep the content cleaner), but mixing styles can create confusion.
///
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
            &line[start_idx..]
        } else {
            ""
        };

        if safe_substring.is_empty() {
            return None;
        }

        // Check if this is a reference definition, not a link
        if REFERENCE_DEF_RE.is_match(safe_substring) {
            return None;
        }

        // Autolink: <https://example.com>
        if let Some(cap) = AUTOLINK_RE.captures(safe_substring) {
            let url = cap.get(1).unwrap().as_str();
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("www.")
            {
                let _match_text = cap.get(0).unwrap().as_str();
                let match_start = start_idx + cap.get(0).unwrap().start();
                let match_end = start_idx + cap.get(0).unwrap().end();
                return Some(("autolink".to_string(), match_start, match_end));
            }
        }

        // Inline: [text](url)
        if let Some(cap) = INLINE_RE.captures(safe_substring) {
            let _match_text = cap.get(0).unwrap().as_str();
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();

            // Check if it's a URL inline (where text and URL are the same)
            let text = cap.get(1).unwrap().as_str();
            let url = cap.get(2).unwrap().as_str();
            if text == url {
                return Some(("url_inline".to_string(), match_start, match_end));
            }

            return Some(("inline".to_string(), match_start, match_end));
        }

        // Full: [text][reference]
        if let Some(cap) = FULL_RE.captures(safe_substring) {
            let _match_text = cap.get(0).unwrap().as_str();
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("full".to_string(), match_start, match_end));
        }

        // Collapsed: [text][]
        if let Some(cap) = COLLAPSED_RE.captures(safe_substring) {
            let _match_text = cap.get(0).unwrap().as_str();
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();
            return Some(("collapsed".to_string(), match_start, match_end));
        }

        // Shortcut: [text]
        if let Some(cap) = SHORTCUT_RE.captures(safe_substring) {
            let _match_text = cap.get(0).unwrap().as_str();
            let match_start = start_idx + cap.get(0).unwrap().start();
            let match_end = start_idx + cap.get(0).unwrap().end();

            // Make sure it's not the start of an inline, full, or collapsed reference
            if safe_substring.len() > _match_text.len() {
                let next_char = &safe_substring[_match_text.len()..].chars().next().unwrap();
                if *next_char == '(' || *next_char == '[' {
                    return None;
                }
            }

            return Some(("shortcut".to_string(), match_start, match_end));
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
                    code_fence = if trimmed.starts_with("```") {
                        "```"
                    } else {
                        "~~~"
                    };
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
                if let Some((style, match_start, match_end)) =
                    self.determine_link_style(line, start_idx)
                {
                    if !self.is_in_code(&lines, line_num, match_start) {
                        found_styles.insert(style.clone());
                    }
                    // Ensure we're making progress to avoid infinite loops
                    if match_end > start_idx {
                        start_idx = match_end;
                    } else {
                        start_idx += 1;
                    }
                } else {
                    // Make sure we advance by at least one character
                    let next_char_width =
                        line[start_idx..].chars().next().map_or(1, |c| c.len_utf8());
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
                if let Some((style, match_start, match_end)) =
                    self.determine_link_style(line, start_idx)
                {
                    if !self.is_in_code(&lines, line_num, match_start)
                        && !allowed_styles.contains(&style)
                    {
                        warnings.push(LintWarning {
            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: match_start + 1,
                            message: format!(
                                "Link/image style '{}' is not consistent with document",
                                style
                            ),
                            severity: Severity::Warning,
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
                    let next_char_width =
                        line[start_idx..].chars().next().map_or(1, |c| c.len_utf8());
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
