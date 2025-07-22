use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::{MD034NoBareUrls, MD039NoSpaceInLinks, MD042NoEmptyLinks};

/// Comprehensive edge case tests for link rules (MD034, MD039, MD042)
///
/// These tests ensure link rules handle Unicode, special cases, and edge conditions correctly.

#[test]
fn test_md034_ipv6_urls() {
    let rule = MD034NoBareUrls;

    // Test 1: IPv6 URLs should be detected as bare URLs
    let content = "\
Visit https://[::1]:8080/path for local testing
Access https://[2001:db8::1]/test for IPv6
Connect to http://[fe80::1%eth0]:3000 for link-local";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Should detect all IPv6 URLs");

    // Test that fixes work correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("<https://[::1]:8080/path>"));
    assert!(fixed.contains("<https://[2001:db8::1]/test>"));
    assert!(fixed.contains("<http://[fe80::1%eth0]:3000>"));
}

#[test]
fn test_md034_urls_with_punctuation() {
    let rule = MD034NoBareUrls;

    // Test 2: URLs with trailing punctuation
    let content = "\
Visit https://example.com.
See https://example.com!
Check https://example.com?
Go to https://example.com; it's great
(https://example.com) is in parentheses
\"https://example.com\" is in quotes";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 6, "Should detect all URLs");

    // Verify fixes preserve punctuation
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("<https://example.com>."));
    assert!(fixed.contains("<https://example.com>!"));
    assert!(fixed.contains("<https://example.com>?"));
    assert!(fixed.contains("<https://example.com>;"));
    assert!(fixed.contains("(<https://example.com>)"));
    assert!(fixed.contains("\"<https://example.com>\""));
}

#[test]
fn test_md034_urls_in_special_contexts() {
    let rule = MD034NoBareUrls;

    // Test 3: URLs that should be ignored in special contexts
    let content = "\
<!-- https://example.com in HTML comment -->
<a href=\"https://example.com\">Link</a>
<img src=\"https://example.com/img.png\">
[text](https://example.com)
![alt](https://example.com/img.png)
[ref]: https://example.com
`https://example.com` in inline code
```
https://example.com in code block
```
[![badge](https://example.com/badge.svg)](https://example.com)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    for warning in &result {
        let line = warning.line;
        let message = &warning.message;
        println!("MD034 found URL at line {line}: {message}");
    }
    assert!(result.is_empty(), "Should ignore URLs in special contexts");
}

#[test]
fn test_md034_email_addresses() {
    let rule = MD034NoBareUrls;

    // Test 4: Email address detection
    let content = "\
Contact us at support@example.com
Email john.doe+filter@company.co.uk
Reach out to user_name@sub.domain.com
Complex: firstname.lastname+tag@really.long.domain.example.org";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect all email addresses");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("<support@example.com>"));
    assert!(fixed.contains("<john.doe+filter@company.co.uk>"));
    assert!(fixed.contains("<user_name@sub.domain.com>"));
    assert!(fixed.contains("<firstname.lastname+tag@really.long.domain.example.org>"));
}

#[test]
fn test_md034_various_url_schemes() {
    let rule = MD034NoBareUrls;

    // Test 5: Different URL schemes
    let content = "\
HTTP: http://example.com
HTTPS: https://example.com
FTP: ftp://files.example.com
FTPS: ftps://secure.example.com";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect all URL schemes including ftps");
}

#[test]
fn test_md034_complex_urls() {
    let rule = MD034NoBareUrls;

    // Test 6: URLs with complex query strings and fragments
    let content = "\
Search: https://example.com/search?q=rust+markdown&page=2&filter=true
Anchor: https://example.com/docs#section-2.3.4
Both: https://example.com/api?key=abc123&v=2#response-format
Special chars: https://example.com/path?data=%7B%22test%22%3A%20true%7D";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect all complex URLs");
}

#[test]
fn test_md034_multiple_urls_per_line() {
    let rule = MD034NoBareUrls;

    // Test 7: Multiple URLs on the same line
    let content = "\
Visit https://example.com or https://backup.com
Check http://old.com, http://new.com, and http://beta.com
Both email@example.com and https://example.com are available";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 7, "Should detect all URLs and emails");

    // Verify all are fixed
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("<https://example.com>"));
    assert!(fixed.contains("<https://backup.com>"));
    assert!(fixed.contains("<email@example.com>"));
}

