//! MyST targets (`(label)=`), block breaks (`+++`) and `%` comments are
//! block-level syntax of their own: rules must not read them as prose, and
//! fixes must not rewrite or join them.

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Runs rumdl in a fresh directory holding `doc.md` and `.rumdl.toml`.
/// Returns the output and the document as it is afterwards.
fn run(input: &str, config: &str, flavor: &str, args: &[&str]) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, input).unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(args)
        .args(["--no-cache", "--flavor", flavor, "doc.md"])
        .output()
        .unwrap();
    let after = fs::read_to_string(&path).unwrap();
    (output, after)
}

fn check(input: &str, config: &str, flavor: &str) -> String {
    let (output, _) = run(input, config, flavor, &["check"]);
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn fmt(input: &str, config: &str) -> String {
    let (output, after) = run(input, config, "myst", &["fmt"]);
    assert!(output.status.success(), "fmt failed: {output:?}");
    after
}

/// Heading levels and texts, in order, as a CommonMark parser reads them.
fn headings(source: &str) -> Vec<(HeadingLevel, String)> {
    let mut found = Vec::new();
    let mut open: Option<(HeadingLevel, String)> = None;
    for event in Parser::new(source) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => open = Some((level, String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, buffer)) = open.as_mut() {
                    buffer.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => found.extend(open.take()),
            _ => {}
        }
    }
    found
}

/// The non-blank lines of a document: a blank-line fix may add blank lines
/// and nothing else.
fn content_lines(source: &str) -> Vec<&str> {
    source.lines().filter(|line| !line.trim().is_empty()).collect()
}

const SPHINX_NAMES: &str = "[global]\nenable = [\"MD044\"]\n[MD044]\nnames = [\"Sphinx\"]\n";

#[test]
fn reporter_target_label_is_not_checked_as_a_proper_name() {
    let input = "(sphinx)=\n\n# Introduction\n\nSee {ref}`sphinx`.\n";
    let stdout = check(input, SPHINX_NAMES, "myst");
    assert!(!stdout.contains("MD044"), "target label flagged: {stdout}");
    assert_eq!(fmt(input, SPHINX_NAMES), input, "fmt rewrote the target label");
}

#[test]
fn target_syntax_inside_a_paragraph_is_still_checked() {
    let input = "# Introduction\n\nText (sphinx)= inline.\n";
    let stdout = check(input, SPHINX_NAMES, "myst");
    assert!(stdout.contains("doc.md:3:7") && stdout.contains("MD044"), "{stdout}");
    assert_eq!(fmt(input, SPHINX_NAMES), "# Introduction\n\nText (Sphinx)= inline.\n");
}

#[test]
fn target_line_in_standard_flavor_is_still_checked() {
    let input = "(sphinx)=\n\n# Introduction\n";
    let stdout = check(input, SPHINX_NAMES, "standard");
    assert!(stdout.contains("doc.md:1:2") && stdout.contains("MD044"), "{stdout}");
}

#[test]
fn names_in_a_myst_comment_are_not_rewritten() {
    let input = "# Title\n\n% sphinx notes\n\nBody.\n";
    assert_eq!(fmt(input, SPHINX_NAMES), input);
}

#[test]
fn block_break_metadata_is_not_checked_as_prose() {
    let input = "# Title\n\n+++ {\"tags\": [\"sphinx\"]}\n\nBody.\n";
    assert_eq!(fmt(input, SPHINX_NAMES), input);
}

#[test]
fn leading_target_comment_or_block_break_satisfies_md041() {
    let config = "[global]\nenable = [\"MD041\"]\n";
    for lead in [
        "(label)=\n",
        "% comment\n",
        "+++\n",
        "+++ {\"a\": 1}\n",
        "(a)=\n% c\n+++\n",
    ] {
        let input = format!("{lead}\n# Title\n\nBody.\n");
        let stdout = check(&input, config, "myst");
        assert!(!stdout.contains("MD041"), "{lead:?} counted as content: {stdout}");
    }
}

