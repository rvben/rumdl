use rumdl_lib::config::Config;
use rumdl_lib::rules;
use serial_test::serial;
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
    let config: Config = sourced.into_validated_unchecked().into();

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
        None,
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
        None,
    )
    .unwrap();

    // README should have no MD033 warnings (rule is ignored)
    assert!(!readme_warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD033")));

    // docs.md should have MD033 warnings (rule is active)
    assert!(docs_warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD033")));
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
    let config: Config = sourced.into_validated_unchecked().into();

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

/// Test for issue #246: per-file-ignores doesn't work when config is in a subdirectory
/// and loaded via relative path (e.g., `--config .config/rumdl.toml`)
#[test]
fn test_per_file_ignores_config_in_subdirectory() {
    let temp_dir = tempdir().unwrap();

    // Create .config subdirectory
    let config_dir = temp_dir.path().join(".config");
    fs::create_dir(&config_dir).unwrap();

    // Create config file in subdirectory
    let config_path = config_dir.join("rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"LICENSE.md" = ["MD050"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Create a markdown file that violates MD050
    let license_path = temp_dir.path().join("LICENSE.md");
    let license_content = "# License\n\n__Bold text__ with underscore.\n";
    fs::write(&license_path, license_content).unwrap();

    // Create .git directory to simulate a project root
    let git_dir = temp_dir.path().join(".git");
    fs::create_dir(&git_dir).unwrap();

    // Load config using the absolute path (simulating --config .config/rumdl.toml)
    let sourced = rumdl_lib::config::SourcedConfig::load(Some(config_path.to_str().unwrap()), None).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify config was loaded with correct project_root
    assert!(config.project_root.is_some(), "project_root should be set");
    let project_root = config.project_root.as_ref().unwrap();
    // The project root should be the directory containing .git, not the .config dir
    assert!(
        project_root.ends_with(temp_dir.path().file_name().unwrap()),
        "project_root should point to temp dir (containing .git), not .config"
    );

    // Verify per-file-ignores is loaded
    assert!(
        config.per_file_ignores.contains_key("LICENSE.md"),
        "per_file_ignores should contain LICENSE.md"
    );

    // Test that the rule is actually ignored for LICENSE.md
    let ignored_rules = config.get_ignored_rules_for_file(&license_path);
    assert!(
        ignored_rules.contains("MD050"),
        "MD050 should be ignored for LICENSE.md, but ignored_rules = {ignored_rules:?}"
    );

    // Test that other files don't have MD050 ignored
    let other_path = temp_dir.path().join("README.md");
    let ignored_rules_other = config.get_ignored_rules_for_file(&other_path);
    assert!(
        !ignored_rules_other.contains("MD050"),
        "MD050 should NOT be ignored for README.md"
    );
}

/// Test for issue #246: Verify relative path handling in find_project_root_from
/// This test changes the current directory to exercise the relative path code path
#[test]
#[serial]
fn test_per_file_ignores_with_actual_relative_path() {
    let temp_dir = tempdir().unwrap();

    // Create .config subdirectory with config file
    let config_dir = temp_dir.path().join(".config");
    fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"CHANGELOG.md" = ["MD024"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Create .git directory to mark project root
    let git_dir = temp_dir.path().join(".git");
    fs::create_dir(&git_dir).unwrap();

    // Create a markdown file
    let changelog_path = temp_dir.path().join("CHANGELOG.md");
    fs::write(&changelog_path, "# Changelog\n").unwrap();

    // Save original directory
    let original_dir = std::env::current_dir().unwrap();

    // Change to temp directory and use RELATIVE path
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Use relative path ".config/rumdl.toml" - this exercises the bug fix
    let relative_config_path = ".config/rumdl.toml";
    let load_result = rumdl_lib::config::SourcedConfig::load(Some(relative_config_path), None);

    // Restore original directory before any assertions (cleanup on panic)
    std::env::set_current_dir(&original_dir).unwrap();

    // Now check results
    let sourced = load_result.expect("Should load config from relative path");
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify project_root is set and points to the temp dir (not empty string!)
    assert!(
        config.project_root.is_some(),
        "project_root should be set when loading from relative path"
    );
    let project_root = config.project_root.as_ref().unwrap();
    assert!(
        !project_root.as_os_str().is_empty(),
        "project_root should NOT be empty string (this was the bug)"
    );
    assert!(
        project_root.join(".git").exists() || project_root == temp_dir.path(),
        "project_root should be the directory containing .git"
    );

    // Verify per-file-ignores works with the correctly resolved project_root
    assert!(
        config.per_file_ignores.contains_key("CHANGELOG.md"),
        "per_file_ignores should contain CHANGELOG.md"
    );

    // Test pattern matching works (this failed with empty project_root)
    let ignored_rules = config.get_ignored_rules_for_file(&changelog_path);
    assert!(
        ignored_rules.contains("MD024"),
        "MD024 should be ignored for CHANGELOG.md, but ignored_rules = {ignored_rules:?}"
    );
}

/// Test the edge case where relative path parent is empty string
/// This directly tests the scenario that caused issue #246
#[test]
#[serial]
fn test_relative_path_with_single_component() {
    let temp_dir = tempdir().unwrap();

    // Create config directly in a subdirectory (simulating ".config" as the parent)
    let config_dir = temp_dir.path().join("configs");
    fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("lint.toml");
    let config_content = r#"
[per-file-ignores]
"docs/*.md" = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Create .git in temp_dir
    fs::create_dir(temp_dir.path().join(".git")).unwrap();

    // Create a docs directory with a markdown file
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir(&docs_dir).unwrap();
    let readme_path = docs_dir.join("README.md");
    fs::write(&readme_path, "# Docs\n").unwrap();

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Load with relative path "configs/lint.toml"
    // Before the fix: Path::new("configs/lint.toml").parent() = "configs"
    // find_project_root_from("configs") would traverse: "configs" -> "" -> panic/wrong behavior
    let load_result = rumdl_lib::config::SourcedConfig::load(Some("configs/lint.toml"), None);

    std::env::set_current_dir(&original_dir).unwrap();

    let sourced = load_result.expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // The critical assertion: project_root should be valid, not empty
    let project_root = config.project_root.as_ref().expect("project_root should be set");
    assert!(
        !project_root.as_os_str().is_empty(),
        "project_root must not be empty string"
    );

    // Glob pattern matching should work
    let ignored = config.get_ignored_rules_for_file(&readme_path);
    assert!(
        ignored.contains("MD013"),
        "MD013 should be ignored for docs/README.md via glob pattern"
    );
}
