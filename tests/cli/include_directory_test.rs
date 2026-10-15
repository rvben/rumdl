//! An `include` pattern that names a directory selects the Markdown files
//! beneath it, the way `exclude` on the same spelling removes them.
//!
//! The oracle is the set of files `rumdl check` actually lints. Every file
//! carries an `MD012` finding, so a file without one was not selected.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const FILES: &[&str] = &[
    "dir/a.md",
    "dir/sub/b.md",
    "dir/notes.txt",
    "other/dir/c.md",
    "other/e.md",
    "top.md",
    ".git/config",
    ".git/hooks/pre-commit.sample",
];

fn project(config_name: &str, config: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(config_name), config).unwrap();
    for file in FILES {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, DOCUMENT).unwrap();
    }
    temp
}

/// A project with no configuration file: a run that selects every file name
/// would otherwise meet `.rumdl.toml` and refuse it as source code.
fn bare_project() -> tempfile::TempDir {
    let temp = config_project("");
    fs::remove_file(temp.path().join(".rumdl.toml")).unwrap();
    temp
}

fn config_project(config: &str) -> tempfile::TempDir {
    project(".rumdl.toml", config)
}

fn canonical(root: &Path) -> String {
    root.canonicalize().unwrap().to_string_lossy().replace('\\', "/")
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "--output-format", "concise"])
        .args(args)
        .output()
        .expect("failed to execute rumdl")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .replace('\\', "/")
}

/// The project files with at least one `MD012` finding.
fn flagged(output: &Output) -> BTreeSet<String> {
    let stdout = text(output);
    FILES
        .iter()
        .filter(|file| {
            stdout
                .lines()
                .any(|line| line.trim_start_matches("./").starts_with(&format!("{file}:")) && line.contains("MD012"))
        })
        .map(|file| file.to_string())
        .collect()
}

fn only(files: &[&str]) -> BTreeSet<String> {
    files.iter().map(|file| file.to_string()).collect()
}

fn toml_include(pattern: &str) -> String {
    format!("[global]\ninclude = [{pattern:?}]\n")
}

const ALL_MARKDOWN: [&str; 5] = ["dir/a.md", "dir/sub/b.md", "other/dir/c.md", "other/e.md", "top.md"];

const DIR_FILES: &[&str] = &["dir/a.md", "dir/sub/b.md"];
const FLOATING: &[&str] = &["dir/a.md", "dir/sub/b.md", "other/dir/c.md"];

#[test]
fn a_bare_directory_name_selects_the_directory_wherever_it_is() {
    for pattern in ["dir", "dir/"] {
        let temp = config_project(&toml_include(pattern));
        let output = run(temp.path(), &["."]);
        assert_eq!(
            flagged(&output),
            only(FLOATING),
            "include = [{pattern:?}]\n{}",
            text(&output)
        );
    }
}

#[test]
fn an_anchored_directory_selects_only_the_one_at_the_root() {
    let temp = config_project("");
    let absolute = format!("{}/dir", canonical(temp.path()));
    for pattern in ["./dir", "./dir/", "dir/sub/..", "./other/dir", absolute.as_str()] {
        fs::write(temp.path().join(".rumdl.toml"), toml_include(pattern)).unwrap();
        let output = run(temp.path(), &["."]);
        let expected: &[&str] = if pattern.contains("other") {
            &["other/dir/c.md"]
        } else {
            DIR_FILES
        };
        assert_eq!(
            flagged(&output),
            only(expected),
            "include = [{pattern:?}]\n{}",
            text(&output)
        );
    }
}

#[test]
fn the_command_line_names_a_directory_the_same_way() {
    let temp = config_project("");
    for (pattern, expected) in [("dir", FLOATING), ("./dir", DIR_FILES)] {
        let output = run(temp.path(), &["--include", pattern, "."]);
        assert_eq!(
            flagged(&output),
            only(expected),
            "--include {pattern}\n{}",
            text(&output)
        );
    }
}

#[test]
fn the_command_line_widens_to_other_types_only_for_a_pattern_that_names_files() {
    let temp = config_project("");
    for pattern in ["dir/**", "dir/notes.txt", "*.txt"] {
        let output = run(temp.path(), &["--include", pattern, "."]);
        assert!(
            flagged(&output).contains("dir/notes.txt"),
            "--include {pattern}\n{}",
            text(&output)
        );
    }
}

#[test]
fn a_directory_include_in_pyproject_selects_its_markdown() {
    let temp = project("pyproject.toml", "[tool.rumdl]\ninclude = [\"./dir\"]\n");
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(DIR_FILES), "{}", text(&output));
}

#[test]
fn a_directory_include_keeps_the_markdown_only_filter() {
    let temp = config_project(&toml_include("dir"));
    let output = run(temp.path(), &["--verbose", "."]);
    assert!(!text(&output).contains("notes.txt"), "{}", text(&output));
}

