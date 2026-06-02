//! Regression tests: `--fix` must not rewrite the contents of a ` ```markdown `
//! code block unless embedded markdown linting is explicitly opted into via
//! code-block-tools.
//!
//! Embedded markdown linting is documented as opt-in (the special `rumdl` tool in
//! `[code-block-tools.languages.markdown] lint = ["rumdl"]`). The check path
//! honored that gate, but the fix path ran the recursive embedded formatter
//! unconditionally, so `rumdl check --fix` silently rewrote code-block content
//! that `rumdl check` never reported. For MyST directive blocks this mangled the
//! directive (MD046 converting the fence to an indented block, MD040 injecting a
//! `text` language), corrupting content the user wrote verbatim inside a fence.
//!
//! These tests run the real binary end-to-end so they exercise the exact fix
//! coordinator + embedded gating used in production.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Run `rumdl check --fix` on a file containing `markdown`, using `config` as the
/// sole configuration source, and return the file contents after the fix. The
/// config is passed via `--config` and the working directory is the temp dir so
/// the project's own config never leaks in. `--no-cache` keeps the result
/// deterministic across repeated runs.
fn fix_with_config(markdown: &str, config: &str) -> String {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("doc.md");
    let config_path = dir.path().join(".rumdl.toml");
    fs::write(&file_path, markdown).unwrap();
    fs::write(&config_path, config).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg("--config")
        .arg(&config_path)
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    let exit_code = output.status.code().unwrap_or(-1);
    assert_ne!(
        exit_code,
        2,
        "rumdl errored (exit 2)\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    fs::read_to_string(&file_path).unwrap()
}

/// The exact reproduction from the issue: a ` ````markdown ` block containing a
/// MyST ` ```{eval-rst} ` directive, under MyST flavor with reflow enabled.
/// Without opting into embedded markdown linting, the block must be untouched.
#[test]
fn eval_rst_directive_in_markdown_block_unchanged_without_optin() {
    let markdown = "````markdown\n```{eval-rst}\n.. autofunction:: example.refraction.snell\n    :noindex:\n    :toctree: generated\n```\n````\n";

    let config = r#"
[global]
flavor = "myst"
disable = ["MD031", "MD057", "MD059"]

[MD013]
reflow = true
code-blocks = false
tables = false
"#;

    let fixed = fix_with_config(markdown, config);
    assert_eq!(
        fixed, markdown,
        "embedded MyST directive must be left byte-for-byte unchanged when embedded markdown linting is not enabled"
    );
}

/// A plain ` ```markdown ` block with an obvious fixable issue inside
/// (MD018: no space after `#`). Without opt-in the inner content must not be
/// touched, so users can show intentionally "broken" markdown examples in docs.
#[test]
fn markdown_block_content_unchanged_without_optin() {
    let markdown = "# Doc\n\n```markdown\n#Heading without space\n```\n";

    let config = "[global]\nflavor = \"standard\"\n";

    let fixed = fix_with_config(markdown, config);
    assert_eq!(
        fixed, markdown,
        "markdown code block content must not be rewritten when embedded markdown linting is not enabled"
    );
}

/// When embedded markdown linting IS opted into, the recursive formatter runs and
/// fixes the inner content. This proves the gate (not a blanket disable) is the
/// deciding factor, keeping the documented feature working.
#[test]
fn markdown_block_content_fixed_with_optin() {
    let markdown = "# Doc\n\n```markdown\n#Heading without space\n```\n";

    let config = r#"
[global]
flavor = "standard"

[code-block-tools]
enabled = true

[code-block-tools.languages.markdown]
lint = ["rumdl"]
"#;

    let fixed = fix_with_config(markdown, config);
    assert!(
        fixed.contains("# Heading without space"),
        "embedded markdown should be formatted when opted in via code-block-tools, got:\n{fixed}"
    );
    assert_ne!(fixed, markdown, "opt-in run should have changed the embedded content");
}

/// Opt-in must not corrupt a MyST directive. Uses the reporter's full config
/// (MyST flavor + reflow + the same disabled rules) so the embedded sub-lint runs
/// every rule that previously mangled the directive: MD046 converting the fence
/// to an indented block, and MD040 injecting a `text` language. The directive,
/// including its multi-line option body, must survive byte-for-byte.
#[test]
fn eval_rst_directive_preserved_under_myst_with_optin() {
    let markdown = "````markdown\n```{eval-rst}\n.. autofunction:: example.refraction.snell\n    :noindex:\n    :toctree: generated\n```\n````\n";

    let config = r#"
[global]
flavor = "myst"
disable = ["MD031", "MD057", "MD059"]

[MD013]
reflow = true
code-blocks = false
tables = false

[code-block-tools]
enabled = true

[code-block-tools.languages.markdown]
lint = ["rumdl"]
"#;

    let fixed = fix_with_config(markdown, config);
    assert_eq!(
        fixed, markdown,
        "MyST directive (fence + indented option body) must be preserved even with embedded linting enabled, got:\n{fixed}"
    );
}
