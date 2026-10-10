//! `--stdin-batch-targets <FILE>` declares paths that exist without linting
//! them: a NUL-terminated list of files, and directories with a trailing `/`.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn describe(&self) -> String {
        format!(
            "exit {:?}\nstdout:\n{}\nstderr:\n{}",
            self.code, self.stdout, self.stderr
        )
    }
}

fn run(dir: &Path, args: &[&str], stdin: &[u8]) -> Outcome {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child.stdin.as_mut().unwrap().write_all(stdin).unwrap();
    let output: Output = child.wait_with_output().unwrap();
    Outcome {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    }
}

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

/// Write a targets file holding each entry NUL-terminated.
fn write_targets(dir: &Path, name: &str, entries: &[&str]) -> String {
    let mut bytes = Vec::new();
    for entry in entries {
        bytes.extend_from_slice(entry.as_bytes());
        bytes.push(0);
    }
    fs::write(dir.join(name), bytes).unwrap();
    name.to_string()
}

fn check(dir: &Path, targets: Option<&str>, closed_world: bool, input: &[u8]) -> Outcome {
    let mut args = vec![
        "check",
        "--stdin-batch",
        "--no-cache",
        "--no-config",
        "--enable",
        "MD057,MD051",
    ];
    if closed_world {
        args.push("--stdin-batch-closed-world");
    }
    if let Some(targets) = targets {
        args.push("--stdin-batch-targets");
        args.push(targets);
    }
    run(dir, &args, input)
}

const LINKS: &str = "# A\n\n[pdf](files/report.pdf)\n\n[img](img/logo.png)\n";

#[test]
fn closed_world_reports_a_link_to_an_unlisted_target() {
    let temp = tempfile::tempdir().unwrap();
    let out = check(temp.path(), None, true, &batch(&[("a.md", LINKS)]));
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("files/report.pdf"), "{}", out.describe());
    assert!(out.stdout.contains("img/logo.png"), "{}", out.describe());
}

#[test]
fn listed_targets_satisfy_links_in_closed_world() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["files/report.pdf", "img/logo.png"]);
    let out = check(temp.path(), Some(&targets), true, &batch(&[("a.md", LINKS)]));
    assert_eq!(out.code, Some(0), "{}", out.describe());
    assert!(!out.stdout.contains("MD057"), "{}", out.describe());
}

#[test]
fn a_link_to_a_path_not_listed_still_fails() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["files/report.pdf"]);
    let out = check(temp.path(), Some(&targets), true, &batch(&[("a.md", LINKS)]));
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("img/logo.png"), "{}", out.describe());
    assert!(!out.stdout.contains("files/report.pdf"), "{}", out.describe());
}

#[test]
fn listed_targets_hide_the_disk_in_open_world_too() {
    // Open world consults disk for unlisted paths: the listed one exists only
    // in the caller's snapshot and is still accepted.
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["files/report.pdf", "img/logo.png"]);
    let out = check(temp.path(), Some(&targets), false, &batch(&[("a.md", LINKS)]));
    assert_eq!(out.code, Some(0), "{}", out.describe());
}

#[test]
fn directories_are_implied_by_listed_paths_and_by_trailing_slash_entries() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["files/report.pdf", "empty/"]);
    let input = batch(&[("a.md", "# A\n\n[d](files/)\n\n[e](empty/)\n\n[n](nothere/)\n")]);
    let out = check(temp.path(), Some(&targets), true, &input);
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("nothere/"), "{}", out.describe());
    assert!(!out.stdout.contains("files/'"), "{}", out.describe());
    assert!(!out.stdout.contains("empty/'"), "{}", out.describe());
}

#[test]
fn a_directory_entry_does_not_satisfy_an_extensionless_document_link() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["guide/"]);
    // `guide` as a directory still exists as a link target.
    let ok = check(
        temp.path(),
        Some(&targets),
        true,
        &batch(&[("a.md", "# A\n\n[g](guide)\n")]),
    );
    assert_eq!(ok.code, Some(0), "{}", ok.describe());
}

