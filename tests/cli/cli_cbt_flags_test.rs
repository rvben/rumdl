use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Write a config that enables code-block-tools with the built-in rumdl tool
/// for embedded markdown blocks (no external binary needed), and a markdown
/// file whose outer content is short but whose embedded markdown block has a
/// line that exceeds MD013's 80-char limit.
///
/// This gives:
/// - default: 2 issues (MD013 from regular check + from embedded check via CBT)
/// - --no-code-block-tools: 1 issue (only MD013 from regular check; CBT off)
/// - --only-code-block-tools: 0 issues (rules disabled; CBT runs but no rules)
fn setup(tmp: &tempfile::TempDir) {
    let base = tmp.path();
    fs::write(
        base.join(".rumdl.toml"),
        "[code-block-tools]\nenabled = true\n\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n",
    )
    .unwrap();
    fs::write(
        base.join("test.md"),
        "# Title\n\nShort outer paragraph.\n\n```markdown\nSome very long embedded markdown line that definitely exceeds eighty characters by a lot for sure yes\n```\n",
    )
    .unwrap();
}

fn rumdl() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// Without any flag, both the regular Markdown check and the embedded
/// code-block-tools (rumdl builtin) run → 2 issues.
#[test]
fn test_cbt_flags_default_runs_both() {
    let tmp = tempdir().unwrap();
    setup(&tmp);
    let out = Command::new(rumdl())
        .current_dir(tmp.path())
        .args(["check", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Issues: Found 2"),
        "expected 2 issues (regular + embedded), got: {stdout}"
    );
}

/// --no-code-block-tools keeps Markdown rules but silences the embedded
/// code-block-tools check → 1 issue (regular only).
#[test]
fn test_no_code_block_tools_flag() {
    let tmp = tempdir().unwrap();
    setup(&tmp);
    let out = Command::new(rumdl())
        .current_dir(tmp.path())
        .args(["check", "--no-code-block-tools", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("MD013"),
        "expected MD013 from regular check, got: {stdout}"
    );
    assert!(
        stdout.contains("Issues: Found 1"),
        "expected 1 issue (regular only), got: {stdout}"
    );
}

/// --only-code-block-tools keeps code-block-tools but disables all regular
/// Markdown rules → 0 issues (no rules to run).
#[test]
fn test_only_code_block_tools_flag() {
    let tmp = tempdir().unwrap();
    setup(&tmp);
    let out = Command::new(rumdl())
        .current_dir(tmp.path())
        .args(["check", "--only-code-block-tools", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Success: No issues found"),
        "expected 0 issues, got: {stdout}"
    );
}

/// --only-code-block-tools also works with `fmt` (via the same CheckArgs
/// conversion) → no Markdown formatting output.
#[test]
fn test_only_code_block_tools_with_fmt() {
    let tmp = tempdir().unwrap();
    setup(&tmp);
    let out = Command::new(rumdl())
        .current_dir(tmp.path())
        .args(["fmt", "--only-code-block-tools", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("MD013"),
        "Markdown rules should be disabled under fmt, got: {stdout}"
    );
}

/// The two flags are mutually exclusive: clap rejects them together.
#[test]
fn test_cbt_flags_are_mutually_exclusive() {
    let tmp = tempdir().unwrap();
    setup(&tmp);
    let out = Command::new(rumdl())
        .current_dir(tmp.path())
        .args(["check", "--no-code-block-tools", "--only-code-block-tools", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected clap to reject the combination");
    assert!(
        stderr.contains("cannot be used with"),
        "expected mutual-exclusion error, got: {stderr}"
    );
}
