//! MD094 through the CLI.
//!
//! A document that is not valid UTF-8 reaches the linter through several
//! adapters (a file on disk, `--stdin`, `--stdin-batch`, a cross-file link
//! target), and each one decides whether the document may be written. These
//! tests pin, per adapter, that the invalid bytes are reported at their
//! position, that the standard rule controls apply to MD094, and that such a
//! document is never rewritten.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

/// `# Title`, a Latin-1 `é` on line 3 and a trailing space MD009 would fix.
const LATIN1: &[u8] = b"# Title\n\nCaf\xe9 menu \n";

fn run_in(dir: &Path, args: &[&str]) -> Output {
    rumdl().current_dir(dir).args(args).output().expect("run rumdl")
}

fn run_stdin(dir: &Path, input: &[u8], args: &[&str]) -> Output {
    let mut child = rumdl()
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rumdl");
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    child.wait_with_output().expect("collect rumdl output")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// Everything a run printed. A document that is left unwritten reports on
/// stderr under `--fix` and `fmt`, as MD092 does.
fn all_output(output: &Output) -> String {
    format!("{}{}", text(&output.stdout), text(&output.stderr))
}

fn lines_for<'a>(output: &'a str, rule: &str) -> Vec<&'a str> {
    let tag = format!("[{rule}]");
    output.lines().filter(|line| line.contains(&tag)).collect()
}

/// A project directory holding `test.md` with `content`, and `.rumdl.toml`
/// when `config` is given.
fn project(content: &[u8], config: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.md"), content).unwrap();
    if let Some(config) = config {
        fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    }
    dir
}

/// 25 lines, each holding one invalid byte, after a heading and a blank line,
/// so invalid sequence `n` (0-based) is on line `n + 3`.
fn many_invalid() -> Vec<u8> {
    let mut content = b"# Title\n\n".to_vec();
    for _ in 0..25 {
        content.extend_from_slice(b"x\xe9\n");
    }
    content
}

#[test]
fn check_reports_each_invalid_sequence_and_lints_the_rest() {
    let dir = project(LATIN1, None);
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);

    assert_eq!(
        lines_for(&stdout, "MD094"),
        ["test.md:3:4: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"],
        "full output:\n{stdout}"
    );
    assert_eq!(
        lines_for(&stdout, "MD009").len(),
        1,
        "the decoded text is linted by the other rules too:\n{stdout}"
    );
    assert_eq!(output.status.code(), Some(1), "stderr:\n{}", text(&output.stderr));
}

#[test]
fn positions_follow_characters_across_crlf_and_multibyte_text() {
    // Line 2 has a valid two-byte character before the invalid byte, so the
    // column counts characters, and CRLF line endings do not shift lines.
    let dir = project(b"# Title\r\n\xc3\xa9t\xe9\r\n", None);
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        lines_for(&stdout, "MD094"),
        ["test.md:2:3: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"],
        "full output:\n{stdout}"
    );
}

#[test]
fn json_output_marks_every_finding_unfixable() {
    let dir = project(LATIN1, None);
    let output = run_in(
        dir.path(),
        &["check", "--no-cache", "--output-format", "json", "test.md"],
    );
    let findings: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    let findings = findings.as_array().expect("array of findings");

    let rules: Vec<&str> = findings.iter().map(|f| f["rule"].as_str().unwrap()).collect();
    assert!(rules.contains(&"MD094") && rules.contains(&"MD009"), "{findings:?}");
    for finding in findings {
        assert_eq!(finding["fixable"], false, "{finding}");
        assert!(finding.get("fix").is_none(), "{finding}");
    }
}

#[test]
fn fix_and_fmt_never_write_an_invalid_file() {
    for args in [
        &["check", "--fix", "--no-cache", "test.md"][..],
        &["fmt", "--no-cache", "test.md"],
        // With MD094 disabled the other findings still carry no fix.
        &["check", "--fix", "--no-cache", "--disable", "MD094", "test.md"],
    ] {
        let dir = project(LATIN1, None);
        let output = run_in(dir.path(), args);
        assert_eq!(
            fs::read(dir.path().join("test.md")).unwrap(),
            LATIN1,
            "`rumdl {}` rewrote the file; stdout:\n{}",
            args.join(" "),
            text(&output.stdout)
        );
    }
}

#[test]
fn control_fix_does_write_the_same_file_once_it_is_valid() {
    // Guards the test above: the same content, valid, is fixed.
    let valid = b"# Title\n\nCafe menu \n";
    let dir = project(valid, None);
    run_in(dir.path(), &["check", "--fix", "--no-cache", "test.md"]);
    assert_eq!(fs::read(dir.path().join("test.md")).unwrap(), b"# Title\n\nCafe menu\n");
}

