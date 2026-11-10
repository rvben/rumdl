//! A tab-indented list inside a definition is reported by the Markdown parser
//! with an item offset that points before the marker, which can fall inside a
//! multi-byte character on an earlier line. Such a document is linted and
//! formatted like any other.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Each document holds non-ASCII text ahead of a tab-indented list in a
/// definition, in shapes that place the reported item offset inside a
/// character.
const DOCUMENTS: [(&str, &str); 4] = [
    ("tight.md", "¡\n:¡\n\t* ¡"),
    ("accented.md", "é\n:é\n\t- é\n"),
    ("wide.md", "語\n:語\n\t1. 語\n"),
    ("nested.md", "> ¡\n> :¡\n> \t* ¡\n"),
];

fn rumdl(subcommand: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args([subcommand, "--isolated", "--no-cache"]);
    command
}

fn assert_no_panic(name: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(!stderr.contains("panicked"), "{name}: panic in stderr: {stderr}");
    assert!(
        !stderr.contains("crashed unexpectedly"),
        "{name}: panic handler fired: {stderr}"
    );
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "{name}: unexpected exit {:?}: {stdout}{stderr}",
        output.status.code()
    );
    format!("{stdout}{stderr}")
}

#[test]
fn check_lints_a_tab_indented_list_in_a_definition_after_non_ascii_text() {
    for (name, content) in DOCUMENTS {
        let dir = tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();

        let output = rumdl("check").arg(&path).output().expect("run rumdl check");
        assert_no_panic(name, &output);
    }
}

#[test]
fn fmt_formats_a_tab_indented_list_in_a_definition_after_non_ascii_text() {
    for (name, content) in DOCUMENTS {
        let dir = tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();

        let output = rumdl("fmt").arg(&path).output().expect("run rumdl fmt");
        assert_no_panic(name, &output);

        let formatted = fs::read_to_string(&path).unwrap();
        let again = rumdl("fmt").arg(&path).output().expect("run rumdl fmt again");
        assert_no_panic(name, &again);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            formatted,
            "{name}: formatting is not idempotent"
        );
    }
}
