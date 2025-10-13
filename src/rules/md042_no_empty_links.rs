use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::mkdocs_patterns::is_mkdocs_auto_reference;

/// Rule MD042: No empty links
///
/// See [docs/md042.md](../../docs/md042.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link has no content (text) or destination (URL).
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

        // Use centralized link parsing from LintContext
        for link in &ctx.links {
            // For reference links, resolve the URL
            let effective_url = if link.is_reference {
                if let Some(ref_id) = &link.reference_id {
                    ctx.get_reference_url(ref_id).unwrap_or("").to_string()
                } else {
                    String::new()
                }
            } else {
                link.url.clone()
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
                    || (link.text.as_str() != stripped_text && Self::is_valid_python_identifier(stripped_text))
                {
                    continue;
                }
            }

            // Check for empty links
            if link.text.trim().is_empty() || effective_url.trim().is_empty() {
                // In MkDocs mode, check if this is an attribute anchor: []() followed by { #anchor }
                if mkdocs_mode
                    && link.text.trim().is_empty()
                    && effective_url.trim().is_empty()
                    && Self::is_mkdocs_attribute_anchor(ctx.content, link.byte_end)
                {
                    // This is a valid MkDocs attribute anchor, skip it
                    continue;
                }

                // Determine if we can provide a meaningful fix
                let replacement = if link.text.trim().is_empty() {
                    // Empty text - can we fix it?
                    if !effective_url.trim().is_empty() {
                        // Has URL but no text - add placeholder text
                        if link.is_reference {
                            Some(format!(
                                "[Link text]{}",
                                &ctx.content[link.byte_offset + 1..link.byte_end]
                            ))
                        } else {
                            Some(format!("[Link text]({effective_url})"))
                        }
                    } else {
                        // Both empty - can't meaningfully auto-fix
                        None
                    }
                } else if link.is_reference {
                    // Reference links with text but no/empty reference - keep the format
                    let ref_part = &ctx.content[link.byte_offset + link.text.len() + 2..link.byte_end];
                    Some(format!("[{}]{}", link.text, ref_part))
                } else {
                    // URL is empty, but text is not
                    // Check if the link text looks like a URL - if so, use it as the destination
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
                };

                // Extract the exact link text from the source
                let link_display = &ctx.content[link.byte_offset..link.byte_end];

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Empty link found: {link_display}"),
                    line: link.line,
                    column: link.start_col + 1, // Convert to 1-indexed
                    end_line: link.line,
                    end_column: link.end_col + 1, // Convert to 1-indexed
                    severity: Severity::Warning,
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
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text should pass");

        let ctx = LintContext::new(
            "[another valid link](path/to/page.html)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text and relative URLs should pass");
    }

    #[test]
    fn test_links_with_empty_text_should_fail() {
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [](https://example.com)");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_links_with_only_whitespace_should_fail() {
        let ctx = LintContext::new("[   ](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [   ](https://example.com)");

        let ctx = LintContext::new("[\t\n](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [\t\n](https://example.com)");
    }

    #[test]
    fn test_reference_links_with_empty_text() {
        let ctx = LintContext::new(
            "[][ref]\n\n[ref]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [][ref]");
        assert_eq!(result[0].line, 1);

        // Empty text with empty reference
        let ctx = LintContext::new(
            "[][]\n\n[]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_images_should_be_ignored() {
        // Images can have empty alt text, so they should not trigger the rule
        let ctx = LintContext::new("![](image.png)", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with empty alt text should be ignored");

        let ctx = LintContext::new("![   ](image.png)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with whitespace alt text should be ignored");
    }

    #[test]
    fn test_links_with_nested_formatting() {
        // Links with nested formatting but empty effective text
        // Note: [**] contains "**" as text, which is not empty after trimming
        let ctx = LintContext::new("[**](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[**] is not considered empty since ** is text");

        let ctx = LintContext::new("[__](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[__] is not considered empty since __ is text");

        // Links with truly empty formatting should fail
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // Links with nested formatting and actual text should pass
        let ctx = LintContext::new(
            "[**bold text**](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with nested formatting and text should pass");

        let ctx = LintContext::new(
            "[*italic* and **bold**](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with multiple nested formatting should pass");
    }

    #[test]
    fn test_multiple_empty_links_on_same_line() {
        let ctx = LintContext::new(
            "[](url1) and [](url2) and [valid](url3)",
            crate::config::MarkdownFlavor::Standard,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should detect both empty links");
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].column, 14);
    }

    #[test]
    fn test_escaped_brackets() {
        // Escaped brackets should not be treated as links
        let ctx = LintContext::new("\\[\\](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Escaped brackets should not be treated as links");

        // But this should still be a link
        let ctx = LintContext::new("[\\[\\]](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Link with escaped brackets in text should pass");
    }

    #[test]
    fn test_links_in_lists_and_blockquotes() {
        // Empty links in lists
        let ctx = LintContext::new(
            "- [](https://example.com)\n- [valid](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);

        // Empty links in blockquotes
        let ctx = LintContext::new(
            "> [](https://example.com)\n> [valid](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);

        // Nested structures
        let ctx = LintContext::new(
            "> - [](url1)\n> - [text](url2)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_unicode_whitespace_characters() {
        // Non-breaking space (U+00A0) - IS considered whitespace by Rust's trim()
        let ctx = LintContext::new(
            "[\u{00A0}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Non-breaking space should be treated as whitespace");

        // Em space (U+2003) - IS considered whitespace by Rust's trim()
        let ctx = LintContext::new(
            "[\u{2003}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Em space should be treated as whitespace");

        // Zero-width space (U+200B) - NOT considered whitespace by Rust's trim()
        // This is a formatting character, not a whitespace character
        let ctx = LintContext::new(
            "[\u{200B}](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Zero-width space is not considered whitespace by trim()"
        );

        // Test with zero-width space between spaces
        // Since trim() doesn't consider zero-width space as whitespace,
        // " \u{200B} " becomes "\u{200B}" after trimming, which is NOT empty
        let ctx = LintContext::new(
            "[ \u{200B} ](https://example.com)",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Zero-width space remains after trim(), so link is not empty"
        );
    }

    #[test]
    fn test_empty_url_with_text() {
        let ctx = LintContext::new("[some text]()", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [some text]()");
    }

    #[test]
    fn test_both_empty_text_and_url() {
        let ctx = LintContext::new("[]()", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: []()");
    }

    #[test]
    fn test_reference_link_with_undefined_reference() {
        let ctx = LintContext::new("[text][undefined]", crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Undefined reference should be treated as empty URL");
    }

    #[test]
    fn test_shortcut_reference_links() {
        // Valid shortcut reference link (implicit reference)
        // Note: [example] by itself is not parsed as a link by the LINK_PATTERN regex
        // It needs to be followed by [] or () to be recognized as a link
        let ctx = LintContext::new(
            "[example][]\n\n[example]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
        );
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid implicit reference link should pass");

        // Empty implicit reference link
        let ctx = LintContext::new(
            "[][]\n\n[]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty implicit reference link should fail");

        // Test actual shortcut-style links are not detected (since they don't match the pattern)
        let ctx = LintContext::new(
            "[example]\n\n[example]: https://example.com",
            crate::config::MarkdownFlavor::Standard,
        );
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Shortcut links without [] or () are not parsed as links"
        );
    }

    #[test]
    fn test_fix_suggestions() {
        let rule = MD042NoEmptyLinks::new();

        // Case 1: Empty text, has URL - fixable (add placeholder text)
        let ctx = LintContext::new("[](https://example.com)", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_some(), "Empty text with URL should be fixable");
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[Link text](https://example.com)");

        // Case 2: Non-URL text, empty URL - NOT fixable (can't guess the URL)
        let ctx = LintContext::new("[text]()", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result[0].fix.is_none(),
            "Non-URL text with empty URL should NOT be fixable"
        );

        // Case 3: URL text, empty URL - fixable (use text as URL)
        let ctx = LintContext::new("[https://example.com]()", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_some(), "URL text with empty URL should be fixable");
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[https://example.com](https://example.com)");

        // Case 4: Both empty - NOT fixable (can't guess either)
        let ctx = LintContext::new("[]()", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_none(), "Both empty should NOT be fixable");
    }

    #[test]
    fn test_complex_markdown_document() {
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();

        // Count the empty links
        let empty_link_lines = [3, 7, 8, 10, 18];
        assert_eq!(result.len(), empty_link_lines.len(), "Should find all empty links");

        // Verify line numbers
        for (i, &expected_line) in empty_link_lines.iter().enumerate() {
            assert_eq!(
                result[i].line, expected_line,
                "Empty link {i} should be on line {expected_line}"
            );
        }
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();

        // These reference links should NOT be flagged as empty
        assert!(
            result.is_empty(),
            "Should not flag reference links as empty when code blocks contain tildes (issue #29). Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_backtick_wrapped_references() {
        // Test for issue #97 - backtick-wrapped references should be recognized as MkDocs auto-references
        let rule = MD042NoEmptyLinks::new();

        // Module.Class pattern with backticks
        let ctx = LintContext::new("[`module.Class`][]", crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`module.Class`][] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // Reference with explicit ID
        let ctx = LintContext::new("[`module.Class`][ref]", crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`module.Class`][ref] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // Path-like reference with backticks
        let ctx = LintContext::new("[`api/endpoint`][]", crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag [`api/endpoint`][] as empty in MkDocs mode (issue #97). Got: {result:?}"
        );

        // Should still flag in standard mode
        let ctx = LintContext::new("[`module.Class`][]", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should flag [`module.Class`][] as empty in Standard mode (no auto-refs). Got: {result:?}"
        );

        // Should still flag truly empty links even in MkDocs mode
        let ctx = LintContext::new("[][]", crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should still flag [][] as empty in MkDocs mode. Got: {result:?}"
        );
    }
}
