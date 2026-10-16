//! `.` and `..` components in `include`, `exclude` and `per-file-*` patterns name
//! the location they resolve to, and a pattern written with them selects the
//! same files as the plain spelling.
//!
//! The oracle is the set of files `rumdl check` actually lints. Every file
//! carries an `MD012` finding, so a file without one was excluded or had the
//! rule ignored.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const FILES: &[&str] = &[
    "dir/a.md",
    "dir/sub/b.md",
    "other/dir/c.md",
    "a/b/d.md",
    "a/dir/e.md",
    "top.md",
    "other/top.md",
];

fn project(config: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".rumdl.toml"), config).unwrap();
    for file in FILES {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, DOCUMENT).unwrap();
    }
    temp
}

/// The canonical project path, in the `/`-separated form a pattern is written in.
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

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace('\\', "/")
}

/// The project files with at least one `MD012` finding.
fn flagged(output: &Output) -> BTreeSet<String> {
    let stdout = stdout_of(output);
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

fn all_but(excluded: &[&str]) -> BTreeSet<String> {
    FILES
        .iter()
        .filter(|file| !excluded.contains(file))
        .map(|file| file.to_string())
        .collect()
}

fn only(files: &[&str]) -> BTreeSet<String> {
    files.iter().map(|file| file.to_string()).collect()
}

/// Lint `config` once by walking and once by naming every file, so the walker
/// overrides and rumdl's own matchers are both held to `expected`.
fn assert_exclude(pattern: &str, expected: &BTreeSet<String>) {
    let temp = project(&format!("[global]\nexclude = ['{pattern}']\n"));
    let walk = run(temp.path(), &["."]);
    assert_eq!(
        &flagged(&walk),
        expected,
        "walk, exclude {pattern}\n{}",
        stdout_of(&walk)
    );
    let named = run(temp.path(), FILES);
    assert_eq!(
        &flagged(&named),
        expected,
        "named files, exclude {pattern}\n{}",
        stdout_of(&named)
    );
    // The same run twice gives the same answer.
    assert_eq!(flagged(&run(temp.path(), &["."])), flagged(&walk), "idempotent");

    let cli = project("");
    let walk = run(cli.path(), &[".", "--exclude", pattern]);
    assert_eq!(
        &flagged(&walk),
        expected,
        "walk, --exclude {pattern}\n{}",
        stdout_of(&walk)
    );
}

const DIR: [&str; 2] = ["dir/a.md", "dir/sub/b.md"];

#[test]
fn plain_directory_exclude_is_the_control() {
    assert_exclude("dir/**", &all_but(&DIR));
    // Negative control: a nested `dir` is untouched by an anchored pattern.
    assert!(all_but(&DIR).contains("other/dir/c.md"));
}

#[test]
fn dot_components_in_a_relative_exclude_are_ignored() {
    for pattern in ["./dir/**", "dir/./**", "./dir", "./dir/", "dir/.", "./dir/./sub/../**"] {
        assert_exclude(pattern, &all_but(&DIR));
    }
    assert_exclude("dir/./sub/b.md", &all_but(&["dir/sub/b.md"]));
}

#[test]
fn dot_dot_components_in_a_relative_exclude_cancel_a_literal_component() {
    for pattern in ["dir/../dir/**", "nonexist/../dir/**", "a/b/../../dir/**", "a/../dir"] {
        assert_exclude(pattern, &all_but(&DIR));
    }
    assert_exclude("a/b/../b/d.md", &all_but(&["a/b/d.md"]));
}

/// `./top.md` has a slash, so it is anchored to the root: it must not start to
/// float once the `./` is gone.
#[test]
fn a_dotted_exclude_stays_anchored() {
    assert_exclude("./top.md", &all_but(&["top.md"]));
    assert_exclude("./dir", &all_but(&DIR));
    // Control: the bare name floats, as before.
    assert_exclude("top.md", &all_but(&["top.md", "other/top.md"]));
    assert_exclude(
        "dir",
        &all_but(&["dir/a.md", "dir/sub/b.md", "other/dir/c.md", "a/dir/e.md"]),
    );
}

#[test]
fn unresolved_dot_dot_in_a_relative_exclude_stays_as_written() {
    // A leading `..` and a `..` after a wildcard cannot be resolved here; they
    // match nothing, as before.
    for pattern in ["../dir/**", "*/../dir/**"] {
        assert_exclude(pattern, &all_but(&[]));
    }
}

#[test]
fn dot_components_in_an_absolute_exclude_are_ignored() {
    for suffix in [
        "dir/**",
        "./dir/**",
        "dir/./**",
        "dir/../dir/**",
        "nonexist/../dir/**",
        "a/b/../../dir/**",
    ] {
        let temp = project("");
        let pattern = format!("{}/{suffix}", canonical(temp.path()));
        fs::write(
            temp.path().join(".rumdl.toml"),
            format!("[global]\nexclude = ['{pattern}']\n"),
        )
        .unwrap();
        let walk = run(temp.path(), &["."]);
        assert_eq!(
            flagged(&walk),
            all_but(&DIR),
            "walk, exclude {pattern}\n{}",
            stdout_of(&walk)
        );
        let named = run(temp.path(), FILES);
        assert_eq!(
            flagged(&named),
            all_but(&DIR),
            "named, exclude {pattern}\n{}",
            stdout_of(&named)
        );
    }
}

#[test]
fn dot_components_in_an_include_are_ignored() {
    for pattern in ["dir/**", "./dir/**", "dir/./**", "dir/../dir/**", "nonexist/../dir/**"] {
        let temp = project(&format!("[global]\ninclude = ['{pattern}']\n"));
        let walk = run(temp.path(), &["."]);
        assert_eq!(flagged(&walk), only(&DIR), "include {pattern}\n{}", stdout_of(&walk));
        // Idempotent.
        assert_eq!(flagged(&run(temp.path(), &["."])), flagged(&walk));

        let cli = project("");
        let walk = run(cli.path(), &[".", "--include", pattern]);
        assert_eq!(flagged(&walk), only(&DIR), "--include {pattern}\n{}", stdout_of(&walk));
    }
}

#[test]
fn a_dotted_include_stays_anchored() {
    let temp = project("[global]\ninclude = ['./top.md']\n");
    let walk = run(temp.path(), &["."]);
    assert_eq!(flagged(&walk), only(&["top.md"]), "{}", stdout_of(&walk));
    // Control: the bare name floats.
    let temp = project("[global]\ninclude = ['top.md']\n");
    let walk = run(temp.path(), &["."]);
    assert_eq!(
        flagged(&walk),
        only(&["top.md", "other/top.md"]),
        "{}",
        stdout_of(&walk)
    );
}

#[test]
fn dot_components_in_an_absolute_include_are_ignored() {
    for suffix in ["dir/**", "./dir/**", "nonexist/../dir/**", "a/../dir/**"] {
        let temp = project("");
        let pattern = format!("{}/{suffix}", canonical(temp.path()));
        fs::write(
            temp.path().join(".rumdl.toml"),
            format!("[global]\ninclude = ['{pattern}']\n"),
        )
        .unwrap();
        let walk = run(temp.path(), &["."]);
        assert_eq!(flagged(&walk), only(&DIR), "include {pattern}\n{}", stdout_of(&walk));
    }
}

#[test]
fn dot_components_in_a_per_file_ignores_key_are_ignored() {
    for key in ["dir/a.md", "./dir/a.md", "dir/./a.md", "x/../dir/a.md"] {
        let temp = project(&format!("[per-file-ignores]\n'{key}' = [\"MD012\"]\n"));
        let output = run(temp.path(), &["."]);
        assert_eq!(
            flagged(&output),
            all_but(&["dir/a.md"]),
            "key {key}\n{}",
            stdout_of(&output)
        );
    }
}

#[test]
fn per_file_ignores_without_a_dot_keeps_other_files() {
    // Negative control for the key above: the match is not widened.
    let temp = project("[per-file-ignores]\n'./a.md' = [\"MD012\"]\n");
    let output = run(temp.path(), &["."]);
    assert_eq!(flagged(&output), all_but(&[]), "{}", stdout_of(&output));
}

/// `link` points at `a/b`, so `link/..` is `a` and not the project root: the
/// filesystem decides, not a lexical pop.
#[cfg(unix)]
#[test]
fn dot_dot_after_a_symlink_follows_the_filesystem() {
    let temp = project("");
    std::os::unix::fs::symlink(temp.path().join("a/b"), temp.path().join("link")).unwrap();
    let root = canonical(temp.path());
    let pattern = format!("{root}/link/../dir/**");
    fs::write(
        temp.path().join(".rumdl.toml"),
        format!("[global]\nexclude = ['{pattern}']\n"),
    )
    .unwrap();
    let walk = run(temp.path(), &["."]);
    assert_eq!(flagged(&walk), all_but(&["a/dir/e.md"]), "{}", stdout_of(&walk));
    let named = run(temp.path(), FILES);
    assert_eq!(flagged(&named), all_but(&["a/dir/e.md"]), "{}", stdout_of(&named));
}

/// On Windows a backslash in an `exclude` glob is a path separator, so the
/// directory forms a user types natively work.
#[cfg(windows)]
#[test]
fn backslash_is_a_separator_in_an_exclude_on_windows() {
    for pattern in [r"dir\**", r"dir\**\", r"dir\sub\..\**", r".\dir\**"] {
        assert_exclude(pattern, &all_but(&DIR));
    }
    assert_exclude(r"a\b\*.md", &all_but(&["a/b/d.md"]));
    // A trailing directory separator keeps gitignore's floating-name semantics.
    assert_exclude(
        r"dir\",
        &all_but(&["dir/a.md", "dir/sub/b.md", "other/dir/c.md", "a/dir/e.md"]),
    );
}

/// On Unix the same spelling keeps its meaning: `\*` is a literal star, so the
/// pattern names no real file and excludes nothing.
#[cfg(not(windows))]
#[test]
fn backslash_is_an_escape_in_an_exclude_off_windows() {
    assert_exclude(r"dir\**", &all_but(&[]));
    assert_exclude(r"a\b\*.md", &all_but(&[]));
}

/// A pattern written as an absolute path names one location. Once the project
/// root is stripped, a single-component remainder must not float to every depth
/// the way the bare relative name `top.md` does.
#[test]
fn an_absolute_include_of_a_root_file_does_not_float() {
    for suffix in ["top.md", "./top.md", "docs/../top.md", "nonexist/../top.md"] {
        let temp = project("");
        let pattern = format!("{}/{suffix}", canonical(temp.path()));
        fs::write(
            temp.path().join(".rumdl.toml"),
            format!("[global]\ninclude = ['{pattern}']\n"),
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("docs")).unwrap();
        let walk = run(temp.path(), &["."]);
        assert_eq!(
            flagged(&walk),
            only(&["top.md"]),
            "include {pattern}\n{}",
            stdout_of(&walk)
        );

        let cli = project("");
        fs::create_dir_all(cli.path().join("docs")).unwrap();
        let pattern = format!("{}/{suffix}", canonical(cli.path()));
        let walk = run(cli.path(), &[".", "--include", &pattern]);
        assert_eq!(
            flagged(&walk),
            only(&["top.md"]),
            "--include {pattern}\n{}",
            stdout_of(&walk)
        );
    }
}

#[test]
fn an_absolute_include_with_a_wildcard_name_stays_at_the_root() {
    let temp = project("");
    let pattern = format!("{}/*.md", canonical(temp.path()));
    fs::write(
        temp.path().join(".rumdl.toml"),
        format!("[global]\ninclude = ['{pattern}']\n"),
    )
    .unwrap();
    let walk = run(temp.path(), &["."]);
    assert_eq!(flagged(&walk), only(&["top.md"]), "{pattern}\n{}", stdout_of(&walk));
}

/// Controls: the bare relative names keep floating, and an absolute exclude or
/// per-file key already named one location.
#[test]
fn relative_names_float_and_absolute_exclude_and_keys_do_not() {
    for name in ["top.md", "*.md"] {
        let temp = project(&format!("[global]\ninclude = ['{name}']\n"));
        let walk = run(temp.path(), &["."]);
        assert!(flagged(&walk).contains("other/top.md"), "include {name} floats");
    }
    let temp = project("");
    let abs = format!("{}/top.md", canonical(temp.path()));
    for config in [
        format!("[global]\nexclude = ['{abs}']\n"),
        format!("[per-file-ignores]\n'{abs}' = [\"MD012\"]\n"),
    ] {
        fs::write(temp.path().join(".rumdl.toml"), &config).unwrap();
        let walk = run(temp.path(), &["."]);
        assert_eq!(flagged(&walk), all_but(&["top.md"]), "{config}\n{}", stdout_of(&walk));
    }
}
