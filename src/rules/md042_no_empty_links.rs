use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;

/// Rule MD042: No empty links
///
/// See [docs/md042.md](../../docs/md042.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link has no content (text) or destination (URL).
#[derive(Clone)]
pub struct MD042NoEmptyLinks;

impl Default for MD042NoEmptyLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl MD042NoEmptyLinks {
    pub fn new() -> Self {
        Self
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

            // Check for empty links
            if link.text.trim().is_empty() || effective_url.trim().is_empty() {
                let replacement = if link.text.trim().is_empty() && effective_url.trim().is_empty() {
                    "[Link text](https://example.com)".to_string()
                } else if link.text.trim().is_empty() {
                    if link.is_reference {
                        format!("[Link text]{}", &ctx.content[link.byte_offset + 1..link.byte_end])
                    } else {
                        format!("[Link text]({effective_url})")
                    }
                } else if link.is_reference {
                    // Keep the reference format
                    let ref_part = &ctx.content[link.byte_offset + link.text.len() + 2..link.byte_end];
                    format!("[{}]{}", link.text, ref_part)
                } else {
                    format!("[{}](https://example.com)", link.text)
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Empty link found: [{}]({})", link.text, effective_url),
                    line: link.line,
                    column: link.start_col + 1, // Convert to 1-indexed
                    end_line: link.line,
                    end_column: link.end_col + 1, // Convert to 1-indexed
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link.byte_offset..link.byte_end,
                        replacement,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = _ctx.content;
        // Early return if there are no links
        if structure.links.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Get pre-computed empty links
        let empty_links = structure.get_empty_links();

        for link in empty_links {
            let replacement = if link.text.trim().is_empty() && link.url.trim().is_empty() {
                "[Link text](https://example.com)".to_string()
            } else if link.text.trim().is_empty() {
                format!("[Link text]({})", link.url)
            } else {
                format!("[{}](https://example.com)", link.text)
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: format!("Empty link found: [{}]({})", link.text, link.url),
                line: link.line,
                column: link.start_col,
                end_line: link.line,
                end_column: link.end_col + 1,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range_with_length(
                        link.line,
                        link.start_col,
                        (link.end_col + 1).saturating_sub(link.start_col),
                    ),
                    replacement,
                }),
            });
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
        let content = ctx.content;
        content.is_empty() || !content.contains('[')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD042NoEmptyLinks)
    }
}

impl DocumentStructureExtensions for MD042NoEmptyLinks {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.links.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_links_with_text_should_pass() {
        let ctx = LintContext::new("[valid link](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text should pass");

        let ctx = LintContext::new("[another valid link](path/to/page.html)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with text and relative URLs should pass");
    }

