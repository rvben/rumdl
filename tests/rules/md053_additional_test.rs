use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD053LinkImageReferenceDefinitions;

#[test]
fn test_references_in_blockquote() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "> [link][id]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_in_code_blocks() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "```markdown\n[link][id]\n```\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // The reference inside code block is not detected in current implementation
}

#[test]
fn test_references_with_special_chars() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][special-id!@#]\n\n[special-id!@#]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_with_unicode() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][üñîçødé]\n\n[üñîçødé]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_trailing_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id  ]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_leading_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][  id]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_angle_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id]\n\n[id]: <http://example.com>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_escaped_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link]\\[id\\]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // The implementation still recognizes the reference
}

#[test]
fn test_reference_in_html_comment() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "<!-- [link][id] -->\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // The implementation still recognizes references in HTML comments
}

#[test]
fn test_empty_reference_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[](id)\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Should be unused since reference format is invalid
}

#[test]
fn test_duplicate_reference_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id]\n\n[id]: http://example.com\n[id]: http://another.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Duplicate definition should be flagged
}

#[test]
fn test_case_difference_between_usage_and_definition() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][ID]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Case insensitive matching
}

#[test]
fn test_references_separated_by_content() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id]\n\nLots of content here...\nMore content...\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignored_definitions_case_insensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[ignored]: http://example.com\nNo references here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Assert it's detected as unused (default behavior)
}

#[test]
fn test_multiple_ignored_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content =
        "[ignore1]: http://example1.com\n[ignore2]: http://example2.com\n[used]: http://example3.com\n[used] is a link";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Assert they are detected as unused (default behavior)
}

#[test]
fn test_empty_reference_link() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[text][]\n\n[text]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_image_reference() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "![alt][]\n\n[alt]: http://example.com/image.png";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_performance_with_many_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let mut content = String::new();

    // Add 100 reference definitions
    for i in 1..101 {
        content.push_str(&format!("[ref{i}]: http://example.com/{i}\n"));
    }

    // Use only 50 of them
    content.push('\n');
    for i in 1..51 {
        content.push_str(&format!("[link{i}][ref{i}]\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 50); // 50 unused references
}

#[test]
fn test_with_front_matter() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "---\ntitle: Document\n---\n\n[link][ref]\n\n[ref]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Reference in document with front matter should be detected as used"
    );
}

#[test]
fn test_multiline_definition() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[multiline]: http://example.com\n  \"Title that spans\n  multiple lines\"\n\n[link][multiline]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiline reference definition should be properly detected"
    );
}

#[test]
fn test_nested_formatting() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[**bold _italic_ reference**][ref]\n\n[ref]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Formatted reference should be properly detected");
}

#[test]
fn test_code_span_in_reference() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[Reference with `code span`][ref]\n\n[ref]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Reference with code span should be properly detected"
    );
}

#[test]
fn test_references_in_list() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "- [List item link][ref1]\n- Another item\n- ![Image in list][ref2]\n\n[ref1]: http://example.com/1\n[ref2]: http://example.com/image.png";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "References in lists should be properly detected");
}

#[test]
fn test_performance_detection() {
    use std::time::Instant;

    let rule = MD053LinkImageReferenceDefinitions::default();
    let mut content = String::with_capacity(10000);

    // Create content with many references
    for i in 1..101 {
        content.push_str(&format!("# Section {i}\n\n"));
        if i % 2 == 0 {
            content.push_str(&format!("[link{i}][ref{i}]\n\n"));
        }
    }

    // Add reference definitions (half used, half unused)
    for i in 1..101 {
        content.push_str(&format!("[ref{i}]: http://example.com/{i}\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Should detect 50 unused references (odd numbered ones)
    assert_eq!(result.len(), 50, "Should detect exactly 50 unused references");

    // Verify performance is reasonable
    println!("MD053 detection performance test completed in {duration:?}");
    assert!(duration.as_millis() < 1000, "Detection should complete in under 1000ms");
}

#[test]
fn test_nested_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[![alt][img]][link]\n\n[img]: /path/to/img.png\n[link]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Both 'img' and 'link' are used according to CommonMark spec
    assert_eq!(result.len(), 0); // No unused references
}

#[test]
fn test_reference_with_markdown_in_link_text() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[*Formatted* **text** `code`][id]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_followed_by_text_on_same_line() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id] some text follows here\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_backslash_in_definition() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][slash-id]\n\n[slash\\-id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The implementation now handles escaped chars in definitions
    assert!(result.is_empty());
}

#[test]
fn test_multiline_content_with_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    // Create content with lots of references across multiple lines
    let mut content = String::from("# Document with multiple references\n\n");
    content.push_str("Some paragraph with [link1][id1] and [link2][id2].\n\n");
    content.push_str("Another paragraph with ![image][id3].\n\n");
    content.push_str("* List item with [item][id4]\n");
    content.push_str("* Another item with [shortcut][]\n\n");

    // Add definitions, some used and some unused
    content.push_str("[id1]: http://example1.com\n");
    content.push_str("[id2]: http://example2.com\n");
    content.push_str("[id3]: http://example3.com/image.png\n");
    content.push_str("[id4]: http://example4.com\n");
    content.push_str("[shortcut]: http://shortcut.com\n");
    content.push_str("[unused1]: http://unused1.com\n");
    content.push_str("[unused2]: http://unused2.com\n");

    // Check that only unused references are detected
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);

    // MD053 is now warning-only, fix should return original content
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Fix should return original content unchanged");
}

#[test]
fn test_shortcut_reference_with_colon() {
    let rule = MD053LinkImageReferenceDefinitions::default();
    let content = "[link][id]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_in_list_continuation_paragraph() {
    // This is the exact example from PyO3 documentation that triggered the bug
    let content = r#"- `__richcmp__(<self>, object, pyo3::basic::CompareOp) -> object`

    Implements Python comparison operations.
    You can use [`CompareOp::matches`] to adapt.

[`CompareOp::matches`]: https://example.com
"#;

    let rule = MD053LinkImageReferenceDefinitions::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Debug: Print line info to see what's happening
    eprintln!("\nDEBUG: List continuation test");
    for (i, line) in ctx.lines.iter().enumerate() {
        eprintln!(
            "Line {}: in_code_block={}, content={:?}",
            i + 1,
            line.in_code_block,
            line.content(ctx.content)
        );
    }

    let result = rule.check(&ctx).unwrap();

    // The reference [`CompareOp::matches`] IS used in the list continuation paragraph
    // It should NOT be reported as unused
    assert_eq!(
        result.len(),
        0,
        "Reference should not be reported as unused. Got warnings: {result:?}"
    );
}

#[test]
fn test_list_continuation_not_marked_as_code_block() {
    // Test that list continuation paragraphs are not marked as code blocks in LintContext
    let content = r#"- List item

    This is a continuation paragraph with 4 spaces.
    It should NOT be marked as in_code_block.

- Another item
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Line 3 (index 2) is the continuation paragraph
    let continuation_line = ctx.lines.get(2).expect("Line 3 should exist");
    assert!(
        continuation_line
            .content(ctx.content)
            .trim()
            .starts_with("This is a continuation"),
        "Got unexpected line content: {:?}",
        continuation_line.content(ctx.content)
    );

    assert!(
        !continuation_line.in_code_block,
        "List continuation paragraph should NOT be marked as in_code_block"
    );
}
