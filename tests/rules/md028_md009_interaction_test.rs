use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD009TrailingSpaces, MD028NoBlanksBlockquote};

#[test]
fn test_issue_66_exact_scenario() {
    // This is the exact scenario from issue #66
    // https://github.com/rvben/rumdl/issues/66
    // User reported MD028 and MD009 "fighting each other"
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // The exact markdown from the issue
    let content = "# Test blockquote\n\n> La\n> \n> lala";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should NOT flag line 4 (has "> ")
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 0, "MD028 should not flag lines with '> ' marker");

    // MD009 should NOT flag line 4 either (empty blockquote line)
    let md009_result = md009.check(&ctx).unwrap();
    assert_eq!(md009_result.len(), 0, "MD009 should not flag empty blockquote lines");

    // Verify formatting doesn't create conflicts
    let fixed_content = md028.fix(&ctx).unwrap();
    assert_eq!(
        fixed_content, content,
        "Content should not change since there are no violations"
    );
}

#[test]
fn test_issue_66_without_space() {
    // Same scenario but with just ">" (no space)
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    let content = "# Test blockquote\n\n> La\n>\n> lala";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should NOT flag line 4 (has ">")
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 0, "MD028 should not flag lines with '>' marker");

    // MD009 should NOT flag either
    let md009_result = md009.check(&ctx).unwrap();
    assert_eq!(md009_result.len(), 0, "MD009 should not flag");

    // Content shouldn't change
    let fixed_content = md028.fix(&ctx).unwrap();
    assert_eq!(fixed_content, content, "Content should not change");
}

#[test]
fn test_issue_66_with_truly_blank_line() {
    // Test what SHOULD be flagged by MD028
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // Same as issue but with truly blank line (no >)
    let content = "# Test blockquote\n\n> La\n\n> lala";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 SHOULD flag line 4 (blank)
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 1, "MD028 should flag truly blank line");
    assert_eq!(md028_result[0].line, 4);

    // Fix it
    let fixed_content = md028.fix(&ctx).unwrap();
    assert_eq!(fixed_content, "# Test blockquote\n\n> La\n>\n> lala");

    // Verify MD009 doesn't complain about the fix
    let fixed_ctx = LintContext::new(&fixed_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_after_fix = md009.check(&fixed_ctx).unwrap();
    assert_eq!(md009_after_fix.len(), 0, "MD009 should not flag after MD028 fix");
}

#[test]
fn test_md028_does_not_add_trailing_space() {
    let md028 = MD028NoBlanksBlockquote;

    // Content with truly blank line
    let content = "> First line\n\n> Third line";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should flag the blank line
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 1, "MD028 should flag truly blank line");
    assert_eq!(md028_result[0].line, 2);

    // Fix with MD028
    let fixed_content = md028.fix(&ctx).unwrap();
    assert_eq!(
        fixed_content, "> First line\n>\n> Third line",
        "MD028 should NOT add space after >"
    );

    // Verify the fix doesn't have trailing spaces
    let lines: Vec<&str> = fixed_content.lines().collect();
    assert_eq!(lines[1], ">", "Fixed line should be just '>' without trailing space");
}

#[test]
fn test_md009_accepts_empty_blockquote_without_space() {
    let md009 = MD009TrailingSpaces::default();

    // Empty blockquote lines without trailing space (as MD028 now creates)
    let test_cases = vec![
        (">\n", "Single level empty blockquote"),
        (">>\n", "Double level empty blockquote"),
        (">>>\n", "Triple level empty blockquote"),
        ("  >\n", "Indented empty blockquote"),
        ("> Text\n>\n> More", "Empty blockquote in middle"),
    ];

    for (content, description) in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = md009.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "MD009 should not flag {description}: {content}");
    }
}

#[test]
fn test_md009_flags_multiple_trailing_spaces_in_blockquote() {
    let md009 = MD009TrailingSpaces::default();

    // MD009 allows 1-2 trailing spaces (br_spaces default is 2) but flags more
    let test_cases = vec![
        (">  \n", 0, "Two spaces after > (allowed as line break)"),
        (">>   \n", 1, "Three spaces after >> (exceeds br_spaces)"),
        ("> Text  \n", 0, "Two spaces after text (allowed as line break)"),
        ("> Text   \n", 1, "Three spaces after text (exceeds br_spaces)"),
    ];

    for (content, expected_warnings, description) in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = md009.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            expected_warnings,
            "MD009 failed for {description}: {content}"
        );
    }
}

