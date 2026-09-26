//! MD051 checks a cross-file fragment against the same file MD057 resolves the
//! link to: the path is percent-decoded, classified the way MD057 classifies a
//! file destination, and resolved in MD057's order and under MD057's settings.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin must be piped")
            .write_all(input)
            .expect("failed to write stdin");
    }
    child.wait_with_output().expect("failed to collect rumdl output")
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
        }
    }

    fn describe(&self) -> String {
        format!(
            "exit {:?}\nstdout:\n{}\nstderr:\n{}",
            self.code, self.stdout, self.stderr
        )
    }
}

/// Lint the directory with only the named rules and no discovered config.
fn check_dir(dir: &Path, rules: &str) -> Outcome {
    Outcome::of(run(
        dir,
        &["check", "--no-cache", "--no-config", "--enable", rules, "."],
        None,
    ))
}

/// Lint the directory with only the named rules and the directory's `.rumdl.toml`.
fn check_dir_with_config(dir: &Path, rules: &str) -> Outcome {
    Outcome::of(run(dir, &["check", "--no-cache", "--enable", rules, "."], None))
}

fn check_batch(dir: &Path, input: &[u8], extra: &[&str]) -> Outcome {
    let mut args = vec!["check", "--stdin-batch", "--no-cache", "--no-config"];
    args.extend_from_slice(extra);
    Outcome::of(run(dir, &args, Some(input)))
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Batch framing for `(path, content)` pairs.
fn batch(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (path, content) in entries {
        out.extend_from_slice(path.as_bytes());
        out.push(0);
        out.extend_from_slice(content.as_bytes());
        out.push(0);
    }
    out
}

fn cwd_absolute(dir: &Path, rel: &str) -> String {
    dir.join(rel).to_string_lossy().into_owned()
}

// Percent-encoded paths (MD057 decodes them; MD051 must name the same file).

#[test]
fn encoded_path_with_extension_is_fragment_checked() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "a.md",
        "# A\n\n[good](guide%20one.md#real)\n\n[bad](guide%20one.md#missing)\n",
    );
    write(temp.path(), "guide one.md", "# Guide\n\n## Real\n");

    let out = check_dir(temp.path(), "MD051,MD057");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout
            .contains("Link fragment 'missing' not found in 'guide%20one.md'"),
        "the missing fragment in the decoded target must be reported.\n{}",
        out.describe()
    );
    assert!(!out.stdout.contains("'real'"), "{}", out.describe());
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
    assert!(
        !out.stdout.contains("MD057"),
        "MD057 must resolve the file.\n{}",
        out.describe()
    );
}

#[test]
fn encoded_extensionless_path_is_checked_against_the_target_not_the_linking_document() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "a.md",
        "# A\n\n[good](guide%20one#real)\n\n[bad](guide%20one#missing)\n",
    );
    write(temp.path(), "guide one.md", "# Guide\n\n## Real\n");

    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout
            .contains("Link fragment 'missing' not found in 'guide%20one'"),
        "{}",
        out.describe()
    );
    assert!(
        !out.stdout.contains("does not exist in document headings"),
        "the link must not be checked against the linking document.\n{}",
        out.describe()
    );
    assert!(!out.stdout.contains("'real'"), "{}", out.describe());
}

#[test]
fn path_with_a_semicolon_is_a_cross_file_link() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "a.md", "# A\n\n[good](guide;one#real)\n");
    write(temp.path(), "guide;one.md", "# Guide\n\n## Real\n");

    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(0), "{}", out.describe());
}

#[test]
fn a_file_whose_name_contains_an_at_sign_is_fragment_checked() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "a.md",
        "# A\n\n[good](file@name.md#real) [bad](file@name.md#missing) [mail](user@example.com#missing)\n",
    );
    write(temp.path(), "file@name.md", "# File\n\n## Real\n");

    for rules in ["MD051", "MD051,MD057"] {
        let out = check_dir(temp.path(), rules);
        assert_eq!(out.code, Some(1), "{rules}: {}", out.describe());
        let reported: Vec<&str> = out.stdout.lines().filter(|line| line.contains("[MD051]")).collect();
        assert_eq!(
            reported,
            ["a.md:3:27: [MD051] Link fragment 'missing' not found in 'file@name.md'"],
            "{rules}: {}",
            out.describe()
        );
    }
}

