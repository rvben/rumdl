use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const CONFLICT: &str = "# Test\r\n\n<<<<<<< HEAD\r\nThis is a test.  \n=======\r\nHello, world!\n>>>>>>> master";

fn run(dir: &TempDir, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .current_dir(dir.path())
        .args(args)
        .args(["--isolated", "--no-cache"]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(content) = stdin {
        child.stdin.take().unwrap().write_all(content.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn merge_conflict_file_modes_preserve_bytes_and_report_conflict() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("conflict.md");
    for (args, code) in [
        (vec!["check", "conflict.md"], 1),
        (vec!["check", "--fix", "conflict.md"], 1),
        (vec!["check", "--diff", "conflict.md"], 1),
        (vec!["fmt", "conflict.md"], 0),
        (vec!["fmt", "--check", "conflict.md"], 0),
        (vec!["fmt", "--diff", "conflict.md"], 0),
    ] {
        fs::write(&path, CONFLICT).unwrap();
        let output = run(&dir, &args, None);
        assert_eq!(output.status.code(), Some(code), "{args:?}: {output:?}");
        assert_eq!(fs::read(&path).unwrap(), CONFLICT.as_bytes());
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(diagnostics.contains("MD092"), "{args:?}: {diagnostics}");
        assert!(!diagnostics.contains("[fixed]"));
        assert!(diagnostics.contains("formatting skipped"), "{args:?}: {diagnostics}");
    }
}

/// The safeguard is a rule, so the invocation's rule selection decides whether
/// it reports, the same way on every adapter that reads a document. A selection
/// that drops it leaves the conflicted document to the ordinary rules.
#[test]
fn merge_conflict_finding_follows_cli_rule_selection() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("conflict.md"), CONFLICT).unwrap();
    let batch = format!("conflict.md\0{CONFLICT}\0");
    let selections: [&[&str]; 4] = [
        &[],
        &["--disable", "MD092"],
        &["--extend-disable", "merge-conflict"],
        &["--enable", "MD009"],
    ];
    for selection in selections {
        let with = |base: &[&'static str]| {
            let mut args = base.to_vec();
            args.extend_from_slice(selection);
            args
        };
        let runs = [
            ("path", run(&dir, &with(&["check", "conflict.md"]), None)),
            ("stdin", run(&dir, &with(&["check", "--stdin"]), Some(CONFLICT))),
            (
                "stdin-batch",
                run(&dir, &with(&["check", "--stdin-batch"]), Some(&batch)),
            ),
        ];
        for (adapter, output) in runs {
            let diagnostics = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let reported = diagnostics.matches("MD092").count();
            let expected = usize::from(selection.is_empty());
            assert_eq!(reported, expected, "{adapter} {selection:?}:\n{diagnostics}");
        }
    }
}

#[test]
fn merge_conflict_stdin_preserves_mixed_endings_and_missing_final_newline() {
    let dir = TempDir::new().unwrap();
    for (args, code) in [
        (vec!["fmt", "-"], 0),
        (vec!["fmt", "--silent", "-"], 0),
        (vec!["check", "--fix", "-"], 1),
        (vec!["check", "--fix", "--fail-on", "never", "-"], 0),
    ] {
        let output = run(&dir, &args, Some(CONFLICT));
        assert_eq!(output.status.code(), Some(code), "{args:?}: {output:?}");
        assert_eq!(output.stdout, CONFLICT.as_bytes());
        if args.contains(&"--silent") {
            assert!(output.stderr.is_empty());
        } else {
            assert!(String::from_utf8_lossy(&output.stderr).contains("MD092"));
        }
    }
}

#[test]
fn merge_conflict_has_structured_diagnostic() {
    let dir = TempDir::new().unwrap();
    let content = CONFLICT.to_string();
    fs::write(dir.path().join("conflict.md"), &content).unwrap();
    for stdin in [false, true] {
        let output = run(
            &dir,
            &[
                "check",
                "--enable",
                "MD092",
                "--output-format",
                "json",
                if stdin { "-" } else { "conflict.md" },
            ],
            stdin.then_some(content.as_str()),
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(diagnostics.as_array().unwrap().len(), 1);
        assert_eq!(diagnostics[0]["rule"], "MD092");
        assert_eq!(diagnostics[0]["line"], 3);
    }
}

#[test]
fn merge_conflict_does_not_prevent_other_files_from_formatting() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("conflict.md"), CONFLICT).unwrap();
    fs::write(dir.path().join("clean.md"), "# Title\n\nText   \n").unwrap();
    let output = run(&dir, &["fmt", "conflict.md", "clean.md"], None);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(dir.path().join("conflict.md")).unwrap(), CONFLICT.as_bytes());
    assert_eq!(
        fs::read_to_string(dir.path().join("clean.md")).unwrap(),
        "# Title\n\nText\n"
    );
}

