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

// =============================================================================
// PATH-BASED EXCLUDE PATTERN TESTS
// =============================================================================
// These tests verify that exclude patterns with path components (e.g., "subdir/file.md",
// "docs/*", "**/*.md") work correctly regardless of which directory rumdl is run from.
// The patterns should be resolved relative to the config file location (project root),
// not the current working directory.

/// Create a project structure with subdirectories for path-based pattern testing:
/// ```
/// parent/
///   project/
///     .rumdl.toml (with path-based exclude patterns)
///     root.md (should be linted)
///     subdir/
///       ignored.md (should be excluded by "subdir/ignored.md")
///       other.md (should be linted)
///     docs/
///       api.md (should be excluded by "docs/*")
///       guide.md (should be excluded by "docs/*")
///     generated/
///       deep/
///         nested/
///           file.md (should be excluded by "generated/**/*.md")
/// ```
fn setup_path_pattern_project() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent = temp_dir.path().to_path_buf();
    let project = parent.join("project");

    // Create directory structure
    fs::create_dir(&project).expect("Failed to create project dir");
    fs::create_dir(project.join("subdir")).expect("Failed to create subdir");
    fs::create_dir(project.join("docs")).expect("Failed to create docs dir");
    fs::create_dir_all(project.join("generated/deep/nested")).expect("Failed to create nested dirs");

    // Config with path-based exclude patterns
    let config_content = r#"[global]
exclude = [
    "subdir/ignored.md",
    "docs/*",
    "generated/**/*.md"
]
"#;
    fs::write(project.join(".rumdl.toml"), config_content).expect("Failed to write config");

    // Content with lint violations (multiple blank lines - MD012)
    let content = "# Test\n\n\n\n# Another heading\n";

    // Files that should be linted
    fs::write(project.join("root.md"), content).expect("Failed to write root.md");
    fs::write(project.join("subdir/other.md"), content).expect("Failed to write other.md");

    // Files that should be excluded
    fs::write(project.join("subdir/ignored.md"), content).expect("Failed to write ignored.md");
    fs::write(project.join("docs/api.md"), content).expect("Failed to write api.md");
    fs::write(project.join("docs/guide.md"), content).expect("Failed to write guide.md");
    fs::write(project.join("generated/deep/nested/file.md"), content).expect("Failed to write nested file");

    (temp_dir, parent, project)
}

