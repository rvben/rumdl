use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(args)
        .current_dir(root)
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .env("APPDATA", root.join("xdg"))
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn aliases_override_rule_ids_and_reenable_disabled_rules() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(root, "docs/test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
    for (parent, child, fails) in [
        (
            r#"{"MD013":{"line_length":100}}"#,
            r#"{"line-length":{"line_length":120}}"#,
            false,
        ),
        (r#"{"MD013":false}"#, r#"{"line-length":true}"#, true),
        (r#"{"line-length":false}"#, r#"{"MD013":true}"#, true),
    ] {
        write(root, ".markdownlint-cli2.jsonc", &format!("{{\"config\":{parent}}}"));
        write(
            root,
            "docs/.markdownlint-cli2.jsonc",
            &format!("{{\"config\":{child}}}"),
        );
        let result = run(root, &["check", "--no-cache", "docs/test.md"]);
        assert_eq!(text(&result).contains("[MD013]"), fails, "{}", text(&result));
        assert_eq!(result.status.code(), Some(i32::from(fails)), "{}", text(&result));
    }
}

#[test]
fn literal_overwrites_keep_property_order_before_alias_resolution() {
    // Updating MD013 does not move it behind the already present alias in a
    // JavaScript object. The alias still wins, exactly as in CLI2.
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"MD013":{"line_length":100},"line-length":{"line_length":120}}}"#,
    );
    write(
        root,
        "docs/.markdownlint-cli2.jsonc",
        r#"{"config":{"MD013":{"line_length":80}}}"#,
    );
    write(root, "docs/test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
    let result = run(root, &["check", "--no-cache", "docs/test.md"]);
    assert!(result.status.success(), "{}", text(&result));
}

#[test]
fn home_equal_to_checkout_keeps_cli2_parent_and_child_settings() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  MD013:\n    line_length: 100\n",
    );
    write(root, "docs/.markdownlint-cli2.yaml", "config:\n  MD018: false\n");
    write(
        root,
        "docs/test.md",
        &format!("# T\n\n{}end\n\n#Heading\n", "word ".repeat(18)),
    );
    let result = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "docs/test.md"])
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

#[test]
fn fmt_reflow_uses_inherited_line_length() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  MD013:\n    line_length: 100\n    reflow: true\n",
    );
    write(root, "docs/.markdownlint-cli2.yaml", "config:\n  MD041: false\n");
    let document = format!("# T\n\n{}end\n", "word ".repeat(18));
    write(root, "docs/test.md", &document);
    let result = run(root, &["fmt", "--no-cache", "docs/test.md"]);
    assert!(result.status.success(), "{}", text(&result));
    assert_eq!(fs::read_to_string(root.join("docs/test.md")).unwrap(), document);
}

#[test]
fn cli2_ignores_apply_to_explicit_paths_scans_and_fmt_in_every_format() {
    for (name, contents) in [
        (".markdownlint-cli2.yaml", "ignores:\n  - docs/notes/**\n"),
        (".markdownlint-cli2.yml", "ignores:\n  - docs/notes/**\n"),
        (
            ".markdownlint-cli2.jsonc",
            "// ignore generated notes\n{\"ignores\":[\"docs/notes/**\"]}",
        ),
    ] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        write(root, name, contents);
        write(root, "docs/notes/x.md", "# T\n\ntext   \n");
        // Plain rule configuration wins, but must not discard CLI2 selection.
        write(root, ".markdownlint.json", r#"{"default":false,"MD009":true}"#);
        for args in [
            vec!["check", "--no-cache", "docs/notes/x.md"],
            vec!["check", "--no-cache", "."],
            vec!["fmt", "--no-cache", "docs/notes/x.md"],
        ] {
            let result = run(root, &args);
            assert!(result.status.success(), "{name}: {}", text(&result));
            assert!(!text(&result).contains("[MD009]"), "{}", text(&result));
            assert_eq!(fs::read(root.join("docs/notes/x.md")).unwrap(), b"# T\n\ntext   \n");
        }
        let included = run(root, &["check", "--no-cache", "--no-exclude", "docs/notes/x.md"]);
        assert_eq!(included.status.code(), Some(1), "{}", text(&included));
        assert!(text(&included).contains("[MD009]"));
    }
}

