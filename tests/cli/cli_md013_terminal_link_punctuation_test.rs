//! Opt-in sentence ends at terminal link text or image alt punctuation.

use pulldown_cmark::{Event, Options, Parser};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

fn structure(content: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut text = String::new();
    for event in Parser::new_ext(content, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES) {
        match event {
            Event::Text(value) => text.push_str(&value),
            Event::SoftBreak => text.push(' '),
            other => {
                if !text.is_empty() {
                    events.push(format!("Text({})", std::mem::take(&mut text)));
                }
                events.push(format!("{other:?}"));
            }
        }
    }
    if !text.is_empty() {
        events.push(format!("Text({text})"));
    }
    events
}

fn cli(command: &str, path: &std::path::Path, config: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(command.split_ascii_whitespace())
        .args(["--no-cache", "--config"])
        .arg(config)
        .arg(path)
        .output()
        .expect("run rumdl")
}

fn assert_reflows(content: &str, expected: &str, extra: &str) {
    assert_with_config(
        content,
        expected,
        &format!("reflow-mode = \"sentence-per-line\"\nlink-text-punctuation = \"trailing\"\n{extra}"),
    );
}

fn assert_with_config(content: &str, expected: &str, settings: &str) {
    assert_at_width(content, expected, settings, 0);
}

fn assert_at_width(content: &str, expected: &str, settings: &str, width: usize) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config = dir.path().join(".rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(
        &config,
        format!("[global]\nenable = [\"MD013\"]\n[MD013]\nline-length = {width}\nreflow = true\n{settings}"),
    )
    .unwrap();
    let check = cli("check", &path, &config);
    assert_eq!(
        check.status.code(),
        Some(i32::from(content != expected)),
        "{content:?}: {check:?}"
    );
    if content != expected {
        assert!(
            String::from_utf8_lossy(&check.stdout).contains("[MD013]"),
            "{content:?}: {check:?}"
        );
    }
    assert!(check.stderr.is_empty(), "{content:?}: {check:?}");
    for command in ["fmt", "check --fix"] {
        fs::write(&path, content).unwrap();
        let output = cli(command, &path, &config);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let formatted = fs::read_to_string(&path).unwrap();
        assert_eq!(formatted, expected);
        assert_eq!(structure(content), structure(&formatted), "rendered structure changed");
        let check = cli("check", &path, &config);
        assert!(check.status.success(), "{content:?}: {check:?}");
        let second = cli("fmt", &path, &config);
        assert!(second.status.success(), "{second:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "fmt is not idempotent");
    }
}

#[test]
fn terminal_link_and_image_punctuation_can_end_the_sentence() {
    assert_reflows(
        "Read [Done.](https://example.test) Next sentence.\n\nSee ![Done.](image.png) Next sentence.\n\n查看 ![完成。](image.png) 下一句开始。\n",
        "Read [Done.](https://example.test)\nNext sentence.\n\nSee ![Done.](image.png)\nNext sentence.\n\n查看 ![完成。](image.png)\n下一句开始。\n",
        "",
    );
}

#[test]
fn link_labels_need_a_strict_sentence_opener_even_in_relaxed_mode() {
    for extra in ["", "require-sentence-capital = false"] {
        for content in [
            "Visit [Yahoo!](https://yahoo.test) today.\n",
            "See [What's new?](changelog) for details.\n",
            "Read [the docs.](docs) then continue.\n",
            "Read [Done.](docs) `next_part` continues.\n",
        ] {
            assert_reflows(content, content, extra);
        }
        assert_reflows(
            "Read [Done.](docs) 2. Next item.\n",
            "Read [Done.](docs) 2.\nNext item.\n",
            extra,
        );
        assert_reflows(
            "Read [Done.](docs)\nnext part.\n",
            "Read [Done.](docs) next part.\n",
            extra,
        );
    }
}

#[test]
fn only_terminal_visible_literal_punctuation_counts() {
    for content in [
        "Read [docs](https://example.test/v1. \"Title.\") Next part.\n",
        "Read [`foo.`](docs) Next part.\n",
        "Read [Done.<!-- note -->](docs) Next part.\n",
        "Read [Done&#46;](docs) Next part.\n",
        "Read [Done. More words](docs) before continuing.\n",
        "Read [Done.\\*](docs) Next part.\n",
        "Visit <https://example.test/a.> Next part.\n",
        "See ![<https://example.test/a.>](image.png) Next part.\n",
        "Read [![Done.](image.png) more](docs) Next part.\n",
        "Read [Done.](docs), next part.\n",
        "Read [Done.][missing] Next part.\n",
        "Read [Done.][] Next part.\n",
    ] {
        assert_reflows(content, content, "");
    }
    assert_reflows(
        "See ![Done](image.png). Next sentence.\n",
        "See ![Done](image.png).\nNext sentence.\n",
        "",
    );
    assert_reflows(
        "See ![Done. More words](image.png) before continuing. Next sentence.\n",
        "See ![Done. More words](image.png) before continuing.\nNext sentence.\n",
        "",
    );
}

