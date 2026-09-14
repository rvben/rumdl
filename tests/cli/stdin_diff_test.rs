//! A piped document is previewed the way a file is: `fmt --check`, `fmt --diff`
//! and `check --diff` print a patch of what `fmt -` would write and never the
//! document itself, list beside the diff the findings a run over the file lists,
//! and exit the way that run exits.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Every preview mode, with the exit code it gives a document it would change.
const MODES: &[(&[&str], i32)] = &[
    (&["check", "--diff"], 1),
    (&["fmt", "--check"], 1),
    (&["fmt", "--diff"], 0),
    (&["fmt", "--check", "--diff"], 1),
];

/// A heading missing the blank line below it, which `fmt` inserts.
const FIXABLE: &[u8] = b"# Title\ntext\n";

/// What every mode prints for `FIXABLE` piped as `doc.md`.
const FIXABLE_DIFF: &str = "--- doc.md\n+++ doc.md\n@@ -1,2 +1,3 @@\n # Title\n+\n text\n";

fn run(cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(args)
        .args(["--no-cache", "--no-config", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().expect("failed to collect rumdl output")
}

/// `mode` reading stdin, with `extra` after it.
fn preview(cwd: &Path, mode: &[&str], extra: &[&str], input: &[u8]) -> Output {
    run(cwd, &[mode, &["-"], extra].concat(), input)
}

fn describe(args: &[&str], output: &Output) -> String {
    format!(
        "{args:?}\nexit: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Each input is something a line diff of a piped document gets wrong: CRLF and
/// mixed endings the internal normalization hides, a missing final newline, a
/// nested name, and a Rust source whose doc comments are its only markdown.
#[test]
fn each_mode_prints_a_patch_that_writes_what_fmt_writes() {
    let inputs: &[(&str, &[u8])] = &[
        ("doc.md", FIXABLE),
        ("docs/crlf.md", b"# Title\r\ntext\r\n\r\n\r\nmore\r\n"),
        ("docs/mixed.md", b"# Title\r\ntext\nmore\r\n"),
        ("docs/no-newline.md", b"# Title\n\ntext"),
        ("src/lib.rs", b"//! # Title\n//! text\n\nfn main() {}\n"),
    ];
    for (name, input) in inputs {
        let scratch = tempfile::tempdir().unwrap();
        let formatted = run(
            scratch.path(),
            &["fmt", "-", "--silent", "--stdin-filename", name],
            input,
        );
        assert!(formatted.status.success(), "{}", describe(&["fmt"], &formatted));
        assert_ne!(
            formatted.stdout, *input,
            "{name}: fmt changes nothing, so no diff is exercised"
        );
        let formatted_text = String::from_utf8(formatted.stdout.clone()).unwrap();

        for (mode, code) in MODES {
            let args = [*mode, &["-", "--stdin-filename", name]].concat();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, input).unwrap();

            let output = run(dir.path(), &args, input);
            let context = describe(&args, &output);
            assert_eq!(output.status.code(), Some(*code), "{context}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert_eq!(
                stdout.matches(&format!("--- {name}\n+++ {name}\n")).count(),
                1,
                "{context}"
            );
            assert!(!stdout.contains(&formatted_text), "the document is echoed\n{context}");

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
                "git apply rejected the output: {}\n{context}",
                String::from_utf8_lossy(&apply.stderr)
            );
            assert_eq!(fs::read(&path).unwrap(), formatted.stdout, "{context}");
        }
    }
}

#[test]
fn a_document_with_nothing_to_fix_is_neither_diffed_nor_echoed() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, _) in MODES {
        let output = preview(dir.path(), mode, &[], b"# Title\n\nParagraph body.\n");
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(0), "{context}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "No issues found in <stdin>\n",
            "{context}"
        );
        assert!(output.stderr.is_empty(), "{context}");
    }
}

