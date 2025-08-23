use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;
use std::time::Instant;

#[test]
fn test_extremely_long_lines() {
    // Test with extremely long lines that could cause buffer overflows
    let long_line = "a".repeat(100000);
    let content = format!("# Heading\n{long_line}\n## Another heading");

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD013LineLength::new(80, true, true, true, false)),
        Box::new(MD022BlanksAroundHeadings::new()),
    ];

    for rule in &rules {
        let ctx = LintContext::new(&content);

        // Should not panic or take excessive time
        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        assert!(
            check_duration.as_secs() < 5,
            "Rule {} took too long on extremely long line: {}s",
            rule.name(),
            check_duration.as_secs()
        );

        // Test CLI/LSP consistency even with extreme content
        if !warnings.is_empty() {
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(&content, &warnings).expect("LSP fix should succeed");

            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Rule {} produced different CLI vs LSP results with extremely long lines",
                rule.name()
            );
        }
    }
}

#[test]
fn test_deeply_nested_blockquotes() {
    // Test with deeply nested blockquotes that could cause stack overflow
    let mut content = String::new();
    for depth in 1..=100 {
        content.push_str(&format!("{} Level {} blockquote\n", ">".repeat(depth), depth));
    }

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD027MultipleSpacesBlockquote),
        Box::new(MD028NoBlanksBlockquote),
    ];

    for rule in &rules {
        let ctx = LintContext::new(&content);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        assert!(
            check_duration.as_millis() < 1000,
            "Rule {} took too long on deeply nested blockquotes: {}ms",
            rule.name(),
            check_duration.as_millis()
        );

        // Test consistency
        if !warnings.is_empty() {
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(&content, &warnings).expect("LSP fix should succeed");

            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Rule {} produced different CLI vs LSP results with deeply nested blockquotes",
                rule.name()
            );
        }
    }
}