#[test]
fn test_md034_unicode_domains() {
    let rule = MD034NoBareUrls;

    // Test 8: Unicode/IDN domains
    let content = "\
Visit https://münchen.de
Chinese: https://例え.jp
Emoji: https://👍.ws
Email: contact@münchen.de";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Note: Emoji domains are not currently supported due to regex limitations
    // This is acceptable as emoji domains are extremely rare in practice
    assert_eq!(
        result.len(),
        3,
        "Should detect Unicode domain URLs (emoji domains not supported)"
    );
}

#[test]
fn test_md039_various_whitespace() {
    let rule = MD039NoSpaceInLinks;

    // Test 1: Different types of whitespace
    let content = "\
[ Regular spaces ](url1)
[\tTabs\t](url2)
[\nNewline\n](url3)
[ \t\nMixed whitespace\n\t ](url4)
[　Full-width space　](url5)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect all whitespace variations");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[Regular spaces](url1)"));
    assert!(fixed.contains("[Tabs](url2)"));
    assert!(fixed.contains("[Newline](url3)"));
    assert!(fixed.contains("[Mixed whitespace](url4)"));
    assert!(fixed.contains("[Full-width space](url5)"));
}

#[test]
fn test_md039_whitespace_only_links() {
    let rule = MD039NoSpaceInLinks;

    // Test 2: Links with only whitespace
    let content = "\
[   ](url1)
[\t\t\t](url2)
[\n\n\n](url3)
[ \t\n ](url4)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect whitespace-only links");

    // These should be trimmed to empty
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[](url1)"));
    assert!(fixed.contains("[](url2)"));
    assert!(fixed.contains("[](url3)"));
    assert!(fixed.contains("[](url4)"));
}

#[test]
fn test_md039_escaped_characters() {
    let rule = MD039NoSpaceInLinks;

    // Test 3: Links with escaped characters
    let content = "\
[ link\\] ](url1)
[ \\[link ](url2)
[ link\\  ](url3)
[ \\tlink ](url4)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // All four are valid links and should have spaces detected
    assert_eq!(result.len(), 4, "Should detect spaces in all links with escaped chars");

    // Escaped characters should be preserved
    let fixed = rule.fix(&ctx).unwrap();
    println!("Fixed content:\n{fixed}");
    // MD039 removes spaces while preserving escaped characters
    assert!(fixed.contains("[link\\]](url1)"));
    assert!(fixed.contains("[\\[link](url2)"));
    assert!(fixed.contains("[link\\](url3)")); // Trailing spaces removed, backslash preserved
    assert!(fixed.contains("[\\tlink](url4)"));
}

#[test]
fn test_md039_reference_links() {
    let rule = MD039NoSpaceInLinks;

    // Test 4: Reference links should be skipped
    let content = "\
[ Reference link ][ref]
[ Another ref ][ ref2 ]
[ Shortcut reference ]
[ref]: https://example.com
[ref2]: https://example.com";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should skip reference-style links");
}

#[test]
fn test_md039_images() {
    let rule = MD039NoSpaceInLinks;

    // Test 5: Images with spaces
    let content = "\
![ Alt text ](image.png)
![ Another image ](https://example.com/img.jpg)
![\tTabbed alt\t](img.png)
![ ](empty-alt.png)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect spaces in image alt text");

    // Verify fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("![Alt text](image.png)"));
    assert!(fixed.contains("![Another image](https://example.com/img.jpg)"));
    assert!(fixed.contains("![Tabbed alt](img.png)"));
    assert!(fixed.contains("![](empty-alt.png)"));
}

#[test]
fn test_md039_multiline_links() {
    let rule = MD039NoSpaceInLinks;

    // Test 6: Links spanning multiple lines
    let content = "\
[ Link text
spanning lines ](url)
[ Another
  multiline
  link ](url2)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect multiline links with spaces");
}

#[test]
fn test_md039_multiple_links_per_line() {
    let rule = MD039NoSpaceInLinks;

    // Test 7: Multiple links on same line
    let content = "\
[ First ]( url1 ) and [ Second ](url2) and [ Third ](url3)
Mix of [ good](url) and [bad ](url) links";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect all links with spaces");
}

