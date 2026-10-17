//! rumdl refuses a source-code file a run names, instead of linting it as
//! Markdown.
//!
//! A Swift `#if DEBUG` is an MD018 heading to a Markdown fixer and a KDoc ` * `
//! gutter is a list, so `fmt` over either rewrote the program and exited 0. A
//! file is named by a path argument, `--stdin-filename` or a `--stdin-batch`
//! path, and each of those is a tool error (exit 2) that leaves the bytes
//! alone. A file an `include` wildcard happens to match names nothing, so it is
//! skipped with one warning instead. Files rumdl has always read as Markdown (`.txt`, no
//! extension, a `README.md.jinja` template) still are.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Swift whose directives `fmt` turned into `# if DEBUG` / `## endif`.
const SWIFT: &str = "#if DEBUG\nlet x  =  1\n#endif\n";

/// A KDoc comment whose gutter `fmt` turned into a Markdown list.
const KOTLIN: &str = "/**\n * Adds two numbers.\n * @param a first\n */\nfun add(a: Int, b: Int) = a + b\n";

/// Two blank-line runs too many: MD012 fires twice, and `fmt` rewrites it.
const MARKDOWN: &str = "# Title\n\na\n\n\n\nb\n";

const REFUSED: &str = "is source code, not Markdown";

fn run(cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input)
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to collect rumdl output")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn describe(output: &Output) -> String {
    format!(
        "exit: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr)
    )
}

/// A project anchored by a marker, holding the given files.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    for (name, content) in files {
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    temp
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

fn assert_refused(output: &Output, name: &str) {
    assert_eq!(output.status.code(), Some(2), "{}", describe(output));
    let stderr = text(&output.stderr);
    assert!(stderr.contains(&format!("{name} {REFUSED}")), "{}", describe(output));
}

#[test]
fn fmt_leaves_a_named_swift_file_unchanged() {
    let temp = project(&[("a.swift", SWIFT)]);
    let output = run(temp.path(), &["fmt", "--no-config", "--no-cache", "a.swift"], b"");
    assert_refused(&output, "a.swift");
    assert_eq!(read(temp.path(), "a.swift"), SWIFT);
}

#[test]
fn check_fix_leaves_a_named_kotlin_file_unchanged() {
    let temp = project(&[("b.kt", KOTLIN)]);
    let output = run(
        temp.path(),
        &["check", "--fix", "--no-config", "--no-cache", "b.kt"],
        b"",
    );
    assert_refused(&output, "b.kt");
    assert_eq!(read(temp.path(), "b.kt"), KOTLIN);
}

#[test]
fn check_reports_nothing_about_the_code_of_a_named_source_file() {
    let temp = project(&[("a.swift", SWIFT)]);
    let output = run(temp.path(), &["check", "--no-config", "--no-cache", "a.swift"], b"");
    assert_refused(&output, "a.swift");
    assert!(!text(&output.stdout).contains("MD018"), "{}", describe(&output));
}

#[test]
fn an_uppercase_rs_extension_is_refused_not_read_for_doc_comments() {
    // rustc and cargo only build `.rs`, so `LIB.RS` is no Rust file rumdl can
    // read doc comments from; read as Markdown, `#[derive]` is a heading.
    let source = "#[derive(Debug)]\nstruct S;\n";
    let temp = project(&[("LIB.RS", source)]);
    let output = run(temp.path(), &["fmt", "--no-config", "--no-cache", "LIB.RS"], b"");
    assert_refused(&output, "LIB.RS");
    assert_eq!(read(temp.path(), "LIB.RS"), source);
}

#[test]
fn a_run_naming_markdown_and_source_files_changes_neither() {
    let temp = project(&[("a.md", MARKDOWN), ("a.swift", SWIFT)]);
    let output = run(
        temp.path(),
        &["fmt", "--no-config", "--no-cache", "a.md", "a.swift"],
        b"",
    );
    assert_refused(&output, "a.swift");
    assert_eq!(read(temp.path(), "a.swift"), SWIFT);
    assert_eq!(read(temp.path(), "a.md"), MARKDOWN);
}

