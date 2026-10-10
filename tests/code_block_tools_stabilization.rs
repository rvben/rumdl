//! Invocation-level coverage contracts and project binary cache identity.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/native_tool.rs"]
mod native_tool;

fn write_tool(root: &Path, mode: &str) {
    native_tool::install(root, mode);
}

fn setup(extra: &str, languages: &str) -> tempfile::TempDir {
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
    write_tool(dir.path(), "quiet");
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn diagnostics(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {output:?}"))
}

#[test]
fn coverage_warn_deduplicates_across_files_and_cache_hits() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let content = "```pyhton\nx\n```\n\n```pyhton\ny\n```\n";
    for file in ["a.md", "b.md"] {
        fs::write(dir.path().join(file), content).unwrap();
    }
    for _ in 0..2 {
        let output = run(dir.path(), &["check", "--only-code-block-tools", "a.md", "b.md"]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(
            stderr(&output)
                .matches("Unrecognized code block language tag 'pyhton'")
                .count(),
            1
        );
        assert!(!stderr(&output).contains("No tools configured"));
    }
}

#[test]
fn coverage_fail_reports_each_block_in_json_and_continues_valid_tools() {
    let dir = setup(
        "on-missing-language-tag = \"fail\"\non-unknown-language-tag = \"fail\"",
        "python = { lint = [\"test\"] }",
    );
    write_tool(dir.path(), "count");
    fs::write(
        dir.path().join("t.md"),
        "```\nx\n```\n\n```pyhton\nx\n```\n\n```python\nx\n```\n",
    )
    .unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "--output-format",
            "json",
            "t.md",
        ],
    );
    assert!(!output.status.success());
    let findings = diagnostics(&output);
    assert_eq!(findings.as_array().unwrap().len(), 2, "{findings}");
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
}

#[test]
fn fail_fast_stops_before_later_files_and_does_not_write_current_file() {
    let dir = setup(
        "on-missing-language-tag = \"fail-fast\"",
        "python = { lint = [\"test\"], format = [\"test\"] }",
    );
    write_tool(dir.path(), "count-echo");
    let first = "#  Title\n\n```\nx\n```\n";
    fs::write(dir.path().join("a.md"), first).unwrap();
    fs::write(dir.path().join("b.md"), "```python\nx\n```\n").unwrap();
    let output = run(
        dir.path(),
        &["fmt", "--no-cache", "--output-format", "json", "a.md", "b.md"],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(!dir.path().join("calls").exists());
    assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), first);
    assert!(
        diagnostics(&output)
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["message"].as_str().unwrap().contains("no language tag"))
    );
}

#[test]
fn mode_policy_does_not_fall_through_to_missing_language_definition() {
    let dir = setup(
        "on-missing-mode-definition = \"fail\"\non-missing-language-definition = \"fail-fast\"",
        "python = { format = [\"test\"] }",
    );
    fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "--output-format",
            "json",
            "t.md",
        ],
    );
    let findings = diagnostics(&output);
    assert_eq!(findings.as_array().unwrap().len(), 1);
    assert!(findings[0]["message"].as_str().unwrap().contains("No lint tools"));
}

#[test]
fn custom_language_and_alias_are_recognized_and_disabled_languages_stay_silent() {
    let dir = setup(
        "[code-block-tools.language-aliases]\nmy-alias = \"my-language\"",
        "my-language = { lint = [\"test\"] }\npyhton = { enabled = false }",
    );
    fs::write(dir.path().join("t.md"), "```my-alias\nx\n```\n\n```pyhton\ny\n```\n").unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "--deny-config-warnings",
            "t.md",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn cached_tool_checks_satisfy_no_tools_policy_without_another_process() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let config_path = dir.path().join(".rumdl.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("on-no-tools-run = \"ignore\"", "on-no-tools-run = \"fail\"");
    fs::write(config_path, config).unwrap();
    write_tool(dir.path(), "count");
    fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
    for _ in 0..2 {
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--output-format", "json", "t.md"],
        );
        assert!(output.status.success(), "{output:?}");
        assert_eq!(diagnostics(&output), serde_json::json!([]));
    }
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
}

