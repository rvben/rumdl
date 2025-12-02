use rumdl_lib::utils::code_block_utils::CodeBlockUtils;

/// Test CommonMark Example 108 - List continuation takes precedence over code block
#[test]
fn test_commonmark_example_108_list_continuation_precedence() {
    let content = "  - foo\n\n    bar";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // "bar" at 4 spaces indentation could be a code block at document level,
    // but list continuation takes precedence per CommonMark
    assert_eq!(
        blocks.len(),
        0,
        "Example 108: list continuation should take precedence over code block interpretation"
    );
}

/// Test CommonMark Example 270 - Code block within list
#[test]
fn test_commonmark_example_270_code_block_in_list() {
    let content = "- foo\n\n      bar";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker at column 0 (width 1) + 1 space = continuation at column 2
    // "bar" has 6 spaces = continuation (2) + 4 = code block WITHIN list
    assert_eq!(
        blocks.len(),
        1,
        "Example 270: 6 spaces in list (continuation + 4) should be code block"
    );
    assert!(
        content[blocks[0].0..blocks[0].1].contains("bar"),
        "Code block should contain 'bar'"
    );
}

/// Test CommonMark Example 273 - Multiple code blocks in list
#[test]
fn test_commonmark_example_273_multiple_code_blocks_in_list() {
    let content = "1.     indented code\n\n   paragraph\n\n       more code";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Per CommonMark spec (verified with cmark reference implementation):
    // - "indented code" is a code block within the list item
    // - "paragraph" is a paragraph within the list item
    // - "more code" is ALSO a code block within the list item
    // Both "indented code" and "more code" are wrapped in <pre><code> per cmark
    assert_eq!(blocks.len(), 2, "Example 273: two code blocks detected (in list)");
}

/// Test CommonMark Example 257 - Insufficient continuation indent
#[test]
fn test_commonmark_example_257_insufficient_indent() {
    let content = " -    one\n\n     two";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker at column 1 (width 1) + 4 spaces = continuation at column 6
    // "two" has 5 spaces total, which is < 6 (required continuation)
    // So "two" is NOT part of the list
    // After the list ends, "two" with 5 spaces follows a blank line
    // Our implementation treats this as a potential code block (5 spaces > 4)
    // This is reasonable behavior - it's indented code after the list ends
    // CommonMark is ambiguous here since 5 spaces could be:
    // - A paragraph with extra indent (less than 6 needed for continuation)
    // - An indented code block (more than 4 needed for code)
    // Our choice to treat it as code block is defensible
    assert_eq!(
        blocks.len(),
        1,
        "Example 257: 5-space indent after list ends is treated as code block"
    );
}

/// Test multi-digit ordered list markers
#[test]
fn test_multidigit_ordered_list_not_code_block() {
    let content = "Paragraph\n\n    10. Item one\n    11. Item two\n\n    code";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // "10. Item one" and "11. Item two" should be recognized as list items
    // "code" after blank should be code block within the list
    // Marker "10." at column 4 (width 3) + 1 space = continuation at column 8
    // "code" at column 4 is < continuation, ends list
    // "code" with 4 spaces after blank = document code block? Actually no, it's after list
    // Let me reconsider: after "11. Item two", blank line, then "code" at 4 spaces
    // This should be continuation (column 8 required) or end list
    assert_eq!(
        blocks.len(),
        1,
        "Multi-digit ordered lists followed by 4-space indented line should create code block"
    );
}

/// Test ordered list with single space after marker
#[test]
fn test_ordered_list_single_space() {
    let content = "1. First\n\n   continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "1." at column 0 (width 2) + 1 space = continuation at column 3
    // "continuation" at column 3 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "Ordered list continuation with single space should not be code block"
    );
}