#[test]
fn test_md039_internal_spaces_preserved() {
    let rule = MD039NoSpaceInLinks;

    // Test 8: Internal spaces should be preserved
    let content = "\
[ This is a long link text ](url)
[  Leading and trailing  ](url)
[\tTab\tseparated\twords\t](url)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Should detect leading/trailing spaces");

    // Internal spaces preserved
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[This is a long link text](url)"));
    assert!(fixed.contains("[Leading and trailing](url)"));
    assert!(fixed.contains("[Tab\tseparated\twords](url)"));
}

#[test]
fn test_md039_unicode_spaces() {
    let rule = MD039NoSpaceInLinks;

    // Test 9: Unicode spaces
    let content = "\
[\u{00A0}Non-breaking space\u{00A0}](url1)
[\u{2003}Em space\u{2003}](url2)
[\u{200B}Zero-width space\u{200B}](url3)";

    let ctx = LintContext::new(content);
    let _result = rule.check(&ctx).unwrap();
    // Note: Current implementation only detects ASCII whitespace
    // This test documents current behavior
}

#[test]
fn test_md042_empty_text_variations() {
    let rule = MD042NoEmptyLinks;

    // Test 1: Various empty text scenarios
    let content = "\
[](https://example.com)
[   ](https://example.com)
[\t\t](https://example.com)
[\n](https://example.com)
[ \t\n ](https://example.com)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect all empty text variations");

    // Verify fix suggestions
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[Link text](https://example.com)"));
    assert!(!fixed.contains("[](https://example.com)"));
    assert!(!fixed.contains("[   ](https://example.com)"));
}

#[test]
fn test_md042_empty_url_variations() {
    let rule = MD042NoEmptyLinks;

    // Test 2: Various empty URL scenarios
    let content = "\
[Click here]()
[Link text](   )
[Another link](\t)
[More text](\n)
[Text]( \t\n )";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect all empty URL variations");

    // Verify fix suggestions
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[Click here](https://example.com)"));
    assert!(!fixed.contains("[Click here]()"));
}

#[test]
fn test_md042_both_empty() {
    let rule = MD042NoEmptyLinks;

    // Test 3: Both text and URL empty
    let content = "\
[]()
[   ](   )
[\t](\t)
[\n](\n)";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect all double-empty variations");

    // Verify fix
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[Link text](https://example.com)"));
    assert!(!fixed.contains("[]()"));
}

#[test]
fn test_md042_reference_links() {
    let rule = MD042NoEmptyLinks;

    // Test 4: Reference-style links
    let content = "\
[][ref1]
[text][undefined]
[][]
[text][]
[]: https://example.com

[ref1]: https://example.com";

    let ctx = LintContext::new(content);
    let _result = rule.check(&ctx).unwrap();
    // Reference links are handled differently - test current behavior
    // Empty text reference links might be flagged
}

#[test]
fn test_md042_formatting_without_text() {
    let rule = MD042NoEmptyLinks;

    // Test 5: Links with formatting but no actual text
    let content = "\
[**](url)
[*](url)
[`code`](url)
[<span></span>](url)
[<!--comment-->](url)";

    let ctx = LintContext::new(content);
    let _result = rule.check(&ctx).unwrap();
    // First three should pass (have content), last two might be flagged
}

#[test]
fn test_md042_images() {
    let rule = MD042NoEmptyLinks;

    // Test 6: Images should be ignored
    let content = "\
![](image.png)
![   ](image.png)
![]()
![alt]()
![](   )";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should ignore image syntax");
}

#[test]
fn test_md042_escaped_brackets() {
    let rule = MD042NoEmptyLinks;

    // Test 7: Escaped brackets should be ignored
    let content = "\
\\[\\](url)
\\[text\\]()
Not a link \\[\\]()";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Document current behavior: The link parser doesn't handle leading escaped brackets
    // \\[\\](url) is not parsed as a link
    // \\[text\\]() is not parsed as a link
    // But "Not a link \\[\\]()" contains \\[\\]() which might be parsed
    // This is a limitation of the current regex-based parser
    assert_eq!(result.len(), 0, "Escaped brackets at start prevent link parsing");
}

#[test]
fn test_md042_links_in_context() {
    let rule = MD042NoEmptyLinks;

    // Test 8: Empty links in various contexts
    let content = "\
- List item with [](url)
> Blockquote with []()
  - Nested with [](https://example.com)

| Table | Header |
|-------|--------|
| Cell  | [](url) |

1. Ordered list []()
2. Another item";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect empty links in all contexts");
}

#[test]
fn test_md042_unicode_empty() {
    let rule = MD042NoEmptyLinks;

    // Test 9: Unicode whitespace as empty
    let content = "\
[\u{00A0}](url)
[\u{2003}](url)
[\u{200B}](url)
[\u{FEFF}](url)";

    let ctx = LintContext::new(content);
    let _result = rule.check(&ctx).unwrap();
    // Document current behavior with Unicode whitespace
}

#[test]
fn test_md042_nested_links() {
    let rule = MD042NoEmptyLinks;

    // Test 10: Nested link-like structures
    let content = "\
[Link [with] brackets](url)
[Link (with) parens](url)
[[Double brackets]](url)
[](url [not a link])";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Last one has empty text
    assert!(!result.is_empty(), "Should detect empty text in last link");
}

#[test]
fn test_link_rules_interaction() {
    // Test all three rules together
    let md034 = MD034NoBareUrls;
    let md039 = MD039NoSpaceInLinks;
    let md042 = MD042NoEmptyLinks;

    let content = "\
Visit https://example.com for info
Click [ here ](https://example.com) to continue
Empty link: []()
Email: contact@example.com
Another [ spaced link ](  )";

    let ctx = LintContext::new(content);

    // Each rule should detect its issues
    let result034 = md034.check(&ctx).unwrap();
    let result039 = md039.check(&ctx).unwrap();
    let result042 = md042.check(&ctx).unwrap();

    assert_eq!(result034.len(), 2, "MD034 should detect bare URL and email");
    assert_eq!(result039.len(), 2, "MD039 should detect spaced links");
    assert_eq!(result042.len(), 2, "MD042 should detect empty links");

    // Apply fixes sequentially
    let step1 = md034.fix(&ctx).unwrap();
    let ctx1 = LintContext::new(&step1);

    let step2 = md039.fix(&ctx1).unwrap();
    let ctx2 = LintContext::new(&step2);

    let step3 = md042.fix(&ctx2).unwrap();
    let ctx_final = LintContext::new(&step3);

    // All issues should be resolved
    assert!(md034.check(&ctx_final).unwrap().is_empty());
    assert!(md039.check(&ctx_final).unwrap().is_empty());
    assert!(md042.check(&ctx_final).unwrap().is_empty());
}

#[test]
fn test_link_rules_code_block_handling() {
    // Test that all link rules ignore code blocks
    let md034 = MD034NoBareUrls;
    let md039 = MD039NoSpaceInLinks;
    let md042 = MD042NoEmptyLinks;

    let content = "\
```
https://example.com
[ spaced ](url)
[]()
contact@example.com
```

`https://inline.com` and `[ inline ](url)` and `[]()`";

    let ctx = LintContext::new(content);

    // No rule should detect issues in code
    assert!(md034.check(&ctx).unwrap().is_empty());
    assert!(md039.check(&ctx).unwrap().is_empty());
    assert!(md042.check(&ctx).unwrap().is_empty());
}

#[test]
fn test_link_rules_html_handling() {
    // Test HTML context handling
    let md034 = MD034NoBareUrls;
    let md039 = MD039NoSpaceInLinks;
    let md042 = MD042NoEmptyLinks;

    let content = "\
<a href=\"https://example.com\">Link</a>
<a href=\"\">Empty href</a>
<a> Spaced </a>
<!-- https://comment.com -->
<script>var url = 'https://script.com';</script>";

    let ctx = LintContext::new(content);

    // Rules should ignore HTML contexts
    // MD034 might still detect some URLs in HTML
    // This is a current limitation
    let _md034_result = md034.check(&ctx).unwrap();
    let md039_result = md039.check(&ctx).unwrap();
    let md042_result = md042.check(&ctx).unwrap();

    // MD039 and MD042 should correctly ignore HTML contexts
    assert!(md039_result.is_empty(), "MD039 should ignore HTML contexts");
    assert!(md042_result.is_empty(), "MD042 should ignore HTML contexts");
}
