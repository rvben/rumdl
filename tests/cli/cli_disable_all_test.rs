//! The `all` keyword in `disable`, through the CLI.
//!
//! `rumdl check` selects its rules through the same function as the LSP and
//! wasm entry points, so `--disable all`, a config file's `disable = ["all"]`
//! and an inline `--config 'global.disable=["all"]'` empty the rule set the
//! same way everywhere. Every case here reads the findings, never a warning:
//! a keyword that is accepted without effect is the defect these tests pin.

use std::fs;
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

/// MD022 twice (no blank line after `# Title`, none before `## Second`),
/// MD009 (trailing spaces) and MD047 (no trailing newline).
const CONTENT: &str = "# Title\nText right after heading   \n## Second\n\nmore";
const RULES: [&str; 3] = ["MD009", "MD022", "MD047"];

fn workspace(config: Option<&str>) -> TempDir {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test.md"), CONTENT).unwrap();
    if let Some(config) = config {
        fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    }
    dir
}

fn check(dir: &TempDir, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .args(extra)
        .output()
        .expect("Failed to execute rumdl")
}

/// Which of the fixture's rules the run reported, in `RULES` order.
fn reported(output: &Output) -> Vec<&'static str> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    RULES
        .into_iter()
        .filter(|rule| stdout.contains(&format!("[{rule}]")))
        .collect()
}

fn assert_reports(output: &Output, expected: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        reported(output),
        expected,
        "expected findings for {expected:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        output.status.code(),
        Some(if expected.is_empty() { 0 } else { 1 }),
        "exit code should follow the findings\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Positive control: the fixture trips every rule the other cases switch off.
#[test]
fn baseline_reports_every_fixture_rule() {
    let dir = workspace(None);
    assert_reports(&check(&dir, &["--no-config"]), &RULES);
}

#[test]
fn cli_disable_all_reports_nothing() {
    let dir = workspace(None);
    assert_reports(&check(&dir, &["--no-config", "--disable", "all"]), &[]);
}

#[test]
fn cli_disable_all_is_case_insensitive() {
    let dir = workspace(None);
    assert_reports(&check(&dir, &["--no-config", "--disable", "ALL"]), &[]);
}

#[test]
fn cli_disable_all_lists_no_enabled_rules_in_verbose_output() {
    let dir = workspace(None);
    let output = check(&dir, &["--no-config", "--disable", "all", "--verbose"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let listed = stdout
        .lines()
        .skip_while(|line| *line != "Enabled rules:")
        .skip(1)
        .take_while(|line| line.starts_with("  - "))
        .count();
    assert!(
        stdout.contains("Enabled rules:"),
        "verbose run should list rules:\n{stdout}"
    );
    assert_eq!(listed, 0, "no rule should be listed as enabled:\n{stdout}");
}

#[test]
fn config_disable_all_reports_nothing() {
    let dir = workspace(Some("[global]\ndisable = [\"all\"]\n"));
    assert_reports(&check(&dir, &[]), &[]);
}

#[test]
fn inline_config_override_disable_all_reports_nothing() {
    let dir = workspace(None);
    assert_reports(
        &check(&dir, &["--no-config", "--config", "global.disable=[\"all\"]"]),
        &[],
    );
}

/// `--disable all --enable MD009` is the "run only MD009" spelling: the
/// explicit enable list survives the keyword.
#[test]
fn cli_disable_all_keeps_cli_enabled_rules() {
    let dir = workspace(None);
    assert_reports(
        &check(&dir, &["--no-config", "--disable", "all", "--enable", "MD009"]),
        &["MD009"],
    );
}

#[test]
fn config_disable_all_keeps_config_enabled_rules() {
    let dir = workspace(Some("[global]\ndisable = [\"all\"]\nenable = [\"MD009\"]\n"));
    assert_reports(&check(&dir, &[]), &["MD009"]);
}

#[test]
fn config_enable_all_cancels_disable_all() {
    let dir = workspace(Some("[global]\nenable = [\"ALL\"]\ndisable = [\"all\"]\n"));
    assert_reports(&check(&dir, &[]), &RULES);
}

/// `extend-enable` adds to the base set, and `disable = ["all"]` leaves no
/// base set to add to: disabling wins.
#[test]
fn config_disable_all_is_not_undone_by_extend_enable() {
    let dir = workspace(Some("[global]\ndisable = [\"all\"]\nextend-enable = [\"MD009\"]\n"));
    assert_reports(&check(&dir, &[]), &[]);
}

/// The enable list that survives `disable = ["all"]` is still subject to
/// `extend-disable`, which always wins over enabling.
#[test]
fn extend_disable_still_applies_to_rules_enabled_over_disable_all() {
    let dir = workspace(Some(
        "[global]\ndisable = [\"all\"]\nenable = [\"MD009\", \"MD022\"]\nextend-disable = [\"MD009\"]\n",
    ));
    assert_reports(&check(&dir, &[]), &["MD022"]);
}

/// A config `disable = ["all"]` and a CLI `--disable` naming one rule union
/// like any two disable lists: everything stays off.
#[test]
fn cli_disable_adds_to_config_disable_all() {
    let dir = workspace(Some("[global]\ndisable = [\"all\"]\n"));
    assert_reports(&check(&dir, &["--disable", "MD009"]), &[]);
}
