//! Project version constraints verified through the CLI and native processes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "support/native_tool.rs"]
mod native_tool;

fn project(policy: &str, metadata: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            r#"[code-block-tools]
enabled = true
tool-version-definition = "pyproject.toml"
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
    fs::write(dir.path().join("pyproject.toml"), metadata).unwrap();
    fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n").unwrap();
    dir
}

fn install(root: &Path, name: &str, version: &str, mode: &str) -> PathBuf {
    let fixture = native_tool::install(root, "quiet");
    let tool = fixture.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    native_tool::copy_executable(&fixture, &tool);
    fs::write(tool.with_extension("mode"), mode).unwrap();
    fs::write(tool.with_extension("version"), version).unwrap();
    tool
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
fn pyproject_declared_range_rejects_an_out_of_range_formatter() {
    let dir = project("fail", "[project]\ndependencies = ['ruff>=0.11,<0.12']\n");
    install(dir.path(), "ruff", "ruff 0.12.1\n", "count-format");
    let original = fs::read(dir.path().join("doc.md")).unwrap();
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("version mismatch") && text.contains("pyproject.toml"),
        "{text}"
    );
    assert!(text.contains("0.12.1") && text.contains("<0.12"), "{text}");
    assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
    assert!(!dir.path().join("calls").exists(), "mismatched formatter ran");
}

#[test]
fn matching_installed_versions_satisfy_python_dependency_specifiers() {
    for (declaration, version) in [
        ("ruff>=0.11,<0.12", "0.11.2"),
        ("ruff>=0.11,", "0.11.2"),
        ("RUFF [extra] ( >=0.11, <0.12 )", "0.11.2"),
        ("ruff~=0.11.0", "0.11.5"),
        ("ruff==0.11.*", "0.11.2+vendor.1"),
        ("ruff==0.11.2", "0.11.2+vendor.1"),
        ("ruff==0.11.2+vendor.1", "0.11.2+vendor.1"),
        ("ruff!=0.11.1,>=0.11", "0.11.2"),
        ("ruff>=1!0.11,<2!0", "1!0.12"),
        ("ruff>=0.11", "0.12rc1"),
        ("ruff>=0.12rc1,<0.13", "0.12rc2"),
        ("ruff<=0.11.2", "0.11.2"),
        ("ruff==0.11.2.post1", "0.11.2.post1"),
        ("ruff==0.11.2.dev1", "0.11.2.dev1"),
    ] {
        let dir = project("fail", &format!("[project]\ndependencies = ['{declaration}']\n"));
        install(dir.path(), "ruff", &format!("ruff {version}\n"), "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert!(output.status.success(), "{declaration}, {version}: {text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            "```python\ngood\n```\n"
        );
        assert!(
            !text.contains("version mismatch") && !text.contains("Cannot "),
            "{text}"
        );
    }
}

#[test]
fn optional_dependency_constraints_are_enforced_alongside_project_dependencies() {
    let dir = project(
        "fail",
        "[project]\ndependencies = ['ruff>=0.11']\n[project.optional-dependencies]\ndev = ['ruff<0.12']\n",
    );
    install(dir.path(), "ruff", "ruff 0.12.1\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("version mismatch") && text.contains("0.12"), "{text}");
    assert!(!dir.path().join("calls").exists(), "excluded formatter ran");
}

#[test]
fn conditional_requirements_are_unresolved_even_when_the_marker_is_a_tautology() {
    let dir = project(
        "fail",
        "[project]\ndependencies = [\"ruff==0.11.2; os_name == 'nt' or os_name != 'nt'\"]\n",
    );
    install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("Cannot resolve") && text.contains("pyproject.toml"),
        "{text}"
    );
    assert!(!dir.path().join("calls").exists(), "conditional formatter ran");
}

#[test]
fn dependency_groups_supply_the_tool_constraint_without_selecting_an_environment() {
    let dir = project(
        "fail",
        "[dependency-groups]\nlint = ['ruff>=0.11']\ncompat = ['ruff<0.12']\ndev = [{include-group = 'lint'}, {include-group = 'compat'}]\n",
    );
    install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\ngood\n```\n"
    );
    assert!(!text.contains("Cannot "), "{text}");
}

#[test]
fn legacy_uv_development_and_constraint_dependencies_both_apply() {
    let dir = project(
        "fail",
        "[tool.uv]\ndev-dependencies = ['ruff>=0.11']\nconstraint-dependencies = ['ruff<0.12']\n",
    );
    install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\ngood\n```\n"
    );
    install(dir.path(), "ruff", "ruff 0.12.1\n", "count-format");
    let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
    let text = output_text(&output);
    assert!(!output.status.success() && text.contains("version mismatch"), "{text}");
}

