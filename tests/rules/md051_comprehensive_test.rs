// Comprehensive test suite for MD051 rule to address gaps identified in issue 39
// This test suite covers all the edge cases and missing scenarios that would have
// caught the bugs before they were released.

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;

/// Helper function to assert that fragments are generated correctly
fn assert_fragments(test_cases: &[(&str, &str)]) {
    let rule = MD051LinkFragments::new();

    for (heading, expected_fragment) in test_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Fragment generation failed for heading '{}': expected '{}' but link was flagged as broken. Warnings: {:?}",
            heading,
            expected_fragment,
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

// Removed unused assert_fragment_variations function

#[test]
fn test_punctuation_removal_order() {
    // Test official GitHub/kramdown behavior for punctuation handling
    // Fixed: All official tools generate "--" for ampersands, not single "-"
    assert_fragments(&[
        ("A & B", "a--b"),                 // ampersand becomes "--", space becomes "-"
        ("A --> B", "a----b"),             // "-->" becomes "----", spaces absorbed
        ("A: B", "a-b"),                   // colon removed, space becomes hyphen
        ("A (B) C", "a-b-c"),              // parens removed, spaces become hyphens
        ("A!!! B", "a-b"),                 // multiple punctuation removed
        ("Pre -> Post", "pre---post"),     // "->": becomes "---"
        ("Before: After", "before-after"), // simple colon
    ]);
}

#[test]
fn test_consecutive_punctuation_handling() {
    // These are the specific failing cases from issue 39
    // Current algorithm generates wrong fragments with multiple hyphens
    // Fixed: GitHub actually generates "cbrown----sbrown---unsafe-paths"
    // Verified against GitHub gist behavior
    assert_fragments(&[("cbrown --> sbrown: --unsafe-paths", "cbrown----sbrown---unsafe-paths")]);

    // Fixed: GitHub actually generates "cbrown---sbrown"
    // Verified against official behavior
    assert_fragments(&[("cbrown -> sbrown", "cbrown---sbrown")]);

    // Additional complex punctuation patterns - fixed with actual GitHub behavior
    assert_fragments(&[
        ("API!!! Methods??? & Properties", "api-methods--properties"), // & becomes --
        ("Step 1: (Optional) Setup", "step-1-optional-setup"),
        ("Testing & Coverage & More", "testing--coverage--more"), // & becomes --
        ("One -> Two -> Three", "one---two---three"),             // -> becomes ---
        ("A: B: C", "a-b-c"),
    ]);
}

#[test]
fn test_ampersand_handling_variations() {
    // Test official GitHub ampersand handling behavior
    // Ampersand with spaces becomes "--", without spaces it's removed
    assert_fragments(&[
        ("Testing & Coverage", "testing--coverage"),   // With spaces: & becomes --
        ("A&B", "ab"),                                 // No spaces: & is removed
        ("A & B & C", "a--b--c"),                      // Multiple ampersands with spaces
        ("Testing&Development", "testingdevelopment"), // Adjacent ampersand is removed
        ("API & Documentation", "api--documentation"), // Common pattern with spaces
    ]);

    // Remove the variations test since official behavior is consistent
    // All official tools (GitHub, kramdown GFM, pure kramdown) generate "testing--coverage"
}

#[test]
fn test_complex_punctuation_clusters() {
    // Test combinations of different punctuation types - fixed with GitHub behavior
    assert_fragments(&[
        ("Title: (Part 1) - Overview", "title-part-1---overview"), // Space + hyphen + space → ---
        ("FAQ??? What's New!!!", "faq-whats-new"),
        ("Step 1: Setup (Required)", "step-1-setup-required"),
        (
            "API Reference: Methods & Properties",
            "api-reference-methods--properties", // & becomes --
        ),
        ("Version 2.0: New Features & Fixes", "version-20-new-features--fixes"), // & becomes --
        ("Install: (macOS) & (Windows)", "install-macos--windows"),              // & becomes --
    ]);
}

#[test]
fn test_special_character_edge_cases() {
    // Test punctuation characters that might be handled inconsistently - fixed with GitHub behavior
    assert_fragments(&[
        ("Quote \"Test\" Unquote", "quote-test-unquote"),
        ("Em—Dash & En–Dash", "emdash--endash"), // & becomes --, em/en dashes are removed entirely
        ("Symbols: @#$%^&*()", "symbols-"),      // All symbols removed, just one hyphen from colon space
        ("Math: x + y = z", "math-x--y--z"),     // spaces become hyphens, + becomes hyphen
        ("Code: foo(bar)", "code-foobar"),
        ("File.ext: Details", "fileext-details"),
    ]);
}

#[test]
fn test_unicode_punctuation_edge_cases() {
    // Test Unicode punctuation that might be missed by ASCII-only filters
    assert_fragments(&[
        ("Smart \"Quotes\" Test", "smart-quotes-test"),
        ("List • Item • Two", "list--item--two"),       // Bullet becomes --
        ("Range … Ellipsis", "range--ellipsis"),        // Ellipsis becomes --
        ("Math ÷ Division", "math--division"),          // Division sign becomes --
        ("Degree 90° Angle", "degree-90-angle"),        // Degree symbol removed
        ("Currency $100€ Price", "currency-100-price"), // Currency symbols removed
    ]);
}

#[test]
fn test_mixed_script_edge_cases() {
    // Test mixed scripts with punctuation - fixed with actual GitHub behavior
    assert_fragments(&[
        ("English & 中文", "english--中文"),       // & becomes --
        ("Café & Restaurant", "café--restaurant"), // & becomes --
        ("Naïve & Smart", "naïve--smart"),         // & becomes --
        ("Русский & English", "русский--english"), // & becomes --
        ("العربية & English", "العربية--english"), // & becomes --
    ]);
}

#[test]
fn test_whitespace_punctuation_interaction() {
    // Test edge cases where punctuation is adjacent to various whitespace - fixed with GitHub behavior
    assert_fragments(&[
        ("A  &  B", "a--b"),                        // Multiple spaces around ampersand: & becomes --
        ("Trailing & ", "trailing-"),               // Trailing ampersand with space: becomes single -
        (" & Leading", "--leading"),                // Leading space before punctuation: space+& becomes --
        ("Multiple   Spaces", "multiple---spaces"), // Multiple spaces preserved as multiple hyphens
    ]);

    // These test cases with tabs/newlines need special handling since they create invalid headings
    // Tabs and newlines can't appear in markdown headings literally
}

#[test]
fn test_edge_case_heading_structures() {
    // Test headings that might confuse the parser
    assert_fragments(&[
        ("Hash in Heading", "hash-in-heading"),
        ("Heading with [brackets]", "heading-with-brackets"),
        ("*Emphasis* in Heading", "emphasis-in-heading"),
        ("`Code` in Heading", "code-in-heading"),
        ("**Bold** & *Italic*", "bold--italic"),
        ("Link [text](url) stripped", "link-text-stripped"),
    ]);
}

#[test]
fn test_kramdown_vs_github_differences() {
    use rumdl_lib::utils::anchor_styles::AnchorStyle;

    // Test cases where Kramdown and GitHub modes should differ
    let github_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::GitHub);
    let kramdown_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);
    let kramdown_gfm_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::KramdownGfm);

    // Test cases where different anchor styles should behave differently
    // Based on verified behavior from official tools
    let test_cases = vec![
        ("test_method", "test_method", "test_method", "testmethod"), // GitHub/GFM preserve _, pure kramdown removes _
        ("Café Menu", "café-menu", "café-menu", "caf-menu"),         // Pure kramdown removes accents, others preserve
        ("über_cool", "über_cool", "über_cool", "bercool"), // Pure kramdown removes accents & _, others preserve _
        ("naïve_approach", "naïve_approach", "naïve_approach", "naveapproach"), // Pure kramdown removes accents & _
    ];

    for (heading, github_expected, kramdown_gfm_expected, kramdown_expected) in test_cases {
        // Test GitHub mode (preserves underscores)
        let content = format!("# {heading}\n\n[Link](#{github_expected})");
        let ctx = LintContext::new(&content);
        let result = github_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "GitHub mode failed for '{heading}' -> '{github_expected}'"
        );

        // Test Kramdown GFM mode (preserves underscores)
        let content = format!("# {heading}\n\n[Link](#{kramdown_gfm_expected})");
        let ctx = LintContext::new(&content);
        let result = kramdown_gfm_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Kramdown GFM mode failed for '{heading}' -> '{kramdown_gfm_expected}'"
        );

        // Test Pure Kramdown mode (removes underscores)
        let content = format!("# {heading}\n\n[Link](#{kramdown_expected})");
        let ctx = LintContext::new(&content);
        let result = kramdown_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Pure Kramdown mode failed for '{heading}' -> '{kramdown_expected}'"
        );
    }
}

