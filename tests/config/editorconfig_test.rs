//! End-to-end tests for `[global] editorconfig = true`.
//!
//! These drive the real binary, because the behavior under test is the
//! interaction between config precedence, per-file property resolution and
//! group formation, none of which a rule-level test exercises.
//!
//! Every fixture writes `root = true` so the walk stops inside the temp
//! directory and no `.editorconfig` from the machine running the tests leaks in.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::{TempDir, tempdir};

/// A line of 62 characters: over an `.editorconfig` limit of 40, under 80.
const LONG_LINE: &str = "This line is definitely longer than forty characters in total.";

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, contents).unwrap();
}

/// A project with an `.editorconfig`, a rumdl config, and one long-lined file.
fn project(editorconfig: &str, rumdl_toml: &str) -> TempDir {
    let temp = tempdir().unwrap();
    write(temp.path(), ".editorconfig", &format!("root = true\n{editorconfig}"));
    write(temp.path(), ".rumdl.toml", rumdl_toml);
    write(temp.path(), "doc.md", &format!("# Title\n\n{LONG_LINE}\n"));
    temp
}

fn check(dir: &Path, paths: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .args(paths)
        .current_dir(dir)
        .output()
        .expect("failed to run rumdl")
}

/// Run the binary over content piped in on stdin.
fn check_stdin(dir: &Path, content: &str, extra: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "-"])
        .args(extra)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run rumdl");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(content.as_bytes())
        .unwrap();
    child.wait_with_output().expect("failed to read rumdl output")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn max_line_length_fills_in_a_limit_no_rumdl_config_sets() {
    let temp = project(
        "[*.md]\nmax_line_length = 40\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        stdout(&out).contains("Line length 62 exceeds 40 characters"),
        "expected the .editorconfig limit to apply, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn the_editorconfig_is_ignored_without_the_opt_in() {
    let temp = project("[*.md]\nmax_line_length = 40\n", "[global]\nenable = [\"MD013\"]\n");

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        !stdout(&out).contains("MD013"),
        "a .editorconfig must do nothing until a rumdl config opts in, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn a_rumdl_line_length_wins_over_the_editorconfig() {
    let temp = project(
        "[*.md]\nmax_line_length = 40\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\nline-length = 100\n",
    );

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        !stdout(&out).contains("MD013"),
        "an explicit rumdl line-length must outrank the .editorconfig, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn a_rumdl_rule_line_length_wins_over_the_editorconfig() {
    // 80 is also MD013's default, and the rule falls back to the global limit
    // whenever it holds that value. An explicit 80 must still be explicit.
    let temp = project(
        "[*.md]\nmax_line_length = 120\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n\n[MD013]\nline-length = 80\n",
    );
    // 97 characters: over the rule's limit, under the `.editorconfig` one, so a
    // build that lets the `.editorconfig` through reports nothing at all.
    write(
        temp.path(),
        "doc.md",
        &format!("# Title\n\n{LONG_LINE} It is over eighty characters wide.\n"),
    );

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        stdout(&out).contains("Line length 97 exceeds 80 characters"),
        "a line length set on the rule must outrank the .editorconfig, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn max_line_length_off_means_no_limit() {
    let temp = project(
        "[*.md]\nmax_line_length = off\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );
    // Long enough to exceed rumdl's own default, so a limit of any kind shows up.
    write(temp.path(), "doc.md", &format!("# Title\n\n{LONG_LINE} {LONG_LINE}\n"));

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        !stdout(&out).contains("MD013"),
        "`max_line_length = off` must lift the limit, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn indent_size_sets_the_expected_list_indent() {
    let temp = project(
        "[*.md]\nindent_size = 4\n",
        "[global]\neditorconfig = true\nenable = [\"MD007\"]\n",
    );
    write(temp.path(), "doc.md", "# Title\n\n- a\n  - b\n");

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        stdout(&out).contains("Expected 4 spaces for indent depth 1, found 2"),
        "expected the .editorconfig indent to apply, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn an_indent_size_rumdl_cannot_use_is_reported_not_guessed() {
    let temp = project(
        "[*.md]\nindent_size = 12\n",
        "[global]\neditorconfig = true\nenable = [\"MD007\"]\n",
    );
    write(temp.path(), "doc.md", "# Title\n\n- a\n  - b\n");

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        stderr(&out).contains("outside the 1-8 spaces MD007 accepts"),
        "an unusable indent must be reported, got stderr:\n{}",
        stderr(&out)
    );
    assert!(
        !stdout(&out).contains("MD007"),
        "the default 2-space indent must still hold, got:\n{}",
        stdout(&out)
    );
}

#[test]
fn a_section_glob_applies_per_file_not_per_directory() {
    let temp = project(
        "[narrow.md]\nmax_line_length = 40\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );
    write(temp.path(), "narrow.md", &format!("# Title\n\n{LONG_LINE}\n"));
    write(temp.path(), "wide.md", &format!("# Title\n\n{LONG_LINE}\n"));

    let out = check(temp.path(), &["narrow.md", "wide.md"]);
    let stdout = stdout(&out);
    assert!(
        stdout.contains("narrow.md:3") && stdout.contains("exceeds 40 characters"),
        "the section must apply to the file it names, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("wide.md"),
        "a sibling the section does not name must keep the default limit, got:\n{stdout}"
    );
}

#[test]
fn a_nested_editorconfig_overrides_the_one_above_it() {
    let temp = project(
        "[*.md]\nmax_line_length = 100\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );
    write(temp.path(), "docs/.editorconfig", "[*.md]\nmax_line_length = 40\n");
    write(temp.path(), "docs/nested.md", &format!("# Title\n\n{LONG_LINE}\n"));

    let out = check(temp.path(), &["doc.md", "docs/nested.md"]);
    let stdout = stdout(&out);
    assert!(
        stdout.contains("exceeds 40 characters"),
        "the nearer .editorconfig must win for the file it covers, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("doc.md"),
        "the file outside it keeps the outer limit, got:\n{stdout}"
    );
}

#[test]
fn stdin_named_as_a_file_gets_that_file_s_editorconfig() {
    let temp = project(
        "[*.md]\nmax_line_length = 40\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );
    let content = format!("# Title\n\n{LONG_LINE}\n");

    let expected = "Line length 62 exceeds 40 characters";
    assert!(
        stdout(&check(temp.path(), &["doc.md"])).contains(expected),
        "the file on disk is the reference this content is compared against"
    );

    // Reading stdin keeps stdout for the content itself, so the findings are on
    // stderr here rather than where file mode puts them.
    let piped = check_stdin(temp.path(), &content, &["--stdin-filename", "doc.md"]);
    assert!(
        stderr(&piped).contains(expected),
        "the same content named as the same file must lint the same way, got stderr:\n{}",
        stderr(&piped)
    );
}

#[test]
fn stdin_without_a_filename_has_no_editorconfig_to_resolve() {
    // Properties are resolved per file, and unnamed content is not one. The
    // limit is rumdl's own rather than a guess at which file this might be.
    let temp = project(
        "[*.md]\nmax_line_length = 40\n",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );

    let out = check_stdin(temp.path(), &format!("# Title\n\n{LONG_LINE}\n"), &[]);
    assert!(
        !stderr(&out).contains("MD013"),
        "there is no file to resolve properties for, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_through_stdin_is_a_config_problem() {
    let temp = project("[*.md]\nindent_style = tab\n", "[global]\neditorconfig = true\n");

    let out = check_stdin(
        temp.path(),
        "# Title\n",
        &["--stdin-filename", "doc.md", "--deny-config-warnings"],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdin owns its own exit, so it has to make the same decision, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_property_rumdl_contradicts_is_reported_once_per_run() {
    let temp = project("[*.md]\nindent_style = tab\n", "[global]\neditorconfig = true\n");
    write(temp.path(), "second.md", "# Second\n");

    let out = check(temp.path(), &["doc.md", "second.md"]);
    let occurrences = stderr(&out).matches("`indent_style = tab` is not applied").count();
    assert_eq!(
        occurrences,
        1,
        "one .editorconfig covering many files must warn once, got stderr:\n{}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("MD010"),
        "the warning must name the rule responsible, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_is_a_config_problem_under_deny_config_warnings() {
    let temp = project("[*.md]\nindent_style = tab\n", "[global]\neditorconfig = true\n");

    let denied = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--deny-config-warnings", "doc.md"])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert_eq!(
        denied.status.code(),
        Some(2),
        "a reported .editorconfig problem must fail the run like any other config problem, got stderr:\n{}",
        stderr(&denied)
    );

    // Suppressing the output does not make the problem go away.
    let silent = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--silent", "--deny-config-warnings", "doc.md"])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert_eq!(silent.status.code(), Some(2), "--silent must not change the exit code");

    assert_eq!(
        check(temp.path(), &["doc.md"]).status.code(),
        Some(0),
        "without the flag it stays a warning"
    );
}

#[test]
fn a_warning_that_is_dropped_does_not_fail_deny_config_warnings() {
    let temp = project(
        "[*.md]\nindent_style = tab\n",
        "[global]\neditorconfig = true\ndisable = [\"MD010\"]\n",
    );

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--deny-config-warnings", "doc.md"])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a warning nobody reports cannot be a problem, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_is_dropped_when_per_file_ignores_exclude_its_rule() {
    // The rule is enabled for the project and turned off for this file, so it
    // reports nothing about the tab and the warning about it is not true here.
    let temp = project(
        "[*.md]\nindent_style = tab\n",
        "[global]\neditorconfig = true\n\n[per-file-ignores]\n\"*.md\" = [\"MD010\"]\n",
    );
    write(temp.path(), "doc.md", "# Title\n\n\ttabbed line\n");

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--deny-config-warnings", "doc.md"])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert!(
        !stderr(&out).contains("indent_style"),
        "the rule the warning names does not run for this file, got stderr:\n{}",
        stderr(&out)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "and a warning nobody reports cannot fail the run, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_answers_for_the_file_it_came_from() {
    // The property reaches one file, and that file is the only one that could
    // have reported the rule the warning names. A sibling in the same run still
    // running MD010 says nothing about the tab in `special.md`.
    let temp = project(
        "[special.md]\nindent_style = tab\n",
        "[global]\neditorconfig = true\n\n[per-file-ignores]\n\"special.md\" = [\"MD010\"]\n",
    );
    write(temp.path(), "special.md", "# Title\n\n\ttabbed line\n");
    write(temp.path(), "other.md", "# Other\n");

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "special.md",
            "other.md",
        ])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert!(
        !stderr(&out).contains("indent_style"),
        "the rule the warning names does not run for the file the property applies to, got stderr:\n{}",
        stderr(&out)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "and a warning nobody reports cannot fail the run, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_survives_while_a_file_it_covers_still_runs_the_rule() {
    // Here the property covers both files and only one of them ignores MD010, so
    // a tab in the sibling would still be flagged: the property is genuinely not
    // being honored.
    let temp = project(
        "[*.md]\nindent_style = tab\n",
        "[global]\neditorconfig = true\n\n[per-file-ignores]\n\"special.md\" = [\"MD010\"]\n",
    );
    write(temp.path(), "special.md", "# Title\n\n\ttabbed line\n");
    write(temp.path(), "other.md", "# Other\n");

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "special.md",
            "other.md",
        ])
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    assert!(
        stderr(&out).contains("indent_style"),
        "the warning is true for a file the property reaches, got stderr:\n{}",
        stderr(&out)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "and a reported config warning fails the run, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_divergence_warning_is_dropped_when_its_rule_is_disabled() {
    let temp = project(
        "[*.md]\nindent_style = tab\n",
        "[global]\neditorconfig = true\ndisable = [\"MD010\"]\n",
    );

    let out = check(temp.path(), &["doc.md"]);
    assert!(
        !stderr(&out).contains("indent_style"),
        "a warning about a disabled rule is no longer true, got stderr:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_subdirectory_config_opts_in_on_its_own() {
    let temp = tempdir().unwrap();
    write(
        temp.path(),
        ".editorconfig",
        "root = true\n[*.md]\nmax_line_length = 40\n",
    );
    write(temp.path(), ".rumdl.toml", "[global]\nenable = [\"MD013\"]\n");
    write(temp.path(), "doc.md", &format!("# Title\n\n{LONG_LINE}\n"));
    write(
        temp.path(),
        "docs/.rumdl.toml",
        "[global]\neditorconfig = true\nenable = [\"MD013\"]\n",
    );
    write(temp.path(), "docs/nested.md", &format!("# Title\n\n{LONG_LINE}\n"));

    let out = check(temp.path(), &["."]);
    let stdout = stdout(&out);
    assert!(
        stdout.contains("nested.md") && stdout.contains("exceeds 40 characters"),
        "the subdirectory config's opt-in must apply to its own files, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("doc.md"),
        "the root config did not opt in, so its files keep the default, got:\n{stdout}"
    );
}

#[test]
fn rumdl_config_says_editorconfig_values_are_not_shown() {
    let temp = project("[*.md]\nmax_line_length = 40\n", "[global]\neditorconfig = true\n");

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("config")
        .current_dir(temp.path())
        .output()
        .expect("failed to run rumdl");
    let stdout = stdout(&out);
    assert!(
        stdout.contains("resolved") && stdout.contains("per file"),
        "the output cannot show per-file values, so it must say so, got:\n{stdout}"
    );
}
