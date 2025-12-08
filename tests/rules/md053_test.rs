use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD053LinkImageReferenceDefinitions;

#[test]
fn test_all_references_used() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Should detect id2 as unused
}

#[test]
fn test_shortcut_reference() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[example]\n\n[example]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content =
        "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Should detect id2 and id3 as unused
}

#[test]
fn test_case_insensitive() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[example][ID]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // All references are unused
}

#[test]
fn test_mixed_used_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][used]\nSome text\n\n[used]: http://example.com/used\n[unused]: http://example.com/unused";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Should detect unused reference
}

#[test]
fn test_valid_reference_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[ref]: https://example.com\n[ref] is a link";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference_definition() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[unused]: https://example.com\nThis has no references";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[ref1]: https://example1.com\n[ref2]: https://example2.com\n[ref1] and [ref2] are links";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_image_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[img]: image.png\n![Image][img]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[ref]: https://example.com\n[img]: image.png\n[ref] is a link and ![Image][img] is an image";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignored_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[ignored]: https://example.com\nNo references here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_case_sensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[REF]: https://example.com\n[ref] is a link";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

// test_fix_unused_references removed - MD053 no longer provides fixes

#[test]
fn test_with_document_structure() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_case_insensitive_with_backticks() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case mismatch with backticks
    let content = "# Test\n\nThis is [`Example`].\n\n[`example`]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Case-insensitive matching should work with backticks"
    );
}

#[test]
fn test_case_insensitive_dotted_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case mismatch with dots in backticks (like dataclasses.InitVar from ruff repo)
    let content = "From the Python documentation on [`dataclasses.InitVar`]:\n\n[`dataclasses.initvar`]: https://docs.python.org/3/library/dataclasses.html#dataclasses.InitVar\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Case-insensitive matching should work for dotted references in backticks"
    );
}

#[test]
fn test_references_with_apostrophes() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case mismatch with apostrophes (like De Morgan's)
    let content = "The [De Morgan's Laws] are important.\n\n[de morgan's laws]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Case-insensitive matching should work with apostrophes"
    );
}

#[test]
fn test_references_with_dots_not_filtered() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test references with dots (previously incorrectly filtered as config sections)
    let content = "See [tool.ruff] for details.\n\n[tool.ruff]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "References with dots should be recognized");
}

#[test]
fn test_references_with_slashes_not_filtered() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test references with forward slashes (previously incorrectly filtered as file paths)
    let content = "See [docs/api] for details.\n\n[docs/api]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "References with forward slashes should be recognized"
    );
}

#[test]
fn test_single_letter_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test single letter references (previously incorrectly filtered)
    let content = "See [T] for type parameter.\n\n[T]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Single letter references should be recognized");
}

#[test]
fn test_common_type_name_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test common type name references (previously incorrectly filtered)
    let content = "The [str] type in Python.\n\n[str]: https://docs.python.org/3/library/stdtypes.html#str\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Common type names as references should be recognized"
    );
}

#[test]
fn test_shortcut_reference_with_colon() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test shortcut reference followed by colon (common in documentation)
    let content = "As stated in [`reference`]:\n\n[`reference`]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Shortcut reference followed by colon should be recognized"
    );
}

#[test]
fn test_shortcut_reference_with_period() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test shortcut reference followed by period
    let content = "See [reference].\n\n[reference]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Shortcut reference followed by period should be recognized"
    );
}

#[test]
fn test_numeric_footnote_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test numeric references (could be footnotes)
    let content = "See note [1] for details.\n\n[1]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Numeric references should be recognized");
}

#[test]
fn test_patterns_still_skipped() {
    let rule = MD053LinkImageReferenceDefinitions::default();

    // Alert patterns (GitHub alerts) should still be skipped
    let content = "[!NOTE]\nThis is a note.\n\n[other]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Alert patterns should be skipped");

    // Pure punctuation should still be skipped
    let content = "Array[...] notation.\n\n[other]: https://example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Pure punctuation patterns should be skipped");
}

