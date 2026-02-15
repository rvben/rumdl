//! Tests for per-directory configuration resolution (issue #390).
//!
//! Verifies that files in subdirectories pick up config files from
//! their subdirectory when running `rumdl check .` from the project root.

use rumdl_lib::config::SourcedConfig;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

/// Helper to create a file with parent directories
fn create_file(root: &Path, relative_path: &str, content: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

// ─── discover_config_for_dir() tests ───

#[test]
fn test_discover_config_no_subdirectory_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Only root config
    create_file(root, ".rumdl.toml", "[global]\n");
    create_file(root, "docs/guide.md", "# Guide\n");

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    // Should find the root config
    assert_eq!(result.unwrap(), root.join(".rumdl.toml"));
}

#[test]
fn test_discover_config_subdirectory_config_found() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(root, ".rumdl.toml", "[global]\nline-length = 80\n");
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    assert_eq!(result.unwrap(), root.join("docs/.rumdl.toml"));
}

#[test]
fn test_discover_config_nested_inherits_parent_subdir() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(root, ".rumdl.toml", "[global]\n");
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");
    create_file(root, "docs/api/endpoint.md", "# API\n");

    // docs/api/ should find docs/.rumdl.toml (walks up within project root)
    let result = SourcedConfig::discover_config_for_dir(&root.join("docs/api"), root);
    assert_eq!(result.unwrap(), root.join("docs/.rumdl.toml"));
}

#[test]
fn test_discover_config_stops_at_project_root() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Create a config ABOVE root (simulating a parent project)
    // This should NOT be found since we bound by project_root
    let parent_dir = root.join("parent");
    let project_root = parent_dir.join("project");
    create_file(&parent_dir, ".rumdl.toml", "[global]\n");
    fs::create_dir_all(&project_root).unwrap();

    let result = SourcedConfig::discover_config_for_dir(&project_root, &project_root);
    // No config within project root boundary
    assert!(result.is_none());
}

#[test]
fn test_discover_config_pyproject_toml_with_tool_rumdl() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(
        root,
        "docs/pyproject.toml",
        "[tool.rumdl]\n[tool.rumdl.global]\nline-length = 120\n",
    );

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    assert_eq!(result.unwrap(), root.join("docs/pyproject.toml"));
}

#[test]
fn test_discover_config_pyproject_toml_without_tool_rumdl_skipped() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // pyproject.toml without [tool.rumdl] should be skipped
    create_file(root, "docs/pyproject.toml", "[project]\nname = \"foo\"\n");
    create_file(root, ".rumdl.toml", "[global]\n");

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    // Should skip pyproject.toml without [tool.rumdl] and find root config
    assert_eq!(result.unwrap(), root.join(".rumdl.toml"));
}

#[test]
fn test_discover_config_markdownlint_fallback() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Only markdownlint config in subdirectory
    create_file(root, "docs/.markdownlint.json", "{\"MD013\": false}\n");

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    assert_eq!(result.unwrap(), root.join("docs/.markdownlint.json"));
}

#[test]
fn test_discover_config_rumdl_takes_precedence_over_markdownlint() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(root, "docs/.rumdl.toml", "[global]\n");
    create_file(root, "docs/.markdownlint.json", "{}\n");

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    // rumdl config has higher precedence
    assert_eq!(result.unwrap(), root.join("docs/.rumdl.toml"));
}

#[test]
fn test_discover_config_no_config_at_all() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join("docs")).unwrap();

    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    assert!(result.is_none());
}

#[test]
fn test_discover_config_dot_config_dir_found() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Config in .config/ subdirectory (a root-level config location)
    create_file(root, ".config/rumdl.toml", "[global]\nline-length = 100\n");

    // Root-level files should find .config/rumdl.toml
    let result = SourcedConfig::discover_config_for_dir(root, root);
    assert_eq!(result.unwrap(), root.join(".config/rumdl.toml"));

    // Files in subdirectories without their own config should also find it
    fs::create_dir_all(root.join("docs")).unwrap();
    let result = SourcedConfig::discover_config_for_dir(&root.join("docs"), root);
    assert_eq!(result.unwrap(), root.join(".config/rumdl.toml"));
}

