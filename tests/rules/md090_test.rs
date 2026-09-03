//! MD090 through the real lint pipeline and the CLI.
//!
//! The rule-level tests prove the verdict; these prove what a user sees.
//! `rumdl fmt` applies fixes to a fixpoint across every enabled rule, so the
//! MUST_FIX rows assert exact output bytes with MD022 and MD065 active
//! alongside MD090 (their fixes touch the same lines), and the MUST_KEEP rows
//! assert `fmt` is a byte no-op. A rule that silently stopped firing would
//! fail the MUST_FIX rows instead of passing everything. The reporter's own
//! document runs once more under the default rule set plus MD090, which is
//! how a user enables it.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::{MD022BlanksAroundHeadings, MD065BlanksAroundHorizontalRules, MD090NoHrBeforeHeading};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn lint(content: &str, rules: &[Box<dyn Rule>]) -> Vec<LintWarning> {
    rumdl_lib::lint(content, rules, false, MarkdownFlavor::Standard, None, None).unwrap()
}

fn rumdl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("rumdl runs")
}

/// Writes `input` to a temp file, runs `rumdl fmt` on it with `rule_args`
/// appended, and returns the file's bytes afterwards.
fn fmt_with(input: &str, rule_args: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(&file, input).unwrap();
    let mut args = vec!["fmt", "--color", "never", "--no-cache", "--no-config"];
    args.extend_from_slice(rule_args);
    args.push("doc.md");
    let output = rumdl(dir.path(), &args);
    assert!(
        output.status.code().is_some(),
        "fmt did not exit normally: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(&file).unwrap()
}

/// The matrix runs with exactly MD022, MD065 and MD090 enabled. Under the
/// default rule set MD003 rewrites a setext heading to ATX, MD025 demotes a
/// `# Title` below a front-matter `title:`, and MD023/MD035 dedent a heading
/// or break inside a list item, so a default-rules run would change the
/// MUST_KEEP rows for reasons that have nothing to do with this rule.
fn fmt(input: &str) -> String {
    fmt_with(input, &["--enable", "MD022,MD065,MD090"])
}

#[test]
fn pipeline_reports_the_break_with_a_fix() {
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD090NoHrBeforeHeading::new())];
    let warnings = lint("Prose.\n\n---\n\n## Next\n", &rules);
    assert_eq!(warnings.len(), 1, "got: {warnings:?}");
    assert_eq!(warnings[0].rule_name.as_deref(), Some("MD090"));
    assert_eq!(warnings[0].line, 3);
    assert!(warnings[0].fix.is_some());
}

#[test]
fn pipeline_with_md022_and_md065_reports_each_rule_once_on_tight_spacing() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD065BlanksAroundHorizontalRules),
        Box::new(MD090NoHrBeforeHeading::new()),
    ];
    let warnings = lint("Prose\n\n---\n## Next\n", &rules);
    let mut names: Vec<&str> = warnings.iter().filter_map(|w| w.rule_name.as_deref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["MD022", "MD065", "MD090"], "got: {warnings:?}");
}

