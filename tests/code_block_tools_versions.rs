//! Version policies exercised through the CLI and real native child processes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "support/native_tool.rs"]
mod native_tool;

fn install(root: &Path, name: &str, version: &str, mode: &str) -> PathBuf {
    let fixture = native_tool::install(root, "quiet");
    let tool = fixture.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    native_tool::copy_executable(&fixture, &tool);
    fs::write(tool.with_extension("mode"), mode).unwrap();
    fs::write(tool.with_extension("version"), version).unwrap();
    tool
}

fn project(policy: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            r#"[code-block-tools]
enabled = true
tool-version-definition = "uv.lock"
on-tool-version-mismatch = "{policy}"
binary-preferences = {{ ruff = "only-project" }}

[code-block-tools.tools.test]
command = ["ruff"]
stdin = true
stdout = true

[code-block-tools.languages.python]
lint = ["test"]
format = ["test"]
"#
        ),
    )
    .unwrap();
    fs::write(
        dir.path().join("uv.lock"),
        "version = 1\nrevision = 3\n\n[[package]]\nname = \"ruff\"\nversion = \"0.11.2\"\nsource = { registry = \"https://pypi.org/simple\" }\n",
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n").unwrap();
    dir
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn mismatched_tool_is_reported_and_skipped_before_checking_code() {
    let dir = project("fail");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-finding");
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch"), "{text}");
    assert!(text.contains("0.11.2") && text.contains("0.11.1"), "{text}");
    assert!(!text.contains("project-b-finding"), "{text}");
    assert!(!dir.path().join("calls").exists(), "mismatched tool ran");
}

#[test]
fn mismatched_formatter_leaves_the_block_unchanged() {
    let dir = project("fail");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-format");
    let original = fs::read(dir.path().join("doc.md")).unwrap();
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch"), "{text}");
    assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
    assert!(!dir.path().join("calls").exists(), "mismatched formatter ran");
}

#[test]
fn warn_checks_once_per_executable_and_runs_every_block_across_files() {
    let dir = project("warn");
    let tool = install(dir.path(), "ruff", "ruff 0.11.1\n", "count");
    for name in ["a.md", "b.md", "c.md", "d.md"] {
        fs::write(
            dir.path().join(name),
            "```python\nx\n```\n\n```python\ny\n```\n\n```python\nz\n```\n",
        )
        .unwrap();
    }
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "a.md",
            "b.md",
            "c.md",
            "d.md",
        ],
    );
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(text.matches("version mismatch").count(), 1, "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(),
        12
    );
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        1,
        "the selected executable should be probed once per invocation"
    );
}

#[test]
fn known_python_binaries_match_their_locked_versions_and_format_normally() {
    for (name, reported) in [
        ("ruff", "ruff 0.11.2\n"),
        ("black", "black, 0.11.2 (compiled: yes)\nPython (CPython) 3.13.1\n"),
        ("black", "BLACK.EXE, version 0.11.2 (compiled: no)\n"),
        ("djlint", "djlint, version 0.11.2\n"),
        ("sqlfluff", "sqlfluff, version 0.11.2\n"),
    ] {
        let dir = project("fail");
        let config = dir.path().join(".rumdl.toml");
        fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
        let lock = dir.path().join("uv.lock");
        fs::write(&lock, fs::read_to_string(&lock).unwrap().replace("ruff", name)).unwrap();
        install(dir.path(), name, reported, "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert!(output.status.success(), "{name}: {text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            "```python\ngood\n```\n",
            "{name}: {text}"
        );
        assert!(
            !text.contains("Cannot ") && !text.contains("version mismatch"),
            "{name}: {text}"
        );
    }
}

#[test]
fn fail_fast_checks_source_order_before_later_formatters_or_files_run() {
    let dir = project("fail-fast");
    let config = dir.path().join(".rumdl.toml");
    let text = fs::read_to_string(&config)
        .unwrap()
        .replace("lint = [\"test\"]\n", "")
        .replace(
            "{ ruff = \"only-project\" }",
            "{ ruff = \"only-project\", black = \"only-project\" }",
        );
    fs::write(&config, format!("{text}\n[code-block-tools.tools.other]\ncommand = [\"black\"]\nstdin = true\nstdout = true\n[code-block-tools.languages.json]\nformat = [\"other\"]\n")).unwrap();
    let lock = dir.path().join("uv.lock");
    fs::write(
        &lock,
        format!(
            "{}\n[[package]]\nname = \"black\"\nversion = \"0.11.2\"\n",
            fs::read_to_string(&lock).unwrap()
        ),
    )
    .unwrap();
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-format");
    install(dir.path(), "black", "black, 0.11.2 (compiled: no)\n", "count-format");
    let original = "#  Title\n\n```python\nbad\n```\n\n```json\nbad\n```\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    fs::write(dir.path().join("b.md"), "```json\nbad\n```\n").unwrap();
    let output = run(dir.path(), &["fmt", "--no-cache", "a.md", "b.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch"), "{text}");
    assert!(
        !dir.path().join("calls").exists(),
        "a later formatter ran before fail-fast: {text}"
    );
    assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(dir.path().join("b.md")).unwrap(),
        "```json\nbad\n```\n"
    );
}

