use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD044ProperNames, MD045NoAltText, MD052ReferenceLinkImages};

/// Comprehensive edge case tests for inline content rules (MD044, MD045, MD052)
///
/// These tests ensure inline content rules handle Unicode, special cases, and edge conditions correctly.

#[test]
fn test_md044_unicode_proper_names() {
    let rule = MD044ProperNames::new(
        vec![
            "JavaScript".to_string(),
            "中文名称".to_string(),
            "العربية".to_string(),
            "Café".to_string(),
            "naïve".to_string(),
            "Zürich".to_string(),
            "Москва".to_string(),
            "🚀Rocket".to_string(),
        ],
        true,
    );

    // Test 1: Unicode proper names with various scripts
    let content = "\
I love javascript and javascript is great.

The 中文名称 and 中文名稱 should be detected.

Visit москва for the conference.

Try the cafe in zurich.

The implementation is naive.

🚀rocket is launching soon.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: MD044 MUST detect ALL improper capitalizations including accented characters
    // KNOWN ISSUE: Currently only detects 4/7 due to Unicode word boundary limitations (see docs/KNOWN_PRODUCTION_ISSUES.md)
    assert_eq!(
        result.len(),
        7,
        "Should detect ALL improper capitalizations: javascript(x2), 中文名称, москва, cafe, zurich, naive, 🚀rocket"
    );

    // Verify fixes handle Unicode correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("JavaScript"));
    assert!(fixed.contains("Москва"));
    assert!(fixed.contains("Café"), "Must fix cafe -> Café");
    assert!(fixed.contains("Zürich"), "Must fix zurich -> Zürich");
    assert!(fixed.contains("naïve"), "Must fix naive -> naïve");
    assert!(fixed.contains("🚀Rocket"));
}

#[test]
fn test_md044_special_characters_names() {
    let rule = MD044ProperNames::new(
        vec![
            "Node.js".to_string(),
            "ASP.NET".to_string(),
            "C++".to_string(),
            "C#".to_string(),
            "F#".to_string(),
            ".NET".to_string(),
            "@angular/core".to_string(),
            "package.json".to_string(),
            "Wi-Fi".to_string(),
            "e-mail".to_string(),
        ],
        true,
    );

    // Test 2: Names with special characters
    let content = "\
I use node.js and nodejs for development.

Working with asp.net and ASP.net frameworks.

Programming in c++ and c# languages.

The .net framework and f# language.

Import from @angular/Core module.

Edit the Package.json file.

Connect to wifi or wi-fi network.

Send an Email or e-Mail message.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.len() >= 8, "Should detect special character names");

    // Verify fixes preserve special characters
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("Node.js"));
    assert!(fixed.contains("ASP.NET"));
    assert!(fixed.contains("C++"));
    assert!(fixed.contains("C#"));
    assert!(fixed.contains("F#"));
    assert!(fixed.contains("@angular/core"));
    assert!(fixed.contains("package.json"));
}

#[test]
fn test_md044_word_boundaries() {
    let rule = MD044ProperNames::new(
        vec!["Go".to_string(), "IT".to_string(), "I".to_string(), "A".to_string()],
        true,
    );

    // Test 3: Short names and word boundary edge cases
    let content = "\
Let's go with Go programming.

The word 'going' should not match go.

it department handles IT issues.

i think I should use a framework.

This is a test of A versus a.

Don't match 'ago' or 'bit' or 'ai'.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should only match whole words
    assert!(
        result
            .iter()
            .any(|r| r.message.contains("go") && !r.message.contains("going"))
    );
    assert!(
        result
            .iter()
            .any(|r| r.message.contains("it") && !r.message.contains("bit"))
    );
}

