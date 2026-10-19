use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn has_format_cache(root: &Path) -> bool {
    let mut directories = vec![root.join("cache")];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
            } else if entry.file_name().to_string_lossy().starts_with("format-") {
                return true;
            }
        }
    }
    false
}

fn assert_no_format_cache(root: &Path) {
    assert!(!has_format_cache(root));
}

#[test]
fn silent_format_keeps_native_urls_in_reference_continuations_and_empty_labels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    let options = [
        "--no-config",
        "--no-cache",
        "--respect-gitignore=false",
        "--flavor=standard",
        "--enable=MD034",
        "a.md",
    ];
    for (atom, replacement) in [
        ("https://example.com", "<https://example.com>"),
        ("name@example.com", "<name@example.com>"),
        ("www.example.com", "<https://www.example.com>"),
    ] {
        for source in [
            format!("[x]:\n  {atom}\n"),
            format!("[x]: https://example.org\n  \"{atom}\"\n"),
            format!("![{atom}][r]\n\n[r]: <>\n"),
            format!("[{atom}][r]\n\n[r]: <>\n"),
        ] {
            let expected = source.replace(atom, replacement);
            fs::write(&path, &source).unwrap();
            let checked = run(dir.path(), &[&["fmt", "--check", "--silent"][..], &options].concat());
            assert_eq!(checked.status.code(), Some(1), "{source:?}: {checked:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), source);

            for mode in [
                &["fmt", "--silent"][..],
                &["fmt"][..],
                &["check", "--fix", "--silent"][..],
            ] {
                fs::write(&path, &source).unwrap();
                let output = run(dir.path(), &[mode, &options].concat());
                assert_eq!(output.status.code(), Some(0), "{source:?}, {mode:?}: {output:?}");
                assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{source:?}, {mode:?}");
            }

            let stable = run(dir.path(), &[&["fmt", "--check", "--silent"][..], &options].concat());
            assert_eq!(stable.status.code(), Some(0), "{source:?}: {stable:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        }
    }
}

#[test]
fn silent_format_keeps_native_list_ownership_around_display_math() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    let source = "1. a\n\n$$x\n- root\n  see `$$`\n  more\n";
    let expected = "1. a\n\n$$x\n- root\n   see `$$`\n   more\n";
    let options = [
        "--no-config",
        "--no-cache",
        "--flavor=standard",
        "--enable=MD077",
        "a.md",
    ];

    fs::write(&path, source).unwrap();
    let checked = run(dir.path(), &[&["fmt", "--check", "--silent"][..], &options].concat());
    assert_eq!(checked.status.code(), Some(1), "{checked:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);

    for mode in [
        &["fmt", "--silent"][..],
        &["fmt"][..],
        &["check", "--fix", "--silent"][..],
    ] {
        fs::write(&path, source).unwrap();
        let output = run(dir.path(), &[mode, &options].concat());
        assert_eq!(output.status.code(), Some(0), "{mode:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{mode:?}");
    }

    let stable = run(dir.path(), &[&["fmt", "--check", "--silent"][..], &options].concat());
    assert_eq!(stable.status.code(), Some(0), "{stable:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn silent_format_reuses_a_complete_clean_check_without_running_fixes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    fs::write(&path, "# Title\n\nPlain text.\n").unwrap();
    let checked = run(
        dir.path(),
        &["check", "--no-config", "--silent", "--cache-dir", "cache", "a.md"],
    );
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "fmt",
            "--check",
            "--no-config",
            "--silent",
            "--cache-dir",
            "cache",
            "a.md",
        ])
        .output()
        .unwrap();
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_no_format_cache(dir.path());
    assert_eq!(fs::read_to_string(&path).unwrap(), "# Title\n\nPlain text.\n");

    let dirty = "#Title\n\nPlain text.\n";
    fs::write(&path, dirty).unwrap();
    let formatted = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "fmt",
            "--check",
            "--no-config",
            "--silent",
            "--cache-dir",
            "cache",
            "a.md",
        ])
        .output()
        .unwrap();
    assert_eq!(formatted.status.code(), Some(1), "{formatted:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), dirty);
}

#[test]
fn silent_format_reuses_identical_clean_files_with_the_same_settings() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.md", "b.md", "c.md", "d.md"] {
        fs::write(dir.path().join(name), "# Title\n\nPlain text.\n").unwrap();
    }
    let checked = run(
        dir.path(),
        &["check", "--no-config", "--silent", "--cache-dir", "cache", "."],
    );
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["fmt", "--check", "--no-config", "--silent", "--cache-dir", "cache", "."])
        .output()
        .unwrap();
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_no_format_cache(dir.path());
}