#[test]
fn oversized_version_output_is_unresolved_without_running_the_tool() {
    let dir = project("fail");
    install(
        dir.path(),
        "ruff",
        &format!("ruff 0.11.2\n{}", " ".repeat(8192)),
        "count",
    );
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    let text = output_text(&output);
    assert!(
        !output.status.success(),
        "oversized version output was accepted: {text}"
    );
    assert!(text.contains("Cannot determine version"), "{text}");
    assert!(!dir.path().join("calls").exists(), "unverifiable tool ran");
}

#[test]
fn ignore_and_absent_source_run_without_requiring_metadata_or_probing() {
    for (policy, source) in [("ignore", true), ("fail-fast", false)] {
        let dir = project(policy);
        let tool = install(dir.path(), "ruff", "unrecognizable version\n", "count");
        fs::remove_file(dir.path().join("uv.lock")).unwrap();
        if !source {
            let config = dir.path().join(".rumdl.toml");
            fs::write(
                &config,
                fs::read_to_string(&config)
                    .unwrap()
                    .replace("tool-version-definition = \"uv.lock\"\n", ""),
            )
            .unwrap();
        }
        let output = run(
            dir.path(),
            &[
                "check",
                "--only-code-block-tools",
                "--no-cache",
                "--deny-config-warnings",
                "doc.md",
            ],
        );
        assert!(output.status.success(), "{}", output_text(&output));
        assert!(dir.path().join("calls").exists());
        assert!(!tool.with_extension("version-calls").exists());
    }
}

#[test]
fn fail_continues_other_tools_even_when_execution_errors_are_skipped() {
    let dir = project("fail");
    let config = dir.path().join(".rumdl.toml");
    let text = fs::read_to_string(&config)
        .unwrap()
        .replace("enabled = true", "enabled = true\non-error = \"skip\"")
        .replace(
            "{ ruff = \"only-project\" }",
            "{ ruff = \"only-project\", black = \"only-project\" }",
        )
        .replace("lint = [\"test\"]", "lint = [\"test\", \"other\"]");
    fs::write(
        &config,
        format!("{text}\n[code-block-tools.tools.other]\ncommand = [\"black\"]\nstdin = true\nstdout = true\n"),
    )
    .unwrap();
    let lock = dir.path().join("uv.lock");
    fs::write(
        &lock,
        format!(
            "{}\n[[package]]\nname = \"black\"\nversion = \"0.11.2\"\n",
            fs::read_to_string(&lock).unwrap()
        ),
    )
    .unwrap();
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-finding");
    install(dir.path(), "black", "black, 0.11.2 (compiled: no)\n", "count-finding");
    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "--output-format",
            "json",
            "doc.md",
        ],
    );
    assert!(!output.status.success(), "{}", output_text(&output));
    let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings.as_array().unwrap().len(), 2, "{findings}");
    assert!(
        findings[0]["message"].as_str().unwrap().contains("version mismatch"),
        "{findings}"
    );
    assert!(
        findings[1]["message"].as_str().unwrap().contains("project-b-finding"),
        "{findings}"
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
}