#[test]
fn merge_conflict_fenced_partial_and_custom_markers_skip_whole_document() {
    let dir = TempDir::new().unwrap();
    for marker in ["<<<<<<< HEAD", ">>>>>>> branch", "<<<<<<<<< HEAD"] {
        let content = format!("# Title\n\nText   \n\n```text\n{marker}\n```\n");
        let output = run(&dir, &["fmt", "-"], Some(&content));
        assert!(output.status.success());
        assert_eq!(output.stdout, content.as_bytes());
    }
    let output = run(&dir, &["fmt", "-"], Some("Title\n=======\n"));
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("MD092"));
}

#[test]
fn merge_conflict_skips_external_tools_even_in_tools_only_mode() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("conflict.md"),
        format!("{CONFLICT}\n\n```testlang\ntext\n```\n"),
    )
    .unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        r#"
[code-block-tools]
enabled = true
[code-block-tools.tools.unavailable]
command = ["rumdl-858-tool-that-must-not-run"]
stdin = true
stdout = true
[code-block-tools.languages]
testlang = { lint = ["unavailable"], format = ["unavailable"] }
"#,
    )
    .unwrap();
    let before = fs::read(dir.path().join("conflict.md")).unwrap();
    for mode in ["check", "fmt"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args([
                mode,
                "--only-code-block-tools",
                "--no-cache",
                "--output-format",
                "json",
                "conflict.md",
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(if mode == "check" { 1 } else { 0 }),
            "{output:?}"
        );
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(diagnostics.as_array().unwrap().len(), 1, "{diagnostics}");
        assert_eq!(diagnostics[0]["rule"], "MD092");
        assert_eq!(fs::read(dir.path().join("conflict.md")).unwrap(), before);
    }
}

/// Dropping the rule drops the guard whole: the same run that rewrites the outer
/// Markdown of a conflicted document formats its fenced code too. Rewriting the
/// document while silently skipping its code blocks would be half a format.
#[cfg(unix)]
#[test]
fn external_formatters_follow_the_cli_rule_selection() {
    let project = || {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            concat!(
                "[code-block-tools]\n",
                "enabled = true\n\n",
                "[code-block-tools.tools.fakefmt]\n",
                "command = [\"sh\", \"-c\", \"cat >/dev/null; printf 'FORMATTED\\\\n'\"]\n\n",
                "[code-block-tools.languages.python]\n",
                "format = [\"fakefmt\"]\n",
            ),
        )
        .unwrap();
        fs::write(
            dir.path().join("conflict.md"),
            format!("{CONFLICT}\n\n```python\nx=1\n```\n"),
        )
        .unwrap();
        dir
    };
    let fmt = |dir: &TempDir, args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(["fmt", "--no-cache", "conflict.md"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        fs::read_to_string(dir.path().join("conflict.md")).unwrap()
    };

    let guarded = project();
    let before = fs::read(guarded.path().join("conflict.md")).unwrap();
    assert!(
        !fmt(&guarded, &[]).contains("FORMATTED"),
        "the tool ran on a guarded file"
    );
    assert_eq!(fs::read(guarded.path().join("conflict.md")).unwrap(), before);

    let selected_out = project();
    assert!(
        fmt(&selected_out, &["--disable", "MD092"]).contains("FORMATTED"),
        "the tool was skipped for a rule the run had disabled"
    );
}

