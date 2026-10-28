//! MD087 through the CLI.
//!
//! The rule reports from a whole-run report rather than from its own `check`, so
//! the paths that assemble that report are worth pinning at the command level:
//! opt-in selection, what `fmt` counts, and what `fmt` leaves in the file.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

const UNUSED: &str = "# Title\n\nShort line. <!-- rumdl-disable-line MD013 -->\n";

fn run(content: &str, args: &[&str]) -> (TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.md"), content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(args)
        .args(["--no-cache", "--no-config", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (dir, stdout)
}

fn md087_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|line| line.contains("[MD087]")).collect()
}

#[test]
fn md087_is_off_until_it_is_enabled() {
    let (_dir, default_run) = run(UNUSED, &["check"]);
    assert!(
        md087_lines(&default_run).is_empty(),
        "MD087 is opt-in, got:\n{default_run}"
    );

    let (_dir, enabled_run) = run(UNUSED, &["check", "--extend-enable", "MD087"]);
    assert_eq!(
        md087_lines(&enabled_run),
        ["test.md:3:13: [MD087] Unused disable-line comment: MD013"],
        "full output:\n{enabled_run}"
    );
}

#[test]
fn a_comment_that_suppresses_a_finding_is_silent() {
    let used = format!("# Title\n\n{} <!-- rumdl-disable-line MD013 -->\n", "word ".repeat(30));
    let (_dir, stdout) = run(&used, &["check", "--extend-enable", "MD087"]);
    assert!(md087_lines(&stdout).is_empty(), "full output:\n{stdout}");
}

/// `fmt` reports the warnings that survive the fix pass. It computes them by
/// re-linting the fixed content, which has to go through the same entry point as
/// the first pass or a rule reporting from the run's report is dropped: the
/// warning prints and the summary claims the file is clean.
#[test]
fn fmt_counts_the_finding_it_prints() {
    let (_dir, stdout) = run(UNUSED, &["fmt", "--extend-enable", "MD087"]);

    assert_eq!(md087_lines(&stdout).len(), 1, "full output:\n{stdout}");
    assert!(
        stdout.contains("Issues: Found 1 issue in 1 file"),
        "the printed warning must reach the summary, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("No issues found"),
        "summary contradicts the warning above it:\n{stdout}"
    );
}

#[test]
fn a_finding_takes_its_place_in_document_order() {
    // The rule reports after every other rule has run, so its findings only reach
    // the reader in the right place if they go through the same sort as the rest.
    let content = "Short line. <!-- rumdl-disable-line MD013 -->\n\n\nlast\n";
    let (_dir, stdout) = run(content, &["check", "-e", "MD012,MD013,MD087"]);
    let rules: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split(": [").nth(1))
        .filter_map(|rest| rest.split(']').next())
        .collect();

    assert_eq!(rules, ["MD087", "MD012"], "full output:\n{stdout}");
}

#[test]
fn fmt_leaves_the_comment_in_place() {
    // Removing a comment is a content decision the author owns: the line it
    // protects may be about to come back.
    let (dir, _stdout) = run(UNUSED, &["fmt", "--extend-enable", "MD087"]);
    assert_eq!(fs::read_to_string(dir.path().join("test.md")).unwrap(), UNUSED);
}

#[test]
fn heading_suppression_is_used_with_all_rules_enabled_issue_973() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.md");
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[global]\nenable = [\"ALL\"]\n[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n",
    )
    .unwrap();

    for content in [
        "# asdf <!-- rumdl-disable-line heading-capitalization -->\n",
        "# Example\n\n```markdown\n# asdf <!-- rumdl-disable-line heading-capitalization -->\n```\n",
    ] {
        for command in [vec!["check"], vec!["check", "--fix"], vec!["fmt"]] {
            fs::write(&path, content).unwrap();
            let output = Command::new(rumdl_bin())
                .current_dir(dir.path())
                .args(&command)
                .args(["--no-cache", "test.md"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{command:?}: {content:?}: {output:?}");
            assert!(
                md087_lines(&String::from_utf8_lossy(&output.stdout)).is_empty(),
                "{output:?}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }
}

#[test]
fn outer_disable_gets_credit_only_when_embedded_linting_runs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let source = "<!-- rumdl-disable-file MD063 -->\n\n```markdown\n# asdf\n```\n";
    fs::write(&path, source).unwrap();
    fs::write(dir.path().join(".rumdl.toml"),
        "[global]\nenable = [\"MD063\", \"MD087\"]\n[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n"
    ).unwrap();
    for args in [vec!["check"], vec!["check", "--fix"], vec!["fmt"]] {
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args(args)
            .args(["--no-cache", "doc.md"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            md087_lines(&String::from_utf8_lossy(&output.stdout)).is_empty(),
            "{output:?}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "--no-code-block-tools", "doc.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(md087_lines(&String::from_utf8_lossy(&output.stdout)).len(), 1);
}

#[test]
fn doc_comment_unused_disables_are_reported_without_rewriting_rust() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    let content = "/// Short <!-- rumdl-disable-line MD013 -->\nfn example() {}\n";
    fs::write(&path, content).unwrap();
    for command in ["check", "fmt"] {
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args([command, "--no-cache", "--isolated", "--enable", "MD013,MD087", "lib.rs"])
            .output()
            .unwrap();
        assert_eq!(
            md087_lines(&String::from_utf8_lossy(&output.stdout)).len(),
            1,
            "{output:?}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }
}

#[test]
fn raw_suppression_evidence_never_enables_cli_fixes() {
    for (rule, options, body) in [
        ("MD001", "", "# Heading\n\n### Skipped level\n"),
        ("MD040", "", "```\ncode\n```\n"),
        ("MD022", "", "# Heading\nParagraph\n"),
        ("MD022", "[MD022]\nallowed-at-start = false\n", "# Heading\nParagraph\n"),
        (
            "MD022",
            "[MD022]\nlines-above-mode = 'exact'\nlines-above = 1\nlines-below = 1\n",
            "# Heading\n\n\n\n## Heading\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(
            dir.path().join(".rumdl.toml"),
            format!("[global]\nenable = [\"{rule}\", \"MD087\"]\n{options}"),
        )
        .unwrap();
        for directive in ["disable", "disable-file"] {
            let source = format!("<!-- rumdl-{directive} {rule} -->\n{body}");
            for args in [vec!["check"], vec!["check", "--fix"], vec!["fmt"]] {
                fs::write(&path, &source).unwrap();
                let output = Command::new(rumdl_bin())
                    .current_dir(dir.path())
                    .args(&args)
                    .args(["--no-cache", "doc.md"])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{rule}, {options}, {directive}, {args:?}: {output:?}"
                );
                assert_eq!(fs::read_to_string(&path).unwrap(), source);
            }
        }
    }
}