#[test]
fn fmt_check_and_diff_print_no_diff_for_an_invalid_file() {
    for args in [
        &["fmt", "--check", "--no-cache", "test.md"][..],
        &["fmt", "--diff", "--no-cache", "test.md"],
        &["check", "--diff", "--no-cache", "test.md"],
    ] {
        let dir = project(LATIN1, None);
        let output = run_in(dir.path(), args);
        let stdout = text(&output.stdout);
        assert!(
            !stdout.contains("+++") && !stdout.contains("@@"),
            "`rumdl {}` printed a diff:\n{stdout}",
            args.join(" ")
        );
        assert_eq!(fs::read(dir.path().join("test.md")).unwrap(), LATIN1);
    }
}

#[test]
fn fmt_keeps_its_formatter_exit_code() {
    let dir = project(LATIN1, None);
    let output = run_in(dir.path(), &["fmt", "--no-cache", "test.md"]);
    let printed = all_output(&output);
    assert_eq!(output.status.code(), Some(0), "{printed}");
    assert_eq!(lines_for(&printed, "MD094").len(), 1, "{printed}");
}

#[test]
fn the_standard_rule_controls_silence_md094() {
    let cases: [(&[u8], Option<&str>, &[&str]); 4] = [
        (LATIN1, None, &["--disable", "MD094"]),
        (LATIN1, Some("[global]\ndisable = [\"invalid-encoding\"]\n"), &[]),
        (LATIN1, Some("[per-file-ignores]\n\"test.md\" = [\"MD094\"]\n"), &[]),
        (
            b"# Title\n\n<!-- rumdl-disable-next-line MD094 -->\nCaf\xe9 menu\n",
            None,
            &[],
        ),
    ];
    for (content, config, extra) in cases {
        let dir = project(content, config);
        let mut args = vec!["check", "--no-cache"];
        args.extend_from_slice(extra);
        args.push("test.md");
        let output = run_in(dir.path(), &args);
        let stdout = text(&output.stdout);
        assert!(
            lines_for(&stdout, "MD094").is_empty(),
            "config {config:?}, args {extra:?}:\n{stdout}"
        );
    }
}

#[test]
fn severity_override_applies_to_md094() {
    // Both the lossy and the binary findings take the configured severity.
    for content in [LATIN1, b"#bad\n\x00\xff\n"] {
        let dir = project(content, Some("[MD094]\nseverity = \"error\"\n"));
        let output = run_in(
            dir.path(),
            &["check", "--no-cache", "--output-format", "json", "test.md"],
        );
        let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let md094: Vec<_> = findings
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["rule"] == "MD094")
            .collect();
        assert_eq!(md094.len(), 1, "{findings}");
        assert_eq!(md094[0]["severity"], "error", "{findings}");
    }
}

#[test]
fn a_disable_comment_that_hides_md094_is_used() {
    // The comment suppresses the invalid byte on its line, so MD087 is silent.
    let used = b"# Title\n\n<!-- rumdl-disable-next-line MD094 -->\nCaf\xe9\n";
    let dir = project(used, None);
    let output = run_in(
        dir.path(),
        &["check", "--no-cache", "--extend-enable", "MD087", "test.md"],
    );
    let stdout = text(&output.stdout);
    assert!(lines_for(&stdout, "MD087").is_empty(), "{stdout}");
    assert!(lines_for(&stdout, "MD094").is_empty(), "{stdout}");

    // Control: the same comment above a valid line suppresses nothing.
    let unused = b"# Title\n\n<!-- rumdl-disable-next-line MD094 -->\nCafe\n\nCaf\xe9\n";
    let dir = project(unused, None);
    let output = run_in(
        dir.path(),
        &["check", "--no-cache", "--extend-enable", "MD087", "test.md"],
    );
    let stdout = text(&output.stdout);
    assert_eq!(lines_for(&stdout, "MD087").len(), 1, "{stdout}");
    assert_eq!(lines_for(&stdout, "MD094").len(), 1, "{stdout}");
}

#[test]
fn findings_past_the_cap_are_summarized() {
    let dir = project(&many_invalid(), None);
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);
    let md094 = lines_for(&stdout, "MD094");

    assert_eq!(md094.len(), 21, "{stdout}");
    assert_eq!(
        md094[0],
        "test.md:3:2: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"
    );
    assert_eq!(
        md094[19],
        "test.md:22:2: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"
    );
    assert_eq!(
        md094[20],
        "test.md:23:2: [MD094] 5 more invalid UTF-8 sequences not shown"
    );
}

