//! Comprehensive UTF-8 stress tests for all rules
//!
//! This module tests every rule against content with various multi-byte UTF-8 characters
//! to catch panics from invalid string slicing at character boundaries.
//!
//! The key insight: Regex matches return byte offsets, but string slicing with
//! arithmetic on those offsets (e.g., `line[start - 5..start]`) can land inside
//! multi-byte characters and cause panics.

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use std::panic;

/// Multi-byte test strings for various scripts
/// Each string is designed to have multi-byte characters in positions that
/// could cause issues with backward slicing operations
const TEST_SCRIPTS: &[(&str, &str)] = &[
    // Bengali (3 bytes per character) - the script that triggered the MD034 panic
    ("bengali", "কুবারনেটিস কমিউনিটির মধ্যে ঘটে যাওয়া ঘটনাগুলির জন্য"),
    // Arabic (2-3 bytes per character)
    ("arabic", "مرحبا بكم في هذا النص العربي الطويل"),
    // Chinese (3 bytes per character)
    ("chinese", "这是一段很长的中文文本用于测试"),
    // Japanese (3 bytes per character)
    ("japanese", "日本語テキストのサンプルです"),
    // Korean (3 bytes per character)
    ("korean", "한글 텍스트 샘플입니다"),
    // Thai (3 bytes per character)
    ("thai", "นี่คือข้อความภาษาไทยสำหรับการทดสอบ"),
    // Hindi (3 bytes per character)
    ("hindi", "यह हिंदी में एक लंबा पाठ है"),
    // Russian (2 bytes per character)
    ("russian", "Это длинный русский текст для тестирования"),
    // Greek (2 bytes per character)
    ("greek", "Αυτό είναι ένα μεγάλο ελληνικό κείμενο"),
    // Emoji (4 bytes per character)
    ("emoji", "🎉🚀💻🔥✨🎊🎁🎈🎂🌟"),
    // Mixed emoji with ZWJ sequences (variable length)
    ("emoji_zwj", "👨‍👩‍👧‍👦 👩‍💻 🏳️‍🌈"),
];

/// Content patterns that combine multi-byte text with constructs that require parsing
fn generate_test_content(_script_name: &str, script_text: &str) -> Vec<(&'static str, String)> {
    vec![
        // Email addresses after multi-byte text (MD034 pattern)
        (
            "email_after_multibyte",
            format!("{script_text} user@example.com more text"),
        ),
        // Email addresses surrounded by multi-byte text
        (
            "email_surrounded",
            format!("{script_text} contact@test.org {script_text}"),
        ),
        // URLs after multi-byte text
        ("url_after_multibyte", format!("{script_text} https://example.com/path")),
        // Headings with multi-byte text
        ("heading_multibyte", format!("# {script_text}\n\nSome content here.")),
        // Lists with multi-byte text
        (
            "list_multibyte",
            format!("- {script_text}\n- Another item\n- Third {script_text}"),
        ),
        // Code spans with multi-byte text nearby
        ("code_span_nearby", format!("{script_text} `code here` {script_text}")),
        // Links with multi-byte text
        (
            "link_multibyte",
            format!("[{script_text}](https://example.com) more text"),
        ),
        // Emphasis with multi-byte text
        ("emphasis_multibyte", format!("*{script_text}* and **{script_text}**")),
        // Block quotes with multi-byte text
        ("blockquote_multibyte", format!("> {script_text}\n> More quoted text")),
        // Tables with multi-byte text
        (
            "table_multibyte",
            format!("| Header 1 | {script_text} |\n| --- | --- |\n| {script_text} | Data |"),
        ),
        // Mixed content stress test
        (
            "mixed_stress",
            format!(
                "# {script_text}\n\n{script_text} user@example.com\n\n- {script_text}\n- https://test.com\n\n> {script_text}\n\n`{script_text}`"
            ),
        ),
    ]
}

/// Get all rules for testing
/// Note: Some rules require specific configuration and are excluded to keep the test simple.
/// The important rules for UTF-8 testing (especially those with regex/string operations) are included.
fn get_all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(MD001HeadingIncrement::default()),
        Box::new(MD003HeadingStyle::default()),
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD005ListIndent::default()),
        Box::new(MD007ULIndent::default()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD010NoHardTabs::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD014CommandsShowOutput::default()),
        Box::new(MD018NoMissingSpaceAtx),
        Box::new(MD019NoMultipleSpaceAtx),
        Box::new(MD020NoMissingSpaceClosedAtx),
        Box::new(MD021NoMultipleSpaceClosedAtx),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD023HeadingStartLeft),
        Box::new(MD024NoDuplicateHeading::default()),
        Box::new(MD025SingleTitle::default()),
        Box::new(MD026NoTrailingPunctuation::default()),
        Box::new(MD027MultipleSpacesBlockquote),
        Box::new(MD028NoBlanksBlockquote),
        Box::new(MD029OrderedListPrefix::default()),
        Box::new(MD030ListMarkerSpace::default()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD033NoInlineHtml::default()),
        Box::new(MD034NoBareUrls), // The rule that had the UTF-8 panic
        Box::new(MD035HRStyle::default()),
        Box::new(MD036NoEmphasisAsHeading::default()),
        Box::new(MD037NoSpaceInEmphasis),
        Box::new(MD038NoSpaceInCode::default()),
        Box::new(MD039NoSpaceInLinks),
        Box::new(MD040FencedCodeLanguage),
        Box::new(MD041FirstLineHeading::default()),
        Box::new(MD042NoEmptyLinks::default()),
        // MD044 requires names config, skip
        Box::new(MD045NoAltText::default()),
        // MD046 and MD048 require style config, skip
        Box::new(MD047SingleTrailingNewline),
        Box::new(MD049EmphasisStyle::default()),
        Box::new(MD050StrongStyle::default()),
        Box::new(MD051LinkFragments::new()),
        Box::new(MD052ReferenceLinkImages::default()),
        Box::new(MD053LinkImageReferenceDefinitions::default()),
        Box::new(MD054LinkImageStyle::default()),
        Box::new(MD055TablePipeStyle::default()),
        Box::new(MD056TableColumnCount),
        Box::new(MD057ExistingRelativeLinks::default()),
        Box::new(MD058BlanksAroundTables::default()),
    ]
}