#[test]
fn test_path_pattern_subdir_file_from_project_root() {
    // Pattern "subdir/ignored.md" should work when running from project directory
    let (_temp_dir, _parent, project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(&project)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // subdir/ignored.md should be excluded
    assert!(
        !stdout.contains("subdir/ignored.md") && !stdout.contains("ignored.md:"),
        "subdir/ignored.md should be excluded. stdout: {stdout}"
    );
    // subdir/other.md should be linted
    assert!(
        stdout.contains("other.md"),
        "subdir/other.md should be linted. stdout: {stdout}"
    );
}

#[test]
fn test_path_pattern_subdir_file_from_parent_directory() {
    // CRITICAL TEST: Pattern "subdir/ignored.md" should work when running from parent
    // This was the bug in issue #185 - path patterns failed when cwd != project root
    let (_temp_dir, parent, _project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // subdir/ignored.md should be excluded (this was failing before the fix)
    assert!(
        !stdout.contains("subdir/ignored.md") && !stdout.contains("ignored.md:"),
        "subdir/ignored.md should be excluded when running from parent. stdout: {stdout}"
    );
    // subdir/other.md should be linted
    assert!(
        stdout.contains("other.md"),
        "subdir/other.md should be linted. stdout: {stdout}"
    );
}

#[test]
fn test_glob_pattern_docs_star_from_project_root() {
    // Pattern "docs/*" should exclude all files in docs/ when running from project
    let (_temp_dir, _parent, project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(&project)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // docs/api.md and docs/guide.md should be excluded
    assert!(
        !stdout.contains("api.md"),
        "docs/api.md should be excluded. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("guide.md"),
        "docs/guide.md should be excluded. stdout: {stdout}"
    );
}

#[test]
fn test_glob_pattern_docs_star_from_parent_directory() {
    // Pattern "docs/*" should work when running from parent directory
    let (_temp_dir, parent, _project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // docs/api.md and docs/guide.md should be excluded
    assert!(
        !stdout.contains("api.md"),
        "docs/api.md should be excluded when running from parent. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("guide.md"),
        "docs/guide.md should be excluded when running from parent. stdout: {stdout}"
    );
}

#[test]
fn test_deep_glob_pattern_from_project_root() {
    // Pattern "generated/**/*.md" should exclude deeply nested files
    let (_temp_dir, _parent, project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(&project)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // generated/deep/nested/file.md should be excluded
    assert!(
        !stdout.contains("generated") && !stdout.contains("nested"),
        "generated/**/*.md files should be excluded. stdout: {stdout}"
    );
}

#[test]
fn test_deep_glob_pattern_from_parent_directory() {
    // Pattern "generated/**/*.md" should work when running from parent
    let (_temp_dir, parent, _project) = setup_path_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // generated/deep/nested/file.md should be excluded
    assert!(
        !stdout.contains("generated") && !stdout.contains("nested"),
        "generated/**/*.md should be excluded when running from parent. stdout: {stdout}"
    );
}

#[test]
fn test_path_pattern_from_sibling_directory() {
    // Run from a sibling directory to test path resolution
    let (_temp_dir, parent, _project) = setup_path_pattern_project();

    // Create sibling directory
    let sibling = parent.join("sibling");
    fs::create_dir(&sibling).expect("Failed to create sibling dir");

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("../project")
        .arg("--no-cache")
        .current_dir(&sibling)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // All path-based excludes should still work
    assert!(
        !stdout.contains("ignored.md:"),
        "subdir/ignored.md should be excluded from sibling. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("api.md"),
        "docs/api.md should be excluded from sibling. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("generated"),
        "generated/**/*.md should be excluded from sibling. stdout: {stdout}"
    );

    // But non-excluded files should be linted
    assert!(stdout.contains("root.md"), "root.md should be linted. stdout: {stdout}");
}

#[test]
fn test_path_pattern_with_explicit_config_flag() {
    // When using --config flag, patterns should still resolve relative to config location
    let (_temp_dir, parent, project) = setup_path_pattern_project();

    let config_path = project.join(".rumdl.toml");

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Path-based patterns should work with explicit config
    assert!(
        !stdout.contains("ignored.md:"),
        "subdir/ignored.md should be excluded with explicit config. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("api.md"),
        "docs/api.md should be excluded with explicit config. stdout: {stdout}"
    );
}

#[test]
fn test_multiple_nested_subdirs_pattern() {
    // Test patterns at various nesting depths
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent = temp_dir.path();
    let project = parent.join("project");

    // Create deep structure
    fs::create_dir(&project).expect("Failed to create project");
    fs::create_dir_all(project.join("a/b/c/d")).expect("Failed to create nested dirs");

    let config = r#"[global]
exclude = ["a/b/c/d/deep.md", "a/b/mid.md", "a/shallow.md"]
"#;
    fs::write(project.join(".rumdl.toml"), config).expect("Failed to write config");

    let content = "# Test\n\n\n\n# Violation\n";
    fs::write(project.join("root.md"), content).expect("Failed to write root.md");
    fs::write(project.join("a/shallow.md"), content).expect("Failed to write shallow.md");
    fs::write(project.join("a/b/mid.md"), content).expect("Failed to write mid.md");
    fs::write(project.join("a/b/c/d/deep.md"), content).expect("Failed to write deep.md");
    fs::write(project.join("a/b/c/d/other.md"), content).expect("Failed to write other.md");

    // Run from parent
    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // All specifically excluded files should be excluded
    assert!(
        !stdout.contains("shallow.md:"),
        "a/shallow.md should be excluded. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("mid.md:"),
        "a/b/mid.md should be excluded. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("deep.md:"),
        "a/b/c/d/deep.md should be excluded. stdout: {stdout}"
    );

    // Non-excluded files should be linted
    assert!(stdout.contains("root.md"), "root.md should be linted. stdout: {stdout}");
    assert!(
        stdout.contains("other.md"),
        "a/b/c/d/other.md should be linted (not excluded). stdout: {stdout}"
    );
}

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

// =============================================================================
// DIRECTORY-ONLY EXCLUDE PATTERN TESTS
// =============================================================================
// These tests verify that exclude patterns WITHOUT glob suffixes (e.g., "content/blog/2014")
// correctly exclude all files within those directories. This was the key issue from
// HarHarLinks' comment on issue #185 - patterns like "content/blog/2014" were not
// matching "content/blog/2014/file.md" because globset only matches exact paths.
//
// The fix: expand directory-only patterns to include "/**" suffix.

/// Create a project structure that mimics HarHarLinks' scenario from issue #185:
/// ```
/// parent/
///   project/
///     .rumdl.toml (with exclude = ["content/blog/2014"])
///     content/
///       blog/
///         2014/
///           old-post.md (should be excluded)
///           archived/
///             deep.md (should also be excluded - nested)
///         2015/
///           new-post.md (should be linted)
///       pages/
///         about.md (should be linted)
/// ```
fn setup_directory_only_pattern_project() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent = temp_dir.path().to_path_buf();
    let project = parent.join("project");

    // Create directory structure
    fs::create_dir(&project).expect("Failed to create project dir");
    fs::create_dir_all(project.join("content/blog/2014/archived")).expect("Failed to create 2014 dirs");
    fs::create_dir(project.join("content/blog/2015")).expect("Failed to create 2015 dir");
    fs::create_dir(project.join("content/pages")).expect("Failed to create pages dir");

    // Config with directory-only pattern (NO glob suffix)
    let config_content = r#"[global]
exclude = ["content/blog/2014"]
"#;
    fs::write(project.join(".rumdl.toml"), config_content).expect("Failed to write config");

    // Content with lint violations (multiple blank lines - MD012)
    let content = "# Test\n\n\n\n# Another heading\n";

    // Files that should be excluded (within content/blog/2014)
    fs::write(project.join("content/blog/2014/old-post.md"), content).expect("Failed to write old-post.md");
    fs::write(project.join("content/blog/2014/archived/deep.md"), content).expect("Failed to write deep.md");

    // Files that should be linted
    fs::write(project.join("content/blog/2015/new-post.md"), content).expect("Failed to write new-post.md");
    fs::write(project.join("content/pages/about.md"), content).expect("Failed to write about.md");

    (temp_dir, parent, project)
}

#[test]
fn test_directory_only_pattern_excludes_contents() {
    // Issue #185 (HarHarLinks comment): Pattern "content/blog/2014" should exclude
    // all files within that directory, not just the exact path
    let (_temp_dir, _parent, project) = setup_directory_only_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(&project)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Files in content/blog/2014/ should be excluded (the key fix!)
    assert!(
        !stdout.contains("old-post.md"),
        "content/blog/2014/old-post.md should be excluded. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("deep.md"),
        "content/blog/2014/archived/deep.md should be excluded. stdout: {stdout}"
    );

    // Files outside 2014/ should be linted
    assert!(
        stdout.contains("new-post.md"),
        "content/blog/2015/new-post.md should be linted. stdout: {stdout}"
    );
    assert!(
        stdout.contains("about.md"),
        "content/pages/about.md should be linted. stdout: {stdout}"
    );
}

#[test]
fn test_directory_only_pattern_from_parent_directory() {
    // Same test but from parent directory (the original #185 scenario)
    let (_temp_dir, parent, _project) = setup_directory_only_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Files in content/blog/2014/ should be excluded
    assert!(
        !stdout.contains("old-post.md"),
        "2014/old-post.md should be excluded from parent. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("deep.md"),
        "2014/archived/deep.md should be excluded from parent. stdout: {stdout}"
    );

    // Files outside 2014/ should be linted
    assert!(
        stdout.contains("new-post.md"),
        "2015/new-post.md should be linted. stdout: {stdout}"
    );
}

#[test]
fn test_directory_only_pattern_with_explicit_config() {
    // Using --config flag should also work with directory-only patterns
    let (_temp_dir, parent, project) = setup_directory_only_pattern_project();

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg("--config")
        .arg(project.join(".rumdl.toml").to_str().unwrap())
        .arg("project")
        .arg("--no-cache")
        .current_dir(&parent)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should work the same with explicit --config
    assert!(
        !stdout.contains("old-post.md"),
        "2014/old-post.md should be excluded with explicit config. stdout: {stdout}"
    );
}

#[test]
fn test_mixed_directory_and_glob_patterns() {
    // Test combining directory-only patterns with glob patterns
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project = temp_dir.path().join("project");

    fs::create_dir(&project).expect("Failed to create project dir");
    fs::create_dir_all(project.join("vendor/lib")).expect("Failed to create vendor dirs");
    fs::create_dir(project.join("docs")).expect("Failed to create docs dir");
    fs::create_dir(project.join("generated")).expect("Failed to create generated dir");

    // Mix of directory-only and glob patterns
    let config = r#"[global]
exclude = [
    "vendor",
    "docs/*.tmp.md",
    "generated/**"
]
"#;
    fs::write(project.join(".rumdl.toml"), config).expect("Failed to write config");

    let content = "# Test\n\n\n\n# Violation\n";

    // Files under vendor/ (directory-only pattern)
    fs::write(project.join("vendor/external.md"), content).expect("Failed to write file");
    fs::write(project.join("vendor/lib/nested.md"), content).expect("Failed to write file");

    // Files under docs/
    fs::write(project.join("docs/guide.md"), content).expect("Failed to write guide.md");
    fs::write(project.join("docs/temp.tmp.md"), content).expect("Failed to write temp file");

    // Files under generated/
    fs::write(project.join("generated/output.md"), content).expect("Failed to write output");

    // Regular file
    fs::write(project.join("README.md"), content).expect("Failed to write README");

    let output = Command::new(rumdl_binary())
        .arg("check")
        .arg(".")
        .arg("--no-cache")
        .current_dir(&project)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // vendor/ (directory-only) should exclude all nested files
    assert!(
        !stdout.contains("external.md"),
        "vendor/external.md should be excluded. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("nested.md"),
        "vendor/lib/nested.md should be excluded. stdout: {stdout}"
    );

    // docs/*.tmp.md (glob) should only exclude matching files
    assert!(
        !stdout.contains("temp.tmp.md"),
        "docs/temp.tmp.md should be excluded. stdout: {stdout}"
    );
    assert!(
        stdout.contains("guide.md"),
        "docs/guide.md should be linted. stdout: {stdout}"
    );

    // generated/** should exclude everything
    assert!(
        !stdout.contains("output.md"),
        "generated/output.md should be excluded. stdout: {stdout}"
    );

    // README.md should be linted
    assert!(
        stdout.contains("README.md"),
        "README.md should be linted. stdout: {stdout}"
    );
}