#[test]
fn suppressed_findings_do_not_count_toward_the_cap() {
    // Three of the 25 invalid lines are suppressed, leaving 22: 20 shown, 2 summarized.
    let mut content = b"# Title\n\n<!-- rumdl-disable MD094 -->\n".to_vec();
    content.extend_from_slice(&b"x\xe9\n".repeat(3));
    content.extend_from_slice(b"<!-- rumdl-enable MD094 -->\n");
    content.extend_from_slice(&b"x\xe9\n".repeat(22));
    let dir = project(&content, None);
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);
    let md094 = lines_for(&stdout, "MD094");

    assert_eq!(md094.len(), 21, "{stdout}");
    assert_eq!(
        md094[0],
        "test.md:8:2: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"
    );
    assert!(
        md094[20].ends_with("[MD094] 2 more invalid UTF-8 sequences not shown"),
        "{stdout}"
    );
}

#[test]
fn the_cap_applies_to_stdin_and_stdin_batch() {
    let dir = tempfile::tempdir().unwrap();
    let content = many_invalid();

    let output = run_stdin(dir.path(), &content, &["check", "--no-cache", "--stdin"]);
    let md094_stdin = lines_for(&text(&output.stdout), "MD094").len();

    let mut batch = b"doc.md\0".to_vec();
    batch.extend_from_slice(&content);
    batch.push(0);
    let output = run_stdin(dir.path(), &batch, &["check", "--no-cache", "--stdin-batch"]);
    let stdout = text(&output.stdout);

    assert_eq!(md094_stdin, 21);
    assert_eq!(lines_for(&stdout, "MD094").len(), 21, "{stdout}");
    assert!(
        stdout.contains("doc.md:23:2: [MD094] 5 more invalid UTF-8 sequences not shown"),
        "{stdout}"
    );
}

#[test]
fn stdin_check_reports_and_fails() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_stdin(dir.path(), LATIN1, &["check", "--no-cache", "--stdin"]);
    let stdout = text(&output.stdout);
    assert_eq!(lines_for(&stdout, "MD094").len(), 1, "{stdout}");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn stdin_fix_and_fmt_echo_the_input_unchanged() {
    for (args, code) in [
        (&["check", "--fix", "--no-cache", "--stdin"][..], 1),
        (&["fmt", "--no-cache", "-"], 0),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run_stdin(dir.path(), LATIN1, args);
        assert_eq!(output.stdout, LATIN1, "`rumdl {}`", args.join(" "));
        assert_eq!(
            lines_for(&text(&output.stderr), "MD094").len(),
            1,
            "diagnostics go to stderr when stdout carries the document:\n{}",
            text(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(code), "`rumdl {}`", args.join(" "));
    }
}

/// A merge conflict takes precedence over linting, and the document it hands
/// back must still be the input bytes, not their lossy decoding.
#[test]
fn stdin_fix_and_fmt_echo_invalid_input_that_also_has_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let input: &[u8] = b"# Title\n\nCaf\xe9\n\n<<<<<<< HEAD\na\n=======\nb\n>>>>>>> side\n";
    for args in [
        &["fmt", "--no-cache", "-"][..],
        &["check", "--fix", "--no-cache", "--stdin"][..],
    ] {
        let output = run_stdin(dir.path(), input, args);
        assert_eq!(output.stdout, input, "{args:?}");
        assert_eq!(lines_for(&text(&output.stderr), "MD092").len(), 1, "{args:?}");
    }
}

#[test]
fn stdin_previews_print_no_diff() {
    for (args, code) in [
        (&["check", "--diff", "--no-cache", "--stdin"][..], 1),
        (&["fmt", "--check", "--no-cache", "-"], 0),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run_stdin(dir.path(), LATIN1, args);
        let stdout = text(&output.stdout);
        assert!(
            !stdout.contains("@@"),
            "`rumdl {}` printed a diff:\n{stdout}",
            args.join(" ")
        );
        // Which stream carries the finding is pinned by the routing matrix in
        // `skipped_file_streams_test`; here it only has to be reported once.
        let diagnostics = format!("{stdout}{}", text(&output.stderr));
        assert_eq!(
            lines_for(&diagnostics, "MD094").len(),
            1,
            "`rumdl {}`:\n{diagnostics}",
            args.join(" ")
        );
        assert_eq!(output.status.code(), Some(code), "`rumdl {}`", args.join(" "));
    }
}