#[test]
fn ruff_required_version_accepts_exact_versions_and_specifiers() {
    for (required, matches) in [
        ("0.11.2", true),
        (">=0.11,<0.12", true),
        ("==0.11.*", true),
        (">=0.12", false),
    ] {
        let dir = project("fail", &format!("[tool.ruff]\nrequired-version = '{required}'\n"));
        install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert_eq!(output.status.success(), matches, "{required}: {text}");
        assert_eq!(dir.path().join("calls").exists(), matches, "{required}: {text}");
        if !matches {
            assert!(text.contains("version mismatch"), "{text}");
        }
    }
}

#[test]
fn black_required_version_uses_native_major_or_exact_text_semantics() {
    for (key, required, actual, matches) in [
        ("required-version", "'25'", "25.1.0", true),
        ("required_version", "25", "25.1.0", true),
        ("required-version", "'25.1.0'", "25.1.0", true),
        ("required-version", "'25.1'", "25.1.0", false),
        ("required-version", "'025.1.0'", "25.1.0", false),
        ("required-version", "'25.1.0'", "025.1.0", false),
        ("required-version", "'v25.1.0'", "25.1.0", false),
        ("required-version", "'25.1.0.0'", "25.1.0", false),
        ("required-version", "'>=25'", "25.1.0", false),
        ("required-version", "25.1", "25.1.0", false),
        ("required-version", "true", "25.1.0", false),
    ] {
        let dir = project("fail", &format!("[tool.black]\n{key} = {required}\n"));
        let config = dir.path().join(".rumdl.toml");
        fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", "black")).unwrap();
        install(
            dir.path(),
            "black",
            &format!("black, {actual} (compiled: yes)\n"),
            "count-format",
        );
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert_eq!(
            output.status.success(),
            matches,
            "{key}={required}, actual={actual}: {text}"
        );
        assert_eq!(dir.path().join("calls").exists(), matches, "{text}");
        if !matches {
            let unresolved = matches!(required, "'>=25'" | "25.1" | "true");
            assert!(
                text.contains(if unresolved {
                    "Cannot resolve declared version constraints"
                } else {
                    "version mismatch"
                }),
                "{text}"
            );
        }
    }
}

#[test]
fn dynamic_dependency_fields_and_uv_overrides_are_unresolved() {
    for metadata in [
        "[project]\ndependencies=['ruff==0.11.2']\ndynamic=['dependencies']\n",
        "[project]\ndependencies=['ruff==0.11.2']\ndynamic=['optional-dependencies']\n",
        "[project]\ndependencies=['ruff==0.11.2']\n[tool.uv]\noverride-dependencies=['RUFF==0.11.2']\n",
    ] {
        let dir = project("fail", metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{metadata}: {text}"
        );
        assert!(!dir.path().join("calls").exists(), "unresolved formatter ran");
    }
}

#[test]
fn malformed_or_unsupported_selected_requirements_fail_closed_without_echoing_values() {
    for declaration in [
        "ruff>=0.11,,<0.12",
        "ruff>=0.11\n",
        "ruff>=0.11\u{a0}",
        "ruff.",
        "ruff[extra,]>=0.11",
        "ruff ()",
        "ruff===0.11.2",
        "ruff @ https://private.invalid/DO_NOT_ECHO",
        "ruff>=0.11; os_name == 'DO_NOT_ECHO'",
    ] {
        let dependencies = serde_json::to_string(&vec!["ruff>=0.11", declaration]).unwrap();
        let dir = project("fail", &format!("[project]\ndependencies={dependencies}\n"));
        install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{declaration:?}: {text}"
        );
        assert!(
            !text.contains("DO_NOT_ECHO") && !text.contains("private.invalid"),
            "{text}"
        );
        assert!(!dir.path().join("calls").exists());
    }
}

