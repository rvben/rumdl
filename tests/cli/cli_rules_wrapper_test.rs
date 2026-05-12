//! End-to-end CLI tests for the [rules.MDxxx] config wrapper form (issue #627).
//!
//! Verifies that `[rules.MD033]` in .rumdl.toml is treated identically to
//! the flat `[MD033]` form so users can port markdownlint configs without
//! their allow-lists being silently ignored.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// Exact reproduction of the user scenario from issue #627:
/// allowed-elements configured under [rules.MD033] must suppress MD033
/// for those elements.
#[test]
fn test_rules_wrapper_md033_allowed_elements_respected() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path();

    fs::write(
        base.join(".rumdl.toml"),
        r#"[rules]
  [rules.MD033]
    enabled = true
    allowed-elements = ["div", "img"]
"#,
    )
    .unwrap();

    fs::write(base.join("README.md"), "<div><img src=\"x.png\"></div>\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(base)
        .args(["check", "--no-cache", "README.md"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // MD033 must NOT fire for div or img — they are in the allow-list
    assert!(
        !stdout.contains("MD033"),
        "MD033 must not fire for allowed elements div/img, got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// [rules.MDxxx] in a flat .rumdl.toml (without the [rules] header trick)
/// also resolves correctly.
#[test]
fn test_rules_wrapper_flat_form_resolves() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path();

    fs::write(
        base.join(".rumdl.toml"),
        r#"[rules.MD013]
line-length = 200
"#,
    )
    .unwrap();

    // Long line that would trigger MD013 at default 80 chars but not at 200
    let long_line = "a".repeat(150);
    fs::write(base.join("test.md"), format!("# Title\n\n{long_line}\n")).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(base)
        .args(["check", "--no-cache", "--enable", "MD013", "test.md"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stdout.contains("MD013"),
        "MD013 must not fire for a 150-char line when line-length = 200 is set via [rules.MD013], got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// [tool.rumdl.rules.MDxxx] in pyproject.toml is parsed and applied.
#[test]
fn test_rules_wrapper_pyproject_md033_allowed_elements() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path();

    fs::write(
        base.join("pyproject.toml"),
        r#"[tool.rumdl.rules.MD033]
allowed-elements = ["div"]
"#,
    )
    .unwrap();

    fs::write(base.join("test.md"), "<div>hello</div>\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(base)
        .args(["check", "--no-cache", "test.md"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stdout.contains("MD033"),
        "MD033 must not fire for allowed element 'div' configured via [tool.rumdl.rules.MD033], got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
