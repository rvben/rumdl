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
