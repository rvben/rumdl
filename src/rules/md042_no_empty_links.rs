use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::mkdocs_patterns::is_mkdocs_auto_reference;

/// Rule MD042: No empty links
///
/// See [docs/md042.md](../../docs/md042.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link has no destination (URL).
/// "Empty links do not lead anywhere and therefore don't function as links."
///
/// Note: Empty TEXT with a valid URL (e.g., `[](url)`) is NOT flagged by MD042.
/// While this may be an accessibility concern, it's not an "empty link" per se.
///
/// # MkDocs Support
///
/// When `flavor = "mkdocs"` is configured, this rule recognizes two types of valid MkDocs patterns:
///
/// ## 1. Auto-References (via mkdocs-autorefs / mkdocstrings)
///
/// Backtick-wrapped Python identifiers used for cross-referencing:
/// ```markdown
/// [`module.Class`][]     // Python class reference
/// [`str`][]              // Built-in type reference
/// [`api.function`][]     // Function reference
/// ```
///
/// **References:**
/// - [mkdocs-autorefs](https://mkdocstrings.github.io/autorefs/)
/// - [mkdocstrings](https://mkdocstrings.github.io/)
///
/// ## 2. Paragraph Anchors (via Python-Markdown attr_list extension)
///
/// Empty links combined with attributes to create anchor points:
/// ```markdown
/// [](){ #my-anchor }              // Basic anchor
/// [](){ #anchor .class }          // Anchor with CSS class
/// [](){: #anchor }                // With colon (canonical attr_list syntax)
/// [](){ .class1 .class2 }         // Classes only
/// ```
///
/// This syntax combines:
/// - Empty link `[]()` → creates `<a href=""></a>`
/// - attr_list syntax `{ #id }` → adds attributes to preceding element
/// - Result: `<a href="" id="my-anchor"></a>`
///
/// **References:**
/// - [Python-Markdown attr_list](https://python-markdown.github.io/extensions/attr_list/)
/// - [MkDocs discussion](https://github.com/mkdocs/mkdocs/discussions/3754)
///
/// **Implementation:** See [`is_mkdocs_attribute_anchor`](Self::is_mkdocs_attribute_anchor)
#[derive(Clone, Default)]
pub struct MD042NoEmptyLinks {}

impl MD042NoEmptyLinks {
    pub fn new() -> Self {
        Self {}
    }

    /// Strip surrounding backticks from a string
    /// Used for MkDocs auto-reference detection where `module.Class` should be treated as module.Class
    fn strip_backticks(s: &str) -> &str {
        s.trim_start_matches('`').trim_end_matches('`')
    }

    /// Check if a string is a valid Python identifier
    /// Python identifiers can contain alphanumeric characters and underscores, but cannot start with a digit
    fn is_valid_python_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let first_char = s.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return false;
        }

        s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Check if an empty link is followed by MkDocs attribute syntax
    /// Pattern: []() followed by { #anchor } or { #anchor .class }
    ///
    /// This validates the Python-Markdown attr_list extension syntax when applied to empty links.
    /// Empty links `[]()` combined with attributes like `{ #anchor }` create anchor points in
    /// documentation, as documented by mkdocs-autorefs and the attr_list extension.
    fn is_mkdocs_attribute_anchor(content: &str, link_end: usize) -> bool {
        // UTF-8 safety: Validate byte position is at character boundary
        if !content.is_char_boundary(link_end) {
            return false;
        }

        // Get the content after the link
        if let Some(rest) = content.get(link_end..) {
            // Trim whitespace and check if it starts with {
            // Note: trim_start() removes all whitespace including newlines
            // This is intentionally permissive to match real-world MkDocs usage
            let trimmed = rest.trim_start();

            // Check for opening brace (with optional colon per attr_list spec)
            let stripped = if let Some(s) = trimmed.strip_prefix("{:") {
                s
            } else if let Some(s) = trimmed.strip_prefix('{') {
                s
            } else {
                return false;
            };

            // Look for closing brace
            if let Some(end_brace) = stripped.find('}') {
                // DoS prevention: Limit attribute section length
                if end_brace > 500 {
                    return false;
                }

                let attrs = stripped[..end_brace].trim();

                // Empty attributes should not be considered valid
                if attrs.is_empty() {
                    return false;
                }

                // Check if it contains an anchor (starts with #) or class (starts with .)
                // Valid patterns: { #anchor }, { #anchor .class }, { .class #anchor }
                // At least one attribute starting with # or . is required
                return attrs
                    .split_whitespace()
                    .any(|part| part.starts_with('#') || part.starts_with('.'));
            }
        }
        false
    }
}