#[test]
fn installed_version_constraint_corpus_matches_pep440() {
    #[derive(serde::Deserialize)]
    struct Case {
        specifier: String,
        version: String,
        matches: bool,
    }
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/tool_version_constraints.json")).unwrap();
    for case in cases {
        let dir = project("fail", &format!("[project]\ndependencies=['ruff{}']\n", case.specifier));
        install(dir.path(), "ruff", &format!("ruff {}\n", case.version), "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert_eq!(
            output.status.success(),
            case.matches,
            "{}, {}: {text}",
            case.specifier,
            case.version
        );
        assert_eq!(dir.path().join("calls").exists(), case.matches, "{text}");
        assert!(!text.contains("Cannot "), "{text}");
    }
}

#[test]
fn unrelated_declarations_and_malformed_native_settings_are_isolated_per_tool() {
    for metadata in [
        "[project]\ndependencies=['ruff>=0.11', 'RUFF', 'ruff-lsp @ bad', 'unrelated !invalid']\n[tool.black]\nrequired-version=true\n",
        "[tool.ruff]\nrequired-version='0.11.2'\n[tool.uv]\noverride-dependencies=['black @ bad']\n",
        "[dependency-groups]\nlint=['ruff==0.11.2', {include-group='missing'}]\n",
    ] {
        let dir = project("fail", metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n", "count-format");
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        assert!(output.status.success(), "{metadata}: {}", output_text(&output));
    }
}

#[test]
fn all_four_supported_packages_and_native_constraints_combine() {
    for (name, reported) in [
        ("ruff", "ruff 0.11.2\n"),
        ("black", "black, 0.11.2 (compiled: yes)\n"),
        ("djlint", "djlint, version 0.11.2\n"),
        ("sqlfluff", "sqlfluff, version 0.11.2\n"),
    ] {
        let dir = project("fail", &format!("[project]\ndependencies=['{name}>=0.11,<0.12']\n"));
        let config = dir.path().join(".rumdl.toml");
        fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
        install(dir.path(), name, reported, "count-format");
        assert!(
            run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"])
                .status
                .success()
        );
    }
    for metadata in [
        "[project]\ndependencies=['ruff<0.11']\n[tool.ruff]\nrequired-version='0.11.2'\n",
        "[project]\ndependencies=['black<25']\n[tool.black]\nrequired-version='25'\n",
        "[tool.black]\nrequired-version='25'\nrequired_version='25'\n",
    ] {
        let name = if metadata.contains("black") { "black" } else { "ruff" };
        let dir = project("fail", metadata);
        let config = dir.path().join(".rumdl.toml");
        fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
        install(
            dir.path(),
            name,
            if name == "ruff" {
                "ruff 0.11.2\n"
            } else {
                "black, 25.1.0\n"
            },
            "count-format",
        );
        let output = run(dir.path(), &["fmt", "--only-code-block-tools", "--no-cache", "doc.md"]);
        let text = output_text(&output);
        assert!(!output.status.success(), "{text}");
        let unresolved = metadata.contains("required_version");
        assert!(
            text.contains(if unresolved {
                "Cannot resolve declared version constraints"
            } else {
                "version mismatch"
            }),
            "{text}"
        );
        assert!(!dir.path().join("calls").exists());
    }
}

#[test]
fn policies_control_unresolved_versions_before_check_and_format() {
    for command in ["check", "fmt"] {
        for (policy, succeeds, runs) in [
            ("warn", true, true),
            ("ignore", true, true),
            ("fail", false, false),
            ("fail-fast", false, false),
        ] {
            let dir = project(policy, "[project]\ndependencies=['ruff']\n");
            let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count-echo");
            let original = fs::read(dir.path().join("doc.md")).unwrap();
            let output = run(
                dir.path(),
                &[command, "--only-code-block-tools", "--no-cache", "doc.md"],
            );
            let text = output_text(&output);
            assert_eq!(output.status.success(), succeeds, "{command} {policy}: {text}");
            assert_eq!(dir.path().join("calls").exists(), runs, "{text}");
            let actual = fs::read_to_string(tool.with_extension("version-calls"))
                .unwrap_or_default()
                .lines()
                .count();
            assert_eq!(actual, 0);
            assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
            if policy == "warn" {
                let denied = run(
                    dir.path(),
                    &[
                        command,
                        "--only-code-block-tools",
                        "--no-cache",
                        "--deny-config-warnings",
                        "doc.md",
                    ],
                );
                assert!(!denied.status.success(), "{}", output_text(&denied));
            }
        }
    }
}

#[test]
fn nearest_metadata_is_authoritative_without_fallback_to_outer_files_or_uv_lock() {
    let dir = project("fail", "[project]\ndependencies=['ruff==0.11.2']\n");
    fs::write(dir.path().join(".git"), "gitdir: fixture\n").unwrap();
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    let nested = dir.path().join("package/docs");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("doc.md"), "```python\nx\n```\n").unwrap();
    let args = ["check", "--only-code-block-tools", "--no-cache", "package/docs/doc.md"];
    assert!(run(dir.path(), &args).status.success());
    let metadata = dir.path().join("package/pyproject.toml");
    for contents in [
        "[project]\ndependencies=['black==25']\n",
        "DO_NOT_ECHO [invalid",
        "[project]\ndependencies=[false]\n",
        "[dependency-groups]\nlint=[{include-group='lint', unknown=true}]\n",
    ] {
        fs::write(&metadata, contents).unwrap();
        fs::write(
            dir.path().join("package/uv.lock"),
            "version=1\n[[package]]\nname='ruff'\nversion='0.11.2'\n",
        )
        .unwrap();
        let output = run(dir.path(), &args);
        assert!(!output.status.success(), "{}", output_text(&output));
        assert!(!output_text(&output).contains("DO_NOT_ECHO"));
    }
    fs::remove_file(metadata).unwrap();
    fs::write(dir.path().join("package/.git"), "gitdir: other\n").unwrap();
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        fs::read_to_string(&config).unwrap().replace(
            "command = [\"ruff\"]",
            &format!("command = [{}]", serde_json::to_string(tool.to_str().unwrap()).unwrap()),
        ),
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
            "package/docs/doc.md",
        ],
    );
    assert!(
        !bounded.status.success() && output_text(&bounded).contains("pyproject.toml not found"),
        "{}",
        output_text(&bounded)
    );
}

