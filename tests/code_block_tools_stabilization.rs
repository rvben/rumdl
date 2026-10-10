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
    let dir = setup("on-unknown-language-tag = \"warn\"", "python = { lint = [\"test\"] }");
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
fn external_tool_checks_are_repeated_and_satisfy_no_tools_policy() {
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
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 2);
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
        assert_eq!(diagnostics(&output), serde_json::json!([]));
        assert!(stderr(&output).contains("No tools executed"));
        if policy == "fail-fast" {
            assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), content);
        }
    }
}

#[test]
fn invalid_entries_warn_once_and_valid_entries_still_execute() {
    let dir = setup("", "python = { lint = [\"unknown-id\", \"test\"] }");
    write_tool(dir.path(), "count");
    for file in ["a.md", "b.md"] {
        fs::write(dir.path().join(file), "```python\nx\n```\n").unwrap();
    }
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--output-format",
            "json",
            "a.md",
            "b.md",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(diagnostics(&output), serde_json::json!([]));
    assert_eq!(stderr(&output).matches("Unknown tool in").count(), 1);
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 2);
    let denied = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--output-format",
            "json",
            "a.md",
            "b.md",
        ],
    );
    assert_eq!(denied.status.code(), Some(2));
    assert_eq!(diagnostics(&denied), serde_json::json!([]));
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
    for definition in [
        "command = []",
        "command = [\"\"]",
        r#"command = ["tool", "\u0000"]"#,
        r#"command = ["tool"]
lint-args = ["\u0000"]"#,
    ] {
        let dir = setup("", "python = { lint = [\"test\"] }");
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
                "--deny-config-warnings",
                "t.md",
            ],
        );
        assert!(!output.status.success(), "{output:?}");
        assert!(stderr(&output).contains("Invalid tool definition"), "{output:?}");
    }
}

#[test]
fn invalid_formatter_stdout_cannot_replace_code() {
    let dir = setup("", "python = { format = [\"test\"] }");
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
            "--deny-config-warnings",
            "t.md",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), content);
    assert!(stderr(&output).contains("stdout = false"));
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
    assert_eq!(diagnostics(&output), serde_json::json!([]));
    assert!(stderr(&output).contains("No tools executed"));
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
binary-preferences = {{ rumdl-policy-test = "project" }}
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

#[test]
fn external_config_changes_are_seen_without_document_or_binary_changes() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let binary = native_tool::install(dir.path(), "count");
    let before = fs::metadata(&binary).unwrap();
    fs::write(dir.path().join("t.md"), "```python\nx\n```\n").unwrap();
    let args = ["check", "--only-code-block-tools", "--output-format", "json", "t.md"];
    assert!(run(dir.path(), &args).status.success());
    fs::write(binary.with_extension("mode"), "count-finding").unwrap();
    let after = fs::metadata(&binary).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    let changed = run(dir.path(), &args);
    assert_eq!(changed.status.code(), Some(1), "{changed:?}");
    assert!(
        diagnostics(&changed)[0]["message"]
            .as_str()
            .unwrap()
            .contains("project-b-finding")
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap(), "ran\nran\n");
}

#[test]
fn valid_internal_cache_hits_satisfy_no_tools_policy() {
    let dir = setup("", "markdown = { lint = [\"rumdl\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("on-no-tools-run = \"ignore\"", "on-no-tools-run = \"fail\""),
    )
    .unwrap();
    fs::write(dir.path().join("t.md"), "````markdown\n# Clean\n````\n").unwrap();
    let args = [
        "check",
        "--only-code-block-tools",
        "--enable",
        "MD009",
        "--verbose",
        "t.md",
    ];
    let first = run(dir.path(), &args);
    assert!(first.status.success(), "{first:?}");
    let cached = run(dir.path(), &args);
    assert!(cached.status.success(), "{cached:?}");
    assert!(
        String::from_utf8_lossy(&cached.stdout).contains("Cache hit"),
        "{cached:?}"
    );
    assert!(!stderr(&cached).contains("No tools executed"));
    assert!(!dir.path().join("calls").exists());
}