#[test]
fn test_liquid_template_complex_patterns() {
    // Test advanced Liquid patterns that might confuse the cross-file detection
    let rule = MD051LinkFragments::new();

    let content = r#"# Real Heading

## Another Section

These Liquid patterns should be ignored:
[Complex post]({% post_url 2023-03-25-post param="value" %}#section)
[Variable include]({% include file.html var=site.data %}#anchor)
[Nested tags]({% assign x = "val" %}{% link {{ x }}.md %}#frag)
[Variable path]({{ site.url }}/{{ page.slug }}#section)
[Complex variable]({{ site.posts | where: "slug", "test" | first | url }}#anchor)

Valid internal links:
[Should work](#real-heading)
[Should also work](#another-section)
[Should fail](#missing-section)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the missing internal link
    assert_eq!(result.len(), 1, "Should only warn about missing-section");
    assert!(result[0].message.contains("missing-section"));
}

#[test]
fn test_cross_file_detection_edge_cases() {
    // Test edge cases in cross-file link detection that might cause false positives
    let rule = MD051LinkFragments::new();

    let content = r#"# Main Heading

Cross-file patterns (should be ignored):
[Complex path](./docs/api/methods.md#get-user)
[Query params](file.md?version=1.0&format=json#section)
[Encoded chars](file%20name.md#section%20name)
[Multiple dots](config.local.dev.yaml#database-config)
[Archive file](backup.tar.gz#file-list)
[Hidden file](.eslintrc.js#rules)
[Network path](//server.com/file.md#section)
[Absolute path](/usr/local/docs/manual.md#install)

Ambiguous patterns (might be fragment-only):
[No extension](somefile#section)
[Dot only](file.#section)
[Hidden no ext](.hidden#section)

Valid internal:
[Works](#main-heading)
[Fails](#missing)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag ambiguous patterns + missing internal = 4 warnings
    // (3 ambiguous + 1 missing)
    assert_eq!(result.len(), 4, "Expected 4 warnings for ambiguous + missing");

    // Verify one is about missing
    assert!(result.iter().any(|w| w.message.contains("missing")));
}

#[test]
fn test_performance_with_complex_patterns() {
    // Ensure complex patterns don't cause performance regression
    let mut content = String::from("# Performance Test\n\n");

    // Add many complex headings
    let complex_headings = [
        "Complex: (Pattern) -> Result & More!!!",
        "API Reference: Methods & Properties",
        "Step 1: Setup (Required) & Configuration",
        "FAQ??? What's New & Updated",
        "Testing & Coverage & Documentation",
        "Unicode: Café & 中文 & Русский",
        "Punctuation: @#$%^&*() Symbols",
        "Arrows: -> <- <-> <=> --> <--",
        "Multiple!!! Exclamations??? & Questions",
        "Mixed: A -> B: C & D (E) F",
    ];

    for (i, heading) in complex_headings.iter().enumerate() {
        content.push_str(&format!("## {heading}\n\n"));
        content.push_str("Some content here.\n\n");

        // Add links to some headings
        if i % 2 == 0 {
            let simple_fragment = heading
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
                .replace(' ', "-");
            content.push_str(&format!("[Link](#{simple_fragment})\n\n"));
        }
    }

    let start = std::time::Instant::now();
    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(&content);
    let _result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Performance should remain reasonable even with complex patterns
    assert!(
        duration.as_millis() < 50,
        "Performance regression: took {}ms",
        duration.as_millis()
    );
}

#[test]
fn test_regression_prevention_issue_39() {
    // Specific regression test for the exact patterns from issue 39
    // These MUST work to prevent the bug from reoccurring
    let rule = MD051LinkFragments::new();

    let issue_39_cases = vec![
        // Cases with corrected GitHub behavior
        ("Testing & Coverage", "testing--coverage"), // & becomes --
        (
            "API Reference: Methods & Properties",
            "api-reference-methods--properties", // & becomes --
        ),
        // These are the patterns that actually work with GitHub
    ];

    for (heading, expected_fragment) in issue_39_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();

        if result.is_empty() {
            println!("✓ Issue 39 case PASSED: '{heading}' -> '{expected_fragment}'");
        } else {
            println!("⚠ Issue 39 case NEEDS FIX: '{heading}' -> '{expected_fragment}'");
            println!(
                "  Current behavior produces warnings: {:?}",
                result.iter().map(|w| &w.message).collect::<Vec<_>>()
            );
        }
    }

    // Ensure the corrected cases work with actual GitHub behavior
    assert_fragments(&[
        ("Testing & Coverage", "testing--coverage"), // Corrected: & becomes --
        (
            "API Reference: Methods & Properties",
            "api-reference-methods--properties", // Corrected: & becomes --
        ),
    ]);
}

#[test]
fn test_boundary_conditions() {
    // Test boundary conditions that might cause edge case failures
    assert_fragments(&[
        ("", ""),       // Empty heading (should generate empty fragment)
        ("   ", ""),    // Whitespace only
        ("!!!", ""),    // Punctuation only
        ("123", "123"), // Numbers only
        ("_", "_"),     // Single underscore
        ("-", ""),      // Single hyphen (should be trimmed)
        ("a", "a"),     // Single character
        ("A", "a"),     // Single uppercase character
    ]);
}

#[test]
fn test_markdown_formatting_in_headings() {
    // Test that markdown formatting is properly stripped from headings
    assert_fragments(&[
        ("**Bold** Text", "bold-text"),
        ("*Italic* Text", "italic-text"),
        ("`Code` Text", "code-text"),
        ("~~Strikethrough~~ Text", "strikethrough-text"),
        ("***Bold Italic*** Text", "bold-italic-text"),
        ("**_Mixed_** Formatting", "mixed-formatting"),
        ("[Link](url) Text", "link-text"),
        ("![Image](url) Text", "image-text"),
    ]);
}

#[test]
fn test_zero_width_and_control_characters() {
    // Test handling of zero-width and control characters
    assert_fragments(&[
        ("Zero\u{200B}Width", "zerowidth"),   // Zero-width space
        ("Soft\u{00AD}Hyphen", "softhyphen"), // Soft hyphen
        ("Word\u{2060}Joiner", "wordjoiner"), // Word joiner
    ]);

    // Note: Tab and newline characters can't appear literally in markdown headings
    // They would break the heading syntax
}

#[test]
fn test_duplicate_heading_numbering() {
    // Test that duplicate headings get numbered correctly
    let rule = MD051LinkFragments::new();

    let content = r#"# Duplicate

## Section

## Section

## Section

[Link 1](#duplicate)
[Link 2](#section)
[Link 3](#section-1)
[Link 4](#section-2)
[Invalid](#section-3)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid link (section-3 doesn't exist)
    assert_eq!(result.len(), 1, "Should only warn about section-3");
    assert!(result[0].message.contains("section-3"));
}

#[test]
fn test_custom_header_id_edge_cases() {
    // Test edge cases with custom header IDs
    let rule = MD051LinkFragments::new();

    let content = r#"# Normal Heading {#custom-id}

## Another {#with-hyphens-in-id}

### Third {:#colon-style}

#### Fourth {: #spaced-colon }

[Link 1](#custom-id)
[Link 2](#with-hyphens-in-id)
[Link 3](#colon-style)
[Link 4](#spaced-colon)
[Link 5](#normal-heading)
[Invalid](#missing-id)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid link
    assert_eq!(result.len(), 1, "Should only warn about missing-id");
    assert!(result[0].message.contains("missing-id"));
}
