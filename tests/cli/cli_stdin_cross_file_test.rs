//! Regression tests for issue #789: cross-file link validation over `--stdin`.
//!
//! A run over files indexes the workspace before resolving cross-file references.
//! A piped document has no workspace, so MD051's cross-file check used to be
//! silently skipped: `rumdl check .` reported a broken `[link](other.md#missing)`
//! and piping the identical bytes through `--stdin --stdin-filename` reported
//! nothing. Editors and pre-commit hooks drive rumdl this way, so the finding
//! simply disappeared for them.
//!
//! The stdin path now resolves the targets the piped document names, reading each
//! one from disk exactly as MD057 already does here. These tests pin that, plus
//! the boundaries: no `--stdin-filename` means no directory to resolve against,
//! and a document linking to itself is judged against the piped buffer rather
//! than whatever is saved under that name.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Pipe `content` into rumdl running in `dir`, returning (stdout, stderr).
fn run_with_stdin(dir: &Path, content: &str, args: &[&str]) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
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
        .write_all(content.as_bytes())
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to collect rumdl output");
    (
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    )
}

/// `b.md` offers one anchor, so `#real-heading` resolves and `#nonexistent` does not.
fn write_target(dir: &Path) {
    fs::write(dir.join("b.md"), "# Target\n\n## Real Heading\n").unwrap();
}

#[test]
fn stdin_reports_a_missing_fragment_in_a_linked_file() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);

    let content = "# Source\n\n[link](b.md#nonexistent)\n";
    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Link fragment 'nonexistent' not found in 'b.md'"),
        "stdin must resolve the file the link names, got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// The oracle: piping a file's bytes must report what scanning the directory
/// holding it reports. A single-file `rumdl check a.md` is *not* a valid oracle
/// here, because it never indexes `b.md` either.
#[test]
fn stdin_agrees_with_a_directory_scan() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);

    // Two files a walk never reaches, both readable and both parsing as markdown
    // if anything were to read them.
    fs::write(dir.join("sprite.svg"), "# Not Markdown\n").unwrap();
    fs::write(dir.join("notes"), "# Extensionless\n").unwrap();

    let content = "# Source\n\n\
         [broken](b.md#nonexistent)\n\n\
         [valid](b.md#real-heading)\n\n\
         [extensionless](b#nonexistent)\n\n\
         [query](b.md?raw=true#nonexistent)\n\n\
         [missing-file](gone.md#nonexistent)\n\n\
         [asset](sprite.svg#icon)\n\n\
         [not-walked](notes#nonexistent)\n";
    fs::write(dir.join("a.md"), content).unwrap();

    let scan = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "."])
        .output()
        .expect("failed to execute rumdl");
    let scan_out = format!(
        "{}{}",
        String::from_utf8_lossy(&scan.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&scan.stderr).replace("\r\n", "\n")
    );

    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let piped = format!("{stdout}{stderr}");

    // Every MD051 finding the scan reports, and no others.
    let md051_lines = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| l.contains("MD051"))
            .map(|l| l.trim().to_string())
            .collect()
    };
    assert_eq!(
        md051_lines(&piped),
        md051_lines(&scan_out),
        "piped findings must match the directory scan.\npiped:\n{piped}\nscan:\n{scan_out}"
    );

    // Positive control: the comparison above is only meaningful if the scan found
    // something. Three of the seven links are broken; the valid one, the one
    // naming a file that does not exist, and the two a walk never reaches are the
    // negative controls.
    assert_eq!(
        md051_lines(&scan_out).len(),
        3,
        "expected 3 broken cross-file fragments in the oracle, got:\n{scan_out}"
    );
    assert!(
        !piped.contains("real-heading"),
        "a fragment that exists must not be reported, got:\n{piped}"
    );
}

