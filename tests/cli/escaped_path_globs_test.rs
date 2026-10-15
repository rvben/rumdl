//! A backslash escapes the next character in every path glob a config holds, off
//! Windows. On Windows a backslash is a path separator and a literal
//! metacharacter is written as a class or a brace (`[[]x[]].md`, `{!}bang.md`).
//!
//! The walk reads `include` and `exclude` patterns through `ignore`, which always
//! treats `\` as an escape. `globset` alone treats it as a path separator on
//! Windows, so every glob rumdl compiled itself (named files, stdin, `include`'s
//! explicit-file matching, `per-file-ignores`, `per-file-flavor`) silently read
//! `\[x\].md` as `/[x/].md` there. File names here avoid `*` and `?`, which
//! Windows forbids; the unit tests cover those on candidate strings.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `MD012` fires on every file, so a file with no findings was excluded or
/// had the rule ignored.
const DOCUMENT: &str = "# Title\n\na\n\n\n\nb\n";

const FILES: &[&str] = &["[x].md", "x.md", "!bang.md", "[x].txt", "x.txt"];

fn write_project(root: &Path, config: &str) {
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".rumdl.toml"), config).unwrap();
    for file in FILES {
        fs::write(root.join(file), DOCUMENT).unwrap();
    }
}

fn run(cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "--output-format", "concise"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    child.wait_with_output().expect("failed to collect rumdl output")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace('\\', "/")
}

/// The files with at least one `MD012` finding.
fn flagged(output: &Output) -> BTreeSet<String> {
    let stdout = stdout_of(output);
    FILES
        .iter()
        .filter(|file| {
            stdout
                .lines()
                .any(|line| line.contains(&format!("{file}:")) && line.contains("MD012"))
        })
        .map(|file| file.to_string())
        .collect()
}

fn set(files: &[&str]) -> BTreeSet<String> {
    files.iter().map(|file| file.to_string()).collect()
}

fn project(config: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path(), config);
    temp
}

#[cfg(not(windows))]
#[test]
fn per_file_ignores_escape_names_the_literal_file() {
    let temp = project("[per-file-ignores]\n'\\[x\\].md' = [\"MD012\"]\n");
    let first = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&first), set(&["x.md", "!bang.md"]), "{}", stdout_of(&first));
    // The same run twice gives the same answer.
    assert_eq!(flagged(&run(temp.path(), &["."], b"")), flagged(&first));

    // Control: unescaped, the brackets are a class that selects `x.md` instead.
    let temp = project("[per-file-ignores]\n'[x].md' = [\"MD012\"]\n");
    let class = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&class), set(&["[x].md", "!bang.md"]), "{}", stdout_of(&class));
}

#[cfg(not(windows))]
#[test]
fn per_file_ignores_escaped_bang_names_the_literal_file() {
    let temp = project("[per-file-ignores]\n'\\!bang.md' = [\"MD012\"]\n");
    let output = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&output), set(&["[x].md", "x.md"]), "{}", stdout_of(&output));
}

/// `{{#runtime-import ...}}` is a bare URL for the standard flavor and plain
/// content for gh-aw, so the flavor a file gets decides whether `MD034` fires.
#[cfg(not(windows))]
#[test]
fn per_file_flavor_escape_names_the_literal_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    fs::write(
        temp.path().join(".rumdl.toml"),
        "[global]\nenable = [\"MD034\"]\n\n[per-file-flavor]\n'\\[x\\].md' = \"gh-aw\"\n",
    )
    .unwrap();
    let content = "# Triage\n\n{{#runtime-import https://example.com/shared.md:10-50}}\n";
    for file in ["[x].md", "x.md"] {
        fs::write(temp.path().join(file), content).unwrap();
    }
    let output = run(temp.path(), &["."], b"");
    let stdout = stdout_of(&output);
    assert!(
        !stdout
            .lines()
            .any(|line| line.contains("[x].md:") && line.contains("MD034")),
        "the escaped key gives [x].md the gh-aw flavor\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("x.md:") && line.contains("MD034")),
        "control: x.md keeps the standard flavor and is flagged\n{stdout}"
    );
}

#[cfg(not(windows))]
#[test]
fn exclude_escape_names_the_literal_file_on_every_route() {
    let temp = project("[global]\nexclude = ['\\[x\\].md', '\\!bang.md']\n");
    // The walk reads only markdown; a file named on the command line is linted
    // whatever its extension.
    let walk = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&walk), set(&["x.md"]), "walk\n{}", stdout_of(&walk));

    let expected = set(&["x.md", "[x].txt", "x.txt"]);
    let named = run(temp.path(), FILES, b"");
    assert_eq!(flagged(&named), expected, "named files\n{}", stdout_of(&named));

    let batch_input: String = FILES.iter().map(|file| format!("{file}\0{DOCUMENT}\0")).collect();
    let batch = run(temp.path(), &["--stdin-batch"], batch_input.as_bytes());
    assert_eq!(flagged(&batch), expected, "--stdin-batch\n{}", stdout_of(&batch));
}

/// An explicit `include` of a non-markdown file is matched by the walker and, for
/// the final gate, by rumdl's own matcher. Both must read the escape: the walk
/// finds `[x].txt` only if the type filter and the matcher both keep it.
#[cfg(not(windows))]
#[test]
fn include_escape_names_the_literal_file() {
    let temp = project("[global]\ninclude = ['\\[x\\].txt', '\\[x\\].md']\n");
    let expected = set(&["[x].md", "[x].txt"]);

    let walk = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&walk), expected, "walk\n{}", stdout_of(&walk));

    assert_eq!(flagged(&run(temp.path(), &["."], b"")), flagged(&walk), "idempotent");
}

/// Windows twin of the escape tests: with no escape character, a literal
/// metacharacter is spelled with a class or a brace.
#[cfg(windows)]
#[test]
fn windows_spellings_name_the_literal_file() {
    let temp = project("[per-file-ignores]\n'[[]x[]].md' = [\"MD012\"]\n");
    let output = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&output), set(&["x.md", "!bang.md"]), "{}", stdout_of(&output));

    let temp = project("[per-file-ignores]\n'{!}bang.md' = [\"MD012\"]\n");
    let output = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&output), set(&["[x].md", "x.md"]), "{}", stdout_of(&output));

    let temp = project("[global]\nexclude = ['[[]x[]].md', '{!}bang.md']\n");
    let walk = run(temp.path(), &["."], b"");
    assert_eq!(flagged(&walk), set(&["x.md"]), "walk\n{}", stdout_of(&walk));
    let named = run(temp.path(), FILES, b"");
    assert_eq!(
        flagged(&named),
        set(&["x.md", "[x].txt", "x.txt"]),
        "named files\n{}",
        stdout_of(&named)
    );
}