#[test]
fn fmt_must_fix_rows_produce_exact_bytes() {
    let rows: &[(&str, &str, &str)] = &[
        (
            "tight spacing with MD022 and MD065 active",
            "# T\n\nProse\n\n---\n## Next\n",
            "# T\n\nProse\n\n## Next\n",
        ),
        ("break on first line", "---\n\n# Title\n", "# Title\n"),
        (
            "break after front matter",
            "---\ntitle: x\n---\n\n---\n\n# Title\n",
            "---\ntitle: x\n---\n\n# Title\n",
        ),
        (
            "run of breaks",
            "# T\n\nProse\n\n---\n\n---\n\n## H\n",
            "# T\n\nProse\n\n## H\n",
        ),
        (
            "break before setext heading",
            "# T\n\nProse\n\n---\n\nNext topic\n----------\n",
            "# T\n\nProse\n\nNext topic\n----------\n",
        ),
        (
            "tight break above setext heading keeps separation",
            "# T\n\nProse\n***\nNext\n====\n",
            "# T\n\nProse\n\nNext\n====\n",
        ),
        (
            "break directly after a table",
            "# T\n\n| a |\n| - |\n| x |\n---\n\n## H\n",
            "# T\n\n| a |\n| - |\n| x |\n\n## H\n",
        ),
        (
            "star run under paragraph text is a break, not an underline",
            "# T\n\n*Label*\n***\n\n## Next\n",
            "# T\n\n*Label*\n\n## Next\n",
        ),
        (
            "tight break above atx heading keeps separation",
            "# T\n\nProse\n***\n## Next\n",
            "# T\n\nProse\n\n## Next\n",
        ),
        (
            "tight run of breaks leaves a single blank line",
            "# T\n\nProse\n***\n***\n## H\n",
            "# T\n\nProse\n\n## H\n",
        ),
        (
            "space and tab line between break and heading is blank",
            "# T\n\nProse\n\n---\n \t\n## H\n",
            "# T\n\nProse\n\n## H\n",
        ),
        (
            "crlf document keeps crlf",
            "# T\r\n\r\nProse\r\n\r\n---\r\n\r\n## H\r\n",
            "# T\r\n\r\nProse\r\n\r\n## H\r\n",
        ),
        (
            "crlf run of breaks keeps crlf",
            "# T\r\n\r\nProse\r\n\r\n---\r\n\r\n---\r\n\r\n## H\r\n",
            "# T\r\n\r\nProse\r\n\r\n## H\r\n",
        ),
    ];
    for (name, input, expected) in rows {
        assert_eq!(&fmt(input), expected, "row {name:?}");
    }

    // Every thematic-break spelling CommonMark allows, including the three
    // leading spaces it tolerates and a marker longer than three characters.
    for marker in ["***", "___", "- - -", "* * *", "   ---", "-----"] {
        let input = format!("# T\n\nProse\n\n{marker}\n\n## H\n");
        assert_eq!(fmt(&input), "# T\n\nProse\n\n## H\n", "marker {marker:?}");
    }

    // `markdown="1"` content is Markdown under both flavors; the blank line
    // after the opening tag ends the HTML block.
    let div = "<div markdown=\"1\">\n\n---\n\n## H\n\n</div>\n";
    let div_fixed = "<div markdown=\"1\">\n\n## H\n\n</div>\n";
    assert_eq!(&fmt(div), div_fixed, "markdown html block, standard flavor");
    assert_eq!(
        &fmt_with(div, &["--flavor", "mkdocs", "--enable", "MD022,MD065,MD090"]),
        div_fixed,
        "markdown html block, mkdocs flavor"
    );

    // The detector phantom-records `::: note` as setext text underlined by
    // the dash run; the fix must still remove the break inside the div and
    // keep both fences.
    let div_break = "::: note\n---\n\n# H\n:::\n";
    assert_eq!(
        &fmt_with(div_break, &["--flavor", "quarto", "--enable", "MD022,MD065,MD090"]),
        "::: note\n\n# H\n:::\n",
        "break directly after div opener, quarto flavor"
    );

    // A break above the opener is a real break: a fence marks no section.
    // The same run still removes the break inside the div, so a fix that
    // simply gave up on documents holding a div would fail this row.
    assert_eq!(
        &fmt_with(
            "***\n::: note\n---\n\n# H\n:::\n",
            &["--flavor", "quarto", "--enable", "MD090"]
        ),
        "***\n::: note\n\n# H\n:::\n",
        "break above a div opener is kept, break inside it is removed"
    );

    // Nesting and PyMdown blocks reach the same guard: the marker line is in a
    // container, so the break above it survives while the ATX heading below
    // still loses its own break.
    assert_eq!(
        &fmt_with(
            ":::: outer\n\n***\n::: inner\n---\n\n# H\n:::\n::::\n",
            &["--flavor", "quarto", "--enable", "MD090"]
        ),
        ":::: outer\n\n***\n::: inner\n\n# H\n:::\n::::\n",
        "break above a nested div opener is kept"
    );
    assert_eq!(
        &fmt_with(
            "***\n/// note\n---\n\n# H\n///\n",
            &["--flavor", "mkdocs", "--enable", "MD090"]
        ),
        "***\n/// note\n\n# H\n///\n",
        "break above a pymdown block opener is kept"
    );
}

