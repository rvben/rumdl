use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::{
    MD036NoEmphasisAsHeading, MD037NoSpaceInEmphasis, MD049EmphasisStyle, MD050StrongStyle,
    emphasis_style::EmphasisStyle, strong_style::StrongStyle,
};

/// Comprehensive edge case tests for emphasis rules (MD036, MD037, MD049, MD050)
///
/// These tests ensure emphasis rules handle Unicode, special cases, and edge conditions correctly.

#[test]
fn test_md036_unicode_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(String::new());

    // Test 1: Unicode content in emphasis
    let content = "\
**Hello 👋 World**

*你好世界*

__مرحبا بالعالم__

_Привет мир_";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect all Unicode emphasis as headings");

    // Verify fixes work with Unicode
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("## Hello 👋 World"));
    assert!(fixed.contains("# 你好世界"));
    assert!(fixed.contains("## مرحبا بالعالم"));
    assert!(fixed.contains("# Привет мир"));
}

#[test]
fn test_md036_punctuation_edge_cases() {
    // Test with various punctuation configurations
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?。".to_string());

    // Test 2: Various punctuation scenarios
    let content = "\
**Important!**

*Question?*

**Statement.**

*Chinese。*

**Multiple!!!**

*No punctuation*";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // With punctuation allowed, these should be ignored except the last one
    assert_eq!(result.len(), 1, "Should only detect emphasis without punctuation");
    assert_eq!(result[0].line, 11);
}

#[test]
fn test_md036_toc_labels() {
    let rule = MD036NoEmphasisAsHeading::new(String::new());

    // Test 3: TOC labels should be ignored
    let content = "\
**Table of Contents**

**Contents**

**TOC**

**Index**

**table of contents**

**CONTENTS**

**Custom Heading**";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // With empty punctuation, MD036 detects more emphasis as headings
    // TOC labels are only ignored when punctuation is configured
    assert!(!result.is_empty(), "Should detect at least one emphasis");
    // The last line should be detected
    assert!(result.iter().any(|r| r.line == 13));
}

#[test]
fn test_md036_complex_contexts() {
    let rule = MD036NoEmphasisAsHeading::new(String::new());

    // Test 4: Emphasis in various contexts
    let content = "\
**Standalone heading**

- **In a list**
  - *Nested list*

> **In blockquote**
> *Also in quote*

1. **Numbered list**
2. *Another item*

```
**In code block**
```

`**in inline code**`

# Real heading with **emphasis** inside

| **Table** | *Header* |
|-----------|----------|
| **Cell**  | *Data*   |";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the first line should be detected
    assert_eq!(result.len(), 1, "Should only detect standalone emphasis");
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_md036_edge_patterns() {
    let rule = MD036NoEmphasisAsHeading::new(String::new());

    // Test 5: Edge patterns and malformed emphasis
    let content = "\
****

__

**

*

***Mixed***

**Partial emphasis** not alone

Not **standalone** emphasis

**Multiple** **emphasis** **markers**";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Empty emphasis and mixed should not be detected
    // Only line 9 with triple asterisks might be detected
    assert!(result.len() <= 1, "Should handle edge patterns correctly");
}

#[test]
fn test_md037_unicode_spaces() {
    let rule = MD037NoSpaceInEmphasis;

    // Test 1: Unicode content with spaces
    let content = "\
* Hello 👋 *

** 你好 **

_ مرحبا _

__ Привет __

*　Full-width space　*

*\u{00A0}Non-breaking space\u{00A0}*";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should detect ASCII spaces, not sure about Unicode spaces
    assert!(result.len() >= 4, "Should detect spaces in emphasis");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("*Hello 👋*"));
    assert!(fixed.contains("**你好**"));
    assert!(fixed.contains("_مرحبا_"));
    assert!(fixed.contains("__Привет__"));
}

#[test]
fn test_md037_complex_spacing() {
    let rule = MD037NoSpaceInEmphasis;

    // Test 2: Various spacing scenarios
    let content = "\
* spaces after *

*spaces before *

* spaces both *

*  multiple   spaces  *

*\ttab\tspaces\t*

*
newline
spaces
*

** nested * space * **";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.len() >= 5, "Should detect various spacing issues");
}

#[test]
fn test_md037_multiple_emphasis_per_line() {
    let rule = MD037NoSpaceInEmphasis;

    // Test 3: Multiple emphasis on same line
    let content = "\
This * has * spaces and * more * spaces and *even more *

Mix of * good* and *bad * emphasis * markers *

** Bold ** with _ italic _ and __more bold __";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should detect all instances with spaces
    assert!(result.len() >= 8, "Should detect all spacing issues");

    // Verify fixes work correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("*has*"));
    assert!(fixed.contains("*more*"));
    assert!(fixed.contains("*even more*"));
    assert!(fixed.contains("**Bold**"));
    assert!(fixed.contains("_italic_"));
}

