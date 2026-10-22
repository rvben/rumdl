//! CLI contracts for bounded parallel execution of independent fenced blocks.
#![cfg(feature = "parallel")]

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "support/native_tool.rs"]
mod native_tool;

fn setup(mode: &str, extra: &str, languages: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("pyproject.toml"), "").unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            r#"
[code-block-tools]
enabled = true
normalize-language = "exact"
on-no-tools-run = "ignore"
on-error = "warn"
binary-preferences = {{ rumdl-policy-test = "project" }}
{extra}
[code-block-tools.tools.test]
command = ["rumdl-policy-test"]
stdin = true
stdout = true
[code-block-tools.languages]
{languages}
"#
        ),
    )
    .unwrap();
    native_tool::install(dir.path(), mode);
    dir
}

fn run(root: &Path, threads: usize, command: &str, files: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .env("RAYON_NUM_THREADS", threads.to_string())
        .args([
            command,
            "--only-code-block-tools",
            "--no-cache",
            "--output-format",
            "json",
        ])
        .args(files)
        .output()
        .unwrap()
}

fn state(root: &Path) -> Vec<usize> {
    fs::read_to_string(root.join("process-state"))
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect()
}

#[test]
fn independent_blocks_overlap_but_diagnostics_stay_in_source_order() {
    let dir = setup("parallel-finding", "", "python = { lint = [\"test\"] }");
    fs::write(
        dir.path().join("doc.md"),
        "```python\nfirst\n```\n\n```python\nsecond\n```\n",
    )
    .unwrap();
    let output = run(dir.path(), 2, "check", &["doc.md"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = findings.as_array().unwrap();
    assert_eq!(findings.len(), 2, "{output:?}");
    assert_eq!(findings[0]["line"], 2);
    assert_eq!(findings[0]["message"], "finding-first");
    assert_eq!(findings[1]["line"], 6);
    assert_eq!(findings[1]["message"], "finding-second");
    assert_eq!(state(dir.path()), vec![0, 2, 2], "{output:?}");
}

#[test]
fn format_blocks_overlap_and_keep_original_offsets_and_container_prefixes() {
    let dir = setup(
        "parallel-format",
        "",
        "python = { format = [\"test\"] }\nshell = { format = [\"test\"] }",
    );
    let input = "Intro π.\n\n> ```python\n> first bad\n> ```\n\n```shell\nsecond bad\n```\n";
    fs::write(dir.path().join("doc.md"), input).unwrap();
    let output = run(dir.path(), 2, "fmt", &["doc.md"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(state(dir.path()), vec![0, 2, 2], "{output:?}");
    let expected = "Intro π.\n\n> ```python\n> first good and longer\n> ```\n\n```shell\nsecond good and longer\n```\n";
    assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), expected);
    let again = run(dir.path(), 2, "fmt", &["doc.md"]);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), expected);
}

