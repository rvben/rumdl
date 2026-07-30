//! MD013's reflow can measure a line the way its check measures one.
//!
//! By default the two disagree: the check forgives a line whose only excess is
//! an inline link destination or an inline code span, while reflow measures
//! every character and rewraps the paragraph anyway. `reflow-length-exemptions`
//! makes reflow consult the same `ignore-link-urls` and `code-spans` options the
//! check reads.
//!
//! The property that must hold in every case below: reflow is never more
//! forgiving than the check. Formatting may leave a line the check accepts, but
//! it must never leave one the check reports.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `rumdl fmt` with the given `MD013` settings and return the rewritten file.
fn fmt(dir: &Path, content: &str, settings: &[&str]) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013");
    for setting in settings {
        command.arg("-c").arg(format!("MD013.{setting}"));
    }
    let output = command.arg(&file_path).output().expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// MD013 findings `rumdl check` reports for `content` under the same settings.
fn check_findings(dir: &Path, content: &str, settings: &[&str]) -> Vec<String> {
    let file_path = dir.join("checked.md");
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg("check")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013");
    for setting in settings {
        command.arg("-c").arg(format!("MD013.{setting}"));
    }
    let output = command.arg(&file_path).output().expect("Failed to execute rumdl");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("[MD013]"))
        .map(str::to_string)
        .collect()
}

/// Assert the formatter left nothing the check objects to under the same config.
fn assert_no_desync(dir: &Path, formatted: &str, settings: &[&str]) {
    let findings = check_findings(dir, formatted, settings);
    assert!(
        findings.is_empty(),
        "fmt produced a line its own check reports under {settings:?}: {findings:?}\n{formatted}"
    );
}

/// The text of every line the check reports, so two formats can be compared
/// without their line numbers having to match.
fn reported_line_texts(dir: &Path, content: &str, settings: &[&str]) -> Vec<String> {
    let content_lines: Vec<&str> = content.lines().collect();
    check_findings(dir, content, settings)
        .iter()
        .filter_map(|finding| {
            let line_no: usize = finding.split(':').nth(1)?.parse().ok()?;
            content_lines.get(line_no.checked_sub(1)?).map(|l| l.to_string())
        })
        .collect()
}

fn lines(text: &str) -> Vec<&str> {
    text.trim_end_matches('\n').lines().collect()
}

const REPORTED: &str = "This is a piece of [text](https://google.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasdjklfhlkajsdhfkjlashdflkjashdflkjahdskfjlhasdjfha) that should be on a single line\n";

#[test]
fn reported_paragraph_wraps_by_default() {
    let dir = TempDir::new().unwrap();
    let settings = ["reflow = true", "line-length = 90"];
    let formatted = fmt(dir.path(), REPORTED, &settings);

    assert_eq!(
        lines(&formatted),
        vec![
            "This is a piece of",
            "[text](https://google.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasdjklfhlkajsdhfkjlashdflkjashdflkjahdskfjlhasdjfha)",
            "that should be on a single line",
        ],
        "the default must keep measuring the markdown as written"
    );
}

#[test]
fn reported_paragraph_is_left_alone_with_exemptions() {
    let dir = TempDir::new().unwrap();
    let settings = ["reflow = true", "line-length = 90", "reflow-length-exemptions = true"];
    let formatted = fmt(dir.path(), REPORTED, &settings);

    assert_eq!(formatted, REPORTED, "the link destination is the only excess");
    assert_no_desync(dir.path(), &formatted, &settings);
}

#[test]
fn exemptions_follow_ignore_link_urls() {
    let dir = TempDir::new().unwrap();
    let settings = [
        "reflow = true",
        "line-length = 90",
        "reflow-length-exemptions = true",
        "ignore-link-urls = false",
    ];
    let formatted = fmt(dir.path(), REPORTED, &settings);

    assert!(
        lines(&formatted).len() > 1,
        "with the check counting URLs, reflow must count them too:\n{formatted}"
    );
}

#[test]
fn strict_disables_the_link_exemption_in_reflow() {
    let dir = TempDir::new().unwrap();
    let settings = [
        "reflow = true",
        "line-length = 90",
        "reflow-length-exemptions = true",
        "strict = true",
    ];
    let formatted = fmt(dir.path(), REPORTED, &settings);

    assert!(
        lines(&formatted).len() > 1,
        "strict removes every forgiveness from the check, so reflow keeps none either:\n{formatted}"
    );
}