#[test]
fn cached_checks_reverify_changed_versions_and_recover_after_a_lock_update() {
    let dir = project("fail");
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    let args = ["check", "--only-code-block-tools", "doc.md"];
    let matching = run(dir.path(), &args);
    assert!(matching.status.success(), "{}", output_text(&matching));
    fs::write(tool.with_extension("version"), "ruff 0.11.1\n").unwrap();
    let changed = run(dir.path(), &args);
    assert!(!changed.status.success(), "{}", output_text(&changed));
    assert!(output_text(&changed).contains("version mismatch"));
    let lock = dir.path().join("uv.lock");
    fs::write(&lock, fs::read_to_string(&lock).unwrap().replace("0.11.2", "0.11.1")).unwrap();
    let fixed = run(dir.path(), &args);
    assert!(fixed.status.success(), "{}", output_text(&fixed));
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 2);
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn exact_python_versions_preserve_normalization_and_all_suffixes() {
    for (expected, actual, matches) in [
        ("1.0", "1.0.0", true),
        ("1.2rc1", "1.2-rc1", true),
        ("1!1.0.post2.dev3+local.1", "1!1.0.post2.dev3+local.1", true),
        ("1.0", "1.0+local", false),
        ("1.2rc1", "1.2", false),
        ("1.0.post1", "1.0.post2", false),
        ("1.0.dev1", "1.0.dev2", false),
        ("1!1.0", "1.0", false),
    ] {
        let dir = project("fail");
        let lock = dir.path().join("uv.lock");
        fs::write(&lock, fs::read_to_string(&lock).unwrap().replace("0.11.2", expected)).unwrap();
        install(dir.path(), "ruff", &format!("ruff {actual}\n"), "count");
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
        );
        let text = output_text(&output);
        assert_eq!(
            output.status.success(),
            matches,
            "expected {expected}, actual {actual}: {text}"
        );
        assert_eq!(dir.path().join("calls").exists(), matches, "{text}");
        if !matches {
            assert!(text.contains("version mismatch"), "{text}");
        }
    }
}

#[test]
fn unreliable_probes_follow_the_version_policy_without_leaking_output() {
    for (version, sidecar, value) in [
        ("internal-secret 0.11.2\n", "unused", ""),
        ("ruff 0.11.2\nruff 0.11.1\n", "unused", ""),
        ("ruff 0.11.2\n", "version-exit", "1"),
        ("ruff 0.11.2\n", "version-stderr", "internal-secret\n"),
        ("ruff 0.11.2\n", "version-invalid-utf8", "yes"),
        ("ruff 0.11.2\n", "version-delay-ms", "2000"),
    ] {
        let dir = project("fail");
        let tool = install(dir.path(), "ruff", version, "count");
        fs::write(tool.with_extension(sidecar), value).unwrap();
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            fs::read_to_string(&config)
                .unwrap()
                .replace("enabled = true", "enabled = true\ntimeout = 100"),
        )
        .unwrap();
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
        );
        let text = output_text(&output);
        assert!(!output.status.success(), "{sidecar}: {text}");
        assert!(text.contains("Cannot determine version"), "{sidecar}: {text}");
        assert!(!text.contains("internal-secret"), "{text}");
        assert!(!dir.path().join("calls").exists(), "unverifiable tool ran");
    }
}

#[test]
fn nearest_lock_is_authoritative_and_repository_boundaries_are_respected() {
    let dir = project("fail");
    fs::write(dir.path().join(".git"), "gitdir: fixture\n").unwrap();
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    let nested = dir.path().join("package/docs");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("nested.md"), "```python\nx\n```\n").unwrap();
    let args = [
        "check",
        "--only-code-block-tools",
        "--no-cache",
        "package/docs/nested.md",
    ];
    let outer = run(dir.path(), &args);
    assert!(outer.status.success(), "{}", output_text(&outer));
    fs::write(
        dir.path().join("package/uv.lock"),
        "version = 1\n[[package]]\nname = \"black\"\nversion = \"0.11.2\"\n",
    )
    .unwrap();
    let nearest = run(dir.path(), &args);
    assert!(!nearest.status.success(), "{}", output_text(&nearest));
    assert!(output_text(&nearest).contains("unambiguous version"));
    fs::remove_file(dir.path().join("package/uv.lock")).unwrap();
    fs::write(dir.path().join("package/.git"), "gitdir: other-fixture\n").unwrap();
    // Keep tool configuration and executable selection explicit so this assertion
    // isolates lock discovery from the nested repository's config/binary boundary.
    let config = dir.path().join(".rumdl.toml");
    let command = serde_json::to_string(tool.to_str().unwrap()).unwrap();
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("command = [\"ruff\"]", &format!("command = [{command}]")),
    )
    .unwrap();
    let bounded = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-cache",
            "--config",
            ".rumdl.toml",
            "package/docs/nested.md",
        ],
    );
    assert!(!bounded.status.success(), "{}", output_text(&bounded));
    assert!(output_text(&bounded).contains("uv.lock not found"));
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
}

