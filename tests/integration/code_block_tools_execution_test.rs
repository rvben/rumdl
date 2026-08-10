//! End-to-end execution tests for built-in code-block tools.
//!
//! These run the real external tool through the real `rumdl` binary and assert the
//! tool actually lints/formats a fenced code block as expected, rather than only
//! checking the registry command string. Each test is gated on the tool being
//! installed (`tool_available`), so it runs wherever the tool exists (locally, or any
//! CI that installs it) and skips otherwise. This is what would have caught the
//! shuck (no stdin), eslint (needs a config), and shellcheck (missing `--shell`)
//! problems before they shipped.
//!
//! ## Adding a built-in tool
//!
//! 1. Install the tool and run it through rumdl on a fenced block (a temp `.rumdl.toml`
//!    with `[code-block-tools]` plus `rumdl check`/`fmt`). Confirm it reads stdin and
//!    its output parses into real diagnostics / formatted code. If it can't be made to
//!    work over stdin, do not ship it (see the removed eslint/shuck/rubocop entries).
//! 2. Add an execution test below and list its registry id in `VERIFIED`. For a pure
//!    extension/subcommand variant of an already-tested tool (e.g. `prettier:json`),
//!    add it to `EXEMPT` with the reason instead.
//!
//! `every_builtin_tool_is_verified_or_exempt` is a CI gate: a new registry entry that
//! is neither tested nor exempted fails the suite, so unverified tools cannot ship.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// True if `tool` is on PATH and can be executed (not broken).
fn tool_available(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    let exists = Command::new(finder)
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !exists {
        return false;
    }

    // Tool-specific verification to handle wrappers or broken installations
    match tool {
        "terraform" => {
            // terraform version returns success and contains "Terraform"
            Command::new("terraform")
                .arg("version")
                .output()
                .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("Terraform"))
                .unwrap_or(false)
        }
        "black" => {
            // black --version returns success
            Command::new("black")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        "shuck" => {
            // `shuck` is a contested binary name: the shell linter ships as the
            // `shuck-cli` package, while an unrelated microVM manager also
            // installs a `shuck`. Only the linter has a `check` subcommand.
            Command::new("shuck")
                .args(["check", "--help"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        _ => {
            // Default spawn check (safe, won't block even if tool expects stdin,
            // because we pass --version and kill it immediately if it spawns)
            match Command::new(tool).arg("--version").spawn() {
                Ok(mut child) => {
                    let _ = child.kill();
                    true
                }
                Err(_) => false,
            }
        }
    }
}

/// Write a `.rumdl.toml` and a markdown file with a single fenced block, in a temp dir.
fn setup(config_lang: &str, slot: &str, tool: &str, lang_tag: &str, code: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    // `exact` language resolution so config_lang == lang_tag deterministically
    // (avoids linguist-alias surprises like cpp -> c++).
    let config = format!(
        "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\non-error = \"warn\"\n\n\
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

/// The message a formatter produces in a `lint` slot.
const NOT_FORMATTED: &str = "Code block is not formatted";

/// The file line the first line of the fenced block occupies in `setup`'s document.
const FIRST_CODE_LINE: usize = 4;

/// The start of a diagnostic as rumdl prints it, for a position inside the fenced block.
///
/// `line_offset` is 0-based within the block, so a tool that reports its finding relative
/// to its own stdin lands here only if rumdl added the fence's offset. The expected
/// positions below were read off the tools themselves; a diagnostic stuck on the fence
/// (`line_offset` 0 regardless of the finding) is exactly the defect these guard against.
///
/// The needle stops at the opening bracket because rumdl pads short tool ids in the
/// plain-text output (`[jq   ]`), so a closing `]` would never match.
fn at(line_offset: usize, column: usize, tool: &str) -> String {
    format!("t.md:{}:{column}: [{tool}", FIRST_CODE_LINE + line_offset)
}

/// A built-in linter reports nothing on a block it accepts.
///
/// Every positive assertion below is paired with one of these. Without it, a tool invoked
/// so badly that it complains about its own invocation, or a parser that turns a summary
/// line into a diagnostic, satisfies a `contains` check without having linted anything.
fn assert_lint_is_silent(config_lang: &str, tool: &str, lang_tag: &str, clean: &str) {
    let out = lint(config_lang, tool, lang_tag, clean);
    assert!(
        !out.contains(&format!("[{tool}")),
        "{tool} should report nothing on a block it accepts:\n{out}"
    );
}

/// Contents of the first fenced block in `md`, without the trailing newline.
fn fenced_block(md: &str) -> String {
    let mut lines = md.lines().skip_while(|l| !l.trim_start().starts_with("```"));
    lines.next().expect("document has a fenced block");
    lines.take_while(|l| l.trim() != "```").collect::<Vec<_>>().join("\n")
}

/// A built-in formatter in a `lint` slot reports exactly the blocks `fmt` rewrites.
///
/// Positive control: the unformatted block is reported. Negative control: the block as
/// `rumdl fmt` writes it is accepted. The clean sample is produced by the tool itself, so
/// this cannot pass against a hand-written guess at the tool's style, and it fails if a
/// formatter is not idempotent (which would make `check` flag what `fmt` just wrote).
fn assert_lint_matches_fmt(config_lang: &str, tool: &str, lang_tag: &str, unformatted: &str) {
    let out = lint(config_lang, tool, lang_tag, unformatted);
    assert!(
        out.contains(NOT_FORMATTED),
        "{tool} should report the unformatted block:\n{out}"
    );

    let formatted = fenced_block(&format(config_lang, tool, lang_tag, unformatted));
    assert_ne!(
        formatted.trim_end(),
        unformatted.trim_end(),
        "{tool} left the sample unchanged, so the check above proves nothing"
    );

    let out = lint(config_lang, tool, lang_tag, formatted.trim_end());
    assert!(
        !out.contains(NOT_FORMATTED),
        "{tool} should accept a block it formatted itself:\n{out}"
    );
}

/// Declare the `lint`-slot test for a built-in formatter.
macro_rules! lint_by_format_test {
    ($name:ident, $binary:expr, $tool:expr, $lang:expr, $tag:expr, $unformatted:expr) => {
        #[test]
        fn $name() {
            require_tool!($binary);
            assert_lint_matches_fmt($lang, $tool, $tag, $unformatted);
        }
    };
}

// ---- linters --------------------------------------------------------------

#[test]
fn ruff_check_lints_python() {
    require_tool!("ruff");
    let out = lint("python", "ruff:check", "python", "import sys\nx = 1\n");
    assert!(out.contains("F401"), "ruff:check should flag the unused import:\n{out}");
    assert!(
        out.contains(&at(0, 8, "ruff:check")),
        "ruff:check should report the import at its own column:\n{out}"
    );
    assert_lint_is_silent("python", "ruff:check", "python", "x = 1\n");
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
    assert_lint_is_silent("shell", "shellcheck", "shell", "foo=bar\necho \"$foo\"\n");
}

#[test]
fn shuck_lints_shell() {
    require_tool!("shuck");
    // Regression guard for the stdin fix upstream (ewhauser/shuck#1123, shipped in
    // v0.0.43): a pre-0.0.43 shuck treats `-` as a literal filename instead of
    // reading stdin and would report a missing-file error here instead of a
    // real diagnostic.
    let out = lint("shell", "shuck", "shell", "name=\"world\"\necho \"hello $nombre\"\n");
    assert!(
        out.contains("referenced before assignment") || out.contains("C006"),
        "shuck should flag the reference to the undefined variable:\n{out}"
    );
    assert!(
        out.contains(&at(1, 13, "shuck")),
        "shuck should report the reference on the block's second line:\n{out}"
    );
    assert_lint_is_silent("shell", "shuck", "shell", "name=\"world\"\necho \"hello $name\"\n");
}

#[test]
fn jq_lints_invalid_json() {
    require_tool!("jq");
    let out = lint("json", "jq", "json", "{\"a\": 1,}");
    assert!(
        out.contains("parse error"),
        "jq should report a JSON parse error:\n{out}"
    );
    // jq states the position in prose ("at line 1, column 9") instead of prefixing the
    // message with one, which used to anchor the diagnostic on the fence.
    assert!(
        out.contains(&at(0, 9, "jq")),
        "jq should report the parse error at the position its message names:\n{out}"
    );
    assert_lint_is_silent("json", "jq", "json", "{\"a\": 1}");
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

#[test]
fn black_formats_python() {
    require_tool!("black");
    let out = format("python", "black", "python", "x=1");
    assert!(out.contains("x = 1"), "black should reformat the block:\n{out}");
}

#[test]
fn shfmt_formats_shell() {
    require_tool!("shfmt");
    let out = format("shell", "shfmt", "shell", "if true;then echo hi;fi");
    assert!(out.contains("; then"), "shfmt should reformat the block:\n{out}");
}

#[test]
fn shuck_formats_shell() {
    require_tool!("shuck");
    // `shuck:format` runs `shuck format -`, which reads the block from stdin and writes
    // the formatted source to stdout (verified against shuck 0.0.45, where `format` is
    // ungated). A build that treats `-` as a filename leaves the block unchanged and the
    // assertion below fails.
    let out = format("shell", "shuck:format", "shell", "if [ \"$x\" = 1 ];then echo hi;fi");
    assert!(out.contains("; then"), "shuck:format should reformat the block:\n{out}");
}

#[test]
fn goimports_formats_go() {
    require_tool!("goimports");
    let out = format("go", "goimports", "go", "package main\nfunc  main(){}");
    assert!(
        out.contains("func main()"),
        "goimports should reformat the block:\n{out}"
    );
}

#[test]
fn clang_format_formats_cpp() {
    require_tool!("clang-format");
    let out = format("cpp", "clang-format", "cpp", "int  main(){return 0;}");
    assert!(
        out.contains("int main()"),
        "clang-format should reformat the block:\n{out}"
    );
}

#[test]
fn yamlfmt_formats_yaml() {
    require_tool!("yamlfmt");
    let out = format("yaml", "yamlfmt", "yaml", "a:   1");
    assert!(out.contains("a: 1"), "yamlfmt should reformat the block:\n{out}");
}

#[test]
fn taplo_formats_toml() {
    require_tool!("taplo");
    let out = format("toml", "taplo", "toml", "a=1");
    assert!(out.contains("a = 1"), "taplo should reformat the block:\n{out}");
}

#[test]
fn terraform_formats_terraform() {
    require_tool!("terraform");
    // The bare binary name is the id a user guesses; it resolves through `terraform:format`.
    let out = format("terraform", "terraform", "terraform", "a=1");
    assert!(out.contains("a = 1"), "terraform fmt should reformat the block:\n{out}");
}

#[test]
fn terraform_fmt_alias_still_formats() {
    require_tool!("terraform");
    // `terraform-fmt` was the only id this tool had before `terraform:format` existed.
    // Configs written against it must keep working.
    let out = format("terraform", "terraform-fmt", "terraform", "a=1");
    assert!(
        out.contains("a = 1"),
        "the terraform-fmt alias should still reformat the block:\n{out}"
    );
}

#[test]
fn stylua_formats_lua() {
    require_tool!("stylua");
    let out = format("lua", "stylua", "lua", "x=1");
    assert!(out.contains("x = 1"), "stylua should reformat the block:\n{out}");
}

#[test]
fn oxfmt_formats_javascript() {
    require_tool!("oxfmt");
    let out = format("javascript", "oxfmt", "javascript", "const x=1");
    assert!(out.contains("const x = 1;"), "oxfmt should reformat the block:\n{out}");
}

#[test]
fn tombi_formats_toml() {
    require_tool!("tombi");
    let out = format("toml", "tombi:format", "toml", "a=1");
    assert!(out.contains("a = 1"), "tombi:format should reformat the block:\n{out}");
}

#[test]
fn beautysh_formats_shell() {
    require_tool!("beautysh");
    let out = format("shell", "beautysh", "shell", "if true\nthen\necho hi\nfi");
    assert!(out.contains("    echo hi"), "beautysh should indent the block:\n{out}");
}

#[test]
fn nixfmt_formats_nix() {
    require_tool!("nixfmt");
    let out = format("nix", "nixfmt", "nix", "{ a=1; }");
    assert!(out.contains("a = 1"), "nixfmt should reformat the block:\n{out}");
}

#[test]
fn ormolu_formats_haskell() {
    require_tool!("ormolu");
    let out = format("haskell", "ormolu", "haskell", "main=putStrLn \"hi\"");
    assert!(
        out.contains("main = putStrLn"),
        "ormolu should reformat the block:\n{out}"
    );
}

#[test]
fn swift_format_formats_swift() {
    require_tool!("swift-format");
    let out = format("swift", "swift-format", "swift", "let x  =  1");
    assert!(
        out.contains("let x = 1"),
        "swift-format should reformat the block:\n{out}"
    );
}

#[test]
fn ktfmt_formats_kotlin() {
    require_tool!("ktfmt");
    let out = format("kotlin", "ktfmt", "kotlin", "fun main(){}");
    assert!(out.contains("fun main() {}"), "ktfmt should reformat the block:\n{out}");
}

#[test]
fn elm_format_formats_elm() {
    require_tool!("elm-format");
    let out = format("elm", "elm-format", "elm", "module Main exposing (main)\nmain= 1");
    // elm-format moves the body onto its own indented line.
    assert!(
        out.contains("main =\n    1"),
        "elm-format should reformat the block:\n{out}"
    );
}

#[test]
fn sqlfluff_lints_sql_with_dialect() {
    require_tool!("sqlfluff");
    // Two fixes in one guard. `--dialect ansi`: without it sqlfluff errors ("No dialect
    // was specified") instead of linting. `--format github-annotation-native`: its human
    // format spreads a finding over two lines and names no file, so nothing parsed and
    // every finding landed on the fence. `select 1` is clean under both, which is why the
    // sample here has to be one sqlfluff actually complains about.
    let out = lint("sql", "sqlfluff:lint", "sql", "SELECT   1  FROM   t");
    assert!(
        !out.contains("No dialect") && !out.contains("User Error"),
        "sqlfluff should lint with a dialect, not error:\n{out}"
    );
    // One LT01 per run of extra spaces, each at its own column: the three distinct columns
    // are what prove rumdl reads the annotation's `col=` rather than defaulting.
    for column in [7, 11, 17] {
        assert!(
            out.contains(&at(0, column, "sqlfluff:lint")),
            "sqlfluff should report LT01 at column {column} of the block:\n{out}"
        );
    }
    assert_lint_is_silent("sql", "sqlfluff:lint", "sql", "SELECT 1 FROM t");
}

#[test]
fn djlint_lints_html() {
    require_tool!("djlint");
    // Regression guard for the `--linter-output-format` fix: djlint's default report puts
    // the position inside the message ("H025 2:2 Tag seems to be an orphan."), which parses
    // into nothing, so every finding landed on the fence. The orphan is on the block's
    // second line, so the fence and the right answer are different lines here.
    let out = lint("html", "djlint", "html", "<div>\n<p>hi</div>");
    assert!(out.contains("orphan"), "djlint should flag the orphan tag:\n{out}");
    assert!(
        out.contains(&at(1, 0, "djlint")),
        "djlint should report the orphan on the block's second line:\n{out}"
    );
    assert_lint_is_silent("html", "djlint", "html", "<div>\n    <p>hi</p>\n</div>");
}

#[test]
fn djlint_reformats_html() {
    require_tool!("djlint");
    // Bare `djlint` in a format slot resolves to `djlint:reformat`.
    let out = format("html", "djlint", "html", "<div><p>hi</p></div>");
    assert!(
        out.contains("<div>\n    <p>hi</p>\n</div>"),
        "djlint:reformat should indent the block:\n{out}"
    );
}

#[test]
fn tombi_lints_toml() {
    require_tool!("tombi");
    // The bare id is tombi's lint subcommand, so this covers the diagnostics path that
    // `tombi:format` (a formatter) never exercises.
    let out = lint("toml", "tombi", "toml", "a = ");
    assert!(
        out.contains(&at(0, 5, "tombi")),
        "tombi should report the incomplete key/value pair where the value belongs:\n{out}"
    );
    assert_lint_is_silent("toml", "tombi", "toml", "a = 1");
}

// ---- formatters in a `lint` slot ------------------------------------------
//
// A built-in formatter answers a `lint` slot by formatting the block and comparing, so
// each of these asserts the same contract with the tool's own output as the control:
// unformatted is reported, and what `fmt` writes is accepted.

lint_by_format_test!(
    black_lints_python_by_formatting,
    "black",
    "black",
    "python",
    "python",
    "x=1"
);
lint_by_format_test!(
    ruff_format_lints_python_by_formatting,
    "ruff",
    "ruff:format",
    "python",
    "python",
    "x=1"
);
lint_by_format_test!(
    prettier_lints_javascript_by_formatting,
    "prettier",
    "prettier",
    "javascript",
    "javascript",
    "const x=1"
);
lint_by_format_test!(
    rustfmt_lints_rust_by_formatting,
    "rustfmt",
    "rustfmt",
    "rust",
    "rust",
    "fn  main(){let x=1;}"
);
lint_by_format_test!(
    gofmt_lints_go_by_formatting,
    "gofmt",
    "gofmt",
    "go",
    "go",
    "package main\nfunc  main(){}"
);
lint_by_format_test!(
    goimports_lints_go_by_formatting,
    "goimports",
    "goimports",
    "go",
    "go",
    "package main\nfunc  main(){}"
);
lint_by_format_test!(
    clang_format_lints_cpp_by_formatting,
    "clang-format",
    "clang-format",
    "cpp",
    "cpp",
    "int  main(){return 0;}"
);
lint_by_format_test!(
    yamlfmt_lints_yaml_by_formatting,
    "yamlfmt",
    "yamlfmt",
    "yaml",
    "yaml",
    "a:   1"
);
lint_by_format_test!(taplo_lints_toml_by_formatting, "taplo", "taplo", "toml", "toml", "a=1");
lint_by_format_test!(
    terraform_lints_terraform_by_formatting,
    "terraform",
    "terraform",
    "terraform",
    "terraform",
    "a=1"
);
lint_by_format_test!(
    nixfmt_lints_nix_by_formatting,
    "nixfmt",
    "nixfmt",
    "nix",
    "nix",
    "{ a=1; }"
);
lint_by_format_test!(stylua_lints_lua_by_formatting, "stylua", "stylua", "lua", "lua", "x=1");
lint_by_format_test!(
    ormolu_lints_haskell_by_formatting,
    "ormolu",
    "ormolu",
    "haskell",
    "haskell",
    "main=putStrLn \"hi\""
);
lint_by_format_test!(
    elm_format_lints_elm_by_formatting,
    "elm-format",
    "elm-format",
    "elm",
    "elm",
    "module Main exposing (main)\nmain= 1"
);
lint_by_format_test!(
    swift_format_lints_swift_by_formatting,
    "swift-format",
    "swift-format",
    "swift",
    "swift",
    "let x  =  1"
);
lint_by_format_test!(
    ktfmt_lints_kotlin_by_formatting,
    "ktfmt",
    "ktfmt",
    "kotlin",
    "kotlin",
    "fun  main(){}"
);
lint_by_format_test!(
    beautysh_lints_shell_by_formatting,
    "beautysh",
    "beautysh",
    "shell",
    "shell",
    "if true\nthen\necho hi\nfi"
);
lint_by_format_test!(
    shfmt_lints_shell_by_formatting,
    "shfmt",
    "shfmt",
    "shell",
    "shell",
    "if true;then echo hi;fi"
);
lint_by_format_test!(
    shuck_format_lints_shell_by_formatting,
    "shuck",
    "shuck:format",
    "shell",
    "shell",
    "if [ \"$x\" = 1 ];then echo hi;fi"
);
lint_by_format_test!(
    deno_fmt_lints_typescript_by_formatting,
    "deno",
    "deno-fmt:ts",
    "typescript",
    "typescript",
    "const   x=1"
);
lint_by_format_test!(
    oxfmt_lints_javascript_by_formatting,
    "oxfmt",
    "oxfmt",
    "javascript",
    "javascript",
    "const x=1"
);
lint_by_format_test!(
    tombi_format_lints_toml_by_formatting,
    "tombi",
    "tombi:format",
    "toml",
    "toml",
    "a=1"
);

// ---- linters in a `format` slot -------------------------------------------

/// A built-in linter in a `format` slot must leave the block exactly as it was.
///
/// A linter writes its report to stdout, which is where the format path reads the block's
/// replacement from. Before rumdl declined these, `format = ["ruff:check"]` rewrote a clean
/// Python block to the literal text `All checks passed!` - stdout was a summary line, not
/// code, and the empty-output guard never saw it because the linter had nothing to complain
/// about and exited 0. Config validation warns about the same mistake, but a warning cannot
/// undo an overwritten block.
///
/// The positive control is the same tool and sample in the slot it belongs in: `ruff:format`
/// must rewrite `x=1`, so a ruff that stopped working fails this test instead of passing it.
#[test]
fn builtin_linter_in_format_slot_leaves_the_block_alone() {
    require_tool!("ruff");

    let declined = format("python", "ruff:check", "python", "x=1");
    assert_eq!(
        fenced_block(&declined),
        "x=1",
        "a linter in a format slot must not touch the block:\n{declined}"
    );

    let formatted = format("python", "ruff:format", "python", "x=1");
    assert_eq!(
        fenced_block(&formatted),
        "x = 1",
        "ruff did not format the sample, so the assertion above proves nothing:\n{formatted}"
    );
}

// ---- coverage gate --------------------------------------------------------

/// Built-in tool ids with a dedicated `lint`-slot execution test above.
///
/// Every built-in can fill a `lint` slot: a linter reports its diagnostics, a formatter
/// reports the blocks `fmt` would rewrite. So every non-exempt id belongs here.
const VERIFIED_LINT: &[&str] = &[
    "ruff:check",
    "ruff:format",
    "black",
    "prettier",
    "shellcheck",
    "shfmt",
    "shuck",
    "shuck:format",
    "rustfmt",
    "gofmt",
    "goimports",
    "clang-format",
    "sqlfluff:lint",
    "jq",
    "yamlfmt",
    "taplo",
    "terraform:format",
    "nixfmt",
    "stylua",
    "ormolu",
    "elm-format",
    "swift-format",
    "ktfmt",
    "djlint",
    "beautysh",
    "tombi",
    "tombi:format",
    "oxfmt",
    "deno-fmt:ts",
];

/// Built-in tool ids with a dedicated `format`-slot execution test above.
///
/// Only tools that actually format belong here (`builtin_tool_formats`).
const VERIFIED_FORMAT: &[&str] = &[
    "ruff:format",
    "black",
    "prettier",
    "shfmt",
    "shuck:format",
    "rustfmt",
    "gofmt",
    "goimports",
    "clang-format",
    "jq",
    "yamlfmt",
    "taplo",
    "terraform:format",
    "nixfmt",
    "stylua",
    "ormolu",
    "elm-format",
    "swift-format",
    "ktfmt",
    "djlint",
    "beautysh",
    "tombi:format",
    "oxfmt",
    "deno-fmt:ts",
];

/// Built-in tool ids without a dedicated test because they are pure
/// extension/subcommand variants of a VERIFIED tool (same binary), with the reason.
const EXEMPT: &[(&str, &str)] = &[
    (
        "prettier:json",
        "prettier variant (different --stdin-filepath extension)",
    ),
    ("prettier:yaml", "prettier variant"),
    ("prettier:html", "prettier variant"),
    ("prettier:css", "prettier variant"),
    ("prettier:markdown", "prettier variant"),
    ("sqlfluff:fix", "sqlfluff variant (sqlfluff:lint verified)"),
    ("djlint:lint", "djlint variant"),
    (
        "djlint:reformat",
        "djlint variant (bare `djlint` resolves to it in a format slot)",
    ),
    ("tombi:lint", "tombi variant (bare `tombi` verified)"),
    (
        "terraform-fmt",
        "legacy alias of terraform:format, same definition (alias resolution guarded by \
         terraform_fmt_alias_still_formats)",
    ),
    ("oxfmt:js", "oxfmt variant"),
    ("oxfmt:ts", "oxfmt variant"),
    ("oxfmt:jsx", "oxfmt variant"),
    ("oxfmt:tsx", "oxfmt variant"),
    ("oxfmt:json", "oxfmt variant"),
    ("oxfmt:css", "oxfmt variant"),
    ("deno-fmt", "deno-fmt variant (deno-fmt:ts verified)"),
    ("deno-fmt:js", "deno-fmt variant"),
    ("deno-fmt:json", "deno-fmt variant"),
    ("deno-fmt:jsonc", "deno-fmt variant"),
    ("deno-fmt:md", "deno-fmt variant"),
];

/// Gate: every built-in must have an execution test **in each mode it supports**, or an
/// explicit exemption, so a new registry entry cannot ship unverified and neither can a
/// mode nobody ever ran. A mode-blind version of this gate passed while `tombi`'s lint
/// path and every formatter's `lint` slot were untested.
///
/// Fails if a tool is uncovered in either mode, is listed as both verified and exempt, or
/// if the lists name a tool no longer in the registry.
#[test]
fn every_builtin_tool_is_verified_or_exempt() {
    use rumdl_lib::code_block_tools::builtin_tool_formats;
    use std::collections::BTreeSet;

    let registry: BTreeSet<&str> = rumdl_lib::code_block_tools::builtin_tool_ids().into_iter().collect();
    let verified_lint: BTreeSet<&str> = VERIFIED_LINT.iter().copied().collect();
    let verified_format: BTreeSet<&str> = VERIFIED_FORMAT.iter().copied().collect();
    let exempt: BTreeSet<&str> = EXEMPT.iter().map(|(id, _)| *id).collect();

    let verified: BTreeSet<&str> = verified_lint.union(&verified_format).copied().collect();
    let both: Vec<&&str> = verified.intersection(&exempt).collect();
    assert!(both.is_empty(), "ids listed as both verified and exempt: {both:?}");

    let listed: BTreeSet<&str> = verified.union(&exempt).copied().collect();
    let stale: Vec<&&str> = listed.difference(&registry).collect();
    assert!(
        stale.is_empty(),
        "VERIFIED_LINT/VERIFIED_FORMAT/EXEMPT reference tools no longer in the registry (remove them): {stale:?}"
    );

    let mut missing_lint = Vec::new();
    let mut missing_format = Vec::new();
    let mut formats_but_lint_only = Vec::new();

    for id in registry.iter().filter(|id| !exempt.contains(*id)) {
        // Every built-in can fill a lint slot: a linter reports diagnostics, a formatter
        // reports what `fmt` would rewrite.
        if !verified_lint.contains(id) {
            missing_lint.push(*id);
        }

        let formats = builtin_tool_formats(id).expect("registry id has docs metadata");
        if formats && !verified_format.contains(id) {
            missing_format.push(*id);
        }
        if !formats && verified_format.contains(id) {
            formats_but_lint_only.push(*id);
        }
    }

    assert!(
        missing_lint.is_empty(),
        "built-in tools with no `lint`-slot execution test (add one and list it in \
         VERIFIED_LINT, or add an EXEMPT entry): {missing_lint:?}"
    );
    assert!(
        missing_format.is_empty(),
        "built-in tools that format but have no `format`-slot execution test (add one and \
         list it in VERIFIED_FORMAT, or add an EXEMPT entry): {missing_format:?}"
    );
    assert!(
        formats_but_lint_only.is_empty(),
        "VERIFIED_FORMAT lists tools that have no format invocation: {formats_but_lint_only:?}"
    );
}