#[test]
fn a_directory_include_matches_the_wildcard_spelling() {
    let by_name = config_project(&toml_include("./dir"));
    let by_glob = config_project(&toml_include("./dir/**"));
    assert_eq!(
        flagged(&run(by_name.path(), &["."])),
        flagged(&run(by_glob.path(), &["."]))
    );
}

#[test]
fn an_exclude_still_wins_over_an_included_directory() {
    let temp = config_project("[global]\ninclude = [\"dir\"]\nexclude = [\"dir/sub\"]\n");
    let output = run(temp.path(), &["."]);
    assert_eq!(
        flagged(&output),
        only(&["dir/a.md", "other/dir/c.md"]),
        "{}",
        text(&output)
    );
}

#[test]
fn a_name_that_is_a_file_still_selects_that_file() {
    let temp = config_project(&toml_include("top.md"));
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(&["top.md"]), "{}", text(&output));
}

#[test]
fn a_directory_that_does_not_exist_is_reported_when_nothing_is_selected() {
    let temp = config_project(&toml_include("nothing"));
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(&[]));
    assert!(
        text(&output).contains("include pattern 'nothing' matches no file"),
        "{}",
        text(&output)
    );
}

#[test]
fn a_comment_pattern_is_not_turned_into_a_directory() {
    let temp = config_project("[global]\ninclude = [\"./dir\", \"#other\"]\n");
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(DIR_FILES), "{}", text(&output));
}

#[test]
fn a_brace_alternation_final_component_is_a_glob_not_a_directory() {
    let temp = config_project(&toml_include("dir/{sub,zz}"));
    let output = run(temp.path(), &["."]);
    assert!(flagged(&output).is_empty(), "{}", text(&output));
}

#[test]
fn a_command_line_with_only_negations_selects_what_no_include_selects_minus_the_negated() {
    let temp = bare_project();
    let everything = flagged(&run(temp.path(), &["."]));
    assert_eq!(everything, only(&ALL_MARKDOWN), "no include");
    for (include, expected) in [
        ("!other/**", &["dir/a.md", "dir/sub/b.md", "top.md"][..]),
        ("!other/**,!dir/sub/**", &["dir/a.md", "top.md"][..]),
        ("!nothing/**", &ALL_MARKDOWN[..]),
    ] {
        let output = run(temp.path(), &["--include", include, "."]);
        assert_eq!(
            flagged(&output),
            only(expected),
            "--include {include}\n{}",
            text(&output)
        );
    }
}

#[test]
fn an_include_that_names_files_never_walks_a_version_control_directory() {
    let temp = bare_project();
    for pattern in ["*", "**", "**/*", "**/*.md", ".git/**", "dir/**,.git/**"] {
        let output = run(temp.path(), &["--include", pattern, "."]);
        let reached: Vec<_> = flagged(&output)
            .into_iter()
            .filter(|file| file.starts_with(".git/"))
            .collect();
        assert!(
            reached.is_empty(),
            "--include {pattern} reached {reached:?}\n{}",
            text(&output)
        );
    }
    let output = run(temp.path(), &["--include", "*", "."]);
    assert!(flagged(&output).contains("dir/notes.txt"), "{}", text(&output));
}

#[test]
fn a_version_control_directory_named_as_the_root_is_still_walked() {
    let temp = bare_project();
    let output = run(temp.path(), &["--include", "*", ".git"]);
    assert!(flagged(&output).contains(".git/config"), "{}", text(&output));
}

#[test]
fn a_blank_pattern_restricts_nothing() {
    for blank in ["", " ", "\\t"] {
        let temp = config_project(&format!("[global]\ninclude = [\"{blank}\"]\n"));
        let output = run(temp.path(), &["."]);
        let everything: BTreeSet<String> = ["dir/a.md", "dir/sub/b.md", "other/dir/c.md", "other/e.md", "top.md"]
            .iter()
            .map(|file| file.to_string())
            .collect();
        assert_eq!(flagged(&output), everything, "{blank:?}: {}", text(&output));
    }
}

#[test]
fn a_gitignored_directory_named_by_an_include_is_linted() {
    let temp = config_project(&toml_include("./dir"));
    fs::write(temp.path().join(".gitignore"), "dir/\n").unwrap();
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(DIR_FILES), "{}", text(&output));
}

#[test]
fn an_empty_run_blames_the_exclude_for_files_in_a_directory_an_include_rescued() {
    let temp = config_project("[global]\ninclude = [\"./dir\"]\nexclude = [\"**/*.md\"]\n");
    fs::write(temp.path().join(".gitignore"), "dir/\n").unwrap();
    let output = run(temp.path(), &["."]);
    let report = text(&output);
    assert!(report.contains("by exclude"), "{report}");
    assert!(!report.contains("by ignore files"), "{report}");
    assert!(!report.contains("matches no file"), "{report}");
}

#[test]
fn a_trailing_slash_keeps_the_pattern_to_directories() {
    // `top.md` is a file, so `top.md/` names nothing, as it does in a gitignore.
    let temp = config_project(&toml_include("top.md/"));
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), only(&[]), "{}", text(&output));
}