// ─── load_config_for_path() tests ───

#[test]
fn test_load_config_for_path_rumdl_toml() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(
        root,
        "docs/.rumdl.toml",
        "[global]\nline-length = 120\ndisable = [\"MD013\"]\n",
    );

    let config = SourcedConfig::load_config_for_path(&root.join("docs/.rumdl.toml"), root).unwrap();
    assert_eq!(config.global.line_length.get(), 120);
    assert!(config.global.disable.contains(&"MD013".to_string()));
}

#[test]
fn test_load_config_for_path_with_extends() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(root, ".rumdl.toml", "[global]\nline-length = 80\n");
    create_file(
        root,
        "docs/.rumdl.toml",
        "extends = \"../.rumdl.toml\"\n\n[global]\ndisable = [\"MD013\"]\n",
    );

    let config = SourcedConfig::load_config_for_path(&root.join("docs/.rumdl.toml"), root).unwrap();
    // Should inherit line-length from parent and add disable from child
    assert_eq!(config.global.line_length.get(), 80);
    assert!(config.global.disable.contains(&"MD013".to_string()));
}

#[test]
fn test_load_config_for_path_markdownlint_json() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    create_file(
        root,
        "docs/.markdownlint.json",
        "{\"MD013\": false, \"line-length\": false}\n",
    );

    let config = SourcedConfig::load_config_for_path(&root.join("docs/.markdownlint.json"), root).unwrap();
    // MD013 should be disabled
    assert!(config.global.disable.contains(&"MD013".to_string()));
}

// ─── Integration tests with CLI ───

#[test]
fn test_per_directory_config_integration() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // Create project structure with .git dir (project root boundary)
    fs::create_dir_all(root.join(".git")).unwrap();

    // Root config: line-length = 40
    create_file(root, ".rumdl.toml", "[global]\nline-length = 40\n");

    // Docs config: line-length = 120 (more lenient)
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");

    // Create test files with long lines (realistic text so MD013 non-strict doesn't forgive)
    // 50 chars of real text with spaces to avoid the "single token" exemption
    let long_line = "This is a line with real words that exceeds the configured limit easily.";
    create_file(root, "README.md", &format!("# README\n\n{long_line}\n"));
    create_file(root, "docs/guide.md", &format!("# Guide\n\n{long_line}\n"));

    // Build and run rumdl on both files
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // README.md should have MD013 warning (72 chars > 40 limit)
    assert!(
        combined.contains("README.md") && combined.contains("MD013"),
        "Expected MD013 warning for README.md with root config (line-length=40), got:\n{combined}"
    );

    // docs/guide.md should NOT have MD013 warning (72 chars < 120 limit)
    let has_docs_md013 = combined
        .lines()
        .any(|line| line.contains("docs/guide.md") && line.contains("MD013"));
    assert!(
        !has_docs_md013,
        "Expected no MD013 warning for docs/guide.md with docs config (line-length=120), got:\n{combined}"
    );
}

#[test]
fn test_explicit_config_overrides_per_directory() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join(".git")).unwrap();

    // Root config: line-length = 40
    create_file(root, ".rumdl.toml", "[global]\nline-length = 40\n");

    // Docs config: line-length = 120
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");

    // Explicit config: line-length = 200
    create_file(root, "custom.toml", "[global]\nline-length = 200\n");

    // Use realistic text that exceeds 120 chars but is under 200
    let long_line = "This is a very long line with real words that definitely exceeds the default eighty character line length limit for the MD013 rule check.";
    create_file(root, "docs/guide.md", &format!("# Guide\n\n{long_line}\n"));

    // With --config, the explicit config should override per-directory discovery
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--config", "custom.toml", "docs/guide.md"])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should NOT have MD013 warning (line is ~137 chars < 200 limit)
    let has_md013 = combined.lines().any(|line| line.contains("MD013"));
    assert!(
        !has_md013,
        "Expected no MD013 with explicit config (line-length=200), got:\n{combined}"
    );
}

