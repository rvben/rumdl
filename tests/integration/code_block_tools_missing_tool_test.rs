//! What a run says when a configured code-block tool is not installed.
//!
//! Two things have to hold. The fact has to reach every output format, not just
//! the terminal: a `fmt` run that formatted nothing because a formatter was
//! absent used to print `[]` under `--output-format json`, which a CI job reads
//! as a clean file. And the default has to say something at all: the tools rumdl
//! drives are installed separately from rumdl, so `ignore` meant the common CI
//! and pre-commit case (rumdl present, ruff absent) checked no code blocks and
//! reported success.
//!
//! The tools here are synthetic on purpose: names no machine has on PATH.
//! Nothing in this file depends on a real formatter being installed.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Binaries no machine has installed, one per slot, so a test can tell which
/// slot a run consulted.
const ABSENT_LINTER: &str = "rumdl-absent-linter";
const ABSENT_FORMATTER: &str = "rumdl-absent-formatter";

const YAML_DOC: &str = "# T\n\n```yaml\nkey: value\n```\n";

/// Absent binaries in both slots. A `fmt` run lints as well as formats, so a
/// config carrying both slots has both reported; `slots` is how a test that
/// cares about one of them says so.
const BOTH_SLOTS: &str = "lint = [\"absent-lint\"], format = [\"absent-fmt\"]";
const LINT_ONLY: &str = "lint = [\"absent-lint\"]";
const FORMAT_ONLY: &str = "format = [\"absent-fmt\"]";

/// The document is clean apart from the code block, so anything reported can
/// only come from the tool path.
fn setup_with(slots: &str, extra: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config = format!(
        "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n{extra}\n\n\
         [code-block-tools.tools.absent-lint]\ncommand = [\"{ABSENT_LINTER}\", \"-\"]\nstdin = true\nstdout = true\n\n\
         [code-block-tools.tools.absent-fmt]\ncommand = [\"{ABSENT_FORMATTER}\", \"-\"]\nstdin = true\nstdout = true\n\n\
         [code-block-tools.languages]\nyaml = {{ {slots} }}\n"
    );
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("t.md"), YAML_DOC).unwrap();
    dir
}

fn setup(extra: &str) -> TempDir {
    setup_with(BOTH_SLOTS, extra)
}

/// Run rumdl in `dir`. `--no-cache` because a second run of the same content
/// would otherwise be answered from `.rumdl_cache` without consulting a tool.
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .arg("--no-cache")
        .arg("t.md")
        .output()
        .unwrap()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The diagnostics a run reports, as (line, rule, message).
fn json_findings(output: &Output) -> Vec<(u64, String, String)> {
    let stdout = stdout_of(output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not JSON ({e}): {stdout}");
    });
    parsed
        .as_array()
        .expect("JSON output is an array")
        .iter()
        .map(|f| {
            (
                f["line"].as_u64().expect("line"),
                f["rule"].as_str().expect("rule").to_string(),
                f["message"].as_str().expect("message").to_string(),
            )
        })
        .collect()
}

const FAIL: &str = "code-block-tools.on-missing-tool-binary = \"fail\"";
const IGNORE: &str = "code-block-tools.on-missing-tool-binary = \"ignore\"";

// --- The failure reaches machine-readable output -------------------------------