#[test]
fn a_directory_scan_passes_over_source_files() {
    let temp = project(&[("README.md", MARKDOWN), ("src/a.swift", SWIFT)]);
    let output = run(temp.path(), &["fmt", "--no-config", "--no-cache", "."], b"");
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(!text(&output.stderr).contains(REFUSED), "{}", describe(&output));
    assert_eq!(read(temp.path(), "src/a.swift"), SWIFT);
    assert_ne!(read(temp.path(), "README.md"), MARKDOWN);
}

#[test]
fn an_excluded_source_file_is_excluded_rather_than_refused() {
    let temp = project(&[
        (".rumdl.toml", "[global]\nexclude = [\"*.swift\"]\n"),
        ("a.swift", SWIFT),
    ]);
    let output = run(temp.path(), &["check", "--no-cache", "a.swift"], b"");
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(!text(&output.stderr).contains(REFUSED), "{}", describe(&output));
}

#[test]
fn files_rumdl_reads_as_markdown_are_still_fixed() {
    for name in [
        "notes.txt",
        "NOTES",
        "template.md.jinja",
        "rules.mdc",
        "intro.litcoffee",
    ] {
        let temp = project(&[(name, MARKDOWN)]);
        let output = run(temp.path(), &["fmt", "--no-config", "--no-cache", name], b"");
        assert_eq!(output.status.code(), Some(0), "{name}\n{}", describe(&output));
        assert_eq!(read(temp.path(), name), "# Title\n\na\n\nb\n", "{name}");
    }
}

#[test]
fn rust_files_are_still_read_through_their_doc_comments() {
    let source = "/// #Heading\n#[derive(Debug)]\nstruct S;\n";
    let temp = project(&[("lib.rs", source)]);
    let output = run(temp.path(), &["fmt", "--no-config", "--no-cache", "lib.rs"], b"");
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(
        read(temp.path(), "lib.rs"),
        "/// # Heading\n#[derive(Debug)]\nstruct S;\n"
    );
}

#[test]
fn fmt_over_stdin_named_as_source_hands_the_bytes_back() {
    let temp = project(&[]);
    let output = run(
        temp.path(),
        &["fmt", "--no-config", "--no-cache", "--stdin-filename", "a.swift", "-"],
        SWIFT.as_bytes(),
    );
    assert_refused(&output, "a.swift");
    assert_eq!(output.stdout, SWIFT.as_bytes());
}

#[test]
fn check_fix_over_stdin_named_as_source_hands_the_bytes_back() {
    let temp = project(&[]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--fix",
            "--no-config",
            "--no-cache",
            "--stdin-filename",
            "b.kt",
            "-",
        ],
        KOTLIN.as_bytes(),
    );
    assert_refused(&output, "b.kt");
    assert_eq!(output.stdout, KOTLIN.as_bytes());
}

#[test]
fn a_diff_over_stdin_named_as_source_prints_nothing() {
    let temp = project(&[]);
    for args in [
        &[
            "fmt",
            "--check",
            "--no-config",
            "--no-cache",
            "--stdin-filename",
            "a.swift",
            "-",
        ][..],
        &["check", "--no-config", "--no-cache", "--stdin-filename", "a.swift", "-"][..],
    ] {
        let output = run(temp.path(), args, SWIFT.as_bytes());
        assert_refused(&output, "a.swift");
        assert!(output.stdout.is_empty(), "{args:?}\n{}", describe(&output));
    }
}

#[test]
fn stdin_without_a_name_is_still_markdown() {
    let temp = project(&[]);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--enable", "MD018", "-"],
        SWIFT.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(text(&output.stdout).contains("MD018"), "{}", describe(&output));
}