fn append_config(root: &Path, text: &str) {
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(root.join(".rumdl.toml"))
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

#[test]
fn uneven_files_share_the_worker_budget_and_match_one_worker_output() {
    let dir = setup("serial-finding", "", "python = { lint = [\"test\"] }");
    for (file, count) in [("a.md", 1), ("b.md", 9), ("c.md", 2)] {
        let content: String = (0..count)
            .map(|n| format!("```python\nfirst-{file}-{n}\n```\n\n"))
            .collect();
        fs::write(dir.path().join(file), content).unwrap();
    }
    let sequential = run(dir.path(), 1, "check", &["a.md", "b.md", "c.md"]);
    assert_eq!(sequential.status.code(), Some(1), "{sequential:?}");
    assert_eq!(state(dir.path()), vec![0, 1, 12], "{sequential:?}");
    fs::remove_file(dir.path().join("process-state")).unwrap();
    let parallel = run(dir.path(), 2, "check", &["a.md", "b.md", "c.md"]);
    assert_eq!(parallel.status.code(), sequential.status.code(), "{parallel:?}");
    assert_eq!(parallel.stdout, sequential.stdout);
    assert_eq!(parallel.stderr, sequential.stderr);
    assert_eq!(state(dir.path()), vec![0, 2, 12], "{parallel:?}");
}

#[test]
fn effective_language_error_policy_controls_the_active_phase() {
    let dir = setup(
        "parallel-finding",
        "",
        "python = { lint = [\"test\"], on-error = \"warn\" }\nshell = { format = [\"test\"] }",
    );
    let path = dir.path().join(".rumdl.toml");
    // Shell inherits fail, but has no tool in this phase. Python overrides fail.
    let config = fs::read_to_string(&path)
        .unwrap()
        .replacen("on-error = \"warn\"", "on-error = \"fail\"", 1);
    fs::write(path, config).unwrap();
    fs::write(
        dir.path().join("doc.md"),
        "```python\nfirst\n```\n\n```python\nsecond\n```\n\n```shell\nignored\n```\n",
    )
    .unwrap();
    let output = run(dir.path(), 2, "check", &["doc.md"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(state(dir.path()), vec![0, 2, 2], "{output:?}");
    let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings.as_array().unwrap().len(), 2, "{output:?}");
}

#[test]
fn fail_error_policy_keeps_serial_traversal_and_does_not_write_failed_document() {
    for policy in ["global", "language"] {
        for command in ["check", "fmt"] {
            let mode = if command == "check" {
                "serial-finding"
            } else {
                "serial-format"
            };
            let slot = if command == "check" { "lint" } else { "format" };
            let broken_policy = if policy == "language" {
                ", on-error = \"fail\""
            } else {
                ""
            };
            let dir = setup(
                mode,
                "timeout = 500",
                &format!("python = {{ {slot} = [\"test\"] }}\nbrokenlang = {{ {slot} = [\"broken\"]{broken_policy} }}"),
            );
            let config_path = dir.path().join(".rumdl.toml");
            if policy == "global" {
                let config = fs::read_to_string(&config_path)
                    .unwrap()
                    .replace("on-error = \"warn\"", "on-error = \"fail\"");
                fs::write(config_path, config).unwrap();
            }
            let broken_mode = if command == "check" { "slow" } else { "fail" };
            append_config(
                dir.path(),
                &format!("\n[code-block-tools.tools.broken]\ncommand = [\"rumdl-policy-test\", {broken_mode:?}]\n"),
            );
            let input = "```python\nfirst bad\n```\n\n```brokenlang\nfail\n```\n\n```python\nlast bad\n```\n";
            fs::write(dir.path().join("doc.md"), input).unwrap();
            let output = run(dir.path(), 4, command, &["doc.md"]);
            assert!(!output.status.success(), "{policy}/{command}: {output:?}");
            assert_eq!(state(dir.path()), vec![0, 1, 1], "{policy}/{command}: {output:?}");
            let expected_input = if command == "check" {
                "first bad\n"
            } else {
                "last bad\n"
            };
            assert_eq!(
                fs::read_to_string(dir.path().join("process-inputs")).unwrap(),
                expected_input,
                "{policy}/{command}: {output:?}"
            );

            assert_eq!(
                fs::read_to_string(dir.path().join("doc.md")).unwrap(),
                input,
                "{policy}/{command}"
            );
            if command == "check" {
                let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(findings.as_array().unwrap().len(), 2, "{policy}/{command}: {output:?}");
                assert_eq!(
                    findings[0]["message"], "finding-first bad",
                    "{policy}/{command}: {output:?}"
                );
            }
        }
    }
}

#[test]
fn fail_fast_gap_does_not_start_later_blocks() {
    for command in ["check", "fmt"] {
        let dir = setup(
            "serial-format",
            "on-missing-language-definition = \"fail-fast\"",
            "python = { lint = [\"test\"], format = [\"test\"] }",
        );
        let input = "```missing\nunknown\n```\n\n```python\nbad\n```\n";
        fs::write(dir.path().join("doc.md"), input).unwrap();
        let output = run(dir.path(), 4, command, &["doc.md"]);
        assert!(!output.status.success(), "{command}: {output:?}");
        assert!(!dir.path().join("process-state").exists(), "{command}: {output:?}");
        assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), input);
    }
}