#[test]
fn nested_ignores_are_reapplied_and_can_be_replaced_without_losing_root_ignores() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    // CLI2 configuration grouping must also work outside a Git repository.
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["docs/root.md"]}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"ignores":["notes/**"]}"#);
    write(
        root,
        "docs/deep/.markdownlint.json",
        r#"{"default":false,"MD009":true}"#,
    );
    write(root, "docs/deep/sub/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    for path in [
        "docs/root.md",
        "docs/notes/x.md",
        "docs/deep/notes/x.md",
        "docs/deep/sub/notes/x.md",
    ] {
        write(root, path, "# T\n\ntext   \n");
    }
    let result = run(root, &["check", "--no-cache", "."]);
    let output = text(&result);
    let findings: Vec<_> = output.lines().filter(|line| line.contains("[MD009]")).collect();
    assert_eq!(findings.len(), 1, "{output}");
    assert!(findings[0].contains("docs/deep/sub/notes/x.md"), "{output}");
}

#[test]
fn ignored_invalid_utf8_is_not_read_by_fmt_preflight() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\n",
    );
    write(root, "docs/.markdownlint-cli2.yaml", "ignores:\n  - ignored.md\n");
    fs::write(root.join("docs/ignored.md"), [0xff, 0xfe, 0xff]).unwrap();
    write(root, "docs/keep.md", "# T\n\ntext   \n");
    let result = run(root, &["fmt", "--preflight", "--no-cache", "."]);
    assert!(result.status.success(), "{}", text(&result));
    assert_eq!(fs::read(root.join("docs/ignored.md")).unwrap(), [0xff, 0xfe, 0xff]);
    assert_eq!(fs::read_to_string(root.join("docs/keep.md")).unwrap(), "# T\n\ntext\n");
}

#[test]
fn ignored_named_stdin_passes_through_without_formatting() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(root, ".markdownlint-cli2.yaml", "ignores:\n  - docs/**\n");
    let input = "# T\r\n\r\ntext   \r\n";
    for name in ["docs/unsaved.md", "docs/generated.py"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["fmt", "--stdin", "--stdin-filename", name, "--no-cache"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(result.stdout, input.as_bytes());
    }
}

#[test]
fn batch_stdin_skips_ignored_documents_and_keeps_selected_diagnostics() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["docs/ignored.md"]}"#,
    );
    for (input, expected) in [
        ("docs/ignored.md\0# T\n\ntext   \n\0", 0),
        ("docs/ignored.md\0# T\n\ntext   \n\0docs/keep.md\0# T\n\ntext   \n\0", 1),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--stdin-batch", "--output-format", "json", "--no-cache"])
            .current_dir(root)
            .env("XDG_CONFIG_HOME", root.join("xdg"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.code(), Some(expected), "{}", text(&result));
        let diagnostics: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(diagnostics.as_array().unwrap().len(), expected as usize);
        assert!(!String::from_utf8_lossy(&result.stdout).contains("docs/ignored.md"));
    }
}

#[test]
fn options_only_child_inherits_parent_cli2_rules_before_parent_plain_rules() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint.json",
        r#"{"default":false,"MD013":{"line_length":120}}"#,
    );
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD013":{"line_length":80}}}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    write(root, "docs/test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
    let result = run(root, &["check", "--no-cache", "."]);
    assert!(text(&result).contains("[MD013]"), "{}", text(&result));
    // A child rule map also inherits the parent CLI2 map.
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"config":{"MD041":false}}"#);
    let result = run(root, &["check", "--no-cache", "."]);
    assert!(text(&result).contains("[MD013]"), "{}", text(&result));
}

