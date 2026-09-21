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
    // Every ignore file the flag controls is named, and the one it does not is
    // named as such, since --help is often the only reference a user reads.
    let flag_help = stdout
        .split("--respect-gitignore")
        .nth(1)
        .and_then(|rest| rest.split("\n      --").next())
        .expect("help has a --respect-gitignore entry");
    for name in [".gitignore", ".ignore", ".markdownlintignore", "explicitly named files"] {
        assert!(
            flag_help.contains(name),
            "--respect-gitignore help should mention {name}:\n{flag_help}"
        );
    }
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

    // Should lint the explicitly provided file, not merely mention it
    assert!(
        combined.contains("ignored.md:1:1") && combined.contains("MD041"),
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

/// Runs `rumdl check .` in `dir` with config discovery and the cache disabled,
/// returning stdout and stderr together.
fn check_combined(dir: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-config", "--no-cache"])
        .args(args)
        .output()
        .expect("Failed to execute command");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn test_flag_controls_dot_ignore_but_not_markdownlintignore() {
    // `.ignore` is a gitignore-style file the flag governs. `.markdownlintignore`
    // is a linter-specific exclusion list, honored as markdownlint-cli honors it:
    // always, independently of any gitignore handling.
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();
    Command::new("git")
        .current_dir(base_path)
        .args(["init", "-q"])
        .output()
        .expect("Failed to init git repo");
    for name in ["dotignored.md", "lintignored.md", "rumdlignored.md", "kept.md"] {
        fs::write(base_path.join(name), "No heading here.\n").unwrap();
    }
    fs::write(base_path.join(".ignore"), "dotignored.md\n").unwrap();
    fs::write(base_path.join(".markdownlintignore"), "lintignored.md\n").unwrap();
    // Not a file rumdl reads: pinned so that supporting it is a decision.
    fs::write(base_path.join(".rumdlignore"), "rumdlignored.md\n").unwrap();

    let default = check_combined(base_path, &["."]);
    assert!(default.contains("kept.md:1:1"), "{default}");
    assert!(default.contains("rumdlignored.md:1:1"), "{default}");
    assert!(!default.contains("dotignored.md:1:1"), "{default}");
    assert!(!default.contains("lintignored.md:1:1"), "{default}");

    let disabled = check_combined(base_path, &[".", "--respect-gitignore=false"]);
    assert!(disabled.contains("dotignored.md:1:1"), "{disabled}");
    assert!(!disabled.contains("lintignored.md:1:1"), "{disabled}");

    // A named file bypasses .markdownlintignore like every other ignore file.
    let named = check_combined(base_path, &["lintignored.md"]);
    assert!(named.contains("lintignored.md:1:1"), "{named}");
}

#[test]
fn test_parent_markdownlintignore_applies_with_the_flag_off() {
    // Walking a subdirectory reads the ignore files above it. Turning the
    // gitignore family off must not stop that walk reading the one ignore file
    // the flag does not control.
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();
    fs::create_dir(base_path.join("docs")).unwrap();
    fs::write(base_path.join("docs/guide.md"), "No heading here.\n").unwrap();
    fs::write(base_path.join("docs/kept.md"), "No heading here.\n").unwrap();
    fs::write(base_path.join(".markdownlintignore"), "guide.md\n").unwrap();

    for args in [&["docs"][..], &["docs", "--respect-gitignore=false"][..]] {
        let output = check_combined(base_path, args);
        assert!(output.contains("kept.md:1:1"), "`check {}`:\n{output}", args.join(" "));
        assert!(
            !output.contains("guide.md:1:1"),
            "`check {}` linted a file .markdownlintignore lists:\n{output}",
            args.join(" ")
        );
    }
}

#[test]
fn test_a_named_directory_is_scanned_even_when_an_ignore_file_lists_it() {
    // Naming a path on the command line is the explicit request that outranks
    // ignore files, for a directory as for a file; the ignore files still apply
    // to what is inside it.
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();
    fs::create_dir(base_path.join("docs")).unwrap();
    fs::write(base_path.join("docs/guide.md"), "No heading here.\n").unwrap();
    fs::write(base_path.join("docs/skipped.md"), "No heading here.\n").unwrap();
    fs::write(base_path.join(".gitignore"), "docs/\n").unwrap();
    fs::write(base_path.join(".markdownlintignore"), "docs/\nskipped.md\n").unwrap();

    for args in [&["docs"][..], &["docs", "--respect-gitignore=false"][..]] {
        let output = check_combined(base_path, args);
        assert!(output.contains("guide.md:1:1"), "`check {}`:\n{output}", args.join(" "));
        assert!(
            !output.contains("skipped.md:1:1"),
            "`check {}`:\n{output}",
            args.join(" ")
        );
    }

    // Control: scanning the parent prunes the directory.
    let output = check_combined(base_path, &["."]);
    assert!(!output.contains("guide.md:1:1"), "`check .`:\n{output}");
}
