//! A leading UTF-8 byte order mark is an encoding marker, not document text.
//!
//! Analysis reads the text after the mark, so a file that starts with one is
//! linted and fixed exactly like the same file without it. The mark stays in what
//! is written back, and machine-readable fix ranges address the bytes supplied.
//!
//! The oracle is the same document without the mark: every warning (rule, line,
//! column, message) must match, fix ranges must differ by exactly the mark length,
//! and the fixed text must equal the mark followed by the fixed text of the
//! unmarked document. A positive and a negative control keep the comparison from
//! passing vacuously.

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

const BOM: &str = "\u{feff}";

/// Documents with findings on the first line, on later lines, and with directives.
const CORPUS: &[&str] = &[
    "# Title\n\nText\n",
    "#Title\n\nText  \n",
    "Title\n=====\n\ntext\n",
    "* a\n* b\n\n\n\ntext\n",
    "<!-- rumdl-disable MD013 -->\n\nthis line is long enough that without the directive it would exceed the default limit of eighty characters\n",
    "<!-- rumdl-disable-next-line MD013 -->\nthis line is long enough that without the directive it would exceed the default limit of eighty characters\n",
    "<!-- rumdl-disable-file MD041 -->\nplain first line\n",
    "```\ncode\n```\n\n# After\n",
    "---\ntitle: x\n---\n\n# Title\n",
    "[link](#missing)\n\n# Title\n",
    "no trailing newline",
];

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

fn with_crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

/// `rumdl check --output-format json` over the bytes, as parsed warnings.
fn check_json(bytes: &[u8], extra: &[&str]) -> Vec<Value> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, bytes).unwrap();
    let output = rumdl()
        .args(["check", "--isolated", "--no-cache", "--output-format", "json"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}\n{stdout}"))
}

/// The document after `rumdl check --fix`.
fn fix_file(bytes: &[u8], extra: &[&str]) -> Vec<u8> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, bytes).unwrap();
    rumdl()
        .args(["check", "--isolated", "--no-cache", "--fix"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read(&path).unwrap()
}

/// The document after `rumdl fmt`.
fn fmt_file(bytes: &[u8]) -> Vec<u8> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, bytes).unwrap();
    rumdl()
        .args(["fmt", "--isolated", "--no-cache"])
        .arg(&path)
        .output()
        .expect("run rumdl fmt");
    fs::read(&path).unwrap()
}

