//! MD063 `ignore-words` entries are honored when the word is followed or wrapped by
//! punctuation, and when the entry itself contains a hyphen. The config comes from a
//! `.rumdl.toml` on disk, the way a project sets it.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn config(style: &str, words: &str) -> String {
    format!("[MD063]\nenabled = true\nstyle = \"{style}\"\nignore-words = [{words}]\n")
}

/// The document after `check --fix` under the given config file.
fn fixed(config: &str, content: &str) -> String {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["check", "--no-cache", "--fix", "--enable", "MD063"])
        .arg("doc.md")
        .output()
        .expect("run rumdl check");
    fs::read_to_string(&path).unwrap()
}

#[test]
fn sentence_case_keeps_ignored_word_before_punctuation() {
    let cfg = config("sentence-case", "\"Kubernetes\"");
    for punct in ["?", "!", ".", ":", ";", ","] {
        let doc = format!("## What is Kubernetes{punct}\n");
        assert_eq!(fixed(&cfg, &doc), doc, "punctuation {punct:?}");
    }
}

#[test]
fn sentence_case_keeps_ignored_word_inside_wrapping_punctuation() {
    let cfg = config("sentence-case", "\"Kubernetes\"");
    for doc in [
        "## Using (Kubernetes) today\n",
        "## Using \"Kubernetes\", today\n",
        "## Using Kubernetes, today\n",
        "## Why Kubernetes?\n",
    ] {
        assert_eq!(fixed(&cfg, doc), doc);
    }
}

#[test]
fn title_case_keeps_ignored_word_before_punctuation() {
    let cfg = config("title-case", "\"kubectl\"");
    assert_eq!(fixed(&cfg, "## using kubectl, today\n"), "## Using kubectl, Today\n");
    assert_eq!(fixed(&cfg, "## what is kubectl?\n"), "## What Is kubectl?\n");
}

#[test]
fn entry_carrying_its_own_punctuation_matches_with_and_without_trailing_punctuation() {
    let cfg = config("sentence-case", "\"C++\", \"Node.js\"");
    assert_eq!(fixed(&cfg, "## Why C++?\n"), "## Why C++?\n");
    assert_eq!(fixed(&cfg, "## Why C++\n"), "## Why C++\n");
    assert_eq!(fixed(&cfg, "## Using Node.js, today\n"), "## Using Node.js, today\n");
}

#[test]
fn hyphenated_entry_is_kept_whole_in_title_case() {
    let cfg = config("title-case", "\"foo-bar\"");
    assert_eq!(fixed(&cfg, "## using foo-bar\n"), "## Using foo-bar\n");
    assert_eq!(fixed(&cfg, "## using foo-bar?\n"), "## Using foo-bar?\n");
}

// Controls: the fix still changes everything that is not ignored.

#[test]
fn unlisted_words_with_punctuation_are_still_corrected() {
    let cfg = config("sentence-case", "\"Kubernetes\"");
    assert_eq!(fixed(&cfg, "## What is Docker?\n"), "## What is docker?\n");
    assert_eq!(
        fixed(&cfg, "## What is Kubernetes? And Docker!\n"),
        "## What is Kubernetes? and docker!\n"
    );
}

#[test]
fn match_stays_case_sensitive_and_whole_word() {
    let cfg = config("sentence-case", "\"Kubernetes\"");
    assert_eq!(fixed(&cfg, "## What is Kubernetesx?\n"), "## What is kubernetesx?\n");
    let cfg = config("title-case", "\"npm\"");
    assert_eq!(fixed(&cfg, "## Using Npm?\n"), "## Using Npm?\n");
}

#[test]
fn combining_marks_are_part_of_the_word_not_punctuation() {
    // "cafe" followed by U+0301 is a different word from "cafe"
    let cfg = config("title-case", "\"cafe\"");
    assert_eq!(fixed(&cfg, "## cafe\u{301} guide\n"), "## Cafe\u{301} Guide\n");
    assert_eq!(fixed(&cfg, "## cafe guide\n"), "## cafe Guide\n");
}

#[test]
fn unicode_sentence_punctuation_after_an_ignored_word_is_peeled() {
    let cfg = config("sentence-case", "\"Kubernetes\"");
    for punct in ["\u{2026}", "\u{ff1f}", "\u{3002}", "\u{201d}", "\u{bb}"] {
        let doc = format!("## What is Kubernetes{punct}\n");
        assert_eq!(fixed(&cfg, &doc), doc, "punctuation {punct:?}");
    }
}

#[test]
fn symbol_suffix_makes_a_distinct_word() {
    let cfg = config("title-case", "\"c\"");
    assert_eq!(fixed(&cfg, "## c++ guide\n"), "## C++ Guide\n");
    assert_eq!(fixed(&cfg, "## c\u{ff0b} guide\n"), "## C\u{ff0b} Guide\n");
    assert_eq!(fixed(&cfg, "## c# guide\n"), "## C# Guide\n");
    assert_eq!(fixed(&cfg, "## using c, today\n"), "## Using c, Today\n");
}
