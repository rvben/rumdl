//! Config keys are normalized to lowercase kebab-case before serde sees them
//! (`normalize_key`), so a `#[serde(alias = "...")]` written in snake_case or
//! camelCase is unreachable unless the kebab spelling is declared too. Such an
//! alias silently drops the user's value while the key validator additionally
//! reports it as unknown. These tests pin the documented aliases as working, and
//! keep a control proving the probe can detect a dropped value.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// Run `rumdl check` over `content` with `config`, returning (findings for `rule`, stderr).
fn findings_for(rule: &str, config: &str, content: &str) -> (usize, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("test.md"), content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let marker = format!("[{rule}]");
    (
        stdout.lines().filter(|line| line.contains(&marker)).count(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

const HTML_IN_TABLE: &str = "# T\n\n<div>x</div>\n\n| a |\n|---|\n| <span>y</span> |\n";

#[test]
fn md033_table_allowed_alias_is_honored() {
    // docs/md033.md documents `table_allowed` as accepted. Allowing `span` inside
    // tables must drop one of the two findings.
    let baseline = findings_for("MD033", "[MD033]\n", HTML_IN_TABLE).0;
    let canonical = findings_for("MD033", "[MD033]\ntable_allowed_elements = [\"span\"]\n", HTML_IN_TABLE).0;
    assert!(
        canonical < baseline,
        "control failed: the canonical key should reduce findings ({canonical} vs {baseline})"
    );

    for spelling in ["table_allowed", "table-allowed"] {
        let config = format!("[MD033]\n{spelling} = [\"span\"]\n");
        let (count, stderr) = findings_for("MD033", &config, HTML_IN_TABLE);
        assert_eq!(
            count, canonical,
            "`{spelling}` must behave like the canonical key, got {count} findings"
        );
        assert!(
            !stderr.contains("Unknown option"),
            "`{spelling}` is documented and must not be reported as unknown, got:\n{stderr}"
        );
    }
}

const SHORTCUT_REF: &str = "# T\n\nSee [missing] here.\n";

#[test]
fn md052_shortcut_syntax_spellings_that_survive_normalization_work() {
    let baseline = findings_for("MD052", "[MD052]\n", SHORTCUT_REF).0;
    assert_eq!(baseline, 0, "control: shortcut syntax is off by default");

    for spelling in ["shortcut_syntax", "shortcut-syntax"] {
        let config = format!("[MD052]\n{spelling} = true\n");
        assert_eq!(
            findings_for("MD052", &config, SHORTCUT_REF).0,
            1,
            "`{spelling}` must enable shortcut-syntax checking"
        );
    }
}

#[test]
fn md052_camel_case_spelling_is_rejected_rather_than_silently_dropped() {
    // markdownlint itself uses `shortcut_syntax`, so the camelCase spelling has no
    // migration value. It cannot survive key normalization, so rather than
    // pretending to accept it, it must be reported as an unknown option.
    let (count, stderr) = findings_for("MD052", "[MD052]\nshortcutSyntax = true\n", SHORTCUT_REF);
    assert_eq!(count, 0, "camelCase spelling does not enable the check");
    assert!(
        stderr.contains("Unknown option"),
        "an unsupported spelling must be reported, not silently ignored, got:\n{stderr}"
    );
}

#[test]
fn md013_documented_alias_is_not_reported_as_an_unknown_option() {
    // `semantic-link-understanding` is documented in docs/md013.md and honored by
    // serde, but the key validator did not know it and called it unknown.
    let (_, stderr) = findings_for(
        "MD013",
        "[MD013]\nsemantic-link-understanding = true\n",
        "# T\n\nbody\n",
    );
    assert!(
        !stderr.contains("Unknown option"),
        "documented, working alias must not be reported as unknown, got:\n{stderr}"
    );
}

#[test]
fn a_genuinely_unknown_option_is_still_reported() {
    // Control: the validator must still catch real typos.
    let (_, stderr) = findings_for("MD013", "[MD013]\nignore-link-urlz = true\n", "# T\n\nbody\n");
    assert!(
        stderr.contains("Unknown option"),
        "a misspelled key must still be reported, got:\n{stderr}"
    );
}