#[test]
fn test_md044_code_exclusion() {
    let rule = MD044ProperNames::new(
        vec!["JavaScript".to_string(), "Python".to_string()],
        false, // false = exclude code blocks from checking
    );

    // Test 4: Code block and inline code exclusion
    let content = "\
Use javascript in production.

```javascript
// This javascript and python should be ignored
const javascript = 'python';
```

The `javascript` and `python` in backticks should be ignored.

```
plain javascript and python in code block
```

More javascript and python outside code.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: When code_blocks=true, MD044 MUST exclude ALL code (blocks AND inline)
    // Should detect:
    // - "javascript" on line 1
    // - "javascript" and "python" on last line (line 10)
    // Total: 3 warnings
    assert_eq!(
        result.len(),
        3,
        "Should detect only javascript and python outside ALL code contexts"
    );
}

#[test]
fn test_md044_html_comment_handling() {
    // Note: Can't control html_comments parameter with public API
    // Default is to check HTML comments, so this test is adjusted
    let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

    // Test 5: HTML comment handling
    let content = "\
Use javascript here.

<!-- This javascript should be ignored -->

<!--
Multi-line comment with javascript
should also be ignored
-->

More javascript usage.

<!-- javascript --> between <!-- javascript --> comments";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: Default MUST check HTML comments
    assert_eq!(
        result.len(),
        6,
        "Should detect all 6 javascript occurrences including in HTML comments"
    );
}

#[test]
fn test_md044_complex_patterns() {
    let rule = MD044ProperNames::new(
        vec![
            "GitHub".to_string(),
            "GitLab".to_string(),
            "LaTeX".to_string(),
            "macOS".to_string(),
            "iOS".to_string(),
            "iPadOS".to_string(),
            "TypeScript".to_string(),
            "JavaScript".to_string(),
        ],
        true,
    );

    // Test 6: Complex capitalization patterns
    let content = "\
Upload to github, GITHUB, or Github.

Compare gitlab with GITLAB and GitLAB.

Write in latex or LATEX format.

Develop for macos, MacOS, and ios.

Use typescript with javascript.

Support for ipados and IpadOS.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let _result = rule.check(&ctx).unwrap();

    // Verify all variations are caught
    let fixed = rule.fix(&ctx).unwrap();
    assert!(!fixed.contains("github"));
    assert!(!fixed.contains("GITHUB"));
    assert!(!fixed.contains("Github"));
    assert!(fixed.contains("GitHub"));
    assert!(fixed.contains("macOS"));
    assert!(fixed.contains("iOS"));
}

#[test]
fn test_md045_unicode_alt_text() {
    let rule = MD045NoAltText::new();

    // Test 1: Images with Unicode in paths and missing alt text
    let content = "\
![](image.png)

![](图片/photo.jpg)

![](الصور/image.png)

![](фото/картинка.jpg)

![](path/to/🎨.png)

![ ](spaces-only.jpg)

![\t](tab-only.jpg)

![　](full-width-space.jpg)";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 8, "Should detect all images with missing/empty alt text");

    // Verify fixes add placeholder
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("![Image image](image.png)"));
    assert!(fixed.contains("![Photo image](图片/photo.jpg)"));
}

#[test]
fn test_md045_reference_style_images() {
    let rule = MD045NoAltText::new();

    // Test 2: Reference-style images
    let content = "\
![][ref1]

![ ][ref2]

![Valid alt text][ref3]

![][ref-with-unicode-图片]

[ref1]: image1.png
[ref2]: image2.png
[ref3]: image3.png
[ref-with-unicode-图片]: unicode.png

Shortcut reference: ![shortcut]

[shortcut]: shortcut.png";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Should detect reference images without alt text");
}

#[test]
fn test_md045_nested_constructs() {
    let rule = MD045NoAltText::new();

    // Test 3: Images in various contexts
    let content = "\
- List item with ![](image1.png)
  - Nested ![](image2.png)

> Blockquote with ![](image3.png)
> > Nested quote ![](image4.png)

| Table | Header |
|-------|--------|
| Cell  | ![](image5.png) |

[Link with ![](image6.png) inside](url)

*Emphasis with ![](image7.png) inside*

**Strong with ![](image8.png) inside**";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 8, "Should detect images in all contexts");
}

