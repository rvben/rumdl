use std::{fs, process::Command};

#[test]
fn empty_wikilink_labels_preserve_following_content_through_cli_fixes() {
    // A bounded RUSTSEC-2026-0414 reproducer: the old parser duplicates the
    // trailing link thousands of times, without needing a large input.
    let prefix = "[[a|]] ".repeat(12);
    let source = format!("{prefix}[ bad ](url)\n");
    let expected = format!("{prefix}[bad](url)\n");
    for args in [vec!["check", "--fix"], vec!["fmt"]] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, &source).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args([
                "check",
                "--isolated",
                "--no-cache",
                "--enable",
                "MD039",
                "--output-format",
                "json",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let warnings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0]["rule"], "MD039");
        assert_eq!(warnings[0]["column"], prefix.len() + 1);

        for _ in 0..2 {
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(&args)
                .args(["--isolated", "--no-cache", "--enable", "MD039"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}: {output:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        }
    }
}
