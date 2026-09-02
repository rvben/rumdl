//! Regression tests for issue #811: MD013 reflow joined a sentence that ended
//! inside an emphasis span onto the sentence following it.
//!
//! `require-sentence-capital` reads the first character after the terminator.
//! A code span starts on a backtick, so it held the sentence open and the
//! sentence after it was absorbed. The joined line then exceeded `line-length`,
//! which the same rule reported as a line-length violation carrying no fix —
//! `fmt` produced output `check` rejects, and a second `fmt` could not repair it.
//!
//! The other half of the issue, where the line after the span opens on a
//! lowercase word, is not covered here: nothing in the source separates such a
//! span from a label, so it stays with `require-sentence-capital`.
//!
//! The assertion that matters is that invariant, not the exact break: `fmt`
//! output must be clean under `check`, and formatting must converge.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SENTENCE_PER_LINE: &[&str] = &[
    "MD013.reflow = true",
    "MD013.reflow-mode = \"sentence-per-line\"",
    "MD013.line-length = 120",
];

/// Run one rumdl subcommand over `content` and return its stdout, leaving the
/// formatted file behind for the caller to read.
fn run(dir: &Path, name: &str, subcommand: &str, content: &str) -> String {
    let file_path = dir.join(name);
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.arg(subcommand).arg("--no-config").arg("--no-cache");
    for setting in SENTENCE_PER_LINE {
        command.arg("-c").arg(setting);
    }

    let output = command.arg(&file_path).output().expect("Failed to execute rumdl");
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl {subcommand} should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Run `rumdl fmt` on `content` and return the formatted file.
fn fmt(dir: &Path, name: &str, content: &str) -> String {
    run(dir, name, "fmt", content);
    fs::read_to_string(dir.join(name)).unwrap()
}

/// The MD013 findings `rumdl check` reports for `content`.
fn md013_findings(dir: &Path, name: &str, content: &str) -> Vec<String> {
    run(dir, name, "check", content)
        .lines()
        .filter(|line| line.contains("[MD013]"))
        .map(str::to_string)
        .collect()
}

/// The document's non-whitespace characters, in order. Reflow chooses where
/// lines end, never which characters the document holds.
fn content_of(markdown: &str) -> String {
    markdown.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The reporter's paragraph: a bold lead-in, then a sentence opening with a code
/// span. Joining the two produced a 158-character line.
const REPORTED: &str = "\
**An enum column may not be an atom a keyset scan resumes from.**
`ORDER BY` would follow the declaration index while the keyset predicate followed the value.
The two disagree, and the page skips rows.
";

/// Every shape #811 lists whose next line opens on a code span, plus the
/// trailing code span with no text element after it to re-split the line.
fn cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("bold_then_code", REPORTED),
        (
            "italic_then_code",
            "*An enum column may not be an atom a keyset scan resumes from.*\n`ORDER BY` would follow the declaration index while the keyset predicate followed the value.\nThe two disagree, and the page skips rows.\n",
        ),
        (
            "strikethrough_then_code",
            "~~An enum column may not be an atom a keyset scan resumes from.~~\n`ORDER BY` would follow the declaration index while the keyset predicate followed the value.\nThe two disagree, and the page skips rows.\n",
        ),
        (
            "underscore_bold_then_code",
            "__An enum column may not be an atom a keyset scan resumes from.__\n`ORDER BY` would follow the declaration index while the keyset predicate followed the value.\nThe two disagree, and the page skips rows.\n",
        ),
        (
            "bold_question_then_code",
            "**Does an enum column serve as an atom a keyset scan resumes from?**\n`ORDER BY` would follow the declaration index while the keyset predicate followed the value.\nThe two disagree, and the page skips rows.\n",
        ),
        (
            "bold_then_uppercase",
            "**An enum column may not be an atom a keyset scan resumes from.**\nOrder by would follow the declaration index while the keyset predicate followed the value.\nThe two disagree, and the page skips rows.\n",
        ),
        (
            "bold_then_trailing_code",
            "**An enum column may not be an atom that a keyset scan resumes from, and the planner cannot fix that.**\n`ORDER BY key, id`\n",
        ),
    ]
}