#[test]
fn a_stdin_batch_supplying_a_source_file_is_refused() {
    let temp = project(&[]);
    let mut input = Vec::new();
    for (path, content) in [("a.md", MARKDOWN), ("a.swift", SWIFT)] {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
        input.extend_from_slice(content.as_bytes());
        input.push(0);
    }
    let output = run(
        temp.path(),
        &["check", "--stdin-batch", "--no-config", "--no-cache"],
        &input,
    );
    assert_refused(&output, "a.swift");
    assert!(!text(&output.stdout).contains("MD018"), "{}", describe(&output));
}

/// Clean Markdown, so the exit code of a run over it comes from nothing else.
const CLEAN: &str = "# Title\n\ntext\n";

const SKIPPED: &str = "Warning: skipped";

/// A tree of one Markdown file and the config and code files a wildcard sweeps up.
fn sweep_tree(markdown: &str) -> tempfile::TempDir {
    project(&[
        ("a.md", markdown),
        ("pyproject.toml", "[project]\nname = \"x\"\n"),
        ("x.py", "# comment\nx  =  1\n"),
    ])
}

fn skipped_lines(output: &Output) -> Vec<String> {
    text(&output.stderr)
        .lines()
        .filter(|line| line.contains(SKIPPED))
        .map(str::to_string)
        .collect()
}

fn assert_skipped_once(output: &Output, names: &[&str]) {
    let lines = skipped_lines(output);
    assert_eq!(lines.len(), 1, "{}", describe(output));
    for name in names {
        assert!(lines[0].contains(name), "{name}: {}", describe(output));
    }
    assert!(!text(&output.stderr).contains(REFUSED), "{}", describe(output));
}

#[test]
fn a_cli_include_wildcard_skips_source_files_with_one_warning() {
    for pattern in ["*", "**", "**/*.py"] {
        let temp = sweep_tree(CLEAN);
        let output = run(
            temp.path(),
            &["check", "--no-config", "--no-cache", "--include", pattern, "."],
            b"",
        );
        assert_eq!(output.status.code(), Some(0), "{pattern}: {}", describe(&output));
        assert_skipped_once(&output, &["x.py"]);
    }
}

#[test]
fn a_skipped_wildcard_match_does_not_hide_lint_findings() {
    let temp = sweep_tree(MARKDOWN);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "**", "."],
        b"",
    );
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(text(&output.stdout).contains("MD012"), "{}", describe(&output));
    assert_skipped_once(&output, &["pyproject.toml", "x.py"]);
}

#[test]
fn a_config_include_wildcard_skips_source_files_with_one_warning() {
    for pattern in ["*", "**", "**/*.swift"] {
        // Bare wildcards keep the Markdown extension filter, so nothing is
        // swept up to warn about; a pattern naming an extension selects the file.
        let temp = project(&[
            (".rumdl.toml", &format!("[global]\ninclude = [\"{pattern}\"]\n")),
            ("a.md", CLEAN),
            ("x.py", "# comment\n"),
            ("src/a.swift", SWIFT),
        ]);
        let output = run(temp.path(), &["check", "--no-cache"], b"");
        assert_eq!(output.status.code(), Some(0), "{pattern}: {}", describe(&output));
        if pattern == "**/*.swift" {
            assert_skipped_once(&output, &["a.swift"]);
        } else {
            assert!(skipped_lines(&output).is_empty(), "{pattern}: {}", describe(&output));
        }
    }
}

#[test]
fn a_file_named_explicitly_beside_a_wildcard_is_still_refused() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "**", "x.py"],
        b"",
    );
    assert_refused(&output, "x.py");
}

#[test]
fn a_named_source_file_is_refused_under_a_walk_that_also_matches_it() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "**", ".", "x.py"],
        b"",
    );
    assert_refused(&output, "x.py");
}

#[test]
fn an_include_naming_one_source_file_is_refused_like_a_path_argument() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "x.py", "."],
        b"",
    );
    assert_refused(&output, "x.py");
    let temp = project(&[(".rumdl.toml", "[global]\ninclude = [\"x.py\"]\n"), ("x.py", "x = 1\n")]);
    let output = run(temp.path(), &["check", "--no-cache"], b"");
    assert_refused(&output, "x.py");
}

