//! rumdl refuses a source-code file a run names, instead of linting it as
//! Markdown.
//!
//! A Swift `#if DEBUG` is an MD018 heading to a Markdown fixer and a KDoc ` * `
//! gutter is a list, so `fmt` over either rewrote the program and exited 0. A
//! file is named by a path argument, an `include` match, `--stdin-filename` or a
//! `--stdin-batch` path, and each of those is a tool error (exit 2) that leaves
//! the bytes alone. Files rumdl has always read as Markdown (`.txt`, no
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
fn a_cli_include_matching_source_files_is_refused() {
    let temp = project(&[("README.md", MARKDOWN), ("src/a.swift", SWIFT)]);
    let output = run(
        temp.path(),
        &["fmt", "--no-config", "--no-cache", "--include", "**/*.swift", "."],
        b"",
    );
    assert_refused(&output, "a.swift");
    assert_eq!(read(temp.path(), "src/a.swift"), SWIFT);
}

#[test]
fn a_config_include_matching_source_files_is_refused() {
    let temp = project(&[
        (".rumdl.toml", "[global]\ninclude = [\"**/*.swift\"]\n"),
        ("src/a.swift", SWIFT),
    ]);
    let output = run(temp.path(), &["fmt", "--no-cache"], b"");
    assert_refused(&output, "a.swift");
    assert_eq!(read(temp.path(), "src/a.swift"), SWIFT);
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
