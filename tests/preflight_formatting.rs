use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
#[path = "support/native_tool.rs"]
mod native_tool;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn assert_unchanged(root: &Path, content: &[u8]) {
    assert_eq!(fs::read(root.join("a.md")).unwrap(), content);
}

#[test]
fn invalid_encoding_aborts_entire_batch_even_when_encoding_rule_disabled() {
    for bytes in [&[0xff, 0xfe, 0xff][..], &[b'a', 0xff][..], &[0, 0xff][..]] {
        let dir = tempfile::tempdir().unwrap();
        let original = b"#  Title  \r\n";
        fs::write(dir.path().join("a.md"), original).unwrap();
        fs::write(dir.path().join("b.md"), bytes).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--preflight",
                "--disable",
                "MD094",
                "--output-format",
                "json",
                "a.md",
                "b.md",
            ],
        );
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(diagnostics[0]["message"].as_str().unwrap().contains("UTF-8"));
        assert_unchanged(dir.path(), original);
        assert_eq!(fs::read(dir.path().join("b.md")).unwrap(), bytes);
    }
}

#[test]
fn successful_batch_preserves_encoding_line_endings_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "#  Héllo\r\n\r\nText.\r\n").unwrap();
    fs::write(dir.path().join("b.md"), "#  World\n").unwrap();
    for command in ["fmt", "check"] {
        let mut args = vec![command, "--preflight", "--output-format", "json", "a.md", "b.md"];
        if command == "check" {
            args.push("--fix");
        }
        let output = run(dir.path(), &args);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.md")).unwrap(),
            "# Héllo\r\n\r\nText.\r\n"
        );
        assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), "# World\n");
    }
}

#[test]
fn readonly_output_is_detected_before_any_write() {
    let dir = tempfile::tempdir().unwrap();
    let original = b"#  Title\n";
    for name in ["a.md", "b.md"] {
        fs::write(dir.path().join(name), original).unwrap();
    }
    let path = dir.path().join("b.md");
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let output = run(
        dir.path(),
        &["fmt", "--preflight", "--output-format", "json", "a.md", "b.md"],
    );
    fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_unchanged(dir.path(), original);
    assert_eq!(fs::read(path).unwrap(), original);
}

fn tool_config(root: &Path, mode: &str) {
    native_tool::install(root, mode);
    fs::write(root.join("pyproject.toml"), "").unwrap();
    fs::write(
        root.join(".rumdl.toml"),
        r#"
[code-block-tools]
enabled = true
on-error = "fail"
[code-block-tools.tools.test]
command = ["rumdl-policy-test"]
stdin = true
stdout = true
[code-block-tools.languages]
python = { format = ["test"] }
"#,
    )
    .unwrap();
}

#[test]
fn formatter_failure_discards_other_files_planned_changes() {
    for mode in ["fail", "invalid-utf8"] {
        let dir = tempfile::tempdir().unwrap();
        tool_config(dir.path(), mode);
        let original = b"#  Title\n";
        fs::write(dir.path().join("a.md"), original).unwrap();
        let block = "```python\nx\n```\n";
        fs::write(dir.path().join("b.md"), block).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--preflight",
                "--no-cache",
                "--output-format",
                "json",
                "a.md",
                "b.md",
            ],
        );
        assert_eq!(output.status.code(), Some(2), "{mode}: {output:?}");
        assert_unchanged(dir.path(), original);
        assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), block);
    }
}

#[test]
fn external_tools_run_once_during_planning_and_are_not_rerun_to_apply() {
    let dir = tempfile::tempdir().unwrap();
    tool_config(dir.path(), "count-echo");
    fs::write(dir.path().join("a.md"), "#  Title\n\n```python\nx\n```\n").unwrap();
    let output = run(dir.path(), &["fmt", "--preflight", "--no-cache", "a.md"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(dir.path().join("calls")).unwrap(), "ran\n");
    assert!(
        fs::read_to_string(dir.path().join("a.md"))
            .unwrap()
            .starts_with("# Title\n")
    );
}

#[test]
fn edits_during_planning_are_preserved_and_stop_rumdl_writes() {
    let dir = tempfile::tempdir().unwrap();
    tool_config(dir.path(), "edit-input");
    let original = b"#  Title\n\n```python\nx\n```\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    fs::write(dir.path().join("b.md"), "#  Another\n").unwrap();
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--preflight",
            "--no-cache",
            "--output-format",
            "json",
            "a.md",
            "b.md",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_unchanged(dir.path(), original);
    assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), "external edit\n");
}

