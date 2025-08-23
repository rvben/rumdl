use rumdl_lib::MD049EmphasisStyle;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::emphasis_style::EmphasisStyle;

#[test]
fn test_consistent_asterisks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasized* and this is also *emphasized*";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let content = "# Test\n\nThis is _emphasized_ and this is also _emphasized_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis_prefer_asterisks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is *asterisk* and this is *underscore*"));
}

#[test]
fn test_mixed_emphasis_prefer_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is _asterisk_ and this is _underscore_"));
}

#[test]
fn test_consistent_style_first_asterisk() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is *asterisk* and this is *underscore*"));
}

#[test]
fn test_consistent_style_first_underscore() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Mixed emphasis\n\nThis is _underscore_ and this is *asterisk*";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(fixed.contains("This is _underscore_ and this is _asterisk_"));
}

#[test]
fn test_empty_content() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_emphasis() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_strong_emphasis() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasis* and this is **strong**";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_urls_with_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test URL with underscores
    let content = "Here is a [link](https://example.com/page_with_underscores)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URLs with underscores should not be flagged as emphasis"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "URLs with underscores should not be modified");

    // Test complex content with URLs and real emphasis
    let content = "Check out this _emphasis_ and visit [our site](https://example.com/docs/user_guide/page_name)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only the real emphasis should be detected");
    let fixed = rule.fix(&ctx).unwrap();
    // Use contains for more flexible assertion
    assert!(
        fixed.contains("Check out this *emphasis* and visit [our site]"),
        "Only the real emphasis should be changed"
    );

    // Test with multiple URLs and emphasis
    let content = "Visit these links:\n- [Link 1](https://example.com/some_path)\n- [Link 2](https://docs.gitlab.com/ee/user/project/merge_requests/creating_merge_requests.html)\nAnd remember to _check_ the documentation.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only the real emphasis should be detected");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("https://example.com/some_path"),
        "URL should remain unchanged"
    );
    assert!(
        fixed.contains("https://docs.gitlab.com/ee/user/project/merge_requests/creating_merge_requests.html"),
        "Complex URL should remain unchanged"
    );
    assert!(fixed.contains("*check*"), "Emphasis should be converted to asterisks");
}

#[test]
fn test_inline_code_with_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test inline code with underscores
    let content = "Use the `function_name()` in your code and _emphasize_ important parts.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect one emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("`function_name()`"),
        "Inline code should not be modified"
    );
    assert!(
        fixed.contains("*emphasize*"),
        "Emphasis should be converted to asterisks"
    );
}

#[test]
fn test_multiple_backticks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test double backticks
    let content = "Use ``code with `backtick` inside`` and _emphasis_.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect one emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("``code with `backtick` inside``"),
        "Double backtick code should not be modified"
    );
    assert!(
        fixed.contains("*emphasis*"),
        "Emphasis should be converted to asterisks"
    );

    // Test triple backticks
    let content = "Use ```code with `backticks` and ``more`` inside``` and _emphasis_.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect one emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("```code with `backticks` and ``more`` inside```"),
        "Triple backtick code should not be modified"
    );
    assert!(
        fixed.contains("*emphasis*"),
        "Emphasis should be converted to asterisks"
    );
}

#[test]
fn test_code_blocks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test code blocks with emphasis-like content
    let content = "Before _emphasis_\n```\nSome _code_ here\n```\nAfter _emphasis_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect two emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("Before *emphasis*"),
        "Emphasis before code block should be fixed"
    );
    assert!(
        fixed.contains("Some _code_ here"),
        "Content in code block should not be modified"
    );
    assert!(
        fixed.contains("After *emphasis*"),
        "Emphasis after code block should be fixed"
    );

    // Test with tildes
    let content = "Before _emphasis_\n~~~\nSome _code_ here\n~~~\nAfter _emphasis_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect two emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("Before *emphasis*"),
        "Emphasis before code block should be fixed"
    );
    assert!(
        fixed.contains("Some _code_ here"),
        "Content in code block should not be modified"
    );
    assert!(
        fixed.contains("After *emphasis*"),
        "Emphasis after code block should be fixed"
    );
}

#[test]
fn test_environment_variables() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test environment variables in backticks
    let content = "Set `GITLAB_URL` and `CI_PROJECT_ID` variables and _note_ the values.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect one emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("`GITLAB_URL`"),
        "Environment variable should not be modified"
    );
    assert!(
        fixed.contains("`CI_PROJECT_ID`"),
        "Environment variable should not be modified"
    );
    assert!(fixed.contains("*note*"), "Emphasis should be converted to asterisks");
}

#[test]
fn test_nested_code_and_emphasis() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test complex nesting
    let content =
        "1. First step with _emphasis_\n   ```bash\n   echo \"some _code_\"\n   ```\n2. Second step with _emphasis_";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect two emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("First step with *emphasis*"),
        "First emphasis should be fixed"
    );
    assert!(
        fixed.contains("echo \"some _code_\""),
        "Code block content should not be modified"
    );
    assert!(
        fixed.contains("Second step with *emphasis*"),
        "Second emphasis should be fixed"
    );

    // Test with indented code and emphasis
    let content = "1. First step with _emphasis_\n    ```\n    some _code_\n    ```\n   And _more_ text";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect two emphasis to fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("First step with *emphasis*"),
        "First emphasis should be fixed"
    );
    assert!(
        fixed.contains("some _code_"),
        "Code block content should not be modified"
    );
    assert!(fixed.contains("And *more* text"), "Second emphasis should be fixed");
}
