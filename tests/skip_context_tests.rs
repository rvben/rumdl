/// Integration tests for skip context detection across MD011, MD037, and MD052
///
/// These tests verify that the rules properly skip various markdown contexts
/// including HTML comments, math blocks, inline math, tables, and front matter.
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD011NoReversedLinks, MD037NoSpaceInEmphasis, MD052ReferenceLinkImages};

#[test]
fn test_md037_skips_html_comments() {
    let rule = MD037NoSpaceInEmphasis;

    // Test that emphasis markers inside HTML comments are not flagged
    let content = r#"# Test MD037 with HTML Comments

Regular text with * spaces * that should be flagged.

<!-- This has * spaces * inside a comment and should NOT be flagged -->

More text with * another issue * here.

<!--
Multi-line comment with
* spaced emphasis *
should also be ignored
-->

Final * test * outside comments."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 3 issues (outside HTML comments)
    assert_eq!(result.len(), 3, "Expected 3 warnings for emphasis outside comments");

    // Verify the warnings are for the correct lines
    let lines_with_issues: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(lines_with_issues.contains(&3), "Should flag line 3");
    assert!(lines_with_issues.contains(&7), "Should flag line 7");
    assert!(lines_with_issues.contains(&15), "Should flag line 15");
}

#[test]
fn test_md037_skips_math_contexts() {
    let rule = MD037NoSpaceInEmphasis;

    // Test that emphasis markers inside math blocks and inline math are not flagged
    let content = r#"# Test MD037 with Math Contexts

Regular text with * spaces * that should be flagged.

$$
This is a math block with * asterisks * that should NOT be flagged.
They might represent multiplication: a * b * c
$$

Inline math $a * b * c$ should also not be flagged.

Double dollar inline math $$x * y * z$$ should not be flagged.

But this * spaced emphasis * outside math should be flagged."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 issues (outside math contexts)
    assert_eq!(result.len(), 2, "Expected 2 warnings for emphasis outside math");

    // Verify the warnings are for the correct lines
    let lines_with_issues: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(lines_with_issues.contains(&3), "Should flag line 3");
    assert!(lines_with_issues.contains(&14), "Should flag line 14");
}

#[test]
fn test_md052_skips_html_comments() {
    let rule = MD052ReferenceLinkImages::new();

    // Test that reference links inside HTML comments are not flagged
    let content = r#"# Test MD052 with HTML Comments

Regular [undefined][ref1] reference that should be flagged.

<!-- This [hidden][ref2] reference should NOT be flagged -->

Another [missing][ref3] reference outside comments.

<!--
Multi-line comment with
[ignored][ref4] reference
and [another][ref5] one
-->

<!-- Complex patterns like [1:] from issue #20 should not be flagged -->"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 undefined references (outside HTML comments)
    assert_eq!(result.len(), 2, "Expected 2 warnings for references outside comments");

    // Verify the correct references are flagged
    let messages: Vec<String> = result.iter().map(|w| w.message.clone()).collect();
    assert!(messages.iter().any(|m| m.contains("ref1")), "Should flag ref1");
    assert!(messages.iter().any(|m| m.contains("ref3")), "Should flag ref3");

    // Should NOT flag references inside comments
    assert!(
        !messages.iter().any(|m| m.contains("ref2")),
        "Should not flag ref2 in comment"
    );
    assert!(
        !messages.iter().any(|m| m.contains("ref4")),
        "Should not flag ref4 in comment"
    );
    assert!(
        !messages.iter().any(|m| m.contains("ref5")),
        "Should not flag ref5 in comment"
    );
}

#[test]
fn test_md052_skips_math_contexts() {
    let rule = MD052ReferenceLinkImages::new();

    // Test that reference-like patterns in math are not flagged
    let content = r#"# Test MD052 with Math

Regular [undefined] reference that should be flagged.

$$
This is a math block with array notation [0] and [1] that should NOT be flagged.
Matrix element M[i][j] should also be ignored.
$$

Inline math with array $a[0]$ and matrix $M[i][j]$ should not be flagged.

Double dollar inline $$f[x]$$ should not be flagged.

But this [missing] reference outside math should be flagged."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 undefined references (outside math contexts)
    assert_eq!(result.len(), 2, "Expected 2 warnings for references outside math");

    // Verify the correct references are flagged
    let messages: Vec<String> = result.iter().map(|w| w.message.clone()).collect();
    assert!(
        messages.iter().any(|m| m.contains("undefined")),
        "Should flag 'undefined'"
    );
    assert!(messages.iter().any(|m| m.contains("missing")), "Should flag 'missing'");
}

#[test]
fn test_md052_skips_tables() {
    let rule = MD052ReferenceLinkImages::new();

    // Test that reference-like patterns in tables are not flagged
    let content = r#"# Test MD052 with Tables

Regular [undefined] reference that should be flagged.

| Header | Column |
|--------|--------|
| Cell with [ref1] | Another [ref2] |
| More [ref3] data | Final [ref4] cell |

This [missing] reference outside the table should be flagged.

Another table:
| Col 1 | Col 2 | Col 3 |
|-------|-------|-------|
| [a] | [b] | [c] |

Final [broken] reference should be flagged."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 3 undefined references (outside tables)
    // Note: This assumes table detection is working properly
    // If table cells aren't being skipped yet, this test will fail and that's okay
    // as it indicates we need to enhance table detection
    assert_eq!(result.len(), 3, "Expected 3 warnings for references outside tables");

    // Verify the correct references are flagged
    let messages: Vec<String> = result.iter().map(|w| w.message.clone()).collect();
    assert!(
        messages.iter().any(|m| m.contains("undefined")),
        "Should flag 'undefined'"
    );
    assert!(messages.iter().any(|m| m.contains("missing")), "Should flag 'missing'");
    assert!(messages.iter().any(|m| m.contains("broken")), "Should flag 'broken'");
}

