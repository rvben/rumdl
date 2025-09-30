use rumdl_lib::config::Config;
use rumdl_lib::rules;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_per_file_ignores_integration_actual_linting() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let readme_path = temp_dir.path().join("README.md");
    let docs_path = temp_dir.path().join("docs.md");

    // Create config with per-file-ignores
    let config_content = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Create markdown files with HTML (violates MD033)
    let readme_content = "# Test\n\n<div>HTML content</div>\n";
    let docs_content = "# Test\n\n<div>HTML content</div>\n";
    fs::write(&readme_path, readme_content).unwrap();
    fs::write(&docs_path, docs_content).unwrap();

    // Load config
    let sourced = rumdl_lib::config::SourcedConfig::load(Some(config_path.to_str().unwrap()), None).unwrap();
    let config: Config = sourced.into();

    // Get all rules
    let all_rules = rules::all_rules(&config);

    // Filter rules for README.md - MD033 should be excluded
    let ignored_readme = config.get_ignored_rules_for_file(Path::new("README.md"));
    let readme_rules: Vec<_> = all_rules
        .iter()
        .filter(|rule| !ignored_readme.contains(rule.name()))
        .collect();

    // Filter rules for docs.md - MD033 should be included
    let ignored_docs = config.get_ignored_rules_for_file(Path::new("docs.md"));
    let docs_rules: Vec<_> = all_rules
        .iter()
        .filter(|rule| !ignored_docs.contains(rule.name()))
        .collect();

    // Verify README.md rules don't include MD033
    assert!(!readme_rules.iter().any(|r| r.name() == "MD033"));

    // Verify docs.md rules include MD033
    assert!(docs_rules.iter().any(|r| r.name() == "MD033"));

    // Lint both files
    let readme_warnings = rumdl_lib::lint(
        readme_content,
        &readme_rules
            .iter()
            .map(|r| dyn_clone::clone_box(&***r))
            .collect::<Vec<_>>(),
        false,
        config.markdown_flavor(),
    )
    .unwrap();
    let docs_warnings = rumdl_lib::lint(
        docs_content,
        &docs_rules
            .iter()
            .map(|r| dyn_clone::clone_box(&***r))
            .collect::<Vec<_>>(),
        false,
        config.markdown_flavor(),
    )
    .unwrap();

    // README should have no MD033 warnings (rule is ignored)
    assert!(!readme_warnings.iter().any(|w| w.rule_name == Some("MD033")));

    // docs.md should have MD033 warnings (rule is active)
    assert!(docs_warnings.iter().any(|w| w.rule_name == Some("MD033")));
}

#[test]
fn test_per_file_ignores_combined_with_global_disable() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Create config with both global disable and per-file-ignores
    let config_content = r#"
[global]
disable = ["MD013"]

[per-file-ignores]
"README.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Load config
    let sourced = rumdl_lib::config::SourcedConfig::load(Some(config_path.to_str().unwrap()), None).unwrap();
    let config: Config = sourced.into();

    // Verify config was loaded correctly
    assert!(config.global.disable.contains(&"MD013".to_string()));
    assert_eq!(
        config.per_file_ignores.get("README.md"),
        Some(&vec!["MD033".to_string()])
    );

    // For README.md, MD033 should be in per-file ignores
    let ignored_readme = config.get_ignored_rules_for_file(Path::new("README.md"));
    assert!(ignored_readme.contains("MD033"));
    assert_eq!(ignored_readme.len(), 1); // Only MD033, not MD013 (that's in global disable)

    // For other files, per-file ignores should be empty
    let ignored_other = config.get_ignored_rules_for_file(Path::new("other.md"));
    assert!(ignored_other.is_empty());
}
