//!
//! Rule MD054: Link and image style should be consistent
//!
//! See [docs/md054.md](../../docs/md054.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::calculate_match_range;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

mod md054_config;
use md054_config::MD054Config;

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
#[derive(Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Clone)]
pub enum LinkImageStyle {
    Autolink,
    Inline,
    UrlInline,
    Shortcut,
    Collapsed,
    Full,
}

#[derive(Debug, Default, Clone)]
pub struct MD054LinkImageStyle {
    config: MD054Config,
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
            config: MD054Config {
                autolink,
                collapsed,
                full,
                inline,
                shortcut,
                url_inline,
            },
        }
    }
    
    pub fn from_config_struct(config: MD054Config) -> Self {
        Self { config }
    }

    /// Check if a style is allowed based on configuration
    fn is_style_allowed(&self, style: &str) -> bool {
        match style {
            "autolink" => self.config.autolink,
            "collapsed" => self.config.collapsed,
            "full" => self.config.full,
            "inline" => self.config.inline,
            "shortcut" => self.config.shortcut,
            "url_inline" => self.config.url_inline,
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any link patterns before expensive processing
        if !content.contains('[') && !content.contains('<') {
            return Ok(Vec::new());
        }

        let structure = DocumentStructure::new(content);
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip code blocks and reference definitions early
            if structure.is_in_code_block(line_num + 1) {
                continue;
            }
            if REFERENCE_DEF_RE.is_match(line) {
                continue;
            }
            if line.trim_start().starts_with("<!--") {
                continue;
            }

            // Quick check for any link patterns in this line
            if !line.contains('[') && !line.contains('<') {
                continue;
            }

            let mut idx = 0;
            let line_chars: Vec<char> = line.chars().collect();

            while idx < line_chars.len() {
                let byte_idx = line_chars[..idx]
                    .iter()
                    .map(|c| c.len_utf8())
                    .sum::<usize>();
                let slice = &line[byte_idx..];

                // Strict priority: full -> collapsed -> inline/url_inline -> autolink -> shortcut
                // 1. Full reference
                if let Some(cap) = FULL_RE.captures(slice) {
                    let m = cap.get(0).unwrap();
                    let match_start_byte = byte_idx + m.start();
                    let match_end_byte = byte_idx + m.end();
                    let match_start_char = line[..match_start_byte].chars().count();
                    let match_end_char = line[..match_end_byte].chars().count();

                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1) && !self.config.full
                    {
                        let match_len = match_end_char - match_start_char;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, match_start_char, match_len);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Link/image style 'full' is not consistent with document"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
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

                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1)
                        && !self.config.collapsed
                    {
                        let match_len = match_end_char - match_start_char;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, match_start_char, match_len);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Link/image style 'collapsed' is not consistent with document"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
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
                            let match_len = match_end_char - match_start_char;
                            let (start_line, start_col, end_line, end_col) = calculate_match_range(
                                line_num + 1,
                                line,
                                match_start_char,
                                match_len,
                            );

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                message: format!(
                                    "Link/image style '{}' is not consistent with document",
                                    style
                                ),
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

                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1)
                        && !self.config.autolink
                    {
                        let match_len = match_end_char - match_start_char;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, match_start_char, match_len);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Link/image style 'autolink' is not consistent with document"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
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
                    if after.starts_with('[') || after.starts_with("[]") || after.starts_with("][")
                    {
                        idx += 1;
                        continue;
                    }

                    if !structure.is_in_code_span(line_num + 1, match_start_char + 1)
                        && !self.config.shortcut
                    {
                        let match_len = match_end_char - match_start_char;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, match_start_char, match_len);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Link/image style 'shortcut' is not consistent with document"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: None,
                        });
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

    fn fix(&self, _ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Automatic fixing for link styles is not supported and could break content
        Err(LintError::FixFailed(
            "MD054 does not support automatic fixing of link/image style consistency.".to_string(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD054Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}
