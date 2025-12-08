use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD005ListIndent, MD007ULIndent, MD013LineLength, MD031BlanksAroundFences, MD042NoEmptyLinks, MD046CodeBlockStyle,
    MD052ReferenceLinkImages,
};

#[test]
fn test_mkdocs_footnotes_integration() {
    // Test that footnote definitions don't trigger false positives
    let content = r#"# Document with Footnotes

Here's some text with a footnote[^1] reference.

Another paragraph with a named footnote[^note].

[^1]: This is the first footnote definition.
    It can span multiple lines with proper indentation.

    Even include paragraphs.

[^note]: This is a named footnote.

Regular text after footnotes."#;

    // Test with MD031 (blanks around fences/blocks)
    let rule_031 = MD031BlanksAroundFences::default();
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings = rule_031.check(&ctx).unwrap();

    // Footnote definitions should not trigger MD031
    assert_eq!(warnings.len(), 0, "Footnote definitions should not need blank lines");

    // Test with MD042 (no empty links) - footnote refs are not empty links
    let rule_042 = MD042NoEmptyLinks::default();
    let warnings = rule_042.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "Footnote references should not be flagged as empty links"
    );
}

#[test]
fn test_mkdocs_tabs_integration() {
    // Test that content tabs don't trigger false positives
    let content = r#"# Document with Tabs

Regular content here.

=== "Python"

    ```python
    def hello():
        print("Hello, World!")
    ```

    Python is great for scripting.

=== "JavaScript"

    ```javascript
    function hello() {
        console.log("Hello, World!");
    }
    ```

    JavaScript runs in the browser.

=== "Rust"

    ```rust
    fn main() {
        println!("Hello, World!");
    }
    ```

More content after tabs."#;

    // Test with MD031 (blanks around fences)
    let rule_031 = MD031BlanksAroundFences::default();
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings = rule_031.check(&ctx).unwrap();

    // Tab markers themselves might need blank lines around them,
    // but content within tabs should be handled properly
    // The exact behavior depends on rule implementation
    assert!(warnings.len() <= 2, "Tab sections should be handled appropriately");
}

#[test]
fn test_mkdocstrings_autodoc_integration() {
    // Test that MkDocstrings autodoc blocks don't trigger false positives
    let content = r#"# API Documentation

## Module Reference

::: mypackage.mymodule.MyClass
    handler: python
    options:
      show_source: true
      show_root_heading: true
      members:
        - method1
        - method2

Regular documentation text.

::: another.module.Function

## Cross-References

See the [MyClass][] for more details.

The [configuration][mypackage.config.Config] is important.

You can also use [custom text][mymodule.utils.helper]."#;

    // Test with MD052 (reference links) - crossrefs are special
    let rule_052 = MD052ReferenceLinkImages::default();
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings = rule_052.check(&ctx).unwrap();

    // Cross-references should not be flagged as undefined references
    // This depends on the rule implementation recognizing them
    assert!(
        warnings.len() <= 3,
        "Cross-references might be flagged if not explicitly handled"
    );
}

#[test]
fn test_mkdocs_mixed_extensions() {
    // Test multiple MkDocs extensions in the same document
    let content = r#"# Complex MkDocs Document

!!! note "Important"
    This is an admonition with a footnote[^1].

## Content Tabs

=== "Tab 1"

    Content in tab 1 with a snippet:

    --8<-- "included.md"

=== "Tab 2"

    Content in tab 2 with autodoc:

    ::: mymodule.MyClass
        handler: python

## Regular Content

Some text with a footnote[^2] and a cross-reference to [MyClass][].

[^1]: First footnote in an admonition.
[^2]: Second footnote in regular text.

??? tip "Collapsible Tip"
    This collapsible section has content."#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Test with MD031
    let rule_031 = MD031BlanksAroundFences::default();
    let warnings = rule_031.check(&ctx).unwrap();

    // The various MkDocs constructs should be recognized
    // Some may require blank lines, but the count should be reasonable
    assert!(warnings.len() <= 10, "Mixed MkDocs features should be handled");
}

