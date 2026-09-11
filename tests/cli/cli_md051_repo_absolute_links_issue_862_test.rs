//! Regression coverage for issue #862: a repository-absolute cross-file link
//! (`[text](/docs/x.md#section)`) must have its fragment validated, the same as
//! the relative form.
//!
//! A leading `/` names the project root, the way a site generator serves it, so
//! resolving it means walking up to that root rather than joining it onto the
//! linking file's directory (which `Path::join` answers with the *filesystem*
//! root). The candidate has to come out spelled the way the run keys its index,
//! which is what these tests cover from every direction a run can be started:
//! from the project root, from a subdirectory of it, and with an absolute path
//! argument.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// `docs/t.md` links to `docs/x.md` in every form worth distinguishing: the
/// repository-absolute one this issue is about, the relative one that already
/// worked, a same-document fragment, an external URL, and an absolute
/// destination that names nothing in the workspace.
const LINKING_DOCUMENT: &str = "\
# T

[relative missing](x.md#missing-heading)
[absolute missing](/docs/x.md#missing-heading)
[absolute present](/docs/x.md#other-heading)
[same document](#t)
[external](https://example.com/docs/x.md#missing-heading)
[outside the workspace](/elsewhere/y.md#missing-heading)
";

const TARGET_DOCUMENT: &str = "# X\n\n## Other Heading\n";

/// A project whose root is anchored by a marker, so root discovery has the same
/// answer wherever the run is started from inside it.
fn workspace() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs").join("t.md"), LINKING_DOCUMENT).unwrap();
    fs::write(root.join("docs").join("x.md"), TARGET_DOCUMENT).unwrap();
    temp
}

fn run(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "--no-config", "--enable", "MD051"])
        .args(args)
        .output()
        .expect("failed to execute rumdl");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// Every finding the document must produce, and nothing else: the two links
/// naming a fragment that is absent from `docs/x.md`. The absolute destination
/// is reported as the document spelled it.
fn assert_only_the_two_missing_fragments_are_reported(output: &str) {
    assert!(
        output.contains("[MD051] Link fragment 'missing-heading' not found in '/docs/x.md'"),
        "the repository-absolute link's fragment must be validated:\n{output}"
    );
    assert!(
        output.contains("[MD051] Link fragment 'missing-heading' not found in 'x.md'"),
        "the relative link's fragment must still be validated:\n{output}"
    );
    let reported = output.matches("[MD051]").count();
    assert_eq!(
        reported, 2,
        "the fragment that exists, the same-document link, the external URL and the \
         destination outside the workspace must all stay silent:\n{output}"
    );
}

#[test]
fn a_repo_absolute_fragment_is_validated_from_the_project_root() {
    let temp = workspace();
    assert_only_the_two_missing_fragments_are_reported(&run(temp.path(), &["."]));
}

#[test]
fn a_repo_absolute_fragment_is_validated_from_a_subdirectory() {
    // The run's base is `docs/`, one level below the root the `/` refers to, so
    // the candidate is only reachable by resolving through the project marker.
    let temp = workspace();
    assert_only_the_two_missing_fragments_are_reported(&run(&temp.path().join("docs"), &["."]));
}

#[test]
fn a_repo_absolute_fragment_is_validated_through_an_absolute_path_argument() {
    // An absolute path argument keys the index with absolute paths, so the
    // candidate has to come out absolute too.
    let temp = workspace();
    let docs = temp.path().join("docs");
    assert_only_the_two_missing_fragments_are_reported(&run(temp.path(), &[docs.to_str().unwrap()]));
}

#[test]
fn a_repo_absolute_link_is_not_read_as_a_filesystem_path() {
    // `/docs/x.md` must name the project's `docs/x.md`, never the machine's.
    // Pinning the negative is what keeps `Path::join`'s absolute-argument
    // behavior (and its Windows spelling, `C:\docs\x.md`) out of the resolver.
    let temp = workspace();
    let output = run(temp.path(), &["."]);
    assert!(
        !output.contains("not found in 'docs/x.md'"),
        "the destination is reported as written, not as resolved:\n{output}"
    );
    assert_only_the_two_missing_fragments_are_reported(&output);
}

/// Pipe `content` into rumdl running in `cwd`, with the document named
/// `stdin_filename`.
fn run_stdin(cwd: &Path, stdin_filename: &str, content: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args([
            "check",
            "--no-cache",
            "--no-config",
            "--enable",
            "MD051",
            "--stdin",
            "--stdin-filename",
            stdin_filename,
        ])
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
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

#[test]
fn a_piped_document_answers_a_repo_absolute_link_to_itself() {
    // `--stdin-filename` names the document relative to the working directory,
    // so the candidate has to be spelled that way too: a document that links to
    // itself is answered by the bytes being linted, not by whatever is saved
    // under that name. `Buffer Heading` exists only in the piped text, so an
    // answer taken from disk reports it as missing.
    let temp = workspace();
    let piped = "\
# Buffer Heading

[to itself](/docs/t.md#buffer-heading)
[to itself, missing](/docs/t.md#not-a-heading)
";

    let output = run_stdin(temp.path(), "docs/t.md", piped);
    assert!(
        output.contains("[MD051] Link fragment 'not-a-heading' not found in '/docs/t.md'"),
        "a repo-absolute self-link is still validated:\n{output}"
    );
    assert_eq!(
        output.matches("[MD051]").count(),
        1,
        "the piped buffer, not the file on disk, answers for the document's own anchors:\n{output}"
    );
}

#[test]
fn a_piped_document_resolves_a_repo_absolute_link_to_another_file() {
    let temp = workspace();
    let piped = "# Source\n\n[absolute](/docs/x.md#missing-heading)\n[absolute present](/docs/x.md#other-heading)\n";

    let output = run_stdin(temp.path(), "docs/t.md", piped);
    assert!(
        output.contains("[MD051] Link fragment 'missing-heading' not found in '/docs/x.md'"),
        "the piped document's repo-absolute link resolves to the file it names:\n{output}"
    );
    assert_eq!(
        output.matches("[MD051]").count(),
        1,
        "the fragment that exists stays silent:\n{output}"
    );
}

#[test]
fn a_repo_absolute_link_outside_the_project_stays_silent() {
    // A destination the run never indexed is not a fragment question: MD057 is
    // the rule that answers whether a file exists. This is the same silence a
    // relative link out of the workspace already gets, pinned so the resolver
    // change cannot turn it into a report.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join("t.md"), "# T\n\n[gone](/nowhere/y.md#missing-heading)\n").unwrap();

    let output = run(root, &["."]);
    assert!(
        !output.contains("[MD051]"),
        "an unresolvable destination is not reported by MD051:\n{output}"
    );
}
