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
    pub fn new(autolink: bool, collapsed: bool, full: bool, inline: bool, shortcut: bool, url_inline: bool) -> Self {
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

#[derive(Debug)]
struct LinkMatch {
    style: &'static str,
    start: usize,
    end: usize,
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

            // Find all matches in the line
            let mut matches = Vec::new();

            // Find all autolinks
            for cap in AUTOLINK_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();
                matches.push(LinkMatch {
                    style: "autolink",
                    start: m.start(),
                    end: m.end(),
                });
            }

            // Find all full references
            for cap in FULL_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();
                matches.push(LinkMatch {
                    style: "full",
                    start: m.start(),
                    end: m.end(),
                });
            }

            // Find all collapsed references
            for cap in COLLAPSED_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();
                matches.push(LinkMatch {
                    style: "collapsed",
                    start: m.start(),
                    end: m.end(),
                });
            }

            // Find all inline links
            for cap in INLINE_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();
                let text = cap.get(1).unwrap().as_str();
                let url = cap.get(2).unwrap().as_str();
                matches.push(LinkMatch {
                    style: if text == url { "url_inline" } else { "inline" },
                    start: m.start(),
                    end: m.end(),
                });
            }

            // Sort matches by start position to ensure we don't double-count
            matches.sort_by_key(|m| m.start);

            // Remove overlapping matches (keep the first one)
            let mut filtered_matches = Vec::new();
            let mut last_end = 0;
            for m in matches {
                if m.start >= last_end {
                    last_end = m.end;
                    filtered_matches.push(m);
                }
            }

            // Now find shortcut references that don't overlap with other matches
            for cap in SHORTCUT_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();
                let start = m.start();
                let end = m.end();

                // Check if this overlaps with any existing match
                let overlaps = filtered_matches.iter().any(|existing| {
                    (start >= existing.start && start < existing.end) || (end > existing.start && end <= existing.end)
                });

                if !overlaps {
                    // Check if followed by '(', '[', '[]', or ']['
                    let after = &line[end..];
                    if !after.starts_with('(') && !after.starts_with('[') {
                        filtered_matches.push(LinkMatch {
                            style: "shortcut",
                            start,
                            end,
                        });
                    }
                }
            }

            // Sort again after adding shortcuts
            filtered_matches.sort_by_key(|m| m.start);

            // Check each match
            for m in filtered_matches {
                let match_start_char = line[..m.start].chars().count();

                if !structure.is_in_code_span(line_num + 1, match_start_char + 1) && !self.is_style_allowed(m.style) {
                    let match_len = line[m.start..m.end].chars().count();
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num + 1, line, match_start_char, match_len);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Link/image style '{}' is not consistent with document", m.style),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_all_styles_allowed_by_default() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, true, true);
        let content = "[inline](url) [ref][] [ref] <autolink> [full][ref] [url](url)\n\n[ref]: url";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_only_inline_allowed() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[allowed](url) [not][ref] <https://bad.com> [bad][] [shortcut]\n\n[ref]: url\n[shortcut]: url";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4);
        assert!(result[0].message.contains("'full'"));
        assert!(result[1].message.contains("'autolink'"));
        assert!(result[2].message.contains("'collapsed'"));
        assert!(result[3].message.contains("'shortcut'"));
    }

    #[test]
    fn test_only_autolink_allowed() {
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "<https://good.com> [bad](url) [bad][ref]\n\n[ref]: url";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("'inline'"));
        assert!(result[1].message.contains("'full'"));
    }

    #[test]
    fn test_url_inline_detection() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, true);
        let content = "[https://example.com](https://example.com) [text](https://example.com)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // First is url_inline (allowed), second is inline (allowed)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_url_inline_not_allowed() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[https://example.com](https://example.com)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'url_inline'"));
    }

    #[test]
    fn test_shortcut_vs_full_detection() {
        let rule = MD054LinkImageStyle::new(false, false, true, false, false, false);
        let content = "[shortcut] [full][ref]\n\n[shortcut]: url\n[ref]: url2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only shortcut should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'shortcut'"));
    }

    #[test]
    fn test_collapsed_reference() {
        let rule = MD054LinkImageStyle::new(false, true, false, false, false, false);
        let content = "[collapsed][] [bad][ref]\n\n[collapsed]: url\n[ref]: url2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'full'"));
    }

    #[test]
    fn test_code_blocks_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "```\n[ignored](url) <https://ignored.com>\n```\n\n[checked](url)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the link outside code block should be checked
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_spans_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "`[ignored](url)` and `<https://ignored.com>` but [checked](url)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the link outside code spans should be checked
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_reference_definitions_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[ref]: https://example.com\n[ref2]: <https://example2.com>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Reference definitions should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_html_comments_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "<!-- [ignored](url) -->\n  <!-- <https://ignored.com> -->";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_unicode_support() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[café ☕](https://café.com) [emoji 😀](url) [한글](url) [עברית](url)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // All should be detected as inline (allowed)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_line_positions() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "Line 1\n\nLine 3 with <https://bad.com> here";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 13); // Position of '<'
    }

    #[test]
    fn test_multiple_links_same_line() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[ok](url) but <bad> and [also][bad]\n\n[bad]: url";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("'autolink'"));
        assert!(result[1].message.contains("'full'"));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_links() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "Just plain text without any links";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_returns_error() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[link](url)";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx);

        assert!(result.is_err());
        if let Err(LintError::FixFailed(msg)) = result {
            assert!(msg.contains("does not support automatic fixing"));
        }
    }

    #[test]
    fn test_priority_order() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        // Test that [text][ref] is detected as full, not shortcut
        let content = "[text][ref] not detected as [shortcut]\n\n[ref]: url\n[shortcut]: url2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("'full'"));
        assert!(result[1].message.contains("'shortcut'"));
    }

    #[test]
    fn test_not_shortcut_when_followed_by_bracket() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, true, false);
        // [text][ should not be detected as shortcut
        let content = "[text][ more text\n[text](url) is inline";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only second line should have inline link
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_complex_unicode_with_zwj() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        // Test with zero-width joiners and complex Unicode
        let content = "[👨‍👩‍👧‍👦 family](url) [café☕](https://café.com)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Both should be detected as inline (allowed)
        assert_eq!(result.len(), 0);
    }
}
