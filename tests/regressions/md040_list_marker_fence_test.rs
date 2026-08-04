//! Regression test: `MD040` auto-fix corrupts a fenced code block opened on a
//! list marker line.
//!
//! A fence without a language written directly after a list bullet (`- ```)
//! used to be rewritten to an invalid `` - `text `` `` inline span, because the
//! fix anchored its replacement range at the first non-whitespace byte of the
//! line rather than at the fence itself. The block then stopped being a code
//! block, so the rules that normalize list continuation lines flattened its
//! indentation and appended stray fences. `rumdl fmt` reported `Fixed 1/1
//! issues` and exited 0 while destroying the content.
//!
//! This is the same defect that issue #684 fixed for blockquotes; the
//! list-marker prefix was never covered.
//!
//! These tests run the real `rumdl fmt` pipeline (all default rules) through the
//! binary, so they exercise the production path including the interaction with
//! the list rules that produced the original corruption.

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
fn test_list_item_directory_tree_preserved() {
    let input = "# Title\n\n- ```\n  root/\n  └── nested/\n      └── file.txt\n  ```\n";
    let expected = "# Title\n\n- ```text\n  root/\n  └── nested/\n      └── file.txt\n  ```\n";

    let fixed = fmt_with_defaults(input);
    assert_eq!(
        fixed, expected,
        "MD040 must produce a valid `- ```text` fence and leave the indented tree intact"
    );
}

#[test]
fn test_ordered_list_marker_fence_preserved() {
    let input = "# Title\n\n1. ```\n   code\n   ```\n";
    let expected = "# Title\n\n1. ```text\n   code\n   ```\n";

    assert_eq!(fmt_with_defaults(input), expected);
}

#[test]
fn test_tilde_fence_on_list_marker_preserved() {
    // The fence marker has to be measured where it actually starts; a default of
    // three backticks turned `- ~~~` into `- ~text ~~`.
    let input = "# Title\n\n- ~~~\n  code\n  ~~~\n";
    let expected = "# Title\n\n- ~~~text\n  code\n  ~~~\n";

    assert_eq!(fmt_with_defaults(input), expected);
}

#[test]
fn test_long_fence_on_list_marker_preserved() {
    let input = "# Title\n\n- ````\n  code\n  ````\n";
    let expected = "# Title\n\n- ````text\n  code\n  ````\n";

    assert_eq!(fmt_with_defaults(input), expected);
}

#[test]
fn test_list_marker_fence_fix_is_idempotent() {
    let input = "# Title\n\n- ```\n  root/\n  └── nested/\n      └── file.txt\n  ```\n";
    let expected = "# Title\n\n- ```text\n  root/\n  └── nested/\n      └── file.txt\n  ```\n";

    let once = fmt_with_defaults(input);
    // The corrupting output was itself stable across a second pass, so pin what
    // the first pass produced: convergence alone does not prove correctness.
    assert_eq!(once, expected, "the first pass must produce a valid fence");

    let twice = fmt_with_defaults(&once);
    assert_eq!(once, twice, "Formatting the fixed output again must be a no-op");
}

#[test]
fn test_list_marker_fence_with_language_untouched() {
    // A fence that already has a language must be left exactly as-is, including
    // the extra indentation inside the block.
    let input = "# Title\n\n- ```text\n  code\n      indented\n  ```\n";

    assert_eq!(
        fmt_with_defaults(input),
        input,
        "A valid fence on a list marker must not be rewritten"
    );
}
