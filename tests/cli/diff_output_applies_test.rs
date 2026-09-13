//! What `--diff` and `fmt --check` print is a patch: applying a run's whole stdout
//! with `git apply` turns the files into exactly what `rumdl fmt` writes.
//!
//! The project covers what makes a line-based diff go wrong: an inserted line
//! that shifts every later line, a document without a final newline, CRLF and
//! mixed line endings in a nested directory, a carriage return inside a line, a
//! Rust source whose doc comment is the only markdown, a file with an issue no
//! fix can resolve, and a clean file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FILES: &[(&str, &[u8])] = &[
    ("one.md", b"# One\ntext\n"),
    ("two.md", b"# Two\n\ntext"),
    ("docs/crlf.md", b"# CRLF\r\ntext\r\n\r\n\r\nmore\r\n"),
    ("docs/mixed.md", b"# Mixed\r\ntext\nmore\r\n"),
    // `git apply` ends a line only at `\n`, so the lone `\r` stays inside it.
    ("cr.md", b"# CR\n\nhello\rworld   \n"),
    ("src/lib.rs", b"//! # Title\n//! text\n\nfn main() {}\n"),
    ("unfixable.md", b"# Unfixable\n\n[a][missing]\n"),
    ("clean.md", b"# Clean\n\ntext\n"),
];

/// The files `rumdl fmt` rewrites; the rest it leaves alone.
const CHANGED: &[&str] = &[
    "cr.md",
    "docs/crlf.md",
    "docs/mixed.md",
    "one.md",
    "src/lib.rs",
    "two.md",
];

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in FILES {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    dir
}

fn rumdl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .args(["--no-cache", "--no-config"])
        .args(FILES.iter().map(|(path, _)| path))
        .output()
        .unwrap()
}

/// Every file under `dir`, keyed by its path relative to `dir` with `/` separators.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path: PathBuf = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                files.insert(relative, fs::read(&path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

fn formatted() -> BTreeMap<String, Vec<u8>> {
    let dir = project();
    let original = snapshot(dir.path());
    let output = rumdl(dir.path(), &["fmt"]);
    assert!(output.status.success(), "fmt failed: {output:?}");
    let formatted = snapshot(dir.path());

    // The comparison below is only as strong as the set of files fmt rewrites.
    let changed: Vec<&str> = original
        .iter()
        .filter(|(path, content)| formatted[*path] != **content)
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(changed, CHANGED);
    formatted
}

/// Runs `args` over a fresh project, asserts it exits with `expected_code` and
/// writes nothing, then applies its stdout and asserts the result is what `fmt`
/// writes.
fn assert_output_applies(args: &[&str], expected_code: i32) {
    let formatted = formatted();
    let dir = project();
    let original = snapshot(dir.path());

    let output = rumdl(dir.path(), args);
    assert_eq!(output.status.code(), Some(expected_code), "{args:?}: {output:?}");
    assert_eq!(snapshot(dir.path()), original, "{args:?} modified the files");

    let stdout = String::from_utf8(output.stdout).unwrap();
    for path in CHANGED {
        let header = format!("--- {path}\n+++ {path}\n");
        assert_eq!(stdout.matches(&header).count(), 1, "{args:?} stdout:\n{stdout}");
    }

    let patch = tempfile::NamedTempFile::new().unwrap();
    fs::write(patch.path(), &stdout).unwrap();
    let apply = Command::new("git")
        .current_dir(dir.path())
        .args(["-c", "core.autocrlf=false", "apply", "-p0", "--whitespace=nowarn"])
        .arg(patch.path())
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "{args:?}: git apply rejected the output: {}\nstdout:\n{stdout}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert_eq!(snapshot(dir.path()), formatted, "{args:?} stdout:\n{stdout}");
}

#[test]
fn check_diff_output_applies_to_the_formatted_files() {
    assert_output_applies(&["check", "--diff"], 1);
}

#[test]
fn fmt_check_output_applies_to_the_formatted_files() {
    assert_output_applies(&["fmt", "--check"], 1);
}

#[test]
fn fmt_diff_output_applies_to_the_formatted_files() {
    assert_output_applies(&["fmt", "--diff"], 0);
}
