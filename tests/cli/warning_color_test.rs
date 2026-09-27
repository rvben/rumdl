//! Warning labels printed to stderr must follow the same color switches as
//! the rest of the output: `--color`, `NO_COLOR` and `CLICOLOR_FORCE`.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

const ESC: char = '\x1b';

/// Each case raises a different kind of warning label on stderr.
fn cases() -> Vec<(&'static str, Vec<&'static str>, &'static str, &'static str)> {
    vec![
        (
            "cli warning",
            vec!["--no-config", "--enable", "MD999BOGUS"],
            "# T\n",
            "[cli warning]",
        ),
        (
            "config warning",
            vec!["--config", "bad.toml"],
            "# T\n",
            "[config warning]",
        ),
        (
            "inline config warning",
            vec!["--no-config"],
            "# T\n\n<!-- rumdl-disable MD999BOGUS -->\n",
            "[inline config warning]",
        ),
        (
            "rule config warning",
            vec!["--config", "md007.toml"],
            "# T\n",
            "[config warning]",
        ),
    ]
}

fn stderr_for(args: &[&str], content: &str, color: &[&str], env: &[(&str, &str)]) -> String {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("f.md"), content).unwrap();
    fs::write(dir.path().join("bad.toml"), "[MD013]\nno-such-option = 1\n").unwrap();
    fs::write(
        dir.path().join("md007.toml"),
        "[MD007]\nindent = 4\nstyle = \"text-aligned\"\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.current_dir(dir.path())
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("CLICOLOR")
        .arg("check")
        .arg("--no-cache")
        .args(args)
        .args(color)
        .arg("f.md");
    for (key, value) in env {
        cmd.env(key, value);
    }
    String::from_utf8_lossy(&cmd.output().unwrap().stderr).into_owned()
}

#[test]
fn warning_labels_are_plain_with_color_never() {
    for (name, args, content, label) in cases() {
        let stderr = stderr_for(&args, content, &["--color", "never"], &[]);
        assert!(stderr.contains(label), "{name}: expected {label} in stderr: {stderr:?}");
        assert!(
            !stderr.contains(ESC),
            "{name}: escape codes with --color never: {stderr:?}"
        );
    }
}

#[test]
fn warning_labels_are_plain_with_no_color() {
    for (name, args, content, label) in cases() {
        let stderr = stderr_for(&args, content, &[], &[("NO_COLOR", "1")]);
        assert!(stderr.contains(label), "{name}: expected {label} in stderr: {stderr:?}");
        assert!(!stderr.contains(ESC), "{name}: escape codes with NO_COLOR: {stderr:?}");
    }
}

#[test]
fn warning_labels_are_colored_with_color_always() {
    for (name, args, content, label) in cases() {
        let stderr = stderr_for(&args, content, &["--color", "always"], &[]);
        let colored_label = format!("{ESC}[33m{label}{ESC}[0m");
        assert!(
            stderr.contains(&colored_label),
            "{name}: expected a yellow {label} with --color always: {stderr:?}"
        );
    }
}