#[test]
fn silent_format_does_not_import_another_files_flavor() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[per-file-flavor]\n\"ignored.md\" = \"mkdocs\"\n\"target.md\" = \"standard\"\n\n[MD046]\nstyle = \"fenced\"\n",
    )
    .unwrap();
    for name in ["ignored.md", "target.md"] {
        fs::write(dir.path().join(name), "# T\n\n!!! note\n\n    indented content\n").unwrap();
    }
    let options = ["--silent", "--cache-dir=cache", "--enable=MD046"];
    let checked = run(dir.path(), &[&["check"][..], &options, &["ignored.md"]].concat());
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = run(dir.path(), &[&["fmt"][..], &options, &["target.md"]].concat());
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("target.md")).unwrap(),
        "# T\n\n!!! note\n\n```\nindented content\n```\n"
    );
}

#[test]
fn silent_format_does_not_import_another_files_detected_flavor() {
    let dir = tempfile::tempdir().unwrap();
    let source = "# T\n\n{/*\n\n    indented content\n\n*/}\n";
    for name in ["ignored.mdx", "target.md"] {
        fs::write(dir.path().join(name), source).unwrap();
    }
    let options = [
        "--no-config",
        "--silent",
        "--cache-dir=cache",
        "--enable=MD046",
        "--config=MD046.style=\"fenced\"",
    ];
    let checked = run(dir.path(), &[&["check"][..], &options, &["ignored.mdx"]].concat());
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = run(
        dir.path(),
        &[&["fmt", "--check"][..], &options, &["target.md"]].concat(),
    );
    assert_eq!(formatted.status.code(), Some(1), "{formatted:?}");
    assert_eq!(fs::read_to_string(dir.path().join("target.md")).unwrap(), source);
}

#[test]
fn silent_format_does_not_import_another_files_ignored_check() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ignored.md"), "*word*\n").unwrap();
    let target = dir.path().join("target.md");
    fs::write(&target, "*word*\n").unwrap();
    let options = [
        "--no-config",
        "--silent",
        "--cache-dir=cache",
        "--enable=MD049",
        "--config=MD049.style=\"underscore\"",
        "--config=per-file-ignores={\"ignored.md\"=[\"MD049\"]}",
    ];
    let checked = run(dir.path(), &[&["check"][..], &options, &["ignored.md"]].concat());
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = run(dir.path(), &[&["fmt"][..], &options, &["target.md"]].concat());
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_eq!(fs::read_to_string(target).unwrap(), "_word_\n");
}

#[test]
fn silent_format_matches_diagnostic_format_bytes_and_check_status() {
    let dir = tempfile::tempdir().unwrap();
    let original = "<!-- rumdl-configure-file {\"MD049\":{\"style\":\"underscore\"}} -->\r\n#Title\r\n\r\n*word* é🦀\r\n\r\n<!-- rumdl-disable-next-line MD037 -->\r\n* spaced *\r\n\r\n[missing](missing.md)\r\n";
    for flavor in [
        "standard",
        "mkdocs",
        "mdx",
        "kramdown",
        "pandoc",
        "quarto",
        "obsidian",
        "azure_devops",
        "myst",
        "hugo",
        "mdg",
        "gh-aw",
    ] {
        let mut expected = None;
        for silent in [false, true] {
            fs::write(dir.path().join("a.md"), original).unwrap();
            let mut args = vec!["fmt", "--no-config", "--no-cache", "--flavor", flavor, "a.md"];
            if silent {
                args.push("--silent");
            }
            let output = run(dir.path(), &args);
            assert_eq!(output.status.code(), Some(0), "{flavor}: {output:?}");
            let bytes = fs::read(dir.path().join("a.md")).unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&bytes, expected, "{flavor}");
            } else {
                expected = Some(bytes);
            }
            if silent {
                assert!(output.stdout.is_empty());
                assert!(output.stderr.is_empty());
            }
        }
        fs::write(dir.path().join("a.md"), original).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--no-config",
                "--no-cache",
                "--silent",
                "--check",
                "--flavor",
                flavor,
                "a.md",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{flavor}: {output:?}");
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), original);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        fs::write(dir.path().join("a.md"), expected.unwrap()).unwrap();
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--no-config",
                "--no-cache",
                "--silent",
                "--check",
                "--flavor",
                flavor,
                "a.md",
            ],
        );
        assert_eq!(output.status.code(), Some(0), "{flavor}: {output:?}");
    }
}

