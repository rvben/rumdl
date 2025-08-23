use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD039NoSpaceInLinks;

#[test]
fn test_valid_links() {
    let rule = MD039NoSpaceInLinks;
    let content = "[link](url) and [another link](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_both_ends() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link ](url) and [ another link ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_space_at_start() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link](url) and [ another link](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_space_at_end() {
    let rule = MD039NoSpaceInLinks;
    let content = "[link ](url) and [another link ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_link_in_code_block() {
    let rule = MD039NoSpaceInLinks;
    let content = "```\n[ link ](url)\n```\n[ link ](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\n[ link ](url)\n```\n[link](url)");
}

#[test]
fn test_multiple_links() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link ](url) and [ another ](url) in one line";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link](url) and [another](url) in one line");
}

#[test]
fn test_link_with_internal_spaces() {
    let rule = MD039NoSpaceInLinks;
    let content = "[this is link](url) and [ this is also link ](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
}

#[test]
fn test_link_with_punctuation() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link! ](url) and [ link? ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link!](url) and [link?](url) here");
}

mod parity_with_markdownlint {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD039NoSpaceInLinks;

    #[test]
    fn parity_leading_trailing_space() {
        let input = "[ link](url) and [another link ](url)";
        let expected = "[link](url) and [another link](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_both_ends_spaced() {
        let input = "[ link ](url) and [ another link ](url)";
        let expected = "[link](url) and [another link](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_internal_spaces_only() {
        let input = "[this is link](url) and [another link](url)";
        let expected = "[this is link](url) and [another link](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn parity_code_block_containing_links() {
        let input = "```
[ link ](url)
```
[ link ](url)";
        let expected = "```
[ link ](url)
```
[link](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn parity_multiple_links_per_line() {
        let input = "[ link ](url) and [ another ](url) in one line";
        let expected = "[link](url) and [another](url) in one line";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_punctuation_in_link_text() {
        let input = "[ link! ](url) and [ link? ](url) here";
        let expected = "[link!](url) and [link?](url) here";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_link_text_only_spaces() {
        let input = "[   ](url) and [ ](url)";
        // markdownlint removes all spaces, resulting in empty link text
        let expected = "[](url) and [](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_reference_style_links() {
        let input = "[ link ][ref] and [ another ][ref2]\n\n[ref]: url\n[ref2]: url2";
        // markdownlint does not fix reference-style links for MD039, so output is unchanged
        let expected = "[ link ][ref] and [ another ][ref2]\n\n[ref]: url\n[ref2]: url2";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        // Should not warn on reference-style links
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn parity_unicode_whitespace() {
        let input = "[\u{00A0}link\u{00A0}](url) and [\u{2003}another\u{2003}](url)"; // non-breaking space and em space
        let expected = "[link](url) and [another](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_tab_whitespace() {
        let input = "[\tlink\t](url) and [\tanother\t](url)";
        let expected = "[link](url) and [another](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_only_whitespace_and_newlines() {
        let input = "[   \n  ](url) and [\t\n\t](url)";
        let expected = "[](url) and [](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_internal_newlines() {
        let input = "[link\ntext](url) and [ another\nlink ](url)";
        let expected = "[link\ntext](url) and [another\nlink](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn parity_nested_formatting() {
        let input = "[ * link * ](url) and [ _ another _ ](url)";
        let expected = "[* link *](url) and [_ another _](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_escaped_brackets() {
        let input = "[link\\]](url) and [link\\[]](url)";
        let expected = "[link\\]](url) and [link\\[]](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn parity_inline_images() {
        let input = "![ alt ](img.png) and ![ another ](img2.png)";
        let expected = "![alt](img.png) and ![another](img2.png)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn parity_html_entities() {
        let input = "[ &nbsp;link&nbsp; ](url)";
        let expected = "[&nbsp;link&nbsp;](url)";
        let ctx = LintContext::new(input);
        let rule = MD039NoSpaceInLinks::new();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }
}