/// `--enable` replaces the selection the way ruff's `--select` does, so it turns a
/// rule back on that configuration had disabled. The safeguard follows the selection
/// it is part of: the run reports the conflict and writes nothing.
#[test]
fn merge_conflict_enable_flag_outranks_a_configuration_disable() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "[global]\ndisable = [\"MD092\"]\n").unwrap();
    let path = dir.path().join("conflict.md");
    // MD047 is in every selection below, and this document ends without its
    // newline, so each run has something to rewrite: an intact file is evidence
    // the safeguard held rather than evidence that no rule wanted to touch it.
    let format_with = |selection: &[&str]| {
        fs::write(&path, CONFLICT).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(["fmt", "conflict.md", "--no-cache"])
            .args(selection)
            .output()
            .unwrap();
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        (diagnostics, fs::read(&path).unwrap())
    };

    let (diagnostics, bytes) = format_with(&["--enable", "MD092,MD047"]);
    assert_eq!(bytes, CONFLICT.as_bytes(), "the run rewrote a conflicted file");
    assert!(diagnostics.contains("MD092"), "{diagnostics}");

    // The same run without the safeguard in its selection rewrites the file, so
    // the byte-identity above is the safeguard's doing.
    let (diagnostics, rewritten) = format_with(&["--enable", "MD047"]);
    assert_ne!(rewritten, CONFLICT.as_bytes(), "{diagnostics}");
    assert!(!diagnostics.contains("MD092"), "{diagnostics}");

    // With no selection on the command line the configuration disable stands.
    let (diagnostics, rewritten) = format_with(&[]);
    assert_ne!(rewritten, CONFLICT.as_bytes(), "{diagnostics}");
    assert!(!diagnostics.contains("MD092"), "{diagnostics}");
}

/// A conflicted link target indexes as nothing, so a fragment into it is reported
/// missing rather than resolved against half-merged headings. A run that dropped
/// MD092 indexes it like any other document, and every adapter answers the same.
#[test]
fn conflicted_link_target_is_indexed_when_the_selection_drops_the_rule() {
    let dir = TempDir::new().unwrap();
    let source = "# Source\n\nSee [the section](target.md#section).\n";
    fs::write(dir.path().join("source.md"), source).unwrap();
    fs::write(dir.path().join("target.md"), format!("## Section\n\n{CONFLICT}\n")).unwrap();

    for (selection, expect_md051) in [(&[][..], true), (&["--disable", "MD092"][..], false)] {
        let mut path_args = vec!["check", "source.md", "target.md"];
        path_args.extend_from_slice(selection);
        let mut stdin_args = vec!["check", "--stdin", "--stdin-filename", "source.md"];
        stdin_args.extend_from_slice(selection);
        for (adapter, output) in [
            ("path", run(&dir, &path_args, None)),
            ("stdin", run(&dir, &stdin_args, Some(source))),
        ] {
            let diagnostics = String::from_utf8_lossy(&output.stdout).into_owned();
            assert_eq!(
                diagnostics.contains("MD051"),
                expect_md051,
                "{adapter} {selection:?}:\n{diagnostics}"
            );
        }
    }
}

#[test]
fn merge_conflict_stdin_batch_reports_selected_rule() {
    let dir = TempDir::new().unwrap();
    let input = format!("conflict.md\0{CONFLICT}\0clean.md\0# Title\n\0");
    let output = run(
        &dir,
        &["check", "--stdin-batch", "--enable", "MD092", "--output-format", "json"],
        Some(&input),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnostics.as_array().unwrap().len(), 1);
    assert_eq!(diagnostics[0]["rule"], "MD092");
}

const EXAMPLE: &str = "<!-- rumdl-disable merge-conflict -->\n```text\n<<<<<<< conflict 1 of 1\n%%%%%%% diff from: base\n-old\n+new\n+++++++ side B\nother\n>>>>>>> conflict 1 of 1 ends\n```\n<!-- rumdl-enable merge-conflict -->\n";

