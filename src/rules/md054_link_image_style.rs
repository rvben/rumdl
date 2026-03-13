//!
//! Rule MD054: Link and image style should be consistent
//!
//! See [docs/md054.md](../../docs/md054.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use pulldown_cmark::LinkType;

mod md054_config;
use md054_config::MD054Config;

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
/// - Combining characters (e.g., cafe)
/// - Zero-width joiners (e.g., family emojis)
/// - Right-to-left text (e.g., Arabic, Hebrew)
/// - Emojis and other special characters
///
/// ## Rationale
///
/// Consistent link styles improve document readability and maintainability. Different link
/// styles have different advantages (e.g., inline links are self-contained, reference links
/// keep the content cleaner), but mixing styles can create confusion.
///
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

    /// Convert a byte offset to a 1-indexed character column within its line.
    /// Only called for disallowed links (cold path), so O(line_length) is fine.
    fn byte_to_char_col(content: &str, byte_offset: usize) -> usize {
        let before = &content[..byte_offset];
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        before[last_newline..].chars().count() + 1
    }

    /// Check if a style is allowed based on configuration
    fn is_style_allowed(&self, style: &str) -> bool {
        match style {
            "autolink" => self.config.autolink,
            "collapsed" => self.config.collapsed,
            "full" => self.config.full,
            "inline" => self.config.inline,
            "shortcut" => self.config.shortcut,
            "url-inline" => self.config.url_inline,
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
        let mut warnings = Vec::new();

        // Process links from pre-parsed data
        for link in &ctx.links {
            // Skip broken references (empty URL means unresolved reference)
            if matches!(
                link.link_type,
                LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
            ) && link.url.is_empty()
            {
                continue;
            }

            let style = match link.link_type {
                LinkType::Autolink | LinkType::Email => "autolink",
                LinkType::Inline => {
                    if link.text == link.url {
                        "url-inline"
                    } else {
                        "inline"
                    }
                }
                LinkType::Reference => "full",
                LinkType::Collapsed => "collapsed",
                LinkType::Shortcut => "shortcut",
                _ => continue,
            };

            // Filter out links in frontmatter or code blocks
            if ctx
                .line_info(link.line)
                .is_some_and(|info| info.in_front_matter || info.in_code_block)
            {
                continue;
            }

            if !self.is_style_allowed(style) {
                let start_col = Self::byte_to_char_col(content, link.byte_offset);
                let (end_line, _) = ctx.offset_to_line_col(link.byte_end);
                let end_col = Self::byte_to_char_col(content, link.byte_end);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: link.line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Link/image style '{style}' is not allowed"),
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }

        // Process images from pre-parsed data
        for image in &ctx.images {
            // Skip broken references (empty URL means unresolved reference)
            if matches!(
                image.link_type,
                LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
            ) && image.url.is_empty()
            {
                continue;
            }

            let style = match image.link_type {
                LinkType::Autolink | LinkType::Email => "autolink",
                LinkType::Inline => {
                    if image.alt_text == image.url {
                        "url-inline"
                    } else {
                        "inline"
                    }
                }
                LinkType::Reference => "full",
                LinkType::Collapsed => "collapsed",
                LinkType::Shortcut => "shortcut",
                _ => continue,
            };

            // Filter out images in frontmatter or code blocks
            if ctx
                .line_info(image.line)
                .is_some_and(|info| info.in_front_matter || info.in_code_block)
            {
                continue;
            }

            if !self.is_style_allowed(style) {
                let start_col = Self::byte_to_char_col(content, image.byte_offset);
                let (end_line, _) = ctx.offset_to_line_col(image.byte_end);
                let end_col = Self::byte_to_char_col(content, image.byte_end);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: image.line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Link/image style '{style}' is not allowed"),
                    severity: Severity::Warning,
                    fix: None,
                });
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

    fn fix_capability(&self) -> crate::rule::FixCapability {
        crate::rule::FixCapability::Unfixable
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.likely_has_links_or_images() && !ctx.likely_has_html())
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
        let content = "[inline](url) [ref][] [ref] <https://autolink.com> [full][ref] [url](url)\n\n[ref]: url";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_only_inline_allowed() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        // [bad][] has no definition for "bad", so pulldown-cmark doesn't emit it as a link
        let content = "[allowed](url) [not][ref] <https://bad.com> [collapsed][] [shortcut]\n\n[ref]: url\n[shortcut]: url\n[collapsed]: url";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4, "Expected 4 warnings, got: {result:?}");
        assert!(result[0].message.contains("'full'"));
        assert!(result[1].message.contains("'autolink'"));
        assert!(result[2].message.contains("'collapsed'"));
        assert!(result[3].message.contains("'shortcut'"));
    }

    #[test]
    fn test_only_autolink_allowed() {
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "<https://good.com> [bad](url) [bad][ref]\n\n[ref]: url";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Expected 2 warnings, got: {result:?}");
        assert!(result[0].message.contains("'inline'"));
        assert!(result[1].message.contains("'full'"));
    }

    #[test]
    fn test_url_inline_detection() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, true);
        let content = "[https://example.com](https://example.com) [text](https://example.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // First is url_inline (allowed), second is inline (allowed)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_url_inline_not_allowed() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[https://example.com](https://example.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'url-inline'"));
    }

    #[test]
    fn test_shortcut_vs_full_detection() {
        let rule = MD054LinkImageStyle::new(false, false, true, false, false, false);
        let content = "[shortcut] [full][ref]\n\n[shortcut]: url\n[ref]: url2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only shortcut should be flagged
        assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
        assert!(result[0].message.contains("'shortcut'"));
    }

    #[test]
    fn test_collapsed_reference() {
        let rule = MD054LinkImageStyle::new(false, true, false, false, false, false);
        let content = "[collapsed][] [bad][ref]\n\n[collapsed]: url\n[ref]: url2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
        assert!(result[0].message.contains("'full'"));
    }

    #[test]
    fn test_code_blocks_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "```\n[ignored](url) <https://ignored.com>\n```\n\n[checked](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the link outside code block should be checked
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_spans_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "`[ignored](url)` and `<https://ignored.com>` but [checked](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the link outside code spans should be checked
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_reference_definitions_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[ref]: https://example.com\n[ref2]: <https://example2.com>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Reference definitions should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_html_comments_ignored() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "<!-- [ignored](url) -->\n  <!-- <https://ignored.com> -->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_unicode_support() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[cafe](https://cafe.com) [emoji](url) [korean](url) [hebrew](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All should be detected as inline (allowed)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_line_positions() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "Line 1\n\nLine 3 with <https://bad.com> here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 13); // Position of '<'
    }

    #[test]
    fn test_multiple_links_same_line() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[ok](url) but <https://good.com> and [also][bad]\n\n[bad]: url";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Expected 2 warnings, got: {result:?}");
        assert!(result[0].message.contains("'autolink'"));
        assert!(result[1].message.contains("'full'"));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_links() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "Just plain text without any links";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_returns_error() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[link](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Expected 2 warnings, got: {result:?}");
        assert!(result[0].message.contains("'full'"));
        assert!(result[1].message.contains("'shortcut'"));
    }

    #[test]
    fn test_not_shortcut_when_followed_by_bracket() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, true, false);
        // [text][ should not be detected as shortcut
        let content = "[text][ more text\n[text](url) is inline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only second line should have inline link
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_cjk_correct_column_positions() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "日本語テスト <https://example.com>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'autolink'"));
        // The '<' starts at byte position 19 (after 6 CJK chars * 3 bytes + 1 space)
        // which is character position 8 (1-indexed)
        assert_eq!(
            result[0].column, 8,
            "Column should be 1-indexed character position of '<'"
        );
    }

    #[test]
    fn test_code_span_detection_with_cjk_prefix() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        // Link inside code span after CJK characters
        let content = "日本語 `[link](url)` text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The link is inside a code span, so it should not be flagged
        assert_eq!(result.len(), 0, "Link inside code span should not be flagged");
    }

    #[test]
    fn test_complex_unicode_with_zwj() {
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "[family](url) [cafe](https://cafe.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both should be detected as inline (allowed)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_gfm_alert_not_flagged_as_shortcut() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "> [!NOTE]\n> This is a note.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "GFM alert should not be flagged as shortcut link, got: {result:?}"
        );
    }

    #[test]
    fn test_various_alert_types_not_flagged() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        for alert_type in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION", "note", "info"] {
            let content = format!("> [!{alert_type}]\n> Content.\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Alert type {alert_type} should not be flagged, got: {result:?}"
            );
        }
    }

    #[test]
    fn test_shortcut_link_still_flagged_when_disallowed() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "See [reference] for details.\n\n[reference]: https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Regular shortcut links should still be flagged");
    }

    #[test]
    fn test_alert_with_frontmatter_not_flagged() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "---\ntitle: heading\n---\n\n> [!note]\n> Content for the note.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Alert in blockquote with frontmatter should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_alert_without_blockquote_prefix_not_flagged() {
        // Even without the `> ` prefix, [!TYPE] is alert syntax and should not be
        // treated as a shortcut reference
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "[!NOTE]\nSome content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "[!NOTE] without blockquote prefix should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_alert_custom_types_not_flagged() {
        // Obsidian and other flavors support custom callout types
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        for alert_type in ["bug", "example", "quote", "abstract", "todo", "faq"] {
            let content = format!("> [!{alert_type}]\n> Content.\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Custom alert type {alert_type} should not be flagged, got: {result:?}"
            );
        }
    }

    // Tests for issue #488: code spans with brackets in inline link text

    #[test]
    fn test_code_span_with_brackets_in_inline_link() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "Link to [`[myArray]`](#info).";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The inline link should be detected correctly, [myArray] should NOT be flagged as shortcut
        assert!(
            result.is_empty(),
            "Code span with brackets in inline link should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_code_span_with_array_index_in_inline_link() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "See [`item[0]`](#info) for details.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Array index in code span should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_code_span_with_hash_brackets_in_inline_link() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = r#"See [`hash["key"]`](#info) for details."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Hash access in code span should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_488_full_reproduction() {
        // Exact reproduction case from issue #488
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "---\ntitle: heading\n---\n\nLink to information about [`[myArray]`](#information-on-myarray).\n\n## Information on `[myArray]`\n\nSome section content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #488 reproduction case should produce no warnings, got: {result:?}"
        );
    }

    #[test]
    fn test_bracket_text_without_definition_not_flagged() {
        // [text] without a matching [text]: url definition is NOT a link.
        // It should never be flagged regardless of config.
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "Some [noref] text without a definition.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bracket text without definition should not be flagged as a link, got: {result:?}"
        );
    }

    #[test]
    fn test_array_index_notation_not_flagged() {
        // Common bracket patterns that are not links should never be flagged
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "Access `arr[0]` and use [1] or [optional] in your code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Array indices and bracket text should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_real_shortcut_reference_still_flagged() {
        // [text] WITH a matching definition IS a shortcut link and should be flagged
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "See [example] for details.\n\n[example]: https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Real shortcut reference with definition should be flagged, got: {result:?}"
        );
        assert!(result[0].message.contains("'shortcut'"));
    }

    #[test]
    fn test_footnote_syntax_not_flagged_as_shortcut() {
        // [^ref] should not be flagged as a shortcut reference
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "See [^1] for details.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Footnote syntax should not be flagged as shortcut, got: {result:?}"
        );
    }

    #[test]
    fn test_inline_link_with_code_span_detected_as_inline() {
        // When inline is disallowed, code-span-with-brackets inline link should be flagged as inline
        let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
        let content = "See [`[myArray]`](#info) for details.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Inline link with code span should be flagged when inline is disallowed"
        );
        assert!(
            result[0].message.contains("'inline'"),
            "Should be flagged as 'inline' style, got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_autolink_only_document_not_skipped() {
        // Document with only autolinks (no brackets) must still be checked
        let rule = MD054LinkImageStyle::new(false, false, false, true, false, false);
        let content = "Visit <https://example.com> for more info.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !rule.should_skip(&ctx),
            "should_skip must return false for autolink-only documents"
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Autolink should be flagged when disallowed");
        assert!(result[0].message.contains("'autolink'"));
    }

    #[test]
    fn test_nested_image_in_link() {
        // [![alt](img.png)](https://example.com) — image nested inside a link
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = "[![alt text](img.png)](https://example.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Both the inner image (inline) and outer link (inline) should be detected
        assert!(
            result.len() >= 2,
            "Nested image-in-link should detect both elements, got: {result:?}"
        );
    }

    #[test]
    fn test_multi_line_link() {
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = "[long link\ntext](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Multi-line inline link should be detected");
        assert!(result[0].message.contains("'inline'"));
    }

    #[test]
    fn test_link_with_title() {
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = r#"[text](url "title")"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Link with title should be detected as inline");
        assert!(result[0].message.contains("'inline'"));
    }

    #[test]
    fn test_empty_link_text() {
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = "[](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty link text should be detected");
        assert!(result[0].message.contains("'inline'"));
    }

    #[test]
    fn test_escaped_brackets_not_detected() {
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = r"\[not a link\] and also \[not this either\]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Escaped brackets should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_links_in_blockquotes() {
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = "> [link](url) in a blockquote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Links in blockquotes should be detected");
        assert!(result[0].message.contains("'inline'"));
    }

    #[test]
    fn test_image_detection() {
        let rule = MD054LinkImageStyle::new(false, false, false, false, false, false);
        let content = "![alt](img.png)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Inline image should be detected");
        assert!(result[0].message.contains("'inline'"));
    }
}
