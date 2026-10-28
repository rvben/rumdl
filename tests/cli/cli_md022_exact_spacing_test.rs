//! Exact heading spacing through both CLI fix paths.
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn assert_spacing(input: &str, expected: &str, settings: &str) {
    for args in [vec!["fmt"], vec!["check", "--fix"]] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        let config = dir.path().join(".rumdl.toml");
        fs::write(&path, input).unwrap();
        let below = if settings.contains("lines-below") {
            ""
        } else {
            "lines-below = 0\n"
        };
        fs::write(
            &config,
            format!("[global]\nenable = [\"MD022\"]\n[MD022]\n{below}{settings}\n"),
        )
        .unwrap();
        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(args)
                .args(["--no-cache", "--config"])
                .arg(&config)
                .arg(&path)
                .output()
                .unwrap()
        };
        let output = run(&args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{args:?}");
        let output = run(&["check"]);
        assert!(output.status.success(), "fixed document must pass: {output:?}");
        let output = run(&args);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            expected,
            "second fix must be idempotent"
        );
    }
}

#[test]
fn heading_spacing_is_enforced_before_lists_and_fences_without_other_rules() {
    for block in ["- item", "1. item", "```rust\ncode\n```", "~~~\ncode\n~~~"] {
        assert_spacing(
            &format!("# Heading\n{block}\n"),
            &format!("# Heading\n\n{block}\n"),
            "lines-below = 1",
        );
        assert_spacing(
            &format!("# Heading\n{block}\n"),
            &format!("# Heading\n{block}\n"),
            "lines-below = 0",
        );
    }
    assert_spacing("# Heading\n- item\n", "# Heading\n\n\n- item\n", "lines-below = 2");
    assert_spacing(
        "> # Heading\n> - item\n",
        "> # Heading\n>\n> - item\n",
        "lines-below = 1",
    );
    assert_spacing(
        "Heading\n=======\n- item",
        "Heading\n=======\n\n- item",
        "lines-below = 1",
    );
}

#[test]
fn markdownlint_config_enforces_heading_spacing_with_list_rule_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config = dir.path().join(".markdownlint.jsonc");
    fs::write(
        &config,
        r#"{"default": false, "MD019": true, "MD022": {"lines_above": 1, "lines_below": 1}}"#,
    )
    .unwrap();
    for args in [vec!["fmt"], vec!["check", "--fix"]] {
        fs::write(&path, "# Heading\n- list item\n").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(&args)
            .args(["--no-cache", "--config"])
            .arg(&config)
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Heading\n\n- list item\n");
    }
}

#[test]
fn overlapping_heading_list_and_fence_rules_insert_only_one_blank() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    for block in ["- item", "```rust\ncode\n```"] {
        fs::write(&path, format!("# Heading\n{block}\n")).unwrap();
        for _ in 0..2 {
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args([
                    "check",
                    "--fix",
                    "--isolated",
                    "--no-cache",
                    "--enable",
                    "MD022,MD031,MD032",
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), format!("# Heading\n\n{block}\n"));
        }
    }
}

#[test]
fn exact_spacing_preserves_comments_and_comment_interiors() {
    assert_spacing(
        "# Title\n\n\n<!-- KEEP ME -->\n## Section\n",
        "# Title\n\n<!-- KEEP ME -->\n## Section\n",
        "lines-above-mode = \"exact\"",
    );
    assert_spacing(
        "# Title\n\n\n<!--\n\nKEEP ME\n\n-->\n## Section\n",
        "# Title\n\n<!--\n\nKEEP ME\n\n-->\n## Section\n",
        "lines-above-mode = \"exact\"",
    );
    assert_spacing(
        "# Title\n<!-- first -->\n<!-- second -->\n## Section\n",
        "# Title\n<!-- first -->\n<!-- second -->\n## Section\n",
        "lines-above-mode = \"exact\"",
    );
}

#[test]
fn exact_spacing_preserves_crlf_and_missing_final_newline() {
    assert_spacing(
        "# Title\r\n\r\n\r\n## Section\r\n",
        "# Title\r\n\r\n## Section\r\n",
        "lines-above-mode = \"exact\"",
    );
    assert_spacing(
        "# Title\n\n\n## Section",
        "# Title\n\n## Section",
        "lines-above-mode = \"exact\"",
    );
}

#[test]
fn minimum_remains_the_default_and_exact_adds_missing_blanks() {
    let input = "# Title\n\n\n## Section\n";
    assert_spacing(input, input, "");
    assert_spacing(input, input, "lines-above-mode = \"minimum\"");
    assert_spacing(
        "# Title\n## Section\n",
        "# Title\n\n## Section\n",
        "lines-above-mode = \"exact\"",
    );
}

