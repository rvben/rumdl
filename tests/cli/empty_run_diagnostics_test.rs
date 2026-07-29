//! A run that checks zero files must say so.
//!
//! Checking nothing and checking everything cleanly both exit 0 with no findings,
//! so an over-broad ignore file or a mistyped `include` pattern can silently turn
//! rumdl into a no-op that passes CI. These tests pin the notice that separates
//! the two: which stream it goes to, which flags suppress it, what it attributes
//! the emptiness to, and that machine-readable output stays parseable.

use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Runs `rumdl check` in `dir` with config discovery and the cache disabled, so
/// the result depends only on the tree the test built.
fn check(dir: &std::path::Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .current_dir(dir)
        .arg("check")
        .arg("--no-config")
        .arg("--no-cache");
    command.args(args);
    command.output().expect("failed to execute rumdl")
}

/// Runs `rumdl check` in `dir` with config discovery left on, for the cases that
/// turn on what a config file says.
fn check_configured(dir: &std::path::Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.current_dir(dir).arg("check").arg("--no-cache");
    command.args(args);
    command.output().expect("failed to execute rumdl")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A directory holding one markdown file, plus whatever `extra` files the test needs.
fn tree(extra: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("guide.md"), "# Guide\n").unwrap();
    for (path, contents) in extra {
        let full_path = temp_dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full_path, contents).unwrap();
    }
    temp_dir
}

#[test]
fn ignore_file_swallowing_every_file_is_reported() {
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    let output = check(temp_dir.path(), &["."]);
    let stderr = stderr_of(&output);

    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("1 by ignore files"),
        "should attribute the emptiness to ignore files. stderr: {stderr}"
    );
    assert!(
        stderr.contains("--respect-gitignore=false"),
        "should name the flag that undoes it. stderr: {stderr}"
    );
}

#[test]
fn include_pattern_matching_nothing_is_named() {
    let temp_dir = tree(&[]);

    // A plausible typo: the directory is `docs`, the pattern says `dcos`.
    let output = check(temp_dir.path(), &[".", "--include", "dcos/**/*.md"]);
    let stderr = stderr_of(&output);

    assert!(
        stderr.contains("1 by include patterns"),
        "should attribute the emptiness to the include patterns. stderr: {stderr}"
    );
    assert!(
        stderr.contains("include pattern 'dcos/**/*.md' matches no file"),
        "should name the pattern that selected nothing. stderr: {stderr}"
    );
}

#[test]
fn include_pattern_that_matches_is_not_reported_as_unmatched() {
    // Both patterns select a real file, but an exclude removes them, so the run is
    // empty without either include being at fault. Naming them here would send the
    // user after the wrong pattern.
    let temp_dir = tree(&[("docs/api.md", "# API\n")]);

    let output = check(
        temp_dir.path(),
        &[".", "--include", "docs/**/*.md,guide.md", "--exclude", "**/*.md"],
    );
    let stderr = stderr_of(&output);

    assert!(stderr.contains("2 by exclude patterns"), "stderr: {stderr}");
    assert!(
        !stderr.contains("matches no file"),
        "no include pattern is unmatched here. stderr: {stderr}"
    );
}

#[test]
fn a_non_markdown_include_target_counts_as_a_filtered_file() {
    // An include pattern is what makes a non-markdown file lintable at all, so a
    // run that includes one and then excludes it filtered a file out. Calling
    // that an empty directory sends the user looking for files that were never
    // the problem.
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("notes.txt"), "not markdown\n").unwrap();

    // Control: with the include alone the file really is checked.
    let checked = check(temp_dir.path(), &[".", "--include", "*.txt"]);
    assert!(
        stdout_of(&checked).contains("notes.txt"),
        "the include should reach the file. stdout: {}",
        stdout_of(&checked)
    );

    let output = check(
        temp_dir.path(),
        &[
            ".",
            "--include",
            "*.txt",
            "--exclude",
            "*.txt",
            "--deny-config-warnings",
        ],
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("1 by exclude patterns"), "stderr: {stderr}");
    assert_eq!(
        output.status.code(),
        Some(2),
        "an all-filtered run is a configuration problem whatever the extension. stderr: {stderr}"
    );
}