/// A run knows about the file it was handed plus whatever a walk reaches, and the
/// walk's extension filter is one of the things that decides what it reaches.
/// Reading a readable `sprite.svg`, or an extension-less file sitting next to the
/// document, would let the piped run report fragments no directory scan checks.
#[test]
fn stdin_does_not_index_files_a_directory_scan_never_would() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);
    fs::write(dir.join("sprite.svg"), "# Not Markdown\n").unwrap();
    fs::write(dir.join("notes"), "# Extensionless\n").unwrap();

    let content = "# Source\n\n\
         [asset](sprite.svg#icon)\n\n\
         [not-walked](notes#nonexistent)\n\n\
         [walked](b.md#nonexistent)\n";
    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let combined = format!("{stdout}{stderr}");

    // Positive control: the run did reach cross-file checking, so the silence
    // below is a decision about those two targets rather than a dead code path.
    assert!(
        combined.contains("Link fragment 'nonexistent' not found in 'b.md'"),
        "the Markdown target must still be resolved, got:\n{combined}"
    );
    assert!(
        !combined.contains("'icon'"),
        "a non-Markdown target is not part of the workspace, got:\n{combined}"
    );
    assert!(
        !combined.contains("not found in 'notes'"),
        "an extension-less file a walk never reaches is not part of the workspace, got:\n{combined}"
    );
}

/// The extension is not the only thing that keeps a file out of the workspace.
/// The ignore files and the configured exclude patterns do too, and a Markdown
/// target they filter out is one `rumdl check .` never reads, so a piped run that
/// read it would report a finding the batch run does not.
#[test]
fn stdin_does_not_index_a_target_the_workspace_filters_out() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);
    fs::write(dir.join("rumdl.toml"), "exclude = [\"excluded.md\"]\n").unwrap();
    fs::write(dir.join(".markdownlintignore"), "ignored.md\n").unwrap();
    fs::write(dir.join("excluded.md"), "# Excluded\n").unwrap();
    fs::write(dir.join("ignored.md"), "# Ignored\n").unwrap();

    let content = "# Source\n\n\
         [walked](b.md#nonexistent)\n\n\
         [excluded](excluded.md#nonexistent)\n\n\
         [ignored](ignored.md#nonexistent)\n";
    fs::write(dir.join("a.md"), content).unwrap();
    let args = [
        "check",
        "--no-cache",
        "--config",
        "rumdl.toml",
        "--stdin",
        "--stdin-filename",
        "a.md",
    ];
    let (stdout, stderr) = run_with_stdin(dir, content, &args);
    let combined = format!("{stdout}{stderr}");

    // Positive control: the run reached cross-file checking with this config, so
    // the two silences below are decisions about those targets.
    assert!(
        combined.contains("not found in 'b.md'"),
        "an included target must still be resolved, got:\n{combined}"
    );
    assert!(
        !combined.contains("not found in 'excluded.md'"),
        "a config-excluded target is not part of the workspace, got:\n{combined}"
    );
    assert!(
        !combined.contains("not found in 'ignored.md'"),
        "a target an ignore file filters out is not part of the workspace, got:\n{combined}"
    );

    // The oracle agrees on every one of the three, which is the property the
    // silences above exist to hold.
    let scan = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "--config", "rumdl.toml", "."])
        .output()
        .expect("failed to execute rumdl");
    let scan_out = format!(
        "{}{}",
        String::from_utf8_lossy(&scan.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&scan.stderr).replace("\r\n", "\n")
    );
    assert!(
        scan_out.contains("not found in 'b.md'")
            && !scan_out.contains("not found in 'excluded.md'")
            && !scan_out.contains("not found in 'ignored.md'"),
        "oracle: a directory scan reports only the included target, got:\n{scan_out}"
    );
}

