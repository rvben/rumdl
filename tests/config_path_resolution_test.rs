/// Tests for config and exclude path resolution from different working directories
///
/// Regression tests for: https://github.com/rvben/rumdl/issues/185
///
/// Two scenarios that previously behaved unexpectedly:
/// 1. `rumdl check --config ./project/.rumdl.toml project` did not find the config file
///    when the path was relative to cwd (expected shell autocomplete behavior)
/// 2. Exclude patterns were resolved relative to cwd instead of project root,
///    causing excludes to fail when running from outside the project directory
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn rumdl_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rumdl"))
}

/// Create a project structure for testing:
/// ```
/// parent/
///   project/
///     .rumdl.toml (with exclude = ["ignored.md"])
///     test.md (has lint violations)
///     ignored.md (should be excluded)
/// ```
fn setup_nested_project() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent = temp_dir.path().to_path_buf();
    let project = parent.join("project");

    fs::create_dir(&project).expect("Failed to create project dir");

    // Config file with exclude pattern
    let config_content = r#"[global]
exclude = ["ignored.md"]
"#;
    fs::write(project.join(".rumdl.toml"), config_content).expect("Failed to write config");

    // File with lint violations (multiple blank lines - MD012)
    let test_content = "# Test\n\n\n\n# Another heading\n";
    fs::write(project.join("test.md"), test_content).expect("Failed to write test.md");

    // File that should be excluded (also has violations)
    let ignored_content = "# Ignored\n\n\n\n# Another heading\n";
    fs::write(project.join("ignored.md"), ignored_content).expect("Failed to write ignored.md");

    (temp_dir, parent, project)
}

#[test]
fn test_config_path_relative_to_cwd_not_project_root() {
    // Issue #185 point 1: --config ./project/.rumdl.toml should work from parent dir
    let (_temp_dir, parent, _project) = setup_nested_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg("./project/.rumdl.toml") // Relative to cwd (parent)
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Config should be found and exclude should work
    assert!(
        !stderr.contains("Config file not found") && !stderr.contains("error"),
        "Config file should be found with relative path. stderr: {stderr}"
    );

    // ignored.md should be excluded - only test.md should have issues
    assert!(
        stdout.contains("test.md") || stderr.contains("test.md"),
        "test.md should be linted. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        !stdout.contains("ignored.md"),
        "ignored.md should be excluded from linting results. stdout: {stdout}"
    );
}

#[test]
fn test_exclude_patterns_relative_to_project_root_not_cwd() {
    // Issue #185 point 2: Excludes should be resolved relative to project root
    let (_temp_dir, parent, _project) = setup_nested_project();

    // Run from parent directory, targeting project subdirectory
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Auto-discovered config should exclude ignored.md
    // Only 1 file should be processed (test.md)
    assert!(
        stdout.contains("1 file"),
        "Only test.md should be processed (ignored.md excluded). stdout: {stdout}"
    );
    assert!(
        !stdout.contains("ignored.md"),
        "ignored.md should not appear in results. stdout: {stdout}"
    );
}

#[test]
fn test_config_and_exclude_from_deeply_nested_cwd() {
    // Run from a completely unrelated directory with absolute-like relative paths
    let (_temp_dir, parent, _project) = setup_nested_project();

    // Create another unrelated directory to run from
    let unrelated = parent.join("other");
    fs::create_dir(&unrelated).expect("Failed to create other dir");

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg("../project/.rumdl.toml")
        .arg("../project")
        .arg("--no-cache")
        .current_dir(&unrelated)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should work from unrelated directory
    assert!(
        !stderr.contains("Config file not found"),
        "Config should be found via ../project/.rumdl.toml. stderr: {stderr}"
    );

    // Excludes should still work
    assert!(
        !stdout.contains("ignored.md"),
        "ignored.md should be excluded. stdout: {stdout}"
    );
}