#[test]
fn test_md045_code_exclusion() {
    let rule = MD045NoAltText::new();

    // Test 4: Images in code should be excluded
    let content = "\
Regular image: ![](regular.png)

`Inline code with ![](ignored.png) image`

```
Code block with ![](also-ignored.png)
```

```markdown
Even in markdown code blocks ![](still-ignored.png)
```

    Four spaces code ![](indented-ignored.png)

More regular: ![](regular2.png)";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should only detect images outside code");
}

#[test]
fn test_md045_edge_patterns() {
    let rule = MD045NoAltText::new();

    // Test 5: Edge cases and malformed images
    let content = "\
![]()

![ ]( )

![](   )

![]( image.png )

![

](multiline.png)

![](image.png)(extra-parens)

\\![](escaped.png)

![Existing alt](image.png)";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should handle edge cases gracefully
    assert!(result.len() >= 3, "Should detect valid images without alt text");
}

#[test]
fn test_md045_html_images() {
    let rule = MD045NoAltText::new();

    // Test 6: Mixed Markdown and HTML images
    let content = "\
![](markdown.png)

<img src=\"html.png\">

<img src=\"html-with-alt.png\" alt=\"Has alt text\">

<img src=\"html-empty-alt.png\" alt=\"\">

Mixed: ![](md.png) and <img src=\"html2.png\">";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // MD045 only checks Markdown images, not HTML
    assert_eq!(result.len(), 2, "Should only check Markdown images");
}

#[test]
fn test_md052_unicode_references() {
    let rule = MD052ReferenceLinkImages::new();

    // Test 1: Unicode in reference names and definitions
    let content = "\
Check [this link][中文引用]

See [another][עברית]

Image: ![alt][图片引用]

Unicode emoji ref: [click][🔗link]

Missing: [undefined][参照なし]

[中文引用]: https://example.com/chinese
[עברית]: https://example.com/hebrew
[图片引用]: image.png

Note: 🔗link is not defined";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect missing Unicode references");
    assert!(result.iter().any(|r| r.message.contains("参照なし")));
    assert!(result.iter().any(|r| r.message.contains("🔗link")));
}

#[test]
fn test_md052_case_sensitivity() {
    let rule = MD052ReferenceLinkImages::new();

    // Test 2: Case-insensitive reference matching
    let content = "\
Links: [text][REF], [text][ref], [text][Ref]

Images: ![alt][IMG], ![alt][img], ![alt][Img]

Missing: [text][MISSING], [text][missing]

[ref]: https://example.com
[IMG]: image.png";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: MD052 MUST be case-insensitive
    // [MISSING] and [missing] are the same undefined reference
    assert_eq!(
        result.len(),
        1,
        "Should detect exactly 1 unique missing reference (case-insensitive)"
    );
}

#[test]
fn test_md052_shortcut_references() {
    // By default, shortcut_syntax is false (matches markdownlint behavior)
    // so shortcut references like [text] are not checked
    let rule = MD052ReferenceLinkImages::new();

    // Test 3: Shortcut reference syntax
    let content = "\
Shortcut link: [shortcut]

Another: [defined]

Image shortcut: ![image-ref]

Undefined: [no-definition]

[defined]: https://example.com
[image-ref]: image.png

Mixed with [normal][ref] syntax

[ref]: https://ref.com";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Shortcut references are NOT checked by default (shortcut_syntax: false)
    // Only full reference syntax [text][ref] is checked
    assert_eq!(result.len(), 0, "Shortcut references are not checked by default");
}

#[test]
fn test_md052_code_exclusion() {
    let rule = MD052ReferenceLinkImages::new();

    // Test 4: References in code should be excluded
    let content = "\
Real reference: [link][ref1]

`Code with [link][ref2] inside`

```
Code block [link][ref3]
More [refs][ref4]
```

    Indented code [link][ref5]

List context might affect this:
- Item with [link][ref6]
  - Nested [link][ref7]

[ref1]: url1
[ref6]: url6
[ref7]: url7";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should not check references in code blocks
    assert!(!result.iter().any(|r| r.message.contains("ref2")));
    assert!(!result.iter().any(|r| r.message.contains("ref3")));
    assert!(!result.iter().any(|r| r.message.contains("ref4")));
}