#[test]
fn test_complex_real_world_case() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test a complex case combining multiple features
    let content = r#"# Documentation

## Python Types

The [`typing.Optional`] type is equivalent to [`Union[T, None]`].
See also [`dataclasses.InitVar`]: it marks init-only fields.

For file paths, use [`pathlib.Path`] or [os.path] module.

Single type parameters like [T] are common.

[`typing.optional`]: https://docs.python.org/3/library/typing.html#typing.Optional
[`union[t, none]`]: https://docs.python.org/3/library/typing.html#typing.Union
[`dataclasses.initvar`]: https://docs.python.org/3/library/dataclasses.html#dataclasses.InitVar
[`pathlib.path`]: https://docs.python.org/3/library/pathlib.html
[os.path]: https://docs.python.org/3/library/os.path.html
[t]: https://docs.python.org/3/library/typing.html#type-variables
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "All references in complex real-world case should be recognized: {result:?}"
    );
}

#[test]
fn debug_github_issue_77_case() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Reproduce the exact case reported in GitHub issue #77
    let content = r#"# Test

## Case that reproduces the issue
This is about the [type annotation grammar].

From the Python documentation on [`dataclasses.InitVar`]:

## Definitions
[type annotation grammar]: https://docs.python.org/3/reference/grammar.html
[`dataclasses.InitVar`]: https://docs.python.org/3/library/dataclasses.html#dataclasses.InitVar
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    println!("\n=== PARSED LINKS ===");
    for (i, link) in ctx.links.iter().enumerate() {
        println!(
            "Link {}: line {}, text='{}', is_reference={}, reference_id={:?}",
            i, link.line, link.text, link.is_reference, link.reference_id
        );
    }

    println!("\n=== REFERENCE DEFINITIONS ===");
    for (i, ref_def) in ctx.reference_defs.iter().enumerate() {
        println!(
            "RefDef {}: line {}, id='{}', url='{}'",
            i, ref_def.line, ref_def.id, ref_def.url
        );
    }

    println!("\n=== MD053 CHECK RESULTS ===");
    let warnings = rule.check(&ctx).unwrap();
    if warnings.is_empty() {
        println!("No unused reference warnings (all references found correctly)");
    } else {
        for warning in &warnings {
            println!("Line {}: {}", warning.line, warning.message);
        }
    }

    // Both references should be found as used
    assert!(
        warnings.is_empty(),
        "Expected no unused references, but found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_case_sensitivity_with_backticks() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case sensitivity issues with backticks
    let content = r#"# Case Sensitivity Test

From the Python documentation on [`dataclasses.InitVar`]:

[`dataclasses.initvar`]: https://docs.python.org/3/library/dataclasses.html#dataclasses.InitVar
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // This should work due to case-insensitive matching
    assert!(
        warnings.is_empty(),
        "Case-insensitive matching should work for backtick references"
    );
}

#[test]
fn test_backtick_reference_with_double_colon_and_comma() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case from GitHub issue #128: backtick reference with `::` and `, `
    // Previously filtered out because it contains both `:` and space
    let content = "See [`Bound<'_, PyAny>::is_callable`] function.\n\n[`Bound<'_, PyAny>::is_callable`]: foo\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Backtick references with :: and comma should not be filtered out (GitHub issue #128)"
    );
}

#[test]
fn test_backtick_reference_in_list_continuation() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Test case from GitHub issue #128 follow-up: backtick reference in list item continuation
    // The reference usage is in indented list content (4 spaces)
    let content = r#"- `__richcmp__(<self>, object, pyo3::basic::CompareOp) -> object`

    Implements Python comparison operations.
    You can use [`CompareOp::matches`] to adapt.

[`CompareOp::matches`]: https://example.com
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Backtick references in list item continuations should be detected (GitHub issue #128 follow-up)"
    );
}