/// Test that no rule panics when processing multi-byte UTF-8 content
#[test]
fn test_all_rules_no_panic_with_multibyte_utf8() {
    let rules = get_all_rules();
    let mut failures = Vec::new();

    for (script_name, script_text) in TEST_SCRIPTS {
        for (pattern_name, content) in generate_test_content(script_name, script_text) {
            let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

            for rule in &rules {
                let rule_name = rule.name();

                // Test check() doesn't panic
                let check_result = panic::catch_unwind(panic::AssertUnwindSafe(|| rule.check(&ctx)));

                if check_result.is_err() {
                    failures.push(format!(
                        "PANIC in {rule_name}.check() with {script_name}/{pattern_name}"
                    ));
                    continue;
                }

                // Test fix() doesn't panic
                let fix_result = panic::catch_unwind(panic::AssertUnwindSafe(|| rule.fix(&ctx)));

                if fix_result.is_err() {
                    failures.push(format!("PANIC in {rule_name}.fix() with {script_name}/{pattern_name}"));
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "UTF-8 panics detected in {} cases:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

/// Specifically test MD034 with various multi-byte prefixes before emails
/// This is the exact pattern that caused the kubernetes/website panic
#[test]
fn test_md034_email_with_all_scripts() {
    let rule = MD034NoBareUrls;

    for (script_name, script_text) in TEST_SCRIPTS {
        // Create content with email immediately after multi-byte text
        // The "xmpp:" check does `line[start - 5..start]` which could panic
        let content = format!("{script_text} user@example.com");

        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| rule.check(&ctx)));

        assert!(
            result.is_ok(),
            "MD034.check() panicked with {script_name} script before email"
        );

        let warnings = result.unwrap().expect("check should succeed");

        // Email should be detected
        assert!(
            !warnings.is_empty(),
            "MD034 should detect email after {script_name} text"
        );
    }
}

/// Test that fix byte ranges are valid UTF-8 character boundaries
#[test]
fn test_fix_ranges_are_valid_char_boundaries() {
    let rules = get_all_rules();
    let mut failures = Vec::new();

    for (script_name, script_text) in TEST_SCRIPTS {
        for (pattern_name, content) in generate_test_content(script_name, script_text) {
            let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

            for rule in &rules {
                let rule_name = rule.name();

                if let Ok(warnings) = rule.check(&ctx) {
                    for (i, warning) in warnings.iter().enumerate() {
                        if let Some(fix) = &warning.fix {
                            // Check start boundary
                            if !content.is_char_boundary(fix.range.start) {
                                failures.push(format!(
                                    "{rule_name} warning {i}: fix.range.start {} is not a char boundary ({script_name}/{pattern_name})",
                                    fix.range.start
                                ));
                            }

                            // Check end boundary
                            if !content.is_char_boundary(fix.range.end) {
                                failures.push(format!(
                                    "{rule_name} warning {i}: fix.range.end {} is not a char boundary ({script_name}/{pattern_name})",
                                    fix.range.end
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "Invalid fix byte ranges detected in {} cases:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

/// Test edge case: email address at exact byte positions that could cause issues
#[test]
fn test_md034_email_at_various_byte_offsets() {
    let rule = MD034NoBareUrls;

    // Create content where the email starts at different byte offsets
    // to test the `start - 5` check for "xmpp:"
    let test_cases = [
        // Email at byte 0 (start - 5 would be negative)
        "user@example.com",
        // Email at byte 1
        "a user@example.com",
        // Email at byte 2
        "ab user@example.com",
        // Email at byte 3
        "abc user@example.com",
        // Email at byte 4
        "abcd user@example.com",
        // Email at byte 5 (exactly where xmpp: check happens)
        "abcde user@example.com",
        // Email at byte 6
        "abcdef user@example.com",
        // With multi-byte char (3 bytes) + space, email at byte 4
        "日 user@example.com",
        // With two multi-byte chars (6 bytes) + space, email at byte 7
        "日本 user@example.com",
        // XMPP URI should not flag the email part
        "xmpp:user@example.com",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| rule.check(&ctx)));

        assert!(result.is_ok(), "MD034.check() panicked with content: {content}");
    }
}
