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
        assert!(diagnostics.contains("merge-conflict"), "{args:?}: {diagnostics}");
        assert!(!diagnostics.contains("[fixed]"));
        if args[0] == "fmt" {
            assert!(String::from_utf8_lossy(&output.stderr).contains("formatting skipped"));
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
            assert!(String::from_utf8_lossy(&output.stderr).contains("merge-conflict"));
        }
    }
}

#[test]
fn merge_conflict_is_independent_of_rules_and_has_structured_diagnostic() {
    let dir = TempDir::new().unwrap();
    let content = format!("<!-- rumdl-disable -->\n{CONFLICT}");
    fs::write(dir.path().join("conflict.md"), &content).unwrap();
    for stdin in [false, true] {
        let output = run(
            &dir,
            &[
                "check",
                "--enable",
                "MD009",
                "--output-format",
                "json",
                if stdin { "-" } else { "conflict.md" },
            ],
            stdin.then_some(content.as_str()),
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(diagnostics.as_array().unwrap().len(), 1);
        assert_eq!(diagnostics[0]["rule"], "merge-conflict");
        assert_eq!(diagnostics[0]["line"], 4);
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
    assert!(!String::from_utf8_lossy(&output.stderr).contains("merge-conflict"));
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
        assert_eq!(diagnostics[0]["rule"], "merge-conflict");
        assert_eq!(fs::read(dir.path().join("conflict.md")).unwrap(), before);
    }
}

#[test]
fn merge_conflict_stdin_batch_reports_conflict_with_rules_disabled() {
    let dir = TempDir::new().unwrap();
    let input = format!("conflict.md\0{CONFLICT}\0clean.md\0# Title\n\0");
    let output = run(
        &dir,
        &["check", "--stdin-batch", "--enable", "MD009", "--output-format", "json"],
        Some(&input),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnostics.as_array().unwrap().len(), 1);
    assert_eq!(diagnostics[0]["rule"], "merge-conflict");
}
