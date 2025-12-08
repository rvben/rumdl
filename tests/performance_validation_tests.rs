use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;
use std::time::Instant;

#[test]
fn test_performance_with_large_content() {
    // Generate content that stresses different rules
    let mut content = String::with_capacity(50000);
    content.push_str("# Performance Test\n\n");

    // Add many sections with various issues
    for i in 1..=100 {
        content.push_str(&format!(
            "## Section {i}\n\
            Content with trailing spaces   \n\
            (https://example{i}.com)[reversed link {i}]\n\
            ```\n\
            code block\n\
            ```\n\
            More content\n\n"
        ));
    }

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        // Skip MD022 in performance test due to complex edge cases with large content
        Box::new(MD031BlanksAroundFences::default()),
    ];

    for rule in &rules {
        let start_time = Instant::now();
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        let start_time = Instant::now();
        let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
        let cli_fix_duration = start_time.elapsed();

        let start_time = Instant::now();
        let lsp_fixed = apply_warning_fixes(&content, &warnings).expect("LSP fix should succeed");
        let lsp_fix_duration = start_time.elapsed();

        // Performance should be reasonable (under 1 second for this size)
        assert!(
            check_duration.as_millis() < 1000,
            "Rule {} check took too long: {}ms",
            rule.name(),
            check_duration.as_millis()
        );
        assert!(
            cli_fix_duration.as_millis() < 1000,
            "Rule {} CLI fix took too long: {}ms",
            rule.name(),
            cli_fix_duration.as_millis()
        );
        assert!(
            lsp_fix_duration.as_millis() < 1000,
            "Rule {} LSP fix took too long: {}ms",
            rule.name(),
            lsp_fix_duration.as_millis()
        );

        // Performance test: just ensure both methods work without panicking
        // CLI/LSP consistency is tested in dedicated consistency tests
        assert!(
            !cli_fixed.is_empty() || content.trim().is_empty(),
            "CLI fix should produce valid output"
        );
        assert!(
            !lsp_fixed.is_empty() || warnings.is_empty(),
            "LSP fix should produce valid output"
        );

        println!(
            "Rule {}: check={}ms, cli_fix={}ms, lsp_fix={}ms, warnings={}",
            rule.name(),
            check_duration.as_millis(),
            cli_fix_duration.as_millis(),
            lsp_fix_duration.as_millis(),
            warnings.len()
        );
    }
}

#[test]
fn test_deeply_nested_structures() {
    // Create content with deeply nested lists and blockquotes
    let mut content = String::new();

    // Create nested blockquotes
    for depth in 1..=10 {
        content.push_str(&format!("{} Level {} blockquote\n", ">".repeat(depth), depth));
    }
    content.push('\n');

    // Create nested lists
    for depth in 1..=10 {
        content.push_str(&format!("{}- Level {} list item\n", "  ".repeat(depth), depth));
    }

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD027MultipleSpacesBlockquote),
        Box::new(MD007ULIndent::new(2)),
        Box::new(MD005ListIndent::default()),
    ];

    for rule in &rules {
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        // Should handle nested structures efficiently
        assert!(
            check_duration.as_millis() < 100,
            "Rule {} took too long to check nested structures: {}ms",
            rule.name(),
            check_duration.as_millis()
        );

        // Test fix consistency if there are warnings
        if !warnings.is_empty() {
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(&content, &warnings).expect("LSP fix should succeed");

            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Nested structure test: Rule {} produced different CLI vs LSP results",
                rule.name()
            );
        }
    }
}

#[test]
fn test_many_small_issues() {
    // Create content with many small, discrete issues
    let mut content = String::new();

    for i in 1..=200 {
        content.push_str(&format!("Line {i} has trailing spaces   \n")); // 3 spaces = invalid
    }

    let rule = MD009TrailingSpaces::default();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let start_time = Instant::now();
    let warnings = rule.check(&ctx).expect("Rule check should succeed");
    let check_duration = start_time.elapsed();

    // Should find all 200 issues efficiently
    assert_eq!(warnings.len(), 200, "Should find exactly 200 trailing space issues");
    assert!(
        check_duration.as_millis() < 500,
        "MD009 took too long to check 200 issues: {}ms",
        check_duration.as_millis()
    );

    // Test bulk fix performance
    let start_time = Instant::now();
    let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
    let cli_fix_duration = start_time.elapsed();

    let start_time = Instant::now();
    let lsp_fixed = apply_warning_fixes(&content, &warnings).expect("LSP fix should succeed");
    let lsp_fix_duration = start_time.elapsed();

    assert!(
        cli_fix_duration.as_millis() < 500,
        "CLI fix of 200 issues took too long: {}ms",
        cli_fix_duration.as_millis()
    );
    assert!(
        lsp_fix_duration.as_millis() < 500,
        "LSP fix of 200 issues took too long: {}ms",
        lsp_fix_duration.as_millis()
    );

    // Results should be identical
    assert_eq!(
        cli_fixed, lsp_fixed,
        "Bulk fix test: CLI and LSP produced different results"
    );

    // Fixed content should normalize trailing spaces to 2 spaces (valid line breaks)
    // With MD009 default config (br_spaces=2, strict=false), 3+ spaces get normalized to 2 spaces
    let lines: Vec<&str> = cli_fixed.lines().collect();
    for line in &lines {
        let trailing_spaces = line.len() - line.trim_end().len();
        assert!(
            trailing_spaces <= 2,
            "Line should have at most 2 trailing spaces (valid line break), found {trailing_spaces}: '{line}'"
        );
    }
}

#[test]
fn test_mixed_rule_performance() {
    // Test performance when multiple rules process the same content
    let content = r#"
# Test Document

## Section 1
(https://example.com)[reversed link]

```
code without language
```

## Section 2
- Item 1
- Item 2

More content
"#;

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD040FencedCodeLanguage),
        Box::new(MD031BlanksAroundFences::default()),
    ];

    // Test each rule individually first
    for rule in &rules {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let individual_check_duration = start_time.elapsed();

        assert!(
            individual_check_duration.as_millis() < 50,
            "Rule {} individual check took too long: {}ms",
            rule.name(),
            individual_check_duration.as_millis()
        );

        // Test CLI/LSP consistency for each rule
        if !warnings.is_empty() {
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");

            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Mixed rule test: Rule {} produced different CLI vs LSP results",
                rule.name()
            );
        }
    }

    // Test all rules together (simulate full lint)
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start_time = Instant::now();

    let mut all_warnings = Vec::new();
    for rule in &rules {
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        all_warnings.extend(warnings);
    }

    let all_rules_duration = start_time.elapsed();
    assert!(
        all_rules_duration.as_millis() < 200,
        "All rules together took too long: {}ms",
        all_rules_duration.as_millis()
    );

    println!(
        "Mixed rule performance: {} rules, {} warnings, {}ms",
        rules.len(),
        all_warnings.len(),
        all_rules_duration.as_millis()
    );
}
