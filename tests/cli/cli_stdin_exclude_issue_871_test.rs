//! `exclude` applies to a document read from stdin exactly as it applies to a
//! path argument naming the same file.
//!
//! Editors and pre-commit hooks lint and format through `--stdin-filename`, so a
//! file the configuration excludes has to stay excluded there: `check` reports
//! nothing, and `fmt` / `check --fix` hand the document back byte for byte, since
//! an editor replaces its buffer with whatever arrives on stdout.
//!
//! The name is resolved the way a path argument is, relative to the working
//! directory and whether or not a file exists under it. The same resolution keys
//! `per-file-ignores`, so those tests live here too.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Two blank-line runs too many: `MD012` fires twice, and `fmt` rewrites it.
const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const EXCLUDED_NOTICE: &str = "1 by exclude patterns; pass --no-exclude to keep them";

/// A project anchored by a marker and a config, holding `docs/x.md` and `top.md`.
fn workspace(config: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join(".rumdl.toml"), config).unwrap();
    fs::write(root.join("docs").join("x.md"), DOCUMENT).unwrap();
    fs::write(root.join("top.md"), DOCUMENT).unwrap();
    temp
}

fn excluding_docs() -> tempfile::TempDir {
    workspace("[global]\nexclude = [\"docs\"]\n")
}

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

fn check_stdin(cwd: &Path, name: &str, extra: &[&str]) -> Output {
    let mut args = vec!["check", "--no-cache", "--stdin-filename", name];
    args.extend_from_slice(extra);
    args.push("-");
    run(cwd, &args, DOCUMENT.as_bytes())
}

fn fmt_stdin(cwd: &Path, name: &str, extra: &[&str], input: &[u8]) -> Output {
    let mut args = vec!["fmt", "--no-cache", "--stdin-filename", name];
    args.extend_from_slice(extra);
    args.push("-");
    run(cwd, &args, input)
}

/// Nothing linted, nothing reported, and the run says why.
fn assert_skipped_by_exclude(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", describe(output));
    assert!(!text(&output.stdout).contains("MD012"), "{}", describe(output));
    assert!(!text(&output.stderr).contains("MD012"), "{}", describe(output));
    assert!(text(&output.stderr).contains(EXCLUDED_NOTICE), "{}", describe(output));
}

fn assert_linted(output: &Output) {
    assert_eq!(output.status.code(), Some(1), "{}", describe(output));
    assert_eq!(
        text(&output.stdout).matches("[MD012]").count(),
        2,
        "{}",
        describe(output)
    );
}

// --- The reported case -------------------------------------------------------

#[test]
fn check_skips_a_piped_document_whose_name_is_excluded() {
    let temp = excluding_docs();
    assert_skipped_by_exclude(&check_stdin(temp.path(), "docs/x.md", &[]));
}

#[test]
fn check_agrees_with_the_same_name_given_as_a_path() {
    // The control this behavior is measured against.
    let temp = excluding_docs();
    let output = run(temp.path(), &["check", "--no-cache", "docs/x.md"], b"");
    assert_skipped_by_exclude(&output);
}

#[test]
fn fmt_hands_an_excluded_document_back_unchanged() {
    let temp = excluding_docs();
    let output = fmt_stdin(temp.path(), "docs/x.md", &[], DOCUMENT.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, DOCUMENT.as_bytes(), "{}", describe(&output));
    assert!(text(&output.stderr).contains(EXCLUDED_NOTICE), "{}", describe(&output));
}

#[test]
fn check_fix_hands_an_excluded_document_back_unchanged() {
    let temp = excluding_docs();
    let output = run(
        temp.path(),
        &["check", "--fix", "--no-cache", "--stdin-filename", "docs/x.md", "-"],
        DOCUMENT.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, DOCUMENT.as_bytes(), "{}", describe(&output));
    assert!(!text(&output.stderr).contains("MD012"), "{}", describe(&output));
}

// --- The bytes handed back ---------------------------------------------------

#[test]
fn fmt_preserves_crlf_and_mixed_line_endings_of_an_excluded_document() {
    let temp = excluding_docs();
    let input = b"# Title\r\n\r\na\r\n\r\n\r\n\r\nb\n\n\n\r\n";
    let output = fmt_stdin(temp.path(), "docs/x.md", &[], input);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, input, "{}", describe(&output));
}

