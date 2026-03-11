use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD036NoEmphasisAsHeading;
use rumdl_lib::rules::md036_no_emphasis_only_first::{HeadingStyle, MD036Config};

#[test]
fn test_md036_for_emphasis_only_lines() {
    // Test for the proper purpose of MD036 - emphasis-only lines detection
    // By default, MD036 does not provide automatic fixes (fix is opt-in)
    let content = "Normal text\n\n**This should be a heading**\n\nMore text";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md036 = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());

    // Check that MD036 detects the issue
    let warnings = md036.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "MD036 should detect emphasis-only line");
    assert!(warnings[0].message.contains("This should be a heading"));

    // Check that no automatic fix is provided by default
    assert!(
        warnings[0].fix.is_none(),
        "MD036 should not provide automatic fixes by default"
    );

    // Apply fix method - should return unchanged content when fix is disabled
    let fixed_md036 = md036.fix(&ctx).unwrap();

    // The emphasis should remain unchanged (fix is opt-in)
    assert_eq!(fixed_md036, content, "Content should remain unchanged");
    assert!(fixed_md036.contains("**This should be a heading**"));
    assert!(!fixed_md036.contains("## This should be a heading"));
}

#[test]
fn test_md036_with_fix_enabled() {
    // Test MD036 auto-fix when explicitly enabled via config
    let content = "Normal text\n\n**This should be a heading**\n\nMore text";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md036 = MD036NoEmphasisAsHeading::new_with_fix(
        ".,;:!?".to_string(),
        true, // fix enabled
        HeadingStyle::Atx,
        2, // heading level
    );

    // Check that MD036 detects the issue and includes a fix
    let warnings = md036.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "MD036 should detect emphasis-only line");
    assert!(warnings[0].message.contains("This should be a heading"));

    // Check that automatic fix is provided when fix is enabled
    assert!(
        warnings[0].fix.is_some(),
        "MD036 should provide automatic fixes when enabled"
    );
    let fix = warnings[0].fix.as_ref().unwrap();
    assert!(
        fix.replacement.contains("## This should be a heading"),
        "Fix should convert to ATX heading"
    );

    // Apply fix method - should convert emphasis to heading
    let fixed_md036 = md036.fix(&ctx).unwrap();

    // The emphasis should be converted to a heading
    assert!(
        fixed_md036.contains("## This should be a heading"),
        "Emphasis should be converted to heading"
    );
    assert!(
        !fixed_md036.contains("**This should be a heading**"),
        "Original emphasis should be removed"
    );
}

#[test]
fn test_md036_fix_with_different_heading_levels() {
    // Test that heading level config works correctly
    let content = "**Title**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test level 1
    let md036_l1 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 1);
    let fixed_l1 = md036_l1.fix(&ctx).unwrap();
    assert_eq!(fixed_l1, "# Title");

    // Test level 3
    let md036_l3 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 3);
    let fixed_l3 = md036_l3.fix(&ctx).unwrap();
    assert_eq!(fixed_l3, "### Title");

    // Test level 6
    let md036_l6 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 6);
    let fixed_l6 = md036_l6.fix(&ctx).unwrap();
    assert_eq!(fixed_l6, "###### Title");
}

#[test]
fn test_md036_fix_is_idempotent() {
    // Test that running fix twice produces the same result
    let content = "**Section Title**\n\nBody text.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md036 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);

    // First fix
    let fixed1 = md036.fix(&ctx).unwrap();
    assert_eq!(fixed1, "## Section Title\n\nBody text.");

    // Second fix on the fixed content
    let ctx2 = LintContext::new(&fixed1, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed2 = md036.fix(&ctx2).unwrap();
    assert_eq!(fixed2, fixed1, "Fix should be idempotent");
}

#[test]
fn test_md036_invalid_heading_level_defaults_to_2() {
    // Invalid heading levels via constructor should default to level 2
    let content = "**Title**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Level 0 should default to 2
    let md036_l0 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 0);
    let fixed_l0 = md036_l0.fix(&ctx).unwrap();
    assert_eq!(fixed_l0, "## Title", "Level 0 should default to level 2");

    // Level 7 should default to 2
    let md036_l7 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 7);
    let fixed_l7 = md036_l7.fix(&ctx).unwrap();
    assert_eq!(fixed_l7, "## Title", "Level 7 should default to level 2");

    // Level 255 should default to 2
    let md036_l255 = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 255);
    let fixed_l255 = md036_l255.fix(&ctx).unwrap();
    assert_eq!(fixed_l255, "## Title", "Level 255 should default to level 2");
}

#[test]
fn test_md036_config_serde_validation() {
    // Valid heading levels should parse successfully
    for level in 1..=6 {
        let toml_str = format!("heading-level = {level}");
        let config: Result<MD036Config, _> = toml::from_str(&toml_str);
        assert!(
            config.is_ok(),
            "Level {level} should be valid, got error: {:?}",
            config.err()
        );
        assert_eq!(config.unwrap().heading_level.get(), level);
    }

    // Invalid heading levels should fail deserialization
    for level in [0, 7, 10, 100, 255] {
        let toml_str = format!("heading-level = {level}");
        let config: Result<MD036Config, _> = toml::from_str(&toml_str);
        assert!(
            config.is_err(),
            "Level {level} should be rejected during deserialization"
        );
        let err = config.unwrap_err().to_string();
        assert!(
            err.contains("must be between 1 and 6"),
            "Error for level {level} should mention valid range, got: {err}"
        );
    }
}
