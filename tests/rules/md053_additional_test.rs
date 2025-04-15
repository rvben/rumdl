use rumdl::rule::Rule;
use rumdl::rules::MD053LinkImageReferenceDefinitions;

#[test]
fn test_references_in_blockquote() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "> [link][id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_in_code_blocks() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "```markdown\n[link][id]\n```\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1); // The reference inside code block is not detected in current implementation
}

#[test]
fn test_references_with_special_chars() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][special-id!@#]\n\n[special-id!@#]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_with_unicode() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][üñîçødé]\n\n[üñîçødé]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_trailing_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id  ]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_leading_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][  id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_angle_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id]\n\n[id]: <http://example.com>";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_escaped_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link]\\[id\\]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty()); // The implementation still recognizes the reference
}

#[test]
fn test_reference_in_html_comment() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "<!-- [link][id] -->\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty()); // The implementation still recognizes references in HTML comments
}

#[test]
fn test_empty_reference_brackets() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[](id)\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1); // Should be unused since reference format is invalid
}

#[test]
fn test_duplicate_reference_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id]\n\n[id]: http://example.com\n[id]: http://another.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Both definitions should be considered used
}

#[test]
fn test_case_difference_between_usage_and_definition() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][ID]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty()); // Case insensitive matching
}

#[test]
fn test_references_separated_by_content() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content =
        "[link][id]\n\nLots of content here...\nMore content...\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_consecutive_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[used][id1]\n\n[id1]: http://example1.com\n[id2]: http://example2.com";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "[used][id1]\n\n[id1]: http://example1.com");
}

#[test]
fn test_fix_consecutive_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id1]\n\n[id1]: http://example1.com\n[id2]: http://example2.com\n[id3]: http://example3.com";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "[link][id1]\n\n[id1]: http://example1.com");
}

#[test]
fn test_fix_references_at_document_start() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[unused]: http://example.com\n\n# Heading\nSome content";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\nSome content");
}

#[test]
fn test_fix_references_at_document_end() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Heading\nSome content\n\n[unused]: http://example.com";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\nSome content");
}

#[test]
fn test_ignored_definitions_case_insensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec!["IGNORED".to_string()]);
    let content = "[ignored]: http://example.com\nNo references here";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_ignored_definitions() {
    let rule =
        MD053LinkImageReferenceDefinitions::new(vec!["ignore1".to_string(), "ignore2".to_string()]);
    let content = "[ignore1]: http://example1.com\n[ignore2]: http://example2.com\n[used]: http://example3.com\n[used] is a link";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_reference_link() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[text][]\n\n[text]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_image_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "![alt][]\n\n[alt]: http://example.com/image.png";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_performance_with_many_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let mut content = String::new();

    // Add 100 reference definitions
    for i in 1..101 {
        content.push_str(&format!("[ref{}]: http://example.com/{}\n", i, i));
    }

    // Use only 50 of them
    content.push('\n');
    for i in 1..51 {
        content.push_str(&format!("[link{}][ref{}]\n", i, i));
    }

    let result = rule.check(&content).unwrap();
    assert_eq!(result.len(), 50); // 50 unused references
}

#[test]
fn test_with_front_matter() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "---\ntitle: Document\n---\n\n[link][ref]\n\n[ref]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "Reference in document with front matter should be detected as used"
    );
}

#[test]
fn test_multiline_definition() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[multiline]: http://example.com\n  \"Title that spans\n  multiple lines\"\n\n[link][multiline]";
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "Multiline reference definition should be properly detected"
    );
}

#[test]
fn test_nested_formatting() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[**bold _italic_ reference**][ref]\n\n[ref]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "Formatted reference should be properly detected"
    );
}

#[test]
fn test_code_span_in_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[Reference with `code span`][ref]\n\n[ref]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "Reference with code span should be properly detected"
    );
}

#[test]
fn test_references_in_list() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "- [List item link][ref1]\n- Another item\n- ![Image in list][ref2]\n\n[ref1]: http://example.com/1\n[ref2]: http://example.com/image.png";
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "References in lists should be properly detected"
    );
}