#[test]
fn stdin_batch_lints_an_invalid_document_alongside_valid_ones() {
    let dir = tempfile::tempdir().unwrap();
    let input = b"a.md\0# A\n\nCaf\xe9\n\0b.md\0#B\n\0";
    let output = run_stdin(dir.path(), input, &["check", "--no-cache", "--stdin-batch"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        lines_for(&stdout, "MD094"),
        ["a.md:3:4: [MD094] Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)"],
        "{stdout}"
    );
    assert_eq!(
        lines_for(&stdout, "MD018"),
        ["b.md:1:2: [MD018] No space after # in heading [*]"]
    );
}

#[test]
fn binary_files_get_one_finding_and_are_not_linted() {
    let dir = tempfile::tempdir().unwrap();
    // `#bad` would be an MD018 finding if the file were linted.
    let binary: &[u8] = b"#bad\n\x00\xff\n";
    let utf16: &[u8] = b"\xff\xfe#\x00b\x00";
    fs::write(dir.path().join("bin.md"), binary).unwrap();
    fs::write(dir.path().join("u16.md"), utf16).unwrap();

    let output = run_in(dir.path(), &["check", "--no-cache", "bin.md", "u16.md"]);
    let stdout = text(&output.stdout);
    // Files are reported in completion order.
    let mut md094 = lines_for(&stdout, "MD094");
    md094.sort_unstable();
    assert_eq!(
        md094,
        [
            "bin.md:1:1: [MD094] File appears to be binary; not linted",
            "u16.md:1:1: [MD094] File appears to be UTF-16 encoded; not linted, convert it to UTF-8",
        ],
        "{stdout}"
    );
    assert!(lines_for(&stdout, "MD018").is_empty(), "{stdout}");
    assert_eq!(output.status.code(), Some(1));

    run_in(dir.path(), &["check", "--fix", "--no-cache", "bin.md", "u16.md"]);
    run_in(dir.path(), &["fmt", "--no-cache", "bin.md", "u16.md"]);
    assert_eq!(fs::read(dir.path().join("bin.md")).unwrap(), binary);
    assert_eq!(fs::read(dir.path().join("u16.md")).unwrap(), utf16);
}

#[test]
fn binary_finding_follows_per_file_ignores() {
    let dir = project(
        b"#bad\n\x00\xff\n",
        Some("[per-file-ignores]\n\"test.md\" = [\"MD094\"]\n"),
    );
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);
    assert!(lines_for(&stdout, "MD094").is_empty(), "{stdout}");
    assert!(lines_for(&stdout, "MD018").is_empty(), "{stdout}");
    assert_eq!(output.status.code(), Some(0), "{stdout}");
}

/// CLI rule selection reaches the binary finding on every adapter. The
/// `--stdin-batch` protocol separates documents with NUL, so its binary
/// document is one that starts with a UTF-16 byte order mark.
#[test]
fn binary_finding_follows_cli_rule_selection() {
    let dir = tempfile::tempdir().unwrap();
    let binary: &[u8] = b"#bad\n\x00\xff\n";
    fs::write(dir.path().join("bin.md"), binary).unwrap();
    let batch: &[u8] = b"doc.md\0\xff\xfe#bad\n\0";

    let selections: [&[&str]; 4] = [
        &[],
        &["--disable", "MD094"],
        &["--extend-disable", "invalid-encoding"],
        &["--enable", "MD009"],
    ];
    for selection in selections {
        let with = |base: &[&'static str]| {
            let mut args = base.to_vec();
            args.extend_from_slice(selection);
            args
        };
        let outputs = [
            ("disk", run_in(dir.path(), &with(&["check", "--no-cache", "bin.md"]))),
            (
                "stdin",
                run_stdin(dir.path(), binary, &with(&["check", "--no-cache", "--stdin"])),
            ),
            (
                "stdin-batch",
                run_stdin(dir.path(), batch, &with(&["check", "--no-cache", "--stdin-batch"])),
            ),
        ];
        for (adapter, output) in outputs {
            let stdout = text(&output.stdout);
            let reported = lines_for(&stdout, "MD094").len();
            let (expected_findings, expected_code) = if selection.is_empty() { (1, 1) } else { (0, 0) };
            assert_eq!(reported, expected_findings, "{adapter} {selection:?}:\n{stdout}");
            assert_eq!(
                output.status.code(),
                Some(expected_code),
                "{adapter} {selection:?}:\n{stdout}{}",
                text(&output.stderr)
            );
        }
    }
}