#[test]
fn a_named_file_excluded_beside_a_directory_still_counts() {
    let temp_dir = tree(&[]);
    fs::create_dir(temp_dir.path().join("empty")).unwrap();

    // A directory argument in the same invocation must not lose the excluded
    // argument: the run is still one file short, not an absence of files.
    let mixed = check(
        temp_dir.path(),
        &["--exclude", "guide.md", "guide.md", "empty", "--deny-config-warnings"],
    );
    let stderr = stderr_of(&mixed);
    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );
    assert_eq!(mixed.status.code(), Some(2), "stderr: {stderr}");

    // The same file named *and* rediscovered by the walked directory is one
    // file, not two.
    let overlapping = check(
        temp_dir.path(),
        &["--exclude", "guide.md", "guide.md", ".", "--deny-config-warnings"],
    );
    let stderr = stderr_of(&overlapping);
    assert!(
        stderr.contains("1 file found was filtered out."),
        "a file counted twice would read as 2. stderr: {stderr}"
    );
}

#[test]
fn a_filtered_file_under_a_vendor_directory_still_counts() {
    // rumdl checks markdown wherever it lives, including `node_modules`, `target`
    // and `.git`. A diagnosis that skipped those would report a file it really
    // would have checked as one that never existed, which is the silent absence
    // this notice exists to replace.
    let temp_dir = TempDir::new().unwrap();
    for vendor_dir in ["node_modules/pkg", "target/debug", ".git/hooks"] {
        let path = temp_dir.path().join(vendor_dir);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("readme.md"), "# Readme\n").unwrap();
    }

    // Control: the walk being explained really does reach all three.
    let checked = check(temp_dir.path(), &["."]);
    assert!(
        stdout_of(&checked).contains("3 files"),
        "the walk should reach every vendor directory. stdout: {}",
        stdout_of(&checked)
    );

    let output = check(
        temp_dir.path(),
        &[".", "--exclude", "**/*.md", "--deny-config-warnings"],
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("No markdown files left to check: 3 files found were filtered out."),
        "stderr: {stderr}"
    );
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
}

#[test]
fn a_config_include_pinning_no_extension_does_not_invent_a_filtered_file() {
    // `docs/**` widens the walk but names no file type, so rumdl never checks
    // docs/notes.txt under any setting. Reporting it as filtered out would
    // invent a configuration problem and fail a CI run that is working.
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join("docs")).unwrap();
    fs::write(temp_dir.path().join("docs/notes.txt"), "plain text\n").unwrap();
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        "[global]\ninclude = [\"docs/**\"]\n",
    )
    .unwrap();

    let output = check_configured(temp_dir.path(), &[".", "--deny-config-warnings"]);
    let stderr = stderr_of(&output);
    assert_eq!(
        stderr.trim(),
        "No markdown files found to check.",
        "the file was never lintable, so nothing was filtered out"
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");

    // Control: a pattern that does name the file type makes it lintable, and
    // then excluding it really is a file the configuration removed.
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        "[global]\ninclude = [\"docs/*.txt\"]\n",
    )
    .unwrap();
    let checked = check_configured(temp_dir.path(), &["."]);
    assert!(
        stdout_of(&checked).contains("notes.txt"),
        "the pinned pattern should reach the file. stdout: {}",
        stdout_of(&checked)
    );

    let excluded = check_configured(
        temp_dir.path(),
        &[".", "--exclude", "docs/*.txt", "--deny-config-warnings"],
    );
    let stderr = stderr_of(&excluded);
    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );
    assert_eq!(excluded.status.code(), Some(2), "stderr: {stderr}");
}

#[test]
fn a_directory_without_markdown_reports_plain_absence() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("notes.txt"), "not markdown\n").unwrap();

    let output = check(temp_dir.path(), &["."]);
    let stderr = stderr_of(&output);

    // Nothing was filtered out, so nothing is blamed.
    assert_eq!(stderr.trim(), "No markdown files found to check.");
}

#[test]
fn the_notice_goes_to_stderr_and_never_to_stdout() {
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    let output = check(temp_dir.path(), &["."]);

    assert!(
        !stdout_of(&output).contains("No markdown files"),
        "stdout carries machine-readable output. stdout: {}",
        stdout_of(&output)
    );
    assert!(stderr_of(&output).contains("No markdown files"));
}

#[test]
fn quiet_keeps_the_notice_and_silent_suppresses_it() {
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    // -q suppresses summary lines, not diagnostics, and an empty run is a
    // diagnostic about the run itself.
    let quiet = check(temp_dir.path(), &[".", "-q"]);
    assert!(
        stderr_of(&quiet).contains("No markdown files left to check"),
        "stderr: {}",
        stderr_of(&quiet)
    );

    for flag in ["-s", "--silent"] {
        let silent = check(temp_dir.path(), &[".", flag]);
        assert_eq!(stderr_of(&silent), "", "{flag} should suppress the notice");
        assert_eq!(stdout_of(&silent), "", "{flag} should suppress all output");
    }
}

