//! The closing summary states each count once, with the noun agreeing with it,
//! for a run over files and for a piped document alike.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// A heading missing the blank line below it: one fixable finding.
const ONE_FIXABLE: &str = "# Title\ntext\n";
/// Two headings missing the blank line below them: two fixable findings.
const TWO_FIXABLE: &str = "# Title\ntext\n\n## Next\nmore\n";
/// One fixable finding and one undefined reference no fix resolves.
const MIXED: &str = "# Title\ntext\n\n[a][missing]\n";
/// Only the undefined reference.
const UNFIXABLE: &str = "# Title\n\n[a][missing]\n";
const CLEAN: &str = "# Title\n\nParagraph body.\n";

fn rumdl(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .args(["--no-cache", "--no-config", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or_default().as_bytes())
        .unwrap();
    child.wait_with_output().expect("failed to collect rumdl output")
}

/// The non-blank lines after the last blank line of `text`, each without its
/// trailing `(Nms)` timing, which is where every summary is printed.
fn summary(text: &str) -> Vec<String> {
    let tail = text.rsplit_once("\n\n").map_or(text, |(_, tail)| tail);
    tail.lines()
        .filter(|line| !line.is_empty())
        .map(|line| match line.rsplit_once(" (") {
            Some((head, timing)) if timing.ends_with("ms)") => head.to_string(),
            _ => line.to_string(),
        })
        .collect()
}

#[test]
fn a_run_over_files_states_each_count_once() {
    let files = [
        ("one.md", ONE_FIXABLE),
        ("two.md", TWO_FIXABLE),
        ("mixed.md", MIXED),
        ("unfixable.md", UNFIXABLE),
        ("clean.md", CLEAN),
    ];
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["check", "one.md"],
            &[
                "Issues: Found 1 issue in 1 file",
                "Run `rumdl fmt` to automatically fix it",
            ],
        ),
        (
            &["check", "two.md"],
            &[
                "Issues: Found 2 issues in 1 file",
                "Run `rumdl fmt` to automatically fix all 2 issues",
            ],
        ),
        (
            &["check", "mixed.md"],
            &[
                "Issues: Found 2 issues in 1 file",
                "Run `rumdl fmt` to automatically fix 1 of the 2 issues",
            ],
        ),
        (
            &["check", "unfixable.md", "clean.md"],
            &["Issues: Found 1 issue in 1/2 files"],
        ),
        (&["check", "clean.md"], &["Success: No issues found in 1 file"]),
        (&["fmt", "--check", "one.md"], &["Would fix: 1/1 issue in 1 file"]),
        (
            &["fmt", "--diff", "one.md", "mixed.md"],
            &["Would fix: 2/3 issues in 2 files"],
        ),
        (&["check", "--fix", "one.md"], &["Fixed: 1/1 issue in 1 file"]),
        (&["fmt", "one.md", "mixed.md"], &["Fixed: 2/3 issues in 2 files"]),
    ];

    for (args, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            fs::write(dir.path().join(name), content).unwrap();
        }
        let output = rumdl(dir.path(), args, None);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(summary(&stdout), *expected, "{args:?}\nstdout:\n{stdout}");
    }
}

#[test]
fn a_piped_document_states_each_count_once() {
    let dir = tempfile::tempdir().unwrap();
    let cases: &[(&[&str], &str, &[&str])] = &[
        (&["check", "-"], ONE_FIXABLE, &["Found 1 issue in <stdin>"]),
        (&["check", "-"], TWO_FIXABLE, &["Found 2 issues in <stdin>"]),
        (
            &["fmt", "--check", "-"],
            ONE_FIXABLE,
            &["1 issue would be fixed, 0 issues remaining"],
        ),
        (
            &["fmt", "--check", "-"],
            MIXED,
            &["1 issue would be fixed, 1 issue remaining"],
        ),
    ];
    for (args, input, expected) in cases {
        let output = rumdl(dir.path(), args, Some(input));
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(summary(&stdout), *expected, "{args:?} {input:?}\nstdout:\n{stdout}");
    }

    // A fix mode writes the document to stdout, so its summary is on stderr.
    for (input, expected) in [
        (ONE_FIXABLE, "1 issue fixed, 0 issues remaining"),
        (TWO_FIXABLE, "2 issues fixed, 0 issues remaining"),
        (MIXED, "1 issue fixed, 1 issue remaining"),
    ] {
        let output = rumdl(dir.path(), &["fmt", "-"], Some(input));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(summary(&stderr), [expected], "fmt - {input:?}\nstderr:\n{stderr}");
    }
}

#[test]
fn format_only_markdown_changes_are_not_counted_as_lint_findings() {
    const ORIGINAL: &str = "# Title\n\n```markdown\n#  Inside\n```\n";
    const FORMATTED: &str = "# Title\n\n```markdown\n# Inside\n```\n";
    for (args, preview) in [
        (vec!["fmt", "--check"], true),
        (vec!["check", "--diff"], true),
        (vec!["fmt", "--check", "--only-code-block-tools"], true),
        (vec!["fmt"], false),
        (vec!["check", "--fix"], false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            "[global]\nenable = [\"MD019\"]\n[code-block-tools]\nenabled = true\n\
             [code-block-tools.languages.markdown]\nformat = [\"rumdl:format\"]\n",
        )
        .unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, ORIGINAL).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(&args)
            .args(["--no-cache", "--color", "never", "doc.md"])
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(output.status.code(), Some(i32::from(preview)), "{args:?}: {stdout}");
        assert_eq!(
            summary(&stdout),
            [if preview {
                "Would format: 1 file"
            } else {
                "Formatted: 1 file"
            }],
            "{args:?}: {stdout}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            if preview { ORIGINAL } else { FORMATTED }
        );
    }

    // A second file with a real lint finding must not turn one resolved warning
    // plus one format-only change into "2/1 issues" in the aggregate summary.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), ORIGINAL).unwrap();
    fs::write(dir.path().join("outer.md"), "#  Outside\n").unwrap();
    let output = rumdl(
        dir.path(),
        &[
            "fmt",
            "--enable",
            "MD019",
            "--config",
            "code-block-tools.enabled=true\ncode-block-tools.languages.markdown.format=[\"rumdl:format\"]",
            "doc.md",
            "outer.md",
        ],
        None,
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stdout}");
    assert_eq!(summary(&stdout), ["Fixed: 1/1 issue in 2 files"], "{stdout}");
    assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), FORMATTED);
    assert_eq!(fs::read_to_string(dir.path().join("outer.md")).unwrap(), "# Outside\n");
}
