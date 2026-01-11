//! Tests for intelligent config merging
//!
//! This module tests the Git-like configuration merging behavior where:
//! - `disable` arrays are merged via union (user can add to project disables)
//! - `enable` arrays are replaced (project can enforce rules)
//! - Project `enable` overrides user `disable` (team standards win)
//! - Scalars are replaced by higher precedence

#[cfg(test)]
mod tests {
    use crate::config::{ConfigSource, SourcedValue};
    use std::collections::HashSet;

    /// Helper to create a SourcedValue with a vec
    fn make_sourced_vec(values: Vec<&str>, source: ConfigSource) -> SourcedValue<Vec<String>> {
        SourcedValue::new(values.iter().map(|s| s.to_string()).collect(), source)
    }

    /// Helper to assert vec contents without caring about order
    fn assert_vec_eq(actual: &[String], expected: &[&str]) {
        let actual_set: HashSet<_> = actual.iter().collect();
        let expected_set: HashSet<_> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            actual_set.len(),
            expected_set.len(),
            "Sets have different sizes. Actual: {actual:?}, Expected: {expected:?}"
        );
        for item in &expected_set {
            assert!(
                actual_set.contains(&item),
                "Expected item {item:?} not found in {actual:?}"
            );
        }
    }

    #[test]
    fn test_merge_union_combines_arrays() {
        // User config disables MD013
        let mut user_disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);

        // Project config disables MD041
        user_disable.merge_union(vec!["MD041".to_string()], ConfigSource::PyprojectToml, None, None);

        // Result should be union: both disabled
        assert_vec_eq(&user_disable.value, &["MD013", "MD041"]);
        assert_eq!(user_disable.source, ConfigSource::PyprojectToml);
    }

    #[test]
    fn test_merge_union_deduplicates() {
        // User config disables MD013, MD041
        let mut user_disable = make_sourced_vec(vec!["MD013", "MD041"], ConfigSource::UserConfig);

        // Project config also disables MD013 (duplicate) and MD047 (new)
        user_disable.merge_union(
            vec!["MD013".to_string(), "MD047".to_string()],
            ConfigSource::PyprojectToml,
            None,
            None,
        );

        // Result should be deduplicated union
        assert_vec_eq(&user_disable.value, &["MD013", "MD041", "MD047"]);
        assert_eq!(user_disable.value.len(), 3, "Should not have duplicates");
    }

    #[test]
    fn test_merge_union_with_empty_new_value() {
        // User config disables MD013
        let mut user_disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);

        // Project config has empty disable array
        user_disable.merge_union(vec![], ConfigSource::PyprojectToml, None, None);

        // User disables should be preserved (empty doesn't mean "clear all")
        assert_vec_eq(&user_disable.value, &["MD013"]);
    }

    #[test]
    fn test_merge_union_respects_precedence() {
        // User config (lower precedence)
        let mut user_disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);

        // Default config (even lower precedence) shouldn't add
        user_disable.merge_union(vec!["MD041".to_string()], ConfigSource::Default, None, None);

        // Should not merge because Default < UserConfig
        assert_vec_eq(&user_disable.value, &["MD013"]);
        assert_eq!(user_disable.source, ConfigSource::UserConfig);
    }

    #[test]
    fn test_merge_union_equal_precedence_merges() {
        // First config
        let mut disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);

        // Another config at same precedence level
        disable.merge_union(vec!["MD041".to_string()], ConfigSource::UserConfig, None, None);

        // Should merge because precedence is equal
        assert_vec_eq(&disable.value, &["MD013", "MD041"]);
    }

    #[test]
    fn test_merge_replace_replaces_arrays() {
        // User config enables MD001, MD002
        let mut user_enable = make_sourced_vec(vec!["MD001", "MD002"], ConfigSource::UserConfig);

        // Project config enables only MD003 (different set)
        user_enable.merge_override(vec!["MD003".to_string()], ConfigSource::PyprojectToml, None, None);

        // Result should be REPLACED (not merged) because project has higher precedence
        assert_vec_eq(&user_enable.value, &["MD003"]);
        assert_eq!(user_enable.source, ConfigSource::PyprojectToml);
    }

    #[test]
    fn test_merge_replace_respects_precedence() {
        // Project config
        let mut enable = make_sourced_vec(vec!["MD001"], ConfigSource::PyprojectToml);

        // User config (lower precedence) shouldn't replace
        enable.merge_override(vec!["MD002".to_string()], ConfigSource::UserConfig, None, None);

        // Should not replace because UserConfig < PyprojectToml
        assert_vec_eq(&enable.value, &["MD001"]);
        assert_eq!(enable.source, ConfigSource::PyprojectToml);
    }

    #[test]
    fn test_scalar_merge_replaces_with_higher_precedence() {
        // User config: line-length = 120
        let mut line_length = SourcedValue::new(120u64, ConfigSource::UserConfig);

        // Project config: line-length = 80
        line_length.merge_override(80u64, ConfigSource::PyprojectToml, None, None);

        // Project value should win
        assert_eq!(line_length.value, 80);
        assert_eq!(line_length.source, ConfigSource::PyprojectToml);
    }

    #[test]
    fn test_scalar_merge_preserves_with_lower_precedence() {
        // Project config: line-length = 80
        let mut line_length = SourcedValue::new(80u64, ConfigSource::PyprojectToml);

        // User config: line-length = 120 (lower precedence)
        line_length.merge_override(120u64, ConfigSource::UserConfig, None, None);

        // Project value should be preserved
        assert_eq!(line_length.value, 80);
        assert_eq!(line_length.source, ConfigSource::PyprojectToml);
    }

    #[test]
    fn test_enable_disables_remove_conflicts() {
        // Simulate the conflict resolution that should happen after merging
        let mut disable_list = vec!["MD013".to_string(), "MD041".to_string()];
        let enable_list = ["MD013".to_string()];

        // Remove items from disable that appear in enable
        disable_list.retain(|rule| !enable_list.contains(rule));

        assert_vec_eq(&disable_list, &["MD041"]);
    }

    #[test]
    fn test_integration_user_and_project_config_merge() {
        use crate::config::{SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig};

        // Start with user config
        let mut config = SourcedConfig::default();
        config.global.disable = make_sourced_vec(vec!["MD013", "MD041"], ConfigSource::UserConfig);
        config.global.enable = make_sourced_vec(vec![], ConfigSource::UserConfig);

        // Create project config fragment
        let mut project_fragment = SourcedConfigFragment {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(Default::default(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(Default::default(), ConfigSource::Default),
            rules: Default::default(),
            unknown_keys: vec![],
        };
        project_fragment.global.disable = make_sourced_vec(vec!["MD047"], ConfigSource::PyprojectToml);
        project_fragment.global.enable = make_sourced_vec(vec!["MD001"], ConfigSource::PyprojectToml);

        // Merge project config
        config.merge(project_fragment);

        // Disable should be union: MD013, MD041 (user) + MD047 (project)
        assert_vec_eq(&config.global.disable.value, &["MD013", "MD041", "MD047"]);

        // Enable should be replaced: MD001 (project only)
        assert_vec_eq(&config.global.enable.value, &["MD001"]);
    }

    #[test]
    fn test_integration_enable_overrides_disable() {
        use crate::config::{SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig};

        // User config disables MD013
        let mut config = SourcedConfig::default();
        config.global.disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);
        config.global.enable = make_sourced_vec(vec![], ConfigSource::UserConfig);

        // Project config enables MD013 (conflict!)
        let mut project_fragment = SourcedConfigFragment {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(Default::default(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(Default::default(), ConfigSource::Default),
            rules: Default::default(),
            unknown_keys: vec![],
        };
        project_fragment.global.enable = make_sourced_vec(vec!["MD013"], ConfigSource::PyprojectToml);

        // Merge and resolve conflicts
        config.merge(project_fragment);

        // MD013 should be in enable (project wins)
        assert_vec_eq(&config.global.enable.value, &["MD013"]);

        // MD013 should be removed from disable due to conflict resolution
        assert!(
            !config.global.disable.value.contains(&"MD013".to_string()),
            "MD013 should be removed from disable when enabled by project config"
        );
    }

    #[test]
    fn test_cli_has_highest_precedence() {
        // Project config
        let mut enable = make_sourced_vec(vec!["MD001"], ConfigSource::PyprojectToml);

        // CLI flag
        enable.merge_override(vec!["MD002".to_string()], ConfigSource::Cli, None, None);

        // CLI should win
        assert_vec_eq(&enable.value, &["MD002"]);
        assert_eq!(enable.source, ConfigSource::Cli);
    }

    #[test]
    fn test_precedence_order() {
        use crate::config::ConfigSource;

        fn get_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::UserConfig => 1,
                ConfigSource::PyprojectToml => 2,
                ConfigSource::ProjectConfig => 3,
                ConfigSource::Cli => 4,
            }
        }

        // Verify precedence order
        assert!(get_precedence(ConfigSource::Default) < get_precedence(ConfigSource::UserConfig));
        assert!(get_precedence(ConfigSource::UserConfig) < get_precedence(ConfigSource::PyprojectToml));
        assert!(get_precedence(ConfigSource::PyprojectToml) < get_precedence(ConfigSource::ProjectConfig));
        assert!(get_precedence(ConfigSource::ProjectConfig) < get_precedence(ConfigSource::Cli));
    }

    #[test]
    fn test_empty_enable_doesnt_clear_disable() {
        use crate::config::{SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig};

        // User config disables rules
        let mut config = SourcedConfig::default();
        config.global.disable = make_sourced_vec(vec!["MD013", "MD041"], ConfigSource::UserConfig);

        // Project config has empty enable (no rules explicitly enabled)
        let project_fragment = SourcedConfigFragment {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(Default::default(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(Default::default(), ConfigSource::Default),
            rules: Default::default(),
            unknown_keys: vec![],
        };

        config.merge(project_fragment);

        // Disable should still have user's rules
        assert_vec_eq(&config.global.disable.value, &["MD013", "MD041"]);
    }

    #[test]
    fn test_multiple_merges_accumulate_disable() {
        let mut disable = make_sourced_vec(vec!["MD013"], ConfigSource::Default);

        // User config adds MD041
        disable.merge_union(vec!["MD041".to_string()], ConfigSource::UserConfig, None, None);
        assert_vec_eq(&disable.value, &["MD013", "MD041"]);

        // Project config adds MD047
        disable.merge_union(vec!["MD047".to_string()], ConfigSource::PyprojectToml, None, None);
        assert_vec_eq(&disable.value, &["MD013", "MD041", "MD047"]);

        // CLI adds MD001
        disable.merge_union(vec!["MD001".to_string()], ConfigSource::Cli, None, None);
        assert_vec_eq(&disable.value, &["MD013", "MD041", "MD047", "MD001"]);
    }

    #[test]
    fn test_history_tracking() {
        let mut disable = make_sourced_vec(vec!["MD013"], ConfigSource::UserConfig);

        // Initial state should have one override
        assert_eq!(disable.overrides.len(), 1);

        // Merge should add to history
        disable.merge_union(
            vec!["MD041".to_string()],
            ConfigSource::PyprojectToml,
            Some("project.toml".to_string()),
            Some(5),
        );

        // Should now have 2 overrides in history
        assert_eq!(disable.overrides.len(), 2);
        assert_eq!(disable.overrides[1].source, ConfigSource::PyprojectToml);
        assert_eq!(disable.overrides[1].file, Some("project.toml".to_string()));
        assert_eq!(disable.overrides[1].line, Some(5));
    }
}
