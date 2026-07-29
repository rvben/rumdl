//! Tests for the --respect-gitignore CLI flag
//!
//! Verifies that the flag accepts various syntaxes:
//! - Omitted (default: true)
//! - --respect-gitignore (true, requires equals sign now)
//! - --respect-gitignore=true (true)
//! - --respect-gitignore=false (false)
//!
//! Note: With `require_equals(true)`, the flag MUST use `=` syntax when providing a value.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Create a test directory with:
/// - .gitignore that ignores "ignored.md"
/// - ignored.md (should be skipped when respecting gitignore)
/// - included.md (should always be linted)
fn setup_test_directory() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create .gitignore
    fs::write(base_path.join(".gitignore"), "ignored.md\n").unwrap();

    // Create ignored.md with an issue (missing first heading)
    fs::write(
        base_path.join("ignored.md"),
        "This file has no heading and should trigger MD041.\n",
    )
    .unwrap();

    // Create included.md with an issue
    fs::write(
        base_path.join("included.md"),
        "This file also has no heading and should trigger MD041.\n",
    )
    .unwrap();

    // Initialize git repo (required for gitignore to work)
    Command::new("git")
        .current_dir(base_path)
        .args(["init", "-q"])
        .output()
        .expect("Failed to init git repo");

    // Add files to git index (gitignore only applies to untracked files after this)
    Command::new("git")
        .current_dir(base_path)
        .args(["add", "included.md"])
        .output()
        .expect("Failed to add file to git");

    temp_dir
}

#[test]
fn test_respect_gitignore_equals_true_is_accepted() {
    // --respect-gitignore=true should be accepted without parse errors
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "--respect-gitignore=true", "."])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The key test: the argument should be accepted without error
    assert!(
        !stderr.contains("unexpected value"),
        "--respect-gitignore=true should be accepted, got: {stderr}"
    );
    assert!(
        !stderr.contains("error:") || stderr.contains("Found"),
        "--respect-gitignore=true should not cause a parse error, got: {stderr}"
    );
}

#[test]
fn test_respect_gitignore_equals_false_is_accepted() {
    // --respect-gitignore=false should be accepted without parse errors
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "--respect-gitignore=false", "."])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The key test: the argument should be accepted without error
    assert!(
        !stderr.contains("unexpected value"),
        "--respect-gitignore=false should be accepted, got: {stderr}"
    );
    assert!(
        !stderr.contains("error:") || stderr.contains("Found"),
        "--respect-gitignore=false should not cause a parse error, got: {stderr}"
    );
}

#[test]
fn test_respect_gitignore_false_lints_ignored_files() {
    // When --respect-gitignore=false, gitignored files should be linted
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "--respect-gitignore=false", "."])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should lint BOTH files when gitignore is disabled
    assert!(
        combined.contains("ignored.md"),
        "ignored.md should be linted when --respect-gitignore=false, got:\n{combined}"
    );
    assert!(
        combined.contains("included.md"),
        "included.md should be linted, got:\n{combined}"
    );
}

#[test]
fn test_fmt_respect_gitignore_equals_false() {
    // fmt command should also accept --respect-gitignore=false
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["fmt", "--respect-gitignore=false", "--dry-run", "."])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should not error on parsing
    assert!(
        !stderr.contains("unexpected value"),
        "fmt --respect-gitignore=false should be accepted, got: {stderr}"
    );
}

#[test]
fn test_help_shows_respect_gitignore() {
    // Verify the flag appears in help output
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .args(["check", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--respect-gitignore"),
        "Help should mention --respect-gitignore"
    );
    assert!(stdout.contains(".gitignore"), "Help should explain gitignore behavior");
}

#[test]
fn test_explicit_path_ignores_gitignore_setting() {
    // When a file is explicitly provided, it should be linted regardless of gitignore
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Even with respect_gitignore=true (default), explicit paths should work
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "ignored.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should lint the explicitly provided file
    assert!(
        combined.contains("ignored.md") || combined.contains("MD041"),
        "Explicitly provided files should be linted regardless of gitignore, got:\n{combined}"
    );
}

#[test]
fn test_respect_gitignore_without_equals_followed_by_path() {
    // --respect-gitignore . should work (flag uses default, . is the path)
    // With require_equals(true), --respect-gitignore without = uses default_missing_value
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "--respect-gitignore", "."])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT error - the flag should work without an = sign
    assert!(
        !stderr.contains("invalid value '.'"),
        "--respect-gitignore followed by path should work, got: {stderr}"
    );
    assert!(
        !stderr.contains("error: unexpected"),
        "--respect-gitignore should be accepted, got: {stderr}"
    );
}