#[test]
fn a_named_source_file_is_still_refused_without_a_wildcard() {
    let temp = sweep_tree(CLEAN);
    let output = run(temp.path(), &["check", "--no-config", "--no-cache", "x.py"], b"");
    assert_refused(&output, "x.py");
}

#[test]
fn stdin_named_as_source_is_not_a_wildcard_skip() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--include",
            "**",
            "--stdin",
            "--stdin-filename",
            "x.py",
        ],
        b"x  =  1\n",
    );
    assert!(skipped_lines(&output).is_empty(), "{}", describe(&output));
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
}

#[test]
fn fmt_over_a_wildcard_leaves_every_skipped_file_byte_identical() {
    let temp = sweep_tree(MARKDOWN);
    let before: Vec<_> = ["pyproject.toml", "x.py"]
        .iter()
        .map(|n| read(temp.path(), n))
        .collect();
    let output = run(
        temp.path(),
        &["fmt", "--no-config", "--no-cache", "--include", "**", "."],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["pyproject.toml", "x.py"]);
    let after: Vec<_> = ["pyproject.toml", "x.py"]
        .iter()
        .map(|n| read(temp.path(), n))
        .collect();
    assert_eq!(before, after);
    assert_ne!(read(temp.path(), "a.md"), MARKDOWN);
}

#[test]
fn a_markdown_only_tree_prints_no_skip_warning() {
    let temp = project(&[("a.md", CLEAN), ("docs/b.md", CLEAN), ("notes.txt", CLEAN)]);
    for args in [
        vec!["check", "--no-config", "--no-cache", "--include", "**", "."],
        vec!["check", "--no-config", "--no-cache", "."],
    ] {
        let output = run(temp.path(), &args, b"");
        assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
        assert!(skipped_lines(&output).is_empty(), "{}", describe(&output));
    }
}

#[test]
fn an_include_with_negation_and_no_config_file_skips_source_files() {
    let temp = project(&[
        ("docs/a.md", CLEAN),
        ("docs/tool.py", "x = 1\n"),
        ("docs/sub/b.md", MARKDOWN),
        ("docs/sub/tool.py", "y = 1\n"),
    ]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--include",
            "docs/**,!docs/sub/**",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["tool.py"]);
    assert!(!skipped_lines(&output)[0].contains("sub"), "{}", describe(&output));
}

#[test]
fn the_skip_warning_names_only_the_first_files_of_many() {
    let mut files: Vec<(String, &str)> = (0..40).map(|i| (format!("f{i:02}.py"), "x = 1\n")).collect();
    files.push(("a.md".to_string(), CLEAN));
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let temp = project(&borrowed);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "**", "."],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let lines = skipped_lines(&output);
    assert_eq!(lines.len(), 1, "{}", describe(&output));
    assert!(lines[0].contains("f00.py"), "{}", describe(&output));
    assert!(lines[0].contains("and 30 more"), "{}", describe(&output));
    assert!(!lines[0].contains("f39.py"), "{}", describe(&output));
}

#[test]
fn silent_suppresses_the_skip_warning_and_quiet_does_not() {
    let temp = sweep_tree(CLEAN);
    let silent = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--silent", "--include", "**", "."],
        b"",
    );
    assert_eq!(silent.status.code(), Some(0), "{}", describe(&silent));
    assert!(skipped_lines(&silent).is_empty(), "{}", describe(&silent));
    let quiet = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--quiet", "--include", "**", "."],
        b"",
    );
    assert_eq!(quiet.status.code(), Some(0), "{}", describe(&quiet));
    assert_skipped_once(&quiet, &["x.py"]);
}