#[test]
fn relative_entries_resolve_against_the_working_directory_like_batch_paths() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["./docs/../files/report.pdf"]);
    let input = batch(&[("a.md", "# A\n\n[p](files/report.pdf)\n")]);
    let out = check(temp.path(), Some(&targets), true, &input);
    assert_eq!(out.code, Some(0), "{}", out.describe());

    let absolute = temp.path().join("files/report.pdf").to_string_lossy().into_owned();
    let targets = write_targets(temp.path(), "abs", &[&absolute]);
    let out = check(temp.path(), Some(&targets), true, &input);
    assert_eq!(out.code, Some(0), "{}", out.describe());
}

#[test]
fn a_target_that_is_also_a_batch_document_is_linted() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["b.md"]);
    let input = batch(&[
        ("a.md", "# A\n\n[b](b.md#real)\n\n[b](b.md#gone)\n"),
        ("b.md", "# B\n\n## Real\n\n[bad](missing.png)\n"),
    ]);
    let out = check(temp.path(), Some(&targets), true, &input);
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(
        out.stdout.contains("missing.png"),
        "b.md must be linted.\n{}",
        out.describe()
    );
    assert!(
        out.stdout.contains("'gone'"),
        "its headings feed MD051.\n{}",
        out.describe()
    );
    assert!(!out.stdout.contains("'real'"), "{}", out.describe());
}

#[test]
fn a_fragment_into_a_target_only_markdown_file_is_not_reported() {
    // No content is known for a listed path, so its headings are unknown, not empty.
    let temp = tempfile::tempdir().unwrap();
    // The disk copy has no such heading and must not be consulted either.
    fs::write(temp.path().join("other.md"), "# Other\n").unwrap();
    let targets = write_targets(temp.path(), "targets", &["other.md"]);
    let input = batch(&[("a.md", "# A\n\n[o](other.md#anything)\n")]);
    for closed_world in [true, false] {
        let out = check(temp.path(), Some(&targets), closed_world, &input);
        assert_eq!(out.code, Some(0), "closed_world={closed_world}\n{}", out.describe());
    }
    // Control: without the listing, open world reads the disk copy and reports.
    let out = check(temp.path(), None, false, &input);
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("'anything'"), "{}", out.describe());
}

#[cfg(unix)]
#[test]
fn a_listed_file_read_through_an_unlisted_alias_has_its_fragments_checked() {
    // `listed.md` is never read, but `real.md` is the same file under an
    // unlisted name: once open world reads it, its headings are known.
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("real.md"), "# Real\n").unwrap();
    std::os::unix::fs::symlink("real.md", temp.path().join("listed.md")).unwrap();
    let targets = write_targets(temp.path(), "targets", &["listed.md"]);

    // Relative and root-relative links take different lookups; both must agree.
    for listed in ["listed.md", "/listed.md"] {
        let listed_only = batch(&[("a.md", &format!("# A\n\n[l]({listed}#missing)\n"))]);
        let out = check(temp.path(), Some(&targets), false, &listed_only);
        assert_eq!(out.code, Some(0), "{listed}\n{}", out.describe());

        let both = batch(&[(
            "a.md",
            &format!("# A\n\n[l]({listed}#missing)\n\n[r](real.md#missing)\n"),
        )]);
        let out = check(temp.path(), Some(&targets), false, &both);
        assert_eq!(out.code, Some(1), "{listed}\n{}", out.describe());
        assert_eq!(
            out.stdout.matches("'missing'").count(),
            2,
            "{listed}\n{}",
            out.describe()
        );
    }
}

#[test]
fn a_listed_file_answers_an_extensionless_link_before_a_disk_sibling() {
    // The listed set describes the caller's tree and answers before disk, as
    // it does for MD057: with `guide.markdown` listed, `guide` names it, not a
    // `guide.md` that happens to be on disk. Both link forms must agree.
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("guide.md"), "# Guide\n").unwrap();
    let targets = write_targets(temp.path(), "targets", &["guide.markdown"]);
    for link in ["guide", "/guide"] {
        let input = batch(&[("a.md", &format!("# A\n\n[g]({link}#missing)\n"))]);
        let out = check(temp.path(), Some(&targets), false, &input);
        assert_eq!(out.code, Some(0), "{link}\n{}", out.describe());
        // Control: unlisted, the disk sibling is read and the fragment reported.
        let out = check(temp.path(), None, false, &input);
        assert_eq!(out.code, Some(1), "{link}\n{}", out.describe());
        assert!(out.stdout.contains("'missing'"), "{link}\n{}", out.describe());
    }
}