fn stdin(args: &[&str], input: &[u8]) -> (Option<i32>, Vec<u8>, String) {
    let mut child = rumdl()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rumdl");
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code(),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// (rule, line, column, message) of each warning, without file names or fixes.
fn locations(warnings: &[Value]) -> Vec<(String, u64, u64, String)> {
    warnings
        .iter()
        .map(|w| {
            (
                w["rule"].as_str().unwrap().to_string(),
                w["line"].as_u64().unwrap(),
                w["column"].as_u64().unwrap(),
                w["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn fix_ranges(warnings: &[Value]) -> Vec<Option<(u64, u64, String)>> {
    warnings
        .iter()
        .map(|w| {
            w.get("fix").map(|fix| {
                (
                    fix["range"]["start"].as_u64().unwrap(),
                    fix["range"]["end"].as_u64().unwrap(),
                    fix["replacement"].as_str().unwrap().to_string(),
                )
            })
        })
        .collect()
}

fn headings(markdown: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut open = None;
    for event in Parser::new_ext(markdown, Options::empty()) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => open = Some((level as u8, String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, buffer)) = open.as_mut() {
                    buffer.push_str(&text);
                }
            }
            Event::End(_) => found.extend(open.take().map(|(level, text)| format!("{level}:{text}"))),
            _ => {}
        }
    }
    found
}

#[test]
fn heading_after_a_bom_is_the_first_line_heading() {
    let warnings = check_json(format!("{BOM}# Title\n\nText\n").as_bytes(), &["--enable", "MD041"]);
    assert_eq!(locations(&warnings), vec![], "{warnings:?}");
}

#[test]
fn control_a_first_line_that_is_not_a_heading_still_warns_behind_a_bom() {
    let warnings = check_json(format!("{BOM}Text\n\n# Title\n").as_bytes(), &["--enable", "MD041"]);
    assert_eq!(
        locations(&warnings)
            .iter()
            .map(|w| (w.0.as_str(), w.1, w.2))
            .collect::<Vec<_>>(),
        vec![("MD041", 1, 1)],
        "{warnings:?}"
    );
}

#[test]
fn warnings_match_the_unmarked_document_and_fix_ranges_shift_by_the_mark() {
    let mut compared_fixes = 0;
    for variant in [false, true] {
        for doc in CORPUS {
            let doc = if variant { with_crlf(doc) } else { (*doc).to_string() };
            let plain = check_json(doc.as_bytes(), &[]);
            let marked = check_json(format!("{BOM}{doc}").as_bytes(), &[]);
            assert_eq!(locations(&plain), locations(&marked), "crlf={variant} {doc:?}");
            let shifted: Vec<_> = fix_ranges(&plain)
                .into_iter()
                .map(|fix| fix.map(|(s, e, r)| (s + 3, e + 3, r)))
                .collect();
            compared_fixes += shifted.iter().flatten().count();
            assert_eq!(shifted, fix_ranges(&marked), "crlf={variant} {doc:?}");
        }
    }
    assert!(
        compared_fixes >= 8,
        "the corpus must exercise fix ranges ({compared_fixes})"
    );
}

#[test]
fn applying_a_json_fix_to_the_supplied_bytes_gives_the_fixed_document() {
    let input = format!("{BOM}#Title\n\nText\n");
    let warnings = check_json(input.as_bytes(), &["--enable", "MD018"]);
    let fixes = fix_ranges(&warnings);
    let (start, end, replacement) = fixes[0].clone().expect("MD018 carries a fix");
    let mut bytes = input.clone().into_bytes();
    bytes.splice(start as usize..end as usize, replacement.bytes());
    assert_eq!(String::from_utf8(bytes).unwrap(), format!("{BOM}# Title\n\nText\n"));
}

#[test]
fn fix_keeps_exactly_one_mark_and_equals_the_unmarked_fix() {
    let mut changed = 0;
    for variant in [false, true] {
        for doc in CORPUS {
            let doc = if variant { with_crlf(doc) } else { (*doc).to_string() };
            let plain = fix_file(doc.as_bytes(), &[]);
            let marked = fix_file(format!("{BOM}{doc}").as_bytes(), &[]);
            let expected = [BOM.as_bytes(), &plain].concat();
            assert_eq!(
                String::from_utf8_lossy(&marked),
                String::from_utf8_lossy(&expected),
                "crlf={variant} {doc:?}"
            );
            if plain != doc.as_bytes() {
                changed += 1;
            }
            // Idempotence: a second pass changes nothing.
            assert_eq!(fix_file(&marked, &[]), marked, "second pass, crlf={variant} {doc:?}");
            assert_eq!(fmt_file(&marked), marked, "fmt after fix, crlf={variant} {doc:?}");
        }
    }
    assert!(changed >= 8, "the corpus must exercise fixes ({changed})");
}

#[test]
fn fmt_keeps_the_mark() {
    let marked = fmt_file(format!("{BOM}# Title\n\n\n\nText\n").as_bytes());
    assert_eq!(String::from_utf8(marked).unwrap(), format!("{BOM}# Title\n\nText\n"));
}

#[test]
fn fix_does_not_change_the_heading_structure() {
    let doc = format!("{BOM}# Title\n\n##  Sub\n\ntext\n");
    let fixed = String::from_utf8(fix_file(doc.as_bytes(), &[])).unwrap();
    let before = headings(doc.trim_start_matches('\u{feff}'));
    let after = headings(fixed.trim_start_matches('\u{feff}'));
    assert_eq!(before, vec!["1:Title", "2:Sub"]);
    assert_eq!(after, before);
    assert!(fixed.starts_with(BOM) && !fixed[3..].starts_with(BOM), "{fixed:?}");
}

#[test]
fn a_second_mark_is_document_text() {
    let doc = format!("{BOM}{BOM}# Title\n\nText\n");
    let warnings = check_json(doc.as_bytes(), &["--enable", "MD041"]);
    assert_eq!(locations(&warnings).len(), 1, "only one mark is skipped: {warnings:?}");
    let fixed = fix_file(doc.as_bytes(), &["--enable", "MD041"]);
    assert!(
        fixed.starts_with(&[0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF]),
        "both marks survive"
    );
}

#[test]
fn a_mark_inside_the_document_is_left_alone() {
    let doc = format!("# Title\n\ntext{BOM}more\n");
    assert_eq!(fix_file(doc.as_bytes(), &[]), doc.as_bytes());
}

#[test]
fn a_file_holding_only_the_mark_is_clean() {
    for doc in [BOM.to_string(), format!("{BOM}\n"), format!("{BOM}\r\n")] {
        let warnings = check_json(doc.as_bytes(), &["--enable", "MD041,MD047"]);
        let plain = check_json(
            doc.trim_start_matches('\u{feff}').as_bytes(),
            &["--enable", "MD041,MD047"],
        );
        assert_eq!(locations(&warnings), locations(&plain), "{doc:?}");
    }
}

#[test]
fn an_inline_directive_on_line_one_works_behind_a_mark() {
    let long = "this line is long enough that it exceeds the default limit of eighty characters by far";
    let doc = format!("{BOM}<!-- rumdl-disable MD013 -->\n\n# Title\n\n{long}\n");
    assert_eq!(locations(&check_json(doc.as_bytes(), &["--enable", "MD013"])), vec![]);
    let control = format!("{BOM}<!-- note -->\n\n# Title\n\n{long}\n");
    assert_eq!(
        locations(&check_json(control.as_bytes(), &["--enable", "MD013"]))
            .iter()
            .map(|w| w.0.as_str().to_string())
            .collect::<Vec<_>>(),
        vec!["MD013"]
    );
}

#[test]
fn an_unknown_rule_in_a_line_one_directive_is_reported_behind_a_mark() {
    let directive = "<!-- rumdl-disable MD9999 -->\n\n# Title\n";
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let run = |text: &str| {
        fs::write(&path, text).unwrap();
        let output = rumdl()
            .args(["check", "--isolated", "--no-cache"])
            .arg(&path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let plain = run(directive);
    assert!(plain.contains("MD9999"), "control: {plain}");
    assert!(run(&format!("{BOM}{directive}")).contains("MD9999"));
}

#[test]
fn stdin_check_and_fmt_handle_the_mark() {
    let input = format!("{BOM}# Title\n\n\n\nText\n");
    let (code, stdout, _) = stdin(
        &[
            "check",
            "--no-cache",
            "--isolated",
            "--stdin",
            "--enable",
            "MD041,MD012",
            "--output-format",
            "json",
        ],
        input.as_bytes(),
    );
    assert_eq!(code, Some(1));
    let warnings: Vec<Value> = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        locations(&warnings)
            .iter()
            .map(|w| (w.0.as_str().to_string(), w.1, w.2))
            .collect::<Vec<_>>(),
        vec![("MD012".to_string(), 3, 1), ("MD012".to_string(), 4, 1)],
        "the heading is recognised, so MD041 stays quiet: {warnings:?}"
    );
    let (_, stdout, _) = stdin(&["fmt", "--no-cache", "--isolated", "--stdin"], input.as_bytes());
    assert_eq!(String::from_utf8(stdout).unwrap(), format!("{BOM}# Title\n\nText\n"));
}

#[test]
fn a_conflict_marker_on_a_marked_first_line_is_at_column_one() {
    let warnings = check_json(format!("{BOM}<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\n").as_bytes(), &[]);
    let md092: Vec<_> = locations(&warnings).into_iter().filter(|w| w.0 == "MD092").collect();
    assert_eq!(md092.iter().map(|w| (w.1, w.2)).collect::<Vec<_>>(), vec![(1, 1)]);
}

/// A fenced Markdown block is a fragment, not a file: a U+FEFF at its start is
/// ordinary content, so the block is fixed as the lint of that block reads it
/// (no heading, hence no blank line after it), while the file's own mark stays.
#[test]
fn a_mark_at_the_start_of_an_embedded_block_is_content_not_an_encoding_mark() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nformat = [\"rumdl:format\"]\n",
    )
    .unwrap();
    let path = dir.path().join("doc.md");
    let original = format!("{BOM}# Title\n\n```markdown\n{BOM}# Inside\nbody   \n```\n");
    fs::write(&path, &original).unwrap();
    let output = rumdl()
        .current_dir(dir.path())
        .args(["fmt", "--no-cache", "doc.md"])
        .output()
        .expect("run rumdl fmt");
    let fixed = fs::read_to_string(&path).unwrap();
    assert_eq!(
        fixed,
        format!("{BOM}# Title\n\n```markdown\n{BOM}# Inside\nbody\n```\n"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The `fail-fast` preflight looks for code blocks in the text after the mark: a
/// fence on the first line is work, so the run is not aborted for lack of tools.
#[test]
fn the_no_tools_preflight_finds_a_first_line_fence_behind_the_mark() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[code-block-tools]\nenabled = true\non-no-tools-run = \"fail-fast\"\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n",
    )
    .unwrap();
    let mut codes = Vec::new();
    for prefix in ["", BOM] {
        fs::write(
            dir.path().join("doc.md"),
            format!("{prefix}```markdown\ninside \n```\n"),
        )
        .unwrap();
        let output = rumdl()
            .current_dir(dir.path())
            .args(["check", "--no-cache", "doc.md"])
            .output()
            .expect("run rumdl check");
        codes.push(output.status.code());
    }
    assert_eq!(codes[0], codes[1], "the mark must not change the exit code: {codes:?}");
    assert_eq!(
        codes[0],
        Some(1),
        "the control must find the block's finding: {codes:?}"
    );
}

/// A configure-file directive on the first line takes effect behind the mark.
#[test]
fn a_configure_file_directive_on_the_first_line_applies_behind_the_mark() {
    let doc = "<!-- rumdl-configure-file { \"MD013\": { \"line-length\": 20 } } -->\n\n# Title\n\nthis line is longer than twenty characters with words words words words words words\n";
    let plain = locations(&check_json(doc.as_bytes(), &["--enable", "MD013"]));
    let marked = locations(&check_json(format!("{BOM}{doc}").as_bytes(), &["--enable", "MD013"]));
    assert!(
        plain.iter().any(|w| w.0 == "MD013" && w.1 == 5),
        "control: the directive lowers the limit: {plain:?}"
    );
    assert_eq!(plain, marked);
}

/// A project whose python tool records every run in `runs.txt`.
#[cfg(unix)]
fn recording_tool_project(document: &str) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\nenabled = true\n\n",
            "[code-block-tools.tools.fakelint]\n",
            "command = [\"sh\", \"-c\", \"in=$(cat); echo lint >> runs.txt; case \\\"$in\\\" in FORMATTED*) ;; *) echo '1:1: needs formatting';; esac\"]\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; printf 'FORMATTED\\\\n'\"]\n\n",
            "[code-block-tools.languages.python]\nlint = [\"fakelint\"]\nformat = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), document).unwrap();
    dir
}

/// A fence on the first line is found behind the mark: the tool is formatted
/// into place, the mark stays, and a cached run still runs the tool again.
#[cfg(unix)]
#[test]
fn a_first_line_fence_behind_the_mark_is_formatted_and_never_served_from_the_cache() {
    let dir = recording_tool_project(&format!("{BOM}```python\nx=1\n```\n"));
    for _ in 0..2 {
        rumdl()
            .current_dir(dir.path())
            .args(["check", "doc.md"])
            .output()
            .expect("run rumdl check");
    }
    let runs = fs::read_to_string(dir.path().join("runs.txt")).unwrap_or_default();
    assert_eq!(runs, "lint\nlint\n", "each cached run executes the tool again");

    fs::remove_file(dir.path().join("runs.txt")).unwrap();
    let output = rumdl()
        .current_dir(dir.path())
        .args(["fmt", "--no-cache", "doc.md"])
        .output()
        .expect("run rumdl fmt");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        format!("{BOM}```python\nFORMATTED\n```\n"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The full output format quotes the source line and underlines the finding; the
/// quoted line is the text after the mark, so the underline sits under the spaces.
#[test]
fn the_full_output_quotes_the_first_line_without_the_mark() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, format!("{BOM}text   \n\n# T\n")).unwrap();
    let output = rumdl()
        .args([
            "check",
            "--isolated",
            "--no-cache",
            "--enable",
            "MD009",
            "--output-format",
            "full",
        ])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD009"),
        "positive control: the finding is reported: {stdout}"
    );
    assert!(
        !stdout.contains(BOM),
        "the quoted line must not carry the mark: {stdout:?}"
    );
    assert!(
        stdout.contains(" 1 | text   \n   |     ^^^\n"),
        "the underline sits under the trailing spaces: {stdout}"
    );
}

#[test]
fn bom_rust_source_first_doc_comment_is_linted_and_fixed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    for prefix in ["//!", "///"] {
        let input = format!("{BOM}{prefix} #Title\n{prefix}\n{prefix} body   \nfn example() {{}}\n");
        fs::write(&path, &input).unwrap();
        let check = rumdl()
            .args([
                "check",
                "--isolated",
                "--no-cache",
                "--enable",
                "MD018,MD009",
                "--output-format",
                "json",
            ])
            .arg(&path)
            .output()
            .unwrap();
        let warnings: Vec<Value> = serde_json::from_slice(&check.stdout).unwrap();
        assert!(
            warnings.iter().any(|w| w["rule"] == "MD018" && w["line"] == 1),
            "{warnings:?}"
        );
        assert!(warnings.iter().any(|w| w["rule"] == "MD009"));
        let fixed = rumdl()
            .args(["fmt", "--isolated", "--no-cache", "--enable", "MD018,MD009"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(fixed.status.success(), "{}", String::from_utf8_lossy(&fixed.stderr));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{BOM}{prefix} # Title\n{prefix}\n{prefix} body\nfn example() {{}}\n")
        );
    }
}

#[test]
fn bom_embedded_findings_never_expose_fragment_fix_ranges() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[code-block-tools]\nenabled = true\n[code-block-tools.languages.markdown]\nlint = [\"rumdl\"]\n",
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), format!("{BOM}```markdown\n#Title\n```\n")).unwrap();
    let output = rumdl()
        .current_dir(dir.path())
        .args([
            "check",
            "--no-cache",
            "--enable",
            "MD018",
            "--output-format",
            "json",
            "doc.md",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let warnings: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        warnings.iter().any(|w| w["rule"] == "MD018" && w["line"] == 2),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().all(|w| w.get("fix").is_none_or(Value::is_null)),
        "{warnings:?}"
    );
}