#[test]
fn test_md011_skips_html_comments() {
    let rule = MD011NoReversedLinks;

    // Test that reversed link patterns inside HTML comments are not flagged
    let content = r#"# Test MD011 with HTML Comments

Regular (https://example.com)[reversed link] that should be flagged.

<!-- This (https://hidden.com)[in comment] should NOT be flagged -->

Another (https://test.com)[reversed] link outside comments.

<!--
Multi-line comment with
(https://ignored.com)[reversed syntax]
should also be ignored
-->"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 reversed links (outside HTML comments)
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings for reversed links outside comments"
    );

    // Verify the warnings are for the correct lines
    let lines_with_issues: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(lines_with_issues.contains(&3), "Should flag line 3");
    assert!(lines_with_issues.contains(&7), "Should flag line 7");
}

#[test]
fn test_md011_skips_math_contexts() {
    let rule = MD011NoReversedLinks;

    // Test that patterns in math blocks that might look like reversed links are not flagged
    let content = r#"# Test MD011 with Math

Regular (https://example.com)[reversed link] that should be flagged.

$$
Function notation f(x)[0] should NOT be flagged.
Array access pattern (arr)[index] should be ignored.
$$

Inline math $f(x)[i]$ should not be flagged.

Double dollar inline $$g(y)[j]$$ should not be flagged.

But this (https://test.com)[reversed] outside math should be flagged."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 reversed links (outside math contexts)
    assert_eq!(result.len(), 2, "Expected 2 warnings for reversed links outside math");

    // Verify the warnings are for the correct lines
    let lines_with_issues: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(lines_with_issues.contains(&3), "Should flag line 3");
    assert!(lines_with_issues.contains(&14), "Should flag line 14");
}

#[test]
fn test_md011_skips_front_matter() {
    let rule = MD011NoReversedLinks;

    // Test that patterns in front matter are not flagged
    let content = r#"---
title: "My Post"
tags: ["test", "example"]
description: "Pattern (like)[this] in frontmatter"
---

# Content

Regular (https://example.com)[reversed link] that should be flagged.

+++
title = "TOML frontmatter"
tags = ["more", "tags"]
pattern = "(toml)[pattern]"
+++

# More Content

Another (https://test.com)[reversed] link should be flagged."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 3 reversed links (outside front matter)
    // Note: The TOML block at lines 11-15 is NOT front matter (not at beginning),
    // so (toml)[pattern] on line 14 should be flagged
    assert_eq!(
        result.len(),
        3,
        "Expected 3 warnings for reversed links outside front matter"
    );

    // Verify the warnings are for the correct lines
    let lines_with_issues: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(lines_with_issues.contains(&9), "Should flag line 9");
    assert!(
        lines_with_issues.contains(&14),
        "Should flag line 14 (TOML block is not front matter)"
    );
    assert!(lines_with_issues.contains(&19), "Should flag line 19");
}

#[test]
fn test_combined_skip_contexts() {
    // Test that multiple skip contexts work together correctly
    let content = r#"---
frontmatter: "with (pattern)[like] this"
---

# Test Document

Regular * emphasis with spaces * should be flagged.

<!-- HTML comment with * spaces * and [undefined] reference -->

$$
Math block with * asterisks * and [array][notation]
$$

Inline math $f(x) * g(x)$ and $a[i]$ should be skipped.

| Table | Header |
|-------|--------|
| * spaces * | [ref] |

Outside contexts: * spaced * emphasis and [missing] reference and (https://example.com)[reversed] link."#;

    // Test MD037
    let md037 = MD037NoSpaceInEmphasis;
    let ctx = LintContext::new(content);
    let result = md037.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "MD037: Expected 2 warnings outside skip contexts");

    // Test MD052
    let md052 = MD052ReferenceLinkImages::new();
    let result = md052.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "MD052: Expected 1 warning for 'missing' reference");

    // Test MD011
    let md011 = MD011NoReversedLinks;
    let result = md011.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "MD011: Expected 1 warning for reversed link");
}

#[test]
fn test_nested_contexts() {
    // Test that nested contexts (e.g., inline code in HTML comments) work correctly
    let content = r#"# Nested Contexts Test

<!-- Comment with `inline code containing * spaces *` should be skipped entirely -->

Math with inline code: $$`array[0]` is inline code in math$$

Regular * spaces * outside all contexts should be flagged."#;

    let md037 = MD037NoSpaceInEmphasis;
    let ctx = LintContext::new(content);
    let result = md037.check(&ctx).unwrap();

    // Should only flag the last line
    assert_eq!(
        result.len(),
        1,
        "Expected only 1 warning for emphasis outside all contexts"
    );
    assert_eq!(result[0].line, 7, "Should flag line 7");
}
