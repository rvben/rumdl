//! A Rust file is markdown only inside its doc comments, on every path.
//!
//! The lint pass has always read `.rs` files that way, through
//! `check_doc_comment_blocks`. The fix pass did not: it handed the whole source
//! to the markdown fix coordinator, which reads `#[derive(Debug)]` as an MD018
//! heading and `"* {} *"` as MD037 spaces inside emphasis. `rumdl fmt lib.rs`
//! wrote `# [derive(Debug)]` to disk and the file stopped being Rust, with every
//! one of those edits a byte no `check` of the same file ever reported.
//!
//! `.rs` files are a supported target (`include = ["**/*.rs"]`), so this reached
//! whole trees, and the stdin path took the same route under
//! `--stdin-filename lib.rs`.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Rust that a markdown fixer has plenty to say about: an attribute that reads
/// as an unspaced ATX heading, emphasis-looking spacing inside a string literal,
/// and a doc comment holding the one finding that is genuinely there.
const RUST_SOURCE: &str = r#"//! Crate docs.
//!
//!#Overview

use std::fmt;

#[derive(Debug)]
pub struct Widget {
    pub name: String,
}

impl fmt::Display for Widget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "* {} *", self.name)
    }
}
"#;

/// The same source with the only markdown in it corrected: the doc comment's
/// heading. Every other byte is Rust and belongs to the compiler.
const RUST_SOURCE_FIXED: &str = r#"//! Crate docs.
//!
//!# Overview

use std::fmt;

#[derive(Debug)]
pub struct Widget {
    pub name: String,
}

impl fmt::Display for Widget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "* {} *", self.name)
    }
}
"#;

fn rumdl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute rumdl")
}

/// Pipe `content` into rumdl running in `dir`, returning (stdout, stderr).
fn rumdl_with_stdin(dir: &Path, content: &str, args: &[&str]) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(content.as_bytes())
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to collect rumdl output");
    (
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn fmt_rewrites_a_rust_files_doc_comment_and_leaves_the_rust_alone() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("widget.rs"), RUST_SOURCE).unwrap();

    let output = rumdl(
        dir.path(),
        &["fmt", "--color", "never", "--no-cache", "--no-config", "widget.rs"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        fs::read_to_string(dir.path().join("widget.rs")).unwrap(),
        RUST_SOURCE_FIXED,
        "only the doc comment is markdown.\nstdout:\n{stdout}"
    );
    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("widget.rs:")).collect();
    assert_eq!(
        diagnostics,
        ["widget.rs:3:5: [MD018] No space after # in heading [fixed]"],
        "the doc-comment heading is the only finding, and the fix pass resolved it.\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("Fixed 1/1 issues in 1 file"), "stdout:\n{stdout}");
}

/// The control for the test above: the identical bytes under a name that makes
/// them markdown. Everything the `.rs` file kept is rewritten here, so a run that
/// left `widget.rs` alone did so because of what it is, not because the fixer had
/// nothing to say about the content.
#[test]
fn the_same_bytes_as_markdown_are_rewritten_throughout() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("widget.md"), RUST_SOURCE).unwrap();

    let output = rumdl(
        dir.path(),
        &["fmt", "--color", "never", "--no-cache", "--no-config", "widget.md"],
    );
    let fixed = fs::read_to_string(dir.path().join("widget.md")).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        fixed.contains("# [derive(Debug)]"),
        "the attribute reads as a heading in markdown.\nfixed:\n{fixed}\nstdout:\n{stdout}"
    );
    assert!(
        fixed.contains(r#"write!(f, "*{}*", self.name)"#),
        "the string literal reads as emphasis in markdown.\nfixed:\n{fixed}\nstdout:\n{stdout}"
    );
}

/// A diff is what the fix pass would write, so it answers to the same rule.
/// Offering to rewrite the Rust would be the same defect one keystroke away.
#[test]
fn diff_mode_offers_no_change_to_a_rust_files_source() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("widget.rs"), RUST_SOURCE).unwrap();

    let output = rumdl(
        dir.path(),
        &[
            "check",
            "--diff",
            "--color",
            "never",
            "--no-cache",
            "--no-config",
            "widget.rs",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("-//!#Overview") && stdout.contains("+//!# Overview"),
        "the doc-comment fix is still offered.\nstdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("derive") && !stdout.contains("write!"),
        "no line of Rust is offered for rewriting.\nstdout:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("widget.rs")).unwrap(),
        RUST_SOURCE,
        "a diff writes nothing"
    );
}

/// `--stdin-filename lib.rs` says the piped text is that file, so the stdin path
/// answers for it the way a run over the file does. Editors and pre-commit hooks
/// drive rumdl this way, which is where a format-on-save would have destroyed the
/// buffer.
#[test]
fn stdin_named_as_a_rust_file_fixes_only_its_doc_comments() {
    let dir = tempfile::tempdir().unwrap();

    let (stdout, stderr) = rumdl_with_stdin(
        dir.path(),
        RUST_SOURCE,
        &[
            "fmt",
            "--stdin",
            "--stdin-filename",
            "widget.rs",
            "--color",
            "never",
            "--no-config",
        ],
    );

    assert_eq!(stdout, RUST_SOURCE_FIXED, "stderr:\n{stderr}");
    assert!(
        stderr.contains("widget.rs:3:5: [MD018] No space after # in heading [fixed]"),
        "the doc-comment fix is reported.\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("1 issue(s) fixed, 0 issue(s) remaining"),
        "stderr:\n{stderr}"
    );
}

/// The lint side of the same routing: a piped Rust file reports what
/// `rumdl check widget.rs` reports, so no finding names a line of Rust.
#[test]
fn stdin_named_as_a_rust_file_reports_only_its_doc_comments() {
    let dir = tempfile::tempdir().unwrap();

    let (stdout, stderr) = rumdl_with_stdin(
        dir.path(),
        RUST_SOURCE,
        &[
            "check",
            "--stdin",
            "--stdin-filename",
            "widget.rs",
            "--color",
            "never",
            "--no-config",
        ],
    );

    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("widget.rs:")).collect();
    assert_eq!(
        diagnostics,
        ["widget.rs:3:5: [MD018] No space after # in heading"],
        "the doc-comment heading is the only finding.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
