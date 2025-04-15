use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use crate::utils::document_structure::DocumentStructure;

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
}

impl Rule for MD054LinkImageStyle {
    fn name(&self) -> &'static str {
        "MD054"
    }

    fn description(&self) -> &'static str {
        "Link and image style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let structure = DocumentStructure::new(content);
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            if structure.is_in_code_block(line_num + 1) {
                continue;
            }
            // Skip reference definition lines
            if REFERENCE_DEF_RE.is_match(line) {
                continue;
            }
            let mut idx = 0;
            let line_chars: Vec<char> = line.chars().collect();
            while idx < line_chars.len() {
                let byte_idx = line_chars[..idx].iter().map(|c| c.len_utf8()).sum::<usize>();
                let slice = &line[byte_idx..];

                // Strict priority: full -> collapsed -> inline/url_inline -> autolink -> shortcut
                // 1. Full reference
                if let Some(cap) = FULL_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();
                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) {
                        if !self.full {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: line_num + 1,
                                column: match_start_char + 1,
                                message: "Link/image style 'full' is not consistent with document".to_string(),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    idx = match_end_char;
                    continue;
                }
                // 2. Collapsed reference
                if let Some(cap) = COLLAPSED_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();
                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) {
                        if !self.collapsed {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: line_num + 1,
                                column: match_start_char + 1,
                                message: "Link/image style 'collapsed' is not consistent with document".to_string(),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    idx = match_end_char;
                    continue;
                }
                // 3. Inline/url_inline
                if let Some(cap) = INLINE_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();
                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) {
                        let text = cap.get(1).unwrap().as_str();
                        let url = cap.get(2).unwrap().as_str();
                        let style = if text == url { "url_inline" } else { "inline" };
                        if !self.is_style_allowed(style) {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: line_num + 1,
                                column: match_start_char + 1,
                                message: format!("Link/image style '{}' is not consistent with document", style),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    idx = match_end_char;
                    continue;
                }
                // 4. Autolink
                if let Some(cap) = AUTOLINK_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();
                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) {
                        if !self.autolink {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: line_num + 1,
                                column: match_start_char + 1,
                                message: "Link/image style 'autolink' is not consistent with document".to_string(),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    idx = match_end_char;
                    continue;
                }
                // 5. Shortcut (only if not followed by '[', '[]', or '][')
                if let Some(cap) = SHORTCUT_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();
                    let after = &line[match_end_byte..];
                    // Only match as shortcut if not followed by '[', '[]', or ']['
                    if after.starts_with('[')
                        || after.starts_with("[]")
                        || after.starts_with("][") {
                        idx += 1;
                        continue;
                    }
                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) {
                        if !self.shortcut {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: line_num + 1,
                                column: match_start_char + 1,
                                message: "Link/image style 'shortcut' is not consistent with document".to_string(),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    idx = match_end_char;
                    continue;
                }
                // No match, advance by 1
                idx += 1;
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