/// Test ordered list with multiple spaces after marker
#[test]
fn test_ordered_list_multiple_spaces() {
    let content = "1.     First\n\n       continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Per CommonMark: 5 spaces after "1." creates an indented code block
    // within the list item. Both "First" and "continuation" are code.
    // Verified with cmark: <ol><li><pre><code>First\n\ncontinuation</code></pre></li></ol>
    assert_eq!(
        blocks.len(),
        1,
        "5 spaces after ordered list marker creates code block (verified with cmark)"
    );
}

/// Test code block in ordered list
#[test]
fn test_code_block_in_ordered_list() {
    let content = "1. Item\n\n       code";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "1." at column 0 (width 2) + 1 space = continuation at column 3
    // "code" at column 7 = continuation (3) + 4 = code block in list
    assert_eq!(
        blocks.len(),
        1,
        "7 spaces in ordered list (continuation at 3 + 4) should be code block"
    );
}

/// Test unordered list with asterisk
#[test]
fn test_unordered_list_asterisk() {
    let content = "* Item\n\n  continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "*" at column 0 (width 1) + 1 space = continuation at column 2
    // "continuation" at column 2 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "Unordered list with * should not mark continuation as code block"
    );
}

/// Test unordered list with plus
#[test]
fn test_unordered_list_plus() {
    let content = "+ Item\n\n  continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "+" at column 0 (width 1) + 1 space = continuation at column 2
    // "continuation" at column 2 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "Unordered list with + should not mark continuation as code block"
    );
}

/// Test list with extra spaces after marker
#[test]
fn test_list_extra_spaces_after_marker() {
    let content = "-    Item with extra spaces\n\n     continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "-" at column 0 (width 1) + 4 spaces = continuation at column 5
    // "continuation" at column 5 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "List with 4 spaces after marker should calculate continuation indent correctly"
    );
}

/// Test code block with exactly 4 spaces at document level
#[test]
fn test_document_level_code_block_4_spaces() {
    let content = "Paragraph\n\n    code block";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // "code block" at 4 spaces after blank line = document-level code block
    assert_eq!(
        blocks.len(),
        1,
        "4 spaces at document level after blank should be code block"
    );
}

/// Test indented list followed by document-level code block
#[test]
fn test_list_then_document_code_block() {
    let content = "- Item\n\n  continuation\n\nCode after list\n\n    code block";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // First part: list with continuation (no code block)
    // After "Code after list", we have document-level code block
    assert_eq!(
        blocks.len(),
        1,
        "Should detect document-level code block after list ends"
    );
    assert!(
        content[blocks[0].0..blocks[0].1].contains("code block"),
        "Code block should be the last part"
    );
}

/// Test that 3 spaces is not a code block
#[test]
fn test_three_spaces_not_code_block() {
    let content = "Paragraph\n\n   not code";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Only 3 spaces, not enough for code block (need 4)
    assert_eq!(blocks.len(), 0, "3 spaces should not be code block");
}

/// Test ordered list with closing parenthesis
#[test]
fn test_ordered_list_closing_paren() {
    let content = "1) Item\n\n   continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "1)" at column 0 (width 2) + 1 space = continuation at column 3
    // "continuation" at column 3 = continuation paragraph
    assert_eq!(blocks.len(), 0, "Ordered list with ) delimiter should work correctly");
}

/// Test two-digit ordered list
#[test]
fn test_two_digit_ordered_list() {
    let content = "12. Item\n\n    continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "12." at column 0 (width 3) + 1 space = continuation at column 4
    // "continuation" at column 4 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "Two-digit ordered list should calculate continuation correctly"
    );
}

/// Test three-digit ordered list
#[test]
fn test_three_digit_ordered_list() {
    let content = "123. Item\n\n     continuation";
    let blocks = CodeBlockUtils::detect_code_blocks(content);

    // Marker "123." at column 0 (width 4) + 1 space = continuation at column 5
    // "continuation" at column 5 = continuation paragraph
    assert_eq!(
        blocks.len(),
        0,
        "Three-digit ordered list should calculate continuation correctly"
    );
}
