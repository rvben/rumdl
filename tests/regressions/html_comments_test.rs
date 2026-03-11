//! Integration tests for HTML comment handling (issue #119, #20)
//!
//! Ensures that content inside HTML comments is completely ignored by linters

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD005ListIndent, MD039NoSpaceInLinks, MD042NoEmptyLinks, MD052ReferenceLinkImages};

/// Test that links inside HTML comments are ignored (MD039, MD042)
#[test]
fn test_links_in_html_comments_ignored() {
    // Example from issue #119
    let content = r#"# Hello world

<!--
  CFPs go here, use this format: * [project name - title of issue](URL to issue)
  * [ - ]()
-->
<!-- or if none - *No Calls for participation were submitted this week.* -->
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD039: No spaces inside link text
    let md039 = MD039NoSpaceInLinks::new();
    let result = md039.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD039 should not flag links inside HTML comments. Found {} issues",
        result.len()
    );

    // MD042: No empty links
    let md042 = MD042NoEmptyLinks::new();
    let result = md042.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD042 should not flag empty links inside HTML comments. Found {} issues",
        result.len()
    );
}

/// Test that list items inside HTML comments are ignored (MD005)
#[test]
fn test_lists_in_html_comments_ignored() {
    let content = r#"# Valid heading

<!--
  * Item with wrong indentation
  * [ - ]()
-->
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD005: List indentation
    let md005 = MD005ListIndent::default();
    let result = md005.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD005 should not flag list indentation inside HTML comments. Found {} issues",
        result.len()
    );
}

/// Test that reference links inside HTML comments are ignored (MD052)
/// This is the example from issue #20
#[test]
fn test_reference_links_in_html_comments_ignored() {
    let content = r#"<!--- write fake_editor.py 'import sys\nopen(*sys.argv[1:], mode="wt").write("2 3 4 4 2 3 2")' -->
<!--- set_env EDITOR 'python3 fake_editor.py' -->

```bash
$ python3 vote.py
3 votes for: 2
2 votes for: 3, 4
```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD052: Reference links and images should use a label that is defined
    let md052 = MD052ReferenceLinkImages::default();
    let result = md052.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD052 should not flag reference links inside HTML comments. Found {} issues: {:?}",
        result.len(),
        result
    );
}

/// Test that images inside HTML comments are ignored
#[test]
fn test_images_in_html_comments_ignored() {
    let content = r#"# Valid heading

<!-- ![broken image]() -->
<!-- ![](no-alt-text.png) -->

Valid content here.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Check that images inside comments don't get parsed
    assert_eq!(ctx.images.len(), 0, "Images inside HTML comments should not be parsed");
}

/// Test multi-line HTML comments
#[test]
fn test_multiline_html_comments() {
    let content = r#"# Valid heading

<!--
This is a multi-line comment
with various markdown syntax:

* [ - ]() Empty link
![broken](img.png)
-->

Valid content.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD042: No empty links
    let md042 = MD042NoEmptyLinks::new();
    let result = md042.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD042 should not flag content in multi-line HTML comments"
    );

    // Images should not be parsed
    assert_eq!(
        ctx.images.len(),
        0,
        "Images inside multi-line HTML comments should not be parsed"
    );
}

/// Test that HTML comments on the same line as content are handled correctly
#[test]
fn test_inline_html_comments() {
    let content = r#"# Valid heading

Some text <!-- [broken link]() --> more text.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD042: No empty links
    let md042 = MD042NoEmptyLinks::new();
    let result = md042.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MD042 should not flag links inside inline HTML comments"
    );
}

/// Test that content outside HTML comments is still checked
#[test]
fn test_content_outside_comments_is_checked() {
    let content = r#"# Valid heading

<!-- This empty link is in a comment: [ - ]() -->

This empty link is outside: [ - ]()
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MD042: No empty links
    let md042 = MD042NoEmptyLinks::new();
    let result = md042.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "MD042 should still flag empty links outside HTML comments"
    );
    assert_eq!(result[0].line, 5, "The warning should be on line 5");
}

/// Test nested HTML comments (although not standard HTML, some parsers allow it)
#[test]
fn test_nested_html_comments() {
    // Note: Standard HTML doesn't support nested comments, but we should handle them gracefully
    let content = r#"# Valid heading

<!-- Outer comment
<!-- This looks like nested but isn't valid HTML -->
-->

Valid content.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // The parser should handle this without crashing
    // Behavior may vary, but it shouldn't panic
    let md042 = MD042NoEmptyLinks::new();
    let _result = md042.check(&ctx); // Just make sure it doesn't crash
}
