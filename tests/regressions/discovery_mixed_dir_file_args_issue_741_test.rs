//! Regression tests for issue #741: `check` / `fmt` silently dropped every
//! directory argument as soon as any file path was also passed.
//!
//! `rumdl check content/ AGENTS.md README.md` used to lint only the two files
//! and exit as if the run were complete, so a CI task passing a source tree
//! plus a couple of loose top-level docs never inspected the tree at all. The
//! expected behavior is the union: the named files plus every markdown file
//! found by walking the named directories, exactly as passing two directories
//! already worked.
//!
//! These tests run the real binary so they exercise the production discovery
//! path (`find_markdown_files`) end to end.

use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

/// One MD041 violation per file, so the summary's file count equals the number
/// of files actually checked.
const VIOLATION: &str = "Some content without heading.\n";

fn write_violation(path: &Path) {
    fs::write(path, VIOLATION).unwrap();
}

fn run_check(base: &Path, args: &[&str]) -> Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    cmd.arg("check").arg("--no-config").arg("--no-cache");
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(base).output().expect("Failed to execute rumdl")
}

/// Layout used by most tests:
/// AGENTS.md, README.md at the root; content/page.md nested. All violate MD041.
fn setup_tree() -> TempDir {
    let temp = TempDir::new().unwrap();
    let base = temp.path();
    fs::create_dir(base.join("content")).unwrap();
    write_violation(&base.join("AGENTS.md"));
    write_violation(&base.join("README.md"));
    write_violation(&base.join("content").join("page.md"));
    temp
}

#[test]
fn test_directory_plus_files_checks_the_union() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &["content/", "AGENTS.md", "README.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    for file in ["AGENTS.md", "README.md", "content/page.md"] {
        assert!(
            stdout.contains(file),
            "{file} should be checked when mixing a directory with files. stdout:\n{stdout}"
        );
    }
    assert!(
        stdout.contains("in 3 files"),
        "All 3 files (2 explicit + 1 from the walked directory) should be checked. stdout:\n{stdout}"
    );
}

#[test]
fn test_directory_argument_order_is_irrelevant() {
    let temp = setup_tree();
    for args in [
        ["content/", "AGENTS.md", "README.md"],
        ["AGENTS.md", "content/", "README.md"],
        ["AGENTS.md", "README.md", "content/"],
    ] {
        let output = run_check(temp.path(), &args);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("in 3 files"),
            "Directory must be walked regardless of argument position {args:?}. stdout:\n{stdout}"
        );
    }
}

#[test]
fn test_directory_without_trailing_slash() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &["content", "AGENTS.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("content/page.md") && stdout.contains("AGENTS.md"),
        "A directory named without a trailing slash must still be walked. stdout:\n{stdout}"
    );
}

#[test]
fn test_dot_directory_plus_file() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &[".", "AGENTS.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // `.` covers all 3 files; AGENTS.md is also explicit and must be deduplicated.
    assert!(
        stdout.contains("in 3 files"),
        "`.` alongside a file must walk the whole tree (deduplicated). stdout:\n{stdout}"
    );
}

#[test]
fn test_multiple_directories_plus_file() {
    let temp = setup_tree();
    let base = temp.path();
    fs::create_dir(base.join("content2")).unwrap();
    write_violation(&base.join("content2").join("extra.md"));

    let output = run_check(base, &["content/", "content2/", "AGENTS.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("content/page.md") && stdout.contains("content2/extra.md") && stdout.contains("AGENTS.md"),
        "Both directories and the file should be checked. stdout:\n{stdout}"
    );
    assert!(stdout.contains("in 3 files"), "stdout:\n{stdout}");
}

#[test]
fn test_explicit_file_inside_walked_directory_is_deduplicated() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &["content/", "content/page.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("in 1 file"),
        "A file passed explicitly AND found by the directory walk must be checked once. stdout:\n{stdout}"
    );
}

#[test]
fn test_exclude_patterns_still_apply_to_walked_directory() {
    let temp = setup_tree();
    let base = temp.path();
    write_violation(&base.join("content").join("skipped.md"));

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    let output = cmd
        .arg("check")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--exclude")
        .arg("content/skipped.md")
        .arg("content/")
        .arg("AGENTS.md")
        .current_dir(base)
        .output()
        .expect("Failed to execute rumdl");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("skipped.md"),
        "Exclude patterns must keep filtering files found via directory args. stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("content/page.md") && stdout.contains("AGENTS.md"),
        "Non-excluded files from both sources should be checked. stdout:\n{stdout}"
    );
}

#[test]
fn test_explicit_non_markdown_file_alongside_directory() {
    let temp = setup_tree();
    let base = temp.path();
    write_violation(&base.join("notes.txt"));

    let output = run_check(base, &["content/", "notes.txt"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // An explicitly named file is trusted regardless of extension, while the
    // directory walk stays markdown-only.
    assert!(
        stdout.contains("notes.txt"),
        "Explicitly named non-markdown files must still be linted. stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("content/page.md"),
        "The directory must still be walked. stdout:\n{stdout}"
    );
}

#[test]
fn test_missing_file_alongside_directory_errors() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &["content/", "missing.md"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("File not found"),
        "A nonexistent explicit path must still be reported. stderr:\n{stderr}"
    );
}

#[test]
fn test_directories_only_still_works() {
    let temp = setup_tree();
    let base = temp.path();
    fs::create_dir(base.join("content2")).unwrap();
    write_violation(&base.join("content2").join("extra.md"));

    let output = run_check(base, &["content/", "content2/"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("in 2 files"),
        "Multiple directory args (no files) must keep working. stdout:\n{stdout}"
    );
}

#[test]
fn test_files_only_still_works() {
    let temp = setup_tree();
    let output = run_check(temp.path(), &["AGENTS.md", "README.md"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("in 2 files") && !stdout.contains("content/page.md"),
        "File-only invocations must lint exactly the named files. stdout:\n{stdout}"
    );
}

#[test]
fn test_fmt_diff_walks_directory_alongside_files() {
    let temp = TempDir::new().unwrap();
    let base = temp.path();
    fs::create_dir(base.join("content")).unwrap();
    // MD012 double blank line: fixable, so it shows up in `fmt --diff`.
    let fixable = "# Title\n\n\ntext\n";
    fs::write(base.join("AGENTS.md"), fixable).unwrap();
    fs::write(base.join("content").join("page.md"), fixable).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--diff")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("content/")
        .arg("AGENTS.md")
        .current_dir(base)
        .output()
        .expect("Failed to execute rumdl");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("content/page.md") && stdout.contains("AGENTS.md"),
        "fmt routes through the same discovery and must also walk the directory. stdout:\n{stdout}"
    );
}