#[test]
fn an_empty_run_still_exits_zero_by_default() {
    for extra in [vec![(".gitignore", "*.md\n")], vec![]] {
        let temp_dir = tree(&extra);
        let output = check(temp_dir.path(), &["."]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "checking nothing is not a violation. stderr: {}",
            stderr_of(&output)
        );
    }
}

#[test]
fn deny_config_warnings_separates_misconfiguration_from_absence() {
    // Every file filtered away points at the configuration, so an opted-in run fails.
    let filtered = tree(&[(".gitignore", "*.md\n")]);
    let output = check(filtered.path(), &[".", "--deny-config-warnings"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "an all-filtered run is a configuration problem. stderr: {}",
        stderr_of(&output)
    );

    // A directory that simply holds no markdown is not a configuration problem.
    let empty = TempDir::new().unwrap();
    fs::write(empty.path().join("notes.txt"), "not markdown\n").unwrap();
    let output = check(empty.path(), &[".", "--deny-config-warnings"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "an absence of markdown is not a configuration problem. stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn machine_readable_output_stays_a_valid_empty_document() {
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    // A human sentence on stdout, or nothing at all, leaves a consumer unable to
    // tell an empty run from a broken one.
    for format in ["json", "gitlab"] {
        let output = check(temp_dir.path(), &[".", "--output-format", format]);
        let stdout = stdout_of(&output);
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("{format} output is not valid JSON ({e}): {stdout:?}"));
        assert_eq!(
            parsed.as_array().map(Vec::len),
            Some(0),
            "{format} output should be an empty array: {stdout:?}"
        );
    }

    let sarif = stdout_of(&check(temp_dir.path(), &[".", "--output-format", "sarif"]));
    let parsed: serde_json::Value =
        serde_json::from_str(&sarif).unwrap_or_else(|e| panic!("sarif output is not valid JSON ({e}): {sarif:?}"));
    assert!(parsed.get("runs").is_some(), "sarif output should be a full document");
}

#[test]
fn an_include_pattern_takes_the_blame_over_the_ignore_file_it_outranks() {
    // An include pattern outranks an ignore file, so a gitignored file the
    // pattern selects is still checked. That makes the ignore file the wrong
    // thing to blame when an include is active: the run stays empty with ignore
    // handling switched off, and the suggested remedy fixes nothing.
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    // Control: the include really does reach past the ignore file.
    let reached = check(temp_dir.path(), &[".", "--include", "guide.md"]);
    assert!(
        stdout_of(&reached).contains("1 file"),
        "an include should outrank the ignore file. stdout: {}",
        stdout_of(&reached)
    );

    let output = check(temp_dir.path(), &[".", "--include", "dcos/**/*.md"]);
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("1 by include patterns"),
        "the typo'd include is what keeps the run empty. stderr: {stderr}"
    );
    assert!(
        !stderr.contains("ignore files"),
        "--respect-gitignore=false would leave the run just as empty. stderr: {stderr}"
    );

    // Control: with no include pattern the ignore file really is the cause, and
    // the remedy it names works.
    let output = check(temp_dir.path(), &["."]);
    let stderr = stderr_of(&output);
    assert!(stderr.contains("1 by ignore files"), "stderr: {stderr}");
    let kept = check(temp_dir.path(), &[".", "--respect-gitignore=false"]);
    assert!(
        stdout_of(&kept).contains("1 file"),
        "the named remedy should check the file. stdout: {}",
        stdout_of(&kept)
    );
}

#[test]
fn the_notice_yields_the_stream_the_output_was_routed_to() {
    // --stderr sends diagnostics to stderr, so the notice has to move to stdout.
    // Sharing the stream would put a human sentence in front of the document and
    // leave an empty run unparseable, which is the failure this notice exists to
    // prevent.
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    for format in ["json", "gitlab"] {
        let output = check(temp_dir.path(), &[".", "--output-format", format, "--stderr"]);
        let stderr = stderr_of(&output);
        let parsed: serde_json::Value = serde_json::from_str(&stderr)
            .unwrap_or_else(|e| panic!("{format} output on stderr is not valid JSON ({e}): {stderr:?}"));
        assert_eq!(parsed.as_array().map(Vec::len), Some(0), "{format}: {stderr:?}");
        assert!(
            stdout_of(&output).contains("No markdown files left to check"),
            "the notice should move to the free stream. stdout: {}",
            stdout_of(&output)
        );
    }

    // Control: without --stderr the notice stays on stderr and stdout carries
    // the document.
    let output = check(temp_dir.path(), &[".", "--output-format", "json"]);
    assert!(
        stdout_of(&output).trim().starts_with('['),
        "stdout: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("No markdown files left to check"),
        "stderr: {}",
        stderr_of(&output)
    );

    // Control: --silent still suppresses both.
    let quiet = check(
        temp_dir.path(),
        &[".", "--output-format", "json", "--stderr", "--silent"],
    );
    assert_eq!(stdout_of(&quiet), "", "stderr: {}", stderr_of(&quiet));
    assert_eq!(stderr_of(&quiet), "");
}

#[test]
fn overlapping_roots_count_a_file_once() {
    // `rumdl check . docs` hands the walker docs/a.md twice, and the run it
    // explains reduces that to one file. A tally that grew with the argument
    // list would contradict the walk it is describing.
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join("docs")).unwrap();
    fs::write(temp_dir.path().join("docs/a.md"), "# A\n").unwrap();

    // Control: the walk itself counts the file once across both roots.
    let checked = check(temp_dir.path(), &[".", "docs"]);
    assert!(
        stdout_of(&checked).contains("1 file"),
        "the walk should count it once. stdout: {}",
        stdout_of(&checked)
    );

    let overlapping = check(temp_dir.path(), &["--exclude", "*.md", ".", "docs"]);
    let stderr = stderr_of(&overlapping);
    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );

    // Control: roots that do not overlap still count both files.
    fs::create_dir(temp_dir.path().join("spec")).unwrap();
    fs::write(temp_dir.path().join("spec/b.md"), "# B\n").unwrap();
    let disjoint = check(temp_dir.path(), &["--exclude", "*.md", "docs", "spec"]);
    let stderr = stderr_of(&disjoint);
    assert!(
        stderr.contains("No markdown files left to check: 2 files found were filtered out."),
        "stderr: {stderr}"
    );

    // The walk that runs without ignore files has to dedupe on the same terms.
    fs::write(temp_dir.path().join(".gitignore"), "*.md\n").unwrap();
    let ignored = check(temp_dir.path(), &[".", "docs"]);
    let stderr = stderr_of(&ignored);
    assert!(
        stderr.contains("No markdown files left to check: 2 files found were filtered out."),
        "docs/a.md is reached by both roots and spec/b.md by one. stderr: {stderr}"
    );
    assert!(stderr.contains("2 by ignore files"), "stderr: {stderr}");
}