/// `--stdin-filename` is usually relative, so a destination leaving the
/// document's own directory has nothing above it to resolve against. Dropping
/// the traversal renames the target: `../b.md` becomes `b.md`, a sibling that
/// exists, sits in the workspace and answers with its own headings - so the
/// piped run would report a fragment against a file the link does not name and
/// a directory scan never reads.
#[test]
fn stdin_does_not_resolve_a_traversal_to_a_file_inside_the_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("project");
    fs::create_dir_all(&dir).unwrap();
    write_target(&dir);
    // The file the link actually names, one level up and outside the scan.
    fs::write(temp.path().join("b.md"), "# Outside\n\n## Nonexistent\n").unwrap();

    let content = "# Source\n\n\
         [outside](../b.md#nonexistent)\n\n\
         [inside](b.md#nonexistent)\n";
    fs::write(dir.join("a.md"), content).unwrap();
    let args = [
        "check",
        "--no-cache",
        "--no-config",
        "--stdin",
        "--stdin-filename",
        "a.md",
    ];
    let (stdout, stderr) = run_with_stdin(&dir, content, &args);
    let combined = format!("{stdout}{stderr}");

    // Positive control: the sibling really is in the workspace and really is
    // missing the fragment, so the silence below is about the traversal alone.
    assert!(
        combined.contains("not found in 'b.md'"),
        "the sibling target must still be resolved, got:\n{combined}"
    );
    assert!(
        !combined.contains("not found in '../b.md'"),
        "a target above the scanned directory is not part of the workspace, got:\n{combined}"
    );

    let scan = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(&dir)
        .args(["check", "--no-cache", "--no-config", "."])
        .output()
        .expect("failed to execute rumdl");
    let scan_out = format!(
        "{}{}",
        String::from_utf8_lossy(&scan.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&scan.stderr).replace("\r\n", "\n")
    );
    assert!(
        scan_out.contains("not found in 'b.md'") && !scan_out.contains("not found in '../b.md'"),
        "oracle: a directory scan reports the sibling and not the traversal, got:\n{scan_out}"
    );
}

/// A scan indexes each file under the config that governs it, and a directory
/// with its own rumdl config governs the files in it. The flavor set there
/// decides how a heading's anchor is spelled, so a target read under the piped
/// document's config instead of its own is checked against anchors that file
/// does not have.
#[test]
fn stdin_indexes_a_target_under_the_config_that_governs_it() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    fs::create_dir(dir.join("docs")).unwrap();
    fs::write(dir.join("rumdl.toml"), "flavor = \"standard\"\n").unwrap();
    fs::write(dir.join("docs").join("rumdl.toml"), "flavor = \"mkdocs\"\n").unwrap();
    // `## A & B` slugs to `a--b` under GitHub's algorithm (the standard flavor)
    // and to `a-b` under Python-Markdown's (the mkdocs flavor), so exactly one of
    // the two links below is broken and which one says which config was used.
    fs::write(dir.join("docs").join("target.md"), "# Target\n\n## A & B\n").unwrap();

    let content = "# Source\n\n\
         [gh](docs/target.md#a--b)\n\n\
         [pm](docs/target.md#a-b)\n";
    fs::write(dir.join("a.md"), content).unwrap();

    let args = ["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"];
    let (stdout, stderr) = run_with_stdin(dir, content, &args);
    let piped = format!("{stdout}{stderr}");

    let scan = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "."])
        .output()
        .expect("failed to execute rumdl");
    let scan_out = format!(
        "{}{}",
        String::from_utf8_lossy(&scan.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&scan.stderr).replace("\r\n", "\n")
    );

    // The target lives under the mkdocs config, so `a-b` is its anchor and the
    // GitHub spelling is the broken one. Asserting the exact fragment rather than
    // a count is what distinguishes the two configs: reading the target under the
    // root's `standard` flavor reports the other link.
    assert!(
        piped.contains("Link fragment 'a--b' not found") && !piped.contains("Link fragment 'a-b' not found"),
        "the target must be indexed under its own directory's config, got:\n{piped}"
    );
    assert!(
        scan_out.contains("Link fragment 'a--b' not found") && !scan_out.contains("Link fragment 'a-b' not found"),
        "oracle: a directory scan reports the GitHub spelling as broken, got:\n{scan_out}"
    );
}

