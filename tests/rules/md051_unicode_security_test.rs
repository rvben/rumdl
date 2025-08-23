// Unicode security and normalization edge case tests for MD051
// These tests specifically target Unicode-related security vulnerabilities

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;

/// Test Unicode normalization consistency to prevent spoofing attacks
#[test]
fn test_unicode_normalization_spoofing_prevention() {
    let rule = MD051LinkFragments::new();

    // Test visually identical but different Unicode representations
    let normalization_pairs = vec![
        // Composed vs decomposed characters
        ("café", "cafe\u{0301}"),             // é vs e + combining acute
        ("naïve", "nai\u{0308}ve"),           // ï vs i + combining diaeresis
        ("résumé", "re\u{0301}sume\u{0301}"), // Multiple composed vs decomposed
        // Compatibility variants
        ("ﬁle", "file"), // fi ligature vs separate letters
        ("①②③", "123"),  // Circled numbers vs regular numbers
        // Case folding edge cases
        ("İstanbul", "istanbul"), // Turkish I with dot above
        ("MASS", "mass"),         // German sharp s normalization
        // Width variants
        ("ｆｕｌｌ　ｗｉｄｔｈ", "full width"), // Full-width vs half-width
    ];

    for (version1, version2) in normalization_pairs {
        // Test that both versions generate same fragment (if they should)
        let content1 = format!("# {version1}\n\n[Link](#test)");
        let content2 = format!("# {version2}\n\n[Link](#test)");

        let ctx1 = LintContext::new(&content1);
        let ctx2 = LintContext::new(&content2);

        let result1 = rule.check(&ctx1);
        let result2 = rule.check(&ctx2);

        assert!(
            result1.is_ok() && result2.is_ok(),
            "Unicode normalization test failed for: '{version1}' vs '{version2}'"
        );

        // Note: Whether these should generate the same fragment depends on the
        // normalization strategy. This test documents the current behavior.
        println!("✓ Unicode normalization tested: '{version1}' vs '{version2}'");
    }
}

/// Test zero-width and invisible character handling
#[test]
fn test_zero_width_character_injection() {
    let rule = MD051LinkFragments::new();

    let zero_width_cases = vec![
        ("word\u{200B}break", "Zero Width Space (U+200B)"),
        ("word\u{200C}break", "Zero Width Non-Joiner (U+200C)"),
        ("word\u{200D}break", "Zero Width Joiner (U+200D)"),
        ("word\u{FEFF}break", "Zero Width No-Break Space / BOM (U+FEFF)"),
        ("word\u{2060}break", "Word Joiner (U+2060)"),
        ("word\u{061C}break", "Arabic Letter Mark (U+061C)"),
        ("word\u{034F}break", "Combining Grapheme Joiner (U+034F)"),
        // Multiple zero-width characters
        ("\u{200B}\u{200C}\u{200D}text", "Multiple zero-width at start"),
        ("text\u{200B}\u{200C}\u{200D}", "Multiple zero-width at end"),
        ("te\u{200B}st\u{200C}ing\u{200D}now", "Zero-width scattered"),
    ];

    for (input, description) in zero_width_cases {
        let content = format!("# {input}\n\n[Link](#wordbreak)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Zero-width character test failed: {description}");

        // The fragment should be generated correctly without invisible chars
        let warnings = result.unwrap();

        // Should either work (no warnings) or have exactly one warning for mismatch
        assert!(
            warnings.len() <= 1,
            "Unexpected warnings for {}: {} warnings",
            description,
            warnings.len()
        );

        println!("✓ Zero-width character test: {description}");
    }
}

/// Test bidirectional text and RTL override injection
#[test]
fn test_bidirectional_text_injection() {
    let rule = MD051LinkFragments::new();

    let bidi_cases = vec![
        // RTL override characters
        ("left\u{202E}right", "Right-to-Left Override (RLO)"),
        ("normal\u{202D}forced", "Left-to-Right Override (LRO)"),
        // RTL/LTR embedding
        ("text\u{202B}embedded\u{202C}normal", "Right-to-Left Embedding"),
        ("text\u{202A}embedded\u{202C}normal", "Left-to-Right Embedding"),
        // Pop directional formatting
        ("text\u{202C}pop", "Pop Directional Formatting"),
        // Isolate controls
        ("text\u{2066}isolate\u{2069}end", "Left-to-Right Isolate"),
        ("text\u{2067}isolate\u{2069}end", "Right-to-Left Isolate"),
        ("text\u{2068}isolate\u{2069}end", "First Strong Isolate"),
        // Real RTL text mixed with LTR
        ("English العربية English", "Mixed English/Arabic"),
        ("Hello עברית World", "Mixed English/Hebrew"),
        ("Test Русский Text", "Mixed English/Cyrillic"),
    ];

    for (input, description) in bidi_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Bidirectional text test failed: {description}");

        println!("✓ Bidirectional text test: {description}");
    }
}