#[test]
fn label_abbreviations_initials_and_numeric_labels_stay_together() {
    for content in [
        "Ask [Dr.](docs) Smith.\n",
        "See [e.g.](docs) Next example.\n",
        "By [J.](docs) Rowling.\n",
        "See [1.](docs) Next item.\n",
        "See [v2.](docs) Next release.\n",
        "See [Acme.](docs) Next release.\n",
    ] {
        assert_reflows(content, content, "abbreviations = [\"Acme\"]");
    }
    assert_reflows(
        "Read [Wait...](docs) Then continue.\n",
        "Read [Wait...](docs)\nThen continue.\n",
        "",
    );
    assert_reflows(
        "Read [Done\\.](docs) Next sentence.\n",
        "Read [Done\\.](docs)\nNext sentence.\n",
        "",
    );
}

#[test]
fn formatting_nesting_and_external_closers_travel_with_the_whole_link() {
    for (source, expected) in [
        (
            "Read **[Done.](docs)** Next sentence.\n",
            "Read **[Done.](docs)**\nNext sentence.\n",
        ),
        (
            "Read \"[Done.](docs)\" Next sentence.\n",
            "Read \"[Done.](docs)\"\nNext sentence.\n",
        ),
        (
            "Read [**Done.**](docs) Next sentence.\n",
            "Read [**Done.**](docs)\nNext sentence.\n",
        ),
        (
            "Read [![Done.](image.png)](docs) Next sentence.\n",
            "Read [![Done.](image.png)](docs)\nNext sentence.\n",
        ),
        (
            "First. Read [Done.](docs) Next sentence.\n",
            "First.\nRead [Done.](docs)\nNext sentence.\n",
        ),
        (
            "Read [Done.](docs) [Next.](other)\n",
            "Read [Done.](docs)\n[Next.](other)\n",
        ),
        (
            "Read [Done.](docs\n\"Title.\") Next sentence.\n",
            "Read [Done.](docs \"Title.\")\nNext sentence.\n",
        ),
        (
            "Read [Done.](docs)[^note] Next sentence.\n\n[^note]: A note.\n",
            "Read [Done.](docs)[^note]\nNext sentence.\n\n[^note]: A note.\n",
        ),
    ] {
        assert_reflows(source, expected, "");
    }
}

#[test]
fn defined_reference_links_and_images_break_after_the_complete_construct() {
    for link in [
        "[Done.][ref]",
        "[Done.][]",
        "[Done.]",
        "![Done.][ref]",
        "![Done.][]",
        "![Done.]",
    ] {
        let definitions = "\n[ref]: https://example.test \"Title.\"\n[Done.]: image.png\n";
        assert_reflows(
            &format!("Read {link} Next sentence.\n{definitions}"),
            &format!("Read {link}\nNext sentence.\n{definitions}"),
            "",
        );
    }
}

#[test]
fn cjk_boundaries_preserve_existing_spacing_in_both_join_modes() {
    for mode in ["space", "join"] {
        let extra = format!("cjk-soft-break = \"{mode}\"");
        assert_reflows(
            "查看 ![完成。](image.png) 下一句开始。\n",
            "查看 ![完成。](image.png)\n下一句开始。\n",
            &extra,
        );
        assert_reflows(
            "查看![完成。](image.png)下一句开始。\n",
            "查看![完成。](image.png)下一句开始。\n",
            &extra,
        );
    }
}

#[test]
fn default_and_explicit_ignore_leave_link_punctuation_opaque() {
    let content = "Read [Done.](docs) Next sentence.\n\nSee ![Done.](image.png) Next sentence.\n\n查看 ![完成。](image.png) 下一句开始。\n";
    for policy in ["", "link-text-punctuation = \"ignore\""] {
        assert_with_config(
            content,
            content,
            &format!("reflow-mode = \"sentence-per-line\"\n{policy}"),
        );
    }
}

