//! MD010 against the real binary: the tab after a footnote label.
//!
//! The whitespace after `[^id]:` decides whether the footnote's first block is a
//! paragraph or indented code, so the fix writes the columns the tab spans, not
//! the configured tab width. A tab elsewhere in a line keeps the configured width.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn fmt(doc: &str) -> String {
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(&config, "[global]\nenable = [\"MD010\"]\n[MD010]\nspaces-per-tab = 2\n").unwrap();
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

#[test]
fn a_tab_after_a_footnote_label_keeps_the_footnote_a_paragraph() {
    // `[^1]:` is five columns wide, so the tab spans three: a paragraph. Four
    // spaces would make the body indented code.
    assert_eq!(fmt("x[^1]\n\n[^1]:\tNote\n"), "x[^1]\n\n[^1]:   Note\n");
    assert_eq!(fmt("x[^abcde]\n\n[^abcde]:\tNote\n"), "x[^abcde]\n\n[^abcde]:   Note\n");
    assert_eq!(
        fmt("x[^\u{e9}\u{e9}]\n\n[^\u{e9}\u{e9}]:\tNote\n"),
        "x[^\u{e9}\u{e9}]\n\n[^\u{e9}\u{e9}]:  Note\n"
    );
}

#[test]
fn a_tab_outside_a_footnote_label_keeps_the_configured_width() {
    assert_eq!(fmt("[x]:\tfoo\n"), "[x]:  foo\n");
    assert_eq!(fmt("a [^1]:\tb\n"), "a [^1]:  b\n");
}
