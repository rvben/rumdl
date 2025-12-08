use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

#[test]
fn test_list_continuation_not_marked_as_code_block() {
    let content = r#"- Item

    This is a list continuation paragraph.
    It should NOT be marked as in_code_block.

Normal paragraph.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 0: "- Item"
    assert!(!ctx.lines[0].in_code_block, "List item should not be in code block");

    // Line 1: ""
    assert!(!ctx.lines[1].in_code_block, "Blank line should not be in code block");

    // Line 2: "    This is a list continuation paragraph."
    assert!(
        !ctx.lines[2].in_code_block,
        "List continuation paragraph should NOT be marked as in_code_block, but it was! This is the bug."
    );

    // Line 3: "    It should NOT be marked as in_code_block."
    assert!(
        !ctx.lines[3].in_code_block,
        "List continuation paragraph should NOT be marked as in_code_block, but it was!"
    );

    // Line 4: ""
    assert!(!ctx.lines[4].in_code_block, "Blank line should not be in code block");

    // Line 5: "Normal paragraph."
    assert!(
        !ctx.lines[5].in_code_block,
        "Normal paragraph should not be in code block"
    );
}

#[test]
fn test_indented_code_block_is_marked_correctly() {
    let content = r#"Paragraph.

    This is an indented code block.
    It SHOULD be marked as in_code_block.

Normal paragraph.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 0: "Paragraph."
    assert!(
        !ctx.lines[0].in_code_block,
        "Normal paragraph should not be in code block"
    );

    // Line 1: ""
    assert!(!ctx.lines[1].in_code_block, "Blank line should not be in code block");

    // Line 2: "    This is an indented code block."
    assert!(
        ctx.lines[2].in_code_block,
        "Indented code block SHOULD be marked as in_code_block"
    );

    // Line 3: "    It SHOULD be marked as in_code_block."
    assert!(
        ctx.lines[3].in_code_block,
        "Indented code block SHOULD be marked as in_code_block"
    );

    // Line 4: ""
    assert!(!ctx.lines[4].in_code_block, "Blank line should not be in code block");

    // Line 5: "Normal paragraph."
    assert!(
        !ctx.lines[5].in_code_block,
        "Normal paragraph should not be in code block"
    );
}
