use rumdl_lib::config::{Config, GlobalConfig, RuleConfig, RuleRegistry};
use rumdl_lib::rules::{all_rules, filter_rules, opt_in_rules};
use std::collections::{BTreeMap, HashSet};

#[test]
fn test_all_rules_returns_all_rules() {
    let config = Config::default();
    let rules = all_rules(&config);

    // Should return all 71 rules as defined in the RULES array (MD001-MD077)
    assert_eq!(rules.len(), 71);

    // Verify some specific rules are present
    let rule_names: HashSet<String> = rules.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD058"));
    assert!(rule_names.contains("MD025"));
    assert!(rule_names.contains("MD071"));
    assert!(rule_names.contains("MD072"));
    assert!(rule_names.contains("MD073"));
    assert!(rule_names.contains("MD074"));
    assert!(rule_names.contains("MD076"));
}

#[test]
fn test_filter_rules_with_empty_config() {
    let config = Config::default();
    let all = all_rules(&config);
    let global_config = GlobalConfig::default();

    let filtered = filter_rules(&all, &global_config);
    let num_opt_in = opt_in_rules().len();

    // With default config, all non-opt-in rules should be enabled
    assert_eq!(filtered.len(), all.len() - num_opt_in);

    // Opt-in rules should NOT be in the default set
    let filtered_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    for opt_in_name in opt_in_rules() {
        assert!(
            !filtered_names.contains(opt_in_name),
            "Opt-in rule {opt_in_name} should not be in default filter_rules output"
        );
    }
}