/// A paragraph that is genuinely too long is still wrapped: the option forgives
/// link destinations, not prose.
#[test]
fn ordinary_prose_still_wraps() {
    let dir = TempDir::new().unwrap();
    let content =
        "This paragraph is made of ordinary prose words that simply run on and on well past the configured budget.\n";
    let settings = ["reflow = true", "line-length = 80", "reflow-length-exemptions = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert_eq!(
        lines(&formatted).len(),
        2,
        "over-long prose must still wrap:\n{formatted}"
    );
    assert_no_desync(dir.path(), &formatted, &settings);
}

/// A code span cannot be wrapped, so `code-spans = false` tells the check to
/// forgive a line whose only excess is one. With the option on, reflow stops
/// rearranging such a line.
#[test]
fn code_span_exemption_follows_code_spans_option() {
    let dir = TempDir::new().unwrap();
    let content = "Invoke `pipeline --stage build --profile release --target aarch64-apple-darwin` to start.\n";

    let default_settings = ["reflow = true", "line-length = 80", "reflow-length-exemptions = true"];
    let with_default = fmt(dir.path(), content, &default_settings);
    assert!(
        lines(&with_default).len() > 1,
        "code-spans defaults to true, so the check counts the span and reflow must too:\n{with_default}"
    );

    let exempt_settings = [
        "reflow = true",
        "line-length = 80",
        "reflow-length-exemptions = true",
        "code-spans = false",
    ];
    let with_exemption = fmt(dir.path(), content, &exempt_settings);
    assert_eq!(with_exemption, content, "the code span is the only excess");
    assert_no_desync(dir.path(), &with_exemption, &exempt_settings);
}

/// The check tests each exemption against the budget on its own, so a line is
/// forgiven when the link-exempt width fits *or* the code-exempt width fits,
/// never when only the two savings together would. Reflow has to keep them apart
/// the same way: summing them would leave a line the check reports.
///
/// The line below is 102 columns at a budget of 80. Discounting the URL alone
/// leaves 90, discounting the code span alone leaves 90, and discounting both
/// would leave 78.
#[test]
fn exemptions_are_taken_separately_not_summed() {
    let dir = TempDir::new().unwrap();
    let content =
        "alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima `0123456789` [g](a.com/xyzw)\n";
    let settings = [
        "reflow = true",
        "line-length = 80",
        "reflow-length-exemptions = true",
        "code-spans = false",
    ];

    let unformatted = check_findings(dir.path(), content, &settings);
    assert_eq!(
        unformatted.len(),
        1,
        "the premise: neither saving alone brings this line under the budget"
    );

    let formatted = fmt(dir.path(), content, &settings);
    assert_ne!(
        formatted, content,
        "a line the check reports must still be wrapped:\n{formatted}"
    );
    assert_no_desync(dir.path(), &formatted, &settings);
}

/// A reference link carries no inline destination, so the check measures it in
/// full and reflow must as well.
#[test]
fn reference_links_are_not_exempt() {
    let dir = TempDir::new().unwrap();
    let content = "This is a piece of [text][target] that should be on a single line, and it runs past the budget.\n\n[target]: https://example.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasdjklfhlkajsdhf\n";
    let settings = ["reflow = true", "line-length = 80", "reflow-length-exemptions = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(
        lines(&formatted).len() > 3,
        "a reference link earns no saving, so the paragraph still wraps:\n{formatted}"
    );
}

/// A link nested inside an emphasis span is part of that span's content, and
/// reflow charges the span its full width. Measuring too much only wraps a line
/// the check would have forgiven, which is the safe direction; measuring too
/// little would leave a line the check reports.
#[test]
fn link_inside_emphasis_is_measured_in_full() {
    let dir = TempDir::new().unwrap();
    let content = "This is a piece of **bold [text](https://example.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasd) here** that continues.\n";
    let settings = ["reflow = true", "line-length = 80", "reflow-length-exemptions = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(
        lines(&formatted).len() > 1,
        "an emphasis span is charged its full width, so the paragraph wraps:\n{formatted}"
    );
    assert_no_desync(dir.path(), &formatted, &settings);
}

/// Every reflow mode routes through a different set of length gates. All four
/// have to honour the exemption, or the option would silently do nothing in the
/// mode a user happens to have configured.
#[test]
fn every_reflow_mode_honours_the_exemption() {
    let dir = TempDir::new().unwrap();
    for mode in ["default", "normalize", "sentence-per-line", "semantic-line-breaks"] {
        let settings = [
            "reflow = true".to_string(),
            "line-length = 90".to_string(),
            "reflow-length-exemptions = true".to_string(),
            format!("reflow-mode = \"{mode}\""),
        ];
        let refs: Vec<&str> = settings.iter().map(String::as_str).collect();
        let formatted = fmt(dir.path(), REPORTED, &refs);

        assert_eq!(formatted, REPORTED, "mode {mode} must leave the paragraph alone");
        assert_no_desync(dir.path(), &formatted, &refs);
    }
}

