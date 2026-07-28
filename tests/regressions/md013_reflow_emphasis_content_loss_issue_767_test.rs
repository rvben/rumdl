//! Regression tests for issues #767 and #768: MD013 reflow deleted words when a
//! sentence boundary fell inside an emphasis span.
//!
//! Reflow reopened the span on each new line, and the marker it wrote consumed
//! the space that separated the sentences, so the surrounding words were
//! swallowed. A quoted, emphasized paragraph could lose half its text.
//!
//! Every test runs the real `rumdl fmt` pipeline. The core assertion is the
//! invariant reflow must never break: it chooses where lines end, so the output
//! holds exactly the input's non-whitespace characters, in order.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SENTENCE_PER_LINE: &[&str] = &[
    "MD013.reflow = true",
    "MD013.reflow-mode = \"sentence-per-line\"",
    "MD013.line-length = 0",
];

const SEMANTIC_LINE_BREAKS: &[&str] = &[
    "MD013.reflow = true",
    "MD013.reflow-mode = \"semantic-line-breaks\"",
    "MD013.line-length = 80",
];

/// Run `rumdl fmt` on `content` and return the formatted file.
fn fmt(dir: &Path, name: &str, settings: &[&str], content: &str) -> String {
    let file_path = dir.join(name);
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.arg("fmt").arg("--no-config").arg("--no-cache");
    for setting in settings {
        command.arg("-c").arg(setting);
    }

    let output = command.arg(&file_path).output().expect("Failed to execute rumdl");
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// The document's content: every non-whitespace character except the blockquote
/// markers, which are structure. Splitting a quoted paragraph legitimately adds
/// a `>` prefix to each new line.
fn content_of(markdown: &str) -> String {
    markdown
        .lines()
        .map(|line| line.trim_start().trim_start_matches(['>', ' ', '\t']))
        .flat_map(|line| line.chars())
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Format `input` twice, asserting reflow changed only whitespace and converged.
fn assert_reflow_preserves(name: &str, settings: &[&str], input: &str) -> String {
    let temp = TempDir::new().unwrap();
    let once = fmt(temp.path(), "a.md", settings, input);
    let twice = fmt(temp.path(), "b.md", settings, &once);

    assert_eq!(
        content_of(input),
        content_of(&once),
        "{name}: reflow changed content, not just line breaks.\ninput:\n{input}\ngot:\n{once}"
    );
    assert_eq!(
        once, twice,
        "{name}: reflow must be idempotent.\nfirst:\n{once}\nsecond:\n{twice}"
    );
    once
}

/// The reporter's table from #767: an emphasized, quoted paragraph in every
/// marker style and every container. Each one lost words before the fix.
#[test]
fn test_emphasis_spanning_sentences_keeps_every_word() {
    let cases: &[(&str, &str)] = &[
        (
            "plain",
            r#""The reviewers called it "a solid contribution." The paper was accepted.""#,
        ),
        (
            "italic",
            "_The paper was accepted without revision. Publication follows in March._",
        ),
        (
            "italic_parens",
            "_The paper was accepted (after one revision.) Publication follows in March._",
        ),
        (
            "italic_code",
            "_The paper was accepted `after one revision.` Publication follows in March._",
        ),
        (
            "italic_quote",
            r#"_The reviewers called it "a solid contribution." The paper was accepted._"#,
        ),
        (
            "italic_single_quote",
            "_The reviewers called it 'a solid contribution.' The paper was accepted._",
        ),
        (
            "italic_leading_quote",
            r#"_"A solid contribution." The paper was accepted without revision._"#,
        ),
        (
            "italic_nested_quotes",
            r#"_"The reviewers called it "a solid contribution." The paper was accepted."_"#,
        ),
        (
            "bold_nested_quotes",
            r#"**"The reviewers called it "a solid contribution." The paper was accepted."**"#,
        ),
        (
            "three_sentences",
            r#"_The reviewers called it "a solid contribution." The paper was accepted. Publication follows in March._"#,
        ),
        (
            "two_spans",
            r#"_"She said "yes." The vote carried."_ and _"He said "no." The motion failed."_"#,
        ),
        (
            "list_item",
            r#"- _"The reviewers called it "a solid contribution." The paper was accepted."_"#,
        ),
        (
            "nested_list",
            "- outer\n  - _\"The reviewers called it \"a solid contribution.\" The paper was accepted.\"_",
        ),
        (
            "blockquote",
            r#"> _"The reviewers called it "a solid contribution." The paper was accepted."_"#,
        ),
        (
            "strikethrough",
            r#"~~"The reviewers called it "a solid contribution." The paper was accepted."~~"#,
        ),
        (
            "underscore_bold",
            r#"__"The reviewers called it "a solid contribution." The paper was accepted."__"#,
        ),
        (
            "asterisk_italic",
            r#"*"The reviewers called it "a solid contribution." The paper was accepted."*"#,
        ),
    ];

    for (name, paragraph) in cases {
        let input = format!("# Notes\n\n{paragraph}\n");
        assert_reflow_preserves(name, SEMANTIC_LINE_BREAKS, &input);
    }
}

/// The worst case from #767: two emphasized spans on one line, each with nested
/// quotes. Reflow used to reduce this to a fragment of its words.
#[test]
fn test_two_emphasis_spans_on_one_line() {
    let input = "# Notes\n\n_\"She said \"yes.\" The vote carried.\"_ and _\"He said \"no.\" The motion failed.\"_\n";
    let fixed = assert_reflow_preserves("two_spans", SEMANTIC_LINE_BREAKS, input);

    for word in ["She said", "The vote carried", "He said", "The motion failed"] {
        assert!(fixed.contains(word), "'{word}' must survive reflow. got:\n{fixed}");
    }
}

/// #768: a bold span inside a list item, followed by two plain sentences. The
/// trailing sentence disappeared entirely.
#[test]
fn test_bold_span_in_list_item_keeps_trailing_sentences() {
    let input = "- **A \"Widget vs. Gadget: which do I pick?\" guide** - decision-oriented. \
                 Heuristic: pick Widget for one-off jobs, Gadget for repeat jobs. \
                 This sentence must survive.\n";
    let fixed = assert_reflow_preserves("list_bold_quote", SENTENCE_PER_LINE, input);

    assert!(
        fixed.contains("This sentence must survive."),
        "the trailing sentence must survive reflow. got:\n{fixed}"
    );
}

/// #768: a bold span spanning four sentences inside a blockquote. Each sentence
/// gets its own quoted line, and the span still opens and closes exactly once.
#[test]
fn test_bold_span_in_blockquote_splits_without_reopening() {
    let input = "> **Alpha. Beta. Gamma. Delta.**\n";
    let fixed = assert_reflow_preserves("blockquote_bold", SENTENCE_PER_LINE, input);

    assert_eq!(fixed, "> **Alpha.\n> Beta.\n> Gamma.\n> Delta.**\n");
    assert_eq!(fixed.matches("**").count(), 2, "the span must not be reopened per line");
}

/// #768: reflow must not indent the lines it creates.
#[test]
fn test_split_lines_are_not_indented() {
    let input = "**Choosing a type.**\n\
                 Ask one question: *does this affect what users receive?*\n\
                 Yes selects one type; no selects the other.\n";
    let fixed = assert_reflow_preserves("leading_spaces", SENTENCE_PER_LINE, input);

    for line in fixed.lines() {
        assert!(
            !line.starts_with(' '),
            "reflow must not indent a line it created. got:\n{fixed}"
        );
    }
}