#[test]
fn an_include_reaching_past_an_ignore_file_hands_the_blame_to_the_exclude() {
    // The include outranks the ignore file, so the walk really did reach this
    // file and only the exclude can have dropped it. Blaming the ignore file
    // here would name a knob whose remedy leaves the run just as empty.
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    let output = check(
        temp_dir.path(),
        &[".", "--include", "guide.md", "--exclude", "guide.md"],
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("1 by exclude patterns"),
        "the exclude is what the walk applied last. stderr: {stderr}"
    );
    assert!(
        !stderr.contains("by ignore files"),
        "the include overrode the ignore file. stderr: {stderr}"
    );

    // The remedy has to end the emptiness on its own, which is what separates
    // this from naming the ignore file.
    let remedied = check(temp_dir.path(), &[".", "--include", "guide.md", "--no-exclude"]);
    assert!(
        stdout_of(&remedied).contains("No issues found in 1 file"),
        "stdout: {}",
        stdout_of(&remedied)
    );

    // Control: without an include the ignore file does hide the path, and then
    // it is what gets named however many other filters would also have matched.
    let no_include = check(temp_dir.path(), &[".", "--exclude", "guide.md"]);
    assert!(
        stderr_of(&no_include).contains("1 by ignore files"),
        "stderr: {}",
        stderr_of(&no_include)
    );
}