#[test]
fn omitted_unknown_tag_policy_preserves_existing_strict_coverage() {
    for override_policy in ["", "on-unknown-language-tag = \"ignore\""] {
        let dir = setup(
            &format!("on-missing-language-definition = \"fail\"\n{override_policy}"),
            "python = { lint = [\"test\"] }",
        );
        fs::write(dir.path().join("t.md"), "```python\nx\n```\n\n```pyhton\nx\n```\n").unwrap();
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--output-format", "json", "t.md"],
        );
        assert_eq!(
            output.status.code(),
            Some(if override_policy.is_empty() { 1 } else { 0 }),
            "{output:?}"
        );
        assert_eq!(
            diagnostics(&output).as_array().unwrap().len(),
            usize::from(override_policy.is_empty())
        );
    }
}

#[test]
fn default_coverage_is_quiet_for_prose_unlabeled_and_unconfigured_blocks() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("on-no-tools-run = \"ignore\"\n", ""),
    )
    .unwrap();
    for content in [
        "# Prose\n",
        "```\nx\n```\n",
        "```pyhton\nx\n```\n",
        "```json\n{}\n```\n",
    ] {
        fs::write(dir.path().join("t.md"), content).unwrap();
        let output = run(
            dir.path(),
            &[
                "check",
                "--only-code-block-tools",
                "--deny-config-warnings",
                "--output-format",
                "json",
                "t.md",
            ],
        );
        assert!(output.status.success(), "{content}: {output:?}");
        assert_eq!(diagnostics(&output), serde_json::json!([]));
        assert!(stderr(&output).is_empty(), "{output:?}");
    }
}

#[test]
fn invocation_failures_preserve_findings_shapes_in_json_and_sarif() {
    for policy in ["fail", "fail-fast"] {
        for empty in [false, true] {
            let dir = setup("", "python = { lint = [\"test\"] }");
            let config = dir.path().join(".rumdl.toml");
            fs::write(
                &config,
                fs::read_to_string(&config).unwrap().replace(
                    "on-no-tools-run = \"ignore\"",
                    &format!("on-no-tools-run = \"{policy}\""),
                ),
            )
            .unwrap();
            fs::write(dir.path().join("t.md"), "# Clean\n").unwrap();
            fs::create_dir(dir.path().join("empty")).unwrap();
            for format in ["json", "sarif"] {
                let output = run(
                    dir.path(),
                    &[
                        "check",
                        "--only-code-block-tools",
                        "--fail-on",
                        "never",
                        "--output-format",
                        format,
                        if empty { "empty" } else { "t.md" },
                    ],
                );
                assert_eq!(output.status.code(), Some(2), "{output:?}");
                assert!(stderr(&output).contains("No tools executed"));
                let report = diagnostics(&output);
                if format == "json" {
                    assert_eq!(report, serde_json::json!([]));
                } else {
                    assert_eq!(report["runs"][0]["results"], serde_json::json!([]));
                    let invocation = &report["runs"][0]["invocations"][0];
                    assert_eq!(invocation["executionSuccessful"], false);
                    assert!(
                        invocation["toolExecutionNotifications"][0]["message"]["text"]
                            .as_str()
                            .unwrap()
                            .contains("No tools executed")
                    );
                    assert!(invocation["toolExecutionNotifications"][0].get("locations").is_none());
                }
            }
        }
    }
}

#[test]
fn central_invalid_configuration_is_denied_on_an_empty_batch() {
    let dir = setup("", "python = { lint = [\"unknown-id\"] }");
    fs::create_dir(dir.path().join("empty")).unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--output-format",
            "json",
            "empty",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(diagnostics(&output), serde_json::json!([]));
    assert!(stderr(&output).contains("Unknown tool"));
}

#[test]
fn full_language_table_and_md040_custom_labels_are_recognized() {
    let dir = setup("on-unknown-language-tag = \"fail\"", "python = { lint = [\"test\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        format!(
            "{}\n[MD040]\ncustom-languages = [\"acme-output\"]\n",
            fs::read_to_string(&config).unwrap()
        ),
    )
    .unwrap();
    let content = ["console", "mermaid", "tsx", "rst", "jinja", "pytb", "pl", "acme-output"]
        .iter()
        .map(|tag| format!("```{tag}\nx\n```\n\n"))
        .collect::<String>();
    fs::write(dir.path().join("t.md"), content).unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--output-format",
            "json",
            "t.md",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(diagnostics(&output), serde_json::json!([]));
}

