//! MD027 with `list-items = true` against the real binary: indentation that
//! nests or continues a list item inside a quote is list structure, and spacing
//! the item structure does not account for is still reported.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// The lines MD027 reports for `doc` under `list-items = true`.
fn reported_lines(doc: &str) -> Vec<usize> {
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(&config, "[global]\nenable = [\"MD027\"]\n[MD027]\nlist-items = true\n").unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["check", "--config", config.to_str().unwrap(), "--no-cache"])
        .arg(&path)
        .output()
        .expect("run rumdl");
    assert!(matches!(output.status.code(), Some(0 | 1)), "{output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("[MD027]"))
        .map(|line| line.split(':').nth(1).unwrap().parse().unwrap())
        .collect()
}

#[test]
fn nested_items_in_a_quote_are_not_multiple_spaces() {
    assert_eq!(reported_lines("> - a\n>   - b\n"), Vec::<usize>::new());
    assert_eq!(reported_lines("> 1. a\n>    - b\n>      - c\n"), Vec::<usize>::new());
    assert_eq!(reported_lines(">> - a\n>>   - b\n"), Vec::<usize>::new());
    assert_eq!(reported_lines("> - a\n>\n>   para in a\n"), Vec::<usize>::new());
}

#[test]
fn spacing_the_item_structure_does_not_explain_is_reported() {
    assert_eq!(reported_lines("> - a\n>  - b\n"), vec![2]);
    assert_eq!(reported_lines(">  - a\n>   - b\n"), vec![1, 2]);
    assert_eq!(reported_lines("> a\n>   b\n"), vec![2]);
    assert_eq!(reported_lines("> - a\n>\n> text\n>   x\n"), vec![4]);
}