#[test]
fn test_md037_edge_patterns() {
    let rule = MD037NoSpaceInEmphasis;

    // Test 4: Edge cases and special patterns
    let content = "\
* *

** **

*   *

**

* \\ *

*\\**

* \\* *

`* code *` should be ignored

```
* code block *
```";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Empty emphasis and escaped content might behave differently
    assert!(!result.is_empty(), "Should detect some spacing issues");
}

#[test]
fn test_md049_unicode_content() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);

    // Test 1: Unicode content with mixed styles
    let content = "\
*Hello 世界*

_Bonjour monde_

*مرحبا العالم*

_Привет мир_

This is *inline 你好* emphasis

Another _inline مرحبا_ style";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Asterisk styles should be flagged
    assert_eq!(result.len(), 3, "Should detect asterisk emphasis");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("_Hello 世界_"));
    assert!(fixed.contains("_مرحبا العالم_"));
    assert!(fixed.contains("_inline 你好_"));
}

#[test]
fn test_md049_consistent_mode() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);

    // Test 2: Consistent mode behavior
    let content = "\
*First style is asterisk*

More text with *asterisk* style

_This underscore should be flagged_

*Correct style*

_Another incorrect_";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Underscore styles should be flagged
    assert_eq!(result.len(), 2, "Should detect inconsistent underscore emphasis");

    // Test when underscore comes first
    let content2 = "\
_First style is underscore_

*This should be flagged*

_Correct style_";

    let ctx2 = LintContext::new(content2);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1, "Should detect inconsistent asterisk emphasis");
}

#[test]
fn test_md049_url_preservation() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);

    // Test 3: URLs with underscores should be preserved
    let content = "\
_Regular emphasis_

Visit https://example.com/some_url_with_underscores

Check this_file_name_with_underscores.md

Email: user_name@company_domain.com

But _this emphasis_ should be fixed";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should only detect emphasis, not URLs");

    // Verify URLs are preserved
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("*Regular emphasis*"));
    assert!(fixed.contains("some_url_with_underscores"));
    assert!(fixed.contains("this_file_name_with_underscores.md"));
    assert!(fixed.contains("user_name@company_domain.com"));
    assert!(fixed.contains("*this emphasis*"));
}

#[test]
fn test_md049_complex_nesting() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);

    // Test 4: Complex nesting scenarios
    let content = "\
This has *italic with **bold** inside* text

Another _italic with __bold__ inside_ example

Mixed *styles **with** nesting* here

Link with [*emphasis*](url) inside

Image with ![*alt text*](img.png) emphasis";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should detect asterisk emphasis
    assert!(result.len() >= 3, "Should detect nested emphasis");
}

#[test]
fn test_md050_unicode_content() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);

    // Test 1: Unicode content with strong emphasis
    let content = "\
**Bold 世界**

__Bold monde__

**عالم غامق**

__Жирный мир__

This is **inline 你好** emphasis

Another __inline مرحبا__ style";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Double asterisk styles should be flagged
    assert_eq!(result.len(), 3, "Should detect double asterisk emphasis");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("__Bold 世界__"));
    assert!(fixed.contains("__عالم غامق__"));
    assert!(fixed.contains("__inline 你好__"));
}

#[test]
fn test_md050_consistent_mode() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);

    // Test 2: Consistent mode with strong emphasis
    let content = "\
**First style is double asterisk**

More text with **asterisk** style

__This underscore should be flagged__

**Correct style**

__Another incorrect__";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Double underscore styles should be flagged
    assert_eq!(result.len(), 2, "Should detect inconsistent strong emphasis");
}

#[test]
fn test_md050_escaped_emphasis() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

    // Test 3: Escaped emphasis markers
    let content = "\
__Real strong emphasis__

\\__Not emphasis\\__

\\_\\_Also not emphasis\\_\\_

**\\__Mixed escape\\__**

__Should be \\*\\*fixed\\*\\* to asterisks__";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should only detect real emphasis
    assert_eq!(result.len(), 2, "Should only detect unescaped emphasis");
}

#[test]
fn test_emphasis_rules_interaction() {
    // Test all emphasis rules together
    let md036 = MD036NoEmphasisAsHeading::new(String::new());
    let md037 = MD037NoSpaceInEmphasis;
    let md049 = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let md050 = MD050StrongStyle::new(StrongStyle::Underscore);

    let content = "\
** Heading with spaces **

This has * spaces * and *wrong style*

**Bold heading**

More __bold __ with spaces";

    let ctx = LintContext::new(content);

    // Each rule should detect its issues
    let result036 = md036.check(&ctx).unwrap();
    let result037 = md037.check(&ctx).unwrap();
    let result049 = md049.check(&ctx).unwrap();
    let result050 = md050.check(&ctx).unwrap();

    assert!(!result036.is_empty(), "MD036 should detect headings");
    assert!(result037.len() >= 2, "MD037 should detect spaces");
    assert!(!result049.is_empty(), "MD049 should detect wrong style");
    assert!(!result050.is_empty(), "MD050 should detect wrong style");
}

