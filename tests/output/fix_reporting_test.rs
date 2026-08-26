//! `fmt` reports what it actually fixed.
//!
//! Two things make that harder than comparing counts. A rule whose fix rewrites the
//! whole document (MD046) attaches no per-warning `Fix`, so "does this warning carry
//! a fix" says nothing about whether it was resolved. And a fix that changes the line
//! count moves every warning below it, so a survivor sits at a different position
//! afterwards than the one it was originally reported at.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn rumdl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run rumdl")
}

/// `rumdl <subcommand> <args...>`, for a fixture checked with the same flags it
/// was formatted with.
fn rumdl_with<'a>(dir: &Path, subcommand: &[&'a str], args: &[&'a str]) -> Output {
    let mut all: Vec<&str> = subcommand.to_vec();
    all.extend_from_slice(args);
    rumdl(dir, &all)
}

fn rumdl_stdin(dir: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run rumdl");
    child.stdin.as_mut().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

/// An indented code block MD046 rewrites into a fenced one, with an unfixable
/// MD052 reference below it. Fencing adds two lines, so the MD052 warning moves
/// from line 7 to line 9 while never being fixed.
const MIXED_DOC: &str = "# T\n\nText:\n\n    code here\n\nThis is a reference [x][zz]\n";

fn mixed_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), "[MD046]\nstyle = \"fenced\"\n").unwrap();
    fs::write(dir.path().join("doc.md"), MIXED_DOC).unwrap();
    dir
}

fn mixed_args(target: &str) -> Vec<&str> {
    vec![
        "fmt",
        "--color",
        "never",
        "--no-cache",
        "--config",
        "rumdl.toml",
        "--enable",
        "MD046,MD052",
        target,
    ]
}

#[test]
fn document_level_fix_is_reported_as_fixed() {
    let dir = mixed_fixture();
    let output = rumdl(dir.path(), &mixed_args("doc.md"));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap().contains("```"),
        "precondition: MD046 should have fenced the block"
    );
    assert!(
        stdout.contains("doc.md:5:1: [MD046] Use fenced code blocks [fixed]"),
        "MD046 fixed the file, so its warning must be marked [fixed].\nstdout:\n{stdout}"
    );
}

#[test]
fn survivor_below_a_fix_is_reported_at_its_new_position_and_not_marked_fixed() {
    let dir = mixed_fixture();
    let output = rumdl(dir.path(), &mixed_args("doc.md"));
    let stdout = String::from_utf8_lossy(&output.stdout);

    let md052: Vec<&str> = stdout.lines().filter(|l| l.contains("MD052")).collect();
    assert_eq!(md052.len(), 1, "expected exactly one MD052 line.\nstdout:\n{stdout}");
    assert!(
        md052[0].contains("doc.md:9:21:"),
        "MD052 sits at line 9 of the file fmt just wrote, not at its pre-fix line 7.\ngot: {}",
        md052[0]
    );
    assert!(
        !md052[0].contains("[fixed]"),
        "MD052 is still in the file, so it must not be marked [fixed].\ngot: {}",
        md052[0]
    );
}

#[test]
fn summary_counts_only_the_warnings_that_disappeared() {
    let dir = mixed_fixture();
    let output = rumdl(dir.path(), &mixed_args("doc.md"));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Fixed 1/2 issues in 1 file"),
        "one of the two issues was fixed.\nstdout:\n{stdout}"
    );
}

/// The same-rule control: MD075 fixes one of its two findings and the other
/// survives, moving up a line. Crediting every warning of a rule that changed
/// the document would report 2/2 here; matching survivors by position would too.
#[test]
fn partially_fixed_rule_credits_only_the_warning_that_disappeared() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("doc.md"),
        "| A | B |\n| - | - |\n| x | y |\n\n| c | d |\n\nProse separator.\n\n| h | i |\n| j | k |\n",
    )
    .unwrap();

    let output = rumdl(
        dir.path(),
        &[
            "fmt",
            "--color",
            "never",
            "--no-cache",
            "--no-config",
            "--enable",
            "MD075",
            "doc.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains(
            "doc.md:5:1: [MD075] Orphaned table row(s) separated from preceding table by 1 blank line(s) [fixed]"
        ),
        "the orphaned row was joined to the table above it.\nstdout:\n{stdout}"
    );
    let headerless: Vec<&str> = stdout.lines().filter(|l| l.contains("header/delimiter")).collect();
    assert_eq!(
        headerless.len(),
        1,
        "expected exactly one headerless-table line.\nstdout:\n{stdout}"
    );
    assert!(
        headerless[0].contains("doc.md:8:1:") && !headerless[0].contains("[fixed]"),
        "the headerless table survives at line 8 and is not fixed.\ngot: {}",
        headerless[0]
    );
    assert!(
        stdout.contains("Fixed 1/2 issues in 1 file"),
        "exactly one of MD075's two findings was resolved.\nstdout:\n{stdout}"
    );
}

