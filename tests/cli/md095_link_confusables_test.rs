//! MD095 through the real executable, including HTML-only relative links.

use std::fs;
use std::process::{Command, Output};

fn run(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["--no-config"])
        .args(args)
        .args(["--no-cache"])
        .output()
        .expect("run rumdl")
}

#[test]
fn md095_reports_encoded_and_relative_destinations_with_source_columns() {
    for (source, marker) in [
        ("[x](https://&#x435;xample.com)\n", "&#x435;"),
        ("[x](https://&iecy;xample.com)\n", "&iecy;"),
        ("<a href='&iecy;'>x</a>\n", "&iecy;"),
        ("<a href='е'>x</a>\n", "е"),
        ("<area href='&#1077;'>\n", "&#1077;"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.md"), source).unwrap();
        let output = run(
            dir.path(),
            &[
                "check",
                "--enable",
                "link-confusables",
                "--output-format",
                "json",
                "test.md",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{source}: {output:?}");
        let warnings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let warnings = warnings.as_array().unwrap();
        assert_eq!(warnings.len(), 1, "{source}: {warnings:?}");
        assert_eq!(warnings[0]["rule"], "MD095");
        assert_eq!(
            warnings[0]["column"],
            source[..source.find(marker).unwrap()].chars().count() + 1
        );
        assert_eq!(warnings[0]["fixable"], false);
    }
}

#[test]
fn md095_is_opt_in_and_preserves_source_when_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.md");
    let source = "# Heading\n\n[x](https://&iecy;xample.com)\n";
    fs::write(&path, source).unwrap();
    let output = run(dir.path(), &["check", "--output-format", "json", "test.md"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let warnings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        warnings
            .as_array()
            .unwrap()
            .iter()
            .all(|warning| warning["rule"] != "MD095")
    );
    let positive = run(dir.path(), &["check", "--enable", "MD095", "test.md"]);
    assert_eq!(positive.status.code(), Some(1), "{positive:?}");
    run(dir.path(), &["fmt", "--enable", "MD095", "test.md"]);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn md095_honors_inline_suppression() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("test.md"),
        "<!-- rumdl-disable MD095 -->\n<a href='&iecy;'>x</a>\n",
    )
    .unwrap();
    let output = run(
        dir.path(),
        &["check", "--enable", "MD095", "--output-format", "json", "test.md"],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let warnings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(warnings.as_array().unwrap().is_empty());
}