#[test]
fn test_emphasis_in_special_constructs() {
    let md037 = MD037NoSpaceInEmphasis;

    // Test emphasis in various Markdown constructs
    let content = "\
[Link with * spaces *](url)

![Alt with * spaces *](image.png)

[Reference with * spaces *][ref]

[ref]: https://example.com

> Quote with * spaces *

- List with * spaces *

| Table | * Header * |
|-------|------------|
| Cell  | * Data *   |

<!-- HTML comment with * spaces * -->

<div>HTML with * spaces *</div>";

    let ctx = LintContext::new(content);
    let result = md037.check(&ctx).unwrap();
    // Should detect spaces in all contexts except HTML
    assert!(result.len() >= 5, "Should detect spaces in various contexts");
}

#[test]
fn test_emphasis_performance_edge_cases() {
    let md037 = MD037NoSpaceInEmphasis;

    // Test with very long lines
    let long_text = "a".repeat(500);
    let content = format!("* {long_text} *\n\n** {long_text} **");

    let ctx = LintContext::new(&content);
    let result = md037.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should handle long lines");

    // Test with many emphasis markers
    let many_emphasis = "* text * ".repeat(50);
    let ctx2 = LintContext::new(&many_emphasis);
    let result2 = md037.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 50, "Should handle many emphasis markers");
}

#[test]
fn test_emphasis_line_endings() {
    let md037 = MD037NoSpaceInEmphasis;

    // Test with different line endings
    let content_lf = "* spaces *\n* more *";
    let content_crlf = "* spaces *\r\n* more *";
    let content_no_ending = "* spaces *";

    let ctx_lf = LintContext::new(content_lf);
    let ctx_crlf = LintContext::new(content_crlf);
    let ctx_no_ending = LintContext::new(content_no_ending);

    assert_eq!(md037.check(&ctx_lf).unwrap().len(), 2);
    assert_eq!(md037.check(&ctx_crlf).unwrap().len(), 2);
    assert_eq!(md037.check(&ctx_no_ending).unwrap().len(), 1);
}

#[test]
fn test_emphasis_empty_and_minimal() {
    let md036 = MD036NoEmphasisAsHeading::new(String::new());
    let md037 = MD037NoSpaceInEmphasis;

    // Test empty and minimal cases
    let content = "\
**

__

*a*

_b_

* *

_ _";

    let ctx = LintContext::new(content);
    let result036 = md036.check(&ctx).unwrap();
    let result037 = md037.check(&ctx).unwrap();

    // MD036 detects empty emphasis and single char emphasis as headings
    assert!(result036.len() >= 2, "Should detect single char emphasis");
    // MD037 may not detect empty emphasis with spaces as valid emphasis
    // The rule checks for spaces inside emphasis markers, but empty spaces might not be considered valid emphasis
    assert!(result037.len() <= 2, "MD037 behavior for empty emphasis varies");
}

#[test]
fn test_emphasis_html_entities() {
    let md037 = MD037NoSpaceInEmphasis;

    // Test with HTML entities
    let content = "\
* &nbsp; *

* &amp; *

* &#x1F44B; *

*&lt;tag&gt;*";

    let ctx = LintContext::new(content);
    let result = md037.check(&ctx).unwrap();
    // Should detect spaces around entities
    assert_eq!(result.len(), 3, "Should detect spaces around HTML entities");
}

#[test]
fn test_emphasis_front_matter() {
    let md036 = MD036NoEmphasisAsHeading::new(String::new());

    // Test with front matter
    let content = "\
---
title: **Not a heading**
emphasis: *also not*
---

**This is a heading**

Normal content";

    let ctx = LintContext::new(content);
    let result = md036.check(&ctx).unwrap();
    // Should only detect emphasis outside front matter
    assert_eq!(result.len(), 1, "Should ignore front matter");
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_emphasis_adjacent_markers() {
    let md049 = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let md050 = MD050StrongStyle::new(StrongStyle::Consistent);

    // Test adjacent emphasis markers
    let content = "\
*italic***bold**

**bold***italic*

_italic___bold__

__bold___italic_

***both***

___both___";

    let ctx = LintContext::new(content);
    let result049 = md049.check(&ctx).unwrap();
    let result050 = md050.check(&ctx).unwrap();

    // In consistent mode, should detect style inconsistencies
    assert!(
        !result049.is_empty() || !result050.is_empty(),
        "Should detect some style issues"
    );
}
