use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD030ListMarkerSpace;

#[test]
fn test_valid_single_line_lists() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* Item\n- Another item\n+ Third item\n1. Ordered item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_multi_line_lists() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* First line\n  continued\n- Second item\n  also continued";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_spaces_unordered() {
    let rule = MD030ListMarkerSpace::default();
    let content = "*  Too many spaces\n-   Three spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_spaces_ordered() {
    let rule = MD030ListMarkerSpace::default();
    let content = "1.  Too many spaces\n2.   Three spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_unordered_list() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "*  Item\n-   Another\n+    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "*  Item\n-   Another\n+    Third");
}

#[test]
fn test_fix_ordered_list() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "1.  First\n2.   Second\n3.    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    if fixed != "1.  First\n2.   Second\n3.    Third" {
        eprintln!(
            "[DEBUG] test_fix_ordered_list: actual=\n{:?}\nexpected=\n{:?}",
            fixed, "1.  First\n2.   Second\n3.    Third"
        );
    }
    assert_eq!(fixed, "1.  First\n2.   Second\n3.    Third");
}

#[test]
fn test_custom_spacing() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "* One space\n- One space\n1. One space";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "*  One space\n-  One space\n1.  One space");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "*  Unordered\n1.  Ordered\n-   Mixed\n2.   Types";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    if fixed != "*  Unordered\n1.  Ordered\n-   Mixed\n2.   Types" {
        eprintln!(
            "[DEBUG] test_mixed_list_types: actual=\n{:?}\nexpected=\n{:?}",
            fixed, "*  Unordered\n1.  Ordered\n-   Mixed\n2.   Types"
        );
    }
    assert_eq!(fixed, "*  Unordered\n1.  Ordered\n-   Mixed\n2.   Types");
}

#[test]
fn test_nested_lists() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "* First\n  *  Nested\n    *   More nested";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "*   First\n  *   Nested\n    *   More nested");
}

#[test]
fn test_ignore_code_blocks() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* Normal item\n```\n*  Not a list\n1.  Not a list\n```\n- Back to list";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multi_line_items() {
    let rule = MD030ListMarkerSpace::new(1, 2, 1, 2);
    let content = "* Single line\n* Multi line\n  continued here\n* Another single";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // All items in the multi-line context should be flagged
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "*  Single line\n*  Multi line\n  continued here\n*  Another single");
}

#[test]
fn test_readme_md030_config() {
    let rule = MD030ListMarkerSpace::new(1, 3, 1, 2);
    let content = r#"# A title

Single ol:

1. one
1. two
1. three

Single ul:

- one
- two
- three

Unordered nested list:

-   one
    wrapped
-   two
    -   three
        wrapped
    -   four
-   five
    - six
    - seven

Ordered nested list:

1.  one
    wrapped
1.  two
    1.  three
        wrapped
    1.  four
1.  five
    1. six
    1. seven

Mixed nested lists A:

1.  one
    wrapped
1.  two
    -   three
        wrapped
    -   four
1.  five

Mixed nested lists A:

-   one
    wrapped
-   two
    1.   three
        wrapped
    1.  four
-   five
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    let flagged_lines: Vec<_> = result.iter().map(|w| w.line).collect();
    // All items in multi-line contexts should be flagged
    assert!(flagged_lines.contains(&5), "Should flag line 5 (too few spaces)");
    assert!(flagged_lines.contains(&6), "Should flag line 6 (too few spaces)");
    assert!(flagged_lines.contains(&7), "Should flag line 7 (too few spaces)");
    assert!(flagged_lines.contains(&11), "Should flag line 11 (too few spaces)");
    assert!(flagged_lines.contains(&12), "Should flag line 12 (too few spaces)");
    assert!(flagged_lines.contains(&13), "Should flag line 13 (too few spaces)");
    assert!(flagged_lines.contains(&24), "Should flag line 24 (too few spaces)");
    assert!(flagged_lines.contains(&25), "Should flag line 25 (too few spaces)");
    assert!(flagged_lines.contains(&36), "Should flag line 36 (too few spaces)");
    assert!(flagged_lines.contains(&37), "Should flag line 37 (too few spaces)");
    let fixed = rule.fix(&ctx).unwrap();
    let expected = "# A title\n\nSingle ol:\n\n1.   one\n1.   two\n1.   three\n\nSingle ul:\n\n-   one\n-   two\n-   three\n\nUnordered nested list:\n\n-   one\n    wrapped\n-   two\n    -   three\n        wrapped\n    -   four\n-   five\n    -   six\n    -   seven\n\nOrdered nested list:\n\n1.  one\n    wrapped\n1.  two\n    1.  three\n        wrapped\n    1.  four\n1.  five\n    1.  six\n    1.  seven\n\nMixed nested lists A:\n\n1.  one\n    wrapped\n1.  two\n    -   three\n        wrapped\n    -   four\n1.  five\n\nMixed nested lists A:\n\n-   one\n    wrapped\n-   two\n    1.   three\n        wrapped\n    1.  four\n-   five";
    assert_eq!(fixed, expected, "Fixed output should match the correct, spec-compliant Markdown");
}

#[test]
fn test_blockquoted_list_spacing() {
    let rule = MD030ListMarkerSpace::default();
    let content = "> *  Too many spaces\n> -   Three spaces\n> + Item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Per the spec, extra spaces after the marker are allowed; should not flag any warnings
    assert!(result.is_empty(), "Should not flag extra spaces after list marker in blockquotes");
}

#[test]
fn test_nested_blockquoted_list_spacing() {
    let rule = MD030ListMarkerSpace::default();
    let content = "> * Item\n>   *  Nested\n>     *   More nested";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Per the spec, extra spaces after the marker are allowed; should not flag any warnings
    assert!(result.is_empty(), "Should not flag extra spaces after list marker in nested blockquotes");
}

#[test]
fn test_blockquote_with_code_block_and_list() {
    let rule = MD030ListMarkerSpace::default();
    let content = "> ```\n> code\n> ```\n> *  List after code block\n> - List ok";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Per the spec, extra spaces after the marker are allowed; should not flag any warnings
    assert!(result.is_empty(), "Should not flag extra spaces after list marker in blockquotes after code block");
}

#[test]
fn test_parity_single_vs_multi_line_spacing() {
    // This test ensures that lists with no continuation lines are treated as single-line by both rumdl and markdownlint
    // and that the fix does not add extra spaces beyond the single-line config.
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "  *  Item\n    -   Another\n      +    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    // The expected output is the same as the input, since all items are single-line (no continuations)
    assert_eq!(fixed, content, "Single-line lists should not be fixed to multi-line spacing");
    // Optionally, if you have a markdownlint CLI parity harness, you can add a CLI check here as well.
}