#[test]
fn separate_package_locks_share_one_probe_and_keep_distinct_warnings() {
    let dir = project("warn");
    fs::write(dir.path().join(".git"), "gitdir: fixture\n").unwrap();
    let tool = install(dir.path(), "ruff", "ruff 0.11.0\n", "count");
    for (package, version) in [("a", "0.11.1"), ("b", "0.11.2")] {
        let root = dir.path().join(package);
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(
            root.join("uv.lock"),
            format!("version = 1\n[[package]]\nname = \"ruff\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
        for name in ["first.md", "second.md"] {
            fs::write(root.join("docs").join(name), "```python\nx\n```\n").unwrap();
        }
    }
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "a", "b"],
    );
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(text.matches("version mismatch").count(), 2, "{text}");
    assert!(
        text.contains("expected 0.11.1") && text.contains("expected 0.11.2"),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 4);
}

#[test]
fn missing_malformed_and_ambiguous_metadata_fail_closed_without_quoting_it() {
    for lock in [
        "version = 2\n[[package]]\nname = \"ruff\"\nversion = \"0.11.2\"\n",
        "version = 1\n[[package]]\nname = \"ruff\"\n",
        "version = 1\n[[package]]\nname = \"ruff\"\nversion = \"internal-secret\"\n",
        "version = 1\n[[package]]\nname = \"ruff\"\nversion = \"0.11.2\"\n[[package]]\nname = \"ruff\"\nversion = \"0.11.1\"\n",
        "version = 1\n[[package]]\nname = \"black\"\nversion = \"0.11.2\"\n",
        "version = 1\n[[package]]\nname = 42\nversion = \"internal-secret\"\n",
        "version = 1\ninternal-secret = 'unfinished",
        "",
    ] {
        let dir = project("fail");
        let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
        fs::write(dir.path().join("uv.lock"), lock).unwrap();
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
        );
        let text = output_text(&output);
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("uv.lock"), "{text}");
        assert!(!text.contains("internal-secret"), "{text}");
        assert!(!dir.path().join("calls").exists());
        assert!(!tool.with_extension("version-calls").exists());
    }
}

#[test]
fn equal_forked_versions_and_additive_lock_fields_are_supported() {
    let dir = project("fail");
    install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    fs::write(
        dir.path().join("uv.lock"),
        concat!(
            "version = 1\nrevision = 999\nrequires-python = '>=3.10'\n",
            "[[package]]\nname = 'RUFF'\nversion = '0.11.2'\nresolution-markers = [\"python_version >= '3.12'\"]\n",
            "[[package]]\nname = 'ruff'\nversion = '0.11.2.0'\nresolution-markers = [\"python_version < '3.12'\"]\n",
            "[[package]]\nname = 'unrelated'\nversion = ['unrelated-malformed-version']\n",
        ),
    )
    .unwrap();
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    assert!(output.status.success(), "{}", output_text(&output));
    assert!(dir.path().join("calls").exists());
}

#[test]
fn embedded_rumdl_is_exempt_and_warn_obeys_deny_config_warnings() {
    let dir = project("fail-fast");
    fs::remove_file(dir.path().join("uv.lock")).unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "[code-block-tools]\nenabled = true\ntool-version-definition = 'uv.lock'\non-tool-version-mismatch = 'fail-fast'\n[code-block-tools.languages.markdown]\nformat = ['rumdl']\n").unwrap();
    fs::write(dir.path().join("doc.md"), "# Title\n\n```markdown\n#  Nested\n```\n").unwrap();
    let embedded = run(
        dir.path(),
        &[
            "fmt",
            "--only-code-block-tools",
            "--enable",
            "MD019",
            "--deny-config-warnings",
            "--no-cache",
            "doc.md",
        ],
    );
    assert!(embedded.status.success(), "{}", output_text(&embedded));
    assert!(
        fs::read_to_string(dir.path().join("doc.md"))
            .unwrap()
            .contains("# Nested")
    );

    let dir = project("warn");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count");
    let warning = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--no-cache",
            "doc.md",
        ],
    );
    assert_eq!(warning.status.code(), Some(2), "{}", output_text(&warning));
    assert!(output_text(&warning).contains("version mismatch"));
    assert!(dir.path().join("calls").exists(), "warn must still run the tool");
}

