//! Issue #680: `rumdl check --list-rules` was advertised in help/README but the
//! flag was inert (parsed, never read), so it silently ran a normal check.
//!
//! The flag is now hidden and deprecated: `check`/`fmt` no longer advertise it,
//! and using it fails loudly (exit 2) with guidance pointing at the canonical
//! commands (`rumdl rule`, `rumdl check --verbose`, `rumdl config`) instead of a
//! bare "unexpected argument" error.

use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_check_list_rules_redirects_with_guidance() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(temp_dir.path())
        .args(["check", "--list-rules"])
        .output()
        .expect("Failed to execute command");

    // Fails loudly (exit 2) so a script using it does not silently skip linting.
    assert_eq!(
        output.status.code(),
        Some(2),
        "`check --list-rules` must fail as a usage error, not run a lint"
    );
    // ...and points at the canonical rule-listing command rather than a bare error.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rumdl rule"),
        "the message should redirect to `rumdl rule`, got stderr: {stderr}"
    );
}

#[test]
fn test_check_list_rules_short_flag_redirects() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(temp_dir.path())
        .args(["check", "-l"])
        .output()
        .expect("Failed to execute command");

    assert_eq!(output.status.code(), Some(2), "`check -l` must fail as a usage error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rumdl rule"),
        "`-l` should redirect to `rumdl rule`: {stderr}"
    );
}

#[test]
fn test_fmt_list_rules_redirects() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(temp_dir.path())
        .args(["fmt", "--list-rules"])
        .output()
        .expect("Failed to execute command");

    assert_eq!(
        output.status.code(),
        Some(2),
        "`fmt --list-rules` must fail as a usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rumdl rule"),
        "`fmt --list-rules` should redirect: {stderr}"
    );
}

#[test]
fn test_list_rules_flag_is_hidden_from_help() {
    // The flag must not be advertised in help anymore (that false advertising was
    // the original bug); it only exists as a hidden redirect.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "--help"])
        .output()
        .expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("--list-rules"),
        "`check --help` must not advertise the deprecated --list-rules flag"
    );
}

#[test]
fn test_rule_subcommand_lists_rules() {
    // The canonical replacement: `rumdl rule` lists all available rules.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .arg("rule")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "`rumdl rule` should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD001"),
        "`rumdl rule` should list rules including MD001, got: {stdout}"
    );
}