#[test]
fn binary_stdin_gets_one_finding_and_fmt_echoes_it() {
    let dir = tempfile::tempdir().unwrap();
    let binary: &[u8] = b"#bad\n\x00\xff\n";

    let output = run_stdin(dir.path(), binary, &["check", "--no-cache", "--stdin"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        lines_for(&stdout, "MD094"),
        ["<stdin>:1:1: [MD094] File appears to be binary; not linted"],
        "{stdout}"
    );
    assert!(lines_for(&stdout, "MD018").is_empty(), "{stdout}");

    let output = run_stdin(dir.path(), binary, &["fmt", "--no-cache", "-"]);
    assert_eq!(output.stdout, binary);
}

#[test]
fn valid_utf8_containing_nul_is_linted_as_before() {
    let dir = project(b"#bad\n\x00\n", None);
    let output = run_in(dir.path(), &["check", "--no-cache", "test.md"]);
    let stdout = text(&output.stdout);
    assert!(lines_for(&stdout, "MD094").is_empty(), "{stdout}");
    assert_eq!(lines_for(&stdout, "MD018").len(), 1, "{stdout}");
}

#[test]
fn the_cache_does_not_confuse_different_invalid_bytes() {
    // 0xE9 and 0xE8 both decode to U+FFFD, so the decoded texts are identical;
    // a cached result for the first must not answer for the second.
    let dir = project(LATIN1, None);
    let cache_dir = dir.path().join("cache");
    let cache_dir = cache_dir.to_str().unwrap();
    for (content, expected) in [
        (LATIN1, "0xE9"),
        (&b"# Title\n\nCaf\xe8 menu \n"[..], "0xE8"),
        (LATIN1, "0xE9"),
    ] {
        fs::write(dir.path().join("test.md"), content).unwrap();
        let output = run_in(dir.path(), &["check", "--cache-dir", cache_dir, "test.md"]);
        let stdout = text(&output.stdout);
        assert_eq!(
            lines_for(&stdout, "MD094"),
            [format!(
                "test.md:3:4: [MD094] Invalid UTF-8 byte sequence {expected} (shown as U+FFFD)"
            )],
            "{stdout}"
        );
        assert_eq!(output.status.code(), Some(1));
    }

    // `--fix` with the cache enabled leaves the file alone on every run.
    for run in 0..2 {
        let output = run_in(dir.path(), &["check", "--fix", "--cache-dir", cache_dir, "test.md"]);
        let printed = all_output(&output);
        assert_eq!(lines_for(&printed, "MD094").len(), 1, "run {run}:\n{printed}");
        assert_eq!(fs::read(dir.path().join("test.md")).unwrap(), LATIN1, "run {run}");
    }
}

#[test]
fn an_invalid_file_is_still_a_link_target() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("b.md"), b"# Caf\xe9 Title\n\n## Target\n").unwrap();
    let a = b"# A\n\n[found](b.md#target)\n[missing](b.md#nowhere)\n";
    fs::write(dir.path().join("a.md"), a).unwrap();

    let output = run_in(dir.path(), &["check", "--no-cache", "a.md", "b.md"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        lines_for(&stdout, "MD051"),
        ["a.md:4:1: [MD051] Link fragment 'nowhere' not found in 'b.md'"],
        "{stdout}"
    );

    let output = run_stdin(
        dir.path(),
        a,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let stdout = text(&output.stdout);
    assert_eq!(
        lines_for(&stdout, "MD051"),
        ["a.md:4:1: [MD051] Link fragment 'nowhere' not found in 'b.md'"],
        "{stdout}"
    );
}

#[test]
fn cross_file_findings_in_an_invalid_file_show_its_source() {
    // MD051 is reported after the per-file pass, from a second read of the file.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), b"# Caf\xe9\n\n[x](b.md#nowhere)\n").unwrap();
    fs::write(dir.path().join("b.md"), b"# B\n").unwrap();

    let output = run_in(
        dir.path(),
        &["check", "--no-cache", "--output-format", "full", "a.md", "b.md"],
    );
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("MD051 Link fragment 'nowhere' not found in 'b.md'"),
        "{stdout}"
    );
    assert!(stdout.contains(" 3 | [x](b.md#nowhere)"), "{stdout}");
}

#[test]
fn invalid_rust_source_is_still_a_read_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lib.rs"), b"//! Caf\xe9\n").unwrap();

    let output = run_in(dir.path(), &["check", "--no-cache", "lib.rs"]);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{}", text(&output.stderr));

    let output = run_stdin(
        dir.path(),
        b"//! Caf\xe9\n",
        &["check", "--no-cache", "--stdin", "--stdin-filename", "lib.rs"],
    );
    assert!(
        text(&output.stderr).contains("Error reading from stdin"),
        "{}",
        text(&output.stderr)
    );
    assert!(lines_for(&text(&output.stdout), "MD094").is_empty());
    assert_eq!(output.status.code(), Some(1));
}