#[test]
fn md041_still_reports_real_content_before_the_heading() {
    let config = "[global]\nenable = [\"MD041\"]\n";
    for input in [
        "Text first\n\n# Title\n",
        "(label)=\nText first\n\n# Title\n",
        "(a)= tail\n\n# Title\n",
    ] {
        let stdout = check(input, config, "myst");
        assert!(stdout.contains("MD041"), "{input:?} escaped MD041: {stdout}");
    }
    // Standard flavor reads the target as text.
    assert!(check("(label)=\n\n# Title\n", config, "standard").contains("MD041"));
}

const MD022_ONLY: &str = "[global]\nenable = [\"MD022\"]\n";

#[test]
fn target_directly_above_heading_needs_no_blank_line() {
    let input = "Intro text.\n\n(sphinx)=\n# Introduction\n\nBody.\n";
    let stdout = check(input, MD022_ONLY, "myst");
    assert!(!stdout.contains("MD022"), "{stdout}");
    assert_eq!(fmt(input, MD022_ONLY), input);
    // A blank line between the target and the heading stays valid.
    assert!(!check("Intro.\n\n(sphinx)=\n\n# Introduction\n", MD022_ONLY, "myst").contains("MD022"));
}

#[test]
fn blank_line_is_required_above_the_target_run_and_inserted_there() {
    let input = "Intro text.\n(a)=\n(b)=\n## Section\n\nBody.\n";
    let stdout = check(input, MD022_ONLY, "myst");
    assert!(stdout.contains("MD022") && stdout.contains("above"), "{stdout}");
    let expected = "Intro text.\n\n(a)=\n(b)=\n## Section\n\nBody.\n";
    let once = fmt(input, MD022_ONLY);
    assert_eq!(once, expected);
    assert_eq!(fmt(&once, MD022_ONLY), once, "not idempotent");
    assert_eq!(headings(&once), headings(input));
    assert_eq!(content_lines(&once), content_lines(input));
}

#[test]
fn heading_below_heading_with_a_target_between_gets_one_blank_line_above_the_target() {
    let input = "# One\n(label)=\n## Two\n\nBody.\n";
    let expected = "# One\n\n(label)=\n## Two\n\nBody.\n";
    assert_eq!(fmt(input, MD022_ONLY), expected);
    assert_eq!(fmt(expected, MD022_ONLY), expected);
}

#[test]
fn setext_heading_keeps_its_target_attached() {
    let input = "Intro text.\n\n(sphinx)=\nIntroduction\n============\n\nBody.\n";
    assert!(!check(input, MD022_ONLY, "myst").contains("MD022"));
    let missing = "Intro text.\n(sphinx)=\nIntroduction\n============\n\nBody.\n";
    assert_eq!(
        fmt(missing, MD022_ONLY),
        "Intro text.\n\n(sphinx)=\nIntroduction\n============\n\nBody.\n"
    );
}

#[test]
fn exact_mode_counts_blank_lines_above_the_target_run() {
    let config = "[global]\nenable = [\"MD022\"]\n[MD022]\nlines-above-mode = \"exact\"\nlines-below = 0\n";
    let input = "Intro text.\n\n\n\n(sphinx)=\n# Introduction\n";
    assert_eq!(fmt(input, config), "Intro text.\n\n(sphinx)=\n# Introduction\n");
}

#[test]
fn target_stays_a_requirement_in_standard_flavor() {
    let stdout = check("Intro.\n\n(sphinx)=\n# Introduction\n", MD022_ONLY, "standard");
    assert!(stdout.contains("MD022"), "{stdout}");
}

#[test]
fn comment_directly_above_heading_still_needs_a_blank_line() {
    let stdout = check("Intro.\n\n% note\n# Introduction\n", MD022_ONLY, "myst");
    assert!(stdout.contains("MD022"), "{stdout}");
}

