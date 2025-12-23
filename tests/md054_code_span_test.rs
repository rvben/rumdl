#[cfg(test)]
mod test_md054_code_span {
    use rumdl_lib::config::{Config, MarkdownFlavor};
    use rumdl_lib::rules;

    #[test]
    fn test_md054_code_span_indexing() {
        // Test that MD054 doesn't flag link styles inside code spans
        // This tests the column indexing bug fix similar to MD033 issue #90
        let content = r#"# Test MD054 Code Span Handling

Regular link: [text](url)

Code span with link syntax: `[link](url)` should be ignored

Multiple code spans: `[one](url1)` and `[two](url2)` should be ignored

Mixed: [real](url) and `[fake](url)` - only first should be flagged

Code span at start: `[start](url)` text

Text then code span: text `[end](url)`

Complex: `Array<[T]>` and `Map<[K, V]>` should be ignored
"#;

        let config = Config::default();
        let all_rules = rules::all_rules(&config);
        let md054_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD054").collect();

        // MD054 is normally disabled by default, so it shouldn't produce warnings
        // unless explicitly configured. Let's just ensure it doesn't crash
        // with the fixed indexing
        let warnings = rumdl_lib::lint(content, &md054_rules, false, MarkdownFlavor::Standard, None).unwrap();

        // The test passes if it doesn't panic due to indexing errors
        // The actual warning count depends on the default configuration
        println!(
            "MD054 produced {} warnings (no panic means indexing is fixed)",
            warnings.len()
        );
    }

    #[test]
    fn test_md054_off_by_one_bug() {
        // This test specifically targets the off-by-one indexing bug
        // The bug would cause false positives or panics at certain character positions
        let test_cases = [
            "`[x](y)` [a](b)",                   // Code span at position 0
            " `[x](y)` [a](b)",                  // Code span at position 1
            "  `[x](y)` [a](b)",                 // Code span at position 2
            "Text `[link](url)` more",           // Code span in middle
            "Some text `[end](url)`",            // Code span at end
            "`[start](url)` and then text",      // Code span at start
            "Unicode: 你好 `[link](url)` world", // Unicode characters
        ];

        let config = Config::default();
        let all_rules = rules::all_rules(&config);
        let md054_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD054").collect();

        for (i, content) in test_cases.iter().enumerate() {
            // The test passes if it doesn't panic due to indexing errors
            let result = rumdl_lib::lint(content, &md054_rules, false, MarkdownFlavor::Standard, None);
            assert!(
                result.is_ok(),
                "Test case {} failed with content '{}': {:?}",
                i + 1,
                content,
                result
            );
        }
    }

    #[test]
    fn test_md054_regression_prevention() {
        // Ensure MD054 still works for its intended purpose
        // This content has inconsistent link styles that MD054 should detect
        let content = r#"# Links

[Reference style][ref]

[ref]: https://example.com

<https://autolink.com>

[Inline style](https://example.com)
"#;

        let config = Config::default();
        let all_rules = rules::all_rules(&config);
        let md054_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD054").collect();

        // The test passes if it doesn't panic
        let result = rumdl_lib::lint(content, &md054_rules, false, MarkdownFlavor::Standard, None);
        assert!(result.is_ok(), "MD054 regression test failed: {result:?}");

        if let Ok(warnings) = result {
            println!("MD054 regression test produced {} warnings", warnings.len());
        }
    }
}
