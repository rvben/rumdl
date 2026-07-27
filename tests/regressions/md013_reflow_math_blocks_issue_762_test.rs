//! Regression tests for issue #762: MD013 reflow joined the lines of a
//! multi-line `$$` display-math block into one line.
//!
//! Line breaks carry meaning inside such a block. A TeX `%` comment runs to the
//! end of its line, so joining the lines pulls whatever followed on later lines
//! into the comment: the reporter's block lost a matrix row and was left with an
//! unclosed `\end{aligned}`. The joined line also still exceeded the limit that
//! prompted the rewrite, so the "fix" traded correct content for no benefit.
//!
//! Every test drives the real `rumdl fmt` pipeline and asserts byte-exact
//! output, in both reflow modes, for each container a math block can sit in.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// The body of the reporter's equation. Two properties matter:
///
/// - The first row exceeds 40 columns, so the default reflow mode (which only
///   rewrites paragraphs that violate the limit) reaches this block too, not
///   just `normalize`, which rebuilds every paragraph unconditionally.
/// - A single space before the `%`, because runs of spaces are MD064's business
///   and would otherwise show up in these byte-exact assertions.
const MATH: &str =
    "\\begin{aligned}\nE &= mc^2 + \\frac{\\partial \\Phi}{\\partial t} \\\\ % note\nF &= ma\n\\end{aligned}";

/// A paragraph that must still be rewrapped at 40 columns.
const LONG_PROSE: &str =
    "This paragraph is definitely much longer than forty columns and therefore has to be rewrapped.";

/// Prefix every line of `text` with `prefix`, leaving blank lines blank.
fn prefix_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                prefix.trim_end().to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run `rumdl fmt` at a 40-column limit with reflow on and return the result.
///
/// `mode` is `None` for the default reflow mode, which only rewrites paragraphs
/// that violate the limit, and `Some("normalize")` for the mode that rebuilds
/// every paragraph unconditionally. Both must leave math blocks alone.
fn fmt_reflow_40(dir: &Path, name: &str, content: &str, mode: Option<&str>) -> String {
    let file_path = dir.join(name);
    fs::write(&file_path, content).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    cmd.arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("-c")
        .arg("MD013.line-length = 40")
        .arg("-c")
        .arg("MD013.reflow = true");
    if let Some(mode) = mode {
        cmd.arg("-c").arg(format!("MD013.reflow-mode = \"{mode}\""));
    }
    let output = cmd.arg(&file_path).output().expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// Assert `content` survives `rumdl fmt` byte-for-byte in both reflow modes.
fn assert_preserved(name: &str, content: &str) {
    for mode in [None, Some("normalize")] {
        let dir = TempDir::new().unwrap();
        let actual = fmt_reflow_40(dir.path(), "test.md", content, mode);
        assert_eq!(
            actual,
            content,
            "{name} was rewritten in reflow-mode {}\n--- input ---\n{content}\n--- output ---\n{actual}",
            mode.unwrap_or("default")
        );
    }
}

#[test]
fn math_block_at_top_level_is_preserved() {
    assert_preserved("toplevel", &format!("$$\n{MATH}\n$$\n"));
}

#[test]
fn math_block_in_blockquote_is_preserved() {
    let content = format!("{}\n", prefix_lines(&format!("$$\n{MATH}\n$$"), "> "));
    assert_preserved("blockquote", &content);
}

#[test]
fn math_block_in_nested_blockquote_is_preserved() {
    let content = format!("{}\n", prefix_lines(&format!("$$\n{MATH}\n$$"), ">> "));
    assert_preserved("nested blockquote", &content);
}

#[test]
fn math_block_in_list_item_is_preserved() {
    let body = prefix_lines(&format!("$$\n{MATH}\n$$"), "  ");
    assert_preserved("list item", &format!("- Item text\n\n{body}\n"));
}

#[test]
fn math_block_opening_on_the_list_marker_line_is_preserved() {
    let body = prefix_lines(MATH, "  ");
    assert_preserved("tight list item", &format!("- $$\n{body}\n  $$\n"));
}

#[test]
fn math_block_in_footnote_is_preserved() {
    let body = prefix_lines(&format!("$$\n{MATH}\n$$"), "    ");
    assert_preserved("footnote", &format!("[^a]: note\n\n{body}\n"));
}

#[test]
fn math_block_in_blockquote_inside_list_item_is_preserved() {
    let body = prefix_lines(&format!("$$\n{MATH}\n$$"), "  > ");
    assert_preserved("blockquote in list item", &format!("- Item text\n\n{body}\n"));
}

/// The reporter's exact failure: the `%` comment must not swallow the rows that
/// followed it, and `\end{aligned}` must not go missing.
#[test]
fn tex_comment_does_not_swallow_later_rows() {
    let dir = TempDir::new().unwrap();
    let content = format!("$$\n{MATH}\n$$\n");
    let actual = fmt_reflow_40(dir.path(), "test.md", &content, None);

    assert!(
        actual.contains("\nF &= ma\n"),
        "the row after the `%` comment was lost:\n{actual}"
    );
    assert!(
        actual.contains("\n\\end{aligned}\n"),
        "the environment was left unclosed:\n{actual}"
    );
    assert_eq!(
        actual.lines().count(),
        6,
        "the block was joined into fewer lines:\n{actual}"
    );
}

/// An over-long line inside a math block is still reported. rumdl refuses to
/// rewrite it, but it does not pretend the line fits, and the finding is not
/// offered as fixable, so `fmt` and `check` stay in agreement.
#[test]
fn over_long_math_line_is_still_reported_and_not_fixable() {
    let dir = TempDir::new().unwrap();
    let long_row = format!("E &= mc^2 + {} \\\\ % note", "x".repeat(60));
    let content = format!("# Title\n\n$$\n\\begin{{aligned}}\n{long_row}\nF &= ma\n\\end{{aligned}}\n$$\n");
    let file_path = dir.path().join("test.md");
    fs::write(&file_path, &content).unwrap();

    let check = |path: &Path| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-config")
            .arg("--no-cache")
            .arg("-c")
            .arg("MD013.line-length = 40")
            .arg("-c")
            .arg("MD013.reflow = true")
            .arg(path)
            .output()
            .expect("Failed to execute rumdl");
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    let before = check(&file_path);
    assert!(
        before.contains("[MD013]"),
        "the long math line should be reported:\n{before}"
    );
    assert!(
        !before.contains("[MD013] Line length exceeds"),
        "the reflow-style fixable warning must not be offered for a math block:\n{before}"
    );

    let after_fmt = fmt_reflow_40(dir.path(), "test.md", &content, None);
    assert_eq!(after_fmt, content, "fmt must leave the math block alone");
    assert!(
        check(&file_path).contains("[MD013]"),
        "the report must be stable across fmt, not silently dropped"
    );
}

/// Controls. Without these, a guard that simply switched reflow off would pass
/// every test above.
#[test]
fn prose_around_math_still_reflows() {
    let cases = [
        ("blockquote prose", prefix_lines(LONG_PROSE, "> ")),
        ("list item prose", format!("- {LONG_PROSE}")),
        ("single-line inline math", format!("Some prose $x+y$ {LONG_PROSE}")),
    ];

    for (name, body) in cases {
        let content = format!("{body}\n");
        let dir = TempDir::new().unwrap();
        let actual = fmt_reflow_40(dir.path(), "test.md", &content, None);
        assert_ne!(actual, content, "{name} should have been reflowed but was left alone");
        assert!(
            actual.lines().count() > 1,
            "{name} should have been wrapped onto several lines:\n{actual}"
        );
    }
}
