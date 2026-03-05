//!
//! Rule MD054: Link and image style should be consistent
//!
//! See [docs/md054.md](../../docs/md054.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};

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

/// Convert a byte offset in the content to a 1-indexed (line, column) pair.
/// Column is measured in characters, not bytes.
fn byte_offset_to_line_col(content: &str, byte_offset: usize) -> (usize, usize) {
    let before = &content[..byte_offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = before[last_newline..].chars().count() + 1;
    (line, col)
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

        if content.is_empty() || (!content.contains('[') && !content.contains('<')) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Enable task lists and footnotes so pulldown-cmark handles them natively
        // rather than emitting them as broken link references.
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);

        // Resolve broken references so they emit link events with *Unknown link types.
        // This catches shortcut/collapsed/full references without definitions,
        // which helps users find incomplete style migrations.
        let parser = Parser::new_with_broken_link_callback(
            content,
            options,
            Some(|_: BrokenLink<'_>| Some(("".into(), "".into()))),
        )
        .into_offset_iter();

        // Track link Start/End pairs
        // Each entry: (link_type, dest_url, start_byte_offset)
        let mut link_stack: Vec<(LinkType, String, usize)> = Vec::new();
        let mut text_collector: Option<String> = None;

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Link {
                    link_type, dest_url, ..
                })
                | Event::Start(Tag::Image {
                    link_type, dest_url, ..
                }) => {
                    text_collector = Some(String::new());
                    link_stack.push((link_type, dest_url.to_string(), range.start));
                }
                Event::End(TagEnd::Link | TagEnd::Image) => {
                    if let Some((link_type, dest_url, start_byte)) = link_stack.pop() {
                        let text = text_collector.take().unwrap_or_default();
                        let end_byte = range.end;

                        let style = match link_type {
                            LinkType::Autolink | LinkType::Email => "autolink",
                            LinkType::Inline => {
                                if text == dest_url {
                                    "url-inline"
                                } else {
                                    "inline"
                                }
                            }
                            LinkType::Reference | LinkType::ReferenceUnknown => "full",
                            LinkType::Collapsed | LinkType::CollapsedUnknown => "collapsed",
                            LinkType::Shortcut | LinkType::ShortcutUnknown => "shortcut",
                            _ => continue,
                        };

                        // Filter alert/callout syntax [!TYPE]
                        if matches!(
                            link_type,
                            LinkType::ShortcutUnknown | LinkType::CollapsedUnknown | LinkType::ReferenceUnknown
                        ) && text.starts_with('!')
                        {
                            continue;
                        }

                        let (start_line, start_col) = byte_offset_to_line_col(content, start_byte);

                        // Filter out links in frontmatter or code blocks
                        if ctx
                            .line_info(start_line)
                            .is_some_and(|info| info.in_front_matter || info.in_code_block)
                        {
                            continue;
                        }

                        if !self.is_style_allowed(style) {
                            let (end_line, end_col) = byte_offset_to_line_col(content, end_byte);

                            warnings.push(LintWarning {
                                rule_name: Some(self.name().to_string()),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                message: format!("Link/image style '{style}' is not allowed"),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                }
                Event::Text(ref t) | Event::Code(ref t) => {
                    if let Some(ref mut collector) = text_collector {
                        collector.push_str(t);
                    }
                }
                _ => {}
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
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
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
    fn test_broken_shortcut_reference_flagged_when_disallowed() {
        // [text] without a definition looks like a broken/incomplete shortcut reference.
        // When shortcut style is disallowed, flag it to help users find incomplete migrations.
        let rule = MD054LinkImageStyle::new(true, true, true, true, false, true);
        let content = "Some [noref] text without a definition.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Broken shortcut reference should be flagged when shortcut is disallowed, got: {result:?}"
        );
        assert!(result[0].message.contains("'shortcut'"));
    }

    #[test]
    fn test_broken_shortcut_reference_not_flagged_when_allowed() {
        // [text] without a definition should not warn when shortcut style is allowed
        let rule = MD054LinkImageStyle::new(true, true, true, true, true, true);
        let content = "Some [noref] text without a definition.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Broken shortcut should not be flagged when shortcut is allowed, got: {result:?}"
        );
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
}
