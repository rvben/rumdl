//! Semantic reflow keeps an aside together before applying its width limit.

use pulldown_cmark::{Event, Options, Parser};
use std::{fs, path::Path, process::Command};

fn structure(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut text = String::new();
    for event in Parser::new_ext(content, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(value) => text.push_str(&value),
            Event::SoftBreak => text.push(' '),
            other => {
                if !text.is_empty() {
                    result.push(format!("Text({})", std::mem::take(&mut text)));
                }
                result.push(format!("{other:?}"));
            }
        }
    }
    result
}

fn run(command: &str, path: &Path) -> std::process::Output {
    run_width(command, path, 80)
}

fn run_width(command: &str, path: &Path, width: usize) -> std::process::Output {
    run_settings(command, path, width, &[])
}

fn run_settings(command: &str, path: &Path, width: usize, settings: &[&str]) -> std::process::Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    process.args([
        command,
        "--isolated",
        "--no-cache",
        "--enable",
        "MD013",
        "-c",
        "MD013.reflow=true",
        "-c",
        "MD013.reflow-mode=\"semantic-line-breaks\"",
    ]);
    process.arg("-c").arg(format!("MD013.line-length={width}"));
    for setting in settings {
        process.arg("-c").arg(setting);
    }
    process.arg(path).output().unwrap()
}

#[test]
fn semantic_reflow_keeps_questions_inside_parenthetical_asides() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let input = "Ah, but a question mark surely ends a sentence (does it? I'm not sure).\n";
    fs::write(&path, input).unwrap();
    let first = run("fmt", &path);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), input);
    let check = run("check", &path);
    assert!(check.status.success(), "{check:?}");
    let second = run("fmt", &path);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), input);
}

#[test]
fn semantic_check_counts_the_sentences_that_fmt_splits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(
        &path,
        "This explanation keeps a parenthetical aside intact (does it? It really does). Next.\n",
    )
    .unwrap();
    let check = run("check", &path);
    let output = String::from_utf8_lossy(&check.stdout);
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    assert!(output.contains("semantic line breaks (2 sentences)"), "{check:?}");
    let formatted = run("fmt", &path);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "This explanation keeps a parenthetical aside intact (does it? It really does).\nNext.\n"
    );
    let check = run("check", &path);
    assert!(check.status.success(), "{check:?}");
}

#[test]
fn semantic_asides_respect_nested_markdown_and_literal_delimiters() {
    for (input, expected) in [
        (
            "An aside (can it (really? Yes!) work? It can). Done.\n",
            "An aside (can it (really? Yes!) work? It can).\nDone.\n",
        ),
        (
            "The aside (can `)` close it? It cannot) remains intact.\n",
            "The aside (can `)` close it? It cannot) remains intact.\n",
        ),
        (
            "An aside (is *this? Yes!* really useful) stays together.\n",
            "An aside (is *this? Yes!* really useful) stays together.\n",
        ),
        (
            "La méthode (vraiment? Oui!) réussit.\n",
            "La méthode (vraiment? Oui!) réussit.\n",
        ),
        (
            "Marks (。, ！, ？) listed. Next.\n",
            "Marks (。, ！, ？)\nlisted.\nNext.\n",
        ),
        ("A (。; ！) b.\n", "A (。; ！)\nb.\n"),
        (
            "(注意してください。) 次の手順です。\n",
            "(注意してください。)\n次の手順です。\n",
        ),
        ("Before (really? Yes, continue.\n", "Before (really?\nYes, continue.\n"),
        ("A stray ) ends nothing? Next.\n", "A stray ) ends nothing?\nNext.\n"),
        (
            "I'm sad :( today. It rained. Now I'm happy :) again.\n",
            "I'm sad :( today. It rained. Now I'm happy :) again.\n",
        ),
        (
            "The literal \\(really? Yes\\) remains ordinary prose.\n",
            "The literal \\(really?\nYes\\) remains ordinary prose.\n",
        ),
        ("Use `(` here? Yes, continue.\n", "Use `(` here?\nYes, continue.\n"),
        (
            "The [label](https://example.com/(x)) works? Yes.\n",
            "The [label](https://example.com/(x)) works?\nYes.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let output = run("fmt", &path);
        assert!(output.status.success(), "{input:?}: {output:?}");
        let formatted = fs::read_to_string(&path).unwrap();
        assert_eq!(formatted, expected, "{input:?}");
        assert_eq!(structure(input), structure(&formatted), "{input:?}");
        let check = run("check", &path);
        assert!(check.status.success(), "{input:?}: {check:?}");
        let output = run("fmt", &path);
        assert!(output.status.success(), "{input:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{input:?}");
    }
}

#[test]
fn semantic_reflow_splits_before_an_aside_when_the_sentence_exceeds_the_width() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let input = "Ah, but a question mark surely ends a sentence (does it? I'm not sure).\n";
    let expected = "Ah, but a question mark surely ends a sentence\n(does it? I'm not sure).\n";
    fs::write(&path, input).unwrap();
    let output = run_width("fmt", &path, 60);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    assert_eq!(structure(input), structure(expected));
    let check = run_width("check", &path, 60);
    assert!(check.status.success(), "{check:?}");
    let output = run_width("fmt", &path, 60);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn semantic_reflow_can_wrap_an_aside_that_is_itself_over_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let input = "The aside (really? Yes, one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen) continues.\n";
    fs::write(&path, input).unwrap();
    let output = run_width("fmt", &path, 40);
    assert!(output.status.success(), "{output:?}");
    let formatted = fs::read_to_string(&path).unwrap();
    assert_eq!(structure(input), structure(&formatted));
    assert!(formatted.lines().all(|line| line.chars().count() <= 40), "{formatted}");
    let check = run_width("check", &path, 40);
    assert!(check.status.success(), "{check:?}");
    let output = run_width("fmt", &path, 40);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), formatted);
}