/// Markdown inside a fenced block is linted and fixed by its own pass, so the
/// reconciliation only learns about it if that pass reports what it fixed and the
/// re-lint looks in the block at all. The unfixable MD052 beside it is the control
/// for the second half: a re-lint that does not read the block reports no survivor,
/// and a warning nobody fixed and nobody still reports leaves the output entirely.
#[test]
fn an_embedded_markdown_block_reports_what_was_fixed_and_what_survived() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        "[code-block-tools]\nenabled = true\n\n[code-block-tools.languages.markdown]\nenabled = true\nlint = [\"rumdl\"]\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("doc.md"),
        "# T\n\nText.\n\n```markdown\n#Bad\n\nSee [x][zz].\n```\n",
    )
    .unwrap();

    let output = rumdl(
        dir.path(),
        &[
            "fmt",
            "--color",
            "never",
            "--no-cache",
            "--config",
            "rumdl.toml",
            "--enable",
            "MD018,MD052",
            "doc.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap().contains("# Bad"),
        "precondition: the heading inside the block should have been fixed"
    );
    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("doc.md:")).collect();
    assert_eq!(
        diagnostics,
        [
            "doc.md:6:2: [MD018] No space after # in heading [fixed]",
            "doc.md:8:5: [MD052] Reference 'zz' not found",
        ],
        "the fixed heading is credited, the unfixable reference is still reported.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Fixed 1/2 issues in 1 file"),
        "one of the block's two issues was fixed.\nstdout:\n{stdout}"
    );
}

/// A code block handed to an external tool is linted by one set of tools and fixed
/// by another, and their ids do not correspond. So what the format pass resolved
/// can only be attributed to the lint tools as a set, and the block has to be
/// re-linted through the same tools to see whether anything actually went away.
///
/// The tools here are shell scripts so the test depends on nothing installed:
/// `fakelint` reports until the block reads `FORMATTED`, and `fakefmt` writes that.
#[cfg(unix)]
#[test]
fn an_external_code_block_tool_fix_is_reported_as_fixed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.tools.fakelint]\n",
            "command = [\"sh\", \"-c\", \"in=$(cat); case \\\"$in\\\" in FORMATTED*) ;; *) echo '1:1: block needs formatting';; esac\"]\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; printf 'FORMATTED\\\\n'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"fakelint\"]\n",
            "format = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "# T\n\nText.\n\n```python\nx=1\n```\n").unwrap();

    let output = rumdl(
        dir.path(),
        &[
            "fmt",
            "--color",
            "never",
            "--no-cache",
            "--config",
            "rumdl.toml",
            "doc.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        fs::read_to_string(dir.path().join("doc.md"))
            .unwrap()
            .contains("FORMATTED"),
        "precondition: the format tool should have rewritten the block.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("doc.md:6:1: [fakelint] block needs formatting [fixed]"),
        "the format pass resolved what the lint tool reported.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Fixed 1/1 issues in 1 file"),
        "the only issue in the file was fixed.\nstdout:\n{stdout}"
    );
}

/// A Rust file is linted through the markdown in its doc comments and nothing
/// else, so the re-lint has to read it the same way. Handing the source to the
/// markdown linter reports on the Rust code itself: findings that are not in the
/// file, credited to nobody, printed to the user.
#[test]
fn a_fix_in_a_rust_doc_comment_is_reported_as_fixed_and_invents_nothing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lib.rs"), "/// #Bad\npub fn f() {}\n").unwrap();

    let output = rumdl(
        dir.path(),
        &["fmt", "--color", "never", "--no-cache", "--no-config", "lib.rs"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        fs::read_to_string(dir.path().join("lib.rs"))
            .unwrap()
            .starts_with("/// # Bad"),
        "precondition: the doc comment heading should have been fixed"
    );
    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("lib.rs:")).collect();
    assert_eq!(
        diagnostics,
        ["lib.rs:1:6: [MD018] No space after # in heading [fixed]"],
        "the doc-comment fix is the only thing to report.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Fixed 1/1 issues in 1 file"),
        "the only issue in the file was fixed.\nstdout:\n{stdout}"
    );
}