/// Test control character injection and sanitization
#[test]
fn test_control_character_injection() {
    let rule = MD051LinkFragments::new();

    let control_char_cases = vec![
        // C0 control characters (0x00-0x1F)
        ("test\u{0000}null", "Null character (NUL)"),
        ("test\u{0001}start", "Start of Heading (SOH)"),
        ("test\u{0007}bell", "Bell character (BEL)"),
        ("test\u{0008}back", "Backspace (BS)"),
        ("test\u{0009}tab", "Horizontal Tab (HT)"),
        ("test\u{000A}newline", "Line Feed (LF)"),
        ("test\u{000B}vtab", "Vertical Tab (VT)"),
        ("test\u{000C}form", "Form Feed (FF)"),
        ("test\u{000D}return", "Carriage Return (CR)"),
        ("test\u{001B}escape", "Escape (ESC)"),
        ("test\u{001F}unit", "Unit Separator (US)"),
        // C1 control characters (0x80-0x9F)
        ("test\u{0080}control", "Padding Character"),
        ("test\u{009F}application", "Application Program Command"),
        // Delete character
        ("test\u{007F}delete", "Delete character (DEL)"),
        // Line separator / Paragraph separator
        ("test\u{2028}line", "Line Separator"),
        ("test\u{2029}para", "Paragraph Separator"),
    ];

    for (input, description) in control_char_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Control character test failed: {description}");

        // Control characters should be handled gracefully
        println!("✓ Control character test: {description}");
    }
}

/// Test emoji and symbol edge cases
#[test]
fn test_emoji_symbol_edge_cases() {
    let rule = MD051LinkFragments::new();

    let emoji_cases = vec![
        // Basic emoji
        ("test 🎉 party", "Basic emoji"),
        ("🚀 rocket start", "Emoji at start"),
        ("end emoji 🎯", "Emoji at end"),
        // Emoji with modifiers
        ("skin tone 👋🏽 wave", "Emoji with skin tone modifier"),
        ("family 👨‍👩‍👧‍👦 emoji", "Multi-person family emoji"),
        ("flag 🏳️‍🌈 pride", "Flag with modifier"),
        // Emoji sequences
        ("🇺🇸🇬🇧🇫🇷 flags", "Country flag sequence"),
        ("👨‍💻👩‍💻 developers", "Professional emoji sequence"),
        // Mathematical symbols
        ("math ∑∆∇ symbols", "Mathematical symbols"),
        ("operators ±×÷ test", "Mathematical operators"),
        // Currency symbols
        ("money $€¥£ symbols", "Currency symbols"),
        // Arrows and shapes
        ("arrows ←→↑↓ test", "Arrow symbols"),
        ("shapes ●■▲♠ test", "Shape symbols"),
        // Combining emoji
        ("keycap 1️⃣2️⃣3️⃣ test", "Keycap emoji sequence"),
    ];

    for (input, description) in emoji_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Emoji test failed: {description}");

        // Emoji should be handled according to GitHub spec (likely stripped)
        println!("✓ Emoji test: {description}");
    }
}

/// Test private use area and undefined Unicode
#[test]
fn test_private_use_area_handling() {
    let rule = MD051LinkFragments::new();

    let private_use_cases = vec![
        // Basic Multilingual Plane private use (U+E000-U+F8FF)
        ("test\u{E000}private", "Private Use Area start"),
        ("test\u{F8FF}private", "Private Use Area end"),
        ("custom\u{E123}symbol", "Custom private symbol"),
        // Supplementary Private Use Areas
        ("test\u{F0000}plane15", "Plane 15 Private Use"),
        ("test\u{100000}plane16", "Plane 16 Private Use"),
        // Replacement character (used for invalid sequences)
        ("invalid\u{FFFD}char", "Unicode replacement character"),
        // Non-characters (should not appear in text but test anyway)
        ("test\u{FFFE}nonchar", "Non-character U+FFFE"),
        ("test\u{FFFF}nonchar", "Non-character U+FFFF"),
    ];

    for (input, description) in private_use_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Private use area test failed: {description}");

        println!("✓ Private use area test: {description}");
    }
}