#[test]
fn silent_format_retains_configuration_and_read_errors() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "<!-- rumdl-disable MD999 -->\n#Title\n").unwrap();
    for args in [
        vec![
            "fmt",
            "--no-config",
            "--no-cache",
            "--silent",
            "--check",
            "--deny-config-warnings",
            "a.md",
        ],
        vec!["fmt", "--no-config", "--no-cache", "--silent", "missing.md"],
    ] {
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn silent_format_keeps_encoding_and_conflict_guards() {
    let dir = tempfile::tempdir().unwrap();
    for original in [
        b"#Title\n\nbad\xfftext\n".as_slice(),
        b"#Title\n\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n",
        b"\x00\xff\x00",
    ] {
        fs::write(dir.path().join("a.md"), original).unwrap();
        let output = run(dir.path(), &["fmt", "--no-config", "--no-cache", "--silent", "a.md"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), original);
    }
}

#[test]
fn silent_format_does_not_cache_an_empty_diagnostic_report() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "#Title\n\n[missing](missing.md)\n").unwrap();
    let output = run(
        dir.path(),
        &["fmt", "--no-config", "--silent", "--cache-dir", "cache", "a.md"],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for _ in 0..2 {
        let output = run(
            dir.path(),
            &[
                "check",
                "--no-config",
                "--output-format=json",
                "--cache-dir",
                "cache",
                "a.md",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let warnings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            warnings.as_array().unwrap().iter().any(|w| w["rule"] == "MD057"),
            "{warnings}"
        );
    }
}

#[test]
fn silent_format_preserves_exclusion_order_and_kramdown_report_gate() {
    let dir = tempfile::tempdir().unwrap();
    for (original, options) in [
        (
            "{::nomarkdown}\r\n[ref]\r\n- item\r\n  continuation\r\nexport const x = \"_a_\";\r\n</script>\r\n  continuation\r\n:::",
            vec!["--flavor=kramdown"],
        ),
        (
            "[ref]: https://example.com\n- item\nexport const x = \"_a_\";\n   \n:::\n<script>\n{#attribute}\nexport const x = \"_a_\";\n```md\n@ : www.\n@ : www.\n",
            vec!["--config=per-file-ignores={\"*.md\"=[\"MD018\",\"MD013\"]}"],
        ),
    ] {
        let mut expected = None;
        for silent in [false, true] {
            fs::write(dir.path().join("a.md"), original).unwrap();
            let mut args = vec!["fmt", "--no-config", "--no-cache", "a.md"];
            args.extend(&options);
            if silent {
                args.push("--silent");
            }
            let output = run(dir.path(), &args);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            let bytes = fs::read(dir.path().join("a.md")).unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&bytes, expected);
            } else {
                expected = Some(bytes);
            }
        }
    }
}

#[test]
fn silent_format_cache_invalidates_for_text_settings_and_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    fs::write(&path, "*word*\n").unwrap();
    for _ in 0..2 {
        let output = run(
            dir.path(),
            &[
                "fmt",
                "--no-config",
                "--silent",
                "--cache-dir=cache",
                "--enable=MD049",
                "--config=MD049.style=\"asterisk\"",
                "a.md",
            ],
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "*word*\n");
    }
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--no-config",
            "--silent",
            "--cache-dir=cache",
            "--enable=MD049",
            "--config=MD049.style=\"underscore\"",
            "a.md",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "_word_\n");
    fs::write(&path, "_word_").unwrap();
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--no-config",
            "--silent",
            "--cache-dir=cache",
            "--enable=MD049",
            "--config=MD049.style=\"underscore\"",
            "a.md",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--no-config",
            "--silent",
            "--cache-dir=cache",
            "--enable=MD047",
            "a.md",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "_word_\n");
}