#[test]
fn no_tools_failure_is_machine_readable_and_fail_fast_prevents_markdown_writes() {
    for policy in ["fail", "fail-fast"] {
        let dir = setup("", "python = { lint = [\"test\"] }");
        let config_path = dir.path().join(".rumdl.toml");
        fs::write(
            &config_path,
            fs::read_to_string(&config_path).unwrap().replace(
                "on-no-tools-run = \"ignore\"",
                &format!("on-no-tools-run = \"{policy}\""),
            ),
        )
        .unwrap();
        let content = "#  Title\n";
        fs::write(dir.path().join("t.md"), content).unwrap();
        let output = run(dir.path(), &["fmt", "--output-format", "json", "--no-cache", "t.md"]);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(
            diagnostics(&output)
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["message"].as_str().unwrap().contains("No tools executed"))
        );
        if policy == "fail-fast" {
            assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), content);
        }
    }
}

#[test]
fn invalid_entries_follow_the_policy_and_valid_entries_still_execute() {
    for policy in ["ignore", "warn", "fail", "fail-fast"] {
        let dir = setup(
            &format!("on-invalid-tool-definition = \"{policy}\""),
            "python = { lint = [\"unknown-id\", \"test\"] }",
        );
        write_tool(dir.path(), "count");
        fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
        let output = run(
            dir.path(),
            &[
                "check",
                "--only-code-block-tools",
                "--output-format",
                "json",
                "--no-cache",
                "t.md",
            ],
        );
        assert_eq!(
            output.status.success(),
            matches!(policy, "ignore" | "warn"),
            "{policy}: {output:?}"
        );
        assert_eq!(dir.path().join("calls").exists(), policy != "fail-fast");
        assert_eq!(stderr(&output).contains("Unknown tool"), policy == "warn");
    }
}

#[test]
fn project_binary_identity_separates_identical_cached_documents() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    for project in ["a", "b"] {
        let root = dir.path().join(project);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("pyproject.toml"), "").unwrap();
        write_tool(&root, if project == "a" { "count" } else { "finding" });
        fs::write(root.join("t.md"), "```python\nx\n```\n").unwrap();
    }
    let first = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--output-format", "json", "a/t.md"],
    );
    assert!(first.status.success(), "{first:?}");
    let second = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--output-format", "json", "b/t.md"],
    );
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("project-b-finding"),
        "{second:?}"
    );
}

#[test]
fn unusable_custom_definitions_are_rejected_even_for_empty_documents() {
    for definition in ["command = []", "command = [\"\"]"] {
        let dir = setup(
            "on-invalid-tool-definition = \"fail\"",
            "python = { lint = [\"test\"] }",
        );
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            format!(
                "{}\n[code-block-tools.tools.unused]\n{definition}\n",
                fs::read_to_string(&config).unwrap()
            ),
        )
        .unwrap();
        fs::write(dir.path().join("t.md"), "").unwrap();
        let output = run(
            dir.path(),
            &[
                "check",
                "--only-code-block-tools",
                "--output-format",
                "json",
                "--no-cache",
                "t.md",
            ],
        );
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Invalid tool definition"),
            "{output:?}"
        );
    }
}

#[test]
fn invalid_formatter_stdout_cannot_replace_code() {
    let dir = setup(
        "on-invalid-tool-definition = \"fail\"",
        "python = { format = [\"test\"] }",
    );
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("stdout = true", "stdout = false"),
    )
    .unwrap();
    write_tool(dir.path(), "lost");
    let content = "```python\nx\n```\n";
    fs::write(dir.path().join("t.md"), content).unwrap();
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--only-code-block-tools",
            "--output-format",
            "json",
            "--no-cache",
            "t.md",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), content);
    assert!(String::from_utf8_lossy(&output.stdout).contains("stdout = false"));
}