#[test]
fn test_malformed_markdown_edge_cases() {
    let nested_lists = (0..50)
        .map(|i| format!("{}- Item level {}", "  ".repeat(i), i))
        .collect::<Vec<_>>()
        .join("\n");
    let consecutive_headings = (1..=50)
        .map(|i| format!("#{} Heading {}", "#".repeat(i % 6 + 1), i))
        .collect::<Vec<_>>()
        .join("\n");

    let test_cases = [
        // Unclosed code blocks
        "```rust\ncode without closing fence\n# Heading after unclosed fence",
        // Malformed links
        "[unclosed link text\n## Heading\n[another [nested [link]]] structure",
        // Mixed line endings (simulate different platforms)
        "# Heading 1\r\nContent with CRLF\n# Heading 2\nContent with LF\r\n# Heading 3",
        // Invalid UTF-8 sequences (using valid UTF-8 that represents edge cases)
        "# Heading with emoji 🚀 and unicode \u{1F600}\nContent with special chars: ±×÷√∞",
        // Extremely nested lists
        &nested_lists,
        // Many consecutive headings
        &consecutive_headings,
        // Mixed markdown syntax
        "# Heading\n> Blockquote\n```\ncode\n# heading in code?\n```\n- List item\n  > nested blockquote\n    ```\n    nested code\n    ```",
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD040FencedCodeLanguage),
    ];

    for (i, test_content) in test_cases.iter().enumerate() {
        for rule in &rules {
            let ctx = LintContext::new(test_content);

            // Should not panic
            let start_time = Instant::now();
            let result = rule.check(&ctx);
            let check_duration = start_time.elapsed();

            assert!(
                result.is_ok(),
                "Rule {} panicked on test case {}: {}",
                rule.name(),
                i,
                test_content.chars().take(50).collect::<String>()
            );

            assert!(
                check_duration.as_millis() < 1000,
                "Rule {} took too long on test case {}: {}ms",
                rule.name(),
                i,
                check_duration.as_millis()
            );

            let warnings = result.unwrap();

            // Test CLI/LSP consistency on malformed content
            if !warnings.is_empty() {
                let cli_result = rule.fix(&ctx);
                let lsp_result = apply_warning_fixes(test_content, &warnings);

                // Both should succeed or both should fail
                match (cli_result, lsp_result) {
                    (Ok(cli_fixed), Ok(lsp_fixed)) => {
                        assert_eq!(
                            cli_fixed,
                            lsp_fixed,
                            "Rule {} produced different CLI vs LSP results on test case {}",
                            rule.name(),
                            i
                        );
                    }
                    (Err(_), Err(_)) => {
                        // Both failed - that's acceptable for malformed content
                    }
                    _ => {
                        panic!(
                            "Rule {} had inconsistent fix behavior on test case {} - one succeeded, one failed",
                            rule.name(),
                            i
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_unicode_boundary_conditions() {
    let test_cases = vec![
        // Multi-byte UTF-8 characters at various positions
        "# 🚀 Heading with emoji at start",
        "# Heading with emoji at end 🚀",
        "# Héading with áccents",
        "# 中文标题",
        "# Заголовок на русском",
        "# العنوان بالعربية",
        // Unicode in different contexts
        "```\n// 代码中的中文注释\nlet 变量 = \"值\";\n```\n\n# 代码后的标题",
        "> 引用中的中文内容\n\n# 引用后的标题",
        "- 列表项目中的中文\n\n# 列表后的标题",
        // Edge case: Zero-width characters
        "# Heading\u{200B}with\u{200C}zero\u{200D}width\u{FEFF}chars",
        // Combining characters
        "# Café with combining accent: Cafe\u{0301}",
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD013LineLength::new(80, true, true, true, false)),
    ];

    for test_content in &test_cases {
        for rule in &rules {
            let ctx = LintContext::new(test_content);

            // Should handle Unicode correctly without panicking
            let warnings = rule.check(&ctx).expect("Rule should handle Unicode content");

            // Test that fixes preserve Unicode correctly
            if !warnings.is_empty() {
                let cli_fixed = rule.fix(&ctx).expect("CLI fix should preserve Unicode");
                let lsp_fixed = apply_warning_fixes(test_content, &warnings).expect("LSP fix should preserve Unicode");

                assert_eq!(
                    cli_fixed,
                    lsp_fixed,
                    "Rule {} produced different CLI vs LSP results with Unicode content: {}",
                    rule.name(),
                    test_content
                );

                // Verify that the fixed content is still valid UTF-8
                assert!(
                    cli_fixed.is_ascii() || cli_fixed.chars().all(|c| c != '\u{FFFD}'),
                    "Fixed content should not contain replacement characters"
                );
            }
        }
    }
}

#[test]
fn test_memory_intensive_scenarios() {
    // Test scenarios that could cause excessive memory usage
    let scenarios = [
        // Many small allocations
        (0..10000).map(|i| format!("Line {i}")).collect::<Vec<_>>().join("\n"),
        // Large single allocation
        format!("# Large Content\n{}", "content ".repeat(50000)),
        // Many rules on same content
        "# Test\n\nContent with trailing spaces   \n[bad link]()\n```\ncode\n```\n\n## Another\n\nMore content"
            .to_string(),
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD040FencedCodeLanguage),
        Box::new(MD042NoEmptyLinks),
    ];

    for (i, content) in scenarios.iter().enumerate() {
        let ctx = LintContext::new(content);

        // Collect all warnings from all rules
        let mut all_warnings = Vec::new();
        for rule in &rules {
            let start_time = Instant::now();
            let warnings = rule.check(&ctx).expect("Rule check should succeed");
            let check_duration = start_time.elapsed();

            assert!(
                check_duration.as_secs() < 10,
                "Rule {} took too long on memory scenario {}: {}s",
                rule.name(),
                i,
                check_duration.as_secs()
            );

            all_warnings.extend(warnings);
        }

        // Test bulk LSP fix performance
        if !all_warnings.is_empty() {
            let start_time = Instant::now();
            let lsp_fixed = apply_warning_fixes(content, &all_warnings).expect("Bulk LSP fix should succeed");
            let lsp_duration = start_time.elapsed();

            assert!(
                lsp_duration.as_secs() < 10,
                "Bulk LSP fix took too long on memory scenario {}: {}s",
                i,
                lsp_duration.as_secs()
            );

            // Verify result is not empty (unless original was empty)
            assert!(
                !lsp_fixed.is_empty() || content.trim().is_empty(),
                "LSP fix should produce valid output"
            );
        }
    }
}

#[test]
fn test_pathological_regex_patterns() {
    // Test content that could cause regex catastrophic backtracking
    let test_cases = [
        // Many repeated characters
        format!("# {}", "a".repeat(1000)),
        // Nested patterns
        "([([([([text])])])])".to_string(),
        // Many alternatives
        format!("[{}](url)", "link|".repeat(100)),
        // Complex nested structures
        "```\n".repeat(100) + &"text\n".repeat(100) + &"```\n".repeat(100),
        // Mixed quote and code patterns
        "> ".repeat(50) + &"```\n".repeat(25) + "content\n" + &"```\n".repeat(25),
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD034NoBareUrls),
        Box::new(MD039NoSpaceInLinks),
        Box::new(MD038NoSpaceInCode::default()),
        Box::new(MD040FencedCodeLanguage),
    ];

    for (i, test_content) in test_cases.iter().enumerate() {
        for rule in &rules {
            let ctx = LintContext::new(test_content);

            // Should complete within reasonable time (regex should not hang)
            let start_time = Instant::now();
            let result = rule.check(&ctx);
            let check_duration = start_time.elapsed();

            assert!(
                check_duration.as_secs() < 5,
                "Rule {} took too long on regex test case {}: {}s (possible catastrophic backtracking)",
                rule.name(),
                i,
                check_duration.as_secs()
            );

            assert!(
                result.is_ok(),
                "Rule {} should handle pathological regex patterns without error",
                rule.name()
            );
        }
    }
}