#[test]
fn merge_conflict_scoped_example_allows_surrounding_lint_and_format() {
    let dir = TempDir::new().unwrap();
    let content = format!("# Title\n\n{EXAMPLE}\nText   \n");
    let expected = content.replace("Text   ", "Text");
    fs::write(dir.path().join("example.md"), &content).unwrap();
    let check = run(&dir, &["check", "example.md", "--output-format", "json"], None);
    assert_eq!(check.status.code(), Some(1));
    let diagnostics: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(diagnostics.as_array().unwrap().iter().any(|d| d["rule"] == "MD009"));
    assert!(!diagnostics.as_array().unwrap().iter().any(|d| d["rule"] == "MD092"));
    assert!(check.stderr.is_empty(), "{check:?}");
    let formatted = run(&dir, &["fmt", "example.md"], None);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(dir.path().join("example.md")).unwrap(), expected);
    let check = run(&dir, &["check", "--extend-enable", "MD087", "example.md"], None);
    assert!(check.status.success(), "{check:?}");
    let stdin = run(&dir, &["fmt", "-"], Some(&content));
    assert_eq!(stdin.stdout, expected.as_bytes());
}

#[test]
fn merge_conflict_after_suppressed_example_still_protects_every_byte() {
    let dir = TempDir::new().unwrap();
    for marker in ["<<<<<<< HEAD", ">>>>>>> side", "<<<<<<<<< conflict 1 of 1"] {
        for fenced in [false, true] {
            let conflict = if fenced {
                format!("```text\r\n{marker}\n```")
            } else {
                marker.into()
            };
            let content = format!("# Title\r\n\n{EXAMPLE}\nText   \r\n\n{conflict}");
            fs::write(dir.path().join("example.md"), &content).unwrap();
            for (args, code) in [
                (vec!["check", "--fix", "example.md"], 1),
                (vec!["fmt", "example.md"], 0),
                (vec!["fmt", "--diff", "example.md"], 0),
            ] {
                let output = run(&dir, &args, None);
                assert_eq!(output.status.code(), Some(code), "{output:?}");
                assert_eq!(fs::read(dir.path().join("example.md")).unwrap(), content.as_bytes());
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(text.contains("MD092"), "{output:?}");
            }
            let output = run(&dir, &["fmt", "-"], Some(&content));
            assert_eq!(output.stdout, content.as_bytes());
            assert!(String::from_utf8_lossy(&output.stderr).contains("MD092"));
        }
    }
}

#[test]
fn merge_conflict_obeys_global_per_file_and_inline_configuration() {
    let dir = TempDir::new().unwrap();
    let content = "# Title\n\n```text\n<<<<<<< HEAD\n```\n\nText   \n";
    for config in [
        "[global]\ndisable = ['merge-conflict']\n",
        "[global]\nenable = []\n",
        "[global]\nextend-disable = ['MD092']\n",
        "[MD092]\nenabled = false\n",
        "[per-file-ignores]\n'example.md' = ['merge-conflict']\n",
    ] {
        fs::write(dir.path().join("rumdl.toml"), config).unwrap();
        fs::write(dir.path().join("example.md"), content).unwrap();
        // Explicit config rather than --isolated, which intentionally ignores it.
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(["fmt", "--config", "rumdl.toml", "--no-cache", "example.md"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{config}: {output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("MD092"),
            "{config}: {output:?}"
        );
        let expected = if config.contains("enable = []") {
            content.to_string()
        } else {
            content.replace("Text   ", "Text")
        };
        assert_eq!(
            fs::read_to_string(dir.path().join("example.md")).unwrap(),
            expected,
            "{config}"
        );
    }
    for directive in [
        "<!-- rumdl-disable-file MD092 -->",
        "<!-- rumdl-configure-file {\"merge-conflict\": false} -->",
        "<!-- rumdl-disable -->",
    ] {
        let content = format!("{directive}\n{content}");
        let output = run(&dir, &["check", "--output-format", "json", "-"], Some(&content));
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            !diagnostics.as_array().unwrap().iter().any(|d| d["rule"] == "MD092"),
            "{output:?}"
        );
    }
}

#[test]
fn merge_conflict_directive_inside_nested_code_example_does_not_disable_safety() {
    let dir = TempDir::new().unwrap();
    let content = "# Title\n\n````markdown\n<!-- rumdl-disable MD092 -->\n```text\n<<<<<<< HEAD\n```\n````\n\nText   ";
    let output = run(&dir, &["fmt", "-"], Some(content));
    assert_eq!(output.stdout, content.as_bytes());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MD092"));
}