#[test]
fn root_ignores_survive_native_subdirectory_configs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\nignores:\n  - docs/generated/**\n",
    );
    write(root, "docs/.rumdl.toml", "[global]\nenable = [\"MD009\"]\n");
    write(root, "docs/generated/x.md", "# T\n\ntext   \n");
    write(root, "docs/keep.md", "# T\n\ntext   \n");
    for command in ["check", "fmt"] {
        let result = run(root, &[command, "--no-cache", "docs/generated/x.md"]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(
            fs::read_to_string(root.join("docs/generated/x.md")).unwrap(),
            "# T\n\ntext   \n"
        );
    }
    let check = run(root, &["check", "--no-cache", "."]);
    assert!(text(&check).contains("docs/keep.md"), "{}", text(&check));
    assert!(!text(&check).contains("docs/generated/x.md"), "{}", text(&check));
    let fmt = run(root, &["fmt", "--no-cache", "."]);
    assert!(fmt.status.success(), "{}", text(&fmt));
    assert_eq!(
        fs::read_to_string(root.join("docs/generated/x.md")).unwrap(),
        "# T\n\ntext   \n"
    );
    assert_eq!(fs::read_to_string(root.join("docs/keep.md")).unwrap(), "# T\n\ntext\n");
    let included = run(root, &["check", "--no-cache", "--no-exclude", "."]);
    assert!(text(&included).contains("docs/generated/x.md"), "{}", text(&included));
}

#[test]
fn extglob_ignores_keep_generated_files_unchanged_during_fmt() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["**/@(generated|vendor)/**","docs/@(café|vendor).md"]}"#,
    );
    for path in [
        "docs/generated/x.md",
        "docs/vendor/x.md",
        "docs/café.md",
        "docs/vendor.md",
        "docs/keep.md",
    ] {
        write(root, path, "# T\n\ntext   \n");
    }
    let result = run(root, &["fmt", "--no-cache", "."]);
    assert!(result.status.success(), "{}", text(&result));
    for path in [
        "docs/generated/x.md",
        "docs/vendor/x.md",
        "docs/café.md",
        "docs/vendor.md",
    ] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), "# T\n\ntext   \n");
    }
    assert_eq!(fs::read_to_string(root.join("docs/keep.md")).unwrap(), "# T\n\ntext\n");
}

#[cfg(unix)]
#[test]
fn explicitly_ignored_symlinks_do_not_format_their_external_targets() {
    let temp = tempdir().unwrap();
    let target = tempdir().unwrap();
    let root = temp.path();
    write(root, ".markdownlint-cli2.yaml", "ignores:\n  - docs/**\n");
    write(target.path(), "target.md", "# T\n\ntext   \n");
    fs::create_dir(root.join("docs")).unwrap();
    std::os::unix::fs::symlink(target.path().join("target.md"), root.join("docs/ignored.md")).unwrap();
    for path in [
        "docs/ignored.md".to_string(),
        root.join("docs/ignored.md").display().to_string(),
    ] {
        let result = run(root, &["fmt", "--no-cache", &path]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(
            fs::read_to_string(target.path().join("target.md")).unwrap(),
            "# T\n\ntext   \n"
        );
    }
}

#[cfg(unix)]
#[test]
fn nested_ignores_keep_supplied_symlink_names_in_multi_path_runs() {
    let temp = tempdir().unwrap();
    let target = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\n",
    );
    write(root, "docs/.markdownlint-cli2.yaml", "ignores:\n  - ignored.md\n");
    write(root, "keep.md", "# T\n\ntext   \n");
    write(target.path(), "target.md", "# T\n\ntext   \n");
    std::os::unix::fs::symlink(target.path().join("target.md"), root.join("docs/ignored.md")).unwrap();
    for name in [
        "docs/ignored.md".to_string(),
        root.join("docs/ignored.md").display().to_string(),
    ] {
        let result = run(root, &["fmt", "--no-cache", &name, "keep.md"]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(
            fs::read_to_string(target.path().join("target.md")).unwrap(),
            "# T\n\ntext   \n"
        );
    }
    assert_eq!(fs::read_to_string(root.join("keep.md")).unwrap(), "# T\n\ntext\n");
}

#[test]
fn absolute_root_ignores_cover_targets_outside_the_invocation_directory() {
    let temp = tempdir().unwrap();
    let target = tempdir().unwrap();
    let root = temp.path();
    write(target.path(), "generated.md", "# T\n\ntext   \n");
    let file = target.path().join("generated.md");
    let pattern = file.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    write(
        root,
        ".markdownlint-cli2.jsonc",
        &serde_json::json!({"ignores":[pattern]}).to_string(),
    );
    let result = run(root, &["fmt", "--no-cache", file.to_str().unwrap()]);
    assert!(result.status.success(), "{}", text(&result));
    assert_eq!(fs::read_to_string(file).unwrap(), "# T\n\ntext   \n");
}

#[test]
fn numeric_range_ignores_exclude_only_the_declared_chapters() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["docs/chapter{1..3}.md"]}"#,
    );
    for index in 1..=4 {
        write(root, &format!("docs/chapter{index}.md"), "# T\n\ntext   \n");
    }
    let result = run(root, &["fmt", "--no-cache", "."]);
    assert!(result.status.success(), "{}", text(&result));
    for index in 1..=3 {
        assert_eq!(
            fs::read_to_string(root.join(format!("docs/chapter{index}.md"))).unwrap(),
            "# T\n\ntext   \n"
        );
    }
    assert_eq!(
        fs::read_to_string(root.join("docs/chapter4.md")).unwrap(),
        "# T\n\ntext\n"
    );
}

#[test]
fn relative_root_ignores_cover_external_directories() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("project");
    write(&root, ".markdownlint-cli2.yaml", "ignores:\n  - ../generated/**\n");
    write(temp.path(), "generated/x.md", "# T\n\ntext   \n");
    for name in ["../generated/x.md", "../generated"] {
        let result = run(&root, &["fmt", "--no-cache", name]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(
            fs::read_to_string(temp.path().join("generated/x.md")).unwrap(),
            "# T\n\ntext   \n"
        );
    }
}

#[cfg(unix)]
#[test]
fn ignored_symlink_directory_arguments_leave_external_files_unchanged() {
    let temp = tempdir().unwrap();
    let target = tempdir().unwrap();
    let root = temp.path();
    write(root, ".markdownlint-cli2.yaml", "ignores:\n  - docs/**\n");
    write(target.path(), "x.md", "# T\n\ntext   \n");
    std::os::unix::fs::symlink(target.path(), root.join("docs")).unwrap();
    for command in ["check", "fmt"] {
        let result = run(root, &[command, "--no-cache", "docs"]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(
            fs::read_to_string(target.path().join("x.md")).unwrap(),
            "# T\n\ntext   \n"
        );
    }
}

#[test]
fn external_directory_configs_keep_their_own_ignore_scope() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("project");
    write(&root, ".markdownlint-cli2.yaml", "ignores:\n  - ../docs/root.md\n");
    write(
        temp.path(),
        "docs/.markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\nignores:\n  - ignored.md\n",
    );
    for name in ["root.md", "ignored.md", "keep.md"] {
        write(temp.path(), &format!("docs/{name}"), "# T\n\ntext   \n");
    }
    let result = run(&root, &["fmt", "--no-cache", "../docs/ignored.md"]);
    assert!(result.status.success(), "{}", text(&result));
    let result = run(&root, &["fmt", "--no-cache", "../docs"]);
    assert!(result.status.success(), "{}", text(&result));
    for name in ["root.md", "ignored.md"] {
        assert_eq!(
            fs::read_to_string(temp.path().join(format!("docs/{name}"))).unwrap(),
            "# T\n\ntext   \n"
        );
    }
    assert_eq!(
        fs::read_to_string(temp.path().join("docs/keep.md")).unwrap(),
        "# T\n\ntext\n"
    );
}

#[test]
fn malformed_discovered_cli2_options_keep_fallback_and_explicit_errors() {
    for contents in ["{ invalid", r#"{"ignores":42}"#, r#"{"ignores":["[unclosed"]}"#] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        write(root, ".markdownlint-cli2.jsonc", contents);
        write(root, "test.md", "# T\n");
        let result = run(root, &["check", "--no-cache", "test.md"]);
        assert!(result.status.success(), "{}", text(&result));
        let explicit = run(
            root,
            &["--config", ".markdownlint-cli2.jsonc", "check", "--no-cache", "test.md"],
        );
        assert_eq!(explicit.status.code(), Some(2), "{}", text(&explicit));
        write(root, "docs/.rumdl.toml", "[global]\nenable = [\"MD009\"]\n");
        write(root, "docs/test.md", "# T\n\ntext   \n");
        let nested = run(root, &["check", "--no-cache", "docs/test.md"]);
        assert_eq!(nested.status.code(), Some(1), "{}", text(&nested));
        assert!(text(&nested).contains("[MD009]"), "{}", text(&nested));
    }
}

#[test]
fn malformed_companion_options_preserve_plain_rules() {
    for contents in ["{ invalid", r#"{"ignores":42}"#, r#"{"ignores":["[unclosed"]}"#] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        write(
            root,
            ".markdownlint.json",
            r#"{"default":false,"MD013":{"line_length":120}}"#,
        );
        write(root, ".markdownlint-cli2.jsonc", contents);
        write(root, "test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
        let result = run(root, &["check", "--no-cache", "test.md"]);
        assert!(result.status.success(), "{}", text(&result));
    }
}

#[test]
fn native_root_config_suppresses_companion_cli2_selection() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(root, ".rumdl.toml", "[global]\nenable = [\"MD009\"]\n");
    write(root, ".markdownlint-cli2.yaml", "ignores:\n  - docs/**\n");
    write(root, "docs/.markdownlint.yaml", "default: false\nMD009: true\n");
    write(root, "docs/test.md", "# T\n\ntext   \n");
    let result = run(root, &["check", "--no-cache", "docs"]);
    assert_eq!(result.status.code(), Some(1), "{}", text(&result));
    assert!(text(&result).contains("[MD009]"));
    let result = run(root, &["fmt", "--no-cache", "docs"]);
    assert!(result.status.success(), "{}", text(&result));
    assert_eq!(fs::read_to_string(root.join("docs/test.md")).unwrap(), "# T\n\ntext\n");
}

#[test]
fn empty_cli2_selection_honors_no_tools_policy() {
    for (policy, exit) in [("ignore", 0), ("warn", 0), ("fail", 2), ("fail-fast", 2)] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        write(
            root,
            ".rumdl.toml",
            &format!("[code-block-tools]\nenabled = true\non-no-tools-run = \"{policy}\"\n"),
        );
        write(root, "docs/.markdownlint-cli2.yaml", "ignores:\n  - '*.md'\n");
        write(root, "docs/test.md", "# T\n");
        let result = run(root, &["check", "--no-cache", "docs"]);
        assert_eq!(result.status.code(), Some(exit), "{policy}: {}", text(&result));
        if policy != "ignore" {
            assert!(
                text(&result).to_lowercase().contains("no tools executed"),
                "{}",
                text(&result)
            );
        }
    }
}

#[test]
fn nearer_ignore_clear_keeps_explicit_files_in_multi_path_runs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "docs/.markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\nignores:\n  - '**/notes/**'\n",
    );
    write(root, "docs/sub/.markdownlint-cli2.yaml", "ignores: []\n");
    write(root, "docs/notes/ignored.md", "# T\n\ntext   \n");
    write(root, "docs/sub/notes/keep.md", "# T\n\ntext   \n");
    let result = run(
        root,
        &["check", "--no-cache", "docs/notes/ignored.md", "docs/sub/notes/keep.md"],
    );
    assert_eq!(result.status.code(), Some(1), "{}", text(&result));
    assert!(text(&result).contains("docs/sub/notes/keep.md"), "{}", text(&result));
    assert!(!text(&result).contains("docs/notes/ignored.md"), "{}", text(&result));
}

#[test]
fn malformed_ancestor_rules_preserve_valid_child_configuration() {
    for filename in [".markdownlint-cli2.jsonc", ".markdownlint.json"] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        write(root, filename, "{ invalid");
        write(
            root,
            "docs/.markdownlint-cli2.jsonc",
            r#"{"config":{"default":false,"MD013":{"line_length":120}}}"#,
        );
        write(root, "docs/test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
        for target in ["docs", "docs/test.md"] {
            let result = run(root, &["check", "--no-cache", target]);
            assert!(result.status.success(), "{}", text(&result));
        }
        fs::create_dir(root.join(".git")).unwrap();
        let result = run(root, &["check", "--no-cache", "."]);
        assert!(result.status.success(), "{}", text(&result));
    }
}

#[test]
fn nested_ignored_source_code_does_not_block_disk_or_batch_selection() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD009: true\n",
    );
    write(root, "docs/.markdownlint-cli2.yaml", "ignores:\n  - '*.py'\n");
    write(root, "docs/generated.py", "print('keep me')\n");
    write(root, "docs/keep.md", "# T\n\ntext   \n");
    let paths = ["docs/generated.py", "docs/keep.md"];
    let result = run(root, &["check", "--no-cache", paths[0], paths[1]]);
    assert_eq!(result.status.code(), Some(1), "{}", text(&result));
    assert!(text(&result).contains("[MD009]"), "{}", text(&result));
    let unignored = run(root, &["check", "--no-cache", "--no-exclude", paths[0], paths[1]]);
    assert_eq!(unignored.status.code(), Some(2), "{}", text(&unignored));
    let input = "docs/generated.py\0print('keep me')\n\0docs/keep.md\0# T\n\ntext   \n\0";
    for (extra, exit) in [(vec![], 1), (vec!["--no-exclude"], 2)] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--stdin-batch", "--no-cache"])
            .args(extra)
            .current_dir(root)
            .env("XDG_CONFIG_HOME", root.join("xdg"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.code(), Some(exit), "{}", text(&result));
    }
}

#[test]
fn markdownlint_groups_preserve_user_editorconfig_settings_without_git() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(root, "xdg/rumdl/rumdl.toml", "[global]\neditorconfig = true\n");
    write(root, ".editorconfig", "root = true\n\n[*.md]\nmax_line_length = 120\n");
    write(
        root,
        "docs/.markdownlint-cli2.yaml",
        "config:\n  default: false\n  MD013: true\n",
    );
    write(root, "docs/test.md", &format!("# T\n\n{}end\n", "word ".repeat(21)));
    for target in ["docs", "docs/test.md"] {
        let result = run(root, &["check", "--no-cache", target]);
        assert!(result.status.success(), "{}", text(&result));
    }
    write(root, "docs/.rumdl.toml", "[global]\nenable = [\"MD013\"]\n");
    let native = run(root, &["check", "--no-cache", "docs/test.md"]);
    assert_eq!(native.status.code(), Some(1), "{}", text(&native));
    assert!(text(&native).contains("[MD013]"), "{}", text(&native));
}

#[test]
fn root_ignore_patterns_are_inherited_and_reapplied_in_nested_groups() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["generated/*.md"]}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"config":{"MD041":false}}"#);
    write(
        root,
        "docs/deep/.markdownlint.json",
        r#"{"default":false,"MD009":true}"#,
    );
    write(root, "docs/clear/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    for path in [
        "generated/root.md",
        "docs/generated/nested.md",
        "docs/deep/generated/plain.md",
        "docs/clear/generated/keep.md",
    ] {
        write(root, path, "# T\n\ntext   \n");
    }
    let result = run(root, &["fmt", "--no-cache", "."]);
    assert!(result.status.success(), "{}", text(&result));
    for path in [
        "generated/root.md",
        "docs/generated/nested.md",
        "docs/deep/generated/plain.md",
    ] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), "# T\n\ntext   \n");
    }
    assert_eq!(
        fs::read_to_string(root.join("docs/clear/generated/keep.md")).unwrap(),
        "# T\n\ntext\n"
    );
}

#[test]
fn root_negations_filter_discovery_and_each_config_group_separately() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["!keep.md"]}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"config":{"MD041":false}}"#);
    write(root, "docs/clear/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    for path in [
        "keep.md",
        "other.md",
        "docs/keep.md",
        "docs/other.md",
        "docs/clear/other.md",
    ] {
        write(root, path, "# T\n\ntext   \n");
    }
    let result = run(root, &["fmt", "--no-cache", "."]);
    assert!(result.status.success(), "{}", text(&result));
    for path in ["keep.md", "other.md", "docs/other.md"] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), "# T\n\ntext   \n");
    }
    for path in ["docs/keep.md", "docs/clear/other.md"] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), "# T\n\ntext\n");
    }
    let explicit = run(
        root,
        &[
            "--config",
            ".markdownlint-cli2.jsonc",
            "check",
            "--no-cache",
            "docs/other.md",
        ],
    );
    assert!(explicit.status.success(), "{}", text(&explicit));
    let bypass = run(root, &["check", "--no-cache", "--no-exclude", "other.md"]);
    assert_eq!(bypass.status.code(), Some(1), "{}", text(&bypass));
}

#[test]
fn repeated_root_negations_prune_walks_but_keep_explicit_file_selection() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["!!docs/keep.md"]}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    write(root, "docs/keep.md", "# T\n\ntext   \n");
    let scan = run(root, &["check", "--no-cache", "."]);
    assert!(scan.status.success(), "{}", text(&scan));
    for target in ["docs", "docs/keep.md"] {
        let result = run(root, &["check", "--no-cache", target]);
        assert_eq!(result.status.code(), Some(1), "{}", text(&result));
        assert!(text(&result).contains("[MD009]"), "{}", text(&result));
    }
    let bypass = run(root, &["check", "--no-cache", "--no-exclude", "."]);
    assert_eq!(bypass.status.code(), Some(1), "{}", text(&bypass));
}

#[test]
fn negated_literal_directory_pruning_cannot_be_undone_by_a_nested_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true},"ignores":["!docs"]}"#,
    );
    write(root, "docs/.markdownlint-cli2.jsonc", r#"{"ignores":[]}"#);
    write(root, "docs/keep.md", "# T\n\ntext   \n");
    let scan = run(root, &["fmt", "--no-cache", "."]);
    assert!(scan.status.success(), "{}", text(&scan));
    assert_eq!(
        fs::read_to_string(root.join("docs/keep.md")).unwrap(),
        "# T\n\ntext   \n"
    );
    let named = run(root, &["fmt", "--no-cache", "docs/keep.md"]);
    assert!(named.status.success(), "{}", text(&named));
    assert_eq!(fs::read_to_string(root.join("docs/keep.md")).unwrap(), "# T\n\ntext\n");
}

#[cfg(unix)]
#[test]
fn symlink_parent_components_select_the_actual_directory_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(
        root,
        ".markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD009":true}}"#,
    );
    write(
        root,
        "actual/.markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD010":true}}"#,
    );
    fs::create_dir(root.join("actual/nested")).unwrap();
    std::os::unix::fs::symlink(root.join("actual/nested"), root.join("link")).unwrap();
    write(root, "file.md", "# T\n\ntext   \n");
    write(root, "actual/file.md", "# T\n\ntext\tend\n");
    let result = run(root, &["check", "--no-cache", "link/../file.md"]);
    assert_eq!(result.status.code(), Some(1), "{}", text(&result));
    assert!(text(&result).contains("[MD010]"), "{}", text(&result));
    let result = run(root, &["fmt", "--no-cache", "link/../file.md"]);
    assert!(result.status.success(), "{}", text(&result));
    assert!(!fs::read_to_string(root.join("actual/file.md")).unwrap().contains('\t'));
    assert_eq!(fs::read_to_string(root.join("file.md")).unwrap(), "# T\n\ntext   \n");
    write(
        root,
        "actual/.markdownlint-cli2.jsonc",
        r#"{"config":{"default":false,"MD010":true},"ignores":["file.md"]}"#,
    );
    write(root, "actual/file.md", "# T\n\ntext\tend\n");
    let result = run(root, &["fmt", "--no-cache", "link/../file.md"]);
    assert!(result.status.success(), "{}", text(&result));
    assert!(fs::read_to_string(root.join("actual/file.md")).unwrap().contains('\t'));
}

#[test]
fn markdownlint_table_column_style_alias_configures_and_disables_md060() {
    let table = "# T\n\n| a | longer header |\n|---|---|\n| xyz | 1 |\n";
    for (name, enabled, disabled) in [
        (
            ".markdownlint-cli2.yaml",
            "config:\n  default: false\n  table-column-style:\n    style: aligned\n",
            "config:\n  table-column-style: false\n",
        ),
        (
            ".markdownlint.json",
            r#"{"default":false,"table-column-style":{"style":"aligned"}}"#,
            r#"{"table-column-style":false}"#,
        ),
    ] {
        let temp = tempdir().unwrap();
        let root = temp.path();
        write(root, name, enabled);
        write(root, "t.md", table);
        let result = run(root, &["check", "--no-cache", "t.md"]);
        assert_eq!(result.status.code(), Some(1), "{}", text(&result));
        assert!(text(&result).contains("[MD060]"), "{}", text(&result));
        write(
            root,
            "xdg/rumdl/rumdl.toml",
            "[global]\nextend-enable = [\"MD060\"]\n[MD060]\nstyle = \"aligned\"\n",
        );
        // Confirm the platform user config is actually loaded before testing
        // that a project alias can disable its opt-in rule.
        fs::remove_file(root.join(name)).unwrap();
        let user_only = run(root, &["check", "--no-cache", "t.md"]);
        assert_eq!(user_only.status.code(), Some(1), "{}", text(&user_only));
        assert!(text(&user_only).contains("[MD060]"), "{}", text(&user_only));
        write(root, name, disabled);
        let result = run(root, &["check", "--no-cache", "t.md"]);
        assert!(result.status.success(), "{}", text(&result));
        let result = run(root, &["fmt", "--no-cache", "t.md"]);
        assert!(result.status.success(), "{}", text(&result));
        assert_eq!(fs::read_to_string(root.join("t.md")).unwrap(), table);
    }
}