/// A run that fixed nothing already knows what the file says, and asking again is
/// not free: the re-lint runs every configured external tool a second time over
/// every code block, to be told what it was told before the fix pass.
#[cfg(unix)]
#[test]
fn a_run_that_fixed_nothing_does_not_lint_the_file_twice() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.tools.countinglint]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; echo run >> runs.txt; echo '1:1: always complains'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"countinglint\"]\n",
        ),
    )
    .unwrap();
    let doc = "# T\n\nText.\n\n```python\nx = 1\n```\n";
    fs::write(dir.path().join("doc.md"), doc).unwrap();

    let output = rumdl(
        dir.path(),
        &[
            "fmt",
            "--color",
            "never",
            "--no-cache",
            "--config",
            "rumdl.toml",
            "doc.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        doc,
        "precondition: nothing in this document is fixable"
    );
    assert!(
        stdout.contains("[countinglint] always complains"),
        "precondition: the lint tool ran at all.\nstdout:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("runs.txt")).unwrap().lines().count(),
        1,
        "the block should have been handed to the tool once"
    );
}

/// One rule's fix can resolve another rule's finding: trimming the trailing
/// whitespace off an 82-column line takes it under the limit, so MD013 has
/// nothing left to report without MD013 having done anything. Crediting a
/// warning only to the rule that carried a fix for it leaves this one credited
/// to nobody and still printed, at a position in a file that is now clean.
///
/// The re-check is the control: what `fmt` says is left has to be what a plain
/// `check` finds afterwards.
#[test]
fn a_warning_another_rule_resolved_is_reported_as_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let body = "word ".repeat(15) + "abc";
    fs::write(dir.path().join("doc.md"), format!("# Title\n\n{body}    \n")).unwrap();
    let args = [
        "--color",
        "never",
        "--no-cache",
        "--no-config",
        "--enable",
        "MD009,MD013",
        "doc.md",
    ];

    let fmt_out = rumdl_with(dir.path(), &["fmt"], &args);
    let stdout = String::from_utf8_lossy(&fmt_out.stdout);

    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("doc.md:")).collect();
    assert_eq!(
        diagnostics,
        [
            "doc.md:3:79: [MD009] 4 trailing spaces found [fixed]",
            "doc.md:3:81: [MD013] Line length 82 exceeds 80 characters [fixed]",
        ],
        "trimming the trailing spaces shortened the line, so MD013's finding was resolved too.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Fixed 2/2 issues in 1 file"),
        "both findings are gone from the file.\nstdout:\n{stdout}"
    );

    let recheck = rumdl_with(dir.path(), &["check"], &args);
    let recheck_out = String::from_utf8_lossy(&recheck.stdout);
    assert!(
        recheck_out.contains("No issues found"),
        "control: nothing is left in the rewritten file.\nstdout:\n{recheck_out}"
    );
}

/// A rule can report the same line differently before and after a fix. MD013
/// with reflow on says `Line length exceeds 80 characters` for a line it can
/// rewrap and `Line length 88 exceeds 80 characters` for one it cannot, so a
/// paragraph that got reflowed and an unbreakable line that only lost its
/// trailing spaces produce a survivor whose message matches neither original.
/// Matching survivors by text alone reads that as a third disappearance: three
/// of three fixed, with the line still in the file printed nowhere.
#[test]
fn a_survivor_whose_message_changed_is_counted_once_and_still_reported() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), "[MD013]\nreflow = true\n").unwrap();
    let paragraph = ["reflowable"; 12].join(" ");
    let unbreakable = "z".repeat(86);
    fs::write(
        dir.path().join("doc.md"),
        format!("# Title\n\n{paragraph}\n\n{unbreakable}    \n"),
    )
    .unwrap();
    let args = [
        "--color",
        "never",
        "--no-cache",
        "--config",
        "rumdl.toml",
        "--enable",
        "MD009,MD013",
        "doc.md",
    ];

    let fmt_out = rumdl_with(dir.path(), &["fmt"], &args);
    let stdout = String::from_utf8_lossy(&fmt_out.stdout);

    let diagnostics: Vec<&str> = stdout.lines().filter(|line| line.contains("doc.md:")).collect();
    assert_eq!(
        diagnostics,
        [
            "doc.md:3:1: [MD013] Line length exceeds 80 characters [fixed]",
            "doc.md:5:87: [MD009] 4 trailing spaces found [fixed]",
            "doc.md:6:81: [MD013] Line length 88 exceeds 80 characters",
        ],
        "the reworded finding is the line that survived, not a third fix.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Fixed 2/3 issues in 1 file"),
        "the unbreakable line is still too long.\nstdout:\n{stdout}"
    );

    let recheck = rumdl_with(dir.path(), &["check"], &args);
    let recheck_out = String::from_utf8_lossy(&recheck.stdout);
    assert!(
        recheck_out.contains("doc.md:6:81: [MD013] Line length 88 exceeds 80 characters")
            && recheck_out.contains("Found 1 issues"),
        "control: exactly the finding `fmt` left behind.\nstdout:\n{recheck_out}"
    );
}