#[test]
fn parallel_blocks_preserve_formatter_fallback_and_pipeline_order() {
    for (mode, tools, expected) in [
        ("fallback", "[\"broken\", \"first\", \"upper\"]", "good and longer"),
        ("pipeline", "[\"first\", \"broken\", \"upper\"]", "GOOD AND LONGER"),
    ] {
        let dir = setup(
            "parallel-format",
            "",
            &format!("python = {{ format = {tools}, format-mode = {mode:?} }}"),
        );
        append_config(
            dir.path(),
            "\n[code-block-tools.tools.first]\ncommand = [\"rumdl-policy-test\"]\n[code-block-tools.tools.upper]\ncommand = [\"rumdl-policy-test\", \"uppercase\"]\n[code-block-tools.tools.broken]\ncommand = [\"rumdl-policy-test\", \"fail\"]\n",
        );
        fs::write(
            dir.path().join("doc.md"),
            "```python\nfirst bad\n```\n\n```python\nsecond bad\n```\n",
        )
        .unwrap();
        let output = run(dir.path(), 2, "fmt", &["doc.md"]);
        assert!(output.status.success(), "{mode}: {output:?}");
        let first = if mode == "pipeline" { "FIRST" } else { "first" };
        let second = if mode == "pipeline" { "SECOND" } else { "second" };
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            format!("```python\n{first} {expected}\n```\n\n```python\n{second} {expected}\n```\n")
        );
        assert_eq!(state(dir.path()), vec![0, 2, 2], "{mode}: {output:?}");
        let warnings = String::from_utf8_lossy(&output.stderr);
        let first = warnings.find("doc.md:1:").unwrap_or_else(|| panic!("{output:?}"));
        let second = warnings.find("doc.md:5:").unwrap_or_else(|| panic!("{output:?}"));
        assert!(first < second, "{output:?}");
    }
}

#[test]
fn parallel_format_failures_are_collected_in_source_order() {
    let dir = setup(
        "slow",
        "timeout = 500",
        "python = { format = [\"test\"] }\nshell = { format = [\"broken\"] }",
    );
    append_config(
        dir.path(),
        "\n[code-block-tools.tools.broken]\ncommand = [\"rumdl-policy-test\", \"fail-message\"]\n",
    );
    fs::write(
        dir.path().join("doc.md"),
        "```python\nfirst\n```\n\n```shell\nsecond\n```\n",
    )
    .unwrap();
    let output = run(dir.path(), 2, "fmt", &["doc.md"]);
    assert!(output.status.success(), "{output:?}");
    let warnings = String::from_utf8_lossy(&output.stderr);
    assert_eq!(warnings.matches("timed out after 500ms").count(), 1, "{output:?}");
    assert_eq!(warnings.matches("fixture format failure").count(), 1, "{output:?}");
    assert!(
        warnings.find("doc.md:1:").unwrap() < warnings.find("doc.md:5:").unwrap(),
        "{output:?}"
    );
}

