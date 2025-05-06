use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD006StartBullets;

#[test]
fn test_valid_unordered_list() {
    let rule = MD006StartBullets;
    let content = "\
* Item 1
* Item 2
  * Nested item
  * Another nested item
* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_nested_list() {
    let rule = MD006StartBullets;
    let content = "\
* Item 1
  * Item 2
    * Deeply nested item
  * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid nested lists should not generate warnings, found: {:?}",
        result
    );
}

#[test]
fn test_invalid_indented_list() {
    let rule = MD006StartBullets;
    let content = "\
Some text here.

  * First item should not be indented
  * Second item should not be indented
  * Third item should not be indented";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "\
Some text here.

* First item should not be indented
* Second item should not be indented
* Third item should not be indented"
    );
}

#[test]
fn test_mixed_list_styles() {
    let rule = MD006StartBullets;
    let content = "\
* Item 1
  * Nested item
* Item 2

- Another item
  - Nested item
- Final item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_lists() {
    let rule = MD006StartBullets;
    let content = "\
* First list item
* Second list item

Some text here

  * Indented list 1
  * Indented list 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "\
* First list item
* Second list item

Some text here

* Indented list 1
* Indented list 2"
    );
}

#[test]
fn test_empty_lines() {
    let rule = MD006StartBullets;
    let content = "\
* Item 1

  * Nested item

* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_lists() {
    let rule = MD006StartBullets;
    let content = "\
Just some text
More text
Even more text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_code_blocks_ignored() {
    let rule = MD006StartBullets;
    let content = "\
```markdown
  * This indented item is inside a code block
  * These should be ignored
```

* Regular item outside code block";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