#[test]
fn encoded_extension_dot_is_indexed_and_checked() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "a.md",
        "# A\n\n[good](guide%2Emd#real)\n\n[bad](guide%2Emd#nope)\n",
    );
    write(temp.path(), "guide.md", "# Guide\n\n## Real\n");

    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'nope' not found in 'guide%2Emd'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
}

// Resolution order and precedence (MD057's order).

#[test]
fn a_directory_that_shadows_a_markdown_file_is_not_fragment_checked() {
    // MD057 resolves `guide` to the existing directory before trying
    // `guide.md`, so the fragment names no document MD051 can check.
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "a.md", "# A\n\n[g](guide#absent)\n");
    write(temp.path(), "guide.md", "# Guide\n");
    write(temp.path(), "guide/child.md", "# Child\n");

    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(0), "{}", out.describe());

    // Control: without the directory the fragment is checked in guide.md.
    fs::remove_dir_all(temp.path().join("guide")).unwrap();
    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'absent' not found in 'guide'"),
        "{}",
        out.describe()
    );
}

#[test]
fn a_link_that_names_no_file_is_reported_by_md057_alone() {
    // MD057 matches names exactly, so `Other.md` does not name `other.md`
    // even where the filesystem would open it. The indexed `other.md` does
    // not answer for the link either.
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "a.md", "# A\n\n[x](Other.md#missing)\n");
    write(temp.path(), "other.md", "# Other\n");

    let out = check_dir(temp.path(), "MD051,MD057");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("[MD057] Relative link 'Other.md' does not exist"),
        "{}",
        out.describe()
    );
    assert!(!out.stdout.contains("MD051"), "{}", out.describe());

    // Control: spelled as the file is named, the fragment is checked.
    write(temp.path(), "a.md", "# A\n\n[x](other.md#missing)\n");
    let out = check_dir(temp.path(), "MD051,MD057");
    assert!(
        out.stdout.contains("Link fragment 'missing' not found in 'other.md'"),
        "{}",
        out.describe()
    );
}

#[test]
fn a_target_found_through_search_paths_is_fragment_checked() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), ".rumdl.toml", "[MD057]\nsearch-paths = [\"shared\"]\n");
    write(
        temp.path(),
        "docs/a.md",
        "# A\n\n[s](common.md#real)\n\n[s](common.md#gone)\n",
    );
    write(temp.path(), "shared/common.md", "# Common\n\n## Real\n");

    let out = check_dir_with_config(temp.path(), "MD051,MD057");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'gone' not found in 'common.md'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
    assert!(!out.stdout.contains("MD057"), "{}", out.describe());
}

#[test]
fn absolute_links_under_roots_check_the_file_in_the_root() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        ".rumdl.toml",
        "[MD057]\nabsolute-links = \"relative_to_roots\"\nroots = [\"content/en\"]\n",
    );
    write(
        temp.path(),
        "a.md",
        "# A\n\n[r](/guide.md#en-only)\n\n[r](/guide.md#top-only)\n",
    );
    write(temp.path(), "guide.md", "# Top\n\n## Top only\n");
    write(temp.path(), "content/en/guide.md", "# En\n\n## En only\n");

    let out = check_dir_with_config(temp.path(), "MD051,MD057");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'top-only' not found in '/guide.md'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
}

#[test]
fn absolute_links_under_the_default_setting_resolve_against_the_project_root() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "docs/a.md",
        "# A\n\n[r](/guide.md#real)\n\n[r](/guide.md#gone)\n",
    );
    write(temp.path(), "guide.md", "# Guide\n\n## Real\n");

    let out = check_dir(temp.path(), "MD051");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'gone' not found in '/guide.md'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
}

