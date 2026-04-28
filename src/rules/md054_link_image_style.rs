//!
//! Rule MD054: Link and image style should be consistent
//!
//! See [docs/md054.md](../../docs/md054.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use pulldown_cmark::LinkType;
use std::collections::HashMap;

mod label;
mod md054_config;
mod transform;

use md054_config::{MD054Config, PreferredStyles};

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
                preferred_style: PreferredStyles::default(),
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
        let last_newline = before.rfind('\n').map_or(0, |i| i + 1);
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

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Compute the fix plan once and index its planned entries by source
        // byte offset. Each warning whose link/image has an entry gets a Fix
        // attached so the fix coordinator (which gates on `fix.is_some()`) and
        // LSP code actions both see this rule as fixable.
        //
        // Conversions that require a new reference definition (inline → full,
        // etc.) carry the EOF-appended ref-def as an `additional_edit` on the
        // per-warning Fix, so quick-fix paths that apply a single warning
        // produce a complete result (link rewrite + ref def) atomically.
        let plan = if self.should_skip(ctx) {
            transform::FixPlan::default()
        } else {
            transform::plan(ctx, &self.config)
        };
        let entries_by_offset: HashMap<usize, &transform::PlannedEdit> =
            plan.entries.iter().map(|e| (e.edit.range.start, e)).collect();
        let build_fix = |offset: usize| -> Option<Fix> {
            let entry = entries_by_offset.get(&offset)?;
            let primary_range = entry.edit.range.clone();
            let primary_replacement = entry.edit.replacement.clone();
            match &entry.new_ref {
                None => Some(Fix::new(primary_range, primary_replacement)),
                Some(def) => {
                    let appended = transform::render_ref_def_append(content, def)?;
                    let eof_range = content.len()..content.len();
                    Some(Fix::with_additional_edits(
                        primary_range,
                        primary_replacement,
                        vec![Fix::new(eof_range, appended)],
                    ))
                }
            }
        };

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
                    fix: build_fix(link.byte_offset),
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
                    fix: build_fix(image.byte_offset),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let plan = transform::plan(ctx, &self.config);
        Ok(transform::apply(ctx.content, plan))
    }

    fn fix_capability(&self) -> crate::rule::FixCapability {
        // Some (source, target) pairs are intentionally not auto-fixed —
        // see `transform::reachable`.
        crate::rule::FixCapability::ConditionallyFixable
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.likely_has_links_or_images() && !ctx.likely_has_html())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        Some((self.name().to_string(), toml_value))
    }

    fn polymorphic_config_keys(&self) -> &'static [&'static str] {
        // `preferred-style` accepts either a scalar string or a list of strings.
        // The serialized default can only encode one variant, so the registry
        // replaces this entry with a polymorphic sentinel for validation while
        // the user-facing default config (`rumdl config --defaults`) keeps the
        // serialized scalar form.
        &["preferred-style"]
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
    fn test_fix_unreachable_target_is_noop() {
        // inline disallowed but no reachable reference style allowed (only autolink),
        // and the link's text doesn't match its url so autolink is unreachable too.
        // The fix should leave the content unchanged rather than error.
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "[link](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
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

#[cfg(test)]
mod fix_tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;
    use md054_config::PreferredStyle;
    use pulldown_cmark::LinkType;

    /// Resolve a link's destination to its canonical form. Pulldown-cmark stores
    /// email autolinks (`<me@x>`) with the bare email in `url`, but per
    /// CommonMark §6.5 the resolved destination is `mailto:<email>`. Round-trip
    /// equivalence checks compare *destinations*, not raw `url` strings — so we
    /// canonicalize before comparing.
    fn canonical_link_url(link_type: LinkType, url: &str) -> String {
        match link_type {
            LinkType::Email => format!("mailto:{url}"),
            _ => url.to_string(),
        }
    }

    /// Helper: build a rule that disallows `inline` and leaves the remaining
    /// reference styles allowed (mirrors the reporter's MD054 config in #587).
    fn rule_inline_disallowed() -> MD054LinkImageStyle {
        // autolink, collapsed, full, inline, shortcut, url_inline
        MD054LinkImageStyle::new(true, true, true, false, true, true)
    }

    /// Helper: build a rule that disallows reference styles, leaving inline allowed.
    fn rule_only_inline() -> MD054LinkImageStyle {
        MD054LinkImageStyle::new(false, false, false, true, false, false)
    }

    /// Helper: assert four invariants together —
    ///
    /// 1. After `fix()`, MD054 emits zero warnings on the result (round-trip clean).
    /// 2. `fix()` is idempotent (running it twice yields the same content).
    /// 3. The multiset of resolved link URLs is preserved (no silent retargeting).
    /// 4. The multiset of resolved image URLs is preserved.
    fn assert_round_trip_clean(rule: &MD054LinkImageStyle, content: &str) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let before_link_urls: Vec<String> = ctx
            .links
            .iter()
            .map(|l| canonical_link_url(l.link_type, &l.url))
            .filter(|u| !u.is_empty())
            .collect();
        let before_image_urls: Vec<String> = ctx
            .images
            .iter()
            .map(|i| canonical_link_url(i.link_type, &i.url))
            .filter(|u| !u.is_empty())
            .collect();

        let fixed = rule.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx2).unwrap();
        assert!(
            warnings.is_empty(),
            "fix() left disallowed-style warnings: {warnings:?} in:\n{fixed}"
        );

        let mut after_link_urls: Vec<String> = ctx2
            .links
            .iter()
            .map(|l| canonical_link_url(l.link_type, &l.url))
            .filter(|u| !u.is_empty())
            .collect();
        let mut after_image_urls: Vec<String> = ctx2
            .images
            .iter()
            .map(|i| canonical_link_url(i.link_type, &i.url))
            .filter(|u| !u.is_empty())
            .collect();
        let mut before_link_urls_sorted = before_link_urls;
        let mut before_image_urls_sorted = before_image_urls;
        before_link_urls_sorted.sort();
        before_image_urls_sorted.sort();
        after_link_urls.sort();
        after_image_urls.sort();
        assert_eq!(
            before_link_urls_sorted, after_link_urls,
            "fix() changed the set of link URLs.\nbefore: {before_link_urls_sorted:?}\nafter: {after_link_urls:?}\nfixed:\n{fixed}"
        );
        assert_eq!(
            before_image_urls_sorted, after_image_urls,
            "fix() changed the set of image URLs.\nbefore: {before_image_urls_sorted:?}\nafter: {after_image_urls:?}\nfixed:\n{fixed}"
        );

        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed2, "fix() is not idempotent");
        fixed
    }

    // -------------------------------------------------------------------
    // inline → full
    // -------------------------------------------------------------------

    #[test]
    fn fix_inline_to_full_single_link() {
        let rule = rule_inline_disallowed();
        let content = "See the [documentation](https://example.com/docs) for details.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert_eq!(
            fixed,
            "See the [documentation][documentation] for details.\n\n\
             [documentation]: https://example.com/docs\n"
        );
    }

    #[test]
    fn fix_inline_to_full_multiple_links_dedup_by_url() {
        let rule = rule_inline_disallowed();
        let content = "First [docs](https://example.com/x).\nAgain [docs](https://example.com/x).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Both inline links collapse onto the same generated label.
        assert_eq!(
            fixed,
            "First [docs][docs].\nAgain [docs][docs].\n\n\
             [docs]: https://example.com/x\n"
        );
    }

    #[test]
    fn fix_inline_to_full_same_url_different_titles_keeps_both_titles() {
        // Two inline links share a URL but carry distinct titles. A single
        // shared reference definition could only encode one title — silently
        // dropping the other. The fix must produce two distinct ref defs.
        let rule = rule_inline_disallowed();
        let content = "First [a](https://example.com \"Title A\").\nLater [b](https://example.com \"Title B\").\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Both titles must survive the round-trip.
        assert!(fixed.contains(r#""Title A""#), "Title A lost in conversion: {fixed}");
        assert!(fixed.contains(r#""Title B""#), "Title B lost in conversion: {fixed}");
        // Two distinct definitions, not one.
        let def_count = fixed.matches("]: https://example.com").count();
        assert_eq!(def_count, 2, "expected two ref defs (one per title), got:\n{fixed}");
    }

    #[test]
    fn fix_inline_to_full_collision_disambiguates_with_suffix() {
        let rule = rule_inline_disallowed();
        let content = "[docs](https://a.com) and [docs](https://b.com).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Same slug, different URL → second link gets `-2` suffix.
        assert!(fixed.contains("[docs][docs]"));
        assert!(fixed.contains("[docs][docs-2]"));
        assert!(fixed.contains("[docs]: https://a.com"));
        assert!(fixed.contains("[docs-2]: https://b.com"));
    }

    #[test]
    fn fix_inline_to_full_preserves_title() {
        let rule = rule_inline_disallowed();
        let content = "See [link](https://example.com \"My Title\").\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[link][link]"));
        assert!(fixed.contains(r#"[link]: https://example.com "My Title""#));
    }

    #[test]
    fn fix_inline_to_full_title_with_double_quotes_uses_single_quotes() {
        let rule = rule_inline_disallowed();
        let content = "See [link](https://example.com 'has \"double\" quotes').\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Output must use a delimiter that doesn't conflict; single quotes here.
        assert!(
            fixed.contains(r#"[link]: https://example.com 'has "double" quotes'"#),
            "got:\n{fixed}"
        );
    }

    #[test]
    fn fix_inline_to_full_title_with_escaped_quote_unescapes_through_parser() {
        // CommonMark allows backslash-escaping the title delimiter so the same
        // delimiter can appear inside the title: `"has \"escaped\" quotes"`.
        // pulldown-cmark unescapes those characters before handing us the title;
        // when MD054 emits the new ref def, it must pick a delimiter that doesn't
        // conflict — here, single quotes — and not blindly reuse the literal
        // backslashes from the source span.
        let rule = rule_inline_disallowed();
        let content = "See [link](https://example.com \"has \\\"escaped\\\" quotes\").\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // The unescaped title contains real " characters, so output must wrap in 'single' quotes.
        assert!(
            fixed.contains(r#"[link]: https://example.com 'has "escaped" quotes'"#),
            "expected unescaped title with single-quote delimiter, got:\n{fixed}"
        );
        // And no stray backslash-quote in the emitted definition.
        assert!(
            !fixed.contains(r#"\""#),
            "title should be unescaped, not pass through literal `\\\"`:\n{fixed}"
        );
    }

    #[test]
    fn fix_inline_to_full_image() {
        let rule = rule_inline_disallowed();
        let content = "Logo: ![Company logo](https://example.com/logo.png).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("![Company logo][company-logo]"));
        assert!(fixed.contains("[company-logo]: https://example.com/logo.png"));
    }

    #[test]
    fn fix_inline_to_full_unicode_text() {
        let rule = rule_inline_disallowed();
        let content = "Voir [café résumé](https://cafe.example.com).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Slug preserves the Unicode letters (they're alphanumeric) and lowercases them.
        assert!(fixed.contains("[café résumé][café-résumé]"));
        assert!(fixed.contains("[café-résumé]: https://cafe.example.com"));
    }

    #[test]
    fn fix_inline_to_full_reuses_existing_ref_def_for_same_url() {
        let rule = rule_inline_disallowed();
        let content = "Old: [other][site]\n\
                       New: [docs](https://example.com)\n\
                       \n\
                       [site]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // The new conversion should reuse the existing `site` label.
        assert!(
            fixed.contains("[docs][site]"),
            "expected reuse of existing label, got:\n{fixed}"
        );
        // No duplicate definition added.
        assert_eq!(fixed.matches("https://example.com").count(), 1);
    }

    #[test]
    fn fix_inline_to_full_avoids_existing_label_collision() {
        let rule = rule_inline_disallowed();
        let content = "Old: [a][docs]\n\
                       New: [docs](https://other.com)\n\
                       \n\
                       [docs]: https://existing.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // New link must NOT reuse [docs] (different URL); should suffix.
        assert!(fixed.contains("[docs][docs-2]"));
        assert!(fixed.contains("[docs-2]: https://other.com"));
        // Original definition unchanged.
        assert!(fixed.contains("[docs]: https://existing.com"));
    }

    #[test]
    fn fix_inline_to_full_no_trailing_newline() {
        let rule = rule_inline_disallowed();
        let content = "[docs](https://example.com)";
        let fixed = assert_round_trip_clean(&rule, content);
        assert_eq!(fixed, "[docs][docs]\n\n[docs]: https://example.com\n");
    }

    #[test]
    fn fix_inline_to_full_skips_code_blocks() {
        let rule = rule_inline_disallowed();
        let content = "Outside [a](https://x.com).\n\n```\n[fenced](https://y.com)\n```\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // The fenced block content stays untouched.
        assert!(fixed.contains("[fenced](https://y.com)"));
        // Outside link converted.
        assert!(fixed.contains("[a][a]"));
    }

    #[test]
    fn fix_inline_to_full_skips_frontmatter() {
        let rule = rule_inline_disallowed();
        let content = "---\nlink: [foo](https://x.com)\n---\n\n[doc](https://y.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Frontmatter content untouched.
        assert!(fixed.contains("link: [foo](https://x.com)"));
        // Link in body converted.
        assert!(fixed.contains("[doc][doc]"));
    }

    // -------------------------------------------------------------------
    // reference → inline
    // -------------------------------------------------------------------

    #[test]
    fn fix_full_to_inline() {
        let rule = rule_only_inline();
        let content = "See [docs][site].\n\n[site]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Inline splice; ref def remains (MD053 cleans up unused defs separately).
        assert!(fixed.contains("[docs](https://example.com)"));
    }

    #[test]
    fn fix_collapsed_to_inline() {
        let rule = rule_only_inline();
        let content = "See [docs][].\n\n[docs]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[docs](https://example.com)"));
    }

    #[test]
    fn fix_shortcut_to_inline() {
        let rule = rule_only_inline();
        let content = "See [docs].\n\n[docs]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[docs](https://example.com)"));
    }

    #[test]
    fn fix_full_to_inline_preserves_title() {
        let rule = rule_only_inline();
        let content = "See [docs][site].\n\n[site]: https://example.com \"Site Title\"\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains(r#"[docs](https://example.com "Site Title")"#),
            "title not preserved, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_inline_to_full_text_with_code_span_containing_brackets() {
        // Text containing a code span with brackets used to confuse a hand-rolled
        // bracket-counter. We rely on pulldown-cmark for the parse, so the
        // conversion must round-trip cleanly.
        let rule = rule_inline_disallowed();
        let content = "See [`a[0]` index](https://example.com).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[`a[0]` index]["),
            "code-span text not preserved, got:\n{fixed}"
        );
        assert!(
            fixed.contains("]: https://example.com"),
            "missing emitted ref def, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_full_to_inline_image() {
        let rule = rule_only_inline();
        let content = "Logo: ![alt][logo].\n\n[logo]: https://x.com/img.png\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("![alt](https://x.com/img.png)"));
    }

    // -------------------------------------------------------------------
    // Trivial reference inter-conversions
    // -------------------------------------------------------------------

    #[test]
    fn fix_collapsed_to_full() {
        // Allow only full of the reference styles.
        let rule = MD054LinkImageStyle::new(false, false, true, false, false, false);
        let content = "[docs][].\n\n[docs]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[docs][docs]"));
    }

    #[test]
    fn fix_shortcut_to_full() {
        let rule = MD054LinkImageStyle::new(false, false, true, false, false, false);
        let content = "See [docs].\n\n[docs]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("See [docs][docs]"));
    }

    #[test]
    fn fix_shortcut_to_collapsed() {
        let rule = MD054LinkImageStyle::new(false, true, false, false, false, false);
        let content = "See [docs].\n\n[docs]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("See [docs][]"));
    }

    // -------------------------------------------------------------------
    // autolink ↔ inline
    // -------------------------------------------------------------------

    #[test]
    fn fix_autolink_to_inline_form() {
        // Disallow autolink only; allow inline + url_inline (the default for the
        // remaining styles). An autolink's visible text is the URL, so the only
        // inline-shaped conversion is `[url](url)` — which classifies as
        // `url-inline`. Both must be allowed for a clean round-trip.
        let rule = MD054LinkImageStyle::new(false, true, true, true, true, true);
        let content = "Visit <https://example.com> today.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[https://example.com](https://example.com)"),
            "got: {fixed:?}"
        );
    }

    #[test]
    fn fix_autolink_to_full_when_inline_styles_disallowed() {
        // Disallow autolink + inline + url-inline; the only reachable target is
        // a reference style. The rule should fall through to `full` and emit a
        // generated ref def.
        let rule = MD054LinkImageStyle::new(false, true, true, false, true, false);
        let content = "Visit <https://example.com> today.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[https://example.com][https-example-com]"),
            "got: {fixed:?}"
        );
        assert!(fixed.contains("[https-example-com]: https://example.com"));
    }

    #[test]
    fn fix_url_inline_to_autolink() {
        // Disallow url-inline; autolink is the natural target when text==url.
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "Visit [https://example.com](https://example.com).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("<https://example.com>"));
    }

    // -------------------------------------------------------------------
    // No-op / unreachable-target cases
    // -------------------------------------------------------------------

    #[test]
    fn fix_no_op_when_target_unreachable() {
        // Disallow inline, allow ONLY autolink. The inline link's text doesn't
        // match its URL, so autolink is unreachable. The fix is a no-op.
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "See [docs](https://example.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
        // The warning is still produced.
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn fix_preserves_allowed_links() {
        let rule = rule_inline_disallowed();
        let content = "Already [ref][r] is fine.\n\n[r]: https://example.com\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert_eq!(fixed, content);
    }

    // -------------------------------------------------------------------
    // preferred_style override
    // -------------------------------------------------------------------

    #[test]
    fn fix_preferred_style_explicit_full() {
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Full),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](https://example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[docs][docs]"));
    }

    #[test]
    fn fix_inline_to_collapsed_emits_matching_ref_def() {
        // Disallow inline, prefer collapsed. Inline → collapsed must produce
        // `[anchor][]` AND emit a `[anchor]: url` definition so the resulting
        // link still resolves to the original URL.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Collapsed),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[anchor](https://example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[anchor][]"), "got:\n{fixed}");
        assert!(fixed.contains("[anchor]: https://example.com"), "got:\n{fixed}");
    }

    #[test]
    fn fix_inline_to_shortcut_emits_matching_ref_def() {
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Shortcut),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        // Trailing period guarantees the shortcut isn't followed by `[` or `(`.
        let content = "See [anchor](https://example.com).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[anchor]"), "got:\n{fixed}");
        assert!(!fixed.contains("[anchor]("), "shortcut form, not inline: {fixed}");
        assert!(fixed.contains("[anchor]: https://example.com"), "got:\n{fixed}");
    }

    #[test]
    fn fix_inline_to_collapsed_skips_empty_text() {
        // `[](url)` has no text — collapsed/shortcut emission would produce
        // `[][]` / `[]`, which CommonMark cannot parse as a link. The fix must
        // back off and leave the inline form intact.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Collapsed),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[](https://example.com)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "empty text must not collapse: {fixed}");
    }

    #[test]
    fn fix_inline_to_shortcut_skips_empty_text() {
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Shortcut),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "See [](https://example.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn fix_inline_to_collapsed_skips_text_with_brackets() {
        // Text containing literal `[` / `]` cannot be spliced into a label
        // without escaping; emit nothing rather than produce a broken link.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Collapsed),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "See [`a[0]` index](https://example.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "text containing `[` / `]` must not collapse: {fixed}");
    }

    #[test]
    fn fix_inline_to_full_url_with_space_uses_angle_brackets_in_def() {
        // A URL containing a space must be carried in the angle-bracket
        // destination form on both sides of the conversion. The source already
        // uses `<...>`; the appended ref def must do the same so the resulting
        // `[label]: <url with space>` line round-trips through CommonMark.
        let rule = rule_inline_disallowed();
        let content = "See [docs](<./has space.md>).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[docs]: <./has space.md>"),
            "ref def must wrap URL in angle brackets: {fixed}"
        );
    }

    #[test]
    fn fix_inline_to_full_url_with_unbalanced_paren_uses_angle_brackets_in_def() {
        // Unbalanced parens can't appear in the bare ref-def destination
        // either — the appended `[label]: url` must use angle-bracket form.
        let rule = rule_inline_disallowed();
        let content = "See [docs](<https://example.com/a)b>).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[docs]: <https://example.com/a)b>"),
            "ref def must wrap unbalanced-paren URL in angle brackets: {fixed}"
        );
    }

    #[test]
    fn fix_full_to_inline_preserves_backslash_unescaped_title() {
        // pulldown-cmark unescapes `\"` → `"` inside titles. The fix must use
        // pulldown-cmark's resolved title (not the regex-captured raw form),
        // and re-quote it appropriately when emitting the inline destination.
        let rule = rule_only_inline();
        let content = "See [docs][d].\n\n[d]: https://example.com \"He said \\\"hi\\\"\"\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Pulldown-cmark gives us the title `He said "hi"` (unescaped).
        // The serializer chooses parentheses since `\"` appears, but either way
        // the round-trip must reproduce the same logical title without losing
        // backslashes or quotes.
        assert!(fixed.contains("https://example.com"), "URL must round-trip: {fixed}");
        assert!(
            fixed.contains(r#"\"hi\""#) || fixed.contains(r#"He said "hi""#),
            "title must round-trip with quotes preserved: {fixed}"
        );
    }

    #[test]
    fn fix_full_to_inline_url_with_close_paren_uses_angle_brackets() {
        // Existing ref def points to a URL containing `)`. Splicing it into
        // an inline `[t](url)` destination would terminate the destination
        // early. The fix must use the angle-bracket form `<url)>`.
        let rule = rule_only_inline();
        let content = "See [t][r].\n\n[r]: <https://example.com/a)b>\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[t](<https://example.com/a)b>)"),
            "inline form must use angle brackets for `)` URLs: {fixed}"
        );
    }

    #[test]
    fn fix_inline_to_collapsed_skips_when_label_collides_with_different_url() {
        // Existing ref def for `anchor` points to a DIFFERENT URL. Converting
        // `[anchor](other.com)` to collapsed would produce a broken/wrong link
        // (CommonMark would resolve `[anchor][]` to the existing def). The fix
        // must back off rather than silently change the link target.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Collapsed),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[other][anchor]\n[anchor](https://other.com)\n\n[anchor]: https://existing.com\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Inline link is left alone (no safe conversion); the warning persists.
        assert!(fixed.contains("[anchor](https://other.com)"), "got:\n{fixed}");
        // The original `[anchor]` definition is unchanged.
        assert!(fixed.contains("[anchor]: https://existing.com"));
    }

    #[test]
    fn fix_preferred_style_list_picks_first_reachable() {
        // List `[autolink, full]`: an autolinkable URL must convert to autolink
        // because it appears first AND is reachable.
        let config = md054_config::MD054Config {
            url_inline: false,
            preferred_style: PreferredStyles::from_iter([PreferredStyle::Autolink, PreferredStyle::Full]),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[https://example.com](https://example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("<https://example.com>"),
            "expected autolink form, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_preferred_style_list_falls_back_to_next_when_first_unreachable() {
        // List `[autolink, full]`: URL is not autolinkable (relative), so the
        // first entry isn't reachable. Must fall back to the second (`full`).
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::from_iter([PreferredStyle::Autolink, PreferredStyle::Full]),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](./guide.md)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[docs][docs]"),
            "expected fallback to full, got:\n{fixed}"
        );
        assert!(
            fixed.contains("[docs]: ./guide.md"),
            "expected matching ref def, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_preferred_style_auto_in_list_acts_as_wildcard_fallback() {
        // `[autolink, auto]` for a non-autolinkable URL must fall through to
        // the source-aware Auto candidates (which for an inline source defaults
        // to `full`).
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::from_iter([PreferredStyle::Autolink, PreferredStyle::Auto]),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](./guide.md)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[docs][docs]"),
            "Auto fallback should pick full for inline-disallowed config, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_default_auto_prefers_autolink_for_url_inline_source() {
        // Source: `[url](url)` (url-inline). Disallow url-inline; default Auto.
        // Autolink must win over Full because `<url>` is the tightest form when
        // text equals the URL and the URL is autolinkable.
        let rule = MD054LinkImageStyle::new(true, true, true, true, true, false);
        let content = "[https://example.com](https://example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("<https://example.com>"),
            "expected autolink, got:\n{fixed}"
        );
        assert!(
            !fixed.contains("[https://example.com]["),
            "should not produce reference form when autolink is reachable, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_default_auto_falls_back_when_autolink_disallowed() {
        // Same shape as above but with autolink disallowed: must skip autolink
        // and pick the next Auto candidate (`full`).
        let rule = MD054LinkImageStyle::new(false, true, true, true, true, false);
        let content = "[https://example.com](https://example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[https://example.com][https-example-com]"),
            "expected full form, got:\n{fixed}"
        );
        assert!(
            fixed.contains("[https-example-com]: https://example.com"),
            "missing ref def, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_preferred_style_explicit_no_match_skips_fix() {
        // Single-entry list pinning a target that's neither allowed nor reachable
        // must produce no fix (warning persists, content unchanged).
        let config = md054_config::MD054Config {
            inline: false,
            // Pinning `Inline` for an Inline source — same style; not reachable.
            preferred_style: PreferredStyles::single(PreferredStyle::Inline),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](./guide.md)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "expected no-op fix, got:\n{fixed}");
    }

    // -------------------------------------------------------------------
    // Mixed / interaction scenarios
    // -------------------------------------------------------------------

    #[test]
    fn fix_mixes_inline_and_image_in_same_doc() {
        let rule = rule_inline_disallowed();
        let content = "Text [link](https://example.com) and ![pic](https://example.com/p.png).\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[link][link]"));
        assert!(fixed.contains("![pic][pic]"));
        assert!(fixed.contains("[link]: https://example.com"));
        assert!(fixed.contains("[pic]: https://example.com/p.png"));
    }

    #[test]
    fn fix_appends_one_blank_line_separator() {
        let rule = rule_inline_disallowed();
        let content = "Plain prose.\n\n[link](https://x.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Exactly one blank line between body and ref-def block.
        assert!(fixed.ends_with("\n[link]: https://x.com\n"));
        assert!(!fixed.contains("\n\n\n[link]"));
    }

    // -------------------------------------------------------------------
    // Overlapping edits / nested constructs
    // -------------------------------------------------------------------

    #[test]
    fn fix_nested_image_in_link_does_not_panic_or_corrupt() {
        // `[![alt](img.png)](https://x)` is an inline link whose text is itself
        // an inline image. The two spans overlap (the image lives entirely
        // inside the link span). With `inline = false`, both are flagged and
        // both produce candidate edits. Applying both edits to the same byte
        // range would corrupt the document — the planner must drop overlapping
        // edits.
        let rule = rule_inline_disallowed();
        let content = "See [![alt](img.png)](https://x.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        // Must not panic, even though both candidate edits overlap.
        let fixed = rule.fix(&ctx).unwrap();
        // Both candidate edits got dropped, so the doc is unchanged and the
        // warnings persist for the user to resolve manually.
        assert_eq!(fixed, content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2, "both nested constructs should still warn");
    }

    // -------------------------------------------------------------------
    // Email autolinks (CommonMark §6.5: bare email resolves to mailto:URL)
    // -------------------------------------------------------------------

    #[test]
    fn fix_email_autolink_to_inline_preserves_mailto_prefix() {
        // `<me@example.com>` is an email autolink. Per CommonMark §6.5 it
        // resolves to destination `mailto:me@example.com` while displaying the
        // bare email. Converting to any non-autolink form must preserve that
        // resolved destination — losing the `mailto:` prefix would silently
        // retarget the link to a relative path.
        let rule = MD054LinkImageStyle::new(false, true, true, true, true, true);
        let content = "Reach <me@example.com> for support.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("[me@example.com](mailto:me@example.com)"),
            "expected mailto: prefix on resolved destination, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_email_autolink_to_full_preserves_mailto_in_ref_def() {
        // Same invariant as the inline conversion, but routed through `full`
        // when inline is disallowed: the generated ref-def URL must be
        // `mailto:me@example.com`, not the bare email.
        let rule = MD054LinkImageStyle::new(false, true, true, false, true, false);
        let content = "Reach <me@example.com> for support.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            fixed.contains("]: mailto:me@example.com"),
            "ref def should carry the mailto: prefix, got:\n{fixed}"
        );
    }

    #[test]
    fn fix_rejects_bare_email_as_autolink_target() {
        // A `url-inline` link whose URL is a bare email must NOT be rewritten
        // to `<bare-email>`: that wraps the bare email in autolink syntax,
        // which the parser then resolves to `mailto:bare-email` — silently
        // changing the destination. The fix must fall through to a non-autolink
        // target instead.
        let config = md054_config::MD054Config {
            url_inline: false,
            preferred_style: PreferredStyles::from_iter([PreferredStyle::Autolink, PreferredStyle::Auto]),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[me@example.com](me@example.com)\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            !fixed.contains("<me@example.com>"),
            "bare-email autolink target would silently retarget to mailto:, got:\n{fixed}"
        );
    }

    // -------------------------------------------------------------------
    // Autolink target rejected when title would be lost
    // -------------------------------------------------------------------

    // -------------------------------------------------------------------
    // Generated ref defs round-trip through rumdl's own ref-def parser
    // -------------------------------------------------------------------

    #[test]
    fn fix_generated_ref_def_with_both_quote_types_round_trips_to_ctx() {
        // A title containing both `"` and `'` forces format_title onto the
        // paren-form CommonMark §4.7 delimiter. The generated ref def must
        // re-parse through rumdl's `parse_reference_defs` so downstream rules
        // (MD053 unused refs, MD057 link validity) still see the definition
        // — otherwise the URL silently disappears from `ctx.reference_defs`.
        let rule = rule_inline_disallowed();
        let content = "See [docs](https://example.com/x \"and 'both' quotes\") today.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        // Confirm the fixer chose the paren form (the only valid CommonMark
        // delimiter when both quote types appear in the title).
        assert!(
            fixed.contains("(and 'both' quotes)") || fixed.contains("\"and 'both' quotes\""),
            "title should round-trip through some valid delimiter, got:\n{fixed}"
        );
        // The crucial invariant: the generated ref def is visible to rumdl's
        // own parser. `assert_round_trip_clean` already checks the URL set
        // round-trips, but let's also pin the title down explicitly.
        let ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        let def = ctx
            .reference_defs
            .iter()
            .find(|d| d.url == "https://example.com/x")
            .expect("generated ref def must round-trip through parse_reference_defs");
        assert_eq!(
            def.title.as_deref(),
            Some("and 'both' quotes"),
            "title content must survive the round-trip"
        );
    }

    // -------------------------------------------------------------------
    // Generated ref-def block matches the document's line-ending style
    // -------------------------------------------------------------------

    #[test]
    fn fix_appends_generated_refs_with_crlf_when_source_is_crlf() {
        // A CRLF document must come back with CRLF endings — including the
        // separator and the per-ref lines we append. Otherwise `--fix` would
        // produce sequences like `\r\n\n[ref]: ...`, which `git diff` (and
        // any line-ending-strict tooling) flags as whole-file churn even
        // when only one link was rewritten.
        let rule = rule_inline_disallowed();
        let content = "See [docs](https://example.com/x).\r\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).expect("check must succeed");
        assert!(!warnings.is_empty(), "expected at least one warning");
        let fixed = rule.fix(&ctx).expect("fix must succeed");
        assert!(
            fixed.contains("\r\n"),
            "fixed output must preserve CRLF, got:\n{fixed:?}"
        );
        assert!(
            !fixed.lines().any(|l| l.ends_with('\r')) || !fixed.contains("\n\n"),
            "no line should end with stray \\r and there should be no naked LF blanks; got:\n{fixed:?}"
        );
        // The fixed buffer must not contain a naked LF (i.e. `\n` not preceded
        // by `\r`) anywhere — that would be the mixed-ending bug.
        let bytes = fixed.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                assert!(
                    i > 0 && bytes[i - 1] == b'\r',
                    "found naked LF at byte {i} in CRLF document, full output:\n{fixed:?}"
                );
            }
        }
    }

    #[test]
    fn fix_appends_generated_refs_with_lf_when_source_is_lf() {
        // Mirror of the CRLF test: an LF-only document must stay LF-only.
        let rule = rule_inline_disallowed();
        let content = "See [docs](https://example.com/x).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).expect("fix must succeed");
        assert!(
            !fixed.contains('\r'),
            "LF document must not gain any CR characters, got:\n{fixed:?}"
        );
    }

    // -------------------------------------------------------------------
    // Shortcut target rejected when follower would reparse the link
    // -------------------------------------------------------------------

    #[test]
    fn fix_rejects_shortcut_target_when_followed_by_paren() {
        // `[docs](url)(suffix)` is a disallowed inline link followed by literal
        // `(suffix)`. Naively rewriting the link to shortcut form yields
        // `[docs](suffix)`, which CommonMark reparses as an inline link with
        // destination `suffix` — silently retargeting to the wrong URL.
        // The planner must reject Shortcut for this source.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Shortcut),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](https://example.com/x)(suffix)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Shortcut target is unreachable; with no other allowed style chosen
        // explicitly, the fix is a no-op.
        assert_eq!(fixed, content, "shortcut target was unsafe; fix should be a no-op");
    }

    #[test]
    fn fix_rejects_shortcut_target_when_followed_by_bracket() {
        // `[docs](url)[next]` rewritten to `[docs][next]` would parse as a
        // full reference link with label `next` — completely different
        // semantics. Reject Shortcut for this case.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Shortcut),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[docs](https://example.com/x)[next]\n\n[next]: https://example.com/n\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "shortcut target was unsafe; fix should be a no-op");
    }

    #[test]
    fn fix_allows_shortcut_target_when_follower_is_safe() {
        // Sanity: when the follower is plain text (period, space, EOL), the
        // shortcut conversion is safe and proceeds normally. This guards
        // against an over-eager rejection that would block all shortcut fixes.
        let config = md054_config::MD054Config {
            inline: false,
            preferred_style: PreferredStyles::single(PreferredStyle::Shortcut),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "See [docs](https://example.com/x). Also nice.\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(fixed.contains("[docs]"), "expected shortcut form, got:\n{fixed}");
        assert!(fixed.contains("[docs]: https://example.com/x"));
    }

    // -------------------------------------------------------------------
    // Fix metadata is reachable through FixCoordinator
    // -------------------------------------------------------------------

    #[test]
    fn check_attaches_fix_for_self_contained_rewrites() {
        // For rewrites where the per-warning Fix carries the entire change
        // (no paired ref-def needed), check() must attach the Fix so editor
        // quick-fix paths (which apply only `warning.fix.range/replacement`)
        // produce a correct result. autolink → url-inline is fully encoded
        // in a single span replacement.
        let rule = MD054LinkImageStyle::new(false, true, true, true, true, true);
        let content = "See <https://example.com>.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "should warn about the autolink");
        let fix = warnings[0]
            .fix
            .as_ref()
            .expect("self-contained rewrite must carry a Fix so quick-fix paths can apply it");
        assert_eq!(&content[fix.range.clone()], "<https://example.com>");
        assert_eq!(fix.replacement, "[https://example.com](https://example.com)");
    }

    #[test]
    fn check_carries_atomic_fix_when_rewrite_requires_new_ref_def() {
        // inline → collapsed/full/shortcut requires appending `[label]: url`
        // at end-of-file. The per-warning Fix carries the in-place rewrite
        // as its primary edit and the EOF ref-def insertion as an
        // additional_edit, so quick-fix paths that apply a single warning
        // produce a complete, parseable result without relying on a follow-up
        // fix-all pass to materialize the definition.
        let rule = rule_inline_disallowed();
        let content = "See [docs](https://example.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "should warn about the inline link");
        let fix = warnings[0]
            .fix
            .as_ref()
            .expect("ref-emitting rewrite must carry an atomic per-warning Fix");
        assert_eq!(&content[fix.range.clone()], "[docs](https://example.com)");
        assert!(
            fix.replacement.starts_with("[docs]"),
            "primary replacement should rewrite the link to a reference form, got: {:?}",
            fix.replacement
        );
        assert_eq!(
            fix.additional_edits.len(),
            1,
            "ref-emitting fix should carry one additional_edit for the ref-def"
        );
        let extra = &fix.additional_edits[0];
        assert_eq!(
            extra.range,
            content.len()..content.len(),
            "ref-def insertion should be a zero-width edit at EOF"
        );
        assert!(
            extra.replacement.contains("[docs]: https://example.com"),
            "additional_edit should append the ref-def, got: {:?}",
            extra.replacement
        );
        // Applying the per-warning fix in isolation must yield the same shape
        // the whole-document fix() path produces: link rewritten to a
        // reference form AND the ref def appended at EOF.
        let applied = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        let from_fix_all = rule.fix(&ctx).unwrap();
        assert!(
            applied.contains("[docs]: https://example.com"),
            "single-warning application must include ref-def, got:\n{applied}"
        );
        assert!(
            !applied.contains("[docs](https://example.com)"),
            "single-warning application must rewrite the inline link, got:\n{applied}"
        );
        // Both paths must still drive the document into a stable, fixed shape
        // — but exact equality isn't required because per-warning EOF
        // insertions don't deduplicate trailing newlines the way the
        // whole-document apply() does.
        assert!(
            from_fix_all.contains("[docs]: https://example.com"),
            "fix-all path must also produce the ref-def, got:\n{from_fix_all}"
        );
    }

    #[test]
    fn check_attaches_no_fix_when_target_unreachable() {
        // When no allowed style is reachable, no edit is produced — so the
        // warning carries no Fix and the coordinator skips fix-all for it.
        // This avoids advertising an "automatic fix" the user can't actually
        // accept.
        let rule = MD054LinkImageStyle::new(true, false, false, false, false, false);
        let content = "See [docs](https://example.com).\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_none(), "unreachable target should leave fix empty");
    }

    #[test]
    fn fix_skips_autolink_target_when_title_present() {
        // Autolink syntax has no slot for a title. Rewriting
        // `[url](url "title")` to `<url>` would silently drop the title text,
        // so the planner must reject Autolink as a target and fall through to
        // a reference style.
        let config = md054_config::MD054Config {
            url_inline: false,
            preferred_style: PreferredStyles::from_iter([PreferredStyle::Autolink, PreferredStyle::Auto]),
            ..Default::default()
        };
        let rule = MD054LinkImageStyle::from_config_struct(config);
        let content = "[https://example.com](https://example.com \"Homepage\")\n";
        let fixed = assert_round_trip_clean(&rule, content);
        assert!(
            !fixed.contains("<https://example.com>"),
            "autolink target would drop the title, got:\n{fixed}"
        );
        assert!(
            fixed.contains("\"Homepage\""),
            "title text must survive the conversion, got:\n{fixed}"
        );
    }

    #[test]
    fn default_config_section_emits_clean_user_facing_defaults() {
        // `rumdl config --defaults` reads `default_config_section()` and prints the
        // values verbatim. The polymorphic sentinel is a schema-only marker — it
        // must never appear in user-facing output, otherwise the documented default
        // table contains a placeholder string the user can't actually paste back.
        let rule = MD054LinkImageStyle::default();
        let (_, value) = rule.default_config_section().expect("md054 has defaults");
        let table = value.as_table().expect("config section is a table");
        let preferred = table
            .get("preferred-style")
            .expect("preferred-style key must be present in defaults");
        assert!(
            !crate::rule_config_serde::is_polymorphic_sentinel(preferred),
            "preferred-style in user-facing defaults must be the serialized scalar, not the sentinel; got {preferred:?}"
        );
        // The serialized default of a single-element PreferredStyles collapses to a
        // scalar string. Verify the actual shape so a future serde change is caught.
        assert!(
            preferred.is_str(),
            "preferred-style default should serialize as a scalar string; got {preferred:?}"
        );
    }

    #[test]
    fn registry_marks_preferred_style_polymorphic_for_validation() {
        // The schema view (consumed by the validator) must carry the sentinel so
        // the alternative list form of `preferred-style` is accepted alongside the
        // serialized scalar default. This is the counterpart to
        // `default_config_section_emits_clean_user_facing_defaults`: the same key
        // looks different in the two views, by design.
        let registry = crate::config::registry::default_registry();
        let expected = registry
            .expected_value_for("MD054", "preferred-style")
            .or_else(|| registry.expected_value_for("MD054", "preferred_style"));
        // `expected_value_for` returns None precisely when the entry was filtered
        // as a sentinel — that's the contract the validator uses to skip type
        // checking. Any other return value would reintroduce the original bug
        // where the list form is rejected.
        assert!(
            expected.is_none(),
            "preferred-style must be sentinel-marked in the schema so type checking is skipped; got {expected:?}"
        );
        // Sanity check: the key is still recognized as valid (only the type check
        // is skipped, not the key-name check).
        let keys = registry.config_keys_for("MD054").expect("md054 must be registered");
        assert!(keys.contains("preferred-style"));
    }
}