#[test]
fn target_inside_a_list_item_is_not_attached_to_a_heading_outside_the_list() {
    // The target is the item's content and the heading is not, so the heading
    // has no blank line above it even though the target run has one.
    let loose = "- Item.\n\n  (label)=\n# Heading\n\nText.\n";
    let stdout = check(loose, MD022_ONLY, "myst");
    assert!(stdout.contains("doc.md:4:1") && stdout.contains("above"), "{stdout}");
    let fixed = "- Item.\n\n  (label)=\n\n# Heading\n\nText.\n";
    assert_eq!(fmt(loose, MD022_ONLY), fixed);
    assert_eq!(fmt(fixed, MD022_ONLY), fixed);

    // The blank line goes above the heading: above the target it would be
    // inside the list and make a tight list loose.
    let tight = "- Item.\n  (label)=\n# Heading\n\nText.\n";
    assert_eq!(fmt(tight, MD022_ONLY), "- Item.\n  (label)=\n\n# Heading\n\nText.\n");
}

#[test]
fn target_stays_attached_when_it_shares_the_headings_container() {
    for input in [
        // Both in the list item.
        "- Item.\n\n  (label)=\n  # Heading\n\n  Text.\n",
        // Both outside: a target at the left edge ends the list.
        "- Item.\n\n(label)=\n# Heading\n\nText.\n",
        // Both outside: two columns do not reach the content of `1. `.
        "1. Item.\n\n  (label)=\n# Heading\n\nText.\n",
        // Both outside: no list holds the indented target.
        "Para.\n\n  (label)=\n# Heading\n\nText.\n",
    ] {
        let stdout = check(input, MD022_ONLY, "myst");
        assert!(!stdout.contains("MD022"), "{input:?}: {stdout}");
        assert_eq!(fmt(input, MD022_ONLY), input);
    }
    // A target at the left edge ends the list, so the blank line belongs above it.
    assert_eq!(
        fmt("- Item.\n(label)=\n# Heading\n\nText.\n", MD022_ONLY),
        "- Item.\n\n(label)=\n# Heading\n\nText.\n"
    );
}

#[test]
fn target_ends_a_setext_paragraph() {
    // MyST ends the paragraph at the target, so `===` underlines nothing and
    // the document opens with a paragraph (MD041). Standard flavor reads one
    // setext heading, the control showing the check can see the difference.
    let input = "Para\n(x)=\n===\n";
    let config = "[global]\nenable = [\"MD041\"]\n";
    assert!(check(input, config, "myst").contains("MD041"));
    assert!(!check(input, config, "standard").contains("MD041"));
}

fn reflow_config(mode: &str, width: u32) -> String {
    format!("[global]\nenable = [\"MD013\"]\n[MD013]\nreflow = true\nreflow-mode = \"{mode}\"\nline-length = {width}\n")
}

#[test]
fn reflow_never_joins_structural_lines_into_a_paragraph() {
    let long = "word word word word word word word word word word word word word word word word";
    for structural in ["(x)=", "% a comment", "+++", "+++ {\"a\": 1}"] {
        let input = format!("{long}\n{structural}\n{long} tail\n");
        let once = fmt(&input, &reflow_config("default", 80));
        assert!(
            once.lines().any(|line| line == structural),
            "{structural:?} was joined into prose:\n{once}"
        );
        assert_eq!(fmt(&once, &reflow_config("default", 80)), once, "not idempotent");
    }
}

#[test]
fn reflow_does_not_follow_a_code_span_across_a_target() {
    let input = "This is some prose before `word\n(label)=\nword` and some prose after it.\n";
    let once = fmt(input, &reflow_config("normalize", 30));
    assert!(once.lines().any(|line| line == "(label)="), "target joined:\n{once}");
}

#[test]
fn link_syntax_inside_a_target_label_is_not_rewritten() {
    let input = "# Title\n\n([ spaced ](https://example.org))=\n\nBody.\n";
    let config = "[global]\nenable = [\"MD039\"]\n";
    assert!(!check(input, config, "myst").contains("MD039"));
    assert_eq!(fmt(input, config), input);
}

