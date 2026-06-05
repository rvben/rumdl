//! End-to-end execution tests for built-in code-block tools.
//!
//! These run the real external tool through the real `rumdl` binary and assert the
//! tool actually lints/formats a fenced code block as expected, rather than only
//! checking the registry command string. Each test is gated on the tool being
//! installed (`tool_available`), so it runs wherever the tool exists (locally, or any
//! CI that installs it) and skips otherwise. This is what would have caught the
//! shuck (no stdin), eslint (needs a config), and shellcheck (missing `--shell`)
//! problems before they shipped.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// True if `tool` is on PATH.
fn tool_available(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write a `.rumdl.toml` and a markdown file with a single fenced block, in a temp dir.
fn setup(config_lang: &str, slot: &str, tool: &str, lang_tag: &str, code: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config = format!(
        "[code-block-tools]\nenabled = true\non-error = \"warn\"\n\n\
         [code-block-tools.languages]\n{config_lang} = {{ {slot} = [\"{tool}\"] }}\n"
    );
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("t.md"), format!("# T\n\n```{lang_tag}\n{code}\n```\n")).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run rumdl");
    // Diagnostics can land on either stream depending on format; combine for assertions.
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Lint a code block with `tool` and return rumdl's combined output.
fn lint(config_lang: &str, tool: &str, lang_tag: &str, code: &str) -> String {
    let dir = setup(config_lang, "lint", tool, lang_tag, code);
    run(dir.path(), &["check", "--no-cache", "t.md"])
}

/// Format a code block with `tool` and return the resulting file contents.
fn format(config_lang: &str, tool: &str, lang_tag: &str, code: &str) -> String {
    let dir = setup(config_lang, "format", tool, lang_tag, code);
    run(dir.path(), &["fmt", "--no-cache", "t.md"]);
    fs::read_to_string(dir.path().join("t.md")).unwrap()
}

macro_rules! require_tool {
    ($tool:expr) => {
        if !tool_available($tool) {
            eprintln!("skipping: `{}` not installed", $tool);
            return;
        }
    };
}

// ---- linters --------------------------------------------------------------

#[test]
fn ruff_check_lints_python() {
    require_tool!("ruff");
    let out = lint("python", "ruff:check", "python", "import sys\nx = 1\n");
    assert!(out.contains("F401"), "ruff:check should flag the unused import:\n{out}");
}

#[test]
fn shellcheck_lints_shell() {
    require_tool!("shellcheck");
    // Regression guard for the `--shell=bash` fix: without it, a shebang-less snippet
    // yields a "target shell unknown" tip instead of real diagnostics. rumdl strips the
    // SCxxxx code from the message, so assert on the diagnostic text.
    let out = lint("shell", "shellcheck", "shell", "echo $foo\n");
    assert!(
        out.contains("Double quote to prevent globbing"),
        "shellcheck should flag the unquoted variable (SC2086):\n{out}"
    );
    assert!(
        !out.contains("target shell"),
        "shellcheck should not emit the shell-unknown tip with --shell=bash:\n{out}"
    );
}

#[test]
fn jq_lints_invalid_json() {
    require_tool!("jq");
    let out = lint("json", "jq", "json", "{\"a\": 1,}");
    assert!(
        out.contains("parse error"),
        "jq should report a JSON parse error:\n{out}"
    );
}

// ---- formatters -----------------------------------------------------------

#[test]
fn ruff_format_formats_python() {
    require_tool!("ruff");
    let out = format("python", "ruff:format", "python", "x=1");
    assert!(out.contains("x = 1"), "ruff:format should reformat the block:\n{out}");
}

#[test]
fn prettier_formats_javascript() {
    require_tool!("prettier");
    let out = format("javascript", "prettier", "javascript", "const x=1");
    assert!(
        out.contains("const x = 1;"),
        "prettier should reformat the block:\n{out}"
    );
}

#[test]
fn rustfmt_formats_rust() {
    require_tool!("rustfmt");
    let out = format("rust", "rustfmt", "rust", "fn  main(){let x=1;}");
    assert!(out.contains("fn main()"), "rustfmt should reformat the block:\n{out}");
}

#[test]
fn gofmt_formats_go() {
    require_tool!("gofmt");
    let out = format("go", "gofmt", "go", "package main\nfunc  main(){}");
    assert!(out.contains("func main()"), "gofmt should reformat the block:\n{out}");
}

#[test]
fn jq_formats_json() {
    require_tool!("jq");
    let out = format("json", "jq", "json", "{\"a\":1,\"b\":2}");
    assert!(
        out.contains("\"a\": 1") && out.contains('\n'),
        "jq should pretty-print the JSON block:\n{out}"
    );
}

#[test]
fn deno_fmt_formats_typescript() {
    require_tool!("deno");
    let out = format("typescript", "deno-fmt:ts", "typescript", "const   x=1");
    assert!(
        out.contains("const x = 1;"),
        "deno-fmt:ts should reformat the block:\n{out}"
    );
}
