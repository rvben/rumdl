//! Sentence reflow distinguishes labels from initials using their context.

use std::{fs, path::Path, process::Command};

fn run(command: &str, path: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            command,
            "--isolated",
            "--no-cache",
            "--enable",
            "MD013",
            "-c",
            "MD013.reflow=true",
            "-c",
            "MD013.reflow-mode=\"sentence-per-line\"",
            "-c",
            "MD013.line-length=0",
        ])
        .args(extra)
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn check_and_fix_single_letter_sentence_endings() {
    for (command, extra) in [("fmt", vec![]), ("fmt", vec!["--silent"]), ("check", vec!["--fix"])] {
        for (input, expected) in [
            (
                "Service B keeps a copy for service A. It is updated by events.\n",
                "Service B keeps a copy for service A.\nIt is updated by events.\n",
            ),
            (
                "- We choose option C. Then we retry.\n",
                "- We choose option C.\n  Then we retry.\n",
            ),
            (
                "> We choose option C. Then we retry.\n",
                "> We choose option C.\n> Then we retry.\n",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("doc.md");
            fs::write(&path, input).unwrap();
            let check = run("check", &path, &["--output-format", "json"]);
            assert_eq!(check.status.code(), Some(1), "{check:?}");
            let findings: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
            assert!(
                findings.as_array().unwrap().iter().any(|d| d["rule"] == "MD013"),
                "{findings}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
            let fixed = run(command, &path, &extra);
            assert!(fixed.status.success(), "{fixed:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            let check = run("check", &path, &[]);
            assert!(check.status.success(), "{check:?}");
            let again = run(command, &path, &extra);
            assert!(again.status.success(), "{again:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        }
    }
}

#[test]
fn existing_sentence_breaks_and_wrapped_initials_are_preserved() {
    let input = "We fall back to plan B.\nThen we try again.\n\nThe book by Robert C.\nMartin is a classic.\n\nJ. K. Rowling wrote it.\n\nA. Smith wrote it.\n";
    let expected = "We fall back to plan B.\nThen we try again.\n\nThe book by Robert C. Martin is a classic.\n\nJ. K. Rowling wrote it.\n\nA. Smith wrote it.\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, input).unwrap();
    let fixed = run("fmt", &path, &[]);
    assert!(fixed.status.success(), "{fixed:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    let check = run("check", &path, &[]);
    assert!(check.status.success(), "{check:?}");
}
