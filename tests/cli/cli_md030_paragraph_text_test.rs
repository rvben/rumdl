//! `<digits>.` directly followed by text is paragraph text to the CommonMark parser.
//! MD030 may warn about it, but `check --fix` and `fmt` must leave the bytes alone:
//! inserting the space would turn the paragraph line into a list item (or change it
//! for a number too long to be a marker). A real list item with wrong spacing is
//! the control: it is still fixed.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const PARAGRAPH_TEXT: &[&str] = &[
    "0.A\n",
    "1.Foo bar\n",
    "See section\n1.Introduction for details.\n",
    "1234567890.Foo\n",
    "See:\n1.Foo\n",
    "> 1.Foo\n",
];

/// Run `rumdl <subcommand> <args> doc.md` on `content`; return the output and the file afterwards.
fn run(subcommand: &str, args: &[&str], content: &str) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg(subcommand)
        .args(["--no-cache", "--isolated"])
        .args(args)
        .args(["--enable", "MD030"])
        .arg(&path)
        .output()
        .expect("run rumdl");
    (output, fs::read_to_string(&path).unwrap())
}

#[test]
fn test_check_reports_paragraph_text_as_a_guess() {
    for source in PARAGRAPH_TEXT {
        let (output, after) = run("check", &[], source);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("MD030"), "positive control for {source:?}: {stdout}");
        assert!(stdout.contains("paragraph text"), "{source:?}: {stdout}");
        assert_eq!(&after, source);
    }
}

#[test]
fn test_check_fix_leaves_paragraph_text_unchanged() {
    for source in PARAGRAPH_TEXT {
        let (_, after) = run("check", &["--fix"], source);
        assert_eq!(&after, source, "check --fix changed {source:?}");
    }
}

#[test]
fn test_fmt_leaves_paragraph_text_unchanged() {
    for source in PARAGRAPH_TEXT {
        let (_, after) = run("fmt", &[], source);
        assert_eq!(&after, source, "fmt changed {source:?}");
    }
}

#[test]
fn test_real_list_items_with_wrong_spacing_are_still_fixed() {
    for (source, expected) in [("1.  Foo\n", "1. Foo\n"), ("-  item\n", "- item\n")] {
        let (_, fixed) = run("check", &["--fix"], source);
        assert_eq!(fixed, expected, "check --fix on {source:?}");
        let (_, formatted) = run("fmt", &[], source);
        assert_eq!(formatted, expected, "fmt on {source:?}");
    }
}