#[test]
fn bom_lossy_input_keeps_invalid_encoding_diagnostics_in_analysis_coordinates() {
    for (input, expected_line) in [(b"# T\n\na\xffb\n".as_slice(), 3), (b"a\xffb\n".as_slice(), 1)] {
        let plain = check_json(input, &["--enable", "MD094"]);
        let marked = [BOM.as_bytes(), input].concat();
        let warnings = check_json(&marked, &["--enable", "MD094"]);
        assert_eq!(locations(&warnings), locations(&plain));
        assert!(
            warnings
                .iter()
                .any(|w| w["rule"] == "MD094" && w["line"] == expected_line && w["column"] == 2),
            "{warnings:?}"
        );
        assert!(warnings.iter().all(|w| w.get("fix").is_none_or(Value::is_null)));
        assert_eq!(fmt_file(&marked), marked, "lossy bytes must never be rewritten");
    }
}

#[test]
fn bom_cached_fixes_match_uncached_input_ranges() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, format!("{BOM}#Title\n\ntext   \n")).unwrap();
    let run = |extra: &[&str]| {
        rumdl()
            .current_dir(dir.path())
            .args(["check", "--isolated", "--enable", "MD018,MD009"])
            .args(extra)
            .arg(&path)
            .output()
            .unwrap()
    };
    let cold = run(&["--output-format", "json"]);
    let warm = run(&["--output-format", "json"]);
    let uncached = run(&["--no-cache", "--output-format", "json"]);
    let parse = |output: &std::process::Output| -> Vec<Value> {
        assert_eq!(output.status.code(), Some(1));
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let warnings = parse(&cold);
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings, parse(&warm));
    assert_eq!(warnings, parse(&uncached));
    let verbose = run(&["--verbose"]);
    assert!(String::from_utf8_lossy(&verbose.stdout).contains("Cache hit for"));
}
