//! GitHub turns only U+0020 into a hyphen when it builds a heading anchor
//! (github-slugger: `tr(' ', '-')` after the filter that drops everything but
//! word characters, hyphens and spaces). Every other whitespace character is
//! deleted, so `# A<em space>b` is `#ab`, never `#a-b`.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Run `rumdl check --enable MD051` on `content` with the given anchor style.
fn run(content: &str, style: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--enable", "MD051"])
        .args(["--config", &format!("MD051.anchor-style=\"{style}\"")])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn doc(heading_text: &str, anchor: &str) -> String {
    format!("# {heading_text}\n\n[x](#{anchor})\n")
}

#[test]
fn github_anchor_deletes_non_ascii_whitespace() {
    for ws in ["\u{2003}", "\u{a0}", "\u{3000}", "\u{2009}", "\t"] {
        let heading = format!("A{ws}b");
        assert_eq!(
            run(&doc(&heading, "ab"), "github").matches("MD051").count(),
            0,
            "{ws:?}"
        );
        assert!(run(&doc(&heading, "a-b"), "github").contains("MD051"), "{ws:?} #a-b");
    }
    // Controls: a real space is a hyphen, and the oracle-checked `&` forms hold.
    assert!(!run(&doc("A b", "a-b"), "github").contains("MD051"));
    assert!(run(&doc("A b", "ab"), "github").contains("MD051"));
    assert!(!run(&doc("A & b", "a--b"), "github").contains("MD051"));
    assert!(!run(&doc("A\u{2003}&\u{2003}b", "ab"), "github").contains("MD051"));
}

/// The kramdown styles agree with Ruby kramdown 2.5.2 (checked there): the
/// non-ASCII space is deleted, a space is a hyphen.
#[test]
fn kramdown_anchors_delete_non_ascii_whitespace() {
    for style in ["kramdown", "kramdown-gfm"] {
        for ws in ["\u{2003}", "\u{a0}", "\u{3000}"] {
            let heading = format!("A{ws}b");
            assert!(!run(&doc(&heading, "ab"), style).contains("MD051"), "{style} {ws:?}");
            assert!(
                run(&doc(&heading, "a-b"), style).contains("MD051"),
                "{style} {ws:?} #a-b"
            );
        }
    }
}
