//! MD089 through the real binary: opt-in status, the issue example, `fmt`,
//! and inline suppression.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

const ISSUE_843: &str =
    "日本語englishひらがな\nカタカナenglishカタカナ\nﾊﾝｶｸｶﾀｶﾅenglish１２３全角数字\n한글english한글\n";
const ISSUE_843_FIXED: &str =
    "日本語 english ひらがな\nカタカナ english カタカナ\nﾊﾝｶｸｶﾀｶﾅ english１２３全角数字\n한글 english 한글\n";

/// Runs rumdl in `dir` with no cache and no discovered config, returning stdout.
fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new(rumdl_bin())
        .current_dir(dir)
        .args(["--no-config"])
        .args(args)
        .args(["--no-cache"])
        .output()
        .expect("rumdl runs");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn md089_findings(stdout: &str) -> usize {
    stdout.lines().filter(|line| line.contains("[MD089]")).count()
}

#[test]
fn md089_is_opt_in() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test.md"), ISSUE_843).unwrap();
    let stdout = run(dir.path(), &["check", "test.md"]);
    assert_eq!(
        md089_findings(&stdout),
        0,
        "MD089 must not run unless enabled:\n{stdout}"
    );
    // Positive control: the same file, the same binary, only `--enable
    // MD089` added. Without this, an empty stdout caused by a missing file
    // or a rejected flag would satisfy the assertion above for the wrong
    // reason.
    let stdout = run(dir.path(), &["check", "--enable", "MD089", "test.md"]);
    assert!(
        md089_findings(&stdout) > 0,
        "MD089 must report findings once enabled:\n{stdout}"
    );
}

#[test]
fn md089_reports_and_fixes_the_issue_example() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(&file, ISSUE_843).unwrap();
    let stdout = run(dir.path(), &["check", "--enable", "MD089", "test.md"]);
    // Two gaps on each line, except the third where the full-width digits
    // are not Latin text.
    assert_eq!(md089_findings(&stdout), 7, "unexpected report:\n{stdout}");
    run(dir.path(), &["fmt", "--enable", "MD089", "test.md"]);
    assert_eq!(fs::read_to_string(&file).unwrap(), ISSUE_843_FIXED);
    let stdout = run(dir.path(), &["check", "--enable", "MD089", "test.md"]);
    assert_eq!(md089_findings(&stdout), 0, "fixed file must be clean:\n{stdout}");
}

#[test]
fn md089_honours_inline_disable() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.md");
    let content = "中文english <!-- rumdl-disable-line MD089 -->\n中文english\n";
    fs::write(&file, content).unwrap();
    let stdout = run(dir.path(), &["check", "--enable", "MD089", "test.md"]);
    assert_eq!(md089_findings(&stdout), 1, "only line 2 is reported:\n{stdout}");
    assert!(
        stdout.contains("test.md:2:3"),
        "finding is on line 2 column 3:\n{stdout}"
    );
    run(dir.path(), &["fmt", "--enable", "MD089", "test.md"]);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "中文english <!-- rumdl-disable-line MD089 -->\n中文 english\n"
    );
}

#[test]
fn md089_fmt_does_not_turn_a_paragraph_into_a_list() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.md");
    let content = "# T\n\n段落\n1)中文\n";
    fs::write(&file, content).unwrap();
    run(dir.path(), &["fmt", "--enable", "MD089", "test.md"]);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        content,
        "an enumeration label must survive fmt unchanged"
    );
    let stdout = run(dir.path(), &["check", "test.md"]);
    assert!(
        !stdout.contains("[MD032]"),
        "the line must still be a paragraph:\n{stdout}"
    );
    // Positive control: the same document with the space already in place is
    // a list item, so the assertion above can observe MD032 when it applies.
    fs::write(&file, "# T\n\n段落\n1) 中文\n").unwrap();
    let stdout = run(dir.path(), &["check", "test.md"]);
    assert!(
        stdout.contains("[MD032]"),
        "a real list item after a paragraph reports MD032:\n{stdout}"
    );
}

#[test]
fn md089_fmt_does_not_open_a_list_inside_a_list_item() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.md");
    let content = "# T\n\n1. 项\n2. 3)中文\n";
    fs::write(&file, content).unwrap();
    run(dir.path(), &["fmt", "--enable", "MD089", "test.md"]);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        content,
        "an enumeration label inside a list item must survive fmt unchanged"
    );
    let stdout = run(dir.path(), &["check", "test.md"]);
    assert!(
        !stdout.contains("[MD029]"),
        "the item content must still be a paragraph:\n{stdout}"
    );
    // Positive control: the same document with the space already in place
    // opens the nested list, so the assertion above can observe MD029 when it
    // applies.
    fs::write(&file, "# T\n\n1. 项\n2. 3) 中文\n").unwrap();
    let stdout = run(dir.path(), &["check", "test.md"]);
    assert!(
        stdout.contains("[MD029]"),
        "a nested ordered list reports MD029:\n{stdout}"
    );
}

#[test]
fn md089_reads_symbol_sets_from_config() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[global]\nenable = [\"MD089\"]\n\n[MD089]\nsymbols-before-cjk = \"\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("test.md"), "角度為90°的角\n").unwrap();
    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .expect("rumdl runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        md089_findings(&stdout),
        1,
        "° no longer attaches, only 為|90 is reported:\n{stdout}"
    );
}
