use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_explain_command_with_valid_rule() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("MD045");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MD045 - Images should have alternate text"))
        .stdout(predicate::str::contains("What this rule does"))
        .stdout(predicate::str::contains("Examples"))
        .stdout(predicate::str::contains("Configuration"))
        .stdout(predicate::str::contains("Default Configuration:"));
}

#[test]
fn test_explain_command_with_lowercase_rule() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("md045");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MD045 - Images should have alternate text"));
}

#[test]
fn test_explain_command_without_md_prefix() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("045");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MD045 - Images should have alternate text"));
}

#[test]
fn test_explain_command_with_invalid_rule() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("MD999");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error: Rule 'MD999' not found"))
        .stderr(predicate::str::contains("Use 'rumdl rule' to see all available rules"));
}

#[test]
fn test_explain_command_without_argument() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required arguments were not provided"));
}

#[test]
fn test_explain_command_shows_configuration() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("MD013");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MD013 - Line length"))
        .stdout(predicate::str::contains("[MD013]"))
        .stdout(predicate::str::contains("line-length = "));
}

#[test]
fn test_explain_command_rule_with_no_config() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("MD001");

    cmd.assert().success().stdout(predicate::str::contains(
        "MD001 - Heading levels should only increment by one level at a time",
    ));
}

#[test]
fn test_explain_command_shows_examples() {
    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("explain").arg("MD032");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("✅ Correct"))
        .stdout(predicate::str::contains("❌ Incorrect"))
        .stdout(predicate::str::contains("🔧 Fixed"));
}

#[test]
fn test_explain_command_different_rules() {
    // Test a few different rules to ensure the command works for various rule types
    let rules = vec!["MD001", "MD013", "MD022", "MD045", "MD058"];

    for rule in rules {
        let mut cmd = Command::cargo_bin("rumdl").unwrap();
        cmd.arg("explain").arg(rule);

        cmd.assert().success().stdout(predicate::str::contains(rule));
    }
}