#[test]
fn a_root_relative_fragment_link_into_a_target_only_file_is_not_reported() {
    // Root-relative links resolve as unchecked under the default
    // absolute-links setting; the listed path must still not be read.
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("other.md"), "# Other\n").unwrap();
    let targets = write_targets(temp.path(), "targets", &["other.md"]);
    let input = batch(&[("a.md", "# A\n\n[o](/other.md#anything)\n")]);
    let out = check(temp.path(), Some(&targets), false, &input);
    assert_eq!(out.code, Some(0), "{}", out.describe());
    // Control: unlisted, the disk copy is read and the fragment reported.
    let out = check(temp.path(), None, false, &input);
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("'anything'"), "{}", out.describe());
}

#[test]
fn a_directory_the_policy_names_is_never_replaced_by_a_disk_file_of_the_same_name() {
    // `other.md/` names a directory to MD057, so MD051 must not read the
    // Markdown file `other.md` from disk, whether the directory is declared
    // (`declared.md/`) or only implied by a listed file (`implied.md/x.pdf`).
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("declared.md"), "# Disk\n").unwrap();
    fs::write(temp.path().join("implied.md"), "# Disk\n").unwrap();
    let targets = write_targets(temp.path(), "targets", &["declared.md/", "implied.md/x.pdf"]);
    // Relative and root-relative links take different lookups; both must agree.
    for root in ["", "/"] {
        let input = batch(&[(
            "a.md",
            &format!("# A\n\n[d]({root}declared.md/#anything)\n\n[i]({root}implied.md/#anything)\n"),
        )]);
        let out = check(temp.path(), Some(&targets), false, &input);
        assert_eq!(out.code, Some(0), "root={root:?}\n{}", out.describe());
        // Control: unlisted, the disk files are read.
        let out = check(temp.path(), None, false, &input);
        assert_eq!(out.code, Some(1), "root={root:?}\n{}", out.describe());
    }
}

#[test]
fn a_batch_document_wins_over_a_declared_directory_of_the_same_path() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["b.md/"]);
    let input = batch(&[
        ("a.md", "# A\n\n[b](b.md#real)\n\n[b](b.md#gone)\n\n[b](/b.md#rooted)\n"),
        ("b.md", "# B\n\n## Real\n"),
    ]);
    for closed_world in [true, false] {
        let out = check(temp.path(), Some(&targets), closed_world, &input);
        assert_eq!(out.code, Some(1), "{}", out.describe());
        assert!(out.stdout.contains("'gone'"), "{}", out.describe());
        assert!(out.stdout.contains("'rooted'"), "{}", out.describe());
        assert!(!out.stdout.contains("'real'"), "{}", out.describe());
    }
}

#[test]
fn targets_are_not_linted_or_counted() {
    let temp = tempfile::tempdir().unwrap();
    // A listed Markdown path with bad content on disk is never read or linted.
    fs::write(temp.path().join("bad.md"), "#Bad heading\n\n\n\n[x](nope.png)\n").unwrap();
    let targets = write_targets(temp.path(), "targets", &["bad.md"]);
    let args = [
        "check",
        "--stdin-batch",
        "--no-cache",
        "--no-config",
        "--stdin-batch-targets",
        &targets,
    ];
    let out = run(temp.path(), &args, &batch(&[("a.md", "# A\n")]));
    assert_eq!(out.code, Some(0), "{}", out.describe());
    assert!(!out.stdout.contains("bad.md"), "{}", out.describe());
    assert!(out.stdout.contains("1 file"), "{}", out.describe());
}

#[test]
fn an_empty_targets_file_is_valid() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("targets"), b"").unwrap();
    let out = check(
        temp.path(),
        Some("targets"),
        true,
        &batch(&[("a.md", "# A\n\n[x](x.png)\n")]),
    );
    assert_eq!(out.code, Some(1), "{}", out.describe());
    assert!(out.stdout.contains("x.png"), "{}", out.describe());
}

