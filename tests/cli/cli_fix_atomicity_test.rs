//! End-to-end tests that `rumdl fmt` writes fixed files safely.
//!
//! The fix path must replace files atomically (temp + rename) so an interrupted
//! write cannot truncate the user's content, and it must preserve the original
//! file's permissions rather than resetting them.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

#[test]
fn fmt_writes_fixed_content() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    // Trailing spaces (MD009) give the formatter something to fix.
    fs::write(&path, "# Title\n\nsome text   \n").unwrap();

    let status = rumdl()
        .args(["fmt", "--no-cache"])
        .arg(&path)
        .status()
        .expect("run rumdl fmt");
    assert!(status.success());

    let fixed = fs::read_to_string(&path).unwrap();
    assert!(!fixed.contains("text   \n"), "trailing spaces should be fixed");
    assert!(fixed.contains("some text\n"), "content must be preserved and fixed");
}

#[cfg(unix)]
#[test]
fn fmt_preserves_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, "# Title\n\nsome text   \n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    let status = rumdl()
        .args(["fmt", "--no-cache"])
        .arg(&path)
        .status()
        .expect("run rumdl fmt");
    assert!(status.success());

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640, "fmt must preserve the original file mode");
}
