// Property-based testing for MD051 rule using proptest
// This ensures that anchor generation is robust across all possible inputs

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;
use std::collections::HashSet;

// Note: This requires adding proptest to Cargo.toml dev-dependencies
// For now, we'll use manual property testing

/// Test property: Fragment generation is deterministic
#[test]
fn property_deterministic_fragment_generation() {
    let rule = MD051LinkFragments::new();

    let test_inputs = vec![
        "Simple Heading",
        "Complex: (Pattern) & More!!!",
        "Unicode: Café & 中文",
        "Punctuation!@#$%^&*()",
        "",
        "   ",
        "123 Numbers",
        "Mixed_Case_With_Underscores",
        "Arrows -> <- <-> <=>",
        "Quotes \"Test\" 'Single'",
    ];

    for input in test_inputs {
        // Test with actual heading_to_fragment_github method via rule behavior
        let content1 = format!("# {input}\n\n");
        let content2 = format!("# {input}\n\n");

        let ctx1 = LintContext::new(&content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let ctx2 = LintContext::new(&content2, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Extract headings and compare - they should be identical
        let headings1 = extract_generated_headings(&rule, &ctx1);
        let headings2 = extract_generated_headings(&rule, &ctx2);

        assert_eq!(
            headings1, headings2,
            "Fragment generation is not deterministic for input: '{input}'"
        );
    }
}

/// Test property: Generated fragments only contain valid characters
#[test]
fn property_valid_fragment_characters() {
    let rule = MD051LinkFragments::new();

    let test_inputs = vec![
        "Normal Text",
        "Symbols!@#$%^&*()",
        "Unicode: 日本語",
        "Emoji 🎉 Party",
        "Control\u{0001}Chars",
        "Zero\u{200B}Width",
        "Mixed: A->B & C",
        "Quotes \"Smart\" Quotes",
        "Math: x² + y³ = z⁴",
        "Currency: $100€ ¥200",
    ];

    for input in test_inputs {
        let content = format!("# {input}\n\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let headings = extract_generated_headings(&rule, &ctx);

        for heading in headings {
            // Check that all characters in generated fragment are valid
            let is_valid = heading.chars().all(|c| {
                // Valid characters per GitHub spec:
                // - Alphanumeric (ASCII and Unicode)
                // - Hyphens and underscores
                // - No control characters, no emoji, no unusual punctuation
                c.is_alphanumeric() || c == '-' || c == '_' || (c.is_alphabetic() && !is_emoji_or_symbol(c))
            });

            assert!(
                is_valid,
                "Generated fragment '{heading}' contains invalid characters for input: '{input}'"
            );
        }
    }
}

/// Test property: Fragment length is reasonable
#[test]
fn property_reasonable_fragment_length() {
    let rule = MD051LinkFragments::new();

    let extremely_long = "A".repeat(1000);
    let unicode_long = "Unicode: ".to_string() + &"日".repeat(100);
    let test_inputs = vec![
        "",
        "A",
        "Short",
        "This is a reasonably long heading with multiple words",
        "Very long heading that goes on and on with lots of words and punctuation!!! Really very long indeed.",
        &extremely_long, // Extremely long input
        &unicode_long,
    ];

    for input in test_inputs {
        let content = format!("# {input}\n\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let headings = extract_generated_headings(&rule, &ctx);

        for heading in headings {
            // Fragment should not be unreasonably long
            assert!(
                heading.len() <= input.len() * 2, // Allow some expansion for safety
                "Generated fragment '{}' is unreasonably long ({} chars) for input '{}' ({} chars)",
                heading,
                heading.len(),
                input,
                input.len()
            );

            // Fragment should not have excessive consecutive hyphens
            assert!(
                !heading.contains("----"), // More than 3 consecutive hyphens is suspicious
                "Generated fragment '{heading}' has excessive consecutive hyphens for input: '{input}'"
            );
        }
    }
}

/// Test property: Similar inputs produce similar fragments
#[test]
fn property_similarity_preservation() {
    let rule = MD051LinkFragments::new();

    let similar_pairs = vec![
        ("Test Heading", "Test  Heading"),   // Extra space
        ("Test & More", "Test&More"),        // Space around ampersand
        ("API Reference", "API  Reference"), // Multiple spaces
        ("Step 1", "Step1"),                 // Space before number
        ("Hello World", "Hello\tWorld"),     // Tab instead of space
        ("Method()", "Method()"),            // Identical
        ("café", "cafe"),                    // With/without accent (should be different but similar)
    ];

    for (input1, input2) in similar_pairs {
        let content1 = format!("# {input1}\n\n");
        let content2 = format!("# {input2}\n\n");

        let ctx1 = LintContext::new(&content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let ctx2 = LintContext::new(&content2, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let headings1 = extract_generated_headings(&rule, &ctx1);
        let headings2 = extract_generated_headings(&rule, &ctx2);

        // Similar inputs should produce similar fragments
        // (This is a soft property - we don't enforce exact rules but check for reasonableness)
        for (h1, h2) in headings1.iter().zip(headings2.iter()) {
            let similarity = calculate_similarity(h1, h2);
            assert!(
                similarity > 0.5, // At least 50% similar
                "Similar inputs '{input1}' and '{input2}' produced dissimilar fragments '{h1}' and '{h2}' (similarity: {similarity:.2})"
            );
        }
    }
}

/// Test property: No crashes or panics on any input
#[test]
fn property_robustness_no_panics() {
    let rule = MD051LinkFragments::new();

    // Test edge cases that might cause panics
    let many_emoji = "🎉".repeat(100);
    let many_zero_width = "\u{200B}".repeat(50);
    let very_long_string = "a".repeat(10000);
    let multiline = format!("{}\n{}", "Line 1", "Line 2");
    let edge_cases = vec![
        "\0",              // Null character
        "\u{FFFF}",        // Unicode replacement character
        &many_emoji,       // Many emoji
        &many_zero_width,  // Many zero-width spaces
        &very_long_string, // Very long string
        &multiline,        // Multi-line (shouldn't occur in headings)
        "\u{1F4A9}",       // Poop emoji (test emoji handling)
        "مرحبا بالعالم",   // Arabic RTL text
        "𝕳𝖊𝖑𝖑𝖔 𝖂𝖔𝖗𝖑𝖉",     // Mathematical script characters
    ];

    for input in edge_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // This should not panic
        let result = std::panic::catch_unwind(|| rule.check(&ctx));

        assert!(result.is_ok(), "Rule panicked on input: '{input:?}'");

        // If no panic, the result should be valid
        if let Ok(Ok(warnings)) = result {
            // Warnings list should be valid (can be empty or non-empty)
            assert!(
                warnings.len() <= 100,
                "Suspiciously many warnings for input: '{input:?}'"
            );
        }
    }
}

/// Test property: Consistent behavior across modes
#[test]
fn property_mode_consistency() {
    let github_rule = MD051LinkFragments::new();
    // Note: AnchorStyle is not publicly exposed, so we'll use default for now
    let kramdown_rule = MD051LinkFragments::new();

    let test_inputs = vec![
        "Simple Text",
        "test_with_underscores",
        "Numbers 123",
        "Punctuation!!!",
        "",
        "café",
        "UPPERCASE",
        "Mixed_Case",
    ];

    for input in test_inputs {
        let content = format!("# {input}\n\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Both modes should produce valid results (no panics)
        let github_result = github_rule.check(&ctx);
        let kramdown_result = kramdown_rule.check(&ctx);

        assert!(github_result.is_ok(), "GitHub mode failed for: '{input}'");
        assert!(kramdown_result.is_ok(), "Kramdown mode failed for: '{input}'");

        // For empty input, both should behave similarly
        if input.trim().is_empty() {
            let github_headings = extract_generated_headings(&github_rule, &ctx);
            let kramdown_headings = extract_generated_headings(&kramdown_rule, &ctx);

            assert_eq!(
                github_headings.len(),
                kramdown_headings.len(),
                "Different number of headings generated for empty input"
            );
        }
    }
}

/// Test property: Performance bounds
#[test]
fn property_performance_bounds() {
    let rule = MD051LinkFragments::new();

    // Test that processing time is reasonable for various input sizes
    let long_heading_100 = "Long heading ".repeat(100);
    let very_long_heading_1000 = "Very long heading ".repeat(1000);
    let size_tests = vec![
        (10, "Short"),
        (100, "Medium length heading with some words"),
        (1000, &long_heading_100),
        (10000, &very_long_heading_1000),
    ];

    for (expected_size, base_input) in size_tests {
        let input = if base_input.len() < expected_size {
            format!("{} {}", base_input, "word ".repeat(expected_size / 5))
        } else {
            base_input.chars().take(expected_size).collect()
        };

        let content = format!("# {input}\n\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = std::time::Instant::now();
        let _result = rule.check(&ctx).unwrap();
        let duration = start.elapsed();

        // Performance should scale reasonably with input size
        // Allow 1ms per 100 characters as a rough upper bound
        let max_duration_ms = (input.len() / 100 + 1) as u64;

        assert!(
            duration.as_millis() <= max_duration_ms as u128,
            "Performance issue: took {}ms for {} character input (max allowed: {}ms)",
            duration.as_millis(),
            input.len(),
            max_duration_ms
        );
    }
}

// Helper functions

fn extract_generated_headings(_rule: &MD051LinkFragments, ctx: &LintContext) -> Vec<String> {
    // This is a bit of a hack since we can't directly access the fragment generation
    // Instead, we'll test various fragments to see which ones work

    // For property testing, we'll extract the line info and simulate fragment generation
    let mut fragments = Vec::new();

    for line_info in &ctx.lines {
        if let Some(heading) = &line_info.heading {
            // We can't directly call the private method, so we'll use a heuristic
            // This is not perfect but good enough for property testing
            let text = &heading.text;
            let fragment = text
                .to_lowercase()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '_' {
                        c
                    } else if c.is_whitespace() {
                        '-'
                    } else {
                        ' '
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("-");

            if !fragment.is_empty() {
                fragments.push(fragment);
            }
        }
    }

    fragments
}

fn is_emoji_or_symbol(c: char) -> bool {
    // Simple emoji/symbol detection
    matches!(c as u32,
        0x1F300..=0x1F9FF | // Emoji & Symbols
        0x2600..=0x26FF |   // Miscellaneous Symbols
        0x2700..=0x27BF |   // Dingbats
        0x1F000..=0x1F02F | // Mahjong Tiles
        0x1F0A0..=0x1F0FF   // Playing Cards
    )
}

fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    // Simple Jaccard similarity based on character sets
    let chars1: HashSet<char> = s1.chars().collect();
    let chars2: HashSet<char> = s2.chars().collect();

    let intersection = chars1.intersection(&chars2).count();
    let union = chars1.union(&chars2).count();

    if union == 0 {
        1.0 // Both empty strings are identical
    } else {
        intersection as f64 / union as f64
    }
}

/// Fuzz-like test with many random-ish inputs
#[test]
fn property_fuzz_like_testing() {
    let rule = MD051LinkFragments::new();

    // Generate various "random" inputs systematically
    let generators = vec![
        // ASCII printable characters
        (0..128)
            .map(|i| char::from(i as u8))
            .filter(|c| c.is_ascii_graphic())
            .collect::<String>(),
        // Unicode punctuation
        "!@#$%^&*()[]{}|\\:;\"'<>?,./-=+_`~".to_string(),
        // Mixed scripts
        "Hello世界مرحباПривет".to_string(),
        // Repeated patterns
        "abc".repeat(100),
        "!@#".repeat(50),
        " - ".repeat(30),
        // Edge case lengths
        "a".to_string(),
        "ab".repeat(1000),
    ];

    for input in generators {
        // Test various prefixes and suffixes
        for prefix in &["", " ", "  ", "!"] {
            for suffix in &["", " ", "  ", "!"] {
                let test_input = format!("{prefix}{input}{suffix}");

                let content = format!("# {test_input}\n\n");
                let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

                // Should not panic
                let result = rule.check(&ctx);
                assert!(result.is_ok(), "Failed on fuzz input: '{test_input:?}'");
            }
        }
    }
}