#[test]
fn unresolved_tools_apply_the_policy_and_probes_do_not_count_as_tool_execution() {
    let dir = project("fail");
    let config = dir.path().join(".rumdl.toml");
    let text = fs::read_to_string(&config).unwrap().replace("ruff", "unmapped-tool");
    fs::write(&config, text).unwrap();
    install(dir.path(), "unmapped-tool", "ruff 0.11.2\n", "count");
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(output_text(&output).contains("Cannot resolve a Python package version"));
    assert!(!dir.path().join("calls").exists());

    let dir = project("fail");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("enabled = true", "enabled = true\non-no-tools-run = 'fail'"),
    )
    .unwrap();
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("version mismatch") && text.contains("No tools executed"),
        "{text}"
    );
}

fn two_formatter_config(dir: &Path, policy: &str, mode: &str, first: &str) {
    fs::write(
        dir.join(".rumdl.toml"),
        format!(
            r#"[code-block-tools]
enabled = true
tool-version-definition = "uv.lock"
on-tool-version-mismatch = "{policy}"
on-missing-tool-binary = "ignore"
on-error = "skip"
binary-preferences = {{ ruff = "only-project", black = "only-project" }}
[code-block-tools.tools.test]
command = ["ruff"]
stdin = true
stdout = true
[code-block-tools.tools.other]
command = ["black"]
stdin = true
stdout = true
[code-block-tools.tools.missing]
command = ["rumdl-version-fixture-missing"]
[code-block-tools.languages.python]
format = ["{first}", "other"]
format-mode = "{mode}"
"#
        ),
    )
    .unwrap();
    fs::write(dir.join("uv.lock"), "version = 1\n[[package]]\nname = 'ruff'\nversion = '0.11.2'\n[[package]]\nname = 'black'\nversion = '0.11.2'\n").unwrap();
}

#[test]
fn fallback_verifies_only_needed_stages_and_pipeline_preflights_all_stages() {
    for (mode, first, first_succeeds, should_fail) in [
        ("fallback", "test", true, false),
        ("fallback", "test", false, true),
        ("fallback", "missing", true, true),
        ("fallback", "ruff:check", true, true),
        ("pipeline", "test", true, true),
    ] {
        let dir = project("fail-fast");
        two_formatter_config(dir.path(), "fail-fast", mode, first);
        install(
            dir.path(),
            "ruff",
            "ruff 0.11.2\n",
            if first_succeeds { "count-format" } else { "fail-message" },
        );
        let black = install(dir.path(), "black", "black, 0.11.1 (compiled: no)\n", "count-format");
        let original = fs::read(dir.path().join("doc.md")).unwrap();
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert_eq!(
            !output.status.success(),
            should_fail,
            "{mode}/{first}/{first_succeeds}: {text}"
        );
        if should_fail {
            assert!(text.contains("version mismatch"), "{text}");
            assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
            assert!(!dir.path().join("calls").exists(), "unused formatter ran: {text}");
        } else {
            assert_eq!(
                fs::read_to_string(dir.path().join("doc.md")).unwrap(),
                "```python\ngood\n```\n"
            );
            assert!(
                !black.with_extension("version-calls").exists(),
                "unused fallback was probed"
            );
        }
    }
}

#[test]
fn fail_skips_a_pipeline_stage_but_writes_successful_other_stages_and_exits_nonzero() {
    let dir = project("fail");
    two_formatter_config(dir.path(), "fail", "pipeline", "test");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-format");
    install(dir.path(), "black", "black, 0.11.2 (compiled: no)\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch"), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\ngood\n```\n"
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
}

#[test]
fn absent_source_does_not_make_formatting_serial() {
    let dir = project("fail-fast");
    let tool = install(dir.path(), "ruff", "unrecognizable\n", "parallel-format");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("tool-version-definition = \"uv.lock\"\n", "")
            .replace("enabled = true", "enabled = true\non-error = 'warn'")
            .replace("lint = [\"test\"]\n", ""),
    )
    .unwrap();
    fs::write(
        dir.path().join("doc.md"),
        "```python\nfirst bad\n```\n\n```python\nsecond bad\n```\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .env("RAYON_NUM_THREADS", "2")
        .args(["fmt", "--only-code-block-tools", "--no-cache", "doc.md"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_text(&output));
    assert!(dir.path().join("overlap-observed").exists());
    assert!(!tool.with_extension("version-calls").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\nfirst good and longer\n```\n\n```python\nsecond good and longer\n```\n"
    );
}