#[test]
fn test_respect_gitignore_default_value() {
    // When --respect-gitignore is omitted, default is true
    // This means gitignored files should NOT be linted
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "."])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Command should execute without arg parsing errors
    assert!(
        !stderr.contains("error: unexpected") && !stderr.contains("error: invalid"),
        "Default behavior should work, got: {stderr}"
    );

    // With default (respect-gitignore = true), ignored.md should NOT be linted
    assert!(
        !combined.contains("ignored.md"),
        "ignored.md should NOT be linted with default respect-gitignore=true, got:\n{combined}"
    );

    // included.md should still be linted
    assert!(
        combined.contains("included.md"),
        "included.md should be linted, got:\n{combined}"
    );
}

/// Build a repository under a directory whose own `.gitignore` holds `pattern`.
///
/// The repository is marked by a `.git` directory rather than `git init`, which is
/// all the walker looks for and keeps the test from depending on a git binary.
fn setup_nested_repository(pattern: &str) -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(".gitignore"), format!("{pattern}\n")).unwrap();

    let repo = temp_dir.path().join("repo");
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join(".gitignore"), "/.rumdl_cache\n").unwrap();
    fs::write(
        repo.join(".rumdl.toml"),
        "[global]\ninclude = [\n  \"docs/**/*.md\",\n]\n",
    )
    .unwrap();
    fs::write(repo.join("docs/guide.md"), "Body without a heading.\n").unwrap();
    temp_dir
}

fn check_repo(temp_dir: &tempfile::TempDir, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path().join("repo"))
        .arg("check")
        .arg("--no-cache")
        .args(args)
        .output()
        .expect("Failed to execute command")
}

#[test]
fn gitignore_above_the_repository_root_does_not_hide_files_inside_it() {
    // Git stops reading gitignores at the repository root, so a file above it
    // says nothing about what is inside. A directory hidden that way is pruned
    // before the walk descends, which is why an include pattern naming a file
    // underneath cannot rescue it and the run comes back empty instead.
    for pattern in ["docs/", "*.md", "*"] {
        let temp_dir = setup_nested_repository(pattern);
        for args in [vec!["."], vec!["docs/"]] {
            let output = check_repo(&temp_dir, &args);
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                combined.contains("guide.md"),
                "'{pattern}' above the repository root hid guide.md from `check {}`:\n{combined}",
                args.join(" ")
            );
        }
    }

    // Control: with no repository to bound it, the walk keeps reading upward,
    // which is all it has to go on out there.
    let temp_dir = setup_nested_repository("docs/");
    fs::remove_dir(temp_dir.path().join("repo/.git")).unwrap();
    let stderr = String::from_utf8_lossy(&check_repo(&temp_dir, &["."]).stderr).to_string();
    assert!(
        stderr.contains("by ignore files"),
        "outside a repository the ignore file above still applies:\n{stderr}"
    );

    // Control: the repository's own .gitignore keeps deciding what is checked.
    let temp_dir = setup_nested_repository("");
    fs::write(temp_dir.path().join("repo/.gitignore"), "docs/\n").unwrap();
    let stderr = String::from_utf8_lossy(&check_repo(&temp_dir, &["."]).stderr).to_string();
    assert!(
        stderr.contains("by ignore files"),
        "a gitignore inside the repository still applies:\n{stderr}"
    );
}

#[test]
fn test_config_file_respect_gitignore_false() {
    // Config file with respect-gitignore = false should lint gitignored files
    let temp_dir = setup_test_directory();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create config file with respect-gitignore = false
    fs::write(base_path.join(".rumdl.toml"), "[global]\nrespect-gitignore = false\n").unwrap();

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "."])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should lint BOTH files when config disables gitignore respect
    assert!(
        combined.contains("ignored.md"),
        "ignored.md should be linted when config has respect-gitignore=false, got:\n{combined}"
    );
    assert!(
        combined.contains("included.md"),
        "included.md should be linted, got:\n{combined}"
    );
}
