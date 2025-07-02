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
        "Valid nested lists should not generate warnings, found: {result:?}"
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

// REGRESSION TESTS: Prevent false positives that were previously fixed

#[test]
fn test_nested_list_with_multiline_content_not_flagged() {
    let rule = MD006StartBullets;
    // This is the exact pattern that was causing false positives before the fix
    let content = "\
- Introduces changes or additions to the [MLflow REST
  API](https://mlflow.org/docs/latest/rest-api.html)
  - The MLflow REST API is implemented by a variety of open source
    and proprietary platforms. Changes to the REST API impact all of
    these platforms. Accordingly, we encourage developers to
    thoroughly explore alternatives before attempting to introduce
    REST API changes.
- Introduces new user-facing MLflow APIs
  - MLflow's API surface is carefully designed to generalize across
    a variety of common ML operations. It is important to ensure
    that new APIs are broadly useful to ML developers, easy to work
    with, and simple yet powerful.";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not flag any list items since they are properly nested
    assert!(
        result.is_empty(),
        "Properly nested list items with multi-line content should not be flagged. Found {} warnings: {:#?}",
        result.len(),
        result
    );

    // Fix should not change anything since the list structure is correct
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Fix should not change content with properly nested list items"
    );
}

#[test]
fn test_nested_list_with_markdown_link_continuation() {
    let rule = MD006StartBullets;
    // Test that markdown link continuations don't break list nesting validation
    let content = "\
* First item with [a link that spans
  multiple lines](https://example.com/very/long/url)
  * This nested item should be valid
  * Another nested item
* Second top-level item";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not flag any list items since the nesting is correct
    assert!(
        result.is_empty(),
        "List items with markdown link continuations should not break nesting validation. Found {} warnings: {:#?}",
        result.len(),
        result
    );
}

#[test]
fn test_mixed_valid_and_invalid_nesting() {
    let rule = MD006StartBullets;
    // Test that we correctly identify invalid nesting while preserving valid nesting
    let content = "\
* Valid top-level item
  * Valid nested item
    * Valid deeply nested item

Some breaking content

  * This should be flagged as invalid (indented without parent)
  * This should also be flagged

* Valid top-level item after break
  * Valid nested item after break";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the 2 items that are improperly indented after the break
    assert_eq!(
        result.len(),
        2,
        "Should only flag improperly indented items, not valid nested items. Found {} warnings: {:#?}",
        result.len(),
        result
    );

    // Verify the correct lines are flagged
    let flagged_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(
        flagged_lines.contains(&7) && flagged_lines.contains(&8),
        "Should flag lines 7 and 8 (the improperly indented items), but flagged: {flagged_lines:?}"
    );
}