#[test]
fn docs_dir_routes_check_the_section_index() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "mkdocs.yml", "site_name: t\n");
    write(
        temp.path(),
        ".rumdl.toml",
        "[MD057]\nabsolute-links = \"relative_to_docs\"\n",
    );
    write(
        temp.path(),
        "docs/a.md",
        "# A\n\n[s](/section#real)\n\n[s](/section#missing)\n",
    );
    write(temp.path(), "docs/section/index.md", "# Section\n\n## Real\n");

    let out = check_dir_with_config(temp.path(), "MD051,MD057");
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'missing' not found in '/section'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
    assert!(!out.stdout.contains("MD057"), "{}", out.describe());
}

// Batch runs: the supplied set, then disk, as MD057 resolves it.

#[test]
fn batch_open_world_checks_the_supplied_variant_not_an_earlier_disk_file() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "guide.md", "# Disk guide\n");
    let input = batch(&[
        ("c.md", "# C\n\n[g](guide#x)\n"),
        ("guide.markdown", "# Guide\n\n## X\n"),
    ]);

    let out = check_batch(temp.path(), &input, &["--enable", "MD051,MD057"]);
    assert_eq!(out.code, Some(0), "{}", out.describe());
}

#[test]
fn batch_open_world_still_checks_an_unsupplied_disk_target() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "guide.md", "# Disk guide\n\n## Y\n");
    let input = batch(&[("c.md", "# C\n\n[g](guide#y)\n\n[g](guide#x)\n")]);

    let out = check_batch(temp.path(), &input, &["--enable", "MD051"]);
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'x' not found in 'guide'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
}

#[test]
fn batch_absolute_source_finds_a_relatively_supplied_target() {
    let temp = tempfile::tempdir().unwrap();
    let source = cwd_absolute(temp.path(), "d.md");
    let input = batch(&[(&source, "# D\n\n[b](b.md#x)\n"), ("b.md", "# B\n")]);

    let out = check_batch(
        temp.path(),
        &input,
        &["--stdin-batch-closed-world", "--enable", "MD051"],
    );
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'x' not found in 'b.md'"),
        "{}",
        out.describe()
    );
}

#[test]
fn batch_relative_source_finds_an_absolutely_supplied_target() {
    let temp = tempfile::tempdir().unwrap();
    let target = cwd_absolute(temp.path(), "b.md");
    let input = batch(&[("d.md", "# D\n\n[b](b.md#x)\n"), (&target, "# B\n")]);

    let out = check_batch(
        temp.path(),
        &input,
        &["--stdin-batch-closed-world", "--enable", "MD051"],
    );
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("Link fragment 'x' not found in 'b.md'"),
        "{}",
        out.describe()
    );
}

#[test]
fn batch_rejects_a_relative_and_an_absolute_spelling_of_one_file() {
    let temp = tempfile::tempdir().unwrap();
    let absolute = cwd_absolute(temp.path(), "e.md");
    let input = batch(&[("e.md", "# One\n"), (&absolute, "# Two\n")]);

    let out = check_batch(temp.path(), &input, &["--quiet"]);
    assert_eq!(out.code, Some(2), "{}", out.describe());
    assert!(
        out.stderr.contains(&format!("duplicate path '{absolute}'")),
        "{}",
        out.describe()
    );
}

#[test]
fn batch_closed_world_resolves_root_relative_links_in_the_supplied_set() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        ".rumdl.toml",
        "[MD057]\nabsolute-links = \"relative_to_roots\"\nroots = [\"docs\"]\n",
    );
    let input = batch(&[
        ("a.md", "# A\n\n[b](/b.md#real)\n\n[b](/b.md#gone)\n"),
        ("docs/b.md", "# B\n\n## Real\n"),
    ]);

    let out = Outcome::of(run(
        temp.path(),
        &[
            "check",
            "--stdin-batch",
            "--no-cache",
            "--stdin-batch-closed-world",
            "--enable",
            "MD051,MD057",
        ],
        Some(&input),
    ));
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        !out.stdout.contains("MD057"),
        "the supplied root target must exist.\n{}",
        out.describe()
    );
    assert!(
        out.stdout.contains("Link fragment 'gone' not found in '/b.md'"),
        "{}",
        out.describe()
    );
    assert_eq!(out.stdout.matches("MD051").count(), 1, "{}", out.describe());
}
