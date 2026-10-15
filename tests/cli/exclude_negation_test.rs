//! A leading `!` in `exclude` is rejected, and rejected the same way everywhere.
//!
//! `exclude` has no re-inclusion. The walk used to read `!x` as an ordinary
//! exclusion of `x` (and a lone `!` as an exclusion of everything) while a file
//! named directly, a stdin document and the editor integration read the `!` as a
//! literal character that matches nothing. A negation now matches nothing in
//! every consumer, is reported once per run, and the report names
//! `.markdownlintignore`, which does re-include.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `MD012` fires on every file, so a file with no findings was excluded.
const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const FILES: &[&str] = &["docs/api/index.md", "docs/api/gen.md", "top.md", "!bang.md"];

const NEGATION_ADVICE: &str = ".markdownlintignore";

fn write_project(root: &Path, config_name: &str, config: &str) {
    fs::create_dir_all(root.join(".git")).unwrap();
    if !config.is_empty() {
        fs::write(root.join(config_name), config).unwrap();
    }
    for file in FILES {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, DOCUMENT).unwrap();
    }
}

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
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    child.wait_with_output().expect("failed to collect rumdl output")
}

fn linted(output: &Output) -> BTreeSet<String> {
    let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");
    FILES
        .iter()
        .filter(|file| stdout.lines().any(|line| line.starts_with(&format!("{file}:"))))
        .map(|file| file.to_string())
        .collect()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn warnings(output: &Output) -> usize {
    stderr_of(output).matches("Invalid exclude pattern").count()
}

/// Every route lints exactly `expected`, and every route reports the invalid
/// pattern exactly `reports` times. Returns nothing: the assertions carry `case`.
fn assert_routes(root: &Path, args: &[&str], expected: &[&str], reports: usize, case: &str) {
    let expected: BTreeSet<String> = expected.iter().map(|file| file.to_string()).collect();

    let walk = run(root, args, b"");
    assert_eq!(linted(&walk), expected, "walk under {case}\n{}", stderr_of(&walk));
    assert_eq!(
        warnings(&walk),
        reports,
        "walk warnings under {case}\n{}",
        stderr_of(&walk)
    );

    let named = run(root, &[args, FILES].concat(), b"");
    assert_eq!(
        linted(&named),
        expected,
        "named files under {case}\n{}",
        stderr_of(&named)
    );
    assert_eq!(
        warnings(&named),
        reports,
        "named file warnings under {case}\n{}",
        stderr_of(&named)
    );

    let batch_input: String = FILES.iter().map(|file| format!("{file}\0{DOCUMENT}\0")).collect();
    let batch = run(root, &[args, &["--stdin-batch"]].concat(), batch_input.as_bytes());
    assert_eq!(
        linted(&batch),
        expected,
        "--stdin-batch under {case}\n{}",
        stderr_of(&batch)
    );
    // A stdin run reports an invalid pattern only when it scans the project the
    // way a walk does, so it may stay silent, but it never reports one twice.
    assert!(
        warnings(&batch) <= reports,
        "batch warnings under {case}\n{}",
        stderr_of(&batch)
    );

    let stdin = run(
        root,
        &[args, &["--stdin-filename", "docs/api/index.md", "-"]].concat(),
        DOCUMENT.as_bytes(),
    );
    assert_eq!(
        linted(&stdin).contains("docs/api/index.md"),
        expected.contains("docs/api/index.md"),
        "stdin under {case}\n{}",
        stderr_of(&stdin)
    );
    assert!(
        warnings(&stdin) <= reports,
        "stdin warnings under {case}\n{}",
        stderr_of(&stdin)
    );
}

fn assert_config_routes(patterns: &[&str], expected: &[&str], reports: usize) {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), ".rumdl.toml", &exclude_config(patterns));
    assert_routes(temp.path(), &[], expected, reports, &format!("exclude = {patterns:?}"));
}

const ALL: &[&str] = &["docs/api/index.md", "docs/api/gen.md", "top.md", "!bang.md"];

#[test]
fn a_negated_pattern_excludes_nothing_on_any_route() {
    assert_config_routes(&["!docs/api/index.md"], ALL, 1);
    assert_config_routes(&["!docs/**"], ALL, 1);
}

#[test]
fn a_lone_bang_excludes_nothing_on_any_route() {
    assert_config_routes(&["!"], ALL, 1);
}

#[test]
fn a_negation_does_not_re_include_what_an_earlier_pattern_excluded() {
    // The exception is dropped, the broad exclusion stands, on every route.
    assert_config_routes(&["docs/api/**", "!docs/api/index.md"], &["top.md", "!bang.md"], 1);
}

#[test]
fn each_distinct_negation_is_reported_once_and_a_repeat_is_not_a_second_report() {
    assert_config_routes(&["!a.md", "!b.md", "!a.md"], ALL, 2);
}