#[test]
fn test_md052_complex_references() {
    let rule = MD052ReferenceLinkImages::new();

    // Test 5: Complex reference patterns
    let content = "\
Multiple on line: [a][ref1] and [b][ref2] and [c][ref3]

Nested: [outer [inner][ref4] text][ref5]

Adjacent: [first][ref6][second][ref7]

Empty ref: [text][]

Space in ref: [text][ref with spaces]

Special chars: [text][ref-with-dash_and_underscore]

[ref1]: url1
[ref3]: url3
[ref5]: url5
[ref6]: url6
[ref-with-dash_and_underscore]: special-url

Missing: ref2, ref4, ref7, ref with spaces";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.len() >= 4, "Should detect various missing references");
}

#[test]
fn test_md052_reference_definitions() {
    let rule = MD052ReferenceLinkImages::new();

    // Test 6: Various reference definition formats
    let content = "\
Use [link1][ref1] and [link2][ref2]

Also ![image1][img1] and ![image2][img2]

[ref1]: https://example.com \"Title\"
[ref2]: <https://example.com> 'Title'
[img1]: path/to/image.png (Title)
[img2]: ../relative/path.jpg
  \"Multi-line title\"

Undefined: [missing][undefined]

Empty definition should work: [empty][empty-ref]
[empty-ref]:

Duplicate definitions:
[dup]: first.com
[dup]: second.com
[dup]: third.com

Using [dup][dup] should work";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: MD052 MUST detect ONLY truly undefined references
    // Empty definitions are valid, duplicate definitions use the first one
    assert_eq!(result.len(), 1, "Should detect exactly 1 undefined reference");
    assert!(result[0].message.contains("undefined"));
}

#[test]
fn test_inline_rules_interaction() {
    // Test all inline rules together
    let md044 = MD044ProperNames::new(vec!["JavaScript".to_string(), "GitHub".to_string()], true);
    let md045 = MD045NoAltText::new();
    let md052 = MD052ReferenceLinkImages::new();

    let content = "\
Use javascript to upload images to github.

Here's an image without alt text: ![](logo.png)

Check the [javascript guide][js-guide] on [github][gh].

Another image reference: ![github logo][gh-logo]

[gh]: https://github.com
[gh-logo]: github-logo.png

Note: js-guide is not defined";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Each rule should detect its issues independently and correctly
    let result044 = md044.check(&ctx).unwrap();
    let result045 = md045.check(&ctx).unwrap();
    let result052 = md052.check(&ctx).unwrap();

    // PRODUCTION REQUIREMENTS:
    // With link filtering, only standalone proper names should be flagged
    // Only "javascript" and "github" on line 1 should be flagged (not in links/URLs/filenames)
    assert_eq!(
        result044.len(),
        2,
        "MD044: Must detect 2 standalone improper names (not in links)"
    );
    assert_eq!(result045.len(), 1, "MD045: Must detect 1 image without alt text");
    assert_eq!(
        result052.len(),
        1,
        "MD052: Must detect 1 undefined reference [js-guide]"
    );
}

#[test]
fn test_md044_performance_edge_cases() {
    let rule = MD044ProperNames::new(
        vec!["Test".to_string(); 100], // Many names
        true,
    );

    // Test with many occurrences
    let content = "test ".repeat(1000);

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1000, "Should handle many occurrences efficiently");
}

