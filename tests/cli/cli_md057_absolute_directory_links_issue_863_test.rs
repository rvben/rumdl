//! Regression coverage for issue #863: under `absolute-links = "relative_to_roots"`
//! every spelling of a link to an existing directory must agree.
//!
//! The reported symptom was that `/adir/` was reported as "not found" while
//! `/adir` and `../adir/` passed, because a trailing slash re-armed MkDocs'
//! `index.md` routing convention inside filesystem mode. Roots mode resolves
//! against the filesystem, so the directory existing is the whole question.

use std::fs;
use std::path::Path;
use std::process::Command;

/// The reporter's document: the three forms that disagreed, the fragment form
/// that carries a hidden trailing slash, and a directory that is genuinely absent.
const DOCUMENT: &str = "\
# T

[no slash](/adir)
[trailing slash](/adir/)
[relative](../adir/)
[fragment](/adir/#intro)
[missing](/nodir/)
";

fn run(cwd: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "."])
        .output()
        .expect("failed to execute rumdl");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// A project holding `adir/` (no `index.md`) and a document under `docs/`.
fn workspace(config: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join(".rumdl.toml"), config).unwrap();
    fs::create_dir_all(root.join("adir")).unwrap();
    fs::write(root.join("adir").join("some.md"), "# Some\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs").join("t.md"), DOCUMENT).unwrap();
    temp
}

#[test]
fn every_spelling_of_an_existing_directory_link_agrees_under_roots_mode() {
    let temp = workspace("[MD057]\nabsolute-links = \"relative_to_roots\"\nroots = [\".\"]\n");

    let output = run(temp.path());
    assert!(
        output.contains("[MD057] Absolute link '/nodir/' was not found"),
        "a directory that does not exist is still reported:\n{output}"
    );
    assert_eq!(
        output.matches("[MD057]").count(),
        1,
        "every link to the directory that does exist must pass, however it is spelled:\n{output}"
    );
}

#[test]
fn docs_mode_still_requires_an_index_and_says_so() {
    // The control: MkDocs routing is where `index.md` means something, and there
    // the message names the directory instead of claiming it is missing.
    let temp = workspace("[MD057]\nabsolute-links = \"relative_to_docs\"\n");
    fs::write(
        temp.path().join("mkdocs.yml"),
        "site_name: Test\ndocs_dir: .\nnav:\n  - Home: docs/t.md\n",
    )
    .unwrap();

    let output = run(temp.path());
    assert!(
        output.contains("which has no index.md"),
        "docs mode reports the reason, not a missing path:\n{output}"
    );
    assert!(
        !output.contains("[MD057] Absolute link '/adir' was not found"),
        "the directory exists, so the message must not say it was not found:\n{output}"
    );
}
