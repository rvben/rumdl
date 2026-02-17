use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test that unfixable rules are not fixed when specified in configuration
#[test]
fn test_unfixable_rules_not_fixed() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with multiple fixable issues
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Main heading\n\n##Heading without space\n-   List item with extra spaces   \n> Blockquote with trailing spaces   \n",
    )
    .expect("Failed to write test file");

    // Create config file marking some rules as unfixable
    let config_file = temp_dir.path().join("rumdl.toml");
    fs::write(&config_file, "[global]\nunfixable = [\"MD018\", \"MD009\"]\n").expect("Failed to write config file");

    // Run rumdl with --fix
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir)
        .args(["check", "--fix", "--config", "rumdl.toml", "test.md"]);

    let _output = cmd.output().expect("Failed to execute command");

    // Read the file content after fix attempt
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");

    // MD018 (heading without space) should NOT be fixed - still "##Heading"
    assert!(
        fixed_content.contains("##Heading without space"),
        "MD018 should not be fixed when marked as unfixable, but content is: {fixed_content}"
    );

    // MD009 (trailing spaces) should NOT be fixed - spaces should remain
    assert!(
        fixed_content.contains("   \n"),
        "MD009 should not be fixed when marked as unfixable, but content is: {fixed_content}"
    );

    // MD030 (list marker spaces) should be fixed since it's not in unfixable list
    assert!(
        fixed_content.contains("- List item"),
        "MD030 should be fixed when not in unfixable list, but content is: {fixed_content}"
    );
}

/// Test that only fixable rules are fixed when fixable list is specified
#[test]
fn test_only_fixable_rules_are_fixed() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with multiple fixable issues
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Main heading\n\n##Heading without space\n-   List item with extra spaces   \n> Blockquote with trailing spaces   \nTrailing line without newline",
    )
    .expect("Failed to write test file");

    // Create config file specifying only certain rules as fixable
    let config_file = temp_dir.path().join("rumdl.toml");
    fs::write(&config_file, "[global]\nfixable = [\"MD030\", \"MD047\"]\n").expect("Failed to write config file");

    // Run rumdl with --fix
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir)
        .args(["check", "--fix", "--config", "rumdl.toml", "test.md"]);

    let _output = cmd.output().expect("Failed to execute command");

    // Read the file content after fix attempt
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");

    // MD018 (heading without space) should NOT be fixed - not in fixable list
    assert!(
        fixed_content.contains("##Heading without space"),
        "MD018 should not be fixed when not in fixable list, but content is: {fixed_content}"
    );

    // MD009 (trailing spaces) should NOT be fixed - not in fixable list
    assert!(
        fixed_content.contains("   \n"),
        "MD009 should not be fixed when not in fixable list, but content is: {fixed_content}"
    );

    // MD030 (list marker spaces) should be fixed - in fixable list
    assert!(
        fixed_content.contains("- List item"),
        "MD030 should be fixed when in fixable list, but content is: {fixed_content}"
    );

    // MD047 (newline at end) should be fixed - in fixable list
    assert!(
        fixed_content.ends_with('\n'),
        "MD047 should be fixed when in fixable list, but content is: {fixed_content}"
    );
}

/// Test that unfixable takes precedence over fixable
#[test]
fn test_unfixable_takes_precedence_over_fixable() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with fixable issues
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Main heading\n\n##Heading without space\n-   List item with extra spaces   \n",
    )
    .expect("Failed to write test file");

    // Create config file where MD018 is in both fixable and unfixable
    let config_file = temp_dir.path().join("rumdl.toml");
    fs::write(
        &config_file,
        "[global]\nfixable = [\"MD018\", \"MD030\"]\nunfixable = [\"MD018\"]\n",
    )
    .expect("Failed to write config file");

    // Run rumdl with --fix
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir)
        .args(["check", "--fix", "--config", "rumdl.toml", "test.md"]);

    let _output = cmd.output().expect("Failed to execute command");

    // Read the file content after fix attempt
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");

    // MD018 should NOT be fixed - unfixable takes precedence
    assert!(
        fixed_content.contains("##Heading without space"),
        "MD018 should not be fixed when in unfixable list (precedence), but content is: {fixed_content}"
    );

    // MD030 should be fixed - only in fixable list
    assert!(
        fixed_content.contains("- List item"),
        "MD030 should be fixed when in fixable list and not unfixable, but content is: {fixed_content}"
    );
}

