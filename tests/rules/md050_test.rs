use rumdl_lib::MD050StrongStyle;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::strong_style::StrongStyle;

#[test]
fn test_consistent_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is **strong** and this is also **strong**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_underscores() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "# Test\n\nThis is __strong__ and this is also __strong__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_strong_prefer_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is **asterisk** and this is **underscore**"));
}

#[test]
fn test_mixed_strong_prefer_underscores() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is __asterisk__ and this is __underscore__"));
}

#[test]
fn test_consistent_style_first_asterisk() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is **asterisk** and this is **underscore**"));
}

#[test]
fn test_consistent_style_first_underscore() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Mixed strong\n\nThis is __underscore__ and this is **asterisk**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is __underscore__ and this is __asterisk__"));
}

#[test]
fn test_empty_content() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_strong() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_emphasis() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasis* and this is **strong**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_strong_in_code_spans() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"# Test

This is **bold** text.

In inline code: `__text__` and `**text**` should be ignored.

Also in code blocks:

```markdown
Use **asterisks** or __underscores__ for bold.
```

Another **bold** word here.
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not detect strong text inside code spans or blocks
    assert_eq!(result.len(), 0, "Should not detect strong text in code spans or blocks");

    // Test with underscore preference
    let rule_underscore = MD050StrongStyle::new(StrongStyle::Underscore);
    let result_underscore = rule_underscore.check(&ctx).unwrap();

    // Should only detect the two **bold** outside code
    assert_eq!(result_underscore.len(), 2, "Should only detect bold text outside code");
    assert_eq!(result_underscore[0].line, 3); // First **bold**
    assert_eq!(result_underscore[1].line, 13); // Another **bold**

    // Test the fix
    let fixed = rule_underscore.fix(&ctx).unwrap();

    // Should fix only the bold text outside code
    assert!(fixed.contains("This is __bold__ text."));
    assert!(fixed.contains("Another __bold__ word"));

    // Should NOT fix text inside code
    assert!(fixed.contains("`__text__`"));
    assert!(fixed.contains("`**text**`"));
    assert!(fixed.contains("Use **asterisks** or __underscores__ for bold."));
}