#[test]
fn cached_checks_reverify_versions_and_leave_metadata_bytes_and_mtime_unchanged() {
    let dir = project("fail", "[project]\ndependencies=['ruff>=0.11,<0.12']\n");
    let metadata = dir.path().join("pyproject.toml");
    let original = fs::read(&metadata).unwrap();
    let mtime = fs::metadata(&metadata).unwrap().modified().unwrap();
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    let args = ["check", "--only-code-block-tools", "doc.md"];
    assert!(run(dir.path(), &args).status.success());
    fs::write(tool.with_extension("version"), "ruff 0.12.1\n").unwrap();
    let changed = run(dir.path(), &args);
    assert!(!changed.status.success() && output_text(&changed).contains("version mismatch"));
    assert_eq!(fs::read(&metadata).unwrap(), original);
    assert_eq!(fs::metadata(&metadata).unwrap().modified().unwrap(), mtime);
    fs::write(&metadata, "[project]\ndependencies=['ruff>=0.12']\n").unwrap();
    assert!(run(dir.path(), &args).status.success());
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn mixed_sources_share_one_executable_probe_and_keep_distinct_diagnostics() {
    let dir = project("warn", "[project]\ndependencies=['ruff==0.12']\n");
    let tool = install(dir.path(), "ruff", "ruff 0.11.2\n", "count");
    let config = fs::read_to_string(dir.path().join(".rumdl.toml")).unwrap().replace(
        "command = [\"ruff\"]",
        &format!("command = [{}]", serde_json::to_string(tool.to_str().unwrap()).unwrap()),
    );
    for (package, source) in [("a", "pyproject.toml"), ("b", "uv.lock")] {
        let root = dir.path().join(package);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".rumdl.toml"), config.replace("pyproject.toml", source)).unwrap();
        fs::write(root.join("pyproject.toml"), "[project]\ndependencies=['ruff==0.12']\n").unwrap();
        fs::write(
            root.join("uv.lock"),
            "version=1\n[[package]]\nname='ruff'\nversion='0.13'\n",
        )
        .unwrap();
        for name in ["first.md", "second.md"] {
            fs::write(root.join(name), "```python\nx\n```\n").unwrap();
        }
    }
    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "a", "b"],
    );
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(text.matches("version mismatch").count(), 2, "{text}");
    assert!(text.contains("pyproject.toml") && text.contains("uv.lock"), "{text}");
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(), 4);
}