#[test]
fn test_isolated_mode_ignores_per_directory_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join(".git")).unwrap();

    // Docs config: disable MD013
    create_file(root, "docs/.rumdl.toml", "[global]\ndisable = [\"MD013\"]\n");

    // Create a line that exceeds default line-length (80) to trigger MD013
    let long_line = "This is a very long line with real words that definitely exceeds the default eighty character line length limit for the MD013 rule check.";
    create_file(root, "docs/guide.md", &format!("# Guide\n\n{long_line}\n"));

    // With --isolated, per-directory config should be ignored
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "docs/guide.md"])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should have MD013 warning since isolated mode uses defaults (line-length=80)
    assert!(
        combined.contains("MD013"),
        "Expected MD013 with isolated mode (defaults), got:\n{combined}"
    );
}

#[test]
fn test_no_project_root_uses_single_config() {
    // When there's no .git directory, per-directory config resolution
    // falls back to single-config behavior
    let temp = tempdir().unwrap();
    let root = temp.path();

    // No .git directory
    create_file(root, ".rumdl.toml", "[global]\nline-length = 40\n");
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");

    let long_line = "This is a line that has enough words to fill up more than forty characters easily.";
    create_file(root, "docs/guide.md", &format!("# Guide\n\n{long_line}\n"));

    // Without .git, the root config discovery may or may not find configs
    // depending on CWD. The key invariant is it shouldn't crash.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "docs/guide.md"])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    // Just verify it runs without error
    assert!(
        output.status.success() || output.status.code() == Some(1),
        "Expected successful run or lint violations exit code, got: {:?}",
        output.status
    );
}

#[test]
fn test_per_directory_config_controls_cross_file_checks() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join(".git")).unwrap();

    // Root config: all rules enabled (default)
    create_file(root, ".rumdl.toml", "[global]\n");

    // Docs config: disable MD051 (link fragment validation)
    create_file(root, "docs/.rumdl.toml", "[global]\ndisable = [\"MD051\"]\n");

    // Root file: has a broken fragment link → should get MD051 warning
    create_file(
        root,
        "README.md",
        "# README\n\n[link to nonexistent heading](#does-not-exist)\n",
    );

    // Docs file: has a broken fragment link → should NOT get MD051 (disabled in docs config)
    create_file(
        root,
        "docs/guide.md",
        "# Guide\n\n[link to nonexistent heading](#does-not-exist)\n",
    );

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // README.md should have MD051 warning (root config has MD051 enabled)
    let has_readme_md051 = combined
        .lines()
        .any(|line| line.contains("README.md") && line.contains("MD051"));
    assert!(
        has_readme_md051,
        "Expected MD051 warning for README.md (root config), got:\n{combined}"
    );

    // docs/guide.md should NOT have MD051 warning (docs config disables MD051)
    let has_docs_md051 = combined
        .lines()
        .any(|line| line.contains("docs/guide.md") && line.contains("MD051"));
    assert!(
        !has_docs_md051,
        "Expected no MD051 warning for docs/guide.md (docs config disables MD051), got:\n{combined}"
    );
}

#[test]
fn test_dot_config_dir_treated_as_root_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join(".git")).unwrap();

    // Config only in .config/ subdirectory (root-level location)
    create_file(root, ".config/rumdl.toml", "[global]\nline-length = 40\n");

    // Docs config: line-length = 120
    create_file(root, "docs/.rumdl.toml", "[global]\nline-length = 120\n");

    let long_line = "This is a line with real words that exceeds the configured limit easily.";
    create_file(root, "README.md", &format!("# README\n\n{long_line}\n"));
    create_file(root, "docs/guide.md", &format!("# Guide\n\n{long_line}\n"));

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(root)
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // README.md should have MD013 warning (72 chars > 40 limit from .config/rumdl.toml)
    assert!(
        combined.contains("README.md") && combined.contains("MD013"),
        "Expected MD013 for README.md with .config/rumdl.toml (line-length=40), got:\n{combined}"
    );

    // docs/guide.md should NOT have MD013 warning (72 chars < 120 limit from docs config)
    let has_docs_md013 = combined
        .lines()
        .any(|line| line.contains("docs/guide.md") && line.contains("MD013"));
    assert!(
        !has_docs_md013,
        "Expected no MD013 for docs/guide.md with docs config (line-length=120), got:\n{combined}"
    );
}