#[test]
fn semantic_asides_stay_together_across_source_lines_and_inline_elements() {
    for (input, expected) in [
        (
            "An aside (is it?\n`x` yes) ends here.\n",
            "An aside (is it? `x` yes) ends here.\n",
        ),
        (
            "An aside (is it?\n[Yes](https://example.com) ok) ends here.\n",
            "An aside (is it? [Yes](https://example.com) ok) ends here.\n",
        ),
        (
            "- An aside (is it?\n  `x` yes) ends here.\n",
            "- An aside (is it? `x` yes) ends here.\n",
        ),
        (
            "> An aside (is it?\n> [Yes](https://example.com) ok) ends here.\n",
            "> An aside (is it? [Yes](https://example.com) ok) ends here.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let output = run("fmt", &path);
        assert!(output.status.success(), "{input:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{input:?}");
        let check = run("check", &path);
        assert!(check.status.success(), "{input:?}: {check:?}");
        let output = run("fmt", &path);
        assert!(output.status.success(), "{input:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{input:?}");
    }
}

#[test]
fn semantic_sentence_counts_honor_abbreviations_and_capitalization_in_containers() {
    let settings = ["MD013.abbreviations=[\"Acme\"]", "MD013.require-sentence-capital=false"];
    for (prefix, continuation) in [("", ""), ("- ", "  "), ("> ", "> ")] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(
            &path,
            format!("{prefix}An aside (really? Yes) mentions Acme. continues. Next.\n"),
        )
        .unwrap();
        let check = run_settings("check", &path, 80, &settings);
        assert_eq!(check.status.code(), Some(1), "{prefix:?}: {check:?}");
        assert!(
            String::from_utf8_lossy(&check.stdout).contains("semantic line breaks (2 sentences)"),
            "{prefix:?}: {check:?}"
        );
        let output = run_settings("fmt", &path, 80, &settings);
        assert!(output.status.success(), "{prefix:?}: {output:?}");
        let expected = format!("{prefix}An aside (really? Yes) mentions Acme. continues.\n{continuation}Next.\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{prefix:?}");
        let check = run_settings("check", &path, 80, &settings);
        assert!(check.status.success(), "{prefix:?}: {check:?}");
        let output = run_settings("fmt", &path, 80, &settings);
        assert!(output.status.success(), "{prefix:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{prefix:?}");
    }
}

#[test]
fn significant_unicode_spaces_before_links_remain_source_bytes() {
    for input in [
        "aé.\u{a0} [x](https://example.com) suite.\n",
        "😀.\u{202f} [x](https://example.com) suite.\n",
        "😀.\u{3000} [x](https://example.com) suite.\n",
        "ASCII.\u{a0} [x](https://example.com) suite.\n",
    ] {
        for mode in ["semantic-line-breaks", "sentence-per-line"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("doc.md");
            fs::write(&path, input).unwrap();
            let setting = format!("MD013.reflow-mode={mode:?}");
            let output = run_settings("fmt", &path, 80, &[&setting]);
            assert!(output.status.success(), "{input:?}, {mode}: {output:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
            let check = run_settings("check", &path, 80, &[&setting]);
            assert!(check.status.success(), "{input:?}, {mode}: {check:?}");
        }
    }
}

#[test]
fn long_protected_groups_with_inline_elements_wrap_without_losing_structure() {
    for (input, width) in [
        ("I'm sad :( today. It rained. Now I'm happy :) again.\n".to_string(), 40),
        (
            format!(
                "Lead ({}{}) ends here.\n",
                "Sentence with `code` here. ".repeat(100),
                "Really? Yes!"
            ),
            80,
        ),
        (
            format!(
                "Lead ({}{}) ends here.\n",
                "Sentence with `code` here. ".repeat(100),
                "Really? Yes!"
            ),
            0,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, &input).unwrap();
        let output = run_width("fmt", &path, width);
        assert!(output.status.success(), "{output:?}");
        let formatted = fs::read_to_string(&path).unwrap();
        assert_eq!(structure(&input), structure(&formatted));
        if width == 0 {
            assert_eq!(formatted, input);
        } else {
            assert!(
                formatted.lines().all(|line| line.chars().count() <= width),
                "{formatted}"
            );
        }
        let check = run_width("check", &path, width);
        assert!(check.status.success(), "{check:?}");
        let output = run_width("fmt", &path, width);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), formatted);
    }
}

