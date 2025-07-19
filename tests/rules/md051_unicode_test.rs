use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD051LinkFragments;

#[test]
fn test_md051_unicode_headings() {
    let rule = MD051LinkFragments::new();

    // Test various Unicode scenarios
    let test_cases = vec![
        // Basic Unicode
        (
            "## Café Menu\n\n[Link to café](#café-menu)",
            0, // Should not flag - correct fragment
        ),
        (
            "## Café Menu\n\n[Link to café](#cafe-menu)",
            1, // Should flag - missing accent
        ),
        // Chinese/Japanese characters
        (
            "## 日本語 Heading\n\n[Link](#日本語-heading)",
            0, // Should not flag
        ),
        (
            "## 日本語 Heading\n\n[Link](#heading)",
            1, // Should flag - missing Unicode part
        ),
        // Spanish with ñ
        (
            "## Español con Ñ\n\n[Link](#español-con-ñ)",
            0, // Should not flag
        ),
        (
            "## Español con Ñ\n\n[Link](#espanol-con-n)",
            1, // Should flag - missing tildes
        ),
        // Emojis (GitHub strips emojis from fragments)
        (
            "## Emoji 🎉 Party\n\n[Link](#emoji-party)",
            0, // Should not flag - emojis are stripped
        ),
        (
            "## Emoji 🎉 Party\n\n[Link](#emoji-🎉-party)",
            1, // Should flag - emojis should not be in fragment
        ),
        // Mixed Unicode
        (
            "## Über café 北京\n\n[Link](#über-café-北京)",
            0, // Should not flag
        ),
        // Unicode normalization test
        (
            "## Café\n\n[Link](#café)", // é as single char (U+00E9)
            0,                          // Should not flag
        ),
    ];

    for (content, expected_warnings) in test_cases {
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            expected_warnings,
            "Content: {}\nExpected {} warnings, got {}",
            content,
            expected_warnings,
            warnings.len()
        );
    }
}

#[test]
fn test_md051_fragment_generation() {
    let rule = MD051LinkFragments::new();

    // Test the heading_to_fragment_fast method directly
    let test_cases = vec![
        ("Café Menu ☕", "café-menu"),
        ("日本語 (Japanese)", "日本語-japanese"),
        ("Español con Ñ", "español-con-ñ"),
        ("Emoji 🎉 Party", "emoji-party"),
        ("Mixed CASE with УниКОД", "mixed-case-with-уникод"),
        ("Multiple   Spaces", "multiple-spaces"),
        ("Über café 北京", "über-café-北京"),
        ("Special & Characters", "special--characters"), // & becomes --
        ("!!!Leading and Trailing!!!", "leading-and-trailing"),
    ];

    // Note: We can't test the private method directly, so we'll test via the rule behavior
    for (heading, expected_fragment) in test_cases {
        let content = format!("## {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "Heading '{heading}' should generate fragment '{expected_fragment}', but link was flagged as broken"
        );
    }
}

#[test]
fn test_md051_unicode_edge_cases() {
    let rule = MD051LinkFragments::new();

    // Test more realistic Unicode edge cases
    let content = r#"
## Right-to-left العربية
## Mathematical 𝕳𝖊𝖑𝖑𝖔
## Accented Naïveté
## Mixed 中文 English

[Link 1](#right-to-left-العربية)
[Link 2](#mathematical-𝕳𝖊𝖑𝖑𝖔)
[Link 3](#accented-naïveté)
[Link 4](#mixed-中文-english)
"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // All should work correctly with Unicode preserved
    assert_eq!(warnings.len(), 0, "Unicode edge cases should be handled correctly");
}

#[test]
fn test_md051_complex_unicode_edge_cases() {
    let rule = MD051LinkFragments::new();

    // These are known limitations - zero-width spaces and combining diacritics
    // are complex Unicode features that may not be fully supported
    let content = r#"
## Zero-width\u{200B}space
## Combining diacritics e̊

[Link 1](#zero-widthspace)
[Link 2](#combining-diacritics-e)
"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Only zero-width space doesn't match - combining diacritics work
    assert_eq!(warnings.len(), 1, "Zero-width spaces are not handled correctly");
}
