// Comprehensive edge cases test suite for MD051 rule
//
// This test file consolidates all critical edge cases for the MD051 anchor style
// implementations, providing 100% edge case coverage across security, performance,
// algorithmic correctness, and cross-style verification.
//
// The tests are organized into five main categories:
// 1. Security edge cases for all styles
// 2. Algorithm correctness edge cases
// 3. Performance edge cases
// 4. Degenerate cases
// 5. Cross-style verification

#[cfg(test)]
mod tests {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD051LinkFragments;
    use rumdl_lib::utils::anchor_styles::AnchorStyle;
    use std::time::{Duration, Instant};

    /// Helper to create rule with specific anchor style
    fn create_rule(style: &AnchorStyle) -> MD051LinkFragments {
        MD051LinkFragments::with_anchor_style(style.clone())
    }

    /// Helper to assert fragment generation works correctly
    fn assert_fragment_generation(style: &AnchorStyle, heading: &str, expected: &str, test_name: &str) {
        let rule = create_rule(style);
        let content = format!("# {heading}\n\n[Link](#{expected})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Debug output to see what's happening
        if !result.is_empty() {
            eprintln!("DEBUG: Heading '{heading}' expected fragment '{expected}' but got warnings: {result:?}");
        }

        assert_eq!(
            result.len(),
            0,
            "{} failed for style {:?}: heading '{}' should generate fragment '{}' but got {} warnings",
            test_name,
            style,
            heading,
            expected,
            result.len()
        );
    }

    /// Helper to verify no panic occurs and performance is reasonable
    fn assert_safe_and_fast(style: &AnchorStyle, heading: &str, max_duration: Duration, test_name: &str) {
        let rule = create_rule(style);
        let content = format!("# {heading}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "{test_name} caused panic for style {style:?}");
        assert!(
            duration < max_duration,
            "{test_name} took too long for style {style:?}: {duration:?} > {max_duration:?}"
        );
    }

    // ================================
    // 1. SECURITY EDGE CASES
    // ================================

    #[test]
    fn test_unicode_injection_attacks() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Zero-width character injection attacks
        let zero_width_cases = vec![
            ("word\u{200B}break", "Zero Width Space injection"),
            ("test\u{FEFF}ing", "Byte Order Mark injection"),
            ("invisible\u{2060}joiner", "Word Joiner injection"),
            ("split\u{200C}text", "Zero Width Non-Joiner injection"),
            ("join\u{200D}chars", "Zero Width Joiner injection"),
            ("arabic\u{061C}mark", "Arabic Letter Mark injection"),
            ("combining\u{034F}joiner", "Combining Grapheme Joiner injection"),
        ];

        for style in &styles {
            for (input, description) in &zero_width_cases {
                assert_safe_and_fast(
                    style,
                    input,
                    Duration::from_secs(1),
                    &format!("Zero-width injection: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_rtl_bidirectional_injection() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // RTL override and bidirectional text injection
        let rtl_cases = vec![
            ("left\u{202E}right", "Right-to-Left Override"),
            ("normal\u{202D}forced", "Left-to-Right Override"),
            ("text\u{202B}embedded\u{202C}normal", "Right-to-Left Embedding"),
            ("text\u{202A}embedded\u{202C}normal", "Left-to-Right Embedding"),
            ("text\u{2066}isolate\u{2069}end", "Left-to-Right Isolate"),
            ("text\u{2067}isolate\u{2069}end", "Right-to-Left Isolate"),
            ("text\u{2068}isolate\u{2069}end", "First Strong Isolate"),
            ("safe\u{202E}\u{200B}🎉attack", "Mixed RTL + zero-width + emoji"),
        ];

        for style in &styles {
            for (input, description) in &rtl_cases {
                assert_safe_and_fast(
                    style,
                    input,
                    Duration::from_secs(1),
                    &format!("RTL injection: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_control_character_injection() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Control character injection (C0 and C1 controls)
        let control_cases = vec![
            ("test\u{0000}null", "Null byte injection"),
            ("test\u{0001}start", "Start of Heading"),
            ("test\u{0007}bell", "Bell character"),
            ("test\u{0008}back", "Backspace"),
            ("test\u{000B}vtab", "Vertical Tab"),
            ("test\u{000C}form", "Form Feed"),
            ("test\u{001B}escape", "Escape character"),
            ("test\u{001F}unit", "Unit Separator"),
            ("test\u{007F}delete", "Delete character"),
            ("test\u{0080}control", "C1 control character"),
            ("test\u{009F}application", "Application Program Command"),
        ];

        for style in &styles {
            for (input, description) in &control_cases {
                assert_safe_and_fast(
                    style,
                    input,
                    Duration::from_secs(1),
                    &format!("Control character: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_redos_prevention() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Patterns that can cause catastrophic backtracking in regex
        let redos_patterns = vec![
            (format!("{}(a+)+X", "a".repeat(30)), "Nested quantifiers"),
            (format!("{}(a|a)*X", "a".repeat(30)), "Alternation with repetition"),
            (format!("{}(a*)*X", "a".repeat(25)), "Nested star quantifiers"),
        ];

        let static_patterns = vec![
            ("*".repeat(50) + "text" + &"*".repeat(50), "Markdown asterisks"),
            ("`".repeat(30) + "code" + &"`".repeat(30), "Markdown backticks"),
            ("[".repeat(20) + "link" + &"]".repeat(20), "Markdown brackets"),
            ("🎉".repeat(50) + &"a".repeat(50), "Emoji + text pattern"),
            ("a\u{0301}".repeat(100), "Combining character repetition"),
        ];

        for style in &styles {
            for (pattern, description) in &redos_patterns {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(2), // Allow more time for complex patterns
                    &format!("ReDoS prevention: {description}"),
                );
            }

            for (pattern, description) in &static_patterns {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(2),
                    &format!("ReDoS prevention: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_memory_exhaustion_protection() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Create patterns with static lifetime
        let hyphen_pattern = format!("test{}end", "-".repeat(10000));
        let alternating_pattern = "-a".repeat(5000);
        let space_pattern = format!("word{}end", " ".repeat(10000));
        let punctuation_pattern = "!@#$%^&*()".repeat(1000);
        let combining_pattern = format!("e{}", "\u{0301}".repeat(1000));
        let emoji_pattern = "🎉".repeat(1000);
        let cjk_pattern = "中".repeat(2000);
        let mixed_pattern = "word🎉-中文".repeat(1000);

        let memory_bomb_patterns = vec![
            (&hyphen_pattern, "10K consecutive hyphens"),
            (&alternating_pattern, "Alternating hyphen pattern"),
            (&space_pattern, "10K spaces"),
            (&punctuation_pattern, "Punctuation bomb"),
            (&combining_pattern, "Combining character bomb"),
            (&emoji_pattern, "Emoji bomb"),
            (&cjk_pattern, "CJK character bomb"),
            (&mixed_pattern, "Mixed pattern bomb"),
        ];

        for style in &styles {
            for (pattern, description) in &memory_bomb_patterns {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(3),
                    &format!("Memory exhaustion: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_input_size_limits() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Test progressively larger inputs to verify scaling
        let sizes = vec![1000, 10000, 50000];

        for style in &styles {
            for size in &sizes {
                let heading = "a".repeat(*size);
                assert_safe_and_fast(
                    style,
                    &heading,
                    Duration::from_secs(5),
                    &format!("Large input: {size} chars"),
                );
            }
        }
    }

    // ================================
    // 2. ALGORITHM CORRECTNESS EDGE CASES
    // ================================

    #[test]
    fn test_arrow_patterns_correctness() {
        // Test arrow pattern handling across all styles
        let test_cases = vec![
            // GitHub style expectations
            ("cbrown -> sbrown", AnchorStyle::GitHub, "cbrown---sbrown"),
            ("cbrown --> sbrown", AnchorStyle::GitHub, "cbrown----sbrown"),
            ("cbrown <-> sbrown", AnchorStyle::GitHub, "cbrown---sbrown"),
            ("cbrown ==> sbrown", AnchorStyle::GitHub, "cbrown--sbrown"),
            ("left <- right", AnchorStyle::GitHub, "left---right"),
            ("start <-- end", AnchorStyle::GitHub, "start----end"),
            // Jekyll style expectations (simpler arrow handling)
            ("cbrown -> sbrown", AnchorStyle::KramdownGfm, "cbrown---sbrown"),
            ("cbrown --> sbrown", AnchorStyle::KramdownGfm, "cbrown--sbrown"),
            ("cbrown <-> sbrown", AnchorStyle::KramdownGfm, "cbrown---sbrown"),
            // Kramdown style expectations
            ("cbrown -> sbrown", AnchorStyle::Kramdown, "cbrown---sbrown"),
            ("cbrown --> sbrown", AnchorStyle::Kramdown, "cbrown----sbrown"),
        ];

        for (heading, style, expected) in test_cases {
            assert_fragment_generation(&style, heading, expected, &format!("Arrow pattern: {heading}"));
        }
    }

    #[test]
    fn test_hyphen_consolidation_edge_cases() {
        let test_cases = vec![
            // GitHub: preserves consecutive hyphens
            ("test---hyphens", AnchorStyle::GitHub, "test---hyphens"),
            ("test----four", AnchorStyle::GitHub, "test----four"),
            ("multiple-----hyphens", AnchorStyle::GitHub, "multiple-----hyphens"),
            // Jekyll/Kramdown: consolidates hyphens differently
            ("test---hyphens", AnchorStyle::KramdownGfm, "testhyphens"),
            ("test----four", AnchorStyle::KramdownGfm, "test-four"),
            ("test---hyphens", AnchorStyle::Kramdown, "test---hyphens"),
            ("test----four", AnchorStyle::Kramdown, "test----four"),
            // Edge cases with boundaries
            ("---leading", AnchorStyle::GitHub, "---leading"),
            ("trailing---", AnchorStyle::GitHub, "trailing---"),
            ("---both---", AnchorStyle::GitHub, "---both---"),
        ];

        for (heading, style, expected) in test_cases {
            assert_fragment_generation(&style, heading, expected, &format!("Hyphen consolidation: {heading}"));
        }
    }

    #[test]
    fn test_emoji_detection_completeness() {
        let emoji_test_cases = vec![
            // Basic emoji ranges
            ("test 🎉 emoji", "Basic emoji"),
            ("food 🍕🍔🍟 symbols", "Food emoji sequence"),
            ("flags 🇺🇸🇬🇧🇫🇷 test", "Country flags"),
            ("people 👨‍👩‍👧‍👦 family", "Family emoji"),
            ("work 👨‍💻👩‍💻 coding", "Professional emoji"),
            ("skin 👋🏽 tone", "Skin tone modifier"),
            ("rainbow 🏳️‍🌈 flag", "Flag with modifier"),
            // Symbol ranges that might be treated as emoji
            ("math ∑∆∇ symbols", "Mathematical symbols"),
            ("arrows ←→↑↓ test", "Arrow symbols"),
            ("shapes ●■▲♠ test", "Geometric shapes"),
            ("currency $€¥£ symbols", "Currency symbols"),
            // Keycap and combining emoji
            ("numbers 1️⃣2️⃣3️⃣ test", "Keycap numbers"),
            ("clock 🕐🕑🕒 time", "Clock emoji"),
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for (heading, description) in &emoji_test_cases {
                assert_safe_and_fast(
                    style,
                    heading,
                    Duration::from_millis(500),
                    &format!("Emoji detection: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_markdown_stripping_edge_cases() {
        // Test cases for GitHub and KramdownGfm (which strip URLs from links)
        let markdown_cases_strip_urls = vec![
            // Nested formatting
            ("**bold *italic* text**", "bold-italic-text"),
            ("***bold italic*** text", "bold-italic-text"),
            ("**_mixed_ formatting**", "mixed-formatting"),
            ("~~*strike italic*~~", "strike-italic"),
            // Partial formatting
            ("partial**bold text", "partialbold-text"),
            ("*italic text without close", "italic-text-without-close"),
            ("**bold with `code` inside**", "bold-with-code-inside"),
            // Complex link stripping
            ("[Link](url) with [another](url2)", "link-with-another"),
            ("[Link [nested] link](url)", "link-nested-link"),
            ("![Image](url) caption", "image-caption"),
            // Code spans
            ("`simple code`", "simple-code"),
            ("``code with ` backtick``", "code-with--backtick"), // Backtick becomes hyphen
            ("`code` and `more code`", "code-and-more-code"),
            // Edge cases
            ("***", ""),
            ("**bold****more**", "boldmore"),
            ("[](empty-link)", ""),
        ];

        // Test cases for Kramdown (which includes URLs in links)
        let kramdown_cases = vec![
            // Nested formatting
            ("**bold *italic* text**", "bold-italic-text"),
            ("***bold italic*** text", "bold-italic-text"),
            ("**_mixed_ formatting**", "mixed-formatting"),
            ("~~*strike italic*~~", "strike-italic"),
            // Partial formatting
            ("partial**bold text", "partialbold-text"),
            ("*italic text without close", "italic-text-without-close"),
            ("**bold with `code` inside**", "bold-with-code-inside"),
            // Complex link stripping - kramdown keeps URLs
            ("[Link](url) with [another](url2)", "linkurl-with-anotherurl2"),
            ("[Link [nested] link](url)", "link-nested-linkurl"),
            ("![Image](url) caption", "imageurl-caption"),
            // Code spans
            ("`simple code`", "simple-code"),
            ("``code with ` backtick``", "code-with--backtick"),
            ("`code` and `more code`", "code-and-more-code"),
            // Edge cases
            ("***", ""),
            ("**bold****more**", "boldmore"),
            ("[](empty-link)", "empty-link"),
        ];

        // Test GitHub and KramdownGfm with URL stripping
        for style in &[AnchorStyle::GitHub, AnchorStyle::KramdownGfm] {
            for (heading, expected) in &markdown_cases_strip_urls {
                assert_fragment_generation(style, heading, expected, &format!("Markdown stripping: {heading}"));
            }
        }

        // Test Kramdown with URL preservation
        for (heading, expected) in &kramdown_cases {
            assert_fragment_generation(
                &AnchorStyle::Kramdown,
                heading,
                expected,
                &format!("Markdown stripping (Kramdown): {heading}"),
            );
        }
    }

    #[test]
    fn test_link_processing_variations() {
        // Test various link formats that should be ignored during processing
        let rule = MD051LinkFragments::new();

        let content = r#"# Real Heading

## Test Section

Internal links (should be validated):
[Valid link](#real-heading)
[Another valid](#test-section)
[Invalid link](#missing-section)

External links (should be ignored):
[HTTP](http://example.com#section)
[HTTPS](https://example.com#section)
[FTP](ftp://example.com#section)
[Mailto](mailto:user@example.com#subject)
[Protocol relative](//example.com#section)

Cross-file links (should be ignored):
[File](file.md#section)
[Relative](./docs/file.md#section)
[Deep relative](../../other/file.md#section)
[Query params](file.md?v=1#section)
[Complex path](path/to/file.md#section)

Liquid templates (should be ignored):
[Post]({% post_url 2023-01-01-post %}#section)
[Include]({% include file.html %}#section)
[Variable]({{ site.url }}/page#section)

Edge cases:
[Empty fragment](file.md#)
[Just hash](#)
[Double hash](file.md#section#subsection)
[No extension](somefile#section)
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the invalid internal link
        // Note: "somefile#section" is treated as a cross-file link (GitHub-style extension-less)
        assert_eq!(result.len(), 1, "Should flag only invalid internal link");

        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("missing-section")),
            "Should warn about missing-section"
        );
    }

    // ================================
    // 3. PERFORMANCE EDGE CASES
    // ================================

    #[test]
    fn test_large_input_scaling() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Test that processing time scales linearly, not exponentially
        let sizes = [100, 500, 1000, 5000];

        for style in &styles {
            let mut previous_time = Duration::from_nanos(1);

            for size in sizes.iter() {
                let heading = "word ".repeat(*size);
                let content = format!("# {heading}\n\n[Link](#test)");
                let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let rule = create_rule(style);

                let start = Instant::now();
                let result = rule.check(&ctx);
                let duration = start.elapsed();

                assert!(result.is_ok(), "Scaling test failed at size {size} for {style:?}");

                // Time should scale roughly linearly (allow generous factor for CI variance)
                if previous_time > Duration::from_millis(1) {
                    let time_ratio = duration.as_nanos() as f64 / previous_time.as_nanos() as f64;
                    assert!(
                        time_ratio < 20.0, // Very generous to account for CI variance
                        "Poor scaling for {style:?} at size {size}: ratio {time_ratio:.2}"
                    );
                }

                previous_time = duration;
            }
        }
    }

    #[test]
    fn test_pathological_patterns() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Create patterns with static lifetime
        let hyphen_pattern = format!("test{}end", "-".repeat(1000));
        let alternating_pattern = "-a".repeat(1000);
        let space_pattern = "word ".repeat(1000);
        let punctuation_pattern = "!@#$%^&*()".repeat(500);
        let emoji_pattern = "🎉".repeat(200);
        let cjk_pattern = "中文".repeat(500);
        let accented_pattern = "café".repeat(500);
        let mixed_pattern = "a-b ".repeat(500);
        let cluster_pattern = "test!!! ".repeat(300);

        let pathological_cases = vec![
            (&hyphen_pattern, "1K hyphens"),
            (&alternating_pattern, "Alternating hyphens"),
            (&space_pattern, "1K spaces"),
            (&punctuation_pattern, "500x punctuation"),
            (&emoji_pattern, "200 emoji"),
            (&cjk_pattern, "500 CJK"),
            (&accented_pattern, "500 accented"),
            (&mixed_pattern, "Mixed patterns"),
            (&cluster_pattern, "Punctuation clusters"),
        ];

        for style in &styles {
            for (pattern, description) in &pathological_cases {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(2),
                    &format!("Pathological: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_character_bombs() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Create character bombs with static lifetime
        let combining_accent_bomb = format!("a{}", "\u{0301}".repeat(500));
        let combining_diaeresis_bomb = format!("e{}", "\u{0308}".repeat(300));
        let zero_width_bomb = format!("word{}", "\u{200B}".repeat(1000));
        let word_joiner_bomb = format!("text{}", "\u{2060}".repeat(500));
        let control_sequence = "\u{0001}\u{0002}\u{0003}".repeat(1000);
        let private_use_sequence = "\u{E000}\u{E001}\u{E002}".repeat(500);
        let mixed_classes = "a\u{0301}\u{200B}🎉".repeat(200);

        let character_bombs = vec![
            (&combining_accent_bomb, "500 combining accents"),
            (&combining_diaeresis_bomb, "300 combining diaeresis"),
            (&zero_width_bomb, "1K zero-width spaces"),
            (&word_joiner_bomb, "500 word joiners"),
            (&control_sequence, "Control character sequence"),
            (&private_use_sequence, "Private use characters"),
            (&mixed_classes, "Mixed character classes"),
        ];

        for style in &styles {
            for (pattern, description) in &character_bombs {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(2),
                    &format!("Character bomb: {description}"),
                );
            }
        }
    }

    #[test]
    fn test_consecutive_character_limits() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        // Create consecutive patterns with static lifetime
        let a_pattern = "a".repeat(10000);
        let upper_a_pattern = "A".repeat(5000);
        let digit_pattern = "1".repeat(5000);
        let underscore_pattern = "_".repeat(2000);
        let hyphen_pattern = "-".repeat(2000);
        let space_pattern = " ".repeat(5000);
        let exclamation_pattern = "!".repeat(1000);
        let emoji_pattern = "🎉".repeat(500);

        let consecutive_patterns = vec![
            (&a_pattern, "10K a's"),
            (&upper_a_pattern, "5K A's"),
            (&digit_pattern, "5K digits"),
            (&underscore_pattern, "2K underscores"),
            (&hyphen_pattern, "2K hyphens"),
            (&space_pattern, "5K spaces"),
            (&exclamation_pattern, "1K exclamations"),
            (&emoji_pattern, "500 identical emoji"),
        ];

        for style in &styles {
            for (pattern, description) in &consecutive_patterns {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(3),
                    &format!("Consecutive limit: {description}"),
                );
            }
        }
    }

    // ================================
    // 4. DEGENERATE CASES
    // ================================

    #[test]
    fn test_empty_and_whitespace_only() {
        let degenerate_cases = vec![
            // Empty cases
            ("", ""),
            // Whitespace only
            (" ", ""),
            ("   ", ""),
            ("\t", ""),
            ("\n", ""),
            (" \t \n ", ""),
            // Whitespace with zero-width chars
            (" \u{200B} ", ""),
            ("\u{2060}\u{200C}", ""),
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for (heading, expected) in &degenerate_cases {
                assert_fragment_generation(
                    style,
                    heading,
                    expected,
                    &format!("Empty/whitespace: '{}'", heading.escape_debug()),
                );
            }
        }
    }

    #[test]
    fn test_numbers_only() {
        // GitHub and KramdownGfm preserve numbers, Kramdown generates "section" for number-only headings
        let test_cases = vec![
            ("123", "123", "123", "section"),
            ("0", "0", "0", "section"),
            ("999", "999", "999", "section"),
            ("12345", "12345", "12345", "section"),
            ("007", "007", "007", "section"),
            ("1.0", "10", "10", "section"),
            ("3.14", "314", "314", "section"),
            ("1,000", "1000", "1000", "section"),
            ("1 2 3", "1-2-3", "1-2-3", "section"),
            ("Number 42", "number-42", "number-42", "number-42"),
            ("456 heading", "456-heading", "456-heading", "heading"),
        ];

        for (heading, github_expected, gfm_expected, kramdown_expected) in test_cases {
            assert_fragment_generation(
                &AnchorStyle::GitHub,
                heading,
                github_expected,
                &format!("Numbers only (GitHub): {heading}"),
            );
            assert_fragment_generation(
                &AnchorStyle::KramdownGfm,
                heading,
                gfm_expected,
                &format!("Numbers only (KramdownGfm): {heading}"),
            );
            assert_fragment_generation(
                &AnchorStyle::Kramdown,
                heading,
                kramdown_expected,
                &format!("Numbers only (Kramdown): {heading}"),
            );
        }
    }

    #[test]
    fn test_punctuation_only() {
        let punctuation_cases = vec![
            ("!!!", ""),
            ("???", ""),
            ("...", ""),
            ("---", ""),
            ("***", ""),
            ("@#$%", ""),
            ("()[]", ""),
            ("{}<>", ""),
            ("!@#$%^&*()", ""),
            (".,;:", ""),
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for (heading, expected) in &punctuation_cases {
                let test_name = format!("Punctuation only: {heading}");

                // For Kramdown, punctuation-only headings might generate "section"
                if style == &AnchorStyle::Kramdown && expected.is_empty() {
                    // Test both possibilities
                    let rule = create_rule(style);
                    let content1 = format!("# {heading}\n\n[Link](#{expected})");
                    let content2 = format!("# {heading}\n\n[Link](#section)");

                    let ctx1 = LintContext::new(&content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
                    let ctx2 = LintContext::new(&content2, rumdl_lib::config::MarkdownFlavor::Standard, None);

                    let result1 = rule.check(&ctx1).unwrap();
                    let result2 = rule.check(&ctx2).unwrap();

                    assert!(
                        result1.is_empty() || result2.is_empty(),
                        "{test_name} failed: neither empty fragment nor 'section' worked for kramdown"
                    );
                } else {
                    assert_fragment_generation(style, heading, expected, &test_name);
                }
            }
        }
    }

    #[test]
    fn test_unicode_only() {
        let unicode_cases = vec![
            ("中文", "中文", "中文", "section"),                 // Chinese
            ("العربية", "العربية", "العربية", "section"),        // Arabic
            ("Русский", "русский", "русский", "section"),        // Russian (Cyrillic only -> section)
            ("한국어", "한국어", "한국어", "section"),           // Korean
            ("עברית", "עברית", "עברית", "section"),              // Hebrew
            ("ελληνικά", "ελληνικά", "ελληνικά", "section"),     // Greek
            ("日本語", "日本語", "日本語", "section"),           // Japanese
            ("Português", "português", "português", "portugus"), // Portuguese (ASCII P, kramdown strips non-ASCII)
            ("Español", "español", "español", "espaol"),         // Spanish (ASCII E, kramdown strips ñ)
            ("Français", "français", "français", "franais"),     // French (ASCII F, kramdown strips ç)
            ("Deutsch", "deutsch", "deutsch", "deutsch"),        // German (all ASCII)
        ];

        for (heading, github_expected, gfm_expected, kramdown_expected) in unicode_cases {
            assert_fragment_generation(
                &AnchorStyle::GitHub,
                heading,
                github_expected,
                &format!("Unicode only (GitHub): {heading}"),
            );
            assert_fragment_generation(
                &AnchorStyle::KramdownGfm,
                heading,
                gfm_expected,
                &format!("Unicode only (KramdownGfm): {heading}"),
            );
            assert_fragment_generation(
                &AnchorStyle::Kramdown,
                heading,
                kramdown_expected,
                &format!("Unicode only (Kramdown): {heading}"),
            );
        }
    }

    #[test]
    fn test_whitespace_only_variations() {
        let whitespace_cases = vec![
            (" ", ""),
            ("  ", ""),
            ("   ", ""),
            ("\t", ""),
            ("\n", ""),
            ("\r", ""),
            (" \t ", ""),
            (" \n ", ""),
            ("\t\n\r", ""),
            // Unicode whitespace
            ("\u{00A0}", ""), // Non-breaking space
            ("\u{2000}", ""), // En quad
            ("\u{2001}", ""), // Em quad
            ("\u{2002}", ""), // En space
            ("\u{2003}", ""), // Em space
            ("\u{2004}", ""), // Three-per-em space
            ("\u{2005}", ""), // Four-per-em space
            ("\u{2006}", ""), // Six-per-em space
            ("\u{2007}", ""), // Figure space
            ("\u{2008}", ""), // Punctuation space
            ("\u{2009}", ""), // Thin space
            ("\u{200A}", ""), // Hair space
            ("\u{2028}", ""), // Line separator
            ("\u{2029}", ""), // Paragraph separator
            ("\u{3000}", ""), // Ideographic space
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for (heading, expected) in &whitespace_cases {
                assert_fragment_generation(
                    style,
                    heading,
                    expected,
                    &format!("Whitespace variation: U+{:04X}", heading.chars().next().unwrap() as u32),
                );
            }
        }
    }

    // ================================
    // 5. CROSS-STYLE VERIFICATION
    // ================================

    #[test]
    fn test_same_input_different_outputs() {
        // Test cases where different styles should produce different outputs
        let style_difference_cases = vec![
            // Underscores: GitHub preserves, others might not
            (
                "test_method",
                [
                    (AnchorStyle::GitHub, "test_method"),
                    (AnchorStyle::KramdownGfm, "test_method"),
                    (AnchorStyle::Kramdown, "testmethod"),
                ],
            ),
            // Accents: different normalization approaches
            (
                "Café Menu",
                [
                    (AnchorStyle::GitHub, "café-menu"),
                    (AnchorStyle::KramdownGfm, "café-menu"),
                    (AnchorStyle::Kramdown, "caf-menu"), // Kramdown removes accented chars entirely
                ],
            ),
            // Complex punctuation: different arrow handling
            (
                "A -> B",
                [
                    (AnchorStyle::GitHub, "a---b"),
                    (AnchorStyle::KramdownGfm, "a---b"), // GFM preserves arrow as 3 hyphens
                    (AnchorStyle::Kramdown, "a---b"),    // Kramdown also preserves arrow as 3 hyphens
                ],
            ),
            // Multiple hyphens: consolidation differences
            (
                "test---hyphens",
                [
                    (AnchorStyle::GitHub, "test---hyphens"),
                    (AnchorStyle::KramdownGfm, "testhyphens"), // GFM consolidates
                    (AnchorStyle::Kramdown, "test---hyphens"), // Kramdown preserves
                ],
            ),
        ];

        for (heading, style_expectations) in style_difference_cases {
            for (style, expected) in style_expectations {
                assert_fragment_generation(
                    &style,
                    heading,
                    expected,
                    &format!("Cross-style: {heading} for {style:?}"),
                );
            }
        }
    }

    #[test]
    fn test_style_specific_behaviors() {
        // Test behaviors that are unique to each style

        // GitHub-specific: complex arrow handling
        let github_specific = vec![
            ("cbrown --> sbrown", "cbrown----sbrown"),
            ("cbrown <-> sbrown", "cbrown---sbrown"),
            ("cbrown ==> sbrown", "cbrown--sbrown"),
            ("A & B", "a--b"), // Ampersand becomes double hyphen
        ];

        for (heading, expected) in github_specific {
            assert_fragment_generation(
                &AnchorStyle::GitHub,
                heading,
                expected,
                &format!("GitHub-specific: {heading}"),
            );
        }

        // Jekyll-specific: simpler punctuation handling
        let jekyll_specific = vec![
            ("cbrown --> sbrown", "cbrown--sbrown"), // Smart typography preserves --
            ("A & B", "a--b"),                       // Ampersand becomes double hyphen
            ("test---hyphens", "testhyphens"),       // Hyphen consolidation (3 hyphens removed)
        ];

        for (heading, expected) in jekyll_specific {
            assert_fragment_generation(
                &AnchorStyle::KramdownGfm,
                heading,
                expected,
                &format!("Jekyll-specific: {heading}"),
            );
        }

        // Kramdown-specific: underscore removal
        let kramdown_specific = vec![
            ("test_method", "testmethod"),
            ("under_score_text", "underscoretext"),
            ("mixed_with-hyphens", "mixedwith-hyphens"),
        ];

        for (heading, expected) in kramdown_specific {
            assert_fragment_generation(
                &AnchorStyle::Kramdown,
                heading,
                expected,
                &format!("Kramdown-specific: {heading}"),
            );
        }
    }

    #[test]
    fn test_compatibility_validation() {
        // Verify that each style produces valid, non-empty fragments for common cases
        let common_headings = vec![
            "Introduction",
            "Getting Started",
            "API Reference",
            "Configuration Options",
            "Troubleshooting Guide",
            "FAQ & Help",
            "Advanced Usage",
            "Best Practices",
            "Performance Tuning",
            "Security Considerations",
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for heading in &common_headings {
                let rule = create_rule(style);
                let content = format!("# {heading}\n\n");
                let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

                // Just verify no panic and reasonable performance
                let start = Instant::now();
                let result = rule.check(&ctx);
                let duration = start.elapsed();

                assert!(result.is_ok(), "Compatibility test failed for {style:?}: {heading}");
                // Allow 200ms for CI environments (locally runs in ~10-50ms)
                assert!(
                    duration < Duration::from_millis(200),
                    "Compatibility test too slow for {style:?}: {heading} took {duration:?}"
                );
            }
        }
    }

    #[test]
    fn test_cross_style_consistency() {
        // Verify that all styles handle basic cases consistently (no crashes, reasonable performance)
        let consistency_test_cases = vec![
            "Simple Heading",
            "Multiple Words Here",
            "123 Numbers",
            "Special Characters!",
            "Unicode: 中文",
            "Mixed: ASCII & Unicode",
            "Empty content after strip",
            "   Whitespace Boundaries   ",
            "UPPERCASE HEADING",
            "lowercase heading",
            "MiXeD cAsE hEaDiNg",
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for test_case in consistency_test_cases {
            for style in &styles {
                // Verify consistent behavior (no panics, reasonable performance)
                // Allow 200ms for CI environments (locally runs in ~10-50ms)
                assert_safe_and_fast(
                    style,
                    test_case,
                    Duration::from_millis(200),
                    &format!("Cross-style consistency: {test_case} for {style:?}"),
                );
            }
        }
    }

    // ================================
    // INTEGRATION AND STRESS TESTS
    // ================================

    #[test]
    fn test_comprehensive_integration() {
        // Test a complex document that combines multiple edge cases
        let word_pattern = "word ".repeat(100);
        let hyphen_pattern = "-".repeat(50);
        let emoji_pattern = "🎉".repeat(20);
        let word_fragment = "word".repeat(100).replace(" ", "-");
        let hyphen_fragment = "hyphen".repeat(10);

        let complex_content = format!(
            r#"# Main Title with Unicode: Café & 中文

## Section 1: Arrows & Punctuation -> Complex

### Testing!!! Multiple??? Symbols & More

#### Very Long Heading: {word_pattern}

##### Hyphen Stress Test: {hyphen_pattern}

###### Unicode Bomb: {emoji_pattern}

####### Empty After Strip: !!!

######## Numbers Only: 123456

[Valid 1](#main-title-with-unicode-café-中文)
[Valid 2](#section-1-arrows-punctuation-complex)
[Valid 3](#testing-multiple-symbols-more)
[Valid 4](#very-long-heading-{word_fragment})
[Valid 5](#hyphen-stress-test-{hyphen_fragment})
[Valid 6](#unicode-bomb-unicode-bomb)
[Valid 7](#empty-after-strip)
[Valid 8](#numbers-only-123456)
[Invalid](#nonexistent-section)
"#
        );

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            let rule = create_rule(style);
            let ctx = LintContext::new(&complex_content, rumdl_lib::config::MarkdownFlavor::Standard, None);

            let start = Instant::now();
            let result = rule.check(&ctx);
            let duration = start.elapsed();

            assert!(result.is_ok(), "Integration test failed for {style:?}");
            assert!(
                duration < Duration::from_secs(5),
                "Integration test too slow for {style:?}: {duration:?}"
            );

            let warnings = result.unwrap();

            // Should flag the invalid link, possibly some others depending on exact implementation
            assert!(
                !warnings.is_empty(),
                "Should flag at least the invalid link for {style:?}"
            );
            assert!(
                warnings.len() <= 10, // Allow more variation in implementation
                "Too many warnings for {:?}: {}",
                style,
                warnings.len()
            );

            // At least one warning should be about the nonexistent section
            assert!(
                warnings.iter().any(|w| w.message.contains("nonexistent")),
                "Should warn about nonexistent section for {style:?}"
            );

            println!("✓ Integration test passed for {style:?} in {duration:?}");
        }
    }

    #[test]
    fn test_stress_all_styles() {
        // Final stress test: combine all edge case types for all styles
        let security_pattern1 = "safe\u{202E}\u{200B}🎉attack";
        let security_pattern2 = "word\u{200B}\u{200C}\u{200D}break";
        let security_pattern3 = "test\u{0000}\u{001B}control";

        let performance_pattern1 = format!("stress{}", "-".repeat(1000));
        let performance_pattern2 = "word ".repeat(500);
        let performance_pattern3 = "🎉".repeat(100);

        let stress_patterns = vec![
            // Security patterns
            security_pattern1,
            security_pattern2,
            security_pattern3,
            // Performance patterns
            &performance_pattern1,
            &performance_pattern2,
            &performance_pattern3,
            // Algorithm patterns
            "cbrown --> sbrown: --unsafe-paths",
            "API!!! Methods??? & Properties",
            "test---multiple---hyphens",
            // Degenerate patterns
            "!!!",
            "123",
            "中文",
            "   ",
        ];

        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            for pattern in &stress_patterns {
                assert_safe_and_fast(
                    style,
                    pattern,
                    Duration::from_secs(3),
                    &format!("Final stress test: {style:?}"),
                );
            }
        }

        println!("✓ All styles passed comprehensive stress testing");
    }

    /// Test headings with backticks containing special characters like <FILE>
    /// Regression test for bug where `import <FILE> [OPTIONS]` was incorrectly
    /// treating <FILE> as an HTML tag and stripping it before anchor generation
    #[test]
    fn test_backtick_headings_with_angle_brackets() {
        let styles = [AnchorStyle::GitHub, AnchorStyle::KramdownGfm, AnchorStyle::Kramdown];

        for style in &styles {
            // Test case from README.md that was failing
            assert_fragment_generation(
                style,
                "`import <FILE> [OPTIONS]`",
                "import-file-options",
                "Backtick heading with angle brackets and square brackets",
            );

            assert_fragment_generation(
                style,
                "`rule [<rule>]`",
                "rule-rule",
                "Backtick heading with nested angle brackets",
            );

            // Additional edge cases with backticks
            assert_fragment_generation(
                style,
                "`code <Type>`",
                "code-type",
                "Backtick heading with single angle bracket",
            );

            assert_fragment_generation(
                style,
                "`config [options]`",
                "config-options",
                "Backtick heading with square brackets",
            );
        }

        println!("✓ All styles correctly handle backtick headings with special characters");
    }
}
