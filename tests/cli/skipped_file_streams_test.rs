//! Which stream reports a file the run refuses to touch.
//!
//! A document with conflict markers (MD092) or invalid UTF-8 (MD094) is left
//! byte-for-byte alone, and the finding saying so is printed like any other
//! finding of that run: on stdout, moved by `--stderr`, silenced by `--silent`.
//! The exception is a preview whose stdout is an applyable patch, which keeps
//! the notice beside it on stderr.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const CONFLICT: &[u8] = b"# Title\n\n<<<<<<< HEAD\na\n=======\nb\n>>>>>>> side\n";
const INVALID_UTF8: &[u8] = b"# Title\n\nCaf\xe9 au lait\n";
/// A finding no fix can resolve, to show where an ordinary finding of the same
/// run is printed. Without it, an empty stream pair would read as agreement.
const UNFIXABLE: &[u8] = b"# Title\n\n[missing](nowhere.md)\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
    Silent,
}

fn run(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .current_dir(dir)
        .args(args)
        .args(["--isolated", "--no-cache"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(bytes) = stdin {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn reported_on(output: &Output, rule: &str) -> Stream {
    let on_stdout = String::from_utf8_lossy(&output.stdout).contains(rule);
    let on_stderr = String::from_utf8_lossy(&output.stderr).contains(rule);
    match (on_stdout, on_stderr) {
        (true, false) => Stream::Stdout,
        (false, true) => Stream::Stderr,
        (false, false) => Stream::Silent,
        (true, true) => panic!(
            "{rule} reported on both streams:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

/// Each command, and the stream that reports a file it leaves untouched.
const PATH_CASES: &[(&[&str], Stream)] = &[
    (&["check", "doc.md"], Stream::Stdout),
    (&["check", "--fix", "doc.md"], Stream::Stdout),
    (&["check", "--diff", "doc.md"], Stream::Stdout),
    (&["fmt", "doc.md"], Stream::Stdout),
    (&["check", "--stderr", "doc.md"], Stream::Stderr),
    (&["fmt", "--stderr", "doc.md"], Stream::Stderr),
    (&["check", "--silent", "doc.md"], Stream::Silent),
    (&["fmt", "--silent", "doc.md"], Stream::Silent),
    // A preview prints a patch on stdout and nothing else, so `rumdl fmt --diff
    // . > fmt.patch` stays applyable however many files it skips.
    (&["fmt", "--check", "doc.md"], Stream::Stderr),
    (&["fmt", "--diff", "doc.md"], Stream::Stderr),
    // Formats an editor or CI reads: the annotation is the whole point of the
    // run, so it belongs where that consumer reads, not beside it.
    (&["fmt", "--output-format", "github", "doc.md"], Stream::Stdout),
    (&["check", "--output-format", "json-lines", "doc.md"], Stream::Stdout),
    (&["fmt", "--output-format", "json", "doc.md"], Stream::Stdout),
    (&["check", "--output-format", "sarif", "doc.md"], Stream::Stdout),
    // No patch fits in a JSON Lines stream, so the preview prints findings there.
    (
        &["fmt", "--diff", "--output-format", "json-lines", "doc.md"],
        Stream::Stdout,
    ),
];

/// Commands reading the document from stdin. Stdout carries the document back
/// whenever the command formats, which is the one reason to reserve stderr.
const STDIN_CASES: &[(&[&str], Stream)] = &[
    (&["check", "--stdin"], Stream::Stdout),
    (&["check", "--stderr", "--stdin"], Stream::Stderr),
    (&["check", "--silent", "--stdin"], Stream::Silent),
    (&["check", "--fix", "--stdin"], Stream::Stderr),
    (&["fmt", "-"], Stream::Stderr),
    (&["fmt", "--silent", "-"], Stream::Silent),
    (&["fmt", "--check", "--stdin"], Stream::Stderr),
    (&["fmt", "--diff", "--stdin"], Stream::Stderr),
    (&["check", "--diff", "--stdin"], Stream::Stdout),
    (&["check", "--output-format", "github", "--stdin"], Stream::Stdout),
    (
        &["fmt", "--check", "--output-format", "github", "--stdin"],
        Stream::Stderr,
    ),
    // JSON Lines has no room for a diff, so the preview leaves stdout to the
    // findings it does print.
    (
        &["fmt", "--diff", "--output-format", "json-lines", "--stdin"],
        Stream::Stdout,
    ),
];

fn check_cases(rule: &str, document: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    for (args, expected) in PATH_CASES {
        fs::write(dir.path().join("doc.md"), document).unwrap();
        let output = run(dir.path(), args, None);
        assert_eq!(reported_on(&output, rule), *expected, "{rule} {args:?}");
        assert_eq!(
            fs::read(dir.path().join("doc.md")).unwrap(),
            document,
            "{rule} {args:?} rewrote the file"
        );
    }
    for (args, expected) in STDIN_CASES {
        let output = run(dir.path(), args, Some(document));
        assert_eq!(reported_on(&output, rule), *expected, "{rule} {args:?}");
    }
}

#[test]
fn conflict_markers_are_reported_on_the_stream_the_run_uses() {
    check_cases("MD092", CONFLICT);
}

#[test]
fn invalid_utf8_is_reported_on_the_stream_the_run_uses() {
    check_cases("MD094", INVALID_UTF8);
}

/// The control: in every mode that reports an ordinary unfixable finding, a
/// skipped file's finding is reported on the same stream.
#[test]
fn a_skipped_file_follows_an_ordinary_finding_that_is_also_left_unfixed() {
    let dir = tempfile::tempdir().unwrap();
    let ordinary: &[(&[&str], Stream)] = &[
        (&["check", "doc.md"], Stream::Stdout),
        (&["check", "--fix", "doc.md"], Stream::Stdout),
        (&["check", "--diff", "doc.md"], Stream::Stdout),
        (&["fmt", "doc.md"], Stream::Stdout),
        (&["check", "--stderr", "doc.md"], Stream::Stderr),
        (&["fmt", "--stderr", "doc.md"], Stream::Stderr),
        (&["check", "--silent", "doc.md"], Stream::Silent),
        (&["fmt", "--silent", "doc.md"], Stream::Silent),
        (&["fmt", "--output-format", "github", "doc.md"], Stream::Stdout),
        (&["check", "--output-format", "json-lines", "doc.md"], Stream::Stdout),
    ];
    for (args, expected) in ordinary {
        fs::write(dir.path().join("doc.md"), UNFIXABLE).unwrap();
        let output = run(dir.path(), args, None);
        assert_eq!(reported_on(&output, "MD057"), *expected, "{args:?}");
        let skipped = PATH_CASES
            .iter()
            .find(|(case, _)| case == args)
            .expect("every control command is also a skipped-file case");
        assert_eq!(skipped.1, *expected, "{args:?} disagree");
    }
}