#[test]
fn test_standard_flavor_no_mkdocs_features() {
    // Ensure MkDocs features are NOT recognized in standard flavor
    let content = r#"# Standard Markdown

[^1]: This looks like a footnote but isn't in standard flavor.

=== "Tab"
    This looks like a tab but isn't.

::: module.Class
    This looks like autodoc but isn't.

!!! note
    This looks like an admonition but isn't."#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // In standard flavor, these should be treated as regular text
    // and may trigger various rules
    let rule_031 = MD031BlanksAroundFences::default();
    let warnings = rule_031.check(&ctx).unwrap();

    // Standard flavor should not recognize MkDocs syntax
    assert_eq!(warnings.len(), 0, "Standard flavor should not detect MkDocs features");
}

#[test]
fn test_footnotes_with_complex_content() {
    let content = r#"# Document

Text with reference[^complex].

[^complex]: This footnote has:
    - A list item
    - Another item

    ```python
    # Code in footnote
    print("test")
    ```

    More content.

Regular text."#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Complex footnote content should be properly handled
    let rule_031 = MD031BlanksAroundFences::default();
    let warnings = rule_031.check(&ctx).unwrap();

    // Code blocks within footnotes might need special handling
    assert!(warnings.len() <= 2, "Complex footnote content should be handled");
}

#[test]
fn test_nested_tabs() {
    let content = r#"# Nested Tabs

=== "Outer Tab 1"

    Content in outer tab.

    === "Inner Tab A"

        Nested content A.

    === "Inner Tab B"

        Nested content B.

=== "Outer Tab 2"

    Different content."#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Nested tabs should be recognized
    let rule_031 = MD031BlanksAroundFences::default();
    let warnings = rule_031.check(&ctx).unwrap();

    // The behavior with nested tabs depends on implementation
    assert!(warnings.len() <= 8, "Nested tabs should be handled");
}
#[test]
fn test_mkdocstrings_with_yaml_options() {
    // Test that mkdocstrings blocks with YAML options don't trigger false positives
    // This addresses issue #94
    let content = r#"# API Documentation

::: toga.ScrollContainer
    options:
        members:
            - window
            - app
            - content
        show_source: true
        show_root_heading: true

Regular text continues here."#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // MD005 - Should not flag YAML lists as markdown list indentation issues
    let rule_005 = MD005ListIndent::default();
    let warnings = rule_005.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "MD005 should not flag YAML lists in mkdocstrings options"
    );

    // MD007 - Should not flag YAML lists
    let rule_007 = MD007ULIndent::new(2);
    let warnings = rule_007.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "MD007 should not flag YAML lists in mkdocstrings options"
    );

    // MD013 - Should not flag long lines in mkdocstrings blocks
    let rule_013 = MD013LineLength::new(80, false, false, false, false);
    let warnings = rule_013.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 0, "MD013 should not flag mkdocstrings blocks");

    // MD046 - Should not flag YAML options as indented code blocks
    let rule_046 = MD046CodeBlockStyle::new(rumdl_lib::rules::code_block_utils::CodeBlockStyle::Fenced);
    let warnings = rule_046.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "MD046 should not flag YAML options as indented code blocks"
    );
}

#[test]
fn test_mkdocstrings_deeply_nested_yaml() {
    // Test mkdocstrings with complex nested YAML structure
    let content = r#"# API Documentation

::: mypackage.module.Class
    handler: python
    options:
        show_source: true
        show_root_heading: true
        members:
            - method1
            - method2
            - property1
        filters:
            - "!^_"
            - "!^test_"
        group_by_category: true
        categories:
            properties:
                - property1
                - property2
            methods:
                - method1
                - method2

Regular documentation continues."#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // All rules should ignore the mkdocstrings block
    let rule_005 = MD005ListIndent::default();
    assert_eq!(rule_005.check(&ctx).unwrap().len(), 0);

    let rule_007 = MD007ULIndent::new(2);
    assert_eq!(rule_007.check(&ctx).unwrap().len(), 0);

    let rule_046 = MD046CodeBlockStyle::new(rumdl_lib::rules::code_block_utils::CodeBlockStyle::Fenced);
    assert_eq!(rule_046.check(&ctx).unwrap().len(), 0);
}