impl Rule for MD042NoEmptyLinks {
    fn name(&self) -> &'static str {
        "MD042"
    }

    fn description(&self) -> &'static str {
        "No empty links"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Check if we're in MkDocs mode from the context
        let mkdocs_mode = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;
        let quarto_mode = ctx.flavor == crate::config::MarkdownFlavor::Quarto;

        // Use centralized link parsing from LintContext
        for link in &ctx.links {
            // Skip links inside Jinja templates
            if ctx.is_in_jinja_range(link.byte_offset) {
                continue;
            }

            // Skip Quarto/Pandoc citations ([@citation], @citation)
            // Citations look like reference links but are bibliography references
            if quarto_mode && ctx.is_in_citation(link.byte_offset) {
                continue;
            }

            // Skip links inside shortcodes ({{< ... >}} or {{% ... %}})
            // Shortcodes may contain template syntax that looks like links
            if ctx.is_in_shortcode(link.byte_offset) {
                continue;
            }

            // Skip links inside HTML tags (e.g., <a href="...?p[images][0]=...">)
            // Check if the link's byte position falls within any HTML tag range
            let in_html_tag = ctx
                .html_tags()
                .iter()
                .any(|html_tag| html_tag.byte_offset <= link.byte_offset && link.byte_offset < html_tag.byte_end);
            if in_html_tag {
                continue;
            }

            // For reference links with defined references, we don't flag them as empty
            // even if the URL happens to be missing. Undefined references are handled by MD052.
            // MD042 only flags:
            // - Empty text: `[][ref]`, `[](url)`
            // - Empty URL in inline links: `[text]()`
            // NOT: `[text][undefined]` (that's MD052's job)
            let (effective_url, is_undefined_reference): (&str, bool) = if link.is_reference {
                if let Some(ref_id) = &link.reference_id {
                    match ctx.get_reference_url(ref_id.as_ref()) {
                        Some(url) => (url, false),
                        None => ("", true), // Mark as undefined reference
                    }
                } else {
                    ("", false) // Empty reference like `[][]`
                }
            } else {
                (&link.url, false)
            };

            // For MkDocs mode, check if this looks like an auto-reference
            // Note: We check both the reference_id AND the text since shorthand references
            // like [class.Name][] use the text as the implicit reference
            // Also strip backticks since MkDocs resolves `module.Class` as module.Class
            if mkdocs_mode && link.is_reference {
                // Check the reference_id if present (strip backticks first)
                if let Some(ref_id) = &link.reference_id {
                    let stripped_ref = Self::strip_backticks(ref_id);
                    // Accept if it matches MkDocs patterns OR if it's a backtick-wrapped valid identifier
                    // Backticks indicate code/type reference (like `str`, `int`, `MyClass`)
                    if is_mkdocs_auto_reference(stripped_ref)
                        || (ref_id != stripped_ref && Self::is_valid_python_identifier(stripped_ref))
                    {
                        continue;
                    }
                }
                // Also check the link text itself for shorthand references (strip backticks)
                let stripped_text = Self::strip_backticks(&link.text);
                // Accept if it matches MkDocs patterns OR if it's a backtick-wrapped valid identifier
                if is_mkdocs_auto_reference(stripped_text)
                    || (link.text.as_ref() != stripped_text && Self::is_valid_python_identifier(stripped_text))
                {
                    continue;
                }
            }

            // Skip autolinks (like <https://example.com>)
            // Autolinks are valid CommonMark syntax: <URL> where text field is empty but URL is the display
            // Detect by checking if source markdown is wrapped in < and >
            let link_markdown = &ctx.content[link.byte_offset..link.byte_end];
            if link_markdown.starts_with('<') && link_markdown.ends_with('>') {
                continue;
            }

            // Skip wiki-style links (Obsidian/Notion syntax: [[Page Name]] or [[Page|Display]])
            // Wiki links are valid syntax and should never be flagged as "empty links".
            // This covers all wiki link patterns including:
            // - Basic: [[Page Name]]
            // - With path: [[Folder/Page]]
            // - With alias: [[Page|Display Text]]
            // - With heading: [[Page#heading]]
            // - Block references: [[Page#^block-id]] or [[#^block-id]]
            //
            // Detection: pulldown-cmark captures [[Example] as bytes 0..10, with trailing ] at byte 10
            // We check: starts with "[[" AND the char after byte_end is "]"
            if link_markdown.starts_with("[[")
                && link_markdown.ends_with(']')
                && ctx.content.as_bytes().get(link.byte_end) == Some(&b']')
            {
                continue;
            }

            // Skip undefined references - those are handled by MD052, not MD042
            // MD042 is only for truly empty links, not missing reference definitions
            if is_undefined_reference && !link.text.trim().is_empty() {
                continue;
            }

            // Check for empty destination (URL) only
            // MD042 is about links that "do not lead anywhere" - focusing on empty destinations
            // Empty text with valid URL is NOT flagged (that's an accessibility concern, not "empty link")
            if effective_url.trim().is_empty() {
                // In MkDocs mode, check if this is an attribute anchor: []() followed by { #anchor }
                if mkdocs_mode
                    && link.text.trim().is_empty()
                    && Self::is_mkdocs_attribute_anchor(ctx.content, link.byte_end)
                {
                    // This is a valid MkDocs attribute anchor, skip it
                    continue;
                }

                // Determine if we can provide a meaningful fix
                // Check if the link text looks like a URL - if so, use it as the destination
                let replacement = if !link.text.trim().is_empty() {
                    let text_is_url = link.text.starts_with("http://")
                        || link.text.starts_with("https://")
                        || link.text.starts_with("ftp://")
                        || link.text.starts_with("ftps://");

                    if text_is_url {
                        Some(format!("[{}]({})", link.text, link.text))
                    } else {
                        // Text is not a URL - can't meaningfully auto-fix
                        None
                    }
                } else {
                    // Both empty - can't meaningfully auto-fix
                    None
                };

                // Extract the exact link text from the source
                let link_display = &ctx.content[link.byte_offset..link.byte_end];

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Empty link found: {link_display}"),
                    line: link.line,
                    column: link.start_col + 1, // Convert to 1-indexed
                    end_line: link.line,
                    end_column: link.end_col + 1, // Convert to 1-indexed
                    severity: Severity::Error,
                    fix: replacement.map(|r| Fix {
                        range: link.byte_offset..link.byte_end,
                        replacement: r,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Get all warnings first - only fix links that are actually flagged
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all fixes with their ranges
        let mut fixes: Vec<(std::ops::Range<usize>, String)> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.clone(), f.replacement.clone())))
            .collect();

        // Sort fixes by position (descending) to apply from end to start
        fixes.sort_by(|a, b| b.0.start.cmp(&a.0.start));

        let mut result = content.to_string();

        // Apply fixes from end to start to maintain correct positions
        for (range, replacement) in fixes {
            result.replace_range(range, &replacement);
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Flavor is now accessed from LintContext during check
        Box::new(MD042NoEmptyLinks::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_links_with_text_should_pass() {
        let ctx = LintContext::new(
            "[valid link](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text should pass");

        let ctx = LintContext::new(
            "[another valid link](path/to/page.html)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text and relative URLs should pass");
    }

    #[test]
    fn test_links_with_empty_text_but_valid_url_pass() {
        // MD042 only flags empty URLs, not empty text
        // "Empty links do not lead anywhere" - these links DO lead somewhere
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty text with valid URL should NOT be flagged by MD042. Got: {result:?}"
        );
    }

    #[test]
    fn test_links_with_only_whitespace_but_valid_url_pass() {
        // MD042 only flags empty URLs, not empty/whitespace text
        let ctx = LintContext::new(
            "[   ](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Whitespace text with valid URL should NOT be flagged. Got: {result:?}"
        );

        let ctx = LintContext::new(
            "[\t\n](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Whitespace text with valid URL should NOT be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_reference_links_with_empty_text_but_valid_ref() {
        // Empty text with valid reference (has URL) should NOT be flagged
        // MD042 only flags empty URLs, not empty text
        let ctx = LintContext::new(
            "[][ref]\n\n[ref]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty text with valid reference should NOT be flagged. Got: {result:?}"
        );

        // Note: `[]:` (empty reference label) is NOT valid CommonMark
        // So we don't test that case - empty labels are not supported
    }

    #[test]
    fn test_images_should_be_ignored() {
        // Images can have empty alt text, so they should not trigger the rule
        let ctx = LintContext::new("![](image.png)", crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with empty alt text should be ignored");

        let ctx = LintContext::new("![   ](image.png)", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with whitespace alt text should be ignored");
    }

    #[test]
    fn test_links_with_nested_formatting() {
        // MD042 only flags empty URLs - all of these have valid URLs so they pass
        let rule = MD042NoEmptyLinks::new();

        // [**] contains "**" as text, has URL → pass
        let ctx = LintContext::new(
            "[**](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[**](url) has URL so should pass");

        // [__] contains "__" as text, has URL → pass
        let ctx = LintContext::new(
            "[__](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[__](url) has URL so should pass");

        // [](url) - empty text but has URL → pass (per markdownlint behavior)
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[](url) has URL so should pass");

        // [**bold text**](url) - has text and URL → pass
        let ctx = LintContext::new(
            "[**bold text**](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with nested formatting and text should pass");

        // [*italic* and **bold**](url) - has text and URL → pass
        let ctx = LintContext::new(
            "[*italic* and **bold**](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with multiple nested formatting should pass");
    }

    #[test]
    fn test_multiple_empty_links_on_same_line() {
        // MD042 only flags empty URLs - all these have URLs so they pass
        let ctx = LintContext::new(
            "[](url1) and [](url2) and [valid](url3)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty text with valid URL should NOT be flagged. Got: {result:?}"
        );

        // Test multiple truly empty links (empty URL)
        let ctx = LintContext::new(
            "[text1]() and [text2]() and [text3](url)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should detect both empty URL links");
        assert_eq!(result[0].column, 1); // [text1]()
        assert_eq!(result[1].column, 15); // [text2]()
    }

    #[test]
    fn test_escaped_brackets() {
        // Escaped brackets should not be treated as links
        let ctx = LintContext::new(
            "\\[\\](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Escaped brackets should not be treated as links");

        // But this should still be a link
        let ctx = LintContext::new(
            "[\\[\\]](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Link with escaped brackets in text should pass");
    }

    #[test]
    fn test_links_in_lists_and_blockquotes() {
        // MD042 only flags empty URLs - [](url) has URL so it passes
        let rule = MD042NoEmptyLinks::new();

        // Empty text with URL in lists - passes (has URL)
        let ctx = LintContext::new(
            "- [](https://example.com)\n- [valid](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[](url) in lists should pass");

        // Empty text with URL in blockquotes - passes (has URL)
        let ctx = LintContext::new(
            "> [](https://example.com)\n> [valid](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[](url) in blockquotes should pass");

        // Empty URL in lists - FAILS (no URL)
        let ctx = LintContext::new(
            "- [text]()\n- [valid](url)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty URL should be flagged");
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_unicode_whitespace_characters() {
        // MD042 only flags empty URLs - all these have URLs so they pass
        // regardless of the text content (whitespace or not)
        let rule = MD042NoEmptyLinks::new();

        // Non-breaking space (U+00A0) - has URL, passes
        let ctx = LintContext::new(
            "[\u{00A0}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Has URL, should pass regardless of text");

        // Em space (U+2003) - has URL, passes
        let ctx = LintContext::new(
            "[\u{2003}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Has URL, should pass regardless of text");

        // Zero-width space (U+200B) - has URL, passes
        let ctx = LintContext::new(
            "[\u{200B}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Has URL, should pass regardless of text");

        // Test with zero-width space between spaces - has URL, passes
        let ctx = LintContext::new(
            "[ \u{200B} ](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Has URL, should pass regardless of text");
    }

    #[test]
    fn test_empty_url_with_text() {
        let ctx = LintContext::new("[some text]()", crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [some text]()");
    }

    #[test]
    fn test_both_empty_text_and_url() {
        let ctx = LintContext::new("[]()", crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: []()");
    }

    #[test]
    fn test_reference_link_with_undefined_reference() {
        // Undefined references are handled by MD052, not MD042
        // MD042 should NOT flag [text][undefined] - it's not an "empty link"
        let ctx = LintContext::new("[text][undefined]", crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD042 should NOT flag [text][undefined] - undefined refs are MD052's job. Got: {result:?}"
        );

        // But empty text with undefined reference SHOULD be flagged
        let ctx = LintContext::new("[][undefined]", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty text in reference link should still be flagged");
    }

    #[test]
    fn test_shortcut_reference_links() {
        // Valid shortcut reference link (implicit reference)
        // Note: [example] by itself is not parsed as a link by the LINK_PATTERN regex
        // It needs to be followed by [] or () to be recognized as a link
        let ctx = LintContext::new(
            "[example][]\n\n[example]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid implicit reference link should pass");

        // Note: `[]:` (empty reference label) is NOT valid CommonMark
        // Empty labels are not supported, so we don't test `[][]\n\n[]: url`

        // Test actual shortcut-style links are not detected (since they don't match the pattern)
        let ctx = LintContext::new(
            "[example]\n\n[example]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Shortcut links without [] or () are not parsed as links"
        );
    }

    #[test]
    fn test_fix_suggestions() {
        // MD042 only flags empty URLs now
        let rule = MD042NoEmptyLinks::new();

        // Case 1: Empty text, has URL - NOT flagged (has URL)
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Empty text with URL should NOT be flagged");

        // Case 2: Non-URL text, empty URL - flagged, NOT fixable (can't guess the URL)
        let ctx = LintContext::new("[text]()", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty URL should be flagged");
        assert!(
            result[0].fix.is_none(),
            "Non-URL text with empty URL should NOT be fixable"
        );

        // Case 3: URL text, empty URL - flagged, fixable (use text as URL)
        let ctx = LintContext::new("[https://example.com]()", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty URL should be flagged");
        assert!(result[0].fix.is_some(), "URL text with empty URL should be fixable");
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[https://example.com](https://example.com)");

        // Case 4: Both empty - flagged, NOT fixable (can't guess either)
        let ctx = LintContext::new("[]()", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty URL should be flagged");
        assert!(result[0].fix.is_none(), "Both empty should NOT be fixable");
    }

    #[test]
    fn test_complex_markdown_document() {
        // MD042 only flags empty URLs - not empty text
        let content = r#"# Document with various links

[Valid link](https://example.com) followed by [](empty.com).

## Lists with links
- [Good link](url1)
- [](url2)
- Item with [inline empty]() link

> Quote with [](quoted-empty.com)
> And [valid quoted](quoted-valid.com)

Code block should be ignored:
```
[](this-is-code)
```

[Reference style][ref1] and [][ref2]

[ref1]: https://ref1.com
[ref2]: https://ref2.com
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();

        // Only [inline empty]() on line 9 has empty URL - should be the only one flagged
        // All [](url) patterns have URLs so they're NOT flagged
        // [][ref2] has a valid reference so it's NOT flagged
        assert_eq!(result.len(), 1, "Should only flag empty URL links. Got: {result:?}");
        assert_eq!(result[0].line, 8, "Only [inline empty]() should be flagged");
        assert!(result[0].message.contains("[inline empty]()"));
    }

    #[test]
    fn test_issue_29_code_block_with_tildes() {
        // Test for issue #29 - code blocks with tilde markers should not break reference links
        let content = r#"In addition to the [local scope][] and the [global scope][], Python also has a **built-in scope**.

```pycon
>>> @count_calls
... def greet(name):
...     print("Hi", name)
...
>>> greet("Trey")
Traceback (most recent call last):
  File "<python-input-2>", line 1, in <module>
    greet("Trey")
    ~~~~~^^^^^^^^
  File "<python-input-0>", line 4, in wrapper
    calls += 1
    ^^^^^
UnboundLocalError: cannot access local variable 'calls' where it is not associated with a value
```


[local scope]: https://www.pythonmorsels.com/local-and-global-variables/
[global scope]: https://www.pythonmorsels.com/assigning-global-variables/"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();

        // These reference links should NOT be flagged as empty
        assert!(
            result.is_empty(),
            "Should not flag reference links as empty when code blocks contain tildes (issue #29). Got: {result:?}"
        );
    }

    #[test]
    fn test_link_with_inline_code_in_text() {
        // Links with inline code in the text should NOT be flagged as empty
        let ctx = LintContext::new(
            "[`#[derive(Serialize, Deserialize)`](https://serde.rs/derive.html)",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Links with inline code should not be flagged as empty. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_backtick_wrapped_references() {
        // Test for issue #97 - backtick-wrapped references should be recognized as MkDocs auto-references
        let rule = MD042NoEmptyLinks::new();

        // Module.Class pattern with backticks
        let ctx = LintContext::new("[`module.Class`][]", crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`module.Class`][] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // Reference with explicit ID
        let ctx = LintContext::new("[`module.Class`][ref]", crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`module.Class`][ref] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // Path-like reference with backticks
        let ctx = LintContext::new("[`api/endpoint`][]", crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`api/endpoint`][] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // In standard mode, undefined collapsed references are handled by MD052, not MD042
        // MD042 only flags truly empty links, not undefined references
        let ctx = LintContext::new("[`module.Class`][]", crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD042 should NOT flag [`module.Class`][] - undefined refs are MD052's job. Got: {result:?}"
        );

        // Should still flag truly empty links even in MkDocs mode
        let ctx = LintContext::new("[][]", crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should still flag [][] as empty in MkDocs mode. Got: {result:?}"
        );
    }
}