/// The piped document answers for itself whatever it is named: it is the file
/// this run was handed, exactly as `rumdl check notes.txt` lints the file it was
/// given. The walk-only rule above governs every *other* target.
#[test]
fn stdin_resolves_a_self_link_in_a_non_markdown_file() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    let content = "# Notes\n\n[self](notes.txt#nonexistent)\n";
    fs::write(dir.join("notes.txt"), content).unwrap();

    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "notes.txt"],
    );
    let piped = format!("{stdout}{stderr}");

    // The oracle here is naming the file explicitly, which is what a stdin run
    // does: `rumdl check notes.txt` lints and indexes it despite the extension.
    let named = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "notes.txt"])
        .output()
        .expect("failed to execute rumdl");
    let named_out = format!(
        "{}{}",
        String::from_utf8_lossy(&named.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&named.stderr).replace("\r\n", "\n")
    );

    assert!(
        named_out.contains("Link fragment 'nonexistent' not found in 'notes.txt'"),
        "oracle: naming the file reports the broken self-link, got:\n{named_out}"
    );
    assert!(
        piped.contains("Link fragment 'nonexistent' not found in 'notes.txt'"),
        "piping the same file must agree with naming it, got:\n{piped}"
    );
}

/// Without `--stdin-filename` a relative destination has no directory to resolve
/// against, so there is nothing to look up. MD057 is already silent here for the
/// same reason.
#[test]
fn stdin_without_a_filename_reports_no_cross_file_findings() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);

    let content = "# Source\n\n[link](b.md#nonexistent)\n";
    let (stdout, stderr) = run_with_stdin(dir, content, &["check", "--no-cache", "--stdin"]);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("MD051"),
        "without --stdin-filename there is no directory to resolve against, got:\n{combined}"
    );
}

/// A document that links to itself is answered by the text being linted. An
/// editor pipes an unsaved buffer, so the copy on disk is exactly the stale
/// answer `--stdin` exists to avoid.
#[test]
fn stdin_resolves_a_self_link_against_the_piped_buffer() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    // The saved copy still carries the old anchor; the buffer has renamed it.
    fs::write(dir.join("a.md"), "# Old Heading\n").unwrap();

    let content = "# New Heading\n\n[self](a.md#old-heading)\n";
    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Link fragment 'old-heading' not found in 'a.md'"),
        "a self-link must be judged against the piped buffer, not the saved file, got:\n{combined}"
    );

    // The same document under a `./`-spelled filename resolves identically.
    let (dot_stdout, dot_stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--stdin", "--stdin-filename", "./a.md"],
    );
    let dot_combined = format!("{dot_stdout}{dot_stderr}");
    assert!(
        dot_combined.contains("Link fragment 'old-heading' not found in 'a.md'"),
        "`./a.md` names the same document as `a.md`, got:\n{dot_combined}"
    );

    // Negative control: a self-link to an anchor the buffer *does* have is silent,
    // even though the saved copy lacks it.
    let (ok_stdout, ok_stderr) = run_with_stdin(
        dir,
        "# New Heading\n\n[self](a.md#new-heading)\n",
        &["check", "--no-cache", "--stdin", "--stdin-filename", "a.md"],
    );
    let ok_combined = format!("{ok_stdout}{ok_stderr}");
    assert!(
        !ok_combined.contains("MD051"),
        "an anchor the buffer defines must not be reported, got:\n{ok_combined}"
    );
}

/// A cross-file finding carries no fix, so it survives the fix pass. Leaving it
/// out of the re-check would count it as fixed and report a clean run.
#[test]
fn stdin_fix_reports_the_cross_file_finding_as_remaining() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_target(dir);

    // The trailing spaces are MD009, which is fixable; the fragment is not.
    let content = "# Source   \n\n[the target document](b.md#nonexistent)\n";
    let (stdout, stderr) = run_with_stdin(
        dir,
        content,
        &["check", "--no-cache", "--fix", "--stdin", "--stdin-filename", "a.md"],
    );

    assert_eq!(
        stdout, "# Source\n\n[the target document](b.md#nonexistent)\n",
        "fixed content must reach stdout with the link untouched, got:\n{stdout}"
    );
    assert!(
        stderr.contains("1 issue(s) fixed, 1 issue(s) remaining"),
        "the cross-file finding must be counted as remaining, got:\n{stderr}"
    );
    let md051_lines: Vec<&str> = stderr.lines().filter(|l| l.contains("MD051")).collect();
    assert_eq!(
        md051_lines.len(),
        1,
        "expected the cross-file finding to be listed once, got:\n{stderr}"
    );
    for line in md051_lines {
        assert!(
            !line.contains("[fixed]"),
            "MD051 must not be labelled as fixed, got line:\n{line}"
        );
    }
}
