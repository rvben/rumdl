//! Tests for unfixable rules functionality (Issue #56)

use rumdl_lib::config::{Config, GlobalConfig};
use rumdl_lib::rule::{FixCapability, Rule};
use rumdl_lib::rules::MD018NoMissingSpaceAtx;
use rumdl_lib::rules::MD033NoInlineHtml;
use rumdl_lib::rules::MD054LinkImageStyle;

#[test]
fn test_inherently_unfixable_rules_declare_capability() {
    // MD054 is inherently unfixable
    let rule = MD054LinkImageStyle::new(true, true, true, true, true, true);
    assert_eq!(rule.fix_capability(), FixCapability::Unfixable);

    // MD033 is inherently unfixable
    let rule = MD033NoInlineHtml::new();
    assert_eq!(rule.fix_capability(), FixCapability::Unfixable);
}

#[test]
fn test_fixable_rules_declare_capability() {
    // MD018 is fixable
    let rule = MD018NoMissingSpaceAtx::new();
    assert_eq!(rule.fix_capability(), FixCapability::FullyFixable);
}

#[test]
fn test_unfixable_config_prevents_fixing() {
    let _content = "##Heading without space\n[empty link]()\n";

    // Create config with MD018 marked as unfixable
    let config = Config {
        global: GlobalConfig {
            unfixable: vec!["MD018".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };

    // The helper function should identify MD018 as unfixable
    assert!(!is_rule_actually_fixable(&config, "MD018"));
    assert!(is_rule_actually_fixable(&config, "MD042"));
}

#[test]
fn test_fixable_list_restricts_fixing() {
    let _content = "##Heading without space\n[empty link]()\n";

    // Create config with only MD042 in fixable list
    let config = Config {
        global: GlobalConfig {
            fixable: vec!["MD042".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };

    // Only MD042 should be fixable
    assert!(!is_rule_actually_fixable(&config, "MD018"));
    assert!(is_rule_actually_fixable(&config, "MD042"));
    assert!(!is_rule_actually_fixable(&config, "MD047"));
}

#[test]
fn test_unfixable_takes_precedence_over_fixable() {
    // If a rule is in both lists, unfixable takes precedence
    let config = Config {
        global: GlobalConfig {
            unfixable: vec!["MD018".to_string()],
            fixable: vec!["MD018".to_string(), "MD042".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };

    // MD018 should be unfixable (unfixable takes precedence)
    assert!(!is_rule_actually_fixable(&config, "MD018"));
    // MD042 should be fixable (only in fixable list)
    assert!(is_rule_actually_fixable(&config, "MD042"));
}

#[test]
fn test_case_insensitive_rule_names() {
    let config = Config {
        global: GlobalConfig {
            unfixable: vec!["md018".to_string()], // lowercase
            ..Default::default()
        },
        ..Default::default()
    };

    // Should match regardless of case
    assert!(!is_rule_actually_fixable(&config, "MD018"));
    assert!(!is_rule_actually_fixable(&config, "md018"));
    assert!(!is_rule_actually_fixable(&config, "Md018"));
}

#[test]
fn test_empty_config_allows_all_fixes() {
    let config = Config {
        global: GlobalConfig {
            unfixable: vec![],
            fixable: vec![],
            ..Default::default()
        },
        ..Default::default()
    };

    // Everything should be fixable with empty config
    assert!(is_rule_actually_fixable(&config, "MD018"));
    assert!(is_rule_actually_fixable(&config, "MD042"));
    assert!(is_rule_actually_fixable(&config, "MD047"));
}

// Helper function copied from main.rs for testing
fn is_rule_actually_fixable(config: &Config, rule_name: &str) -> bool {
    // Check unfixable list
    if config
        .global
        .unfixable
        .iter()
        .any(|r| r.eq_ignore_ascii_case(rule_name))
    {
        return false;
    }

    // Check fixable list if specified
    if !config.global.fixable.is_empty() {
        return config.global.fixable.iter().any(|r| r.eq_ignore_ascii_case(rule_name));
    }

    true
}

#[test]
fn test_fix_count_excludes_unfixable() {
    let config = Config {
        global: GlobalConfig {
            unfixable: vec!["MD042".to_string()], // Mark MD042 as unfixable
            ..Default::default()
        },
        ..Default::default()
    };

    // Simulate warnings that would be detected from markdown content
    // Each tuple represents (rule_name, has_fix_available)
    let warnings = [
        ("MD018", true), // Missing space after ## - fixable
        ("MD012", true), // Multiple blank lines - fixable
        ("MD042", true), // Empty link - has fix but unfixable by config
        ("MD047", true), // No final newline - fixable
    ];

    // Count fixable warnings (should exclude MD042)
    let fixable_count = warnings
        .iter()
        .filter(|(rule, has_fix)| *has_fix && is_rule_actually_fixable(&config, rule))
        .count();

    assert_eq!(fixable_count, 3); // Should be 3, not 4 (MD042 excluded)
}
