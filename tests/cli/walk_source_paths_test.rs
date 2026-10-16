//! Walk-derived identities must keep the legacy pathname when resolution fails.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

const DOCUMENT: &str = "#Title\n\n[missing](absent.md)\n";

fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    fs::create_dir_all(real.join("nested")).unwrap();
    fs::create_dir(temp.path().join("empty")).unwrap();
    let file = real.join("nested/doc.md");
    fs::write(&file, DOCUMENT).unwrap();
    symlink(&real, temp.path().join("alias")).unwrap();
    (temp, real, file)
}

fn check(temp: &Path, legacy: bool) -> Output {
    let mut command = cargo_bin_cmd!("rumdl");
    command.current_dir(temp).args([
        "check",
        "--no-config",
        "--no-cache",
        "--output-format=json",
        "--color=never",
        "alias",
    ]);
    // Multiple walk roots retain the existing per-file canonicalization. The
    // empty directory adds no files or diagnostics and gives an independent
    // path-resolution oracle in this same executable.
    if legacy {
        command.arg("empty");
    }
    command.output().unwrap()
}

fn assert_same_output(actual: Output, legacy: Output) {
    assert_eq!(actual.status.code(), legacy.status.code());
    assert_eq!(actual.stdout, legacy.stdout);
    assert_eq!(actual.stderr, legacy.stderr);
}

struct RestorePermissions(PathBuf, fs::Permissions);

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, self.1.clone());
    }
}

#[test]
fn unreadable_descendant_lookup_preserves_the_supplied_symlink_path() {
    let (temp, real, file) = fixture();
    let directory = real.join("nested");
    let _restore = RestorePermissions(directory.clone(), fs::metadata(&directory).unwrap().permissions());
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o600)).unwrap();
    // Root or an equivalent capability may bypass directory search checks.
    if fs::symlink_metadata(&file).is_ok() {
        return;
    }
    let legacy = check(temp.path(), true);
    assert_eq!(legacy.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&legacy.stderr).contains("alias/nested/doc.md"));
    assert_same_output(check(temp.path(), false), legacy);
}

#[cfg(target_os = "macos")]
#[test]
fn readable_file_with_denied_attributes_preserves_the_supplied_symlink_path() {
    use std::process::Command;

    struct RemoveAcl(PathBuf);
    impl Drop for RemoveAcl {
        fn drop(&mut self) {
            let _ = Command::new("/bin/chmod").arg("-N").arg(&self.0).output();
        }
    }

    let (temp, _, file) = fixture();
    let applied = Command::new("/bin/chmod")
        .args(["+a", "everyone deny readattr"])
        .arg(&file)
        .output()
        .unwrap();
    if !applied.status.success() {
        // Some temporary filesystems or sandbox policies do not support ACLs.
        eprintln!("ACL fixture unavailable: {}", String::from_utf8_lossy(&applied.stderr));
        return;
    }
    let _remove = RemoveAcl(file.clone());
    if fs::symlink_metadata(&file).is_ok() || fs::read(&file).is_err() {
        eprintln!("Filesystem does not expose a readable file with denied metadata access");
        return;
    }
    let legacy = check(temp.path(), true);
    assert_eq!(legacy.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&legacy.stdout).contains("alias/nested/doc.md"));
    assert_same_output(check(temp.path(), false), legacy);
}