/// Test surrogate pair handling and high code points
#[test]
fn test_surrogate_pair_handling() {
    let rule = MD051LinkFragments::new();

    let high_codepoint_cases = vec![
        // Mathematical script characters (require surrogate pairs in UTF-16)
        ("math 𝕳𝖊𝖑𝖑𝖔 script", "Mathematical script"),
        ("bold 𝐇𝐞𝐥𝐥𝐨 text", "Mathematical bold"),
        // Ancient scripts
        ("ancient 𐎀𐎁𐎂 script", "Ugaritic script"),
        ("linear 𐀀𐀁𐀂 b", "Linear B script"),
        // Musical symbols
        ("music 𝄞𝄢𝅘𝅥𝅮 notes", "Musical notation"),
        // Other high code point ranges
        ("tags 𝟏𝟐𝟑 test", "Mathematical digits"),
        ("symbols 𝟬𝟭𝟮 test", "Mathematical monospace"),
    ];

    for (input, description) in high_codepoint_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "High codepoint test failed: {description}");

        println!("✓ High codepoint test: {description}");
    }
}

/// Test Unicode case folding edge cases
#[test]
fn test_unicode_case_folding_edge_cases() {
    let rule = MD051LinkFragments::new();

    let case_folding_cases = vec![
        // Turkish I problem
        ("İstanbul", "Turkish capital I with dot"),
        ("ıstanbul", "Turkish lowercase dotless i"),
        // German sharp s
        ("Straße", "German sharp s (ß)"),
        ("STRASSE", "German SS uppercase"),
        // Greek case issues
        ("ΏΝΤΆΣ", "Greek with tonos"),
        ("ώντάς", "Greek lowercase with tonos"),
        // Special case mappings
        ("ﬀ", "Latin small ligature ff"),
        ("ﬃ", "Latin small ligature ffi"),
        ("ﬆ", "Latin small ligature st"),
        // Case folding with accents
        ("CAFÉ", "Uppercase with accents"),
        ("café", "Lowercase with accents"),
        ("Café", "Mixed case with accents"),
    ];

    for (input, description) in case_folding_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Case folding test failed: {description}");

        println!("✓ Case folding test: {description}");
    }
}

/// Test combining character sequences and normalization
#[test]
fn test_combining_character_sequences() {
    let rule = MD051LinkFragments::new();

    let combining_cases = vec![
        // Basic combining marks
        ("e\u{0301}", "e with combining acute"),     // é
        ("a\u{0308}", "a with combining diaeresis"), // ä
        ("o\u{0303}", "o with combining tilde"),     // õ
        // Multiple combining marks on one base
        ("e\u{0301}\u{0308}", "e with acute and diaeresis"),
        ("a\u{0300}\u{0323}", "a with grave and dot below"),
        // Combining mark sequences
        ("cafe\u{0301}", "café with combining accent"),
        ("resume\u{0301}", "resumé with combining accent"),
        // Unusual combining sequences
        ("a\u{0363}\u{0364}\u{0365}", "a with multiple combining marks"),
        // Combining marks with emoji
        ("🇺\u{0301}🇸", "Flag with combining mark (unusual)"),
        // Zero-width characters mixed with combining
        ("a\u{200B}\u{0301}", "a with zero-width space and accent"),
    ];

    for (input, description) in combining_cases {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Combining character test failed: {description}");

        println!("✓ Combining character test: {description}");
    }
}

/// Integration test for complex Unicode security scenarios
#[test]
fn test_complex_unicode_security_scenarios() {
    let rule = MD051LinkFragments::new();

    // Real-world attack scenarios combining multiple Unicode techniques
    let combining_bomb = format!("a{}", "\u{0301}".repeat(100));
    let complex_scenarios = vec![
        // Mixed attack: RTL override + zero-width + emoji
        ("safe\u{202E}\u{200B}🎉attack", "Mixed RTL override attack"),
        // Normalization attack: multiple representations
        ("cafe\u{0301}\u{200B}́", "Normalization confusion attack"),
        // Bidirectional spoofing
        ("user\u{202E}moc.evil\u{202C}@bank.com", "Domain spoofing attempt"),
        // Control character injection in realistic text
        ("Click here:\u{202E}gro.buhtig\u{202C}/malicious", "URL spoofing"),
        // Combining character bomb (performance attack)
        (&combining_bomb, "Combining character bomb"),
        // Mixed script confusion
        ("раураl.com", "Cyrillic/Latin script mixing"), // looks like "paypal"
        // Zero-width character splitting
        ("ad\u{200B}min@ex\u{200C}ample.com", "Split legitimate text"),
    ];

    for (input, description) in complex_scenarios {
        let content = format!("# {input}\n\n[Link](#test)");
        let ctx = LintContext::new(&content);

        let start = std::time::Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        // Should handle all attacks gracefully and quickly
        assert!(result.is_ok(), "Unicode security scenario failed: {description}");
        assert!(
            duration < std::time::Duration::from_secs(1),
            "Security scenario took too long: {description} - {duration:?}"
        );

        println!("✓ Security scenario handled: {description} in {duration:?}");
    }
}