#[test]
fn fmt_must_keep_rows_are_byte_no_ops() {
    let rows: &[(&str, &str)] = &[
        ("setext underline", "# T\n\nProse\n---\n\n## Next\n"),
        // The heading detector records no setext heading when the text line
        // opens with `*` or `<`, so these two rows guard the rule's own
        // underline reading rather than the detector's.
        ("emphasis setext underline", "# T\n\n*Label*\n---\n\n## Next\n"),
        (
            "inline html setext underline",
            "# T\n\n<span>Label</span>\n---\n\n## Next\n",
        ),
        // `#hashtag` has no heading space, so it is paragraph text and the
        // dash run under it is its setext underline.
        ("hashtag setext underline", "# T\n\n#hashtag\n---\n\n## Next\n"),
        // Standard flavor gives `:::` no meaning, so `::: note` is paragraph
        // text and the dash run under it is its setext underline.
        ("colon paragraph setext underline", "# T\n\n::: note\n---\n\n## Next\n"),
        // Without a delimiter row the pipes are ordinary text, so the dash
        // run under the line is its setext underline, not a break. The `*`
        // keeps the detector from recording a setext heading, so the rule's
        // own underline reading is what answers.
        ("pipe line that is not a table", "# T\n\n*Label | x*\n---\n\n## Next\n"),
        ("front matter only", "---\ntitle: x\n---\n\n# Title\n"),
        ("comment between", "# T\n\nProse\n\n---\n\n<!-- c -->\n\n## H\n"),
        ("break after heading", "# T\n\n## H\n\n---\n\nProse\n"),
        ("blockquote", "# T\n\n> ---\n>\n> ## H\n"),
        ("empty blockquote line between", "# T\n\nProse\n\n---\n\n>\n\n## H\n"),
        ("list item", "# T\n\n- item\n\n  ---\n\n  ## H\n"),
        ("fenced", "# T\n\n```text\n---\n```\n\n## H\n"),
        ("fence indented one space", "# T\n\n ```\n---\n```\n\n## H\n"),
        ("indented code", "# T\n\nProse\n\n    ---\n\n## H\n"),
        ("attribute line between", "# T\n\nProse\n\n---\n\n{#custom}\n## H\n"),
        ("html comment", "# T\n\n<!--\n---\n-->\n\n## H\n"),
        ("hashtag is not a heading", "# T\n\nProse\n\n---\n\n#hashtag\n"),
        ("break before prose", "# T\n\nProse\n\n---\n\nMore prose\n"),
    ];
    for (name, input) in rows {
        assert_eq!(&fmt(input), input, "row {name:?}");
    }

    // Display math runs without MD022: the parser reads `$$` followed by `---`
    // as a setext heading inside the block and MD022 inserts a blank line
    // below it, which is that rule's concern, not this one's. MD065 stays on
    // because a break inside math is exactly the shape it must also ignore.
    let math = "# T\n\n$$\n---\n$$\n\n## H\n";
    assert_eq!(
        &fmt_with(math, &["--enable", "MD065,MD090"]),
        math,
        "row \"math block\""
    );

    // A no-break space renders as content, so the line holding it keeps the
    // break from sitting directly above the heading. MD065 stays off because
    // it legitimately inserts its own blank line next to that content line.
    let nbsp = "# T\n\nProse\n\n***\n\u{00A0}\n## H\n";
    assert_eq!(
        &fmt_with(nbsp, &["--enable", "MD090"]),
        nbsp,
        "row \"nbsp line between\""
    );
}

#[test]
fn fmt_default_rules_plus_md090_fix_the_reporter_document() {
    // The way a user turns the rule on: default rules, MD090 added.
    let input = "# Title\n\n## Topic\n\nProse.\n\n---\n\n## Next Topic\n\nMore.\n";
    assert_eq!(
        fmt_with(input, &[]),
        input,
        "default rules alone must leave this document alone"
    );
    assert_eq!(
        fmt_with(input, &["--extend-enable", "MD090"]),
        "# Title\n\n## Topic\n\nProse.\n\n## Next Topic\n\nMore.\n"
    );
}

#[test]
fn fmt_positive_control_rewrites_with_the_rule_and_not_without_it() {
    // Without MD090 in the set the same input must survive fmt intact, which
    // proves the MUST_FIX rows are the rule's doing.
    let input = "# T\n\nProse.\n\n---\n\n## Next\n";
    assert_eq!(fmt_with(input, &["--enable", "MD022,MD065"]), input);
    assert_eq!(fmt(input), "# T\n\nProse.\n\n## Next\n");
}