#[test]
fn duplicate_entries_are_collapsed() {
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["x.png", "./x.png", "x.png"]);
    let out = check(
        temp.path(),
        Some(&targets),
        true,
        &batch(&[("a.md", "# A\n\n[x](x.png)\n")]),
    );
    assert_eq!(out.code, Some(0), "{}", out.describe());
}

fn assert_tool_error(out: &Outcome, needle: &str) {
    assert_eq!(out.code, Some(2), "{}", out.describe());
    assert!(
        out.stderr.contains("invalid --stdin-batch-targets input"),
        "{}",
        out.describe()
    );
    assert!(out.stderr.contains(needle), "{}", out.describe());
}

#[test]
fn a_missing_targets_file_is_a_tool_error() {
    let temp = tempfile::tempdir().unwrap();
    let out = check(temp.path(), Some("absent"), true, &batch(&[("a.md", "# A\n")]));
    assert_tool_error(&out, "absent");
}

#[test]
fn malformed_targets_are_tool_errors() {
    let temp = tempfile::tempdir().unwrap();
    let input = batch(&[("a.md", "# A\n")]);
    let cases: [(&[u8], &str); 4] = [
        (b"x.png", "NUL"),
        (b"x.png\0\0y.png\0", "empty"),
        (b"\0", "empty"),
        (b"\xff\xfe\0", "UTF-8"),
    ];
    for (bytes, needle) in cases {
        fs::write(temp.path().join("targets"), bytes).unwrap();
        let out = check(temp.path(), Some("targets"), true, &input);
        assert_tool_error(&out, needle);
    }
}

#[test]
fn stdin_as_the_targets_file_is_a_tool_error() {
    let temp = tempfile::tempdir().unwrap();
    let out = check(temp.path(), Some("-"), true, &batch(&[("a.md", "# A\n")]));
    assert_tool_error(&out, "stdin");
}

#[test]
fn targets_without_stdin_batch_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("targets"), b"").unwrap();
    fs::write(temp.path().join("a.md"), "# A\n").unwrap();
    let out = run(
        temp.path(),
        &[
            "check",
            "--no-cache",
            "--no-config",
            "--stdin-batch-targets",
            "targets",
            "a.md",
        ],
        b"",
    );
    assert_eq!(out.code, Some(2), "{}", out.describe());
    assert!(out.stderr.contains("--stdin-batch"), "{}", out.describe());
}

#[test]
fn targets_with_watch_and_without_stdin_batch_exit_instead_of_watching() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("targets"), b"").unwrap();
    fs::write(temp.path().join("a.md"), "# A\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp.path())
        .args([
            "check",
            "--no-config",
            "--stdin-batch-targets",
            "targets",
            "--watch",
            "a.md",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let status = status.expect("rumdl entered watch mode instead of rejecting the arguments");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn a_root_relative_extensionless_link_to_a_listed_file_is_not_checked_against_a_markdown_sibling() {
    // `/guide` names the listed file `guide`, whose headings are unknown; the
    // batch document `guide.md` is a different file.
    let temp = tempfile::tempdir().unwrap();
    let targets = write_targets(temp.path(), "targets", &["guide"]);
    let input = batch(&[
        ("a.md", "# A\n\n[g](/guide#unknown)\n\n[r](guide#unknown)\n"),
        ("guide.md", "# Guide\n"),
    ]);
    for closed_world in [true, false] {
        let out = check(temp.path(), Some(&targets), closed_world, &input);
        assert_eq!(out.code, Some(0), "closed_world={closed_world}\n{}", out.describe());
    }
    // Control: with nothing listed, both links reach `guide.md` and are checked there.
    for closed_world in [true, false] {
        let out = check(temp.path(), None, closed_world, &input);
        assert_eq!(out.code, Some(1), "closed_world={closed_world}\n{}", out.describe());
        assert_eq!(out.stdout.matches("'unknown'").count(), 2, "{}", out.describe());
    }
}
