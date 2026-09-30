use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

fn assert_sentence_pack(input: &str, expected: &str, width: usize, settings: &str) {
    let dir = tempdir().unwrap();
    let file = dir.path().join("input.md");
    let config = dir.path().join("rumdl.toml");
    fs::write(&file, input).unwrap();
    fs::write(
        &config,
        format!(
            r#"[global]
enable = ["MD013"]

[MD013]
reflow = true
reflow-mode = "sentence-pack"
line-length = {width}
code-blocks = false
tables = false
headings = false
math-blocks = false
{settings}
"#
        ),
    )
    .unwrap();

    let run = |args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(args)
            .arg("--no-cache")
            .arg("--config")
            .arg(&config)
            .arg(&file)
            .output()
            .unwrap()
    };
    let assert_status = |output: &Output, code: i32| {
        assert_eq!(
            output.status.code(),
            Some(code),
            "input: {input:?}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    };

    let changed = input != expected;
    let checked = run(&["check"]);
    assert_status(&checked, i32::from(changed));
    if changed {
        assert!(
            String::from_utf8_lossy(&checked.stdout).contains("MD013"),
            "check must identify MD013: {checked:?}"
        );
    }
    assert_status(&run(&["fmt", "--check"]), i32::from(changed));
    assert_eq!(fs::read_to_string(&file).unwrap(), input, "checks must not write");

    let formatted = run(&["fmt"]);
    assert!(
        formatted.status.success(),
        "fmt failed: {formatted:?}, input: {input:?}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    assert_status(&run(&["check"]), 0);
    assert_status(&run(&["fmt", "--check"]), 0);
    assert_status(&run(&["fmt"]), 0);
    assert_eq!(fs::read_to_string(&file).unwrap(), expected, "fmt must be idempotent");
}

#[test]
fn sentence_pack_short_sentences_and_width_boundary() {
    for width in [23, 24, 80] {
        assert_sentence_pack("First one.\nSecond one.\n", "First one. Second one.\n", width, "");
    }
    assert_sentence_pack("First one. Second one.\n", "First one.\nSecond one.\n", 21, "");
    assert_sentence_pack("First one. Second one.\n", "First one. Second one.\n", 22, "");
}

#[test]
fn sentence_pack_oversized_sentence_is_indivisible() {
    let long =
        "This deliberately long sentence remains one indivisible editing unit even when it exceeds the soft limit.";
    assert_sentence_pack(
        &format!("First one. Second one. {long} Last one.\n"),
        &format!("First one. Second one.\n{long}\nLast one.\n"),
        40,
        "",
    );
    assert_sentence_pack(&format!("{long}\n"), &format!("{long}\n"), 20, "strict = true");
    assert_sentence_pack(
        "This deliberately long sentence\nremains one indivisible editing unit.\n",
        "This deliberately long sentence remains one indivisible editing unit.\n",
        20,
        "",
    );
}

#[test]
fn sentence_pack_lists_include_marker_and_continuation_width() {
    for (prefix, indent) in [("- ", "  "), ("12. ", "    "), ("- [ ] ", "      ")] {
        let packed = format!("{prefix}First one. Second one.\n");
        let split = format!("{prefix}First one.\n{indent}Second one.\n");
        assert_sentence_pack(&split, &packed, 22 + prefix.len(), "");
        assert_sentence_pack(&packed, &split, 21 + prefix.len(), "");
        assert_sentence_pack(
            &format!("{prefix}One sentence split\n{indent}across lines and longer than the limit.\n"),
            &format!("{prefix}One sentence split across lines and longer than the limit.\n"),
            20,
            "",
        );
    }
}

#[test]
fn sentence_pack_uses_each_lines_actual_prefix_budget() {
    assert_sentence_pack("- First one.\n    Second one.\n", "- First one. Second one.\n", 24, "");
    assert_sentence_pack(
        "- [ ] First one.\n  Second one. Third one.\n",
        "- [ ] First one.\n  Second one. Third one.\n",
        24,
        "",
    );
}

#[test]
fn sentence_pack_blockquotes_include_prefix_width() {
    for prefix in ["> ", "> > "] {
        let packed = format!("{prefix}First one. Second one.\n");
        let split = format!("{prefix}First one.\n{prefix}Second one.\n");
        assert_sentence_pack(&split, &packed, 22 + prefix.len(), "");
        assert_sentence_pack(&packed, &split, 21 + prefix.len(), "");
    }
    assert_sentence_pack(
        "> - First one.\n>   Second one.\n",
        "> - First one. Second one.\n",
        26,
        "",
    );
    assert_sentence_pack(
        "> - First one. Second one.\n",
        "> - First one.\n>   Second one.\n",
        25,
        "",
    );
}

#[test]
fn sentence_pack_nested_lists_preserve_boundaries() {
    assert_sentence_pack(
        "- First one.\n  Second one.\n  - Child one.\n    Child two.\n- Last one.\n",
        "- First one. Second one.\n  - Child one. Child two.\n- Last one.\n",
        80,
        "",
    );
}

#[test]
fn sentence_pack_preserves_both_hard_break_forms() {
    for marker in ["  ", "\\"] {
        for (prefix, indent) in [("", ""), ("- ", "  "), ("> ", "> ")] {
            let input = format!("{prefix}First one.{marker}\n{indent}Second one.\n{indent}Third one.\n");
            let expected = format!("{prefix}First one.{marker}\n{indent}Second one. Third one.\n");
            assert_sentence_pack(&input, &expected, 80, "");
        }
    }
}

#[test]
fn sentence_pack_links_and_code_spans_stay_whole() {
    assert_sentence_pack(
        "Read [First. Second](https://example.com) now. Use `first. Second` here. Done.\n",
        "Read [First. Second](https://example.com) now.\nUse `first. Second` here. Done.\n",
        35,
        "",
    );
    assert_sentence_pack(
        "Read the\n[manual](https://example.com) now.\n",
        "Read the [manual](https://example.com) now.\n",
        20,
        "",
    );
    assert_sentence_pack(
        "Read [First. Second][ref] now. Done.\n\n[ref]: https://example.com\n",
        "Read [First. Second][ref] now.\nDone.\n\n[ref]: https://example.com\n",
        20,
        "",
    );
}

#[test]
fn sentence_pack_custom_abbreviations_are_shared_by_check_and_fmt() {
    for (prefix, indent) in [("", ""), ("- ", "  "), ("> ", "> ")] {
        assert_sentence_pack(
            &format!("{prefix}Use Acme.\n{indent}Widgets today. Done.\n"),
            &format!("{prefix}Use Acme. Widgets today.\n{indent}Done.\n"),
            20,
            "abbreviations = [\"Acme\"]",
        );
    }
    assert_sentence_pack(
        "First one. second one.\n",
        "First one.\nsecond one.\n",
        15,
        "require-sentence-capital = false",
    );
}

#[test]
fn sentence_pack_unlimited_width_keeps_paragraph_boundaries() {
    assert_sentence_pack(
        "First one.\nSecond one.\n\nThird one.\nFourth one.\n",
        "First one. Second one.\n\nThird one. Fourth one.\n",
        0,
        "",
    );
}

#[test]
fn sentence_pack_length_modes() {
    let packed = "\u{754c} done. Next one.\n";
    let split = "\u{754c} done.\nNext one.\n";
    assert_sentence_pack(split, packed, 17, "length-mode = \"chars\"");
    assert_sentence_pack(packed, split, 17, "length-mode = \"visual\"");
    assert_sentence_pack(split, packed, 18, "length-mode = \"visual\"");
    assert_sentence_pack(packed, split, 18, "length-mode = \"bytes\"");
    assert_sentence_pack(split, packed, 19, "length-mode = \"bytes\"");
}

#[test]
fn sentence_pack_honors_cjk_soft_break_provenance() {
    let settings = "cjk-soft-break = \"join\"";
    assert_sentence_pack("第一句。\n第二句。\n", "第一句。第二句。\n", 80, settings);
    assert_sentence_pack("第一句。第二句。\n", "第一句。第二句。\n", 80, settings);
    assert_sentence_pack("第一句。 第二句。\n", "第一句。 第二句。\n", 80, settings);
    assert_sentence_pack("第一句。  第二句。\n", "第一句。  第二句。\n", 80, settings);
    assert_sentence_pack("第一句。\u{a0}第二句。\n", "第一句。\u{a0}第二句。\n", 80, settings);
    assert_sentence_pack("第一句。\u{3000}第二句。\n", "第一句。\u{3000}第二句。\n", 80, settings);
    assert_sentence_pack("第一句。 第二句。\n", "第一句。 第二句。\n", 16, settings);
    assert_sentence_pack("第一句。  第二句。\n", "第一句。  第二句。\n", 16, settings);
    assert_sentence_pack("第一句。\u{a0}第二句。\n", "第一句。\u{a0}第二句。\n", 16, settings);
    assert_sentence_pack("第一句。\u{3000}第二句。\n", "第一句。\u{3000}第二句。\n", 16, settings);
    assert_sentence_pack(
        "第一句。第二句。 第三句。\n",
        "第一句。\n第二句。 第三句。\n",
        17,
        settings,
    );
    assert_sentence_pack(
        "这段测试\n文字尚未结束。 Second sentence.\n",
        "这段测试文字尚未结束。 Second sentence.\n",
        80,
        settings,
    );
    assert_sentence_pack(
        "Intro.  - 项目结束。下一句。\n",
        "Intro. - 项目结束。下一句。\n",
        100,
        "cjk-soft-break = \"join\"\nrequire-sentence-capital = false",
    );
}

#[test]
fn sentence_pack_preserves_protected_blocks() {
    let input = "# First one. Second one.\n\n```text\nFirst one. Second one.\n```\n\n    First one. Second one.\n\n| First one. Second one. |\n| --- |\n| Third one. Fourth one. |\n\nFirst one.\nSecond one.\n";
    let expected = input.replace("\nFirst one.\nSecond one.\n", "\nFirst one. Second one.\n");
    assert_sentence_pack(input, &expected, 30, "");
}

#[test]
fn sentence_pack_preserves_crlf_and_missing_final_newline() {
    assert_sentence_pack("First one.\r\nSecond one.\r\n", "First one. Second one.\r\n", 80, "");
    assert_sentence_pack("First one.\nSecond one.", "First one. Second one.", 80, "");
}

#[test]
fn sentence_pack_does_not_create_block_syntax() {
    let input = "First sentence! - This must stay prose. Last one.\n";
    assert_sentence_pack(input, "First sentence! - This must stay prose.\nLast one.\n", 20, "");
}

#[test]
fn sentence_pack_length_exemptions_use_the_same_budget() {
    let input = "Read [manual](https://example.com/a/very/long/path). Done.\n";
    let split = "Read [manual](https://example.com/a/very/long/path).\nDone.\n";
    assert_sentence_pack(input, split, 25, "");
    assert_sentence_pack(split, input, 25, "reflow-length-exemptions = true");
    assert_sentence_pack(
        "Use `a very long code span`. Done.\n",
        "Use `a very long code span`. Done.\n",
        15,
        "reflow-length-exemptions = true\ncode-spans = false",
    );
    assert_sentence_pack(
        "First one. Second one.\n",
        "First one.\nSecond one.\n",
        15,
        "atomic-spans = false\nreflow-break-link-text = true",
    );
}

#[test]
fn sentence_pack_keeps_inline_spans_and_punctuation() {
    for (input, expected) in [
        (
            "**First one. Second one.** Third one.\n",
            "**First one.\nSecond one.**\nThird one.\n",
        ),
        ("Use `a. B` here. Next one.\n", "Use `a. B` here.\nNext one.\n"),
        ("He said \"Done.\" Next one.\n", "He said \"Done.\"\nNext one.\n"),
        ("Use Dr. Smith today. Next one.\n", "Use Dr. Smith today.\nNext one.\n"),
        ("Use [a. B](url) now. Next one.\n", "Use [a. B](url) now.\nNext one.\n"),
    ] {
        assert_sentence_pack(input, expected, 20, "");
    }
}

#[test]
fn sentence_pack_lazy_blockquote_continuations() {
    assert_sentence_pack("> First one.\nSecond one.\n", "> First one. Second one.\n", 24, "");
    assert_sentence_pack(
        "> First sentence.\nSecond sentence.\nTiny phrase.\n",
        "> First sentence.\nSecond sentence. Tiny phrase.\n",
        30,
        "",
    );
}

#[test]
fn sentence_pack_footnotes_join_short_lines() {
    assert_sentence_pack(
        "Text[^1].\n\n[^1]: First one.\n    Second one.\n",
        "Text[^1].\n\n[^1]: First one. Second one.\n",
        80,
        "",
    );
    assert_sentence_pack(
        "Text[^1].\n\n[^1]: First one. Second one. Third one.\n",
        "Text[^1].\n\n[^1]: First one.\n    Second one. Third one.\n",
        26,
        "",
    );
    for marker in ["  ", "\\"] {
        let input = format!("Text[^1].\n\n[^1]: First one.{marker}\n    Second one.\n    Third one.\n");
        assert_sentence_pack(&input, &input, 80, "");
    }
}