#[test]
fn semantic_wrapped_container_prose_is_joined_and_noop_replacements_are_clean() {
    for (input, expected) in [
        (
            "- This explanation\n  continues here.\n",
            "- This explanation continues here.\n",
        ),
        (
            "> - This explanation\n>   continues here.\n",
            "> - This explanation continues here.\n",
        ),
        (
            "This clause continues,\nwhile this clause finishes.\n",
            "This clause continues, while this clause finishes.\n",
        ),
        (
            "- This clause continues,\n  while this clause finishes.\n",
            "- This clause continues, while this clause finishes.\n",
        ),
        (
            "> - This clause continues,\n>   while this clause finishes.\n",
            "> - This clause continues, while this clause finishes.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let check = run("check", &path);
        assert_eq!(check.status.code(), Some(1), "{check:?}");
        let output = run("fmt", &path);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        let check = run("check", &path);
        assert!(check.status.success(), "{check:?}");
    }
}

#[test]
fn overlong_asides_prefer_their_internal_sentence_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let input = "Lead (First sentence has enough words to use most of this line. Second sentence has enough words to use most of this line.) end.\n";
    fs::write(&path, input).unwrap();
    let output = run_width("fmt", &path, 80);
    assert!(output.status.success(), "{output:?}");
    let expected = "Lead (First sentence has enough words to use most of this line.\nSecond sentence has enough words to use most of this line.) end.\n";
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    assert_eq!(structure(input), structure(expected));
    let check = run_width("check", &path, 80);
    assert!(check.status.success(), "{check:?}");
    let output = run_width("fmt", &path, 80);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn sentence_per_line_preserves_emphasis_context_and_source_gaps_before_links() {
    for (input, expected) in [
        (
            "*First. [Second](https://example.com) continues.* Third.\n",
            "*First.\n[Second](https://example.com) continues.*\nThird.\n",
        ),
        (
            "First.\t[label](https://example.com) continues. Third.\n",
            "First.\t[label](https://example.com) continues.\nThird.\n",
        ),
        (
            "First.  [label](https://example.com) continues. Third.\n",
            "First.  [label](https://example.com) continues.\nThird.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let settings = ["MD013.reflow-mode=\"sentence-per-line\""];
        let output = run_settings("fmt", &path, 80, &settings);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(structure(input), structure(expected));
        let check = run_settings("check", &path, 80, &settings);
        assert!(check.status.success(), "{check:?}");
        let output = run_settings("fmt", &path, 80, &settings);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn group_edges_preserve_outer_sentences_inline_markup_and_hard_breaks() {
    for (input, expected) in [
        (
            "First. Second (a `x` b? `y` c) d. Third.\n",
            "First.\nSecond (a `x` b? `y` c) d.\nThird.\n",
        ),
        ("*lead (a? b* c) d. Next.\n", "*lead (a? b* c) d.\nNext.\n"),
        ("Lead (a? b\\\n`x` c) d. Next.\n", "Lead (a?\nb\\\n`x` c) d.\nNext.\n"),
        (
            "Lead (a `x`? b) then (c `y`? d) ends. Next.\n",
            "Lead (a `x`? b) then (c `y`? d) ends.\nNext.\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let output = run("fmt", &path);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(structure(input), structure(expected));
        let check = run("check", &path);
        assert!(check.status.success(), "{check:?}");
        let output = run("fmt", &path);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn semantic_diagnostics_count_paragraph_units_before_width_and_hard_break_splitting() {
    for (input, count) in [
        (
            "Lead (First sentence has enough words to use most of this line. Second sentence has enough words to use most of this line.) end.\n",
            1,
        ),
        ("Lead (a? b\\\n`x` c) d. Next.\n", 2),
        ("- Lead (a? b\\\n  `x` c) d. Next.\n", 2),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let check = run("check", &path);
        assert_eq!(check.status.code(), Some(1), "{check:?}");
        let message = format!("semantic line breaks ({count} sentences)");
        assert!(String::from_utf8_lossy(&check.stdout).contains(&message), "{check:?}");
        let output = run("fmt", &path);
        assert!(output.status.success(), "{output:?}");
        let formatted = fs::read_to_string(&path).unwrap();
        assert_eq!(structure(input), structure(&formatted));
        let check = run("check", &path);
        assert!(check.status.success(), "{check:?}");
    }
}