#[test]
fn silent_format_does_not_import_another_formats_flavor() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[per-file-flavor]\n\"ignored.md\" = \"mkdocs\"\n\"target.md\" = \"standard\"\n\n[MD046]\nstyle = \"fenced\"\n",
    )
    .unwrap();
    for name in ["ignored.md", "target.md"] {
        fs::write(dir.path().join(name), "# T\n\n!!! note\n\n    indented content\n").unwrap();
    }
    let options = ["--silent", "--cache-dir=cache", "--enable=MD046"];
    let checked = run(
        dir.path(),
        &[&["fmt", "--check"][..], &options, &["ignored.md"]].concat(),
    );
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = run(dir.path(), &[&["fmt"][..], &options, &["target.md"]].concat());
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("target.md")).unwrap(),
        "# T\n\n!!! note\n\n```\nindented content\n```\n"
    );
}

#[test]
fn silent_format_does_not_import_another_formats_ignored_rules() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ignored.md"), "*word*\n").unwrap();
    let target = dir.path().join("target.md");
    fs::write(&target, "*word*\n").unwrap();
    let options = [
        "--no-config",
        "--silent",
        "--cache-dir=cache",
        "--enable=MD049",
        "--config=MD049.style=\"underscore\"",
        "--config=per-file-ignores={\"ignored.md\"=[\"MD049\"]}",
    ];
    let checked = run(
        dir.path(),
        &[&["fmt", "--check"][..], &options, &["ignored.md"]].concat(),
    );
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    let formatted = run(dir.path(), &[&["fmt"][..], &options, &["target.md"]].concat());
    assert_eq!(formatted.status.code(), Some(0), "{formatted:?}");
    assert_eq!(fs::read_to_string(target).unwrap(), "_word_\n");
}

#[test]
fn silent_diff_keeps_stdout_empty_and_preserves_native_status() {
    let dir = tempfile::tempdir().unwrap();
    for (command, check, status) in [("check", false, 1), ("fmt", false, 0), ("fmt", true, 1)] {
        let source = "#Title\n";
        fs::write(dir.path().join("a.md"), source).unwrap();
        let mut args = vec![command, "--diff", "--silent", "--no-config", "--no-cache", "a.md"];
        if check {
            args.push("--check");
        }
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(status), "{command}: {output:?}");
        assert!(output.stdout.is_empty(), "{command}: {output:?}");
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), source);
    }
}

#[test]
fn cached_silent_format_preserves_embedded_markdown_work() {
    let source = "# Title\n\n```markdown\n#Inner\n```\n";
    for configuration in [
        "[code-block-tools]\nenabled = false\n",
        "[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nformat = [\"rumdl:format\"]\n",
        "[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n",
    ] {
        for check in [false, true] {
            for prime_check in [false, true] {
                let mut expected = None;
                for cached in [false, true] {
                    let dir = tempfile::tempdir().unwrap();
                    fs::write(dir.path().join(".rumdl.toml"), configuration).unwrap();
                    let path = dir.path().join("a.md");
                    fs::write(&path, source).unwrap();
                    let cache_option = if cached { "--cache-dir=cache" } else { "--no-cache" };
                    if prime_check {
                        let output = run(
                            dir.path(),
                            &["check", "--silent", "--enable=MD018", cache_option, "a.md"],
                        );
                        assert!(matches!(output.status.code(), Some(0 | 1)), "{output:?}");
                    }
                    for _ in 0..2 {
                        fs::write(&path, source).unwrap();
                        let mut args = vec!["fmt", "--silent", "--enable=MD018", cache_option, "a.md"];
                        if check {
                            args.push("--check");
                        }
                        let output = run(dir.path(), &args);
                        let bytes = fs::read(&path).unwrap();
                        if configuration.contains("enabled = false") {
                            assert_eq!(output.status.code(), Some(0), "{output:?}");
                            assert_eq!(bytes, source.as_bytes());
                            if cached && !prime_check {
                                assert!(has_format_cache(dir.path()));
                            }
                        } else if check {
                            assert_eq!(output.status.code(), Some(1), "{output:?}");
                            assert_eq!(bytes, source.as_bytes());
                        } else {
                            assert_eq!(output.status.code(), Some(0), "{output:?}");
                            assert_eq!(bytes, b"# Title\n\n```markdown\n# Inner\n```\n");
                        }
                        let actual = (
                            output.status.code(),
                            output.stdout,
                            output.stderr,
                            fs::read(&path).unwrap(),
                        );
                        if let Some(expected) = &expected {
                            assert_eq!(
                                &actual, expected,
                                "{configuration} check={check} primed={prime_check} cached={cached}"
                            );
                        } else {
                            expected = Some(actual);
                        }
                    }
                }
            }
        }
    }
}
