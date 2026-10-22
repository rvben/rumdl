//! Per-document heading capitalization through the real CLI.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run(command: &str, content: &str, extra: &str) -> (Output, String) {
    let dir = tempdir().unwrap();
    let config = dir.path().join(".rumdl.toml");
    fs::write(
        &config,
        format!("[global]\nenable = [\"MD063\"]\n[MD063]\nenabled = true\nstyle = \"consistent\"\n{extra}"),
    )
    .unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(command.split_ascii_whitespace())
        .args(["--no-cache", "--config"])
        .arg(config)
        .arg(&path)
        .output()
        .expect("run rumdl");
    (output, fs::read_to_string(path).unwrap())
}

#[test]
fn consistent_accepts_title_case_and_sentence_case_documents() {
    for content in [
        "# Title Case Heading\n\n## Another Title Case Heading\n",
        "# Sentence case heading\n\n## Another sentence case heading\n",
    ] {
        let (output, unchanged) = run("check", content, "");
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(unchanged, content);
    }
}

fn assert_rewrites(content: &str, expected: &str, extra: &str) {
    let (check, _) = run("check", content, extra);
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    assert!(String::from_utf8_lossy(&check.stdout).contains("[MD063]"));
    for command in ["fmt", "check --fix"] {
        let (output, formatted) = run(command, content, extra);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(formatted, expected);
    }
    let (check, _) = run("check", expected, extra);
    assert!(check.status.success(), "{check:?}");
    let (output, second) = run("fmt", expected, extra);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(second, expected, "formatting must be idempotent");
}

#[test]
fn ambiguous_headings_wait_for_sentence_case_evidence() {
    assert_rewrites(
        "# Introduction\n\n## Getting started\n\n### Advanced Configuration\n",
        "# Introduction\n\n## Getting started\n\n### Advanced configuration\n",
        "",
    );
}

#[test]
fn partial_style_evidence_is_not_overridden_by_a_later_heading() {
    assert_rewrites("# API X\n\n## Getting started\n", "# API X\n\n## Getting Started\n", "");
}

#[test]
fn invalid_headings_do_not_override_the_first_clear_style() {
    assert_rewrites(
        "# getting started\n\n## Advanced Configuration\n\n### More details\n",
        "# Getting Started\n\n## Advanced Configuration\n\n### More Details\n",
        "",
    );
}

#[test]
fn all_caps_is_detected_when_acronym_preservation_is_disabled() {
    assert_rewrites(
        "# GETTING STARTED\n\n## More Details\n",
        "# GETTING STARTED\n\n## MORE DETAILS\n",
        "preserve-cased-words = false\n",
    );
}

#[test]
fn disabled_excluded_and_code_headings_do_not_choose_the_style() {
    assert_rewrites(
        "# Title Case Heading\n\n<!-- rumdl-disable MD063 -->\n## Another Title Heading\n<!-- rumdl-enable MD063 -->\n\n```md\n## Code Title Heading\n```\n\n## Sentence case heading\n\n### More Details\n",
        "# Title Case Heading\n\n<!-- rumdl-disable MD063 -->\n## Another Title Heading\n<!-- rumdl-enable MD063 -->\n\n```md\n## Code Title Heading\n```\n\n## Sentence case heading\n\n### More details\n",
        "min-level = 2\n",
    );
}

#[test]
fn setext_and_container_headings_share_the_document_style() {
    assert_rewrites(
        "Getting started\n===============\n\n> ## Advanced Configuration\n\n- ### More Details\n",
        "Getting started\n===============\n\n> ## Advanced configuration\n\n- ### More details\n",
        "",
    );
}

#[test]
fn multiline_setext_at_document_start_can_choose_the_style() {
    assert_rewrites(
        "Getting\nstarted\n=======\n\n## More Details\n",
        "Getting\nstarted\n=======\n\n## More details\n",
        "",
    );
}

#[test]
fn disabling_one_line_of_a_multiline_heading_excludes_all_its_evidence() {
    assert_rewrites(
        "Getting <!-- rumdl-disable-next-line MD063 -->\nStarted\n=======\n\n## Sentence case heading\n\n### More Details\n",
        "Getting <!-- rumdl-disable-next-line MD063 -->\nStarted\n=======\n\n## Sentence case heading\n\n### More details\n",
        "",
    );
}

#[test]
fn attributes_and_closing_hashes_are_preserved() {
    assert_rewrites(
        "# Getting started {#GettingStarted}\n\n## More Details ##\n",
        "# Getting started {#GettingStarted}\n\n## More details ##\n",
        "",
    );
}

#[test]
fn protected_content_and_proper_names_keep_their_existing_behavior() {
    assert_rewrites(
        "# `Title Case Code`\n\n## Using iPhone with new Acme Widget\n\n### More Details about [Acme Widget](https://example.test/TitleCase)\n",
        "# `Title Case Code`\n\n## Using iPhone with new Acme Widget\n\n### More details about [Acme Widget](https://example.test/TitleCase)\n",
        "ignore-words = [\"iPhone\"]\n[MD044]\nnames = [\"Acme Widget\"]\n",
    );
}

#[test]
fn protected_proper_nouns_do_not_force_title_case() {
    let content = "# Install Rust\n\n## Using basics\n\n### More Details\n";
    assert_rewrites(content, "# Install Rust\n\n## Using Basics\n\n### More Details\n", "");
    assert_rewrites(
        content,
        "# Install Rust\n\n## Using basics\n\n### More details\n",
        "ignore-words = [\"Rust\"]\n",
    );
}

#[test]
fn documents_without_clear_evidence_use_title_case() {
    assert_rewrites(
        "# getting started\n\n## more details\n",
        "# Getting Started\n\n## More Details\n",
        "",
    );
    for content in [
        "# Introduction\n\n## Overview\n",
        "# API\n\n## HTTP\n",
        "# 中文标题\n\n## `code example`\n",
    ] {
        let (output, formatted) = run("fmt", content, "");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(formatted, content);
    }
}

#[test]
fn one_invocation_detects_each_documents_style_independently() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[global]\nenable = [\"MD063\"]\n[MD063]\nenabled = true\nstyle = \"consistent\"\n",
    )
    .unwrap();
    let docs = [
        (
            "title.md",
            "# Title Case Heading\n\n## More details\n",
            "# Title Case Heading\n\n## More Details\n",
        ),
        (
            "sentence.md",
            "# Sentence case heading\n\n## More Details\n",
            "# Sentence case heading\n\n## More details\n",
        ),
    ];
    for (name, content, _) in docs {
        fs::write(dir.path().join(name), content).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["fmt", "--no-cache", "title.md", "sentence.md"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    for (name, _, expected) in docs {
        assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), expected);
    }
}

#[test]
fn deeply_nested_mdx_headings_can_be_checked_in_parallel() {
    let dir = tempdir().unwrap();
    let content = format!("# {}text{}\n", "<b>".repeat(8192), "</b>".repeat(8192));
    let paths: Vec<_> = (0..4).map(|i| dir.path().join(format!("doc-{i}.mdx"))).collect();
    for path in &paths {
        fs::write(path, &content).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "check",
            "--no-config",
            "--no-cache",
            "--enable",
            "MD063",
            "--flavor",
            "mdx",
        ])
        .args(&paths)
        .env("RAYON_NUM_THREADS", "2")
        .env_remove("RUST_MIN_STACK")
        .output()
        .expect("check deeply nested MDX on the default worker stack");
    assert!(output.status.success(), "{output:?}");
    for path in paths {
        assert_eq!(fs::read_to_string(path).unwrap(), content);
    }
}
