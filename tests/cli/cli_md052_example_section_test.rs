//! MD052 skips undefined references inside an "example section": the lines from a
//! top-level heading titled Output / Example / Output Style / Output Format up to
//! the next top-level heading. The section boundaries must be the document's real
//! headings. Text that only looks like a heading (a `#` line in a code block, an
//! HTML comment or block, a `#hashtag` paragraph) neither starts nor ends one.
//!
//! Each row names the top-level heading lines pulldown-cmark reports, so a row
//! that stops exercising the construct it is about fails on the oracle first.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// 1-indexed start lines of headings outside any list item or blockquote.
fn top_level_heading_lines(content: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(content, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS).into_offset_iter() {
        match event {
            Event::Start(Tag::Item | Tag::BlockQuote(_)) => depth += 1,
            Event::End(TagEnd::Item | TagEnd::BlockQuote(_)) => depth -= 1,
            Event::Start(Tag::Heading { .. }) if depth == 0 => {
                lines.push(content[..range.start].matches('\n').count() + 1);
            }
            _ => {}
        }
    }
    lines
}

fn md052_lines(content: &str) -> Vec<u64> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "--output-format", "json"])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    let warnings: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "unparseable output ({e}): {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    warnings
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["rule"] == "MD052")
        .map(|w| w["line"].as_u64().unwrap())
        .collect()
}

struct Row {
    name: &'static str,
    content: &'static str,
    /// Top-level heading lines according to pulldown-cmark.
    headings: &'static [usize],
    /// Lines MD052 must report.
    reported: &'static [u64],
}

const ROWS: &[Row] = &[
    // Negative control: no example section, the reference is reported.
    Row {
        name: "no section",
        content: "# T\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[3],
    },
    // Positive control: the section hides the reference.
    Row {
        name: "plain section",
        content: "# T\n\n## Example\n\nSee [foo][bar].\n",
        headings: &[1, 3],
        reported: &[],
    },
    Row {
        name: "shell comment in a fence keeps the section",
        content: "# T\n\n## Example\n\n```sh\n# a comment\n```\n\nSee [foo][bar].\n",
        headings: &[1, 3],
        reported: &[],
    },
    Row {
        name: "echo in a fence keeps the section",
        content: "# T\n\n## Example\n\n```sh\necho\n```\n\nSee [foo][bar].\n",
        headings: &[1, 3],
        reported: &[],
    },
    Row {
        name: "Output heading in a fence starts nothing",
        content: "# T\n\n```\n## Output\n```\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[7],
    },
    Row {
        name: "Output heading in an HTML comment starts nothing",
        content: "# T\n\n<!--\n## Output\n-->\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[7],
    },
    Row {
        name: "Output heading in front matter starts nothing",
        content: "---\ntitle: x\n## Output\n---\n\n# T\n\nSee [foo][bar].\n",
        headings: &[6],
        reported: &[8],
    },
    Row {
        name: "hashtag paragraph does not end the section",
        content: "## Example\n\n#hashtag\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
    Row {
        name: "heading-like line in an HTML block does not end the section",
        content: "## Example\n\n<div>\n# not a heading\n</div>\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
    Row {
        name: "setext Example starts a section",
        content: "Example\n-------\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
    Row {
        name: "setext heading ends the section",
        content: "## Example\n\nSee [a][b].\n\nOther\n-----\n\nSee [foo][bar].\n",
        headings: &[1, 5],
        reported: &[8],
    },
    Row {
        name: "closing hashes do not hide the title",
        content: "## Example ##\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
    Row {
        name: "a deeper heading ends the section",
        content: "## Example\n\n### Details\n\nSee [foo][bar].\n",
        headings: &[1, 3],
        reported: &[5],
    },
    Row {
        name: "heading in a list item does not start a section",
        content: "- ## Example\n\n  text\n\nSee [foo][bar].\n",
        headings: &[],
        reported: &[5],
    },
    Row {
        name: "heading in a list item does not end the section",
        content: "## Example\n\n- # x\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
    Row {
        name: "heading in a blockquote does not start a section",
        content: "> ## Example\n\nSee [foo][bar].\n",
        headings: &[],
        reported: &[3],
    },
    Row {
        name: "heading in a blockquote does not end the section",
        content: "## Example\n\n> # x\n\nSee [foo][bar].\n",
        headings: &[1],
        reported: &[],
    },
];

#[test]
fn example_sections_follow_parsed_headings() {
    for row in ROWS {
        assert_eq!(
            top_level_heading_lines(row.content),
            row.headings,
            "oracle disagrees with the row's heading lines: {}",
            row.name
        );
        assert_eq!(
            md052_lines(row.content),
            row.reported,
            "{}: {:?}",
            row.name,
            row.content
        );
    }
}

#[test]
fn check_fix_leaves_example_section_documents_unchanged() {
    // MD052 has no fix; a document it warns about must come back byte-identical.
    let content = "# T\n\n```text\n## Output\n```\n\nSee [foo][bar].\n";
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "--fix"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), content);
    assert_eq!(md052_lines(content), [7], "second run reports the same line");
}
