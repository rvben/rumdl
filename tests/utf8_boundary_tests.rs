use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;

#[test]
fn test_issue_85_german_umlauts_in_lists() {
    // This is the exact content that triggered issue #85
    let content = r#"---
title: Backup
tags:
- Backup
---

# Backup

## Backuparten

- **Voll-Backup**
    Ein Voll-Backup ist eine vollständige Sicherung aller Daten. Es ist die Grundlage für andere Backuparten und ermöglicht eine schnelle Wiederherstellung.

- **Inkrementelles Backup**
    Sichert nur die Änderungen seit dem letzten Backup. Dies spart Speicherplatz und Zeit.

- **Differenzielles Backup**
    Sichert alle Änderungen seit dem letzten Voll-Backup. Größere Datenmenge als inkrementell.

## Überprüfung

Die Überprüfung der Backupintegrität ist wichtig für die Zuverlässigkeit.

## Spezielle Zeichen

Teste äöü ÄÖÜ ß in verschiedenen Kontexten."#;

    // This should not panic when checking if list items are in code blocks
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Run various rules that might interact with list detection and code blocks
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD005ListIndent::default()),
        Box::new(MD007ULIndent::default()),
        Box::new(MD032BlanksAroundLists::default()),
    ];

    for rule in &rules {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.check(&ctx)));

        assert!(
            result.is_ok(),
            "Rule {} panicked with German umlaut content",
            rule.name()
        );

        // Also verify the check actually succeeded
        let warnings = result.unwrap();
        assert!(
            warnings.is_ok(),
            "Rule {} failed to check German umlaut content: {:?}",
            rule.name(),
            warnings.err()
        );
    }
}

#[test]
fn test_umlauts_in_code_spans_within_lists() {
    let content = r#"# Test Document

- List item with `code containing ä ö ü` inline
- Another item with `Überprüfung` in code
- Item with multiple `äöü` and `ÄÖÜ` code spans
- Regular text: Die Größe der Änderungen überprüfen"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test MD038 which checks for spaces in code spans
    let rule = MD038NoSpaceInCode::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.check(&ctx)));

    assert!(result.is_ok(), "MD038 panicked with umlauts in code spans");
}

#[test]
fn test_umlauts_near_code_block_boundaries() {
    let content = r#"# Document

Text with ä right before code block:

```
code block
```

Text with ü right after code block.

- List with ö before code:
    ```
    indented code
    ```
- Item with ß after code"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test rules that analyze code blocks and lists
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD046CodeBlockStyle::from_config_struct(Default::default())),
    ];

    for rule in &rules {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.check(&ctx)));

        assert!(
            result.is_ok(),
            "Rule {} panicked with umlauts near code blocks",
            rule.name()
        );
    }
}

#[test]
fn test_multibyte_characters_at_slice_boundaries() {
    // Test various multi-byte UTF-8 characters that could cause boundary issues
    let test_cases = vec![
        ("List with emoji 🚀 in text", "emoji"),
        ("Chinese 中文 characters", "Chinese"),
        ("Japanese かな and 漢字", "Japanese"),
        ("Korean 한글 text", "Korean"),
        ("Russian Привет мир", "Cyrillic"),
        ("Greek Γεια σου κόσμε", "Greek"),
        ("Hebrew שלום עולם", "Hebrew"),
        ("Arabic مرحبا بالعالم", "Arabic"),
    ];

    for (content, description) in test_cases {
        let list_content = format!("- {content}\n- Another item");
        let ctx = LintContext::new(&list_content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Test list-related rules
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD004UnorderedListStyle::default()),
            Box::new(MD005ListIndent::default()),
            Box::new(MD007ULIndent::default()),
        ];

        for rule in &rules {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.check(&ctx)));

            assert!(
                result.is_ok(),
                "Rule {} panicked with {} characters",
                rule.name(),
                description
            );
        }
    }
}

#[test]
fn test_code_block_detection_with_utf8_boundaries() {
    // Specifically test the code block detection logic with UTF-8 characters
    let content = r#"# Test

- Item with ä
    ```
    code block
    ```
- Item with ö"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test that we can check rules without panicking
    // This triggers the internal list parsing and code block detection
    let rule = MD032BlanksAroundLists::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.check(&ctx)));

    // This should not panic
    assert!(
        result.is_ok(),
        "Should not panic when detecting lists with UTF-8 characters near code blocks"
    );

    // Verify the check actually succeeded
    let warnings = result.unwrap();
    assert!(
        warnings.is_ok(),
        "Should successfully check lists with UTF-8 characters"
    );
}

#[test]
fn test_escaped_backticks_with_umlauts() {
    // Combine the two issues we fixed: escaped backticks and UTF-8 boundaries
    let content = r#"# Test

- Item with \`escaped backtick\` and ä
- Item with `regular code` and ö
- Item with \`escaped\` and ü mixed"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test MD038 which was reporting false positives
    let rule = MD038NoSpaceInCode::default();
    let result = rule.check(&ctx);

    assert!(result.is_ok(), "MD038 should handle escaped backticks with umlauts");

    let warnings = result.unwrap();

    // Should only detect the real code span in line 2, not the escaped ones
    assert!(
        warnings.is_empty() || warnings.iter().all(|w| w.line == 4),
        "MD038 should not flag escaped backticks as code spans"
    );
}