#[test]
fn test_fix_preserves_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Header\n\nText with [link][used].\n\n[used]: http://example.com\n[unused]: http://example.com/unused\n\nMore text.";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Header\n\nText with [link][used].\n\n[used]: http://example.com\n\nMore text."
    );
}

#[test]
fn test_fix_preserves_content_structure() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Start\n\n[unused1]: http://example.com\n\n## Middle\n\n[used]: http://used.com\n\nText [link][used]\n\n[unused2]: http://example.com/2\n\n## End";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Start\n\n## Middle\n\n[used]: http://used.com\n\nText [link][used]\n\n## End"
    );
}

#[test]
fn test_fix_multi_line_unused_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Document\n\n[unused]: http://example.com\n  \"Title spanning\n  multiple lines\"\n\nText with no references.";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Document\n\nText with no references.");
}

#[test]
fn test_fix_with_code_blocks() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Document\n\n```markdown\n[ref]: http://example.com\n```\n\n[unused]: http://example.com\n\nNo references here.";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Document\n\n```markdown\n[ref]: http://example.com\n```\n\nNo references here."
    );
}

#[test]
fn test_performance_fix() {
    use std::time::Instant;

    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let mut content = String::with_capacity(10000);

    // Create content with many references
    for i in 1..101 {
        content.push_str(&format!("# Section {}\n\n", i));
        if i % 2 == 0 {
            content.push_str(&format!("[link{}][ref{}]\n\n", i, i));
        }
    }

    // Add reference definitions (half used, half unused)
    for i in 1..101 {
        content.push_str(&format!("[ref{}]: http://example.com/{}\n", i, i));
    }

    let start = Instant::now();
    let fixed = rule.fix(&content).unwrap();
    let duration = start.elapsed();

    // Verify correctness: should remove 50 unused references
    for i in 1..101 {
        if i % 2 == 1 {
            assert!(
                !fixed.contains(&format!("[ref{}]:", i)),
                "Unused reference [ref{}] should be removed",
                i
            );
        } else {
            assert!(
                fixed.contains(&format!("[ref{}]:", i)),
                "Used reference [ref{}] should be kept",
                i
            );
        }
    }

    // Verify performance is reasonable
    println!("MD053 fix performance test completed in {:?}", duration);
    assert!(
        duration.as_millis() < 1000,
        "Fix operation should complete in under 1000ms"
    );
}

#[test]
fn test_nested_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[![alt][img]][link]\n\n[img]: /path/to/img.png\n[link]: http://example.com";
    let result = rule.check(content).unwrap();
    // Both 'img' and 'link' are used according to CommonMark spec
    assert_eq!(result.len(), 0); // No unused references
}

#[test]
fn test_reference_with_markdown_in_link_text() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[*Formatted* **text** `code`][id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_followed_by_text_on_same_line() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id] some text follows here\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_backslash_in_definition() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][slash-id]\n\n[slash\\-id]: http://example.com";
    let result = rule.check(content).unwrap();
    // The implementation now handles escaped chars in definitions
    assert!(result.is_empty());
}

#[test]
fn test_fix_nested_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[![alt][img1]][link1]\n\n[img1]: /path/to/img1.png\n[img2]: /path/to/img2.png\n[link1]: http://example1.com";
    let fixed = rule.fix(content).unwrap();
    // Both 'img1' and 'link1' are used according to CommonMark spec
    assert_eq!(fixed, "[![alt][img1]][link1]\n\n[img1]: /path/to/img1.png\n[link1]: http://example1.com");
}

#[test]
fn test_multiline_content_with_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
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
    let result = rule.check(&content).unwrap();
    assert_eq!(result.len(), 2);

    // Fix should remove only unused references
    let fixed = rule.fix(&content).unwrap();
    assert!(!fixed.contains("[unused1]"));
    assert!(!fixed.contains("[unused2]"));
    assert!(fixed.contains("[id1]"));
    assert!(fixed.contains("[id2]"));
    assert!(fixed.contains("[id3]"));
    assert!(fixed.contains("[id4]"));
    assert!(fixed.contains("[shortcut]"));
}