#[test]
fn unsupported_modes_fail_before_processing() {
    for args in [
        vec!["check", "--preflight"],
        vec!["fmt", "--preflight", "--diff"],
        vec!["fmt", "--preflight", "--check"],
        vec!["fmt", "--preflight", "--stdin"],
        vec!["fmt", "--preflight", "--watch"],
        vec!["fmt", "--preflight", "-"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let original = b"#  Title\n";
        fs::write(dir.path().join("a.md"), original).unwrap();
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert_unchanged(dir.path(), original);
    }
}

#[test]
fn protected_merge_conflict_aborts_other_planned_changes() {
    let dir = tempfile::tempdir().unwrap();
    let original = b"#  Title\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    let conflict = "<<<<<<< ours\n# Ours\n=======\n# Theirs\n>>>>>>> theirs\n";
    fs::write(dir.path().join("b.md"), conflict).unwrap();
    let output = run(dir.path(), &["fmt", "--preflight", "a.md", "b.md"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_unchanged(dir.path(), original);
    assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), conflict);
}

#[test]
fn denied_coverage_warning_discards_changes_while_warn_policy_allows_them() {
    let dir = tempfile::tempdir().unwrap();
    tool_config(dir.path(), "echo");
    let original = "#  Title\n\n```pyhton\nx\n```\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    let output = run(dir.path(), &["fmt", "--preflight", "--deny-config-warnings", "a.md"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_unchanged(dir.path(), original.as_bytes());
    let output = run(dir.path(), &["fmt", "--preflight", "a.md"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        fs::read_to_string(dir.path().join("a.md"))
            .unwrap()
            .starts_with("# Title\n")
    );
}

#[cfg(unix)]
#[test]
fn preflight_preserves_symlinks_and_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.md");
    fs::write(&path, "#  Title\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&path, dir.path().join("link.md")).unwrap();
    let output = run(dir.path(), &["fmt", "--preflight", "link.md"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        fs::symlink_metadata(dir.path().join("link.md"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(fs::read_to_string(path).unwrap(), "# Title\n");
}

#[test]
fn combined_cli_pipeline_preserves_rendered_content_across_flavors_and_endings() {
    fn render(source: &str) -> String {
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(source));
        html
    }
    let source = "# Visible\n\n## Another\n\nTitle ###\n---\n\n- Alpha with [a link](https://example.com).\n- Beta with <span>HTML</span> and *literal space *.\n\n> Quote with **strong** text.\n\n```text\n- literal code\n> still code\n```\n\n    indented code\n    - literal list marker\n";
    for flavor in [
        "standard",
        "gfm",
        "mkdocs",
        "mdx",
        "pandoc",
        "quarto",
        "obsidian",
        "kramdown",
        "azure_devops",
        "myst",
        "hugo",
        "mdg",
    ] {
        for ending in ["\n", "\r\n", "mixed"] {
            let input = if ending == "mixed" {
                source
                    .split_inclusive('\n')
                    .enumerate()
                    .map(|(i, line)| {
                        if i % 2 == 0 {
                            line.replace('\n', "\r\n")
                        } else {
                            line.into()
                        }
                    })
                    .collect::<String>()
            } else {
                source.replace('\n', ending)
            };
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("a.md"), &input).unwrap();
            let output = run(dir.path(), &["fmt", "--preflight", "--flavor", flavor, "a.md"]);
            assert!(output.status.success(), "{flavor}/{ending}: {output:?}");
            let once = fs::read_to_string(dir.path().join("a.md")).unwrap();
            // MD046's documented conversion may add an explicit fence language.
            assert_eq!(
                render(&once)
                    .replace(" class=\"language-text\"", "")
                    .replace(" class=\"language-plaintext\"", ""),
                render(&input).replace(" class=\"language-text\"", ""),
                "{flavor}/{ending}: {once}"
            );
            let output = run(dir.path(), &["fmt", "--preflight", "--flavor", flavor, "a.md"]);
            assert!(output.status.success(), "{flavor}/{ending}: {output:?}");
            assert_eq!(
                fs::read_to_string(dir.path().join("a.md")).unwrap(),
                once,
                "{flavor}/{ending}"
            );
        }
    }
}

#[test]
fn aborted_preflight_reports_original_findings_and_never_claims_unchecked_files_passed() {
    let dir = tempfile::tempdir().unwrap();
    let original = "#  Title\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    fs::write(dir.path().join("b.md"), [0xff]).unwrap();
    let output = run(
        dir.path(),
        &["fmt", "--preflight", "--output-format", "junit", "a.md", "b.md"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let junit = String::from_utf8(output.stdout).unwrap();
    assert!(!junit.contains("a.md"), "file was not linted: {junit}");
    assert!(
        junit.contains("b.md") && junit.contains("type=\"preflight\""),
        "{junit}"
    );
    fs::write(dir.path().join("b.md"), original).unwrap();
    let path = dir.path().join("b.md");
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let output = run(
        dir.path(),
        &["fmt", "--preflight", "--output-format", "json", "a.md", "b.md"],
    );
    fs::set_permissions(path, permissions).unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["rule"] == "MD018" || d["rule"] == "MD019")
            .count(),
        2,
        "{diagnostics}"
    );
    assert_unchanged(dir.path(), original.as_bytes());
}