#[test]
fn test_filter_rules_disable_specific_rules() {
    let config = Config::default();
    let all = all_rules(&config);
    let num_opt_in = opt_in_rules().len();

    let global_config = GlobalConfig {
        disable: vec!["MD001".to_string(), "MD004".to_string(), "MD003".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should have non-opt-in rules minus 3 disabled ones
    assert_eq!(filtered.len(), all.len() - num_opt_in - 3);

    // Verify disabled rules are not present
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(!rule_names.contains("MD001"));
    assert!(!rule_names.contains("MD004"));
    assert!(!rule_names.contains("MD003"));

    // Verify other rules are still present
    assert!(rule_names.contains("MD005"));
    assert!(rule_names.contains("MD058"));
}

#[test]
fn test_filter_rules_disable_all() {
    let config = Config::default();
    let all = all_rules(&config);

    let global_config = GlobalConfig {
        disable: vec!["all".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should have no rules when all are disabled
    assert_eq!(filtered.len(), 0);
}

#[test]
fn test_filter_rules_disable_all_but_enable_specific() {
    let config = Config::default();
    let all = all_rules(&config);

    let global_config = GlobalConfig {
        disable: vec!["all".to_string()],
        enable: vec!["MD001".to_string(), "MD005".to_string(), "MD010".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should only have the 3 enabled rules
    assert_eq!(filtered.len(), 3);

    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD005"));
    assert!(rule_names.contains("MD010"));

    // Verify other rules are not present
    assert!(!rule_names.contains("MD003"));
    assert!(!rule_names.contains("MD004"));
}

#[test]
fn test_filter_rules_enable_only_specific() {
    let config = Config::default();
    let all = all_rules(&config);

    let global_config = GlobalConfig {
        enable: vec!["MD001".to_string(), "MD004".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should only have the 2 enabled rules
    assert_eq!(filtered.len(), 2);

    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD004"));
    assert!(!rule_names.contains("MD003"));
}

#[test]
fn test_filter_rules_enable_with_disable_override() {
    let config = Config::default();
    let all = all_rules(&config);

    let global_config = GlobalConfig {
        enable: vec!["MD001".to_string(), "MD004".to_string(), "MD003".to_string()],
        disable: vec!["MD004".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should have enabled rules minus disabled ones
    assert_eq!(filtered.len(), 2);

    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(!rule_names.contains("MD004")); // Disabled takes precedence
    assert!(rule_names.contains("MD003"));
}

#[test]
fn test_filter_rules_complex_scenario() {
    let config = Config::default();
    let all = all_rules(&config);

    // Complex scenario: disable multiple rules, enable some that would otherwise be active
    let global_config = GlobalConfig {
        disable: vec![
            "MD001".to_string(),
            "MD003".to_string(),
            "MD004".to_string(),
            "MD005".to_string(),
        ],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Should have non-opt-in rules minus the 4 disabled ones
    let num_opt_in = opt_in_rules().len();
    assert_eq!(filtered.len(), all.len() - num_opt_in - 4);

    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();

    // Verify disabled rules are not present
    assert!(!rule_names.contains("MD001"));
    assert!(!rule_names.contains("MD003"));
    assert!(!rule_names.contains("MD004"));
    assert!(!rule_names.contains("MD005"));

    // Verify some other rules are still present
    assert!(rule_names.contains("MD007"));
    assert!(rule_names.contains("MD010"));
    assert!(rule_names.contains("MD058"));
}

#[test]
fn test_all_rules_consistency() {
    let config = Config::default();
    let rules1 = all_rules(&config);
    let rules2 = all_rules(&config);

    // Multiple calls should return same number of rules
    assert_eq!(rules1.len(), rules2.len());

    // Verify all rule names are unique
    let mut seen_names = HashSet::new();
    for rule in &rules1 {
        let name = rule.name();
        assert!(seen_names.insert(name.to_string()), "Duplicate rule name: {name}");
    }
}

#[test]
fn test_filter_rules_preserves_rule_order() {
    let config = Config::default();
    let all = all_rules(&config);
    let opt_in_set = opt_in_rules();

    // Disable some rules in the middle
    let global_config = GlobalConfig {
        disable: vec!["MD010".to_string(), "MD020".to_string(), "MD030".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // Check that remaining rules maintain their relative order
    // (excluding opt-in rules which are filtered out by default)
    let all_names: Vec<String> = all
        .iter()
        .map(|r| r.name().to_string())
        .filter(|name| !global_config.disable.contains(name) && !opt_in_set.contains(name.as_str()))
        .collect();

    let filtered_names: Vec<String> = filtered.iter().map(|r| r.name().to_string()).collect();

    assert_eq!(all_names, filtered_names);
}

#[test]
fn test_filter_rules_enable_all_keyword() {
    let config = Config::default();
    let all = all_rules(&config);
    let total = all.len();

    let global_config = GlobalConfig {
        enable: vec!["ALL".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // enable: ["ALL"] should enable all rules
    assert_eq!(filtered.len(), total);
}

#[test]
fn test_filter_rules_enable_all_with_disable() {
    let config = Config::default();
    let all = all_rules(&config);
    let total = all.len();

    let global_config = GlobalConfig {
        enable: vec!["ALL".to_string()],
        disable: vec!["MD013".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);

    // enable: ["ALL"] + disable: ["MD013"] → all rules minus MD013
    assert_eq!(filtered.len(), total - 1);

    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(!rule_names.contains("MD013"));
    assert!(rule_names.contains("MD001"));
}

#[test]
fn test_filter_rules_enable_all_case_insensitive() {
    let config = Config::default();
    let all = all_rules(&config);
    let total = all.len();

    // Test lowercase "all"
    let global_config = GlobalConfig {
        enable: vec!["all".to_string()],
        ..Default::default()
    };
    let filtered = filter_rules(&all, &global_config);
    assert_eq!(filtered.len(), total);

    // Test mixed case "All"
    let global_config = GlobalConfig {
        enable: vec!["All".to_string()],
        ..Default::default()
    };
    let filtered = filter_rules(&all, &global_config);
    assert_eq!(filtered.len(), total);
}

#[test]
fn test_filter_rules_enable_all_overrides_disable_all() {
    let config = Config::default();
    let all = all_rules(&config);
    let total = all.len();

    // enable: ["ALL"] + disable: ["all"] → all rules enabled
    let global_config = GlobalConfig {
        enable: vec!["ALL".to_string()],
        disable: vec!["all".to_string()],
        ..Default::default()
    };

    let filtered = filter_rules(&all, &global_config);
    assert_eq!(filtered.len(), total);
}

#[test]
fn test_filter_rules_empty_enable_returns_non_opt_in() {
    // With the default GlobalConfig (enable not explicitly set),
    // all non-opt-in rules should be returned
    let config = Config::default();
    let all = all_rules(&config);
    let num_opt_in = opt_in_rules().len();
    let global_config = GlobalConfig::default();

    let filtered = filter_rules(&all, &global_config);
    assert_eq!(filtered.len(), all.len() - num_opt_in);
}

/// Every rule with configurable options must implement `default_config_section()`
/// so the RuleRegistry knows which config keys are valid. Without it, user-supplied
/// config keys produce false "unknown option" warnings.
///
/// This test catches the class of bug where a rule has a config struct but forgets
/// to implement `default_config_section()`. If the count drops, a rule lost its
/// config section.
#[test]
fn test_all_configurable_rules_expose_config_schema() {
    let config = Config::default();
    let rules = all_rules(&config);
    let registry = RuleRegistry::from_rules(&rules);

    // Collect rules that declare config keys
    let mut rules_with_config = Vec::new();
    let mut rules_without_config = Vec::new();

    for rule in &rules {
        let name = rule.name().to_string();
        if rule.default_config_section().is_some() {
            rules_with_config.push(name);
        } else {
            rules_without_config.push(name);
        }
    }

    // Verify the registry has a non-empty schema for rules that declared config.
    // The registry uses normalized keys (MD001 stays MD001 via normalize_key).
    for name in &rules_with_config {
        assert!(
            registry.rule_schemas.contains_key(name.as_str()),
            "Registry missing schema for configurable rule {name}"
        );
    }

    // Guard against regressions: if this count drops, a rule lost its config.
    // Update this number when adding new configurable rules.
    assert_eq!(
        rules_with_config.len(),
        46,
        "Expected 46 rules with config sections. If you added config to a rule, \
         implement default_config_section(). Rules with config: {rules_with_config:?}"
    );
}

#[test]
fn test_promote_opt_in_enabled_adds_to_extend_enable() {
    let mut config = Config::default();

    // Simulate what the WASM Linter constructor does when parsing
    // `[MD060] enabled = true` from a .rumdl.toml config
    let mut values = BTreeMap::new();
    values.insert("enabled".to_string(), toml::Value::Boolean(true));
    values.insert(
        "style".to_string(),
        toml::Value::String("aligned".to_string()),
    );
    config
        .rules
        .insert("MD060".to_string(), RuleConfig { severity: None, values });

    assert!(
        !config.global.extend_enable.contains(&"MD060".to_string()),
        "MD060 should not be in extend_enable before promotion"
    );

    config.promote_enabled_to_extend_enable();

    assert!(
        config.global.extend_enable.contains(&"MD060".to_string()),
        "MD060 should be in extend_enable after promotion"
    );

    // Verify filter_rules now includes MD060
    let all = all_rules(&config);
    let filtered = filter_rules(&all, &config.global);
    let names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(
        names.contains("MD060"),
        "MD060 should be included by filter_rules after promotion"
    );
}

#[test]
fn test_promote_opt_in_enabled_skips_when_not_enabled() {
    let mut config = Config::default();

    let mut values = BTreeMap::new();
    values.insert("enabled".to_string(), toml::Value::Boolean(false));
    config
        .rules
        .insert("MD060".to_string(), RuleConfig { severity: None, values });

    config.promote_enabled_to_extend_enable();

    assert!(
        !config.global.extend_enable.contains(&"MD060".to_string()),
        "MD060 should NOT be promoted when enabled=false"
    );
}

#[test]
fn test_promote_opt_in_enabled_no_duplicate_when_already_extended() {
    let mut config = Config::default();
    config.global.extend_enable.push("MD060".to_string());

    let mut values = BTreeMap::new();
    values.insert("enabled".to_string(), toml::Value::Boolean(true));
    config
        .rules
        .insert("MD060".to_string(), RuleConfig { severity: None, values });

    config.promote_enabled_to_extend_enable();

    let count = config
        .global
        .extend_enable
        .iter()
        .filter(|s| *s == "MD060")
        .count();
    assert_eq!(count, 1, "MD060 should not be duplicated in extend_enable");
}

#[test]
fn test_promote_enabled_harmless_for_non_opt_in_rules() {
    // promote_enabled_to_extend_enable adds ALL rules with enabled=true,
    // but filter_rules only consults extend_enable for opt-in rules,
    // so adding a non-opt-in rule to extend_enable is harmless.
    let mut config = Config::default();

    let mut values = BTreeMap::new();
    values.insert("enabled".to_string(), toml::Value::Boolean(true));
    config
        .rules
        .insert("MD001".to_string(), RuleConfig { severity: None, values });

    config.promote_enabled_to_extend_enable();

    // MD001 IS added to extend_enable (the method promotes all enabled=true rules)
    assert!(config.global.extend_enable.contains(&"MD001".to_string()));

    // But filter_rules still includes MD001 regardless (it's not opt-in)
    let all = all_rules(&config);
    let filtered = filter_rules(&all, &config.global);
    let names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(names.contains("MD001"), "MD001 should be included (non-opt-in, always active)");
}

#[test]
fn test_promote_opt_in_md060_fix_produces_aligned_table() {
    // End-to-end test: simulates the WASM fix path for obsidian-rumdl issue #15
    let mut config = Config::default();
    config.global.disable.push("MD041".to_string());

    let mut values = BTreeMap::new();
    values.insert("enabled".to_string(), toml::Value::Boolean(true));
    values.insert(
        "style".to_string(),
        toml::Value::String("aligned".to_string()),
    );
    config
        .rules
        .insert("MD060".to_string(), RuleConfig { severity: None, values });

    config.promote_enabled_to_extend_enable();

    let all = all_rules(&config);
    let rules = filter_rules(&all, &config.global);

    let content = "|Column 1 |Column 2|\n|:--|--:|\n|Test|Val |\n|New|Val|\n";

    let warnings = rumdl_lib::lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Obsidian,
        Some(&config),
    )
    .unwrap();

    let has_md060 = warnings.iter().any(|w| {
        w.rule_name
            .as_ref()
            .is_some_and(|name| name == "MD060")
    });
    assert!(
        has_md060,
        "Should detect MD060 warnings for unaligned table"
    );
}

#[test]
fn test_extend_enable_includes_opt_in_rules_in_filter() {
    // Simulates the recommended `extend-enable = ["MD060"]` config path
    let mut config = Config::default();
    config.global.extend_enable.push("MD060".to_string());

    let mut values = BTreeMap::new();
    values.insert(
        "style".to_string(),
        toml::Value::String("aligned".to_string()),
    );
    config
        .rules
        .insert("MD060".to_string(), RuleConfig { severity: None, values });

    let all = all_rules(&config);
    let rules = filter_rules(&all, &config.global);
    let names: HashSet<String> = rules.iter().map(|r| r.name().to_string()).collect();

    assert!(
        names.contains("MD060"),
        "MD060 should be included when in extend_enable"
    );
}
