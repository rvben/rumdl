//! `rumdl fmt` must never delete the text of a table cell. A row with more
//! cells than its header is reported by MD056 and left as written; only the
//! author can tell a missing header column from a stray pipe.

use std::fs;
use std::process::Command;

const DOCUMENT: &str = "\
# T

| A | B | C |
| - | - | - |
| one | two | three | four |
| x | y |
";

#[test]
fn fmt_keeps_extra_cells_and_reports_them() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("probe.md");
    fs::write(&file, DOCUMENT).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp.path())
        .args(["fmt", "--no-cache", "--no-config", "probe.md"])
        .output()
        .expect("failed to execute rumdl");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    // The short row is still padded, so the fix path ran on this table
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        DOCUMENT.replace("| x | y |\n", "| x | y |  |\n"),
        "{text}"
    );
    assert!(
        text.contains("probe.md:5:1: [MD056] Table row has 4 cells, but expected 3\n"),
        "the extra cells must be reported without [fixed]\n{text}"
    );
    assert!(
        text.contains("probe.md:6:1: [MD056] Table row has 2 cells, but expected 3 [fixed]"),
        "{text}"
    );
}