#[test]
fn fmt_reports_the_missing_formatter_in_json_rather_than_an_empty_list() {
    // Format tools only, so the lint pass a `fmt` run makes has nothing to say
    // and the formatter is the sole possible source of a finding. This is the
    // shape that used to print `[]` on a run that formatted nothing.
    let dir = setup_with(FORMAT_ONLY, "");
    let output = run(dir.path(), &["fmt", "--output-format", "json", "--config", FAIL]);

    let findings = json_findings(&output);
    assert_eq!(
        findings.len(),
        1,
        "a run that formatted nothing must not report an empty list: {findings:?}"
    );
    let (line, rule, message) = &findings[0];
    assert_eq!(*line, 3, "reported against the code block, not the file");
    assert_eq!(rule, "code-block-tools");
    assert!(
        message.contains(ABSENT_FORMATTER),
        "the message names the binary: {message}"
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn check_and_fmt_report_the_missing_tool_the_same_way() {
    let lint_dir = setup_with(LINT_ONLY, "");
    let format_dir = setup_with(FORMAT_ONLY, "");
    let check = run(lint_dir.path(), &["check", "--output-format", "json", "--config", FAIL]);
    let fmt = run(format_dir.path(), &["fmt", "--output-format", "json", "--config", FAIL]);

    let check_findings = json_findings(&check);
    let fmt_findings = json_findings(&fmt);
    assert_eq!(check_findings.len(), 1, "{check_findings:?}");
    assert_eq!(fmt_findings.len(), 1, "{fmt_findings:?}");

    // Same place, same name, same wording. Only the binary differs, because each
    // run consults its own slot.
    assert_eq!(check_findings[0].0, fmt_findings[0].0);
    assert_eq!(check_findings[0].1, fmt_findings[0].1);
    assert_eq!(
        check_findings[0].2.replace(ABSENT_LINTER, ""),
        fmt_findings[0].2.replace(ABSENT_FORMATTER, "")
    );
    assert!(check_findings[0].2.contains(ABSENT_LINTER));
    assert!(fmt_findings[0].2.contains(ABSENT_FORMATTER));
}

#[test]
fn check_fix_reports_the_missing_formatter_in_json() {
    let dir = setup("");
    let output = run(
        dir.path(),
        &["check", "--fix", "--output-format", "json", "--config", FAIL],
    );

    let findings = json_findings(&output);
    assert!(
        findings
            .iter()
            .any(|(_, rule, message)| rule == "code-block-tools" && message.contains(ABSENT_FORMATTER)),
        "{findings:?}"
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_tool_error_under_warn_is_not_reported_as_a_finding() {
    // `on-error = "warn"` asks to be told and to carry on. Turning that into a
    // diagnostic would fail the build the setting exists to keep green. The
    // missing-binary setting stays at `ignore` so only `on-error` is in play.
    let dir = setup("on-error = \"warn\"");
    let output = run(dir.path(), &["fmt", "--output-format", "json", "--config", IGNORE]);

    assert_eq!(json_findings(&output), vec![], "stderr: {}", stderr_of(&output));
    assert_eq!(output.status.code(), Some(0));
}

// --- The default says the tools are not installed ------------------------------

#[test]
fn the_default_reports_a_missing_tool_as_a_config_warning() {
    let dir = setup("");
    let output = run(dir.path(), &["check"]);

    let stderr = stderr_of(&output);
    assert!(stderr.contains("[config warning]"), "stderr: {stderr}");
    assert!(stderr.contains(ABSENT_LINTER), "the warning names the binary: {stderr}");
    // A config warning, not a violation: the run still succeeds on its own.
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
}

#[test]
fn the_default_fails_the_run_under_deny_config_warnings() {
    let dir = setup("");
    let output = run(dir.path(), &["check", "--deny-config-warnings"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "the guard a pre-commit hook sets has to catch this: {}",
        stderr_of(&output)
    );
}

#[test]
fn fmt_names_the_formatter_and_check_names_the_linter() {
    // The warning describes the run in hand. Naming the slot that will not run
    // would send the user to install a binary this invocation never wanted.
    let dir = setup("");

    let check = stderr_of(&run(dir.path(), &["check"]));
    assert!(check.contains(ABSENT_LINTER), "{check}");
    assert!(!check.contains(ABSENT_FORMATTER), "{check}");

    let fmt = stderr_of(&run(dir.path(), &["fmt"]));
    assert!(fmt.contains(ABSENT_FORMATTER), "{fmt}");
    assert!(!fmt.contains(ABSENT_LINTER), "{fmt}");
}

#[test]
fn ignore_stays_silent_even_under_deny_config_warnings() {
    // `ignore` is how a user says the gap is acceptable. It has to keep meaning
    // that, or the setting has no way to express it.
    let dir = setup("");
    let output = run(dir.path(), &["check", "--deny-config-warnings", "--config", IGNORE]);

    let stderr = stderr_of(&output);
    assert!(!stderr.contains(ABSENT_LINTER), "stderr: {stderr}");
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
}

#[test]
fn fail_reports_the_missing_tool_once_rather_than_twice() {
    // Under `fail` the block-level diagnostic already says it, so the
    // once-per-run config warning must not say it again.
    let dir = setup("");
    let output = run(dir.path(), &["check", "--config", FAIL]);

    let stderr = stderr_of(&output);
    assert!(!stderr.contains("[config warning]"), "stderr: {stderr}");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn no_code_block_tools_says_nothing_about_tools_it_will_not_run() {
    let dir = setup("");
    let output = run(
        dir.path(),
        &["check", "--no-code-block-tools", "--deny-config-warnings"],
    );

    let stderr = stderr_of(&output);
    assert!(!stderr.contains(ABSENT_LINTER), "stderr: {stderr}");
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
}

#[test]
fn a_disabled_section_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            "[code-block-tools]\nenabled = false\n\n\
             [code-block-tools.tools.absent-lint]\ncommand = [\"{ABSENT_LINTER}\"]\n\n\
             [code-block-tools.languages]\nyaml = {{ lint = [\"absent-lint\"] }}\n"
        ),
    )
    .unwrap();
    fs::write(dir.path().join("t.md"), YAML_DOC).unwrap();

    let output = run(dir.path(), &["check", "--deny-config-warnings"]);
    let stderr = stderr_of(&output);
    assert!(!stderr.contains(ABSENT_LINTER), "stderr: {stderr}");
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
}

#[test]
fn warn_on_the_language_setting_says_it_does_nothing() {
    // `warn` is announced beside the config that named a tool. Which languages a
    // run meets comes from the documents, so the language setting has no such
    // place and behaves as `ignore`. Saying so beats leaving it quietly inert.
    let dir = setup("");
    let output = run(
        dir.path(),
        &[
            "check",
            "--config",
            "code-block-tools.on-missing-language-definition = \"warn\"",
        ],
    );

    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("on-missing-language-definition") && stderr.contains("behaves as"),
        "stderr: {stderr}"
    );
}