#[test]
fn fmt_hands_back_an_excluded_document_that_is_not_utf8() {
    // An excluded document is never read as markdown, so bytes that could not be
    // are not an error: they belong to a file this run does not own.
    let temp = excluding_docs();
    let input = b"# Title\n\n\xff\xfe latin-1 \xe9\n";
    let output = fmt_stdin(temp.path(), "docs/x.md", &[], input);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, input.as_slice(), "{}", describe(&output));
}

#[test]
fn fmt_hands_back_an_empty_excluded_document_as_empty() {
    let temp = excluding_docs();
    let output = fmt_stdin(temp.path(), "docs/x.md", &[], b"");
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(output.stdout.is_empty(), "{}", describe(&output));
}

// --- Spellings of the name ---------------------------------------------------

#[test]
fn a_name_with_no_file_on_disk_is_excluded() {
    // An unsaved editor buffer, including one in a directory not created yet.
    let temp = excluding_docs();
    assert_skipped_by_exclude(&check_stdin(temp.path(), "docs/ghost.md", &[]));
    assert_skipped_by_exclude(&check_stdin(temp.path(), "docs/new/dir/ghost.md", &[]));
}

#[test]
fn an_absolute_name_is_excluded() {
    let temp = excluding_docs();
    let absolute = temp.path().join("docs").join("x.md");
    assert_skipped_by_exclude(&check_stdin(temp.path(), absolute.to_str().unwrap(), &[]));
    let ghost = temp.path().join("docs").join("ghost.md");
    assert_skipped_by_exclude(&check_stdin(temp.path(), ghost.to_str().unwrap(), &[]));
}

#[test]
fn a_relative_name_is_resolved_against_the_working_directory() {
    // From `docs/`, `x.md` is `docs/x.md`, as `rumdl check x.md` there reads it.
    let temp = excluding_docs();
    let docs = temp.path().join("docs");
    assert_skipped_by_exclude(&check_stdin(&docs, "x.md", &[]));
    assert_skipped_by_exclude(&check_stdin(&docs, "./ghost.md", &[]));
    assert_linted(&check_stdin(&docs, "../top.md", &[]));
}

#[test]
fn a_name_that_climbs_through_a_missing_directory_resolves_lexically() {
    let temp = excluding_docs();
    assert_skipped_by_exclude(&check_stdin(temp.path(), "missing/../docs/x.md", &[]));
    assert_linted(&check_stdin(temp.path(), "docs/missing/../../top.md", &[]));
}

// --- Which patterns apply ----------------------------------------------------

#[test]
fn a_name_outside_every_pattern_is_still_linted() {
    let temp = excluding_docs();
    assert_linted(&check_stdin(temp.path(), "top.md", &[]));
    assert_linted(&check_stdin(temp.path(), "documents/x.md", &[]));
}

#[test]
fn stdin_without_a_name_is_always_linted() {
    let temp = excluding_docs();
    let output = run(temp.path(), &["check", "--no-cache", "-"], DOCUMENT.as_bytes());
    assert_linted(&output);
}