#[test]
fn repeated_timeouts_stop_new_attempts_without_losing_block_warnings() {
    for (threads, blocked_entry) in [(1, false), (4, false), (1, true), (4, true)] {
        let dir = setup("slow", "timeout = 100", "python = { lint = [\"test\"] }");
        let tool = native_tool::install(dir.path(), "slow");
        fs::write(tool.with_extension("record"), "").unwrap();
        let _entry_lock = blocked_entry.then(|| {
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(tool.with_extension("entry-lock"))
                .unwrap();
            lock.lock().unwrap();
            lock
        });
        let input: String = (0..12).map(|i| format!("```python\nblock-{i}\n```\n\n")).collect();
        fs::write(dir.path().join("doc.md"), &input).unwrap();
        let output = run(dir.path(), threads, "check", &["doc.md"]);
        assert!(output.status.success(), "{threads}: {output:?}");
        let entries = match fs::read_to_string(tool.with_extension("calls")) {
            Ok(calls) => calls.lines().count(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => panic!("read child entries: {error}"),
        };
        let warnings = String::from_utf8_lossy(&output.stderr);
        let timeouts = warnings.matches("timed out after 100ms").count();
        let skipped = warnings.matches("skipped after timing out").count();
        // A child may time out before it reaches its entry recorder. The CLI
        // reports every timeout, including those unobserved by the child.
        // Three timeouts disable the tool; in-flight attempts may still time out
        // later, but new work cannot multiply that bound by the block count.
        assert!(
            (3..=3 + threads - 1).contains(&timeouts),
            "{threads} workers, blocked entry = {blocked_entry}: {output:?}"
        );
        assert_eq!(timeouts + skipped, 12, "{output:?}");
        assert!(
            entries <= timeouts,
            "{entries} child entries, {timeouts} timeouts: {output:?}"
        );
        if blocked_entry {
            assert_eq!(entries, 0, "a held lock prevents entry: {output:?}");
        }
        let positions: Vec<_> = (0..12)
            .map(|i| {
                warnings
                    .find(&format!("doc.md:{}:", i * 4 + 1))
                    .unwrap_or_else(|| panic!("{output:?}"))
            })
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{output:?}");
        assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), input);
    }
}

#[test]
fn parallel_tools_drain_both_pipes_without_starving_workers() {
    let dir = setup("parallel-pipes", "", "python = { format = [\"test\"] }");
    let code = "large_tool_input ".repeat(9000);
    let input = format!("```python\n{code}\n```\n\n```python\n{code}\n```\n");
    fs::write(dir.path().join("doc.md"), &input).unwrap();
    let output = run(dir.path(), 2, "fmt", &["doc.md"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(state(dir.path()), vec![0, 2, 2], "{output:?}");
    assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), input);
}

#[test]
fn lint_after_parallel_format_uses_updated_line_positions() {
    let dir = setup(
        "parallel-expand",
        "",
        "python = { lint = [\"finding\"], format = [\"test\"] }",
    );
    append_config(
        dir.path(),
        "\n[code-block-tools.tools.finding]\ncommand = [\"rumdl-policy-test\", \"finding\"]\n",
    );
    fs::write(
        dir.path().join("doc.md"),
        "```python\nfirst bad\n```\n\n```python\nsecond bad\n```\n",
    )
    .unwrap();
    let output = run(dir.path(), 2, "fmt", &["doc.md"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\nfirst good\nextra\n```\n\n```python\nsecond good\nextra\n```\n"
    );
    let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings.as_array().unwrap().len(), 2, "{output:?}");
    assert_eq!(findings[0]["line"], 2);
    assert_eq!(findings[1]["line"], 7);
    assert_eq!(state(dir.path()), vec![0, 2, 2], "{output:?}");
}

#[test]
fn one_worker_and_parallel_format_emit_warnings_in_the_same_source_order() {
    let mut previous = None;
    for threads in [1, 2] {
        let dir = setup("serial-format", "", "python = { format = [\"broken\", \"test\"] }");
        append_config(
            dir.path(),
            "\n[code-block-tools.tools.broken]\ncommand = [\"rumdl-policy-test\", \"fail\"]\n",
        );
        fs::write(
            dir.path().join("doc.md"),
            "```python\nfirst bad\n```\n\n```python\nsecond bad\n```\n",
        )
        .unwrap();
        let output = run(dir.path(), threads, "fmt", &["doc.md"]);
        assert!(output.status.success(), "{output:?}");
        let warnings = String::from_utf8_lossy(&output.stderr);
        assert!(
            warnings.find("doc.md:1:").unwrap() < warnings.find("doc.md:5:").unwrap(),
            "{threads}: {output:?}"
        );
        let actual = (
            output.stdout,
            output.stderr,
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        );
        if let Some(expected) = previous {
            assert_eq!(actual, expected);
        }
        previous = Some(actual);
    }
}