/// Semantic mode reaches the budget through a per-sentence gate, a cascade of
/// splits, and a merge that pulls a short trailing line back, and the output is
/// whatever the last of those says. This pins the whole chain on a line the raw
/// width demonstrably breaks.
#[test]
fn semantic_mode_keeps_an_exempt_line_whole() {
    let dir = TempDir::new().unwrap();
    let content = "See [x](https://aaaaaaaaaaaaaaaaaaaaaa) now.\n";
    let settings = [
        "reflow = true",
        "line-length = 40",
        "reflow-mode = \"semantic-line-breaks\"",
    ];

    let without = fmt(dir.path(), content, &settings);
    assert_eq!(
        lines(&without).len(),
        2,
        "premise: the raw width forces a break here:\n{without}"
    );

    let mut exempt: Vec<&str> = settings.to_vec();
    exempt.push("reflow-length-exemptions = true");
    let with = fmt(dir.path(), content, &exempt);
    assert_eq!(with, content, "the exempt width fits, so the line stays whole");
    assert_no_desync(dir.path(), &with, &exempt);
}

/// The property the whole option rests on: turning it on never creates a
/// violation. Reflow measures with its own element parser while the check
/// measures with the document's link and code-span index, so the two could
/// disagree on some construct; a disagreement that over-estimated a saving would
/// show up here as a line the formatter left behind and the checker then
/// reported, which formatting without the option does not produce.
///
/// The comparison is against the option-off format rather than against nothing,
/// because reflow cannot shorten every line either way: an unbreakable badge
/// link is over the budget however it is measured.
#[test]
fn enabling_the_option_never_creates_a_violation() {
    let dir = TempDir::new().unwrap();
    let corpus = [
        "Plain prose with an inline [link](https://example.com/a/very/long/destination/path/here) mid-sentence.",
        "A [nested badge](https://example.com/x) and an ![image](https://example.com/some/long/image/path.png) together.",
        "The badge form [![build status](https://img.example.com/badge/build/passing.svg)](https://ci.example.com/project/builds) is one link.",
        "A shortcut [ref] and a collapsed [ref][] and a full [text][ref] all measure in full here today.",
        "Escaped brackets in [a \\] label](https://example.com/escaped/destination/path) stay one link.",
        "A code span inside [a `code` label](https://example.com/code/label/destination) is still one link.",
        "Emphasis around *[a link](https://example.com/inside/emphasis/destination)* changes the measure.",
        "An autolink <https://example.com/autolinks/are/never/exempt/because/they/are/visible> is content.",
        "Trailing punctuation after [the link](https://example.com/trailing/punctuation/case), then more.",
        "Two links [one](https://example.com/first/destination) and [two](https://example.com/second/destination) here.",
        "Wide characters 日本語のテキストと [リンク](https://example.com/wide/destination/path) が混ざる行。",
        "Run `some --command --with --flags` then read [the guide](https://example.com/guide/path) closely.",
        "",
        "[ref]: https://example.com/reference/definition/target",
    ]
    .join("\n\n");

    for mode in ["default", "normalize", "sentence-per-line", "semantic-line-breaks"] {
        let reported = |exemptions: &str| {
            let settings = [
                "reflow = true".to_string(),
                "line-length = 80".to_string(),
                "code-spans = false".to_string(),
                "code-blocks = false".to_string(),
                "tables = false".to_string(),
                format!("reflow-length-exemptions = {exemptions}"),
                format!("reflow-mode = \"{mode}\""),
            ];
            let refs: Vec<&str> = settings.iter().map(String::as_str).collect();
            let formatted = fmt(dir.path(), &format!("{corpus}\n"), &refs);
            reported_line_texts(dir.path(), &formatted, &refs)
        };

        let baseline = reported("false");
        for line in reported("true") {
            assert!(
                baseline.contains(&line),
                "mode {mode}: the option left a line the check reports that formatting \
                 without it does not produce:\n{line}"
            );
        }
    }
}

/// A second pass must find nothing left to do.
#[test]
fn formatting_is_idempotent_under_the_exemption() {
    let dir = TempDir::new().unwrap();
    let content = "Read [the migration guide](https://example.com/docs/guides/migration/v3) and then the release notes for the current version before upgrading anything.\n";
    let settings = ["reflow = true", "line-length = 80", "reflow-length-exemptions = true"];

    let once = fmt(dir.path(), content, &settings);
    let twice = fmt(dir.path(), &once, &settings);
    assert_eq!(once, twice, "a second pass changed the file");
    assert_no_desync(dir.path(), &once, &settings);
}
