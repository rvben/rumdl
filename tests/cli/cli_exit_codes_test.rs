//! Exit-code contract tests.
//!
//! rumdl follows Ruff's convention: 0 = clean, 1 = lint violations found,
//! 2 = tool error (bad config, unreadable file, missing target). A file that
//! cannot be read, or a target path that does not exist, must surface as a tool
//! error (2), never as "clean" (0) or "violations" (1) - otherwise CI silently
//! passes on corrupt/unreadable files or a mistyped path.

use std::process::Command;
use tempfile::tempdir;

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

const TOOL_ERROR: i32 = 2;

#[test]
fn invalid_utf8_file_is_a_tool_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.md");
    // Not valid UTF-8: lone continuation bytes.
    std::fs::write(&path, [b'#', b' ', 0xff, 0xfe, b'\n']).unwrap();

    let status = rumdl()
        .args(["check", "--no-cache"])
        .arg(&path)
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "an unreadable (invalid UTF-8) file must exit with the tool-error code, not report success"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_a_tool_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let path = dir.path().join("noperm.md");
    std::fs::write(&path, "# ok\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

    let status = rumdl()
        .args(["check", "--no-cache"])
        .arg(&path)
        .status()
        .expect("run rumdl check");
    // Restore perms so tempdir cleanup can remove it.
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "a file that cannot be read must exit with the tool-error code"
    );
}

#[test]
fn nonexistent_target_is_a_tool_error() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist-xyz");

    let status = rumdl()
        .args(["check", "--no-cache"])
        .arg(&missing)
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "a nonexistent target path must exit with the tool-error code, not the violations code"
    );
}

// --- issue #726: --deny-config-warnings turns config problems into tool errors ---
//
// Configuration problems (unknown rule/option in a config file or CLI flag,
// unknown rule in an inline disable comment, shadowed config) are non-fatal
// stderr warnings by default. `--deny-config-warnings` makes any of them exit
// with the tool-error code (2), so CI catches config typos.

/// An unknown rule name in a config file exits 2 under the flag.
#[test]
fn deny_config_warnings_flags_unknown_rule_in_config_file() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("clean.md"), "# Title\n\nText.\n").unwrap();
    std::fs::write(dir.path().join(".rumdl.toml"), "[global]\nenable = [\"MD999\"]\n").unwrap();

    let status = rumdl()
        .args(["check", "--no-cache", "--deny-config-warnings", "clean.md"])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "an unknown rule in the config file must exit 2 under --deny-config-warnings"
    );
}

/// The same config problem WITHOUT the flag stays a non-fatal warning: exit 0
/// on a clean file. Locks in that the default behavior is unchanged.
#[test]
fn config_warnings_are_non_fatal_by_default() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("clean.md"), "# Title\n\nText.\n").unwrap();
    std::fs::write(dir.path().join(".rumdl.toml"), "[global]\nenable = [\"MD999\"]\n").unwrap();

    let status = rumdl()
        .args(["check", "--no-cache", "clean.md"])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(0),
        "config warnings must not affect the exit code by default"
    );
}

/// An unknown rule passed via a CLI flag exits 2 under the flag.
#[test]
fn deny_config_warnings_flags_unknown_rule_in_cli_flag() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("clean.md"), "# Title\n\nText.\n").unwrap();

    let status = rumdl()
        .args([
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "--disable",
            "MD9999",
            "clean.md",
        ])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(status.code(), Some(TOOL_ERROR));
}

/// A clean config plus the flag must not exit 2 (no false positive).
#[test]
fn deny_config_warnings_clean_config_exits_zero() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("clean.md"), "# Title\n\nText.\n").unwrap();

    let status = rumdl()
        .args(["check", "--no-cache", "--deny-config-warnings", "clean.md"])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(status.code(), Some(0), "no config problem means the flag has no effect");
}

/// The issue's headline case: an unknown rule in an inline disable comment is a
/// non-fatal warning by default, fatal (exit 2) under the flag.
#[test]
fn deny_config_warnings_flags_unknown_rule_in_inline_comment() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("inline.md"),
        "# Title\n\nSome text.<!-- rumdl-disable-line asdf -->\n",
    )
    .unwrap();

    let status = rumdl()
        .args(["check", "--no-cache", "--deny-config-warnings", "inline.md"])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "an unknown rule in an inline disable comment must exit 2 under the flag"
    );
}

/// The inline case without the flag stays a non-fatal warning (exit 0 on an
/// otherwise clean file).
#[test]
fn inline_config_warning_is_non_fatal_by_default() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("inline.md"),
        "# Title\n\nSome text.<!-- rumdl-disable-line asdf -->\n",
    )
    .unwrap();

    let status = rumdl()
        .args(["check", "--no-cache", "inline.md"])
        .current_dir(dir.path())
        .status()
        .expect("run rumdl check");
    assert_eq!(
        status.code(),
        Some(0),
        "an inline config warning must not affect the exit code by default"
    );
}

/// stdin path: an inline disable comment with an unknown rule exits 2 under the
/// flag, even though stdin has its own exit path.
#[test]
fn deny_config_warnings_stdin_inline_comment() {
    use std::io::Write;
    let mut child = rumdl()
        .args(["check", "--no-cache", "--stdin", "--deny-config-warnings"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn rumdl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# Title\n\nText.<!-- rumdl-disable-line asdf -->\n")
        .unwrap();
    let status = child.wait().expect("wait rumdl");
    assert_eq!(status.code(), Some(TOOL_ERROR));
}

/// stdin FIX/format path: `fmt --stdin` has its own exit before the check-mode
/// --fail-on block, so the deny decision must also guard that branch.
#[test]
fn deny_config_warnings_stdin_fmt_inline_comment() {
    use std::io::Write;
    let mut child = rumdl()
        .args(["fmt", "--no-cache", "--stdin", "--deny-config-warnings"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn rumdl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# Title\n\nText.<!-- rumdl-disable-line asdf -->\n")
        .unwrap();
    let status = child.wait().expect("wait rumdl");
    assert_eq!(
        status.code(),
        Some(TOOL_ERROR),
        "fmt --stdin must honor --deny-config-warnings despite its separate exit path"
    );
}

/// stdin without the flag stays non-fatal (exit 0 on clean content).
#[test]
fn stdin_inline_config_warning_non_fatal_by_default() {
    use std::io::Write;
    let mut child = rumdl()
        .args(["check", "--no-cache", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn rumdl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# Title\n\nText.<!-- rumdl-disable-line asdf -->\n")
        .unwrap();
    let status = child.wait().expect("wait rumdl");
    assert_eq!(
        status.code(),
        Some(0),
        "stdin inline config warning must not affect exit code by default"
    );
}
