//! Regression coverage for issue #866: a line holding nothing but an HTML
//! comment separates the blocks around it, so the rules that require blank lines
//! around a construct must not report it as missing one.
//!
//! MD022, MD031, MD032 and MD058 each had their own blank test and they did not
//! agree: a comment between two constructs was invisible to one rule, transparent
//! to another and a paragraph to a third. Where a rule reported, its fix inserted
//! a blank line, and for a directive comment (`<!-- prettier-ignore -->`, a
//! generator's own trigger) adjacency is the meaning, so the rewrite turned the
//! directive off.
//!
//! Every silent row below is measured against markdownlint, whose `isBlankLine`
//! convention this is. The controls at the bottom are what keeps the change from
//! reading as "the rule stopped working": a construct written directly under a
//! paragraph is still reported by each of the four rules, and a comment sharing
//! its line with prose leaves that line a paragraph.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// The four rules under test, enabled alone so nothing else can speak.
const RULES: &str = "MD022,MD031,MD032,MD058";

/// Rule ids reported for `content`, sorted and deduplicated.
fn rules_reported(content: &str) -> Vec<String> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("document.md");
    fs::write(&path, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--no-config", "--enable", RULES])
        .arg(&path)
        .output()
        .expect("rumdl check runs");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut found: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let start = line.find("[MD")?;
            let end = line[start..].find(']')? + start;
            Some(line[start + 1..end].to_string())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The document as `rumdl fmt` leaves it.
fn formatted(content: &str) -> String {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("document.md");
    fs::write(&path, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "--no-config", "--enable", RULES])
        .arg(&path)
        .output()
        .expect("rumdl fmt runs");
    assert!(output.status.success() || output.status.code() == Some(1));
    fs::read_to_string(&path).unwrap()
}

const TABLE: &str = "| a | b |\n| - | - |\n| 1 | 2 |";

/// (label, document) pairs where a comment-only line does the separating.
fn comment_separated_documents() -> Vec<(String, String)> {
    let mut documents = Vec::new();
    let mut push = |label: &str, document: String| documents.push((label.to_string(), document));

    // MD031: fenced code blocks
    push("fence, comment above", "Text.\n<!-- c -->\n```py\nx\n```\n".to_string());
    push(
        "fence, comment below",
        "Text.\n\n```py\nx\n```\n<!-- c -->\nTail.\n".to_string(),
    );
    push(
        "fence, comment both sides",
        "Text.\n<!-- c -->\n```py\nx\n```\n<!-- c -->\nTail.\n".to_string(),
    );
    push(
        "fence, multi-line comment above",
        "Text.\n\n<!--\nc\n-->\n```py\nx\n```\n".to_string(),
    );
    push(
        "fence, comment inside a blockquote",
        "> Text.\n> <!-- c -->\n> ```py\n> x\n> ```\n".to_string(),
    );

    // MD022: headings
    push(
        "heading, comment above",
        "Text.\n<!-- c -->\n## After\n\nTail.\n".to_string(),
    );
    push("heading, comment below", "## H\n<!-- c -->\nText.\n".to_string());
    push(
        "heading, comment both sides",
        "Text.\n<!-- c -->\n## H\n<!-- c -->\nText.\n".to_string(),
    );
    push(
        "heading, multi-line comment above",
        "Text.\n\n<!--\nc\n-->\n## H\n\nTail.\n".to_string(),
    );
    push(
        "heading, comment inside a blockquote",
        "> Text.\n> <!-- c -->\n> ## H\n>\n> Tail.\n".to_string(),
    );

    // MD032: lists
    push("list, comment above", "Text.\n<!-- c -->\n- a\n\nTail.\n".to_string());
    push("list, comment below", "- a\n<!-- c -->\n## H\n\nTail.\n".to_string());
    push(
        "list, comment both sides",
        "Text.\n<!-- c -->\n- a\n<!-- c -->\n## H\n\nTail.\n".to_string(),
    );
    push(
        "list, multi-line comment above",
        "Text.\n\n<!--\nc\n-->\n- a\n\nTail.\n".to_string(),
    );
    push(
        "list, comment inside a blockquote",
        "> Text.\n> <!-- c -->\n> - a\n".to_string(),
    );

    // MD058: tables
    push("table, comment above", format!("Text.\n<!-- c -->\n{TABLE}\n\nTail.\n"));
    push("table, comment below", format!("Text.\n\n{TABLE}\n<!-- c -->\nTail.\n"));
    push(
        "table, comment both sides",
        format!("Text.\n<!-- c -->\n{TABLE}\n<!-- c -->\nTail.\n"),
    );
    push(
        "table, multi-line comment above",
        format!("Text.\n\n<!--\nc\n-->\n{TABLE}\n\nTail.\n"),
    );
    push(
        "table, comment inside a blockquote",
        "> Text.\n> <!-- c -->\n> | a | b |\n> | - | - |\n> | 1 | 2 |\n".to_string(),
    );

    documents
}

#[test]
fn a_comment_only_line_is_the_blank_line_each_rule_asks_for() {
    for (label, document) in comment_separated_documents() {
        assert_eq!(
            rules_reported(&document),
            Vec::<String>::new(),
            "{label}: a comment-only line separates the blocks around it\n{document}"
        );
    }
}

#[test]
fn nothing_rewrites_a_document_a_comment_already_separates() {
    // The half of #866 that costs the user something: the fix inserts a blank
    // line, which moves a directive comment away from what it applies to.
    for (label, document) in comment_separated_documents() {
        assert_eq!(
            formatted(&document),
            document,
            "{label}: fmt must leave the document alone"
        );
    }
}

#[test]
fn a_construct_written_straight_under_a_paragraph_is_still_reported() {
    for (label, expected, document) in [
        ("fence", "MD031", "Text.\n```py\nx\n```\n".to_string()),
        ("heading", "MD022", "Text.\n## H\n\nTail.\n".to_string()),
        ("list", "MD032", "Text.\n- a\n\nTail.\n".to_string()),
        ("table", "MD058", format!("Text.\n{TABLE}\n\nTail.\n")),
    ] {
        assert_eq!(
            rules_reported(&document),
            vec![expected.to_string()],
            "{label}: the rule still reports a missing blank line\n{document}"
        );
    }
}

#[test]
fn a_comment_sharing_its_line_with_prose_leaves_that_line_a_paragraph() {
    for (label, expected, document) in [
        ("fence", "MD031", "Text. <!-- c -->\n```py\nx\n```\n".to_string()),
        ("heading", "MD022", "Text. <!-- c -->\n## H\n\nTail.\n".to_string()),
        ("list", "MD032", "Text. <!-- c -->\n- a\n\nTail.\n".to_string()),
        ("table", "MD058", format!("Text. <!-- c -->\n{TABLE}\n\nTail.\n")),
    ] {
        assert_eq!(
            rules_reported(&document),
            vec![expected.to_string()],
            "{label}: a line carrying prose is not blank\n{document}"
        );
    }
}
