//! A setext heading whose last line is an attribute block inside a blockquote or
//! list item corrupts the Markdown parser's tree when heading attributes are
//! enabled, and the next step of the parse panics. One such file must not
//! prevent any other file in the run from being linted or fixed.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const GOOD: &str = "# Good\n\ntrailing spaces   \n";

fn rumdl() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["check", "--isolated", "--no-cache"]);
    command
}

fn assert_no_panic(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(!stderr.contains("panicked"), "panic in stderr: {stderr}");
    assert!(
        !stderr.contains("crashed unexpectedly"),
        "panic handler fired: {stderr}"
    );
    format!("{stdout}{stderr}")
}

#[test]
fn check_lints_the_files_beside_a_setext_heading_ending_in_an_attribute_block() {
    for (name, content) in [
        ("quote.md", ">o\n>      {}\n>=\n"),
        ("quote_two.md", ">o\n>  {}\n>-\n"),
        ("nested.md", ">>o\n>>      {}\n>>=\n"),
        ("list.md", "- o\n   {}\n  =\n"),
        ("ordered.md", "1. o\n     {}\n   =\n"),
    ] {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(name), content).unwrap();
        fs::write(dir.path().join("good.md"), GOOD).unwrap();

        let output = rumdl()
            .arg(dir.path().join(name))
            .arg(dir.path().join("good.md"))
            .output()
            .expect("run rumdl check");
        let text = assert_no_panic(&output);

        assert!(
            text.contains("good.md") && text.contains("MD009"),
            "{name}: the good file was not linted:\n{text}"
        );
        assert_eq!(output.status.code(), Some(1), "{name}: {text}");
    }
}

#[test]
fn fix_repairs_the_files_beside_a_tab_indented_attribute_block_in_a_blockquote() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.md");
    let good = dir.path().join("good.md");
    fs::write(&bad, ">o\n>\t{}\n>=\n").unwrap();
    fs::write(&good, GOOD).unwrap();

    let output = rumdl()
        .arg("--fix")
        .arg(&bad)
        .arg(&good)
        .output()
        .expect("run rumdl check --fix");
    assert_no_panic(&output);

    assert_eq!(fs::read_to_string(&good).unwrap(), "# Good\n\ntrailing spaces\n");
    let fixed = fs::read_to_string(&bad).unwrap();
    assert!(!fixed.contains('\t'), "the tab was not replaced: {fixed:?}");
}

#[test]
fn heading_attributes_still_work_outside_that_shape() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, "# Title {#custom}\n\nSee [the title](#custom).\n").unwrap();

    let output = rumdl().arg(&path).output().expect("run rumdl check");
    let text = assert_no_panic(&output);

    assert_eq!(output.status.code(), Some(0), "{text}");
}