#[test]
fn the_exclude_flag_rejects_a_negation_like_the_config_does() {
    for (flag, expected) in [
        ("!docs/api/index.md", ALL),
        ("!", ALL),
        ("docs/api/**,!docs/api/index.md", &["top.md", "!bang.md"][..]),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_project(temp.path(), "", "");
        assert_routes(
            temp.path(),
            &["--exclude", flag],
            expected,
            1,
            &format!("--exclude {flag:?}"),
        );
    }
}

#[test]
fn pyproject_exclude_rejects_a_negation_like_the_config_does() {
    let temp = tempfile::tempdir().unwrap();
    write_project(
        temp.path(),
        "pyproject.toml",
        "[tool.rumdl]\nexclude = ['!docs/api/index.md']\n",
    );
    assert_routes(temp.path(), &[], ALL, 1, "pyproject exclude");
}

#[test]
fn the_report_quotes_a_pattern_of_the_users_own_config_and_names_the_migration() {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), ".rumdl.toml", &exclude_config(&["!docs/api/index.md"]));
    let stderr = stderr_of(&run(temp.path(), &[], b""));
    assert!(
        stderr.contains("Invalid exclude pattern '!docs/api/index.md'")
            && stderr.contains("does not negate")
            && stderr.contains(NEGATION_ADVICE),
        "{stderr}"
    );
}

#[test]
#[cfg(not(windows))]
fn an_escaped_bang_is_a_literal_name_on_every_route() {
    // `\!bang.md` names the file `!bang.md`: no report, and it is excluded.
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), ".rumdl.toml", &exclude_config(&[r"\!bang.md"]));
    assert_routes(
        temp.path(),
        &[],
        &["docs/api/index.md", "docs/api/gen.md", "top.md"],
        0,
        "exclude = [\\!bang.md]",
    );
}

#[test]
fn an_ordinary_exclude_is_unchanged_and_silent() {
    assert_config_routes(&["docs/api/gen.md"], &["docs/api/index.md", "top.md", "!bang.md"], 0);
}

#[test]
fn markdownlintignore_still_re_includes() {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), "", "");
    fs::write(
        temp.path().join(".markdownlintignore"),
        "docs/api/**\n!docs/api/index.md\n",
    )
    .unwrap();
    let walk = run(temp.path(), &[], b"");
    assert_eq!(
        linted(&walk),
        ["docs/api/index.md", "top.md", "!bang.md"]
            .iter()
            .map(|file| file.to_string())
            .collect::<BTreeSet<_>>(),
        "{}",
        stderr_of(&walk)
    );
}

#[test]
fn a_rejected_negation_is_not_counted_as_an_active_exclude_in_the_empty_run_notice() {
    // `guide.md` survives ignore handling and is left out by `--include` alone. A
    // lone `!` used to exclude it too, which put it under "exclude patterns" and
    // pointed the user at a pattern that excludes nothing.
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    fs::create_dir_all(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("guide.md"), "# Guide\n").unwrap();
    let output = run(
        temp.path(),
        &["--no-config", "--include", "docs/**/*.md", "--exclude", "!", "."],
        b"",
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("No markdown files left to check"), "{stderr}");
    assert!(!stderr.contains("by exclude patterns"), "{stderr}");
    assert!(stderr.contains("1 by include patterns"), "{stderr}");
    assert_eq!(warnings(&output), 1, "{stderr}");

    // Control: the same run with a real exclude does count it.
    let control = run(
        temp.path(),
        &["--no-config", "--include", "docs/**/*.md", "--exclude", "guide.md", "."],
        b"",
    );
    assert!(
        stderr_of(&control).contains("by exclude patterns"),
        "{}",
        stderr_of(&control)
    );
}

#[test]
fn a_stdin_run_that_scans_the_project_reports_a_negation_as_often_as_a_walk() {
    // A link into another file makes the stdin runs scan the project the way a
    // walk does; the report must still come from one place.
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), ".rumdl.toml", &exclude_config(&["!docs/api/index.md"]));
    let linking = "# Top\n\n[x](docs/api/gen.md#frag)\n";

    let walk = run(temp.path(), &[], b"");
    assert_eq!(warnings(&walk), 1, "{}", stderr_of(&walk));

    let single = run(temp.path(), &["--stdin-filename", "top.md", "-"], linking.as_bytes());
    assert_eq!(warnings(&single), 1, "{}", stderr_of(&single));

    let batch_input = format!("top.md\0{linking}\0");
    let batched = run(temp.path(), &["--stdin-batch"], batch_input.as_bytes());
    assert_eq!(warnings(&batched), 1, "{}", stderr_of(&batched));
}
