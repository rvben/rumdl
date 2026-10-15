//! MD003 judges headings inside blockquotes like any other heading, and leaves
//! them alone under `--fix`: the warning stays, the document does not change.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares, per heading, the containers it is nested in.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Per heading in document order: its level, its text, and the list, item and
/// blockquote events it is nested in, outermost first.
fn headings(content: &str) -> Vec<(u8, String, Vec<&'static str>)> {
    let mut path: Vec<&'static str> = Vec::new();
    let mut out = Vec::new();
    let mut current: Option<(u8, String, Vec<&'static str>)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::Heading { level, .. }) => current = Some((level as u8, String::new(), path.clone())),
            Event::Text(text) | Event::Code(text) => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => out.extend(current.take()),
            _ => {}
        }
    }
    out
}

/// Runs `rumdl check` on `content` under an MD003-only config with `style`.
/// Returns the command output and the document afterwards.
fn run(content: &str, style: &str, fix: bool) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config = dir.path().join("rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(
        &config,
        format!("[global]\nenable = [\"MD003\"]\n\n[MD003]\nstyle = \"{style}\"\n"),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["check", "--no-cache", "--config"]).arg(&config);
    if fix {
        command.arg("--fix");
    }
    let output = command.arg(&path).output().expect("run rumdl check");
    (output, fs::read_to_string(&path).unwrap())
}

fn md003_lines(output: &Output) -> Vec<usize> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("MD003"))
        .filter_map(|line| line.split(':').nth(1)?.trim().parse().ok())
        .collect()
}

#[test]
fn issue_input_reports_the_quoted_atx_heading() {
    let src = "Top\n===\n\nNext\n====\n\n> ## Quote atx\n";
    let (output, _) = run(src, "consistent", false);
    assert_eq!(md003_lines(&output), vec![7], "{output:?}");

    // Each style has one heading and a tie goes to ATX.
    let src = "Top\n===\n\n> ## Quote atx\n>\n> Setext in quote\n> ---\n";
    let (output, _) = run(src, "consistent", false);
    assert_eq!(md003_lines(&output), vec![1, 6], "{output:?}");
}

#[test]
fn quoted_setext_heading_is_reported_under_an_atx_document() {
    let src = "# Top\n\n> Setext in quote\n> ---\n";
    let (output, _) = run(src, "consistent", false);
    assert_eq!(md003_lines(&output), vec![3], "{output:?}");
}

#[test]
fn document_with_only_a_quoted_heading_is_not_skipped() {
    let (output, _) = run("> # H\n", "setext", false);
    assert_eq!(md003_lines(&output), vec![1], "{output:?}");
}

#[test]
fn consistent_document_is_clean() {
    for src in [
        "# A\n\n> ## B\n\n> > ### C\n",
        "A\n=\n\n> B\n> -\n",
        "- item\n\n  > ## B\n\n# A\n",
    ] {
        let (output, _) = run(src, "consistent", false);
        assert!(md003_lines(&output).is_empty(), "{src:?}: {output:?}");
    }
}

#[test]
fn fix_leaves_quoted_headings_and_keeps_fixing_the_rest() {
    let src = "# Top\n\n> Quoted\n> ---\n\nPlain\n---\n";
    let (first, fixed) = run(src, "atx", true);
    assert_eq!(fixed, "# Top\n\n> Quoted\n> ---\n\n## Plain\n", "{first:?}");
    let (_, again) = run(&fixed, "atx", true);
    assert_eq!(again, fixed, "second --fix changed the document");
    let (check, _) = run(&fixed, "atx", false);
    assert_eq!(md003_lines(&check), vec![3], "the quoted heading is still reported");
}

/// Container shapes a quoted heading can sit in, with headings in the other
/// style beside them.
const CORPUS: &[&str] = &[
    "> # H\n",
    "> H\n> =\n",
    "> ## H ##\n",
    "> > H\n> > -\n",
    "> > ## H\n",
    "- x\n\n  > ## H\n",
    "- x\n\n  > H\n  > ---\n",
    "- > H\n  > ---\n",
    "> - # H\n",
    "> - x\n>\n>   ## H\n",
    "> H\nlazy\n> ---\n",
    "> H\n---\n",
    "> x\n\n> H\n> ---\n\nPlain\n===\n",
    "1. x\n\n   > ## H ##\n\n# Top\n",
    "> ```\n> # not a heading\n> ```\n",
];

#[test]
fn fix_never_changes_the_structure_of_quoted_headings() {
    for src in CORPUS {
        for style in ["atx", "atx-closed", "setext", "setext-with-atx", "consistent"] {
            let (_, fixed) = run(src, style, true);
            let before = headings(src);
            let after = headings(&fixed);
            let shape =
                |hs: &[(u8, String, Vec<&'static str>)]| hs.iter().map(|h| (h.0, h.2.clone())).collect::<Vec<_>>();
            assert_eq!(shape(&before), shape(&after), "{style} {src:?} -> {fixed:?}");
            let quoted = |hs: &[(u8, String, Vec<&'static str>)]| hs.iter().filter(|h| h.2.contains(&"quote")).count();
            assert_eq!(quoted(&before), quoted(&after), "{style} {src:?} -> {fixed:?}");
            let (_, again) = run(&fixed, style, true);
            assert_eq!(again, fixed, "{style} {src:?}: second --fix changed the document");
        }
    }
}

#[test]
fn quoted_headings_in_every_shape_are_reported_when_they_disagree() {
    // Oracle control: pulldown-cmark sees a quoted heading in each of these.
    for src in ["> # H\n", "> H\n> =\n", "- x\n\n  > ## H\n", "> - # H\n", "> > ## H\n"] {
        assert!(
            headings(src).iter().any(|h| h.2.contains(&"quote")),
            "{src:?}: oracle sees no quoted heading"
        );
    }
    for (src, style, line) in [
        ("> # H\n", "setext", 1),
        ("> H\n> =\n", "atx", 1),
        ("- x\n\n  > ## H\n", "setext", 3),
        ("> - # H\n", "setext", 1),
        ("> > ## H\n", "atx-closed", 1),
    ] {
        let (output, _) = run(src, style, false);
        assert_eq!(md003_lines(&output), vec![line], "{style} {src:?}: {output:?}");
    }
}