#[test]
fn version_probe_timeout_remains_finite_when_normal_tool_timeout_is_disabled() {
    let dir = project("fail");
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    fs::write(tool.with_extension("version-delay-ms"), "11000").unwrap();
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("enabled = true", "enabled = true\ntimeout = 0"),
    )
    .unwrap();
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("--version exceeded its time limit"), "{text}");
    assert!(!dir.path().join("calls").exists());
}

#[test]
fn inherited_version_failures_withhold_configured_tool_details() {
    for policy in ["warn", "fail", "fail-fast"] {
        let dir = project(policy);
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            dir.path().join("base.toml"),
            fs::read_to_string(&config)
                .unwrap()
                .replace("ruff", "private-tool-marker")
                .replace("lint = [\"test\"]", "lint = [\"test\", \"other\"]")
                .replace(
                    "private-tool-marker = \"only-project\"",
                    "private-tool-marker = \"only-project\", private-other-marker = \"only-project\"",
                )
                + "\n[code-block-tools.tools.other]\ncommand=['private-other-marker']\nstdin=true\n",
        )
        .unwrap();
        fs::write(&config, "extends = 'base.toml'\n").unwrap();
        install(dir.path(), "private-other-marker", "unused\n", "count");
        install(
            dir.path(),
            "private-tool-marker",
            "private-tool-marker 0.11.2\n",
            "count",
        );
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
        );
        let text = output_text(&output);
        assert_eq!(output.status.success(), policy == "warn", "{policy}: {text}");
        assert!(
            text.contains("version verification failed (details withheld)"),
            "{text}"
        );
        assert!(
            !text.contains("private-tool-marker") && !text.contains("private-other-marker"),
            "{text}"
        );
        if policy == "warn" {
            assert_eq!(
                text.matches("version verification failed (details withheld)").count(),
                1,
                "{text}"
            );
        }
    }
}

#[test]
fn version_options_survive_config_output_aliases_and_reject_unsupported_sources() {
    let dir = project("fail");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("tool-version-definition", "tool_version_definition")
            .replace("on-tool-version-mismatch", "on_tool_version_mismatch"),
    )
    .unwrap();
    for format in ["toml", "json", "text"] {
        let output = run(dir.path(), &["config", "--output", format]);
        let text = output_text(&output);
        assert!(output.status.success(), "{format}: {text}");
        assert!(
            text.contains("tool-version-definition") && text.contains("uv.lock"),
            "{format}: {text}"
        );
        assert!(
            text.contains("on-tool-version-mismatch") && text.contains("fail"),
            "{format}: {text}"
        );
    }
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("uv.lock", "unsupported-source"),
    )
    .unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--deny-config-warnings",
            "--only-code-block-tools",
            "--no-cache",
            "doc.md",
        ],
    );
    let text = output_text(&output);
    assert!(!output.status.success(), "unsupported source silently accepted: {text}");
    assert!(
        text.contains("unsupported-source") && text.contains("uv.lock"),
        "{text}"
    );
}

#[test]
fn check_fix_shares_verification_between_format_and_lint_and_is_idempotent() {
    let dir = project("fail");
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
    for _ in 0..2 {
        let output = run(
            dir.path(),
            &["check", "--fix", "--only-code-block-tools", "--no-cache", "doc.md"],
        );
        assert!(output.status.success(), "{}", output_text(&output));
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            "```python\ngood\n```\n"
        );
    }
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[cfg(windows)]
#[test]
fn uppercase_windows_executable_names_map_to_the_python_package() {
    let dir = project("fail");
    let tool = install(dir.path(), "RUFF", "ruff 0.11.2\n", "count");
    let config = dir.path().join(".rumdl.toml");
    let literal = tool.display().to_string();
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("command = [\"ruff\"]", &format!("command = ['{literal}']")),
    )
    .unwrap();
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );
    assert!(output.status.success(), "{}", output_text(&output));
    assert!(dir.path().join("calls").exists());
    assert!(tool.with_extension("version-calls").exists());
}

