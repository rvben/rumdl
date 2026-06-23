//! Regression test for issue #684:
//! `MD040` auto-fix corrupts a fenced code block inside a blockquote.
//!
//! A fenced code block without a language inside a blockquote used to be
//! rewritten to an invalid `` > `text `` `` inline span instead of a valid
//! `> ```text` fence. Because the block then stopped being a code block, MD027
//! ("multiple spaces after quote marker") flattened the indentation of its
//! content, destroying directory trees and similar indented text.
//!
//! These tests run the real `rumdl fmt` pipeline (all default rules) through the
//! binary, so they exercise the exact production path including the MD040/MD027
//! interaction that produced the original corruption.

use std::fs;
use tempfile::tempdir;

/// Run `rumdl fmt --no-config --no-cache` on `content` and return the rewritten file.
fn fmt_with_defaults(content: &str) -> String {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("input.md");
    fs::write(&file_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

#[test]
fn test_blockquote_directory_tree_preserved() {
    let input = "# Title\n\n> ```\n> root/\n> └── nested/\n>     └── file.txt\n> ```\n";
    let expected = "# Title\n\n> ```text\n> root/\n> └── nested/\n>     └── file.txt\n> ```\n";

    let fixed = fmt_with_defaults(input);
    assert_eq!(
        fixed, expected,
        "MD040 must produce a valid `> ```text` fence and leave the indented tree intact"
    );
}

#[test]
fn test_blockquote_fence_fix_is_idempotent() {
    let input = "# Title\n\n> ```\n> root/\n> └── nested/\n>     └── file.txt\n> ```\n";

    let once = fmt_with_defaults(input);
    let twice = fmt_with_defaults(&once);
    assert_eq!(once, twice, "Formatting the fixed output again must be a no-op");
}

#[test]
fn test_blockquote_fence_with_language_untouched() {
    // A fence that already has a language must be left exactly as-is.
    let input = "# Title\n\n> ```text\n> root/\n>     nested\n> ```\n";

    let fixed = fmt_with_defaults(input);
    assert_eq!(fixed, input, "A valid in-blockquote fence must not be rewritten");
}
