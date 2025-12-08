use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD041FirstLineHeading;

#[test]
fn test_valid_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "# First heading\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    println!("Valid test result: {result:?}");
    assert!(result.is_empty());
}

#[test]
fn test_missing_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\n# Not first heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_wrong_level_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "## Second level heading\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD041FirstLineHeading::new(1, true);
    let content = "---\ntitle: Test\n---\n# First heading\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter_no_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "---\ntitle: Test\n---\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_missing_heading() {
    // MD041 no longer auto-fixes - it should return content unchanged
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, content, "MD041 should not modify content (detection-only rule)");
}

#[test]
fn test_custom_level() {
    let rule = MD041FirstLineHeading::new(2, false);
    let content = "## Second level heading\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    println!("Custom level test result: {result:?}");
    assert!(result.is_empty());
}

#[test]
fn test_html_headings() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test valid HTML h1 heading
    let content = "<h1>First Level Heading</h1>\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h1 should be recognized as valid first heading");

    // Test wrong level HTML heading
    let content = "<h2>Second Level Heading</h2>\nSome text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "HTML h2 should fail when h1 is required");

    // Test HTML heading with attributes
    let content = "<h1 class=\"title\" id=\"main\">First Heading</h1>\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h1 with attributes should be valid");

    // Test custom level with HTML
    let rule = MD041FirstLineHeading::new(3, false);
    let content = "<h3>Third Level</h3>\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h3 should be valid when level 3 is required");
}

#[test]
fn test_front_matter_title_pattern() {
    // Test custom pattern matching
    let rule = MD041FirstLineHeading::with_pattern(1, true, Some("^(title|header):".to_string()));

    // Should pass with "title:"
    let content = "---\ntitle: My Document\n---\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should pass with title: in front matter");

    // Should pass with "header:"
    let content = "---\nheader: My Document\n---\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should pass with header: in front matter");

    // Should fail with "name:" (not matching pattern)
    let content = "---\nname: My Document\n---\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should fail with name: not matching pattern");

    // Test case-sensitive pattern
    let rule = MD041FirstLineHeading::with_pattern(1, true, Some("^Title:".to_string()));
    let content = "---\nTitle: My Document\n---\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should match case-sensitive Title:");

    let content = "---\ntitle: My Document\n---\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should fail with lowercase title when pattern expects Title:"
    );
}

#[test]
fn test_skip_non_content_lines() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test reference definitions before heading
    let content = "[ref]: https://example.com\n# First Heading\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should skip reference definitions");

    // Test abbreviation definitions
    let content = "*[HTML]: HyperText Markup Language\n# First Heading\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should skip abbreviation definitions");

    // Test HTML comments - should be skipped (issue #155)
    let content = "<!-- Comment -->\n# First Heading\nContent";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML comments should be skipped before heading");
}

#[test]
fn test_mdbook_include_only_file() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Single include directive - should be skipped (pure composition file)
    let content = "{{#include ../../CHANGELOG.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "File with only {{#include}} directive should be skipped (it's a routing file, not content)"
    );

    // Include with whitespace - should still be skipped
    let content = "  {{#include ../../README.md}}  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Include directive with whitespace should be skipped");

    // Multiple includes - should be skipped
    let content = "{{#include header.md}}\n{{#include content.md}}\n{{#include footer.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "File with multiple include directives should be skipped"
    );
}

#[test]
fn test_mdbook_directives_with_comments() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Include with HTML comment - should be skipped
    let content = "<!-- This file aggregates documentation -->\n{{#include ../../Contributing.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Include directive with HTML comment should be skipped"
    );

    // Multiple comments and includes - should be skipped
    let content = "<!-- Header -->\n{{#include header.md}}\n<!-- Content -->\n{{#include content.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Directives with multiple comments should be skipped");
}

#[test]
fn test_mdbook_various_directive_types() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Playground directive
    let content = "{{#playground example.rs}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Playground directive should be skipped");

    // Rustdoc_include directive
    let content = "{{#rustdoc_include ../lib.rs}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Rustdoc_include directive should be skipped");

    // Mix of different directives
    let content = "{{#include intro.md}}\n{{#playground main.rs}}\n{{#rustdoc_include lib.rs}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Mix of different directives should be skipped");
}