#[test]
fn emphasis_like_target_labels_are_not_checked() {
    let input = "# Title\n\n(my_label_x)=\n(*)=\n\nBody.\n";
    let config = "[global]\nenable = [\"MD037\", \"MD049\", \"MD050\", \"MD034\"]\n";
    assert!(!check(input, config, "myst").contains("MD0"));
    assert!(
        !check(
            "(https://example.org/a)=\n\n# T\n",
            "[global]\nenable = [\"MD034\"]\n",
            "myst"
        )
        .contains("MD034")
    );
}

fn nested_config(extra: &str, nested: u32) -> String {
    format!("[global]\nenable = [\"MD022\"]\n[MD022]\nnested-heading-lines-above = {nested}\n{extra}")
}

#[test]
fn target_between_parent_and_child_heading_follows_the_nested_option() {
    // The target labels the child, so the child still directly follows its parent
    // and the nested value (0) replaces the default requirement of one blank line.
    let input = "# One\n(label)=\n## Two\n\nBody.\n";
    let config = nested_config("lines-below = 0\n", 0);
    let stdout = check(input, &config, "myst");
    assert!(!stdout.contains("MD022"), "{stdout}");
    assert_eq!(fmt(input, &config), input);

    // The same layout in standard flavor reads the label as a paragraph between the headings.
    assert!(check(input, &config, "standard").contains("MD022"));
}

#[test]
fn nested_option_blank_lines_go_above_the_target_run() {
    let config = nested_config("", 2);
    let input = "# One\n(a)=\n(b)=\n## Two\n\nBody.\n";
    let expected = "# One\n\n\n(a)=\n(b)=\n## Two\n\nBody.\n";
    let once = fmt(input, &config);
    assert_eq!(once, expected);
    assert_eq!(fmt(&once, &config), once, "not idempotent");
    assert_eq!(headings(&once), headings(input));
    assert!(!check(&once, &config, "myst").contains("MD022"));
}

#[test]
fn nested_option_exact_mode_counts_blank_lines_above_the_target_run() {
    let config = nested_config("lines-above-mode = \"exact\"\nlines-below = 1\n", 1);
    let input = "# One\n\n\n\n(label)=\n## Two\n\nBody.\n";
    assert_eq!(fmt(input, &config), "# One\n\n(label)=\n## Two\n\nBody.\n");
}

#[test]
fn reflow_keeps_targets_apart_with_sentence_per_line_and_double_space_modes() {
    let long = "First sentence of the paragraph is here. Second sentence follows it. Third one ends it now.";
    for mode in ["sentence-per-line", "normalize"] {
        let input = format!("{long}\n(label)=\n{long}\n");
        let once = fmt(&input, &reflow_config(mode, 40));
        assert!(
            once.lines().any(|line| line == "(label)="),
            "{mode}: target joined into prose:\n{once}"
        );
        assert_eq!(fmt(&once, &reflow_config(mode, 40)), once, "{mode}: not idempotent");
    }
}

#[test]
fn reflow_keeps_a_target_after_a_footnote_code_block_and_disabled_lines() {
    let input = "Text before.\n\n[^1]: Note.\n\n    ```\n    code\n    ```\n\n(label)=\nSome long paragraph text that should be wrapped at forty characters wide.\n";
    let once = fmt(input, &reflow_config("normalize", 40));
    assert!(once.contains("    code\n"), "footnote code block changed:\n{once}");
    assert!(once.lines().any(|line| line == "(label)="), "{once}");

    let disabled = "Text.\n\n<!-- rumdl-disable MD013 -->\nkeep this very long line exactly as it is written here\n<!-- rumdl-enable MD013 -->\n(label)=\nSome long paragraph text that should be wrapped at forty characters wide.\n";
    let once = fmt(disabled, &reflow_config("normalize", 40));
    assert!(
        once.contains("keep this very long line exactly as it is written here\n"),
        "{once}"
    );
    assert!(once.lines().any(|line| line == "(label)="), "{once}");
}
