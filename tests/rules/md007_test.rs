use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD007ULIndent;

#[test]
fn test_valid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid indentation, but got {} warnings",
        result.len()
    );
}

#[test]
fn test_invalid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("test_invalid_list_indent: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 4);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[1].column, 7);
}

#[test]
fn test_mixed_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n   * Item 3\n  * Item 4";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("test_mixed_indentation: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].column, 4);
}

#[test]
fn test_fix_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    let expected = "* Item 1\n  * Item 2\n    * Item 3";
    assert_eq!(result, expected);
}

#[test]
fn test_md007_in_yaml_code_block() {
    let rule = MD007ULIndent::default();
    let content = r#"```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.5.0
    hooks:
    -   id: rumdl-check
```"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block, but got warnings: {:?}",
        result
    );
}

#[test]
fn test_blockquoted_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>     * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid blockquoted list indentation, but got {:?}",
        result
    );
}

#[test]
fn test_blockquoted_list_invalid_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>    * Item 2\n>       * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings for invalid blockquoted list indentation, got {:?}",
        result
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_nested_blockquote_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid nested blockquoted list indentation, but got {:?}",
        result
    );
}

#[test]
fn test_blockquote_list_with_code_block() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>   ```\n>   code\n>   ```\n>   * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block within a blockquote, but got warnings: {:?}",
        result
    );
}

mod parity_with_markdownlint {
    use rumdl::lint_context::LintContext;
    use rumdl::rule::Rule;
    use rumdl::rules::MD007ULIndent;

    #[test]
    fn parity_flat_list_default_indent() {
        let input = "* Item 1\n* Item 2\n* Item 3";
        let expected = "* Item 1\n* Item 2\n* Item 3";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_nested_list_default_indent() {
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n  * Nested 1\n    * Nested 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_nested_list_incorrect_indent() {
        let input = "* Item 1\n * Nested 1\n   * Nested 2";
        let expected = "* Item 1\n  * Nested 1\n    * Nested 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_mixed_markers() {
        let input = "* Item 1\n  - Nested 1\n    + Nested 2";
        let expected = "* Item 1\n  - Nested 1\n    + Nested 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_blockquote_list() {
        let input = "> * Item 1\n>   * Nested";
        let expected = "> * Item 1\n>   * Nested";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_tabs_for_indent() {
        let input = "* Item 1\n\t* Nested";
        let expected = "* Item 1\n  * Nested";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_code_block_ignored() {
        let input = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let expected = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_custom_indent_4() {
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n    * Nested 1\n        * Nested 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::new(4);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_empty_input() {
        let input = "";
        let expected = "";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_no_lists() {
        let input = "# Heading\nSome text";
        let expected = "# Heading\nSome text";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_list_with_blank_lines_between_items() {
        let input = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let expected = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, expected,
            "Nested items should maintain proper indentation even after blank lines"
        );
    }

    #[test]
    fn parity_list_items_with_trailing_whitespace() {
        let input = "* Item 1   \n  * Nested item 1   \n* Item 2   ";
        let expected = "* Item 1   \n  * Nested item 1   \n* Item 2   ";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_deeply_nested_blockquotes_with_lists() {
        let input = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let expected = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_inconsistent_marker_styles_different_nesting() {
        let input = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let expected = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_mixed_tabs_and_spaces_in_indentation() {
        let input = "* Item 1\n\t* Nested item 1\n  \t* Nested item 2\n* Item 2";
        let expected = "* Item 1\n  * Nested item 1\n  * Nested item 2\n* Item 2";
        let ctx = LintContext::new(input);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }
}