#[test]
fn test_issue_811_conforming_input_survives_fmt_untouched() {
    for (name, input) in cases() {
        let temp = TempDir::new().unwrap();

        assert_eq!(
            md013_findings(temp.path(), "reported.md", input),
            Vec::<String>::new(),
            "{name}: input is already one sentence per line, so it should report nothing"
        );
        assert_eq!(fmt(temp.path(), "reported.md", input), input, "{name}: fmt rewrote it");
    }
}

#[test]
fn test_issue_811_fmt_output_is_clean_under_check() {
    for (name, input) in cases() {
        let temp = TempDir::new().unwrap();
        let once = fmt(temp.path(), "a.md", input);
        let twice = fmt(temp.path(), "b.md", &once);

        assert_eq!(
            content_of(input),
            content_of(&once),
            "{name}: reflow changed content, not just line breaks.\ninput:\n{input}\ngot:\n{once}"
        );
        assert_eq!(
            once, twice,
            "{name}: reflow must be idempotent.\nfirst:\n{once}\nsecond:\n{twice}"
        );
        assert_eq!(
            md013_findings(temp.path(), "c.md", &once),
            Vec::<String>::new(),
            "{name}: fmt output must be clean under check.\ngot:\n{once}"
        );
    }
}

/// The same paragraphs written on one line: `fmt` has to break them, and the
/// break has to land where `check` counts a sentence, or the finding never
/// clears.
#[test]
fn test_issue_811_joined_input_is_broken_and_converges() {
    for (name, input) in cases() {
        let temp = TempDir::new().unwrap();
        let joined = format!("{}\n", input.trim_end().replace('\n', " "));

        let once = fmt(temp.path(), "a.md", &joined);
        assert_eq!(
            content_of(&joined),
            content_of(&once),
            "{name}: reflow changed content.\ninput:\n{joined}\ngot:\n{once}"
        );
        assert_eq!(
            once,
            fmt(temp.path(), "b.md", &once),
            "{name}: reflow must be idempotent"
        );
        assert_eq!(
            md013_findings(temp.path(), "c.md", &once),
            Vec::<String>::new(),
            "{name}: fmt output must be clean under check.\ngot:\n{once}"
        );
    }
}

/// A sentence boundary *inside* a span still breaks in place, without closing
/// and reopening the markers, which is what #767 and #768 asked for.
#[test]
fn test_issue_811_boundary_inside_span_still_breaks_in_place() {
    let temp = TempDir::new().unwrap();

    assert_eq!(
        fmt(temp.path(), "a.md", "**First. Second.**\n"),
        "**First.\nSecond.**\n"
    );
    assert_eq!(
        fmt(temp.path(), "b.md", "**First. Second.** and more text.\n"),
        "**First.\nSecond.** and more text.\n"
    );
}

/// A span closing mid-sentence is the bolded-command idiom, not a boundary.
/// These are the shapes the narrow rule exists to leave alone.
#[test]
fn test_issue_811_span_closing_mid_sentence_is_left_alone() {
    let unchanged: &[&str] = &[
        "Click **Save.** then restart.\n",
        "Select the Explorer view in the Activity Bar, and select the **New File...** button to create a file.\n",
        "The **code .** command opened VS Code in the current working folder.\n",
        "**Tip:** Start with **Dev Containers: Add Dev Container Configuration Files...** in the Command Palette.\n",
        "Steps: 1. `init` the repo, 2. `build` it.\n",
        "Reads them, e.g. `.npmrc` holds them.\n",
    ];

    for input in unchanged {
        let temp = TempDir::new().unwrap();
        assert_eq!(fmt(temp.path(), "a.md", input), *input, "fmt rewrote: {input}");
        assert_eq!(
            md013_findings(temp.path(), "b.md", input),
            Vec::<String>::new(),
            "check reported: {input}"
        );
    }
}
