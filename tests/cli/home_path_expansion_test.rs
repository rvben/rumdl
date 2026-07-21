//! Issue #746: `~` in a config path expands to the home directory, so a
//! user-level config that is committed to a dotfiles repository can name a home
//! path without hardcoding a username. Every config key that takes a path or a
//! file pattern behaves the same way: `exclude`, `include`, `per-file-ignores`,
//! `per-file-flavor`, and `cache-dir`.
//!
//! For `exclude`, both invocation shapes are covered, because they take
//! different code paths: discovery mode filters the walk, while an explicitly
//! named absolute file bypasses the walk entirely. Each assertion is paired
//! with a control run using a non-matching pattern, so an ambient ignore file
//! cannot make an exclusion assertion pass vacuously.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Content with an MD019 violation, reported unless the file is excluded.
const VIOLATION: &str = "#  Heading with two spaces\n";

/// A fake home containing `.cursor/plans/plan.md` and `docs/guide.md`, plus a
/// user-level `~/.rumdl.toml` holding `config_body`.
fn fake_home_with_config(config_body: &str) -> TempDir {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".cursor/plans")).unwrap();
    fs::create_dir_all(home.path().join("docs")).unwrap();
    fs::write(home.path().join(".cursor/plans/plan.md"), VIOLATION).unwrap();
    fs::write(home.path().join("docs/guide.md"), VIOLATION).unwrap();
    fs::write(home.path().join(".rumdl.toml"), config_body).unwrap();
    home
}

/// A fake home whose config excludes `exclude_pattern`.
fn fake_home(exclude_pattern: &str) -> TempDir {
    fake_home_with_config(&format!("[global]\nexclude = [\"{exclude_pattern}\"]\n"))
}

/// Run `rumdl check --no-cache` with `home` as the user's home directory.
/// Caching is off so a stale entry cannot mask a discovery change.
fn check_in(home: &Path, cwd: &Path, args: &[&str]) -> String {
    let mut with_no_cache = vec!["--no-cache"];
    with_no_cache.extend_from_slice(args);
    check_in_raw(home, cwd, &with_no_cache)
}

/// Run `rumdl check` with `home` as the user's home directory, passing `args`
/// verbatim. `HOME` covers Unix and `USERPROFILE` covers Windows, matching
/// `std::env::home_dir`.
fn check_in_raw(home: &Path, cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .output()
        .expect("failed to execute rumdl");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn tilde_exclude_pattern_applies_in_discovery_mode() {
    let home = fake_home("~/.cursor/plans");
    let output = check_in(home.path(), home.path(), &["."]);

    assert!(
        !output.contains("plan.md"),
        "~/.cursor/plans should exclude the directory's contents. Output:\n{output}"
    );
    assert!(
        output.contains("guide.md"),
        "unexcluded files must still be linted. Output:\n{output}"
    );
}

#[test]
fn discovery_mode_control_reports_the_file_without_a_matching_pattern() {
    let home = fake_home("some-other-directory");
    let output = check_in(home.path(), home.path(), &["."]);

    assert!(
        output.contains("plan.md"),
        "control: a non-matching pattern must leave the file linted. Output:\n{output}"
    );
}

#[test]
fn tilde_exclude_pattern_applies_to_an_explicitly_named_absolute_file() {
    let home = fake_home("~/.cursor/plans");
    let elsewhere = TempDir::new().unwrap();
    let plan = home.path().join(".cursor/plans/plan.md");
    let output = check_in(home.path(), elsewhere.path(), &[plan.to_str().unwrap()]);

    assert!(
        !output.contains("MD019"),
        "an explicitly named file under ~/.cursor/plans should be excluded. Output:\n{output}"
    );
}

#[test]
fn explicit_path_control_reports_the_file_without_a_matching_pattern() {
    let home = fake_home("some-other-directory");
    let elsewhere = TempDir::new().unwrap();
    let plan = home.path().join(".cursor/plans/plan.md");
    let output = check_in(home.path(), elsewhere.path(), &[plan.to_str().unwrap()]);

    assert!(
        output.contains("MD019"),
        "control: a non-matching pattern must leave the file linted. Output:\n{output}"
    );
}

#[test]
fn tilde_include_pattern_selects_files_under_the_home_directory() {
    let home = fake_home_with_config("[global]\ninclude = [\"~/docs/**\"]\n");
    let output = check_in(home.path(), home.path(), &["."]);

    assert!(
        output.contains("guide.md"),
        "~/docs/** should select the home docs directory. Output:\n{output}"
    );
    assert!(
        !output.contains("plan.md"),
        "include must still restrict discovery to matching files. Output:\n{output}"
    );
}

#[test]
fn tilde_per_file_ignores_pattern_silences_rules_for_the_matching_file() {
    let home = fake_home_with_config("[per-file-ignores]\n\"~/.cursor/plans/**\" = [\"MD019\", \"MD064\"]\n");
    let plan = home.path().join(".cursor/plans/plan.md");
    let output = check_in(home.path(), home.path(), &[plan.to_str().unwrap()]);

    assert!(
        !output.contains("MD019") && !output.contains("MD064"),
        "~/.cursor/plans/** should silence the listed rules. Output:\n{output}"
    );
}

#[test]
fn per_file_ignores_control_reports_rules_without_a_matching_pattern() {
    let home = fake_home_with_config("[per-file-ignores]\n\"~/somewhere-else/**\" = [\"MD019\", \"MD064\"]\n");
    let plan = home.path().join(".cursor/plans/plan.md");
    let output = check_in(home.path(), home.path(), &[plan.to_str().unwrap()]);

    assert!(
        output.contains("MD019"),
        "control: a non-matching pattern must leave the rules active. Output:\n{output}"
    );
}

#[test]
fn tilde_cache_dir_resolves_under_the_home_directory() {
    let home = fake_home_with_config("[global]\ncache-dir = \"~/mycache\"\n");
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("README.md"), VIOLATION).unwrap();

    // Caching must stay on here: it is what creates the directory.
    check_in_raw(home.path(), project.path(), &["."]);

    assert!(
        home.path().join("mycache").is_dir(),
        "cache-dir `~/mycache` should resolve to the home directory"
    );
    assert!(
        !project.path().join("~").exists(),
        "a literal `~` directory must not be created in the project"
    );
}

#[test]
fn tilde_is_not_expanded_inside_a_pattern() {
    // A literal `~` directory keeps working: only a leading `~/` is a home
    // reference, so this excludes `<cwd>/~drafts`, not `$HOME/drafts`.
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join("~drafts")).unwrap();
    fs::create_dir_all(home.path().join("drafts")).unwrap();
    fs::write(home.path().join("~drafts/tilde.md"), VIOLATION).unwrap();
    fs::write(home.path().join("drafts/plain.md"), VIOLATION).unwrap();
    fs::write(home.path().join(".rumdl.toml"), "[global]\nexclude = [\"~drafts\"]\n").unwrap();

    let output = check_in(home.path(), home.path(), &["."]);

    assert!(
        !output.contains("tilde.md"),
        "a literal `~drafts` directory must still be excluded. Output:\n{output}"
    );
    assert!(
        output.contains("plain.md"),
        "`~drafts` must not expand to the home directory. Output:\n{output}"
    );
}