#[test]
fn test_md045_image_title_attribute() {
    let rule = MD045NoAltText::new();

    // Test images with title but no alt
    let content = "\
![](image.png \"Title\")

![ ](image.png \"Another title\")

![Good alt](image.png \"Title\")

![][ref]

[ref]: image.png \"Reference title\"";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Title attribute doesn't replace alt text requirement");
}

#[test]
fn test_md052_nested_brackets() {
    let rule = MD052ReferenceLinkImages::new();

    // Test nested brackets and edge cases
    let content = "\
Link with [brackets [inside]][ref1]

Image with ![brackets [in] alt][ref2]

Escaped \\[not a link\\][ref3]

Actually escaped: \\[link\\]\\[ref4\\]

But this is real: [link][ref5]

[ref1]: url1
[ref2]: url2
[ref5]: url5";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // PRODUCTION REQUIREMENT: MD052 MUST handle escaped brackets correctly
    assert_eq!(result.len(), 0, "Should NOT detect escaped references as undefined");
}

#[test]
fn test_inline_content_front_matter() {
    let md044 = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

    // Test with front matter
    let content = "\
---
title: Using javascript
tags: [javascript, programming]
---

# Learning javascript

The javascript ecosystem is vast.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = md044.check(&ctx).unwrap();
    // Should detect in front matter and content
    assert!(result.len() >= 2, "Should check front matter content");
}

#[test]
fn test_inline_content_html_mixed() {
    let md045 = MD045NoAltText::new();
    let md052 = MD052ReferenceLinkImages::new();

    // Test mixed HTML and Markdown
    let content = "\
<div>
  ![](markdown-in-html.png)
  <img src=\"html-image.png\">
</div>

Regular ![](outside.png) image.

<p>Link to [reference][ref] in HTML</p>

<!-- Comment with ![](in-comment.png) -->

[ref]: defined.com";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result045 = md045.check(&ctx).unwrap();
    let result052 = md052.check(&ctx).unwrap();

    // Per CommonMark: Markdown inside HTML blocks is NOT processed
    // Only the image outside HTML (`outside.png`) should be detected
    // Images inside `<div>`, `<p>`, and `<!-- -->` are all ignored
    assert_eq!(
        result045.len(),
        1,
        "Should only detect Markdown images outside HTML blocks (CommonMark compliance)"
    );
    assert_eq!(result052.len(), 0, "All references should be defined");
}

#[test]
fn test_md044_overlapping_names() {
    let rule = MD044ProperNames::new(
        vec![
            "JavaScript".to_string(),
            "Java".to_string(),
            "Script".to_string(),
            "TypeScript".to_string(),
        ],
        true,
    );

    // Test overlapping name patterns
    let content = "\
I love javascript and java programming.

The script uses typescript features.

Don't match 'manuscript' or 'subscription'.

But do match java and script separately.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let _result = rule.check(&ctx).unwrap();

    // Should handle overlapping patterns correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("JavaScript"));
    assert!(fixed.contains("TypeScript"));
    assert!(!fixed.contains("manuScript"));
}

#[test]
fn test_md045_multiline_images() {
    let rule = MD045NoAltText::new();

    // Test multiline image syntax
    let content = "\
![
](multiline1.png)

![

](multiline2.png)

![Good
alt
text](multiline3.png)

![    ](spaces.png)";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Per CommonMark: Blank lines break inline syntax
    // - ![<newline>](url) → valid (1 newline) → empty alt text → flagged
    // - ![<newline><newline>](url) → INVALID (blank line) → not detected
    // - ![Good<newline>alt<newline>text](url) → valid → has alt text → NOT flagged
    // - ![    ](url) → valid → whitespace only alt text → flagged
    // Result: 2 warnings (multiline1.png and spaces.png)
    assert_eq!(
        result.len(),
        2,
        "Should handle multiline image syntax per CommonMark spec"
    );
}

#[test]
fn test_md052_example_sections() {
    let rule = MD052ReferenceLinkImages::new();

    // Test example section exclusion
    let content = "\
Regular reference: [link][ref1]

Example:
```
[example][ref2]
```

Examples:
- [another][ref3]

[ref1]: defined.com

Note: ref2 and ref3 in example sections might be excluded";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Behavior depends on implementation details
    assert!(result.len() <= 2, "May exclude example sections");
}