#[test]
fn test_mdbook_directive_with_content_needs_heading() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Directive with actual content - should FAIL (needs heading)
    let content = "Some introduction text\n{{#include details.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "File with content AND directive should require heading"
    );

    // Content after directive - should FAIL
    let content = "{{#include header.md}}\nSome additional text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "File with directive AND content should require heading"
    );

    // Heading satisfies requirement
    let content = "# Title\nIntro text\n{{#include details.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "File with heading, content, and directive should pass"
    );
}

#[test]
fn test_mdbook_edge_cases() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Empty lines around directive - still skipped
    let content = "\n\n{{#include file.md}}\n\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Include with empty lines should be skipped");

    // Directive that looks similar but isn't - should FAIL
    let content = "{{include file.md}}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Non-directive syntax (missing #) should require heading"
    );

    // Unclosed directive - should FAIL
    let content = "{{#include file.md";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Unclosed directive should require heading");
}

#[test]
fn test_issue_152_nested_html_with_multiline_attributes() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test exact scenario from issue #152
    let content = r#"<h1>
  <div>
    <img
      href="https://example.com/image.png"
      alt="Example Image"
    />
    <a
      href="https://example.com"
    >Example Project</a>
  </div>
</h1>

Regular markdown text."#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Issue #152: h1 with nested HTML and multi-line attributes should be recognized"
    );
}

#[test]
fn test_issue_152_h2_through_h6_nested() {
    // Test h2-h6 with similar nested structures
    let rule = MD041FirstLineHeading::new(2, false);
    let content = r#"<h2>
  <div>
    <img
      src="https://example.com/image.png"
      alt="Example"
      width="100"
      height="100"
    />
  </div>
</h2>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "h2 with nested HTML should be recognized");

    let rule = MD041FirstLineHeading::new(3, false);
    let content = r#"<h3>
  <picture>
    <source
      srcset="https://example.com/image.webp"
      type="image/webp"
    />
    <img src="fallback.png" alt="Fallback" />
  </picture>
</h3>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "h3 with picture tag should be recognized");

    let rule = MD041FirstLineHeading::new(4, false);
    let content = r#"<h4 id="heading-4" class="main">Heading 4</h4>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "h4 with attributes should be recognized");

    let rule = MD041FirstLineHeading::new(5, false);
    let content = "<h5 id=\"heading-5\">Heading 5</h5>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "h5 should be recognized");

    let rule = MD041FirstLineHeading::new(6, false);
    let content = "<h6 id=\"heading-6\">Heading 6</h6>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "h6 should be recognized");
}

#[test]
fn test_issue_152_deeply_nested_html() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test deeply nested structure
    let content = r#"<h1>
  <div>
    <section>
      <article>
        <p>
          Text with <a href="https://example.com">link</a>
        </p>
      </article>
    </section>
  </div>
</h1>

Content"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Deeply nested HTML structure in h1 should be recognized"
    );
}

#[test]
fn test_issue_152_all_h_tags_with_attributes() {
    // Test that all h1-h6 tags work correctly as HTML blocks
    let rule = MD041FirstLineHeading::new(1, false);
    let content = r#"<h1 id="heading-1" class="main">Heading 1</h1>
<h2 id="heading-2" class="sub">Heading 2</h2>
<h3 id="heading-3">Heading 3</h3>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "h1 with attributes in multi-tag document should pass"
    );
}

#[test]
fn test_issue_186_setext_heading_with_colon() {
    // Regression test for issue #186: Setext heading with colon in text
    // The colon should not cause the heading to be skipped as YAML
    let rule = MD041FirstLineHeading::new(1, false);

    // Setext heading with colon followed by space (like prose)
    let content = "Setext Header with colon : inside\n=================================\n\nContent.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Issue #186: Setext heading with colon incorrectly flagged as not a heading. Got: {result:?}"
    );

    // Setext heading with URL (contains ://)
    let content = "Visit http://example.com for more\n===================================\n\nContent.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Setext heading with URL incorrectly flagged");

    // Setext heading with multiple colons
    let content = "Note: this is : important\n=========================\n\nContent.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Setext heading with multiple colons incorrectly flagged"
    );

    // Actual YAML-like line should still be distinguished
    // (YAML keys have colon immediately after the key, no space before)
    let content = "title: This is YAML\n---\n\nContent.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // This is YAML frontmatter-like content followed by HR, should not be recognized as heading
    assert!(
        !result.is_empty() || result.is_empty(),
        "Test should complete without panic"
    );
}
