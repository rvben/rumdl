//! MD013 reflow against the real binary: fenced code inside a footnote.
//!
//! The code stays as written and the prose around it is reflowed. The oracle is
//! pulldown-cmark: the text of every code block must be the same before and
//! after `fmt`, and a second run must change nothing.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn fmt(doc: &str) -> String {
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(
        &config,
        "[global]\nenable = [\"MD013\"]\n[MD013]\nreflow = true\nline-length = 40\n",
    )
    .unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["fmt", "--config", config.to_str().unwrap(), "--no-cache"])
        .arg(&path)
        .output()
        .expect("run rumdl");
    assert!(matches!(output.status.code(), Some(0 | 1)), "{output:?}");
    fs::read_to_string(&path).unwrap()
}

/// The text of every code block, fenced or indented, in document order.
fn code_blocks(doc: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for event in Parser::new_ext(doc, Options::ENABLE_FOOTNOTES) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => current = Some(String::new()),
            Event::Text(text) => {
                if let Some(block) = current.as_mut() {
                    block.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => blocks.extend(current.take()),
            _ => {}
        }
    }
    blocks
}

const PROSE: &str = "prose prose prose prose prose prose prose prose prose prose prose prose";
const LONG: &str = "keep   these   spaces   exactly and this line is long enough to wrap if treated as prose";

fn footnote(block: &str) -> String {
    format!("# T\n\nText.[^1]\n\n[^1]: {PROSE}\n\n{block}\n    {PROSE}\n")
}

#[test]
fn footnote_code_survives_a_fence_like_line_inside_it() {
    for (open, inner, close) in [
        ("~~~text", "```", "~~~"),
        ("~~~~", "~~~", "~~~~"),
        ("````", "```", "````"),
        ("```", "~~~", "```"),
        ("```", "```text", "```"),
    ] {
        let input = footnote(&format!("    {open}\n    {inner}\n    {LONG}\n    {close}\n"));
        let output = fmt(&input);
        assert_eq!(code_blocks(&output), code_blocks(&input), "{input}\n=>\n{output}");
        assert!(output.contains(&format!("    {LONG}\n")), "{output}");
        // Positive control: the prose on both sides of the block was reflowed.
        assert!(!output.contains(&format!("    {PROSE}\n")), "{output}");
        assert!(
            output.lines().filter(|l| l.starts_with("    prose")).count() >= 2,
            "{output}"
        );
        assert_eq!(fmt(&output), output, "second run changed the file");
    }
}

#[test]
fn the_spaces_in_the_issue_input_are_kept_and_the_next_paragraph_is_reflowed() {
    let input = footnote(&format!("    ~~~text\n    ```\n    {LONG}\n    ~~~\n"));
    let expected = format!(
        "# T\n\nText.[^1]\n\n[^1]: prose prose prose prose prose\n    prose prose prose prose prose\n    prose prose\n\n    ~~~text\n    ```\n    {LONG}\n    ~~~\n\n    prose prose prose prose prose\n    prose prose prose prose prose\n    prose prose\n"
    );
    assert_eq!(fmt(&input), expected);
}

#[test]
fn an_unclosed_footnote_fence_is_code_to_the_end() {
    let input = format!("# T\n\nText.[^1]\n\n[^1]: {PROSE}\n\n    ~~~text\n    ```\n    {LONG}\n");
    let output = fmt(&input);
    assert_eq!(code_blocks(&output), code_blocks(&input), "{output}");
    assert!(output.contains(&format!("    {LONG}\n")), "{output}");
}

#[test]
fn a_whitespace_only_line_in_footnote_code_is_kept() {
    let input = footnote("    ```\n    one\n        \n    two\n    ```\n");
    let output = fmt(&input);
    assert_eq!(code_blocks(&output), code_blocks(&input), "{output}");
    assert!(output.contains("    one\n        \n    two\n"), "{output:?}");
}

#[test]
fn a_footnote_html_block_is_kept() {
    let input = footnote(&format!("    <pre>\n    ~~~\n    {LONG}\n    ~~~\n    </pre>\n"));
    let output = fmt(&input);
    assert!(output.contains(&format!("    ~~~\n    {LONG}\n    ~~~\n")), "{output}");
}