#[test]
fn md040_and_tool_coverage_share_flavor_exemptions() {
    for (flavor, info) in [("myst", "{note}"), ("pandoc", "{=html}"), ("mkdocs", "title=example")] {
        let dir = setup(
            "on-missing-language-tag = \"fail\"\non-unknown-language-tag = \"fail\"",
            "python = { lint = [\"test\"] }",
        );
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            format!(
                "{}\n[MD040]\nunknown-language-action = \"error\"\n",
                fs::read_to_string(&config).unwrap()
            ),
        )
        .unwrap();
        fs::write(dir.path().join("t.md"), format!("```{info}\nx\n```\n")).unwrap();
        for only in [false, true] {
            let mut args = vec![
                "check",
                "--enable",
                "MD040",
                "--flavor",
                flavor,
                "--output-format",
                "json",
                "--deny-config-warnings",
                "t.md",
            ];
            if only {
                args.push("--only-code-block-tools");
            }
            let output = run(dir.path(), &args);
            assert!(output.status.success(), "{flavor}/{info}/{only}: {output:?}");
            assert_eq!(diagnostics(&output), serde_json::json!([]));
        }
    }
}

#[test]
fn no_tools_policy_applies_to_check_fix_fmt_diff_and_preflight() {
    for options in [
        vec!["check"],
        vec!["check", "--fix"],
        vec!["fmt"],
        vec!["fmt", "--diff"],
        vec!["fmt", "--preflight"],
        vec!["check", "--fix", "--preflight"],
    ] {
        let dir = setup("", "python = { lint = [\"test\"], format = [\"test\"] }");
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            fs::read_to_string(&config)
                .unwrap()
                .replace("on-no-tools-run = \"ignore\"", "on-no-tools-run = \"fail\""),
        )
        .unwrap();
        fs::write(dir.path().join("t.md"), "#  Title\n").unwrap();
        let mut args = options.clone();
        args.extend(["--output-format", "json", "--fail-on", "never", "t.md"]);
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{options:?}: {output:?}");
        assert!(stderr(&output).contains("No tools executed"));
        assert!(
            !diagnostics(&output)
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["message"].as_str().unwrap().contains("No tools executed"))
        );
        if options.contains(&"--preflight") || options.contains(&"--diff") {
            assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), "#  Title\n");
        }
    }
}

#[test]
fn block_policy_exit_codes_distinguish_findings_and_incomplete_formatting() {
    for policy in ["fail", "fail-fast"] {
        for options in [
            vec!["check"],
            vec!["check", "--fix"],
            vec!["fmt"],
            vec!["fmt", "--diff"],
            vec!["fmt", "--preflight"],
        ] {
            for fail_on in ["any", "never"] {
                let dir = setup(
                    &format!("on-missing-language-tag = \"{policy}\""),
                    "python = { lint = [\"test\"], format = [\"test\"] }",
                );
                fs::write(dir.path().join("t.md"), "```\nx\n```\n").unwrap();
                let mut args = options.clone();
                args.extend([
                    "--only-code-block-tools",
                    "--output-format",
                    "json",
                    "--fail-on",
                    fail_on,
                    "t.md",
                ]);
                let output = run(dir.path(), &args);
                let expected = if options == ["check"] && policy == "fail" {
                    if fail_on == "never" { 0 } else { 1 }
                } else {
                    2
                };
                assert_eq!(
                    output.status.code(),
                    Some(expected),
                    "{policy}/{options:?}/{fail_on}: {output:?}"
                );
                assert!(
                    diagnostics(&output)
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|d| d["message"].as_str().unwrap().contains("no language tag"))
                );
                assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), "```\nx\n```\n");
            }
        }
    }
}

#[test]
fn long_and_nested_fences_execute_external_tools_with_quiet_defaults() {
    for content in [
        "````python\nx\n````\n",
        "> ```python\n> x\n> ```\n",
        "- ```python\n  x\n  ```\n",
    ] {
        let dir = setup("", "python = { lint = [\"test\"] }");
        write_tool(dir.path(), "count");
        fs::write(dir.path().join("t.md"), content).unwrap();
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--output-format", "json", "t.md"],
        );
        assert!(output.status.success(), "{content}: {output:?}");
        assert_eq!(
            fs::read_to_string(dir.path().join("calls")).unwrap(),
            "ran\n",
            "{content}"
        );
    }
}