#[test]
fn with_stderr_output_the_skip_warning_leaves_the_json_document_alone() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--stderr",
            "--output-format",
            "json",
            "--include",
            "**",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let stderr = text(&output.stderr);
    assert!(!stderr.contains(SKIPPED), "{}", describe(&output));
    serde_json::from_str::<serde_json::Value>(stderr.trim()).unwrap_or_else(|e| panic!("{e}\n{}", describe(&output)));
    assert!(text(&output.stdout).contains(SKIPPED), "{}", describe(&output));
}

#[test]
fn a_run_whose_wildcard_matches_only_source_files_exits_clean_even_when_denying_config_warnings() {
    let temp = project(&[("x.py", "x = 1\n")]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--deny-config-warnings",
            "--include",
            "**/*.py",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["x.py"]);
}

#[test]
fn a_cli2_ignore_silences_the_warning_for_the_files_it_covers() {
    let temp = sweep_tree(CLEAN);
    fs::write(
        temp.path().join(".markdownlint-cli2.jsonc"),
        r#"{"ignores": ["**/*.py"]}"#,
    )
    .unwrap();
    let output = run(temp.path(), &["check", "--no-cache", "--include", "**", "."], b"");
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["pyproject.toml"]);
    assert!(!skipped_lines(&output)[0].contains("x.py"), "{}", describe(&output));
}

#[test]
fn an_include_naming_a_file_with_an_exclamation_mark_is_refused() {
    let temp = project(&[("hello!.py", "x = 1\n"), ("a.md", CLEAN)]);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "hello!.py", "."],
        b"",
    );
    assert_refused(&output, "hello!.py");
}

#[test]
#[cfg(not(windows))]
fn an_escaped_include_naming_a_file_with_glob_characters_is_refused() {
    let temp = project(&[("[x].py", "x = 1\n"), ("a.md", CLEAN)]);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", r"\[x\].py", "."],
        b"",
    );
    assert_refused(&output, "[x].py");
}

#[test]
fn a_named_source_file_with_glob_characters_is_refused() {
    let temp = project(&[("[x].py", "x = 1\n"), ("a.md", CLEAN)]);
    let output = run(temp.path(), &["check", "--no-config", "--no-cache", "[x].py"], b"");
    assert_refused(&output, "[x].py");
}

#[test]
#[cfg(windows)]
fn native_separator_include_patterns_distinguish_wildcards_from_named_files() {
    let temp = project(&[("sub/a.py", "x = 1\n"), ("a.md", CLEAN)]);
    for pattern in [r"sub\*.py", r"sub\?.py"] {
        let output = run(
            temp.path(),
            &["check", "--no-config", "--no-cache", "--include", pattern, "."],
            b"",
        );
        assert_eq!(output.status.code(), Some(0), "{pattern}: {}", describe(&output));
        assert_skipped_once(&output, &["a.py"]);
        assert_eq!(read(temp.path(), "sub/a.py"), "x = 1\n");
    }
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", r"sub\a.py", "."],
        b"",
    );
    assert_refused(&output, "a.py");
}

