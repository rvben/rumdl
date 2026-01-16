//! Comprehensive test suite for MDX flavor support.
//!
//! Tests JSX expression detection, MDX comments, and ESM blocks.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

// ====================================================================
// MDX Comment Detection Tests
// ====================================================================

#[test]
fn test_mdx_comment_single_line() {
    let content = r#"# Heading

{/* This is an MDX comment */}

Regular text.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Line 1: heading - not in MDX comment
    assert!(!ctx.lines[0].in_mdx_comment, "Heading should not be in MDX comment");
    // Line 2: blank - not in MDX comment
    assert!(!ctx.lines[1].in_mdx_comment, "Blank line should not be in MDX comment");
    // Line 3: single-line MDX comment - in MDX comment
    assert!(
        ctx.lines[2].in_mdx_comment,
        "Single-line MDX comment should be in MDX comment"
    );
    // Line 4: blank - not in MDX comment
    assert!(
        !ctx.lines[3].in_mdx_comment,
        "Blank line after comment should not be in MDX comment"
    );
}

#[test]
fn test_mdx_comment_multi_line() {
    let content = r#"{/*
  Multi-line
  MDX comment
*/}

Regular text.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // All lines of the comment should be in MDX comment
    assert!(
        ctx.lines[0].in_mdx_comment,
        "Opening multi-line MDX comment should be in MDX comment"
    );
    assert!(
        ctx.lines[1].in_mdx_comment,
        "Content in multi-line MDX comment should be in MDX comment"
    );
    assert!(
        ctx.lines[2].in_mdx_comment,
        "More content in MDX comment should be in MDX comment"
    );
    assert!(
        ctx.lines[3].in_mdx_comment,
        "Closing MDX comment should be in MDX comment"
    );
    // Line after - not in MDX comment
    assert!(
        !ctx.lines[4].in_mdx_comment,
        "Blank line after comment should not be in MDX comment"
    );
}

#[test]
fn test_mdx_comment_not_detected_in_standard_flavor() {
    let content = "{/* This is an MDX comment */}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // MDX comments should NOT be detected in Standard flavor
    assert!(
        !ctx.lines[0].in_mdx_comment,
        "MDX comments should not be detected in Standard flavor"
    );
}

// ====================================================================
// JSX Expression Detection Tests
// ====================================================================

#[test]
fn test_mdx_jsx_expression_multi_line() {
    let content = r#"# Heading

{
  multiLineExpression()
}

Regular text.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Line 1: heading - not in JSX expression
    assert!(
        !ctx.lines[0].in_jsx_expression,
        "Heading should not be in JSX expression"
    );
    // Lines 3-5: multi-line expression - should be detected
    assert!(
        ctx.lines[2].in_jsx_expression,
        "Opening brace of multi-line expression should be in JSX expression"
    );
    assert!(
        ctx.lines[3].in_jsx_expression,
        "Content in multi-line expression should be in JSX expression"
    );
    assert!(
        ctx.lines[4].in_jsx_expression,
        "Closing brace of multi-line expression should be in JSX expression"
    );
}

#[test]
fn test_mdx_jsx_expression_not_detected_in_standard_flavor() {
    let content = "The value is {computeValue()}.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // JSX expressions should NOT be detected in Standard flavor
    assert!(
        !ctx.lines[0].in_jsx_expression,
        "JSX expressions should not be detected in Standard flavor"
    );
}

// ====================================================================
// ESM Block Tests (Import/Export)
// ====================================================================

#[test]
fn test_mdx_esm_import_export() {
    let content = r#"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall

In {year}, the snowfall was above average.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Check that lines 1 and 2 are marked as ESM blocks
    assert!(ctx.lines[0].in_esm_block, "Line 1 (import) should be in_esm_block");
    assert!(ctx.lines[1].in_esm_block, "Line 2 (export) should be in_esm_block");
    assert!(!ctx.lines[2].in_esm_block, "Line 3 (blank) should NOT be in_esm_block");
    assert!(
        !ctx.lines[3].in_esm_block,
        "Line 4 (heading) should NOT be in_esm_block"
    );
}

#[test]
fn test_mdx_esm_not_detected_in_standard() {
    let content = r#"import {Chart} from './snowfall.js'
export const year = 2023
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        !ctx.lines[0].in_esm_block,
        "ESM should not be detected in Standard flavor"
    );
    assert!(
        !ctx.lines[1].in_esm_block,
        "ESM should not be detected in Standard flavor"
    );
}

// ====================================================================
// Flavor Compatibility Tests
// ====================================================================

#[test]
fn test_mdx_standard_flavor_does_not_support_mdx_features() {
    let content = "{/* comment */}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        !ctx.lines[0].in_mdx_comment,
        "Standard flavor should not detect MDX comments"
    );
    assert!(
        !ctx.lines[0].in_jsx_expression,
        "Standard flavor should not detect JSX expressions"
    );
}