#[test]
fn strict_coverage_keeps_flavor_code_chunks_covered_and_md040_preserves_syntax() {
    for (flavor, info) in [
        ("quarto", "{python}"),
        ("quarto", "{{python}}"),
        ("pandoc", "{.python}"),
        ("myst", "{code-cell} python"),
        ("myst", "{code-block} python"),
    ] {
        let dir = setup(
            "on-missing-language-definition = \"fail\"",
            "python = { lint = [\"test\"] }",
        );
        let original = format!("```{info}\nx\n```\n");
        fs::write(dir.path().join("t.md"), &original).unwrap();
        let output = run(
            dir.path(),
            &[
                "check",
                "--only-code-block-tools",
                "--flavor",
                flavor,
                "--output-format",
                "json",
                "t.md",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{flavor}/{info}: {output:?}");
        assert_eq!(diagnostics(&output).as_array().unwrap().len(), 1);
        let md040 = run(
            dir.path(),
            &[
                "fmt",
                "--no-code-block-tools",
                "--enable",
                "MD040",
                "--flavor",
                flavor,
                "t.md",
            ],
        );
        assert!(md040.status.success(), "{md040:?}");
        assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), original);
    }
}

#[test]
fn invalid_nested_config_is_central_deduplicated_and_preflight_denied() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    for child in ["a", "b"] {
        let nested = dir.path().join(child);
        fs::create_dir(&nested).unwrap();
        fs::write(
            nested.join(".rumdl.toml"),
            "[code-block-tools]\nenabled=false\n[code-block-tools.languages]\npython={format=[\"unknown-id\"]}\n",
        )
        .unwrap();
        fs::write(nested.join("t.md"), "#  Title\n").unwrap();
    }
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--preflight",
            "--deny-config-warnings",
            "--enable",
            "MD019",
            "--output-format",
            "json",
            "a/t.md",
            "b/t.md",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(stderr(&output).matches("Unknown tool in").count(), 1);
    assert!(
        !diagnostics(&output)
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("Unknown tool"))
    );
    for child in ["a", "b"] {
        assert_eq!(
            fs::read_to_string(dir.path().join(child).join("t.md")).unwrap(),
            "#  Title\n"
        );
    }
}

#[test]
fn invocation_errors_use_the_diagnostic_stream_without_file_annotations() {
    let dir = setup("", "python = { lint = [\"test\"] }");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("on-no-tools-run = \"ignore\"", "on-no-tools-run = \"fail\""),
    )
    .unwrap();
    fs::write(dir.path().join("t.md"), "# Clean\n").unwrap();
    for format in ["text", "github", "json-lines"] {
        for swap in [false, true] {
            let mut args = vec!["check", "--only-code-block-tools", "--output-format", format, "t.md"];
            if swap {
                args.push("--stderr");
            }
            let output = run(dir.path(), &args);
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            let stream = if swap { &output.stdout } else { &output.stderr };
            assert!(
                String::from_utf8_lossy(stream).contains("No tools executed"),
                "{output:?}"
            );
            let findings = if swap { &output.stderr } else { &output.stdout };
            assert!(!String::from_utf8_lossy(findings).contains("t.md:1"));
            assert!(!String::from_utf8_lossy(findings).contains("<invocation>"));
        }
    }
}

#[test]
fn repeated_root_and_nested_configuration_warnings_are_emitted_once() {
    let dir = setup("", "python = { lint = [\"unknown-id\"] }");
    let root_config = fs::read_to_string(dir.path().join(".rumdl.toml")).unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested/.rumdl.toml"), root_config).unwrap();
    fs::write(dir.path().join("t.md"), "# Clean\n").unwrap();
    fs::write(dir.path().join("nested/t.md"), "# Clean\n").unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--output-format",
            "json",
            "t.md",
            "nested/t.md",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(stderr(&output).matches("Unknown tool in").count(), 1, "{output:?}");
    assert_eq!(diagnostics(&output), serde_json::json!([]));
}