/// `--diff` writes nothing, but an external formatter that could not run is a
/// fact about the run either way and the user has a flag for not hearing it.
/// Hardcoding the notice off in diff mode makes `check --diff` claim a clean
/// preview of a file whose Python blocks were never formatted at all.
#[cfg(unix)]
#[test]
fn a_missing_external_tool_is_reported_in_diff_mode_unless_silenced() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n",
            "on-missing-tool-binary = \"fail\"\n\n",
            "[code-block-tools.tools.ghosttool]\n",
            "command = [\"rumdl-no-such-binary-xyz\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "format = [\"ghosttool\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "# T\n\nText.\n\n```python\nx=1\n```\n").unwrap();
    let args = ["--color", "never", "--no-cache", "--config", "rumdl.toml", "doc.md"];

    let loud = rumdl_with(dir.path(), &["check", "--diff"], &args);
    let loud_err = String::from_utf8_lossy(&loud.stderr);
    assert!(
        loud_err.contains("Tool binary 'rumdl-no-such-binary-xyz' not found in PATH for language 'python' at line 5"),
        "a formatter that could not run is reported in diff mode too.\nstderr:\n{loud_err}"
    );

    let quiet = rumdl_with(dir.path(), &["check", "--diff", "--silent"], &args);
    let quiet_err = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_err.contains("rumdl-no-such-binary-xyz"),
        "--silent is what suppresses it.\nstderr:\n{quiet_err}"
    );
}

#[test]
fn stdin_reports_document_level_fixes() {
    let dir = mixed_fixture();
    let mut args = mixed_args("doc.md");
    args.pop();
    args.push("--stdin");

    let output = rumdl_stdin(dir.path(), &args, MIXED_DOC);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("```"),
        "precondition: the fenced document goes to stdout.\nstdout:\n{stdout}"
    );
    assert!(
        stderr.contains("<stdin>:5:1: [MD046] Use fenced code blocks [fixed]"),
        "stdin must mark a document-level fix the same way the file path does.\nstderr:\n{stderr}"
    );
    let md052: Vec<&str> = stderr.lines().filter(|l| l.contains("MD052")).collect();
    assert_eq!(md052.len(), 1, "expected exactly one MD052 line.\nstderr:\n{stderr}");
    assert!(
        md052[0].contains("<stdin>:9:21:") && !md052[0].contains("[fixed]"),
        "MD052 survives at line 9 of the emitted document.\ngot: {}",
        md052[0]
    );
    assert!(
        stderr.contains("1 issue(s) fixed, 1 issue(s) remaining"),
        "stdin summary must match what was fixed.\nstderr:\n{stderr}"
    );
}

#[test]
fn stdin_reports_a_run_that_fixed_everything() {
    let dir = tempfile::tempdir().unwrap();
    let output = rumdl_stdin(
        dir.path(),
        &[
            "fmt",
            "--color",
            "never",
            "--no-cache",
            "--no-config",
            "--enable",
            "MD018",
            "--stdin",
        ],
        "#Bad\n",
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(stdout, "# Bad\n", "the heading is fixed on stdout");
    assert!(
        stderr.contains("<stdin>:1:2: [MD018] No space after # in heading [fixed]"),
        "a fixed warning is still reported, with its marker.\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("1 issue(s) fixed, 0 issue(s) remaining"),
        "a run that fixed everything still says so.\nstderr:\n{stderr}"
    );
}
