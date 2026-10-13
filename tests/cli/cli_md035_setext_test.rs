use std::fs;
use std::process::{Command, Output};

const ISSUE_INPUT: &str =
    "# Doc\n\nIntro.\n\n---\n\nFirst line of a paragraph.\nSecond line of the paragraph.\n___\nNext paragraph.\n";

fn run(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "--config", "empty.toml", "--fix"])
        .args(args)
        .arg("doc.md")
        .output()
        .unwrap()
}

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("empty.toml"), "").unwrap();
    fs::write(dir.path().join("doc.md"), ISSUE_INPUT).unwrap();
    dir
}

#[test]
fn md035_fix_keeps_paragraph_and_converges_to_a_thematic_break() {
    let dir = setup();
    let output = run(dir.path(), &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "# Doc\n\nIntro.\n\n---\n\nFirst line of a paragraph.\nSecond line of the paragraph.\n\n---\n\nNext paragraph.\n"
    );
}

#[test]
fn md035_fix_without_md065_declines_and_the_warning_remains() {
    let dir = setup();
    let output = run(dir.path(), &["--enable", "MD035"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        ISSUE_INPUT,
        "the document must not change"
    );
    assert!(
        !output.status.success(),
        "the unfixed warning must fail the run: {output:?}"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("MD035"), "{output:?}");
}