#[test]
fn sentence_modes_and_containers_share_the_same_boundary_policy() {
    for (source, expected) in [
        (
            "Read [Done.](docs) Next sentence.\n",
            "Read [Done.](docs)\nNext sentence.\n",
        ),
        (
            "> Read [Done.](docs) Next sentence.\n",
            "> Read [Done.](docs)\n> Next sentence.\n",
        ),
        (
            "- Read [Done.](docs) Next sentence.\n",
            "- Read [Done.](docs)\n  Next sentence.\n",
        ),
    ] {
        for mode in ["sentence-per-line", "semantic-line-breaks", "sentence-pack"] {
            let settings = format!("reflow-mode = \"{mode}\"\nlink-text-punctuation = \"trailing\"");
            // With an unlimited budget, sentence-pack retains both sentences on one line.
            assert_with_config(
                source,
                if mode == "sentence-pack" { source } else { expected },
                &settings,
            );
        }
    }
    let aside = "Read (see [Done.](docs) Next part) before continuing.\n";
    assert_with_config(
        aside,
        aside,
        "reflow-mode = \"semantic-line-breaks\"\nlink-text-punctuation = \"trailing\"",
    );
}

#[test]
fn punctuation_only_labels_cannot_end_a_sentence() {
    for text in ["[...]", "[?]", "[.]", "![！]"] {
        let content = format!("See {text}(docs) Next part.\n");
        assert_reflows(&content, &content, "");
    }
}

#[test]
fn final_atomic_constructs_and_existing_link_breaks_are_stable() {
    for content in ["Read [Done.](docs)\n", "Read `done.`\n"] {
        assert_reflows(content, content, "");
        assert_with_config(content, content, "reflow-mode = \"sentence-per-line\"");
    }
    let content = "Read [Done.](docs)\nNext sentence.\n";
    assert_reflows(content, content, "");
    assert_with_config(
        content,
        "Read [Done.](docs) Next sentence.\n",
        "reflow-mode = \"sentence-per-line\"\nlink-text-punctuation = \"ignore\"",
    );
}

#[test]
fn finite_width_semantic_reflow_preserves_explicit_sentence_boundaries() {
    let settings = "reflow-mode = \"semantic-line-breaks\"\nlink-text-punctuation = \"trailing\"";
    assert_at_width("Read [Done.](docs) Go.\n", "Read [Done.](docs)\nGo.\n", settings, 40);
    assert_at_width("**Done.** Go.\n", "**Done.**\nGo.\n", settings, 40);
}

#[test]
fn finite_width_sentence_pack_uses_label_boundaries() {
    assert_at_width(
        "Read [Done.](docs) Next sentence.\n",
        "Read [Done.](docs)\nNext sentence.\n",
        "reflow-mode = \"sentence-pack\"\nlink-text-punctuation = \"trailing\"",
        25,
    );
}

#[test]
fn configuration_alias_defaults_and_round_trip_work_through_the_cli() {
    for key in ["link-text-punctuation", "link_text_punctuation"] {
        assert_with_config(
            "Read [Done.](docs) Next.\n",
            "Read [Done.](docs)\nNext.\n",
            &format!("reflow-mode = \"sentence-per-line\"\n{key} = \"trailing\""),
        );
    }
    let defaults = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "--isolated", "--defaults", "--output", "toml"])
        .output()
        .unwrap();
    assert!(defaults.status.success(), "{defaults:?}");
    let value: toml::Value = toml::from_str(std::str::from_utf8(&defaults.stdout).unwrap()).unwrap();
    assert_eq!(value["MD013"]["link-text-punctuation"].as_str(), Some("ignore"));
    let dir = tempdir().unwrap();
    let config = dir.path().join("round-trip.toml");
    let serialized = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "config",
            "--isolated",
            "--output",
            "toml",
            "-c",
            "MD013.link-text-punctuation=\"trailing\"",
        ])
        .output()
        .unwrap();
    assert!(serialized.status.success(), "{serialized:?}");
    fs::write(&config, &serialized.stdout).unwrap();
    let loaded = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "--output", "toml", "--config"])
        .arg(&config)
        .output()
        .unwrap();
    assert!(loaded.status.success(), "{loaded:?}");
    let value: toml::Value = toml::from_str(std::str::from_utf8(&loaded.stdout).unwrap()).unwrap();
    assert_eq!(value["MD013"]["link-text-punctuation"].as_str(), Some("trailing"));
    let invalid = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "check",
            "--isolated",
            "-c",
            "MD013.link-text-punctuation=\"invalid\"",
            "--stdin",
        ])
        .output()
        .unwrap();
    // The CLI reports invalid rule values and falls back to defaults.
    assert!(invalid.status.success(), "{invalid:?}");
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("unknown variant `invalid`, expected `ignore` or `trailing`"),
        "{invalid:?}"
    );
}
