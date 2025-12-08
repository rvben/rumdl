use rumdl_lib::MD050StrongStyle;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::strong_style::StrongStyle;

#[test]
fn test_consistent_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is **strong** and this is also **strong**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_underscores() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "# Test\n\nThis is __strong__ and this is also __strong__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_strong_prefer_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is **asterisk** and this is **underscore**"));
}

#[test]
fn test_consistent_style_first_underscore() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    // One underscore and one asterisk - tie prefers asterisk
    let content = "# Mixed strong\n\nThis is __underscore__ and this is **asterisk**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Tie-breaker prefers asterisk (matches CommonMark recommendation)
    assert!(fixed.contains("This is **underscore** and this is **asterisk**"));
}

#[test]
fn test_empty_content() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_strong() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_emphasis() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasis* and this is **strong**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

#[test]
fn test_md050_html_code_content() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    // Test emphasis inside HTML code tags should be skipped
    let content = r#"# Test MD050 with HTML code tags

This is <code>__pycache__</code> in HTML code.

This is real emphasis: __emphasized text__

More examples: <code>__init__.py</code>, <code>__main__.py</code>

Mixed: __real__ emphasis and <code>__code__</code> together"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the real emphasis (lines 5 and 9), not the code content
    assert_eq!(warnings.len(), 2, "Should only flag real emphasis, not code content");
    assert_eq!(warnings[0].line, 5);
    assert_eq!(warnings[0].message, "Strong emphasis should use ** instead of __");
    assert_eq!(warnings[1].line, 9);
}

#[test]
fn test_md050_nested_html_code() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    let content = r#"# Nested HTML code tags

<p>Uses patterns like <code>**/__pycache__/**</code> for globbing.</p>

Real emphasis: __should be flagged__"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag line 5, not the content in code tags on line 3
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 5);
}

#[test]
fn test_md050_multiple_code_tags() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    let content = r#"# Multiple code tags

The <code>__init__</code> method and <code>__name__</code> variable.

Between tags: __this should be flagged__

After tags <code>__main__</code> more text __also flagged__"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should flag lines 5 and 7 but not the code content
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 5);
    assert_eq!(warnings[1].line, 7);
}

#[test]
fn test_md050_self_closing_code_tag() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    // Test with emphasis on separate lines first
    let content_separate = r#"# Self-closing code tag

<code />
__should be flagged__

<code/>
__also flagged__"#;

    let ctx =
        rumdl_lib::lint_context::LintContext::new(content_separate, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Self-closing code tags don't create a code context
    assert_eq!(warnings.len(), 2, "Should detect emphasis on separate lines");
    assert_eq!(warnings[0].line, 4);
    assert_eq!(warnings[1].line, 7);

    // Now test with emphasis on same line as self-closing tag
    let content = r#"# Self-closing code tag

<code /> __should be flagged__

<code/> __also flagged__"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // These should also be flagged
    assert_eq!(
        warnings.len(),
        2,
        "Should detect emphasis on same line as self-closing tag"
    );
    assert_eq!(warnings[0].line, 3);
    assert_eq!(warnings[1].line, 5);
}

#[test]
fn test_md050_code_with_attributes() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    let content = r#"# Code tags with attributes

<code class="python">__init__.py</code> is a special file.

Regular __emphasis__ here."#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag line 5
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 5);
}

#[test]
fn test_md050_fix_preserves_html_code() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    let content = r#"# Fix test

Uses <code>__pycache__</code> but __this__ should be fixed."#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should preserve code content but fix the emphasis
    assert!(fixed.contains("<code>__pycache__</code>"));
    assert!(fixed.contains("**this**"));
    assert!(!fixed.contains(" __this__ "));
}

#[test]
fn test_md050_complex_html_structure() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    let content = r#"# Complex HTML

<div>
  <p>Text with <code>__special__</code> names.</p>
  <p>And __emphasis__ outside code.</p>
</div>

<span>More <code>__code__</code> content</span> and __emphasis__."#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should flag lines 5 and 8
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 5);
    assert_eq!(warnings[1].line, 8);
}

#[test]
fn test_issue_118_underscores_in_link_title_with_code() {
    // Regression test for Issue #118
    // MD050 should not flag underscores in link titles that contain code
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"Here is a link with code in the hover text:

- [An odd but sensible use of `super`](https://www.pythonmorsels.com/how-not-to-use-super/#an-odd-but-sensible-use-of-super "Calling `super().__setitem__` might make sense, depending on how you've implemented your class")
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag __setitem__ inside the quoted title attribute
    assert_eq!(
        result.len(),
        0,
        "MD050 should not flag code with underscores in link title attributes (issue #118)"
    );
}

#[test]
fn test_issue_118_parentheses_in_link_titles() {
    // Regression test for Issue #118
    // MD050 should handle link titles containing parentheses
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"[Link text](https://example.com "Title (with parentheses)")

[Another link](https://example.com "Function call like `func()`")
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag anything - parentheses in titles are valid
    assert_eq!(
        result.len(),
        0,
        "MD050 should handle parentheses in link titles (issue #118)"
    );
}

#[test]
fn test_issue_118_full_document() {
    // Regression test for Issue #118
    // Test the complete document from the issue report
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"Here is **example 1**:

```bash
$ python one_up.py
What's your favorite number? 7
I can one up that.
Traceback (most recent call last):
  File "/home/trey/one_up.py", line 3, in <module>
    print(favorite_number+1)
          ~~~~~~~~~~~~~~~^~
TypeError: can only concatenate str (not "int") to str
```

Here is **example 2**:

```bash
$ python one_up.py
What's your favorite number? 7.82
Traceback (most recent call last):
  File "/home/trey/one_up.py", line 1, in <module>
    favorite_number = int(input("What's your favorite number? "))
                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
ValueError: invalid literal for int() with base 10: '7.82'
```

Here is a link with code in the hover text:

- [An odd but sensible use of `super`](https://www.pythonmorsels.com/how-not-to-use-super/#an-odd-but-sensible-use-of-super "Calling `super().__setitem__` might make sense, depending on how you've implemented your class")
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not report any issues with the full document
    assert_eq!(
        result.len(),
        0,
        "MD050 should not report any issues with Issue #118 document"
    );
}