#[test]
fn exact_spacing_uses_per_level_zero_and_unlimited_requirements() {
    assert_spacing(
        "# Title\n\n\n\n## Section\n\n\n\n### Subsection\n",
        "# Title\n\n\n## Section\n\n\n\n### Subsection\n",
        "lines-above-mode = \"exact\"\nlines-above = [1, 2, -1, 1, 1, 1]",
    );
    assert_spacing(
        "text\n\n\n## Section\n",
        "text\n## Section\n",
        "lines-above-mode = \"exact\"\nlines-above = 0",
    );
}

#[test]
fn exact_spacing_handles_multiline_setext_and_quoted_headings() {
    assert_spacing(
        "# Title\n\n\nSome\nheading\n===\n",
        "# Title\n\nSome\nheading\n===\n",
        "lines-above-mode = \"exact\"",
    );
    assert_spacing(
        "> # Title\n>\n>\n> ## Section\n",
        "> # Title\n>\n> ## Section\n",
        "lines-above-mode = \"exact\"",
    );
    let input = "> # First quote\n\n\n> ## Second quote\n";
    assert_spacing(input, input, "lines-above-mode = \"exact\"");
}

#[test]
fn exact_spacing_respects_inline_disables_and_container_exemptions() {
    let disabled = "# Title\n\n\n<!-- rumdl-disable MD022 -->\n## Section\n<!-- rumdl-enable MD022 -->\n";
    assert_spacing(disabled, disabled, "lines-above-mode = \"exact\"");
    let list = "-\n  ## Section\n";
    assert_spacing(list, list, "lines-above-mode = \"exact\"");
    let start = "---\ntitle: example\n---\n\n\n# Title\n";
    assert_spacing(start, start, "lines-above-mode = \"exact\"");
    let code = "# Title\n\n```md\n\n\n## Not a heading\n```\n";
    assert_spacing(code, code, "lines-above-mode = \"exact\"");
}

#[test]
fn exact_zero_retains_structural_separators() {
    assert_spacing(
        "paragraph\n\n\nHeading\n=======\n",
        "paragraph\n\nHeading\n=======\n",
        "lines-above-mode = \"exact\"\nlines-above = 0",
    );
    assert_spacing(
        "<div>\nbody\n</div>\n\n\n# Heading\n",
        "<div>\nbody\n</div>\n\n# Heading\n",
        "lines-above-mode = \"exact\"\nlines-above = 0",
    );
}

#[test]
fn exact_spacing_preserves_other_blocks_and_disabled_blank_runs() {
    for input in [
        "Para\n\n    <!-- x -->\n\n## Heading\n",
        "# Title\n\n<!-- first -->\n\n<!-- second -->\n\n## Heading\n",
        "Para A\n\n\nPara B <!--\nx\n-->\n\n## Heading\n",
        "<!-- rumdl-disable MD022 -->\ntext\n\n\n<!-- rumdl-enable MD022 -->\n## Heading\n",
    ] {
        assert_spacing(input, input, "lines-above-mode = \"exact\"");
    }
    assert_spacing(
        "# Title\n\n\n<!-- KEEP ME -->\n## Heading\n",
        "# Title\n\n<!-- KEEP ME -->\n## Heading\n",
        "lines-above-mode = \"exact\"\nlines-below = 1",
    );
    assert_spacing(
        "text\n\n> quote\n\n\nHeading\n=======\n",
        "text\n\n> quote\n\nHeading\n=======\n",
        "lines-above-mode = \"exact\"\nlines-above = 0",
    );
}

#[test]
fn exact_spacing_respects_the_preceding_headings_minimum_below() {
    for above in ["0", "[1, 0, 1, 1, 1, 1]"] {
        let settings = format!("lines-above-mode = \"exact\"\nlines-above = {above}\nlines-below = 1");
        let input = "# Title\n\n## Section\n";
        assert_spacing(input, input, &settings);
    }
    assert_spacing(
        "# Title\n\n\n\n## Section\n",
        "# Title\n\n\n## Section\n",
        "lines-above-mode = \"exact\"\nlines-below = 2",
    );
    assert_spacing(
        "Title\n=====\n\n\n\n## Section\n",
        "Title\n=====\n\n\n## Section\n",
        "lines-above-mode = \"exact\"\nlines-below = 2",
    );
    assert_spacing(
        "# Title\n\n\n<!-- attached -->\n## Section\n",
        "# Title\n\n\n<!-- attached -->\n## Section\n",
        "lines-above-mode = \"exact\"\nlines-below = 2",
    );
}

#[test]
fn exact_spacing_and_below_minimum_converge_across_multiline_comments() {
    let input = "# Title\n\n\n<!--\nKEEP\n-->\n## Section\n";
    assert_spacing(input, input, "lines-above-mode = \"exact\"\nlines-below = 3");
}
