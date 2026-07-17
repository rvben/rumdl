//! Regression tests for issue #740: MD013 reflow deleted the space before
//! French double punctuation (`:` `;` `!` `?`) and downgraded or destroyed
//! non-breaking spaces (U+00A0, U+202F).
//!
//! These tests run the real `rumdl fmt` pipeline with the reporter's exact
//! configuration, so they exercise the production reflow path end to end.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// The reporter's exact input: one 143-char French line that wraps at 80.
const FRENCH_LINE: &str = "Voici une phrase en français assez longue pour forcer un retour à la ligne : la ponctuation double doit garder son espace avant le deux-points.\n";

/// Run `rumdl fmt` with the issue's config on `content`, return the result.
fn fmt_reflow_80(dir: &Path, name: &str, content: &str) -> String {
    let file_path = dir.join(name);
    fs::write(&file_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("-c")
        .arg("MD013.line-length = 80")
        .arg("-c")
        .arg("MD013.reflow = true")
        .arg("-c")
        .arg("MD013.code-blocks = false")
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

#[test]
fn test_french_orthographic_space_survives_fmt() {
    let temp = TempDir::new().unwrap();
    let fixed = fmt_reflow_80(temp.path(), "fr.md", FRENCH_LINE);

    assert!(
        fixed.contains("ligne :"),
        "the space before ':' must survive `rumdl fmt`. got:\n{fixed}"
    );
    assert!(
        !fixed.contains("ligne:"),
        "reflow must not glue ':' to the preceding word. got:\n{fixed}"
    );
    // The line must actually have been wrapped, so the reflow path really ran.
    assert!(
        fixed.lines().count() > 1,
        "the 143-char line should wrap at 80 cols. got:\n{fixed}"
    );
}

#[test]
fn test_non_breaking_spaces_survive_fmt_byte_for_byte() {
    let temp = TempDir::new().unwrap();
    // Two U+00A0: a thousands separator and a colon guard.
    let input = "Le prix est de 10\u{00A0}000 euros environ\u{00A0}: une somme assez longue pour forcer un retour a la ligne avec des espaces insecables partout ici.\n";
    let nbsp_count = |s: &str| s.matches('\u{00A0}').count();

    let fixed = fmt_reflow_80(temp.path(), "nbsp.md", input);

    assert_eq!(
        nbsp_count(&fixed),
        nbsp_count(input),
        "every U+00A0 must survive fmt byte-for-byte. got:\n{fixed}"
    );
    assert!(
        fixed.contains("10\u{00A0}000") && fixed.contains("environ\u{00A0}:"),
        "NBSPs must stay in place, not just in count. got:\n{fixed}"
    );
}

#[test]
fn test_reflow_fix_is_idempotent_on_french_text() {
    let temp = TempDir::new().unwrap();
    let once = fmt_reflow_80(temp.path(), "first.md", FRENCH_LINE);
    let twice = fmt_reflow_80(temp.path(), "second.md", &once);
    assert_eq!(once, twice, "formatting the fixed output again must be a no-op");
}