#[test]
fn a_capitalized_extension_is_checked_rather_than_declared_filtered() {
    // A walk that drops a file the linter would otherwise accept leaves the
    // notice with a file to report and no setting to blame for it, which reads
    // as a configuration problem the user cannot act on.
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("README.MD"), "#  Shouting\n").unwrap();

    let output = check(temp_dir.path(), &[".", "--deny-config-warnings"]);
    let stderr = stderr_of(&output);
    assert!(
        !stderr.contains("filtered out"),
        "nothing filtered this file. stderr: {stderr}"
    );
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stdout_of(&output).contains("README.MD"),
        "the file should be linted. stdout: {}",
        stdout_of(&output)
    );

    // Control: naming the file already linted it, so a walk that skips it is
    // rumdl disagreeing with itself rather than a rule about capitalization.
    let named = check(temp_dir.path(), &["README.MD"]);
    assert_eq!(named.status.code(), Some(1), "stderr: {}", stderr_of(&named));

    // Control: widening case does not widen the extension set.
    let other = TempDir::new().unwrap();
    fs::write(other.path().join("README.MDX.txt"), "#  Shouting\n").unwrap();
    assert!(
        stderr_of(&check(other.path(), &["."])).contains("No markdown files found to check."),
        "stderr: {}",
        stderr_of(&check(other.path(), &["."]))
    );
}

#[test]
fn a_hidden_file_is_not_blamed_on_a_filter_that_never_saw_it() {
    // Ignore files hide a path from the walk, so its exclude patterns never get
    // to judge it. Reporting an exclude here would answer for a matcher that
    // never ran, and a run whose files are both ignored and excluded needs both
    // knobs undone whichever one is named first.
    let temp_dir = tree(&[(".gitignore", "*.md\n")]);

    let output = check(temp_dir.path(), &[".", "--exclude", "*.md"]);
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("1 by ignore files"),
        "the ignore file is what the walk actually applied. stderr: {stderr}"
    );
    assert!(
        !stderr.contains("by exclude patterns"),
        "the exclude never saw the file. stderr: {stderr}"
    );

    // Control: once the file is visible the exclude really does judge it, and
    // then it is what gets reported.
    let visible = check(
        temp_dir.path(),
        &[".", "--exclude", "*.md", "--respect-gitignore=false"],
    );
    assert!(
        stderr_of(&visible).contains("1 by exclude patterns"),
        "stderr: {}",
        stderr_of(&visible)
    );

    // Control: an include is not a guess. A matching one overrides the ignore
    // file, so one that selects nothing is why the ignore file applied.
    let missed = check(temp_dir.path(), &[".", "--include", "dcos/**/*.md"]);
    assert!(
        stderr_of(&missed).contains("1 by include patterns"),
        "stderr: {}",
        stderr_of(&missed)
    );
}

#[test]
fn naming_the_excluded_file_does_not_change_the_diagnosis() {
    // Once a candidate has an established cause the diagnosis stops, because an
    // empty run needs one setting to undo rather than a census of everything
    // the ignore files also hid. Whether that candidate was named on the
    // command line or found by the walk is an accident of invocation, so the
    // two forms have to agree on the same tree.
    let temp_dir = tree(&[("ignored.md", "# Ignored\n"), (".gitignore", "ignored.md\n")]);

    let named = check(temp_dir.path(), &["--exclude", "guide.md", "guide.md", "."]);
    let walked = check(temp_dir.path(), &["--exclude", "guide.md", "."]);
    assert_eq!(
        stderr_of(&named),
        stderr_of(&walked),
        "naming the excluded file should not change what the run reports"
    );
    assert!(
        stderr_of(&named).contains("1 by exclude patterns"),
        "stderr: {}",
        stderr_of(&named)
    );

    // Control: the cause reported is the one that ends the empty run.
    let undone = check(
        temp_dir.path(),
        &["--exclude", "guide.md", "guide.md", ".", "--no-exclude"],
    );
    assert!(
        stdout_of(&undone).contains("1 file"),
        "--no-exclude should end the emptiness. stdout: {}",
        stdout_of(&undone)
    );
}

#[test]
fn naming_one_excluded_file_repeatedly_still_counts_one_file() {
    // Duplicate arguments, and spellings that resolve to the same path, describe
    // one file. A count that grows with the argument list overstates how much
    // the configuration removed.
    let temp_dir = tree(&[]);

    let output = check(
        temp_dir.path(),
        &["--exclude", "guide.md", "guide.md", "guide.md", "./guide.md"],
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("No markdown files left to check: 1 file found was filtered out."),
        "stderr: {stderr}"
    );

    // Control: distinct files are still counted separately.
    fs::write(temp_dir.path().join("notes.md"), "# Notes\n").unwrap();
    let output = check(temp_dir.path(), &["--exclude", "*.md", "guide.md", "notes.md"]);
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("No markdown files left to check: 2 files found were filtered out."),
        "stderr: {stderr}"
    );
}
