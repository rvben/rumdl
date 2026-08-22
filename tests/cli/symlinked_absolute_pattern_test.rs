//! Issue #822: an absolute config pattern that names a location through a
//! symlink matches the files there. Discovery reports every file at its real
//! location, so a pattern written the way the user reaches the directory
//! (`/var/folders/...` on macOS, where `/var` is a symlink to `/private/var`)
//! used to compare against a path spelled a different way and match nothing.
//!
//! The unit tests in `src/discovery.rs` and `src/config/tests.rs` pin the
//! matching itself; these run the binary over a directory walk, which is the
//! invocation the report used and the one that installs the walker overrides.
//!
//! Every assertion is paired with a control: a second file the pattern must not
//! speak for, and a run whose pattern names an unrelated absolute path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Content with MD019 and MD064 violations, reported unless the file is
/// ignored or excluded.
const VIOLATION: &str = "#  Heading with two spaces\n";

/// A project at `real/` holding `sub/note.md` and `other.md`, reachable both as
/// itself and through the sibling symlink `link`. Returns the temp directory
/// plus the two spellings of the project root.
fn project_reachable_through_a_symlink() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    // The temp root itself may be symlinked (macOS puts it under `/var`), so
    // resolve it first: `link` must be the only symlink under test.
    let root = fs::canonicalize(temp.path()).unwrap();
    let real = root.join("real");
    let link = root.join("link");

    fs::create_dir_all(real.join("sub")).unwrap();
    fs::write(real.join("sub/note.md"), VIOLATION).unwrap();
    fs::write(real.join("other.md"), VIOLATION).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    (temp, real, link)
}

/// Write `config_body` into the project and run `rumdl check --no-cache .`
/// there, returning stdout and stderr together. Caching is off so a stale entry
/// cannot mask a discovery change, and the home directory points at the empty
/// temp root so a developer's user-level config cannot reach the run.
fn check_with_config(real: &Path, config_body: &str) -> String {
    fs::write(real.join(".rumdl.toml"), config_body).unwrap();
    let home = real.parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(real)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .output()
        .expect("failed to execute rumdl");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn per_file_ignores_silences_a_file_its_pattern_named_through_a_symlink() {
    let (_temp, real, link) = project_reachable_through_a_symlink();
    let output = check_with_config(
        &real,
        &format!(
            "[per-file-ignores]\n'{}/sub/**' = [\"MD019\", \"MD064\"]\n",
            link.display()
        ),
    );

    assert!(
        !output.contains("sub/note.md"),
        "a pattern naming `sub` through the symlink should silence its rules. Output:\n{output}"
    );
    assert!(
        output.contains("other.md"),
        "control: a file outside the pattern must still be reported. Output:\n{output}"
    );
}

#[test]
fn per_file_ignores_control_reports_the_file_for_an_unrelated_absolute_pattern() {
    let (_temp, real, _link) = project_reachable_through_a_symlink();
    let output = check_with_config(
        &real,
        "[per-file-ignores]\n'/nowhere/at/all/sub/**' = [\"MD019\", \"MD064\"]\n",
    );

    assert!(
        output.contains("sub/note.md"),
        "control: an unrelated absolute pattern must leave the rules active. Output:\n{output}"
    );
}

#[test]
fn exclude_drops_a_directory_its_pattern_named_through_a_symlink() {
    let (_temp, real, link) = project_reachable_through_a_symlink();
    let output = check_with_config(&real, &format!("[global]\nexclude = ['{}/sub']\n", link.display()));

    assert!(
        !output.contains("sub/note.md"),
        "a pattern naming `sub` through the symlink should exclude it. Output:\n{output}"
    );
    assert!(
        output.contains("other.md"),
        "control: a file outside the pattern must still be linted. Output:\n{output}"
    );
}

#[test]
fn exclude_control_lints_the_directory_for_an_unrelated_absolute_pattern() {
    let (_temp, real, _link) = project_reachable_through_a_symlink();
    let output = check_with_config(&real, "[global]\nexclude = ['/nowhere/at/all/sub']\n");

    assert!(
        output.contains("sub/note.md"),
        "control: an unrelated absolute pattern must leave the file linted. Output:\n{output}"
    );
}

#[test]
fn include_selects_a_directory_its_pattern_named_through_a_symlink() {
    let (_temp, real, link) = project_reachable_through_a_symlink();
    let output = check_with_config(&real, &format!("[global]\ninclude = ['{}/sub/**']\n", link.display()));

    assert!(
        output.contains("sub/note.md"),
        "a pattern naming `sub` through the symlink should select its files. Output:\n{output}"
    );
    assert!(
        !output.contains("other.md"),
        "control: include must still restrict discovery to matching files. Output:\n{output}"
    );
}

#[test]
fn include_control_selects_nothing_for_an_unrelated_absolute_pattern() {
    let (_temp, real, _link) = project_reachable_through_a_symlink();
    let output = check_with_config(&real, "[global]\ninclude = ['/nowhere/at/all/sub/**']\n");

    assert!(
        !output.contains("note.md") && !output.contains("other.md"),
        "control: an unrelated absolute include must select no file. Output:\n{output}"
    );
}

#[test]
fn a_brace_alternation_reaches_through_the_symlink_for_per_file_ignores() {
    let (_temp, real, link) = project_reachable_through_a_symlink();
    let output = check_with_config(
        &real,
        &format!(
            "[per-file-ignores]\n'{{/nowhere/at/all,{}}}/sub/**' = [\"MD019\", \"MD064\"]\n",
            link.display()
        ),
    );

    assert!(
        !output.contains("sub/note.md"),
        "an alternation whose second branch names the symlink should silence its rules. Output:\n{output}"
    );
    assert!(
        output.contains("other.md"),
        "control: a file outside the pattern must still be reported. Output:\n{output}"
    );
}