#[cfg(unix)]
#[test]
fn an_ignored_alias_does_not_turn_its_targets_skip_into_a_refusal() {
    let temp = sweep_tree(CLEAN);
    fs::create_dir_all(temp.path().join("sub")).unwrap();
    std::os::unix::fs::symlink("../x.py", temp.path().join("sub/alias.py")).unwrap();
    fs::write(
        temp.path().join("sub/.markdownlint-cli2.jsonc"),
        r#"{"ignores": ["alias.py"]}"#,
    )
    .unwrap();
    let output = run(
        temp.path(),
        &["check", "--no-cache", "--include", "**", ".", "sub/alias.py"],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(!text(&output.stderr).contains(REFUSED), "{}", describe(&output));
}

#[test]
fn skipped_source_files_do_not_hide_markdown_removed_by_an_exclude() {
    let temp = sweep_tree(CLEAN);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--deny-config-warnings",
            "--include",
            "**",
            "--exclude",
            "*.md",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
    assert!(
        text(&output.stdout).contains("a.md") || text(&output.stderr).contains("filtered out"),
        "{}",
        describe(&output)
    );
}

#[test]
fn an_excluded_literal_include_is_reported_as_filtered_not_as_an_empty_tree() {
    let temp = project(&[("x.py", "x = 1\n")]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--deny-config-warnings",
            "--include",
            "x.py",
            "--exclude",
            "x.py",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
}

#[test]
fn an_include_naming_a_file_with_a_stray_closing_bracket_is_refused() {
    let temp = project(&[("foo].py", "x = 1\n"), ("a.md", CLEAN)]);
    let output = run(
        temp.path(),
        &["check", "--no-config", "--no-cache", "--include", "foo].py", "."],
        b"",
    );
    assert_refused(&output, "foo].py");
}

#[test]
fn editorconfig_settings_for_a_skipped_file_are_not_a_config_warning() {
    let temp = project(&[
        (".rumdl.toml", "[global]\neditorconfig = true\n"),
        (".editorconfig", "root = true\n\n[*.py]\nindent_style = tab\n"),
        ("a.md", CLEAN),
        ("x.py", "x = 1\n"),
    ]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "--include",
            "**",
            "--exclude",
            ".editorconfig",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["x.py"]);
    assert!(
        !text(&output.stderr).contains("config warning"),
        "{}",
        describe(&output)
    );
}

#[test]
fn a_broken_nested_config_over_only_skipped_files_is_not_a_config_warning() {
    let temp = project(&[
        ("a.md", CLEAN),
        ("src/x.py", "x = 1\n"),
        ("src/.rumdl.toml", "[global\nbroken"),
    ]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "--include",
            "**",
            "--exclude",
            "src/.rumdl.toml",
            ".",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_skipped_once(&output, &["x.py"]);
    assert!(
        !text(&output.stderr).contains("config warning"),
        "{}",
        describe(&output)
    );
}

#[cfg(unix)]
#[test]
fn editorconfig_warnings_follow_the_target_of_a_source_named_symlink_to_markdown() {
    let temp = project(&[
        (".rumdl.toml", "[global]\neditorconfig = true\n"),
        (".editorconfig", "root = true\n\n[*]\nindent_style = tab\n"),
        ("a.md", CLEAN),
    ]);
    std::os::unix::fs::symlink("a.md", temp.path().join("x.py")).unwrap();
    let direct = run(
        temp.path(),
        &["check", "--no-cache", "--deny-config-warnings", "a.md"],
        b"",
    );
    let alias = run(
        temp.path(),
        &["check", "--no-cache", "--deny-config-warnings", "x.py"],
        b"",
    );
    assert_eq!(direct.status.code(), Some(2), "{}", describe(&direct));
    assert_eq!(alias.status.code(), Some(2), "{}", describe(&alias));
}

#[test]
fn a_literal_include_is_refused_when_the_directory_is_spelled_absolutely() {
    let temp = sweep_tree(CLEAN);
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/x.py"), "x = 1\n").unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let output = run(
        &dir,
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--include",
            "**,src/x.py",
            dir.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
}

#[test]
fn an_include_matching_only_skipped_files_is_not_described_as_matching_no_file() {
    let temp = project(&[("a.md", CLEAN), ("x.swift", "let x = 1\n")]);
    let output = run(
        temp.path(),
        &[
            "check",
            "--no-config",
            "--no-cache",
            "--include",
            "**/*.swift,**/*.nothing",
            ".",
        ],
        b"",
    );
    let stderr = text(&output.stderr) + &text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(
        stderr.contains("include pattern '**/*.swift' matches only files that are not Markdown (skipped)"),
        "{}",
        describe(&output)
    );
    assert!(
        !stderr.contains("'**/*.swift' matches no file"),
        "{}",
        describe(&output)
    );
    assert!(
        stderr.contains("include pattern '**/*.nothing' matches no file"),
        "{}",
        describe(&output)
    );
}