#[test]
fn no_tools_policy_also_covers_empty_discovery() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("on-no-tools-run = \"ignore\"", "on-no-tools-run = \"fail\""),
    )
    .unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--output-format", "json", "empty"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(diagnostics(&output).as_array().unwrap().len(), 1);
}

#[test]
fn invalid_commands_warn_without_also_being_reported_as_missing_binaries() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("command = [\"rumdl-policy-test\"]", "command = [\"\"]"),
    )
    .unwrap();
    fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
    let output = run(dir.path(), &["check", "--only-code-block-tools", "--no-cache", "t.md"]);
    assert!(output.status.success(), "{output:?}");
    let warnings = stderr(&output);
    assert!(warnings.contains("Invalid tool definition"), "{warnings}");
    assert!(!warnings.contains("code-block tools not installed"), "{warnings}");
}

#[test]
fn removed_and_reinstalled_project_binary_invalidates_cached_results() {
    let dir = setup("on-missing-tool-binary = \"fail\"", "python = { lint = [\"test\"] }");
    write_tool(dir.path(), "count");
    fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
    let args = ["check", "--only-code-block-tools", "--output-format", "json", "t.md"];
    let first = run(dir.path(), &args);
    assert!(first.status.success(), "{first:?}");
    let tool = dir
        .path()
        .join(if cfg!(windows) { ".venv/Scripts" } else { ".venv/bin" })
        .join(format!("rumdl-policy-test{}", std::env::consts::EXE_SUFFIX));
    fs::remove_file(&tool).unwrap();
    let missing = run(dir.path(), &args);
    assert!(!missing.status.success(), "{missing:?}");
    assert!(
        String::from_utf8_lossy(&missing.stdout).contains("not found"),
        "{missing:?}"
    );
    write_tool(dir.path(), "finding");
    // The fixture executable is copied unchanged, while its behavior comes
    // from a sidecar. Model a newly installed binary revision explicitly:
    // sidecar changes alone are intentionally outside the cache fingerprint.
    let revised = fs::metadata(&tool).unwrap().modified().unwrap() + std::time::Duration::from_secs(3);
    fs::OpenOptions::new()
        .write(true)
        .open(&tool)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(revised))
        .unwrap();
    let reinstalled = run(dir.path(), &args);
    assert!(
        String::from_utf8_lossy(&reinstalled.stdout).contains("project-b-finding"),
        "{reinstalled:?}"
    );
}

#[test]
fn native_formatters_cover_fallback_pipeline_and_failure_policies() {
    for (format_mode, error_policy, expected, success) in [
        ("fallback", "warn", "good", true),
        ("pipeline", "warn", "GOOD", true),
        ("pipeline", "fail", "bad", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        native_tool::install(dir.path(), "quiet");
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            format!(
                r#"
[code-block-tools]
enabled = true
on-error = "{error_policy}"
[code-block-tools.tools.first]
command = ["rumdl-policy-test", "count-format"]
[code-block-tools.tools.broken]
command = ["rumdl-policy-test", "fail"]
[code-block-tools.tools.second]
command = ["rumdl-policy-test", "uppercase"]
[code-block-tools.languages]
python = {{ format = ["first", "broken", "second"], format-mode = "{format_mode}" }}
"#
            ),
        )
        .unwrap();
        let original = "```python\nbad\n```\n";
        fs::write(dir.path().join("t.md"), original).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--only-code-block-tools",
                "--no-cache",
                "--output-format",
                "json",
                "t.md",
            ],
        );
        assert_eq!(
            output.status.success(),
            success,
            "{format_mode}/{error_policy}: {output:?}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("t.md")).unwrap(),
            format!("```python\n{expected}\n```\n")
        );
        assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap(), "ran\n");
        if !success {
            assert_eq!(output.status.code(), Some(2));
        }
    }
}
