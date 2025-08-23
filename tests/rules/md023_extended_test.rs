use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD023HeadingStartLeft;

#[test]
fn test_complex_mixed_headings() {
    let rule = MD023HeadingStartLeft;

    // Test case with a mix of different heading styles and indentation
    let content = r#"# Valid heading

  ## Indented ATX heading

### Valid heading

   #### Another indented heading

Setext heading
-------------

  Another setext heading
  ---------------------

   # Indented closed ATX heading #
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings for all indented headings
    assert_eq!(warnings.len(), 5);

    // Verify the correct lines are flagged
    assert_eq!(warnings[0].line, 3); // "  ## Indented ATX heading"
    assert_eq!(warnings[1].line, 7); // "   #### Another indented heading"
    assert_eq!(warnings[2].line, 12); // "  Another setext heading"
    assert_eq!(warnings[3].line, 13); // "  ---------------------"
    assert_eq!(warnings[4].line, 15); // "   # Indented closed ATX heading #"

    // Verify the fix
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(!fixed.contains("  ## Indented"));
    assert!(!fixed.contains("   ####"));
    assert!(!fixed.contains("  Another setext"));
    assert!(!fixed.contains("   # Indented closed"));

    // Verify that properly aligned headings are preserved
    assert!(fixed.contains("# Valid heading"));
    assert!(fixed.contains("### Valid heading"));
}

#[test]
fn test_front_matter_with_headings() {
    let rule = MD023HeadingStartLeft;

    // Test case with front matter and various headings
    let content = r#"---
title: Test Document
author: Test Author
---

# Valid heading

  ## Indented heading in content

Content after front matter
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have one warning for the indented heading (line 8)
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 8);

    // Verify the fix preserves front matter
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("---\ntitle:"));
    assert!(fixed.contains("---\n\n# Valid"));
    assert!(fixed.contains("## Indented heading"));
    assert!(!fixed.contains("  ## Indented"));
}

#[test]
fn test_code_blocks_with_headings() {
    let rule = MD023HeadingStartLeft;

    // Test case with code blocks and headings
    let content = r#"# Valid heading

```markdown
# This is a heading in a code block
  ## This is an indented heading in a code block
```

  ## This is an indented heading outside code block

```
  # Another code block heading
```

   ### Another indented heading
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings only for headings outside code blocks
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 8); // "  ## This is an indented heading outside code block"
    assert_eq!(warnings[1].line, 14); // "   ### Another indented heading"

    // Verify the fix preserves code blocks
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("```markdown\n# This is a heading"));
    assert!(fixed.contains("  ## This is an indented heading in a code block"));
    assert!(fixed.contains("```\n\n## This is an indented")); // Fixed with no indentation
    assert!(fixed.contains("```\n  # Another code block heading\n```"));
    assert!(fixed.contains("### Another indented heading")); // Fixed with no indentation
}

#[test]
fn test_nested_headings_with_mixed_styles() {
    let rule = MD023HeadingStartLeft;

    // Test case with nested headings of mixed styles
    let content = r#"# Main heading

## Subheading

  ### Indented ATX Subheading

  Indented Setext SubSubheading
  ----------------------------

#### Regular SubSubSubheading
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings for indented headings
    assert_eq!(warnings.len(), 3);
    assert_eq!(warnings[0].line, 5); // "  ### Indented ATX Subheading"
    assert_eq!(warnings[1].line, 7); // "  Indented Setext SubSubheading"

    // Also check fix
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("### Indented ATX Subheading")); // Fixed with no indentation
    assert!(fixed.contains("Indented Setext SubSubheading")); // Fixed with no indentation
    assert!(fixed.contains("----------------------------")); // Fixed underline with no indentation
    assert!(!fixed.contains("  ### Indented"));
    assert!(!fixed.contains("  Indented Setext"));
}

#[test]
fn test_heading_with_special_characters() {
    let rule = MD023HeadingStartLeft;

    // Test case with special characters in headings
    let content = r#"# Heading with *emphasis*

  ## Indented heading with **bold** and `code`

   ### Indented heading with [link](https://example.com)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings for indented headings
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // "  ## Indented heading with **bold** and `code`"
    assert_eq!(warnings[1].line, 5); // "   ### Indented heading with [link](https://example.com)"

    // Verify the fix preserves special characters
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("## Indented heading with **bold** and `code`"));
    assert!(fixed.contains("### Indented heading with [link](https://example.com)"));
    assert!(!fixed.contains("  ## Indented"));
    assert!(!fixed.contains("   ### Indented"));
}

#[test]
fn test_empty_indented_headings() {
    let rule = MD023HeadingStartLeft;

    // Test case with empty indented headings
    let content = r#"# Valid heading

  ##

   ###
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings for indented headings
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // "  ## "
    assert_eq!(warnings[1].line, 5); // "   ### "

    // Verify the fix works for empty headings
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("##"));
    assert!(fixed.contains("###"));
    assert!(!fixed.contains("  ##"));
    assert!(!fixed.contains("   ###"));
}

#[test]
fn test_multiple_indentation_levels() {
    let rule = MD023HeadingStartLeft;

    // Test case with multiple indentation levels
    let content = r#"# Valid heading

 ## Heading with 1 space

  ## Heading with 2 spaces

   ## Heading with 3 spaces

    ## Heading with 4 spaces
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should have warnings for all indented headings except the one with 4+ spaces (code block)
    assert_eq!(warnings.len(), 3);
    assert_eq!(warnings[0].line, 3); // " ## Heading with 1 space"
    assert_eq!(warnings[1].line, 5); // "  ## Heading with 2 spaces"
    assert_eq!(warnings[2].line, 7); // "   ## Heading with 3 spaces"
    // Line 9 with 4 spaces is a code block, not a heading

    // Verify the fix works for different indentation levels
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    // The fix should have 3 unindented headings + 1 unchanged line that looks like a heading
    assert_eq!(fixed.matches("## Heading with").count(), 4);
    // The 4-space indented line should remain unchanged (it's not detected as a heading)
    assert!(fixed.contains("    ## Heading with 4 spaces"));
    assert!(!fixed.contains(" ## Heading with 1"));
    assert!(!fixed.contains("  ## Heading with 2"));
    assert!(!fixed.contains("   ## Heading with 3"));
}

#[test]
fn test_md023_heading_after_list() {
    // Test content with heading after list
    let content = r#"# Configuration Precedence

Settings are applied in the following order (later sources override earlier ones):

1. **Built-in defaults**
2. **Configuration file** (`.rumdl.toml` or `pyproject.toml`)
3. **Command-line arguments**

   ### Example: Precedence in Action

Given this configuration file:"#;

    let rule = MD023HeadingStartLeft;
    let ctx = LintContext::new(content);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "Should detect one indented heading");
    assert_eq!(warnings[0].line, 9);
    assert!(warnings[0].message.contains("should not be indented"));

    // Verify the fix works
    let fixed = rule.fix(&ctx).unwrap();
    assert_ne!(fixed, content, "MD023 should fix indented heading after list");

    // Verify the indented heading was fixed
    assert!(fixed.contains("### Example: Precedence in Action"));
    assert!(!fixed.contains("   ### Example: Precedence in Action"));
}