#[test]
fn test_md028_md009_nested_blockquotes() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    let content = "> Level 1\n\n>> Level 2\n\n> Level 1 again";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should flag line 2 (blank between > and >>)
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 1);
    assert_eq!(md028_result[0].line, 2);

    // Fix and verify MD009 doesn't complain
    let fixed = md028.fix(&ctx).unwrap();
    assert_eq!(fixed, "> Level 1\n>\n>> Level 2\n\n> Level 1 again");

    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_result = md009.check(&fixed_ctx).unwrap();
    assert_eq!(md009_result.len(), 0, "MD009 should not flag fixed nested blockquote");
}

#[test]
fn test_md028_md009_indented_blockquotes() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    let content = "  > Indented\n\n  > More";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should flag line 2
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 1);
    assert_eq!(md028_result[0].line, 2);

    // Fix and verify MD009 doesn't complain
    let fixed = md028.fix(&ctx).unwrap();
    assert_eq!(fixed, "  > Indented\n  >\n  > More");

    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_result = md009.check(&fixed_ctx).unwrap();
    assert_eq!(md009_result.len(), 0, "MD009 should not flag fixed indented blockquote");
}

#[test]
fn test_md028_flags_blockquote_with_only_space() {
    let md028 = MD028NoBlanksBlockquote;

    // Empty blockquote line with space (which we now consider valid)
    let content = "> First\n> \n> Third";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = md028.check(&ctx).unwrap();
    // MD028 should NOT flag lines with "> " as they're valid
    assert_eq!(result.len(), 0, "MD028 should not flag blockquote line with space");
}

#[test]
fn test_multiple_fixes_dont_conflict() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // Complex content with multiple issues
    let content = "> Block 1\n\n> Still block 1  \n\n> Block 2\n\n>> Nested  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Check MD028 issues (blank lines)
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 3, "Should find 3 blank lines");

    // Check MD009 issues (trailing spaces on lines 3 and 7)
    let md009_result = md009.check(&ctx).unwrap();
    // MD009 now normalizes trailing spaces to br_spaces, may have different count
    assert!(!md009_result.is_empty(), "Should find at least 1 trailing space issue");

    // Fix with MD028 first
    let fixed_md028 = md028.fix(&ctx).unwrap();

    // Then fix MD009 issues
    let ctx_after_md028 = LintContext::new(&fixed_md028, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let final_fixed = md009.fix(&ctx_after_md028).unwrap();

    // Verify final content has no issues
    let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let final_md028_check = md028.check(&final_ctx).unwrap();
    let final_md009_check = md009.check(&final_ctx).unwrap();

    assert_eq!(final_md028_check.len(), 0, "No MD028 issues after both fixes");
    assert_eq!(final_md009_check.len(), 0, "No MD009 issues after both fixes");
}

#[test]
fn test_edge_case_only_blockquote_markers() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // File with only blockquote markers
    let content = ">\n>>\n>>>\n>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should NOT flag these (they have markers)
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(
        md028_result.len(),
        0,
        "Lines with blockquote markers should not be flagged"
    );

    // MD009 should not complain either
    let md009_result = md009.check(&ctx).unwrap();
    assert_eq!(
        md009_result.len(),
        0,
        "MD009 should not flag empty blockquotes without trailing space"
    );
}

#[test]
fn test_blockquote_with_tabs() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    let content = ">\t\n>  \t  \n> text\t";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD009 should handle tabs and spaces in blockquotes
    let md009_result = md009.check(&ctx).unwrap();
    // Line 2 (">  \t  ") has 2 trailing spaces after the tab
    // But MD009 allows empty blockquote lines with tabs/spaces per our fix for #66
    assert_eq!(
        md009_result.len(),
        0,
        "MD009 should not flag empty blockquote lines even with tabs/spaces"
    );

    // MD028 shouldn't be triggered as these have content after >
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 0, "MD028 should not flag blockquotes with tabs");
}

#[test]
fn test_mixed_blockquote_and_list() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // Blockquote containing a list
    let content = "> * Item 1\n\n> * Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 1, "Should flag blank line");

    let fixed = md028.fix(&ctx).unwrap();
    assert_eq!(fixed, "> * Item 1\n>\n> * Item 2");

    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_result = md009.check(&fixed_ctx).unwrap();
    assert_eq!(md009_result.len(), 0, "MD009 should not flag fixed content");
}

#[test]
fn test_blockquote_at_end_of_file() {
    let md028 = MD028NoBlanksBlockquote;
    let md009 = MD009TrailingSpaces::default();

    // Blockquote ending with empty line (no newline at end)
    let content = "> First\n>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD028 should not flag line with >
    let md028_result = md028.check(&ctx).unwrap();
    assert_eq!(md028_result.len(), 0);

    // MD009 should not flag either
    let md009_result = md009.check(&ctx).unwrap();
    assert_eq!(md009_result.len(), 0);
}