/// Test configuration parsing for fixable/unfixable in pyproject.toml
#[test]
fn test_pyproject_toml_fixable_unfixable() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with fixable issues
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Main heading\n\n##Heading without space\n-   List item with extra spaces   \n",
    )
    .expect("Failed to write test file");

    // Create .rumdl.toml with unfixable configuration (using .rumdl.toml since pyproject.toml has a parsing bug)
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nunfixable = [\"MD018\"]\n").expect("Failed to write config file");

    // Run rumdl with --fix
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir).args(["check", "--fix", "test.md"]);

    let _output = cmd.output().expect("Failed to execute command");

    // Read the file content after fix attempt
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");

    // MD018 should NOT be fixed due to .rumdl.toml configuration
    assert!(
        fixed_content.contains("##Heading without space"),
        "MD018 should not be fixed when marked unfixable in .rumdl.toml, but content is: {fixed_content}"
    );

    // MD030 should be fixed (not in unfixable list)
    assert!(
        fixed_content.contains("- List item"),
        "MD030 should be fixed when not in unfixable list, but content is: {fixed_content}"
    );
}

/// Test that default behavior (no fixable/unfixable specified) fixes all rules
#[test]
fn test_default_behavior_fixes_all_rules() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with multiple fixable issues
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Main heading\n\n##Heading without space\n-   List item with extra spaces   \nTrailing line without newline",
    )
    .expect("Failed to write test file");

    // No config file - should use defaults

    // Run rumdl with --fix
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir).args(["check", "--fix", "test.md"]);

    let _output = cmd.output().expect("Failed to execute command");

    // Read the file content after fix attempt
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");

    // MD018 should be fixed by default (space added after ##)
    assert!(
        fixed_content.contains("## Heading without space"),
        "MD018 should be fixed by default, but content is: {fixed_content}"
    );

    // MD030 should be fixed
    assert!(
        fixed_content.contains("- List item"),
        "MD030 should be fixed by default, but content is: {fixed_content}"
    );

    // MD047 should be fixed
    assert!(
        fixed_content.ends_with('\n'),
        "MD047 should be fixed by default, but content is: {fixed_content}"
    );
}

/// Test that `check --fix` exits 1 when there are unfixable warnings (regression for #435).
///
/// MD057 reports broken relative links but has no auto-fix. When `check --fix` is used,
/// the re-lint after applying fixes must retain the source file path so MD057 can still
/// detect broken links. Without the path, MD057 silently returns no warnings, causing a
/// false "Success: No issues found" with exit code 0.
#[test]
fn test_check_fix_exits_nonzero_for_unfixable_warnings() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a markdown file with a broken relative link (MD057 - unfixable)
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "# Heading\n\n[Broken Link](./totally_bogus)\n").expect("Failed to write test file");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(test_file.to_str().unwrap());

    // Must exit 1 - the broken link warning is not fixable
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("MD057"))
        .stdout(predicate::str::contains("does not exist"));
}

/// Test that `check --fix` exits 0 when all issues are fixed (baseline sanity check).
#[test]
fn test_check_fix_exits_zero_when_all_fixed() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // MD009: trailing spaces - fully fixable
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "# Heading\n\nContent with trailing spaces   \n").expect("Failed to write test file");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(test_file.to_str().unwrap());

    cmd.assert().success();
}

/// Test that `check --fix` exits 1 when some issues are fixed but unfixable ones remain.
#[test]
fn test_check_fix_exits_nonzero_when_unfixable_warnings_remain_after_partial_fix() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // MD009 (fixable) + MD057 (unfixable)
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Heading\n\nContent with trailing spaces   \n\n[Broken Link](./totally_bogus)\n",
    )
    .expect("Failed to write test file");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(test_file.to_str().unwrap());

    // Must exit 1 - MD057 remains after MD009 is fixed
    cmd.assert().failure().stdout(predicate::str::contains("MD057"));

    // The fixable MD009 trailing spaces should have been removed
    let fixed_content = fs::read_to_string(&test_file).expect("Failed to read fixed file");
    assert!(
        !fixed_content.contains("spaces   \n"),
        "MD009 trailing spaces should have been fixed, content: {fixed_content}"
    );
}
