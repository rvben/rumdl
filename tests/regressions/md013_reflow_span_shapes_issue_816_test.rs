use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

const SENTENCE_PER_LINE: &[&str] = &[
    "MD013.reflow = true",
    "MD013.reflow-mode = \"sentence-per-line\"",
    "MD013.line-length = 500",
];

fn run(dir: &Path, name: &str, subcommand: &str, content: &str) -> Output {
    let file_path = dir.join(name);
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg(subcommand)
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013");
    for setting in SENTENCE_PER_LINE {
        command.arg("-c").arg(setting);
    }

    command.arg(&file_path).output().expect("Failed to execute rumdl")
}

fn fmt(dir: &Path, name: &str, content: &str) -> String {
    let output = run(dir, name, "fmt", content);
    assert!(
        output.status.success(),
        "rumdl fmt failed with status {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(dir.join(name)).unwrap()
}

fn nonempty_line_count(content: &str) -> usize {
    content.lines().filter(|line| !line.trim().is_empty()).count()
}

fn boundary_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("double_tilde", "~~Struck.~~ Next sentence.\n"),
        ("single_star", "*Emph.* Next sentence.\n"),
        ("double_star", "**Bold.** Next sentence.\n"),
        ("plain", "Plain one. Next sentence.\n"),
        ("single_tilde", "~Struck.~ Next sentence.\n"),
        ("triple_star", "***Emphasis.*** Next sentence.\n"),
        ("mixed_nested", "**_Bold ital._** Next sentence.\n"),
        ("four_stars", "****Bold.**** Next.\n"),
    ]
}

#[test]
fn sentence_counts_match_formatted_lines_and_converge() {
    for (name, input) in boundary_cases() {
        let temp = TempDir::new().unwrap();
        let check = run(temp.path(), "input.md", "check", input);
        let stdout = String::from_utf8_lossy(&check.stdout);

        assert_eq!(
            check.status.code(),
            Some(1),
            "{name}: joined input should report exactly one fixable finding; stdout: {stdout}"
        );
        assert!(
            stdout.contains("Line contains 2 sentences (one sentence per line required)"),
            "{name}: counter should report two sentences; stdout: {stdout}"
        );

        let once = fmt(temp.path(), "once.md", input);
        assert_eq!(
            nonempty_line_count(&once),
            2,
            "{name}: counter reported two sentences but fmt produced:\n{once}"
        );

        let clean = run(temp.path(), "clean.md", "check", &once);
        assert_eq!(
            clean.status.code(),
            Some(0),
            "{name}: fmt output should pass check; stdout: {}",
            String::from_utf8_lossy(&clean.stdout)
        );
        assert_eq!(
            fmt(temp.path(), "twice.md", &once),
            once,
            "{name}: second fmt should be byte-identical"
        );
    }
}

#[test]
fn mid_sentence_span_stays_on_one_line() {
    let temp = TempDir::new().unwrap();
    let input = "Text with *emphasis* inside continues.\n";

    assert_eq!(fmt(temp.path(), "mid.md", input), input);
    assert_eq!(run(temp.path(), "check.md", "check", input).status.code(), Some(0));
}

#[test]
fn a_marker_that_opens_no_span_still_ends_the_sentence() {
    // Neither marker opens a span: `*Emph.` has no closer, and `5*3` is
    // arithmetic. A soft break between the two sentences leaves both inside the
    // same paragraph, so the rendering is unchanged either way, and refusing to
    // split would be a boundary the mode was asked to make and did not.
    for (name, input, expected) in [
        ("unmatched", "*Emph. Next sentence.\n", "*Emph.\nNext sentence.\n"),
        ("stray", "Cost is 5*3. Next one.\n", "Cost is 5*3.\nNext one.\n"),
    ] {
        let temp = TempDir::new().unwrap();
        let once = fmt(temp.path(), name, input);

        assert_eq!(once, expected, "{name}: unexpected split");
        assert_eq!(
            run(temp.path(), "clean.md", "check", &once).status.code(),
            Some(0),
            "{name}: check and fmt should agree"
        );
        assert_eq!(fmt(temp.path(), "twice.md", &once), once, "{name}: fmt should converge");
    }
}

#[test]
fn issue_816_reproduction_breaks_after_every_span_shape() {
    let temp = TempDir::new().unwrap();
    let input = "# T\n\n~Struck lead.~ Plain follows.\n\n***Emphasis lead.*** Plain follows.\n\n**_Bold ital lead._** Plain follows.\n\n~~Struck lead.~~ Plain follows.\n";

    assert_eq!(
        fmt(temp.path(), "issue.md", input),
        "# T\n\n~Struck lead.~\nPlain follows.\n\n***Emphasis lead.***\nPlain follows.\n\n**_Bold ital lead._**\nPlain follows.\n\n~~Struck lead.~~\nPlain follows.\n"
    );
}

#[test]
fn intraword_underscore_does_not_hide_sentence_boundaries() {
    let temp = TempDir::new().unwrap();
    let input = "Use snake_case here. _Emphasized text._ Next sentence.\n";
    let check = run(temp.path(), "input.md", "check", input);
    let stdout = String::from_utf8_lossy(&check.stdout);

    assert_eq!(check.status.code(), Some(1));
    assert!(
        stdout.contains("Line contains 3 sentences (one sentence per line required)"),
        "counter should report three sentences; stdout: {stdout}"
    );

    let once = fmt(temp.path(), "once.md", input);
    assert_eq!(nonempty_line_count(&once), 3, "fmt should produce three lines:\n{once}");
    assert_eq!(run(temp.path(), "clean.md", "check", &once).status.code(), Some(0));
}

#[test]
fn code_span_markers_do_not_hide_sentence_boundaries() {
    let temp = TempDir::new().unwrap();
    let input = "Paths may begin with `~/`. Next sentence.\n";
    let once = fmt(temp.path(), "once.md", input);

    assert_eq!(nonempty_line_count(&once), 2, "fmt should produce two lines:\n{once}");
    assert_eq!(run(temp.path(), "clean.md", "check", &once).status.code(), Some(0));
}
