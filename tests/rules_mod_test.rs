use rumdl::config::{Config, GlobalConfig};
use rumdl::rules::{all_rules, filter_rules};
use std::collections::HashSet;

#[test]
fn test_all_rules_returns_all_rules() {
    let config = Config::default();
    let rules = all_rules(&config);
    
    // Should return all 54 rules as defined in the RULES array
    assert_eq!(rules.len(), 54);
    
    // Verify some specific rules are present
    let rule_names: HashSet<String> = rules.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD058"));
    assert!(rule_names.contains("MD025"));
}

#[test]
fn test_filter_rules_with_empty_config() {
    let config = Config::default();
    let all = all_rules(&config);
    let global_config = GlobalConfig::default();
    
    let filtered = filter_rules(&all, &global_config);
    
    // With default config, all rules should be enabled
    assert_eq!(filtered.len(), all.len());
}

#[test]
fn test_filter_rules_disable_specific_rules() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    global_config.disable = vec!["MD001".to_string(), "MD002".to_string(), "MD003".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should have 3 fewer rules
    assert_eq!(filtered.len(), all.len() - 3);
    
    // Verify disabled rules are not present
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(!rule_names.contains("MD001"));
    assert!(!rule_names.contains("MD002"));
    assert!(!rule_names.contains("MD003"));
    
    // Verify other rules are still present
    assert!(rule_names.contains("MD004"));
    assert!(rule_names.contains("MD058"));
}

#[test]
fn test_filter_rules_disable_all() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    global_config.disable = vec!["all".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should have no rules when all are disabled
    assert_eq!(filtered.len(), 0);
}

#[test]
fn test_filter_rules_disable_all_but_enable_specific() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    global_config.disable = vec!["all".to_string()];
    global_config.enable = vec!["MD001".to_string(), "MD005".to_string(), "MD010".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should only have the 3 enabled rules
    assert_eq!(filtered.len(), 3);
    
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD005"));
    assert!(rule_names.contains("MD010"));
    
    // Verify other rules are not present
    assert!(!rule_names.contains("MD002"));
    assert!(!rule_names.contains("MD003"));
    assert!(!rule_names.contains("MD004"));
}

#[test]
fn test_filter_rules_enable_only_specific() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    global_config.enable = vec!["MD001".to_string(), "MD002".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should only have the 2 enabled rules
    assert_eq!(filtered.len(), 2);
    
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(rule_names.contains("MD002"));
    assert!(!rule_names.contains("MD003"));
}

#[test]
fn test_filter_rules_enable_with_disable_override() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    global_config.enable = vec!["MD001".to_string(), "MD002".to_string(), "MD003".to_string()];
    global_config.disable = vec!["MD002".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should have enabled rules minus disabled ones
    assert_eq!(filtered.len(), 2);
    
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    assert!(rule_names.contains("MD001"));
    assert!(!rule_names.contains("MD002")); // Disabled takes precedence
    assert!(rule_names.contains("MD003"));
}

#[test]
fn test_filter_rules_complex_scenario() {
    let config = Config::default();
    let all = all_rules(&config);
    
    // Complex scenario: disable multiple rules, enable some that would otherwise be active
    let mut global_config = GlobalConfig::default();
    global_config.disable = vec![
        "MD001".to_string(),
        "MD002".to_string(), 
        "MD003".to_string(),
        "MD004".to_string(),
        "MD005".to_string(),
    ];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Should have all rules minus the 5 disabled ones
    assert_eq!(filtered.len(), all.len() - 5);
    
    let rule_names: HashSet<String> = filtered.iter().map(|r| r.name().to_string()).collect();
    
    // Verify disabled rules are not present
    for i in 1..=5 {
        assert!(!rule_names.contains(&format!("MD00{}", i)));
    }
    
    // Verify some other rules are still present
    assert!(rule_names.contains("MD006"));
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
        assert!(seen_names.insert(name.to_string()), "Duplicate rule name: {}", name);
    }
}

#[test]
fn test_filter_rules_preserves_rule_order() {
    let config = Config::default();
    let all = all_rules(&config);
    
    let mut global_config = GlobalConfig::default();
    // Disable some rules in the middle
    global_config.disable = vec!["MD010".to_string(), "MD020".to_string(), "MD030".to_string()];
    
    let filtered = filter_rules(&all, &global_config);
    
    // Check that remaining rules maintain their relative order
    let all_names: Vec<String> = all.iter()
        .map(|r| r.name().to_string())
        .filter(|name| !global_config.disable.contains(name))
        .collect();
        
    let filtered_names: Vec<String> = filtered.iter()
        .map(|r| r.name().to_string())
        .collect();
        
    assert_eq!(all_names, filtered_names);
}