#[test]
fn no_exclude_lints_and_formats_an_excluded_name() {
    let temp = excluding_docs();
    assert_linted(&check_stdin(temp.path(), "docs/x.md", &["--no-exclude"]));

    let output = fmt_stdin(temp.path(), "docs/x.md", &["--no-exclude"], DOCUMENT.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(text(&output.stdout), "# Title\n\na\n\nb\n", "{}", describe(&output));
}

#[test]
fn a_cli_exclude_replaces_the_configured_patterns() {
    let temp = excluding_docs();
    assert_linted(&check_stdin(temp.path(), "docs/x.md", &["--exclude", "elsewhere"]));
    assert_skipped_by_exclude(&check_stdin(temp.path(), "top.md", &["--exclude", "top.md"]));
}

#[test]
fn glob_and_file_patterns_match_a_piped_name() {
    let temp = workspace("[global]\nexclude = [\"docs/**/*.md\", \"top.md\"]\n");
    assert_skipped_by_exclude(&check_stdin(temp.path(), "docs/x.md", &[]));
    assert_skipped_by_exclude(&check_stdin(temp.path(), "docs/deep/ghost.md", &[]));
    assert_skipped_by_exclude(&check_stdin(&temp.path().join("docs"), "../top.md", &[]));
}

// --- What the run reports ----------------------------------------------------

#[test]
fn deny_config_warnings_fails_an_excluded_run_like_the_path_flow() {
    let temp = excluding_docs();
    let path_flow = run(
        temp.path(),
        &["check", "--no-cache", "--deny-config-warnings", "docs/x.md"],
        b"",
    );
    let stdin = check_stdin(temp.path(), "docs/x.md", &["--deny-config-warnings"]);
    assert_eq!(path_flow.status.code(), Some(2), "{}", describe(&path_flow));
    assert_eq!(stdin.status.code(), Some(2), "{}", describe(&stdin));
}

#[test]
fn json_output_of_an_excluded_check_is_an_empty_document() {
    let temp = excluding_docs();
    let output = check_stdin(temp.path(), "docs/x.md", &["--output-format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    assert_eq!(parsed, serde_json::json!([]), "{}", describe(&output));
}

#[test]
fn json_output_of_an_excluded_fmt_keeps_stdout_for_the_document() {
    let temp = excluding_docs();
    let output = fmt_stdin(
        temp.path(),
        "docs/x.md",
        &["--output-format", "json"],
        DOCUMENT.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, DOCUMENT.as_bytes(), "{}", describe(&output));
    match serde_json::from_slice::<serde_json::Value>(&output.stderr) {
        Ok(parsed) => {
            assert_eq!(parsed, serde_json::json!([]), "{}", describe(&output));
        }
        Err(e) => {
            panic!("Failed to parse stderr as JSON: {e}\n{}", describe(&output));
        }
    }
}

#[test]
fn silent_suppresses_the_notice_but_not_the_document() {
    let temp = excluding_docs();
    let output = fmt_stdin(temp.path(), "docs/x.md", &["--silent"], DOCUMENT.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, DOCUMENT.as_bytes(), "{}", describe(&output));
    assert!(output.stderr.is_empty(), "{}", describe(&output));
}

#[test]
fn verbose_names_the_pattern_that_excluded_the_document() {
    let temp = excluding_docs();
    let output = check_stdin(temp.path(), "docs/x.md", &["--verbose"]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(
        // The same notice `rumdl check --verbose docs/x.md` prints, naming the
        // directory pattern in the form it matched files with.
        text(&output.stderr).contains("docs/x.md ignored because of exclude pattern 'docs/**'"),
        "{}",
        describe(&output)
    );
}

// --- Batch stdin -------------------------------------------------------------

fn batch(cwd: &Path, input: &[u8], extra: &[&str]) -> Output {
    let mut args = vec!["check", "--stdin-batch", "--no-cache"];
    args.extend_from_slice(extra);
    run(cwd, &args, input)
}

#[test]
fn a_batch_lints_only_the_documents_exclude_leaves() {
    let temp = excluding_docs();
    let input = format!("docs/x.md\0{DOCUMENT}\0top.md\0{DOCUMENT}\0");
    let output = batch(temp.path(), input.as_bytes(), &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert_eq!(stdout.matches("top.md:").count(), 2, "{}", describe(&output));
    assert!(!stdout.contains("docs/x.md"), "{}", describe(&output));
}

#[test]
fn a_batch_resolves_its_paths_against_the_working_directory() {
    let temp = excluding_docs();
    let input = format!("x.md\0{DOCUMENT}\0../top.md\0{DOCUMENT}\0");
    let output = batch(&temp.path().join("docs"), input.as_bytes(), &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(stdout.contains("top.md:"), "{}", describe(&output));
    assert!(!stdout.contains("x.md:"), "{}", describe(&output));
}

#[test]
fn a_batch_whose_every_document_is_excluded_reports_an_empty_run() {
    let temp = excluding_docs();
    let input = format!("docs/x.md\0{DOCUMENT}\0docs/ghost.md\0{DOCUMENT}\0");

    let output = batch(temp.path(), input.as_bytes(), &["--output-format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    assert_eq!(parsed, serde_json::json!([]), "{}", describe(&output));
    assert!(
        text(&output.stderr).contains("2 by exclude patterns; pass --no-exclude to keep them"),
        "{}",
        describe(&output)
    );

    let denied = batch(temp.path(), input.as_bytes(), &["--deny-config-warnings"]);
    assert_eq!(denied.status.code(), Some(2), "{}", describe(&denied));
}

#[test]
fn a_batch_with_no_exclude_lints_every_document() {
    let temp = excluding_docs();
    let input = format!("docs/x.md\0{DOCUMENT}\0top.md\0{DOCUMENT}\0");
    let output = batch(temp.path(), input.as_bytes(), &["--no-exclude"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert_eq!(stdout.matches("[MD012]").count(), 4, "{}", describe(&output));
}

#[test]
fn an_excluded_batch_document_still_exists_as_a_link_target() {
    // Excluded means not linted, not absent: the snapshot still supplies the
    // file, so a link to it from a linted document resolves.
    let temp = excluding_docs();
    let input = "docs/new.md\0# New\n\0top.md\0# Top\n\n[new](docs/new.md)\n\0";
    let output = batch(temp.path(), input.as_bytes(), &["--enable", "MD057"]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(!text(&output.stdout).contains("MD057"), "{}", describe(&output));
}

#[test]
fn stdin_warns_about_an_invalid_exclude_pattern_as_often_as_a_walk_does() {
    // Deciding whether a stdin name is excluded compiles the patterns too, so
    // the warning must come from one place or it repeats per consumer. A link
    // into another file makes the stdin runs scan the project, the way a walk does.
    let temp = workspace("[global]\nexclude = [\"[\", \"docs\"]\n");
    let warnings = |output: &Output| text(&output.stderr).matches("Invalid exclude pattern '['").count();

    let walk = run(temp.path(), &["check", "--no-cache"], b"");
    let expected = warnings(&walk);
    assert!(expected > 0, "the walk must report the pattern\n{}", describe(&walk));

    let linking = "# Top\n\n[x](docs/x.md#frag)\n";
    let single = run(
        temp.path(),
        &["check", "--no-cache", "--stdin-filename", "top.md", "-"],
        linking.as_bytes(),
    );
    assert_eq!(warnings(&single), expected, "{}", describe(&single));

    let batched = batch(temp.path(), format!("top.md\0{linking}\0").as_bytes(), &[]);
    assert_eq!(warnings(&batched), expected, "{}", describe(&batched));
}

// --- per-file-ignores follows the same resolution ----------------------------

fn ignoring_md012_in_docs() -> tempfile::TempDir {
    workspace("[per-file-ignores]\n\"docs/**\" = [\"MD012\"]\n")
}

#[test]
fn per_file_ignores_apply_to_a_name_relative_to_a_subdirectory() {
    let temp = ignoring_md012_in_docs();
    let docs = temp.path().join("docs");

    // The path flow from the same directory is the control.
    let path_flow = run(&docs, &["check", "--no-cache", "x.md"], b"");
    assert_eq!(path_flow.status.code(), Some(0), "{}", describe(&path_flow));

    let output = check_stdin(&docs, "x.md", &[]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert!(!text(&output.stdout).contains("MD012"), "{}", describe(&output));

    let ghost = check_stdin(&docs, "ghost.md", &[]);
    assert_eq!(ghost.status.code(), Some(0), "{}", describe(&ghost));
}

#[test]
fn per_file_ignores_keep_fmt_from_rewriting_a_name_relative_to_a_subdirectory() {
    let temp = ignoring_md012_in_docs();
    let output = fmt_stdin(&temp.path().join("docs"), "x.md", &[], DOCUMENT.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    assert_eq!(output.stdout, DOCUMENT.as_bytes(), "{}", describe(&output));
}

#[test]
fn per_file_ignores_do_not_leak_onto_a_name_outside_their_pattern() {
    let temp = ignoring_md012_in_docs();
    assert_linted(&check_stdin(&temp.path().join("docs"), "../top.md", &[]));
    assert_linted(&check_stdin(temp.path(), "top.md", &[]));
}

#[test]
fn per_file_ignores_apply_to_batch_paths_relative_to_a_subdirectory() {
    let temp = ignoring_md012_in_docs();
    let input = format!("x.md\0{DOCUMENT}\0../top.md\0{DOCUMENT}\0");
    let output = batch(&temp.path().join("docs"), input.as_bytes(), &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert_eq!(stdout.matches("[MD012]").count(), 2, "{}", describe(&output));
    assert!(!stdout.contains("x.md:"), "{}", describe(&output));
}