#[test]
fn test_explicit_config_overrides_autodiscovery() {
    // When --config is specified, it should be used instead of auto-discovered config
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base = temp_dir.path();

    let project = base.join("project");
    fs::create_dir(&project).expect("Failed to create project dir");

    // Project config excludes "excluded_by_project.md"
    let project_config = r#"[global]
exclude = ["excluded_by_project.md"]
"#;
    fs::write(project.join(".rumdl.toml"), project_config).expect("Failed to write project config");

    // External config excludes "excluded_by_external.md"
    let external_config = r#"[global]
exclude = ["excluded_by_external.md"]
"#;
    let external_config_path = base.join("external.toml");
    fs::write(&external_config_path, external_config).expect("Failed to write external config");

    // Create both files with violations
    let content = "# Test\n\n\n\n# Violation\n";
    fs::write(project.join("excluded_by_project.md"), content).expect("Failed to write file");
    fs::write(project.join("excluded_by_external.md"), content).expect("Failed to write file");
    fs::write(project.join("normal.md"), content).expect("Failed to write file");

    // Use external config - should exclude "excluded_by_external.md" but NOT "excluded_by_project.md"
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg(external_config_path.to_str().unwrap())
        .arg("project")
        .arg("--no-cache")
        .current_dir(base)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // excluded_by_external.md should be excluded (from explicit config)
    assert!(
        !stdout.contains("excluded_by_external.md"),
        "excluded_by_external.md should be excluded by explicit config. stdout: {stdout}"
    );

    // excluded_by_project.md should NOT be excluded (project config not used)
    assert!(
        stdout.contains("excluded_by_project.md"),
        "excluded_by_project.md should be linted (project config overridden). stdout: {stdout}"
    );
}

// NOTE: Glob patterns with path components (e.g., "docs/*", "docs/**/*.md") may not work
// correctly when running from a different cwd. This is a known limitation.
// Simple file patterns (e.g., "ignored.md") work correctly from any cwd.
// See: https://github.com/rvben/rumdl/issues/185

#[test]
fn test_absolute_config_path_works() {
    // Absolute config paths should always work regardless of cwd
    let (_temp_dir, parent, project) = setup_nested_project();

    let config_absolute = project.join(".rumdl.toml");

    // Run from parent with absolute config path
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg(config_absolute.to_str().unwrap())
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("ignored.md"),
        "ignored.md should be excluded with absolute config path. stdout: {stdout}"
    );
}

#[test]
fn test_github_action_scenario() {
    // Simulates the exact GitHub Actions scenario from issue #185
    // GitHub Actions runs from repo root, project may be in subdirectory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_root = temp_dir.path();

    // Typical GitHub Actions structure: .github/workflows/ at root
    let github_dir = repo_root.join(".github");
    fs::create_dir(&github_dir).expect("Failed to create .github dir");

    // Config at repo root
    let config = r#"[global]
exclude = ["vendor/**", "node_modules/**", ".github/**"]
"#;
    fs::write(repo_root.join(".rumdl.toml"), config).expect("Failed to write config");

    // Various markdown files
    let content = "# Test\n\n\n\n# Violation\n";
    fs::write(repo_root.join("README.md"), content).expect("Failed to write README.md");

    let vendor = repo_root.join("vendor");
    fs::create_dir(&vendor).expect("Failed to create vendor dir");
    fs::write(vendor.join("external.md"), content).expect("Failed to write external.md");

    // Run as GitHub Action would (from repo root, targeting repo root)
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(repo_root)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // README.md should be linted
    assert!(
        stdout.contains("README.md"),
        "README.md should be linted. stdout: {stdout}"
    );

    // vendor/** should be excluded
    assert!(
        !stdout.contains("external.md"),
        "vendor/external.md should be excluded. stdout: {stdout}"
    );
}

#[test]
fn test_pyproject_toml_exclude_from_different_cwd() {
    // Same tests but with pyproject.toml instead of .rumdl.toml
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent = temp_dir.path();
    let project = parent.join("project");

    fs::create_dir(&project).expect("Failed to create project dir");

    // pyproject.toml with rumdl config
    let pyproject = r#"[tool.rumdl]
exclude = ["ignored.md"]
"#;
    fs::write(project.join("pyproject.toml"), pyproject).expect("Failed to write pyproject.toml");

    let content = "# Test\n\n\n\n# Violation\n";
    fs::write(project.join("test.md"), content).expect("Failed to write test.md");
    fs::write(project.join("ignored.md"), content).expect("Failed to write ignored.md");

    // Run from parent
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("test.md"), "test.md should be linted. stdout: {stdout}");
    assert!(
        !stdout.contains("ignored.md"),
        "ignored.md should be excluded via pyproject.toml. stdout: {stdout}"
    );
}
