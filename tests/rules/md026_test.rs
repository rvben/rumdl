use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD026NoTrailingPunctuation;

#[test]
fn test_md026_valid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md026_invalid() {
    let rule = MD026NoTrailingPunctuation::default();
    // With new lenient rules: ! and ? are generally allowed, . still flagged
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the period should be flagged with the new lenient behavior
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md026_mixed() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, exclamation marks are generally allowed
    let content = "# Heading 1\n## Heading 2!\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // No issues expected with the new lenient behavior
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md026_fix() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, only period should be fixed
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    // Only the period should be removed with the new lenient behavior
    assert_eq!(result, "# Heading 1!\n## Heading 2?\n### Heading 3\n");
}

#[test]
fn test_md026_custom_punctuation() {
    // When using custom punctuation, the lenient rules don't apply
    let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Only ! and ? should be detected, not .
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md026_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are generally allowed
    let content = "Heading 1!\n=======\nHeading 2?\n-------\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // No issues expected with the new lenient behavior
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md026_closed_atx() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are generally allowed
    let content = "# Heading 1! #\n## Heading 2? ##\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // No issues expected with the new lenient behavior
    assert_eq!(result.len(), 0);
    let fixed = rule.fix(&ctx).unwrap();
    // Content should remain unchanged
    assert_eq!(fixed, "# Heading 1! #\n## Heading 2? ##\n");
}

#[test]
fn test_md026_empty_document() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Empty documents should not produce warnings"
    );
}

#[test]
fn test_md026_with_code_blocks() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Valid heading\n\n```\n# This is a code block with heading syntax!\n```\n\n```rust\n# This is another code block with a punctuation mark.\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Content in code blocks should be ignored"
    );
}

#[test]
fn test_md026_with_front_matter() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, exclamation marks are generally allowed
    let content = "---\ntitle: This is a title with punctuation!\ndate: 2023-01-01\n---\n\n# Correct heading\n## Heading with punctuation!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // No issues expected with the new lenient behavior
    assert_eq!(
        result.len(),
        0,
        "No headings should be detected with lenient rules"
    );

    let fixed = rule.fix(&ctx).unwrap();
    // Content should remain unchanged
    assert_eq!(
        fixed, content,
        "Fix should not modify content when no issues are detected"
    );
}

#[test]
fn test_md026_multiple_trailing_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are allowed, but . is still flagged
    let content = "# Heading with multiple marks!!!???\n## Another heading.....";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the periods should be flagged
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Only the periods should be removed
    assert_eq!(fixed, "# Heading with multiple marks!!!???\n## Another heading");
}

#[test]
fn test_md026_indented_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are generally allowed
    let content = "  # Indented heading!\n    ## Deeply indented heading?";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // No issues expected with the new lenient behavior
    assert_eq!(
        result.len(),
        0,
        "No headings should be detected with lenient rules"
    );

    let fixed = rule.fix(&ctx).unwrap();
    // Content should remain unchanged
    assert_eq!(fixed, content);
}

#[test]
fn test_md026_fix_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are generally allowed
    let content = "Heading 1!\n=======\nHeading 2?\n-------";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Content should remain unchanged with lenient behavior
    assert_eq!(
        fixed, content,
        "Content should not be modified with lenient rules"
    );
}

#[test]
fn test_md026_performance() {
    let rule = MD026NoTrailingPunctuation::default();

    // Create a large document with many headings
    // With lenient rules, use periods (which are still flagged) for testing
    let mut content = String::new();
    for i in 1..=100 {
        content.push_str(&format!(
            "# Heading {}{}\n\nSome content paragraph.\n\n",
            i,
            if i % 3 == 0 { "." } else { "" }  // Use periods instead of ! for testing
        ));
    }

    // Measure performance
    use std::time::Instant;
    let start = Instant::now();
    let ctx = LintContext::new(&content);
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Verify correctness - only periods are flagged now
    assert_eq!(
        result.len(),
        33,
        "Should detect exactly 33 headings with periods"
    );

    // Verify performance
    println!("MD026 performance test completed in {:?}", duration);
    assert!(
        duration.as_millis() < 1000,
        "Performance check should complete in under 1000ms"
    );
}

#[test]
fn test_md026_non_standard_punctuation() {
    let rule = MD026NoTrailingPunctuation::new(Some("@$%".to_string()));
    let content =
        "# Heading 1@\n## Heading 2$\n### Heading 3%\n#### Heading 4#\n##### Heading 5!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4#\n##### Heading 5!\n"
    );
}

#[test]
fn test_md026_legitimate_punctuation_patterns() {
    let rule = MD026NoTrailingPunctuation::default();
    
    // Test legitimate colon usage
    let colon_content = r#"# FAQ: Frequently Asked Questions
## API: Methods
### Step 1: Setup
#### Version 2.0: New Features
##### Chapter 1: Introduction
###### Error: File Not Found
####### Note: Implementation Details"#;
    
    let ctx = LintContext::new(colon_content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Legitimate colon patterns should not be flagged");
    
    // Test legitimate question marks
    let question_content = r#"# What is Markdown?
## How does it work?
### Why use this tool?
#### When should I run it?
##### Where can I find help?
###### Which option is best?
####### Can this be automated?
######## Should we continue?
######### Would this work?
########## Could this help?
########### Is this correct?
############ Are we done?
############# Do we proceed?
############## Does this work?
############### Did it succeed?"#;
    
    let ctx = LintContext::new(question_content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Legitimate question patterns should not be flagged");
    
    // Test legitimate exclamation marks
    let exclamation_content = r#"# Important!
## New!
### Warning!
#### Alert!
##### Notice!
###### Attention!"#;
    
    let ctx = LintContext::new(exclamation_content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Legitimate exclamation patterns should not be flagged");
    
    // Test that inappropriate punctuation is still flagged
    let bad_content = r#"# This is a regular sentence.
## Random heading;
### This seems wrong,"#;
    
    let ctx = LintContext::new(bad_content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Inappropriate punctuation should still be flagged");
}