#[test]
fn version_warnings_are_deduplicated_across_document_directories() {
    for unmapped in [false, true] {
        let dir = project("warn");
        fs::create_dir(dir.path().join(".git")).unwrap();
        let name = if unmapped { "custom-version-tool" } else { "ruff" };
        install(dir.path(), name, "unused\n", "count");
        if unmapped {
            let config = dir.path().join(".rumdl.toml");
            fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
        }
        fs::remove_file(dir.path().join("uv.lock")).unwrap();
        for folder in ["a", "b"] {
            fs::create_dir(dir.path().join(folder)).unwrap();
            fs::write(dir.path().join(folder).join("doc.md"), "```python\nx\n```\n").unwrap();
        }
        let output = run(
            dir.path(),
            &["check", "--only-code-block-tools", "--no-cache", "a/doc.md", "b/doc.md"],
        );
        let text = output_text(&output);
        assert!(output.status.success(), "{text}");
        let message = if unmapped {
            "Cannot resolve a Python package version"
        } else {
            "uv.lock not found"
        };
        assert_eq!(text.matches(message).count(), 1, "{text}");
        assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 2);
    }
}

#[test]
fn empty_blocks_do_not_probe_or_reject_unused_mismatched_tools() {
    let dir = project("fail-fast");
    let tool = install(dir.path(), "ruff", "ruff 0.11.1\n", "count-format");
    fs::write(dir.path().join("doc.md"), "```python\n```\n").unwrap();
    for args in [
        vec!["check", "--only-code-block-tools", "--no-cache", "doc.md"],
        vec!["fmt", "--only-code-block-tools", "--no-cache", "doc.md"],
        vec!["check", "--fix", "--only-code-block-tools", "--no-cache", "doc.md"],
    ] {
        let output = run(dir.path(), &args);
        assert!(output.status.success(), "{}", output_text(&output));
        assert!(!tool.with_extension("version-calls").exists());
        assert!(!dir.path().join("calls").exists());
    }
}

#[test]
fn built_in_ruff_modes_and_djlint_native_checks_verify_the_executed_binary() {
    for id in ["ruff", "ruff:format", "djlint:html:format-check"] {
        let dir = project("fail");
        let djlint = id.starts_with("djlint");
        let (name, reported, version) = if djlint {
            ("djlint", "djlint, version 1.39.5\n", "1.39.5")
        } else {
            ("ruff", "ruff 0.11.2\n", "0.11.2")
        };
        let tool = install(dir.path(), name, reported, "quiet");
        fs::write(tool.with_extension("record"), "yes").unwrap();
        fs::write(
            dir.path().join("uv.lock"),
            format!("version=1\n[[package]]\nname='{name}'\nversion='{version}'\n"),
        )
        .unwrap();
        let slot = if djlint { "lint" } else { "format" };
        fs::write(dir.path().join(".rumdl.toml"), format!("[code-block-tools]\nenabled=true\ntool-version-definition='uv.lock'\non-tool-version-mismatch='fail'\nbinary-preferences={{ {name}='only-project' }}\n[code-block-tools.languages.python]\n{slot}=['{id}']\n")).unwrap();
        let output = run(
            dir.path(),
            &[
                if djlint { "check" } else { "fmt" },
                "--only-code-block-tools",
                "--no-cache",
                "doc.md",
            ],
        );
        let text = output_text(&output);
        assert!(output.status.success(), "{id}: {text}");
        assert!(tool.with_extension("version-calls").exists());
        let args = fs::read_to_string(tool.with_extension("arguments")).unwrap();
        if djlint {
            assert!(
                args.lines().any(|arg| arg == "--check"),
                "native check did not run: {args}"
            );
        } else {
            assert!(args.starts_with("format\n"), "{args}");
            assert_eq!(
                fs::read_to_string(dir.path().join("doc.md")).unwrap(),
                "```python\ngood\n```\n"
            );
        }
    }
}

#[test]
fn check_fix_reports_one_mismatch_per_block_and_preserves_the_file() {
    let dir = project("fail");
    install(dir.path(), "ruff", "ruff 0.11.1\n", "count-format");
    let original = fs::read(dir.path().join("doc.md")).unwrap();
    let output = run(
        dir.path(),
        &[
            "check",
            "--fix",
            "--only-code-block-tools",
            "--no-cache",
            "--output-format",
            "json",
            "doc.md",
        ],
    );
    assert!(!output.status.success(), "{}", output_text(&output));
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnostics.as_array().unwrap().len(), 1, "{diagnostics}");
    assert!(
        diagnostics[0]["message"].as_str().unwrap().contains("version mismatch"),
        "{diagnostics}"
    );
    assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
    assert!(!dir.path().join("calls").exists());
}

