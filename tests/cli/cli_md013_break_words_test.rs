//! Project-specific semantic break words extend the built-in English list.

use std::{fs, path::Path, process::Command};

fn run(command: &str, dir: &Path) -> std::process::Output {
    run_with(command, dir, &[])
}

fn run_with(command: &str, dir: &Path, overrides: &[&str]) -> std::process::Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    process.current_dir(dir).args([
        command,
        "--no-cache",
        "--enable",
        "MD013",
        "--deny-config-warnings",
        "--output-format",
        "concise",
        "--config",
        ".rumdl.toml",
    ]);
    for setting in overrides {
        process.arg("-c").arg(setting);
    }
    process.arg("doc.md").output().unwrap()
}

#[test]
fn custom_break_words_extend_the_semantic_reflow_defaults() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[MD013]\nreflow = true\nreflow-mode = \"semantic-line-breaks\"\nline-length = 60\nbreak-words = [\"parce\"]\n",
    )
    .unwrap();
    let path = dir.path().join("doc.md");
    fs::write(
        &path,
        "Cette explication donne assez de contexte parce que la suite explique les conséquences.\n\nThis explanation gives enough background and the remaining words describe the details.\n",
    )
    .unwrap();
    let output = run("fmt", dir.path());
    assert!(output.status.success(), "{output:?}");
    let expected = "Cette explication donne assez de contexte\nparce que la suite explique les conséquences.\n\nThis explanation gives enough background\nand the remaining words describe the details.\n";
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    let check = run("check", dir.path());
    assert!(check.status.success(), "{check:?}");
    let repeated = run("fmt", dir.path());
    assert!(repeated.status.success(), "{repeated:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn break_words_accept_aliases_and_unicode_case_without_changing_source_bytes() {
    for (key, words, input, expected) in [
        (
            "break_words",
            "[\"über\", \"\", \"über\"]",
            "İstanbul erklärt zuerst die wichtigsten Grundlagen ÜBER die wir später ausführlich sprechen.\n",
            "İstanbul erklärt zuerst die wichtigsten Grundlagen\nÜBER die wir später ausführlich sprechen.\n",
        ),
        (
            "break-words",
            "[\"straße\"]",
            "Diese Erklärung gibt genügend Kontext STRASSE und folgt dem vereinbarten Beispiel.\n",
            "Diese Erklärung gibt genügend Kontext\nSTRASSE und folgt dem vereinbarten Beispiel.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            format!(
                "[MD013]\nreflow = true\nreflow-mode = \"semantic-line-breaks\"\nline-length = 60\n{key} = {words}\n"
            ),
        )
        .unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let output = run("fmt", dir.path());
        assert!(output.status.success(), "{key}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        let check = run("check", dir.path());
        assert!(check.status.success(), "{key}: {check:?}");
        let repeated = run("fmt", dir.path());
        assert!(repeated.status.success(), "{repeated:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn inline_break_words_override_file_settings() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[MD013]\nreflow = true\nreflow-mode = \"semantic-line-breaks\"\nline-length = 60\nbreak-words = [\"puisque\"]\n",
    )
    .unwrap();
    let path = dir.path().join("doc.md");
    fs::write(
        &path,
        "Cette explication donne assez de contexte PARCE que la suite explique les conséquences.\n",
    )
    .unwrap();
    let output = run_with("fmt", dir.path(), &["MD013.break-words=[\"parce\"]"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "Cette explication donne assez de contexte\nPARCE que la suite explique les conséquences.\n"
    );
}

#[test]
fn custom_words_respect_whole_words_and_inline_protection() {
    for input in [
        "This explanation supplies sufficient context parceled into several additional sections for readers.\n",
        "This explanation supplies sufficient context `words parce more words` followed by details.\n",
        "This explanation supplies sufficient context [words parce more words](https://example.com) followed by details.\n",
        "Notes (this explanation supplies sufficient context parce que more words) followed by the details.\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            "[MD013]\nreflow = true\nreflow-mode = \"semantic-line-breaks\"\nline-length = 60\nbreak-words = [\"parce\"]\n",
        ).unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let baseline = run_with("fmt", dir.path(), &["MD013.break-words=[]"]);
        assert!(baseline.status.success(), "{baseline:?}");
        let expected = fs::read_to_string(&path).unwrap();
        fs::write(&path, input).unwrap();
        let custom = run("fmt", dir.path());
        assert!(custom.status.success(), "{input:?}: {custom:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{input:?}");
        let check = run("check", dir.path());
        assert!(check.status.success(), "{input:?}: {check:?}");
    }
}

#[test]
fn custom_break_words_do_not_change_other_reflow_modes() {
    for mode in ["default", "normalize", "sentence-per-line", "sentence-pack"] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".rumdl.toml"),
            format!("[MD013]\nreflow = true\nreflow-mode = {mode:?}\nline-length = 60\nbreak-words = [\"parce\"]\n"),
        )
        .unwrap();
        let path = dir.path().join("doc.md");
        let input = "Cette explication donne assez de contexte parce que la suite explique les conséquences.\n";
        fs::write(&path, input).unwrap();
        let baseline = run_with("fmt", dir.path(), &["MD013.break-words=[]"]);
        assert!(baseline.status.success(), "{mode}: {baseline:?}");
        let expected = fs::read_to_string(&path).unwrap();
        fs::write(&path, input).unwrap();
        let custom = run("fmt", dir.path());
        assert!(custom.status.success(), "{mode}: {custom:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{mode}");
    }
}
