use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;

#[test]
fn test_unicode_cli_lsp_consistency() {
    // Unicode content similar to our test file but smaller for focused testing
    let unicode_content = r#"# 🚀 This is a header with emoji
مرحبا بكم في هذا النص العربي
(https://例え.テスト)[Japanese domain link]
Here is some `中文代码` in inline code
"#;

    // Test key rules with Unicode content
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD034NoBareUrls),
        Box::new(MD047SingleTrailingNewline),
    ];

    for rule in &rules {
        let ctx = LintContext::new(unicode_content);

        // Get warnings from check method (LSP style)
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        // Apply fixes using both CLI and LSP methods
        let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
        let lsp_fixed = apply_warning_fixes(unicode_content, &warnings).expect("LSP fix should succeed");

        // Critical test: Results should be identical
        assert_eq!(
            cli_fixed,
            lsp_fixed,
            "Rule {} produced different CLI vs LSP results for Unicode content:\nCLI: '{}'\nLSP: '{}'",
            rule.name(),
            cli_fixed,
            lsp_fixed
        );

        // Validate that all fixes have valid byte ranges
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                assert!(
                    fix.range.start <= unicode_content.len(),
                    "Rule {} fix range start {} exceeds content length {} for Unicode content",
                    rule.name(),
                    fix.range.start,
                    unicode_content.len()
                );

                assert!(
                    fix.range.end <= unicode_content.len(),
                    "Rule {} fix range end {} exceeds content length {} for Unicode content",
                    rule.name(),
                    fix.range.end,
                    unicode_content.len()
                );

                // Ensure byte boundaries are valid UTF-8 character boundaries
                assert!(
                    unicode_content.is_char_boundary(fix.range.start),
                    "Rule {} fix range start {} is not a valid UTF-8 char boundary",
                    rule.name(),
                    fix.range.start
                );

                assert!(
                    unicode_content.is_char_boundary(fix.range.end),
                    "Rule {} fix range end {} is not a valid UTF-8 char boundary",
                    rule.name(),
                    fix.range.end
                );
            }
        }
    }
}

#[test]
fn test_complex_unicode_scenarios() {
    let test_cases = vec![
        // Combining characters
        ("Café with combining é", "é"), // e + combining acute
        // Right-to-left text
        ("مرحبا بكم في هذا النص العربي", "Arabic text"),
        // Mixed scripts
        ("Hello世界こんにちは", "Mixed Japanese/English"),
        // Emoji with zero-width joiners
        ("👨‍👩‍👧‍👦 Family emoji", "family with ZWJ"),
        // CJK punctuation
        ("这是中文。", "Chinese with CJK period"),
        // Mathematical symbols
        ("∑ᵢ₌₁ⁿ xᵢ = total", "Math symbols with subscripts/superscripts"),
    ];

    let rule = MD047SingleTrailingNewline;

    for (content, description) in test_cases {
        let ctx = LintContext::new(content);

        // Test that rule can handle the content without panicking
        let warnings_result = rule.check(&ctx);
        assert!(
            warnings_result.is_ok(),
            "Rule {} failed to check content with {}: '{}'",
            rule.name(),
            description,
            content
        );

        let fix_result = rule.fix(&ctx);
        assert!(
            fix_result.is_ok(),
            "Rule {} failed to fix content with {}: '{}'",
            rule.name(),
            description,
            content
        );

        // If there are warnings, test LSP consistency
        let warnings = warnings_result.unwrap();
        if !warnings.is_empty() {
            let cli_fixed = fix_result.unwrap();
            let lsp_fixed = apply_warning_fixes(content, &warnings);
            assert!(
                lsp_fixed.is_ok(),
                "LSP fix failed for content with {description}: '{content}'"
            );

            assert_eq!(
                cli_fixed,
                lsp_fixed.unwrap(),
                "CLI/LSP inconsistency for content with {description}: '{content}'"
            );
        }
    }
}

#[test]
fn test_unicode_byte_boundary_validation() {
    // Test that all fix ranges respect UTF-8 byte boundaries
    let unicode_content = "# 🚀🎉🔥 Unicode Header\n中文内容 with 日本語\n```\n코드 블록\n```\n";

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD047SingleTrailingNewline),
    ];

    for rule in &rules {
        let ctx = LintContext::new(unicode_content);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                // Validate byte ranges
                assert!(
                    fix.range.start <= unicode_content.len(),
                    "Fix range start exceeds content length"
                );
                assert!(
                    fix.range.end <= unicode_content.len(),
                    "Fix range end exceeds content length"
                );

                // Critical: Validate UTF-8 byte boundaries
                assert!(
                    unicode_content.is_char_boundary(fix.range.start),
                    "Fix range start {} is not on UTF-8 char boundary for rule {}",
                    fix.range.start,
                    rule.name()
                );
                assert!(
                    unicode_content.is_char_boundary(fix.range.end),
                    "Fix range end {} is not on UTF-8 char boundary for rule {}",
                    fix.range.end,
                    rule.name()
                );

                // Test that the replacement can be applied safely
                let mut test_content = unicode_content.to_string();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    test_content.replace_range(fix.range.clone(), &fix.replacement);
                    test_content
                }));

                assert!(
                    result.is_ok(),
                    "Fix replacement panicked for rule {} with range {:?}",
                    rule.name(),
                    fix.range
                );

                // Ensure result is valid UTF-8
                let fixed_content = result.unwrap();
                assert!(
                    fixed_content.is_ascii() || std::str::from_utf8(fixed_content.as_bytes()).is_ok(),
                    "Fix produced invalid UTF-8 for rule {}",
                    rule.name()
                );
            }
        }
    }
}
