//! Regression test for issue #819: MD054's shortcut rewrite ran after MD013's
//! reflow, so wrap points were chosen on the `[text](url)` width and the
//! shortened lines were never revisited (the default reflow mode only reacts to
//! a line that is over the limit). Reflow has to run after every rewrite that
//! changes inline width, which the fix coordinator now guarantees for the whole
//! rule set rather than for a hand-picked list.

use std::fs;
use tempfile::TempDir;

/// The reporter's fixture: three links whose URLs push a single paragraph well
/// past 100 columns while the link texts alone fit in about two lines.
const SOURCE: &str = "Some text here followed by [a link to a short url](https://example.com/short/url) and then more text. Then [an even longer link, like this longer link](https://example.com/and/url/a/little/bit/longer) to a different url but now more text. Here's [a short one](https://example.com/but/long/long/long/long/long/long/) to compare.\n";

/// The reporter's expected output, which is also what MD054 followed by MD013 in
/// two separate invocations produces: a greedy 100-column fill of the shortcut
/// text (reflow does not break inside a link, hence three lines rather than two).
const EXPECTED: &str = "\
Some text here followed by [a link to a short url] and then more text. Then
[an even longer link, like this longer link] to a different url but now more text. Here's
[a short one] to compare.

[a link to a short url]: https://example.com/short/url
[an even longer link, like this longer link]: https://example.com/and/url/a/little/bit/longer
[a short one]: https://example.com/but/long/long/long/long/long/long/
";

/// Run the issue's exact `check --fix` command against a file and return it.
fn fix_with_reflow_and_shortcut_links(content: &str) -> String {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.md");
    fs::write(&file_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--fix", "--no-cache", "--no-config", "--disable", "MD041"])
        .args(["--config", "MD013.line-length=100"])
        .args(["--config", "MD013.reflow=true"])
        .args(["--config", "MD054.inline=false"])
        .args(["--config", "MD054.preferred-style='shortcut'"])
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl check --fix should run, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

#[test]
fn reflow_fills_to_the_limit_after_links_become_shortcut_references() {
    let fixed = fix_with_reflow_and_shortcut_links(SOURCE);
    assert_eq!(fixed, EXPECTED);
}

#[test]
fn a_second_run_changes_nothing() {
    // The first run must already be the fixed point: a run that only reaches the
    // right layout on its second invocation would still leave every user's first
    // `rumdl fmt` short.
    let fixed = fix_with_reflow_and_shortcut_links(EXPECTED);
    assert_eq!(fixed, EXPECTED);
}
