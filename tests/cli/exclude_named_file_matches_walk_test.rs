//! A file named on the command line, or read from stdin under the name it has on
//! disk, is excluded exactly when a directory walk would skip it.
//!
//! The walk applies `exclude` the way `.gitignore` reads a pattern, so a pattern
//! with no `/` matches at any depth, a pattern matching a directory removes its
//! contents, and `*` or `?` matches within one path component. Pre-commit hooks
//! name files and editors pipe them, so either route deciding differently lints
//! files the project excludes.
//!
//! Each case lints the same project three ways and compares the sets of files
//! that produced findings: a bare walk, every file named at once, and every file
//! supplied through `--stdin-batch`.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `MD012` fires on every file, so a file with no findings was excluded.
const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const FILES: &[&str] = &[
    "top.md",
    "docs/a.md",
    "docs/deep/a.md",
    "gen/x.md",
    "sub/gen/x.md",
    "sub/top.md",
    "sub/a/b.md",
];

/// Writes `FILES` into a project at `root` whose config file holds `config`.
fn write_project(root: &Path, config: &str) {
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".rumdl.toml"), config).unwrap();
    for file in FILES {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, DOCUMENT).unwrap();
    }
}

/// A config file excluding `patterns`.
fn exclude_config(patterns: &[&str]) -> String {
    // A TOML literal string holds any pattern without escaping.
    let patterns: Vec<String> = patterns.iter().map(|pattern| format!("'{pattern}'")).collect();
    format!("[global]\nexclude = [{}]\n", patterns.join(", "))
}

fn run(cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "--output-format", "concise"])
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

/// The files a run reported findings for, with `/` separators.
fn linted(output: &Output) -> BTreeSet<String> {
    let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");
    FILES
        .iter()
        .filter(|file| stdout.lines().any(|line| line.starts_with(&format!("{file}:"))))
        .map(|file| file.to_string())
        .collect()
}

/// Every route over the project at `root`, given `args`, lints exactly
/// `expected`. `case` names the exclude configuration in a failure.
fn assert_routes_lint_at(root: &Path, args: &[&str], expected: &[&str], case: &str) {
    let expected: BTreeSet<String> = expected.iter().map(|file| file.to_string()).collect();
    let walk = run(root, args, b"");
    assert_eq!(linted(&walk), expected, "walk under {case}");
    let named = run(root, &[args, FILES].concat(), b"");
    assert_eq!(linted(&named), expected, "named files under {case}");
    let batch_input: String = FILES.iter().map(|file| format!("{file}\0{DOCUMENT}\0")).collect();
    let batch = run(root, &[args, &["--stdin-batch"]].concat(), batch_input.as_bytes());
    assert_eq!(linted(&batch), expected, "--stdin-batch under {case}");
}

/// Every route lints exactly `expected` under `exclude = [pattern]`.
fn assert_routes_lint(pattern: &str, expected: &[&str]) {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), &exclude_config(&[pattern]));
    assert_routes_lint_at(temp.path(), &[], expected, &format!("exclude = [{pattern:?}]"));
}

/// Every route lints exactly `expected` under `--exclude pattern`.
fn assert_flag_routes_lint(pattern: &str, expected: &[&str]) {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), "");
    assert_routes_lint_at(
        temp.path(),
        &["--exclude", pattern],
        expected,
        &format!("--exclude {pattern:?}"),
    );
}

#[test]
fn a_directory_name_without_a_slash_excludes_that_directory_at_any_depth() {
    let kept = ["top.md", "docs/a.md", "docs/deep/a.md", "sub/top.md", "sub/a/b.md"];
    assert_routes_lint("gen", &kept);
    assert_routes_lint("gen/", &kept);
    assert_routes_lint(
        "deep",
        &[
            "top.md",
            "docs/a.md",
            "gen/x.md",
            "sub/gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
    assert_routes_lint("{gen,deep}", &["top.md", "docs/a.md", "sub/top.md", "sub/a/b.md"]);
}

#[test]
fn a_file_name_without_a_slash_excludes_that_file_at_any_depth() {
    assert_routes_lint(
        "x.md",
        &["top.md", "docs/a.md", "docs/deep/a.md", "sub/top.md", "sub/a/b.md"],
    );
    assert_routes_lint(
        "top.md",
        &["docs/a.md", "docs/deep/a.md", "gen/x.md", "sub/gen/x.md", "sub/a/b.md"],
    );
}

#[test]
fn a_wildcard_matching_a_directory_excludes_its_contents() {
    let kept = ["top.md", "docs/a.md", "docs/deep/a.md", "sub/top.md", "sub/a/b.md"];
    assert_routes_lint("g?n", &kept);
    assert_routes_lint("[g]en", &kept);
    assert_routes_lint(
        "sub/?en",
        &[
            "top.md",
            "docs/a.md",
            "docs/deep/a.md",
            "gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
    assert_routes_lint(
        "de*",
        &[
            "top.md",
            "docs/a.md",
            "gen/x.md",
            "sub/gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
}

#[test]
fn a_wildcard_matches_within_one_path_component() {
    for assert_lint in [assert_routes_lint as fn(&str, &[&str]), assert_flag_routes_lint] {
        assert_lint(
            "sub/*.md",
            &[
                "top.md",
                "docs/a.md",
                "docs/deep/a.md",
                "gen/x.md",
                "sub/gen/x.md",
                "sub/a/b.md",
            ],
        );
        assert_lint(
            "sub/?/b.md",
            &[
                "top.md",
                "docs/a.md",
                "docs/deep/a.md",
                "gen/x.md",
                "sub/gen/x.md",
                "sub/top.md",
            ],
        );
        // Each would reach `sub/a/b.md` if its wildcard spanned the `/`.
        assert_lint("s*/b.md", FILES);
        assert_lint("sub?a/b.md", FILES);
    }
}

#[test]
fn a_pattern_containing_a_slash_stays_anchored_at_the_project_root() {
    assert_routes_lint(
        "sub/gen",
        &[
            "top.md",
            "docs/a.md",
            "docs/deep/a.md",
            "gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
    assert_routes_lint(
        "a/b.md",
        &[
            "top.md",
            "docs/a.md",
            "docs/deep/a.md",
            "gen/x.md",
            "sub/gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
    assert_routes_lint(
        "docs/deep",
        &[
            "top.md",
            "docs/a.md",
            "gen/x.md",
            "sub/gen/x.md",
            "sub/top.md",
            "sub/a/b.md",
        ],
    );
    assert_routes_lint(
        "docs",
        &["top.md", "gen/x.md", "sub/gen/x.md", "sub/top.md", "sub/a/b.md"],
    );
}

#[test]
fn a_relative_pattern_never_matches_a_directory_above_the_project() {
    // The project sits in a directory named `docs`. The absolute pattern makes
    // each file's absolute path worth matching, and the floating `docs` must not
    // match the directory holding the project.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("docs");
    let kept = ["top.md", "gen/x.md", "sub/gen/x.md", "sub/top.md", "sub/a/b.md"];

    write_project(&root, &exclude_config(&["docs", "/unrelated"]));
    assert_routes_lint_at(&root, &[], &kept, "exclude = [\"docs\", \"/unrelated\"]");

    fs::write(root.join(".rumdl.toml"), "").unwrap();
    assert_routes_lint_at(
        &root,
        &["--exclude", "docs,/unrelated"],
        &kept,
        "--exclude \"docs,/unrelated\"",
    );
}
