use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD024NoDuplicateHeading, MD052ReferenceLinkImages};

#[test]
fn test_mkdocs_snippets_md024_duplicate_headings() {
    // Test that MD024 doesn't flag duplicates within snippet sections
    let content = r#"# Document

## Installation

Some content here.

<!-- --8<-- [start:included-content] -->

## Installation

This is included content that might have duplicate headings.

<!-- --8<-- [end:included-content] -->

## Another Section
"#;

    let rule = MD024NoDuplicateHeading::default();

    // Test with standard flavor - should flag duplicate
    let ctx_standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings_standard = rule.check(&ctx_standard).unwrap();
    assert_eq!(
        warnings_standard.len(),
        1,
        "Standard flavor should flag duplicate heading"
    );
    assert!(
        warnings_standard[0]
            .message
            .contains("Duplicate heading: 'Installation'")
    );

    // Test with MkDocs flavor - should NOT flag duplicate
    let ctx_mkdocs = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings_mkdocs = rule.check(&ctx_mkdocs).unwrap();
    assert_eq!(
        warnings_mkdocs.len(),
        0,
        "MkDocs flavor should not flag duplicate heading in snippet section"
    );
}

#[test]
fn test_mkdocs_snippets_file_inclusion() {
    // Test that direct file inclusion syntax is handled correctly
    let content = r#"# Document

## Including Files

Here's how to include a file:

--8<-- "docs/installation.md"

More content here.
"#;

    let rule = MD052ReferenceLinkImages::default();

    // With MkDocs flavor, the snippet line should be ignored
    let ctx_mkdocs = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings_mkdocs = rule.check(&ctx_mkdocs).unwrap();
    assert_eq!(
        warnings_mkdocs.len(),
        0,
        "MkDocs flavor should not flag snippet inclusion"
    );
}

#[test]
fn test_mkdocs_nested_snippets() {
    // Test nested snippet sections
    let content = r#"# Document

## Section 1

<!-- --8<-- [start:outer] -->

## Outer Section

<!-- --8<-- [start:inner] -->

## Inner Section

<!-- --8<-- [end:inner] -->

## Outer Section

<!-- --8<-- [end:outer] -->

## Section 2
"#;

    let rule = MD024NoDuplicateHeading::default();

    // With MkDocs flavor, duplicates within snippets should be ignored
    let ctx_mkdocs = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings_mkdocs = rule.check(&ctx_mkdocs).unwrap();

    // Debug: Print warnings if any
    for warning in &warnings_mkdocs {
        eprintln!("Warning: {} at line {}", warning.message, warning.line);
    }

    assert_eq!(
        warnings_mkdocs.len(),
        0,
        "MkDocs flavor should handle nested snippets correctly"
    );
}

#[test]
fn test_mkdocs_snippet_variations() {
    // Test different snippet syntax variations
    let content = r#"# Document

--8<-- "file1.md"

--8<-- 'file2.md'

<!-- --8<-- "file3.md" -->

<!-- -8<- [start:section] -->

<!-- -8<- [end:section] -->
"#;

    let rule = MD052ReferenceLinkImages::default();

    // All variations should be recognized in MkDocs mode
    let ctx_mkdocs = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings_mkdocs = rule.check(&ctx_mkdocs).unwrap();

    // Debug: Print warnings if any
    for warning in &warnings_mkdocs {
        eprintln!("Warning: {} at line {}", warning.message, warning.line);
    }

    assert_eq!(
        warnings_mkdocs.len(),
        0,
        "All snippet syntax variations should be recognized"
    );
}