#[test]
fn test_mdx_mkdocs_flavor_does_not_support_mdx_features() {
    let content = "{/* comment */}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    assert!(!ctx.lines[0].in_mdx_comment, "MkDocs should not detect MDX comments");
}

#[test]
fn test_mdx_quarto_flavor_does_not_support_mdx_features() {
    let content = "{/* comment */}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(!ctx.lines[0].in_mdx_comment, "Quarto should not detect MDX comments");
}

// ====================================================================
// Code Block Exclusion Tests
// ====================================================================

#[test]
fn test_mdx_features_not_detected_in_code_blocks() {
    let content = r#"```jsx
{/* This is not a real MDX comment */}
import Something from 'somewhere'
```
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Inside code block - should NOT be detected as MDX constructs
    assert!(ctx.lines[1].in_code_block, "Line inside code block");
    assert!(
        !ctx.lines[1].in_mdx_comment,
        "Code block content should not be MDX comment"
    );
    assert!(!ctx.lines[2].in_esm_block, "Code block content should not be ESM block");
}

// ====================================================================
// Mixed Content Tests
// ====================================================================

#[test]
fn test_mdx_mixed_content_basic() {
    let content = r#"# Heading

Regular **markdown** text with `code`.

{/* A comment */}

More text here.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Heading - not in any MDX construct
    assert!(!ctx.lines[0].in_mdx_comment);
    assert!(!ctx.lines[0].in_jsx_expression);

    // Regular markdown - not in any MDX construct
    assert!(!ctx.lines[2].in_mdx_comment);

    // Comment
    assert!(ctx.lines[4].in_mdx_comment, "MDX comment should be detected");

    // Text after
    assert!(
        !ctx.lines[6].in_mdx_comment,
        "Text after comment should not be in comment"
    );
}

// ====================================================================
// JSX Component Detection Tests
// Note: in_jsx_component is a placeholder field for future implementation.
// Currently MDX detection focuses on expressions, comments, and ESM blocks.
// ====================================================================

// ====================================================================
// JSX Fragment Tests
// Note: in_jsx_fragment is a placeholder field for future implementation.
// ====================================================================

// ====================================================================
// JSX Expression Edge Cases
// ====================================================================

#[test]
fn test_mdx_jsx_expression_with_object_literal() {
    let content = r#"The config is {{ key: "value", nested: { a: 1 } }}.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Double braces for object literal in expression
    assert!(ctx.lines[0].in_jsx_expression, "Object literal in expression");
}

#[test]
fn test_mdx_jsx_expression_with_arrow_function() {
    let content = r#"{items.map((item) => (
  <li key={item.id}>{item.name}</li>
))}
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Arrow function start");
    assert!(ctx.lines[1].in_jsx_expression, "Arrow function body");
    assert!(ctx.lines[2].in_jsx_expression, "Arrow function end");
}

#[test]
fn test_mdx_jsx_expression_with_ternary() {
    let content = "{isActive ? <Active /> : <Inactive />}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Ternary expression");
}

#[test]
fn test_mdx_jsx_expression_with_template_literal() {
    let content = "{`Hello, ${name}! You have ${count} messages.`}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Template literal");
}

#[test]
fn test_mdx_jsx_expression_nested_braces() {
    let content = r#"{(() => {
  const obj = { a: { b: { c: 1 } } };
  return obj.a.b.c;
})()}
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    for i in 0..4 {
        assert!(ctx.lines[i].in_jsx_expression, "Nested braces line {i}");
    }
}

#[test]
fn test_mdx_jsx_expression_spread_operator() {
    // Spread operator in JSX props contains an expression
    let content = "<Component {...props} />\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // The line contains a JSX expression {..props}
    // Component detection is not yet implemented, but expression detection works
    assert!(ctx.lines[0].in_jsx_expression, "Spread expression should be detected");
}

// ====================================================================
// ESM Edge Cases
// ====================================================================

#[test]
fn test_mdx_esm_multiline_import() {
    let content = r#"import {
  Component1,
  Component2,
  Component3
} from './components'

# Content
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    for i in 0..5 {
        assert!(ctx.lines[i].in_esm_block, "Multi-line import line {i}");
    }
    assert!(!ctx.lines[6].in_esm_block, "Heading not in ESM");
}

#[test]
fn test_mdx_esm_dynamic_import() {
    let content = r#"export const LazyComponent = dynamic(() => import('./Heavy'))
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_esm_block, "Dynamic import export");
}

#[test]
fn test_mdx_esm_reexport() {
    let content = r#"export { default as Button } from './Button'
export * from './utils'
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_esm_block, "Re-export default");
    assert!(ctx.lines[1].in_esm_block, "Re-export all");
}

#[test]
fn test_mdx_esm_import_with_alias() {
    let content = r#"import { useState as useStateHook } from 'react'
import * as ReactDOM from 'react-dom'
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_esm_block, "Import with alias");
    assert!(ctx.lines[1].in_esm_block, "Namespace import");
}

#[test]
fn test_mdx_esm_anywhere_in_document() {
    // MDX 2.0+ allows imports/exports anywhere
    let content = r#"# Introduction

import { Note } from './components'

Some content here.

export const metadata = { title: "Test" }

More content.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(!ctx.lines[0].in_esm_block, "Heading not ESM");
    assert!(ctx.lines[2].in_esm_block, "Mid-document import");
    assert!(!ctx.lines[4].in_esm_block, "Content not ESM");
    assert!(ctx.lines[6].in_esm_block, "Mid-document export");
    assert!(!ctx.lines[8].in_esm_block, "Final content not ESM");
}

// ====================================================================
// MDX Comment Edge Cases
// ====================================================================

#[test]
fn test_mdx_comment_with_jsx_inside() {
    let content = r#"{/*
  <DisabledComponent />
  Some text here
*/}
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // All lines should be in the comment context
    for i in 0..4 {
        assert!(ctx.lines[i].in_mdx_comment, "Comment with JSX line {i}");
    }
}

#[test]
fn test_mdx_comment_adjacent_to_expression() {
    let content = "{/* comment */}{expression}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // The line contains both comment and expression
    assert!(ctx.lines[0].in_mdx_comment || ctx.lines[0].in_jsx_expression);
}

#[test]
fn test_mdx_comment_with_urls() {
    let content = "{/* See https://example.com for details */}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_mdx_comment, "Comment with URL");
}

// ====================================================================
// Complex Integration Tests
// ====================================================================

#[test]
fn test_mdx_real_world_blog_post() {
    let content = r#"import { Author } from './components/Author'
import { CodeBlock } from './components/CodeBlock'

export const metadata = {
  title: 'Getting Started with MDX',
  author: 'Jane Doe',
  date: '2024-01-15'
}

# {metadata.title}

<Author name={metadata.author} />

This is a **blog post** written in MDX.

{/* TODO: Add more examples */}

<CodeBlock language="javascript">
{`const greeting = "Hello, World!";
console.log(greeting);`}
</CodeBlock>

## Conclusion

More content here.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // ESM at top
    assert!(ctx.lines[0].in_esm_block, "First import");
    assert!(ctx.lines[1].in_esm_block, "Second import");

    // Multi-line export
    assert!(ctx.lines[3].in_esm_block, "Export start");

    // Comment
    let comment_line = content.lines().position(|l| l.contains("TODO")).unwrap();
    assert!(ctx.lines[comment_line].in_mdx_comment, "TODO comment");

    // Expression in heading
    let heading_line = content.lines().position(|l| l.contains("{metadata.title}")).unwrap();
    assert!(ctx.lines[heading_line].in_jsx_expression, "Expression in heading");
}

#[test]
fn test_mdx_component_library_documentation() {
    let content = r#"import { Button, Card, Modal } from '@mylib/components'
import { Playground } from './Playground'

# Button Component

Some content with {expression} here.

## Props

| Prop | Type | Default |
|------|------|---------|
| variant | string | "primary" |
| disabled | boolean | false |

{/* API reference below */}

More documentation content.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Verify imports
    assert!(ctx.lines[0].in_esm_block, "Component import");
    assert!(ctx.lines[1].in_esm_block, "Playground import");

    // Verify comment detection
    let comment_line = content.lines().position(|l| l.contains("API reference")).unwrap();
    assert!(ctx.lines[comment_line].in_mdx_comment, "Comment detected");

    // Verify expression detection
    let expr_line = content.lines().position(|l| l.contains("{expression}")).unwrap();
    assert!(ctx.lines[expr_line].in_jsx_expression, "Expression detected");
}

// ====================================================================
// Stress and Edge Cases
// ====================================================================

#[test]
fn test_mdx_many_expressions() {
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("Value {i}: {{value{i}}}\n"));
    }

    let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);

    for i in 0..50 {
        assert!(ctx.lines[i].in_jsx_expression, "Expression on line {i}");
    }
}

#[test]
fn test_mdx_many_comments() {
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("{{/* Comment {i} */}}\n"));
    }

    let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);

    for i in 0..50 {
        assert!(ctx.lines[i].in_mdx_comment, "Comment on line {i}");
    }
}

#[test]
fn test_mdx_empty_expression() {
    let content = "{}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Empty expression");
}

#[test]
fn test_mdx_expression_with_only_whitespace() {
    let content = "{   }\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Whitespace-only expression");
}

#[test]
fn test_mdx_unicode_in_expressions() {
    let content = r#"{/* 你好世界 */}
{`Привет мир`}
{`Emoji: 🎉 ${emoji}`}
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_mdx_comment, "Chinese comment");
    assert!(ctx.lines[1].in_jsx_expression, "Russian expression");
    assert!(ctx.lines[2].in_jsx_expression, "Emoji expression");
}

#[test]
fn test_mdx_html_entities_in_expressions() {
    let content = "{`Hello &amp; World`}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(ctx.lines[0].in_jsx_expression, "Expression with HTML entity");
}