#[test]
fn binary_fail_fast_does_not_execute_or_require_a_linter_in_a_format_slot() {
    let dir = project("ignore");
    fs::write(dir.path().join(".rumdl.toml"), "[code-block-tools]\nenabled=true\non-missing-tool-binary='fail-fast'\nbinary-preferences={ruff='only-project'}\n[code-block-tools.languages.python]\nformat=['ruff:check']\n").unwrap();
    let original = fs::read(dir.path().join("doc.md")).unwrap();
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
}

#[test]
fn embedded_markdown_does_not_exempt_additional_external_formatters() {
    for verify in [false, true] {
        let dir = project("fail-fast");
        let external = install(dir.path(), "unmapped-tool", "unmapped 1.0\n", "count-format");
        let source = if verify {
            "tool-version-definition='uv.lock'\n"
        } else {
            ""
        };
        fs::write(dir.path().join(".rumdl.toml"), format!("[code-block-tools]\nenabled=true\n{source}on-tool-version-mismatch='fail-fast'\nbinary-preferences={{unmapped-tool='only-project'}}\n[code-block-tools.languages.markdown]\nformat=['rumdl','external']\n[code-block-tools.tools.external]\ncommand=['unmapped-tool']\nstdin=true\nstdout=true\n")).unwrap();
        let original = "````markdown\n#  Title\n````\n";
        fs::write(dir.path().join("doc.md"), original).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--only-code-block-tools",
                "--enable",
                "MD019",
                "--deny-config-warnings",
                "--no-cache",
                "doc.md",
            ],
        );
        let text = output_text(&output);
        assert_eq!(output.status.success(), !verify, "{text}");
        if verify {
            assert!(text.contains("Cannot resolve a Python package version"), "{text}");
            assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), original);
            assert!(!dir.path().join("calls").exists());
        } else {
            assert_eq!(
                fs::read_to_string(dir.path().join("doc.md")).unwrap(),
                "````markdown\n# Title\n````\n"
            );
            assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
        }
        assert!(!external.with_extension("version-calls").exists());
    }
}

#[test]
fn lint_only_fail_fast_mismatch_prevents_markdown_and_formatter_writes() {
    let dir = project("fail-fast");
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace(
                "binary-preferences = { ruff = \"only-project\" }",
                "binary-preferences = { ruff = \"only-project\", black = \"only-project\" }",
            )
            .replace("lint = [\"test\"]", "lint = [\"other\"]")
            + "\n[code-block-tools.tools.other]\ncommand=['black']\nstdin=true\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("uv.lock"),
        "version=1\n[[package]]\nname='ruff'\nversion='0.11.2'\n[[package]]\nname='black'\nversion='0.11.2'\n",
    )
    .unwrap();
    install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
    install(dir.path(), "black", "black, 0.11.1 (compiled: no)\n", "count-finding");
    let original = "#  Title\n\n```python\nbad\n```\n";
    fs::write(dir.path().join("doc.md"), original).unwrap();
    let output = run(dir.path(), &["check", "--fix", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch"), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        original,
        "lint-only fail-fast wrote the document: {text}"
    );
    assert!(
        !dir.path().join("calls").exists(),
        "a tool ran after the lint-only fail-fast mismatch"
    );
}

#[test]
fn unavailable_first_formatter_preserves_warn_and_fail_fallback_semantics() {
    for policy in ["warn", "fail"] {
        let dir = project("fail-fast");
        two_formatter_config(dir.path(), "fail-fast", "fallback", "missing");
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            fs::read_to_string(&config).unwrap().replace(
                "on-missing-tool-binary = \"ignore\"",
                &format!("on-missing-tool-binary = '{policy}'"),
            ),
        )
        .unwrap();
        install(dir.path(), "black", "black, 0.11.2 (compiled: no)\n", "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert_eq!(output.status.success(), policy == "warn", "{policy}: {text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            "```python\ngood\n```\n",
            "{text}"
        );
        assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 1);
    }
}