/// `check --diff` lists what its diff leaves unfixed, as it does for a file, and
/// `fmt` reports findings only through its summary.
#[test]
fn the_findings_beside_the_diff_are_the_ones_it_cannot_fix() {
    let dir = tempfile::tempdir().unwrap();
    let input = b"# Title\ntext\n\n[a][missing]\n";
    for (mode, _) in MODES {
        let output = preview(dir.path(), mode, &["--stdin-filename", "doc.md"], input);
        let context = describe(mode, &output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("\n@@ -1,4 +1,5 @@\n"), "{context}");
        assert!(
            !stdout.contains("[MD022]"),
            "a finding the diff fixes is listed\n{context}"
        );
        if mode[0] == "check" {
            assert!(stdout.contains("doc.md:4:1: [MD052]"), "{context}");
            assert!(stdout.ends_with("\nFound 2 issues in doc.md\n"), "{context}");
        } else {
            assert!(!stdout.contains("[MD052]"), "{context}");
            assert!(
                stdout.ends_with("\n1 issue would be fixed, 1 issue remaining\n"),
                "{context}"
            );
        }
    }
}

#[test]
fn exit_codes_match_a_run_over_the_file_when_nothing_can_be_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let input = b"# Title\n\n[a][missing]\n";
    for (mode, code) in [
        (&["check", "--diff"][..], 1),
        (&["check", "--diff", "--fail-on", "never"][..], 0),
        (&["fmt", "--check"][..], 0),
        (&["fmt", "--diff"][..], 0),
    ] {
        let output = preview(dir.path(), mode, &[], input);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(code), "{context}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("@@") && !stdout.contains("[a][missing]"), "{context}");
    }
}

#[test]
fn quiet_leaves_exactly_the_diff() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, code) in MODES {
        let output = preview(dir.path(), mode, &["--quiet", "--stdin-filename", "doc.md"], FIXABLE);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(*code), "{context}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), FIXABLE_DIFF, "{context}");
        assert!(output.stderr.is_empty(), "{context}");
    }
}

#[test]
fn silent_prints_nothing_and_keeps_the_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, code) in MODES {
        let output = preview(dir.path(), mode, &["--silent"], FIXABLE);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(*code), "{context}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty(), "{context}");
    }
}

#[test]
fn stderr_moves_the_diff_with_the_rest_of_the_output() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, code) in MODES {
        let output = preview(dir.path(), mode, &["--stderr", "--stdin-filename", "doc.md"], FIXABLE);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(*code), "{context}");
        assert!(output.stdout.is_empty(), "{context}");
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with(FIXABLE_DIFF),
            "{context}"
        );
    }
}

#[test]
fn without_a_name_the_diff_names_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let output = preview(dir.path(), &["fmt", "--diff"], &["--quiet"], FIXABLE);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        FIXABLE_DIFF.replace("doc.md", "<stdin>"),
        "{}",
        describe(&["fmt", "--diff"], &output)
    );
}

/// A batch format is one document, so it carries the findings and no diff, as
/// it does for a run over files.
#[test]
fn a_batch_format_gets_the_findings_document_and_no_diff() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, code) in MODES {
        let output = preview(dir.path(), mode, &["--output-format", "json"], FIXABLE);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(*code), "{context}");
        let findings: serde_json::Value = serde_json::from_slice(&output.stdout).expect(&context);
        assert_eq!(findings.as_array().map(Vec::len), Some(1), "{context}");
        assert_eq!(findings[0]["rule"], "MD022", "{context}");
    }
}

#[test]
fn a_merge_conflict_is_reported_and_never_echoed() {
    let dir = tempfile::tempdir().unwrap();
    let input = b"# Title\n\n<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n";
    for (mode, code) in [
        (&["check", "--diff"][..], 1),
        (&["fmt", "--check"][..], 0),
        (&["fmt", "--diff"][..], 0),
    ] {
        let output = preview(dir.path(), mode, &[], input);
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(code), "{context}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("<<<<<<< ours"),
            "{context}"
        );
        assert!(context.contains("merge-conflict"), "{context}");
    }
}

#[test]
fn an_excluded_name_is_not_echoed() {
    let dir = tempfile::tempdir().unwrap();
    for (mode, _) in MODES {
        let output = preview(
            dir.path(),
            mode,
            &["--stdin-filename", "skip.md", "--exclude", "skip.md"],
            FIXABLE,
        );
        let context = describe(mode, &output);
        assert_eq!(output.status.code(), Some(0), "{context}");
        assert!(output.stdout.is_empty(), "{context}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("1 by exclude patterns"),
            "{context}"
        );
    }
}