    #[test]
    fn test_links_with_empty_text_should_fail() {
        let ctx = LintContext::new("[](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [](https://example.com)");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_links_with_only_whitespace_should_fail() {
        let ctx = LintContext::new("[   ](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [   ](https://example.com)");

        let ctx = LintContext::new("[\t\n](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [\t\n](https://example.com)");
    }

    #[test]
    fn test_reference_links_with_empty_text() {
        let ctx = LintContext::new("[][ref]\n\n[ref]: https://example.com");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [](https://example.com)");
        assert_eq!(result[0].line, 1);

        // Empty text with empty reference
        let ctx = LintContext::new("[][]\n\n[]: https://example.com");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_images_should_be_ignored() {
        // Images can have empty alt text, so they should not trigger the rule
        let ctx = LintContext::new("![](image.png)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with empty alt text should be ignored");

        let ctx = LintContext::new("![   ](image.png)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Images with whitespace alt text should be ignored");
    }

    #[test]
    fn test_links_with_nested_formatting() {
        // Links with nested formatting but empty effective text
        // Note: [**] contains "**" as text, which is not empty after trimming
        let ctx = LintContext::new("[**](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[**] is not considered empty since ** is text");

        let ctx = LintContext::new("[__](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "[__] is not considered empty since __ is text");

        // Links with truly empty formatting should fail
        let ctx = LintContext::new("[](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // Links with nested formatting and actual text should pass
        let ctx = LintContext::new("[**bold text**](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with nested formatting and text should pass");

        let ctx = LintContext::new("[*italic* and **bold**](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Links with multiple nested formatting should pass");
    }

    #[test]
    fn test_multiple_empty_links_on_same_line() {
        let ctx = LintContext::new("[](url1) and [](url2) and [valid](url3)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should detect both empty links");
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].column, 14);
    }

    #[test]
    fn test_escaped_brackets() {
        // Escaped brackets should not be treated as links
        let ctx = LintContext::new("\\[\\](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Escaped brackets should not be treated as links");

        // But this should still be a link
        let ctx = LintContext::new("[\\[\\]](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Link with escaped brackets in text should pass");
    }

    #[test]
    fn test_links_in_lists_and_blockquotes() {
        // Empty links in lists
        let ctx = LintContext::new("- [](https://example.com)\n- [valid](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);

        // Empty links in blockquotes
        let ctx = LintContext::new("> [](https://example.com)\n> [valid](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);

        // Nested structures
        let ctx = LintContext::new("> - [](url1)\n> - [text](url2)");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_unicode_whitespace_characters() {
        // Non-breaking space (U+00A0) - IS considered whitespace by Rust's trim()
        let ctx = LintContext::new("[\u{00A0}](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Non-breaking space should be treated as whitespace");

        // Em space (U+2003) - IS considered whitespace by Rust's trim()
        let ctx = LintContext::new("[\u{2003}](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Em space should be treated as whitespace");

        // Zero-width space (U+200B) - NOT considered whitespace by Rust's trim()
        // This is a formatting character, not a whitespace character
        let ctx = LintContext::new("[\u{200B}](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Zero-width space is not considered whitespace by trim()"
        );

        // Test with zero-width space between spaces
        // Since trim() doesn't consider zero-width space as whitespace,
        // " \u{200B} " becomes "\u{200B}" after trimming, which is NOT empty
        let ctx = LintContext::new("[ \u{200B} ](https://example.com)");
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Zero-width space remains after trim(), so link is not empty"
        );
    }

    #[test]
    fn test_empty_url_with_text() {
        let ctx = LintContext::new("[some text]()");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: [some text]()");
    }

    #[test]
    fn test_both_empty_text_and_url() {
        let ctx = LintContext::new("[]()");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty link found: []()");
    }

    #[test]
    fn test_reference_link_with_undefined_reference() {
        let ctx = LintContext::new("[text][undefined]");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Undefined reference should be treated as empty URL");
    }

    #[test]
    fn test_shortcut_reference_links() {
        // Valid shortcut reference link (implicit reference)
        // Note: [example] by itself is not parsed as a link by the LINK_PATTERN regex
        // It needs to be followed by [] or () to be recognized as a link
        let ctx = LintContext::new("[example][]\n\n[example]: https://example.com");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid implicit reference link should pass");

        // Empty implicit reference link
        let ctx = LintContext::new("[][]\n\n[]: https://example.com");
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Empty implicit reference link should fail");

        // Test actual shortcut-style links are not detected (since they don't match the pattern)
        let ctx = LintContext::new("[example]\n\n[example]: https://example.com");
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Shortcut links without [] or () are not parsed as links"
        );
    }

    #[test]
    fn test_fix_suggestions() {
        let ctx = LintContext::new("[](https://example.com)");
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_some());
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[Link text](https://example.com)");

        let ctx = LintContext::new("[text]()");
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_some());
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[text](https://example.com)");

        let ctx = LintContext::new("[]()");
        let result = rule.check(&ctx).unwrap();
        assert!(result[0].fix.is_some());
        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "[Link text](https://example.com)");
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

        let ctx = LintContext::new(content);
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

        let ctx = LintContext::new(content);
        let rule = MD042NoEmptyLinks::new();
        let result = rule.check(&ctx).unwrap();

        // These reference links should NOT be flagged as empty
        assert!(
            result.is_empty(),
            "Should not flag reference links as empty when code blocks contain tildes (issue #29). Got: {result:?}"
        );
    }
}
