//! MD013 `reflow-sentence-double-space`: keeping two spaces after sentences.
//!
//! Two spaces after a sentence mark its end apart from an abbreviation, a
//! convention some editors and authors rely on (see issue #294, which added
//! MD064's `allow-sentence-double-space` for the same convention). Reflow
//! used to rejoin every gap with one space, so wrapping a paragraph lost the
//! spacing. With `reflow-sentence-double-space = true`, reflow keeps two
//! spaces after the end of a sentence and writes two where a line break that
//! ends a sentence is joined; every other gap is still one space. Every test
//! here re-runs `check` on the formatted output with MD064's
//! `allow-sentence-double-space` on, so the two rules agree on the result.

use indoc::indoc;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Settings that reflow at 80 columns with the option on
const ON: &[&str] = &["MD013.reflow=true", "MD013.reflow-sentence-double-space=true"];

/// Run `rumdl fmt` on `content` with MD013 and MD064 enabled and the given
/// settings, then assert that `check` with the same settings is clean and
/// that formatting again changes nothing. Returns the formatted file.
fn fmt(dir: &Path, content: &str, settings: &[&str]) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let run = |subcommand: &str| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command
            .arg(subcommand)
            .arg("--no-config")
            .arg("--no-cache")
            .arg("--enable")
            .arg("MD013,MD064")
            .arg("-c")
            .arg("MD064.allow-sentence-double-space=true");
        for setting in settings {
            command.arg("-c").arg(setting);
        }
        command.arg(&file_path).output().expect("Failed to execute rumdl")
    };

    let output = run("fmt");
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let formatted = fs::read_to_string(&file_path).unwrap();

    let output = run("check");
    assert_eq!(
        output.status.code(),
        Some(0),
        "fmt output must be clean under the same check settings; violations:\n{}\nformatted:\n{formatted}",
        String::from_utf8_lossy(&output.stdout)
    );

    run("fmt");
    assert_eq!(
        fs::read_to_string(&file_path).unwrap(),
        formatted,
        "a second fmt pass must change nothing"
    );
    formatted
}

#[test]
fn test_two_spaces_after_a_sentence_stay_when_reflow_wraps_the_line() {
    let dir = TempDir::new().unwrap();
    let input = "The first sentence of this paragraph runs on for quite a while.  The second sentence follows two spaces later, and the line goes well past eighty columns.\n";
    let expected = indoc! {"
        The first sentence of this paragraph runs on for quite a while.  The second
        sentence follows two spaces later, and the line goes well past eighty columns.
    "};
    assert_eq!(fmt(dir.path(), input, ON), expected);
}

#[test]
fn test_a_line_break_that_ends_a_sentence_joins_with_two_spaces() {
    let dir = TempDir::new().unwrap();
    let input = indoc! {"
        A short first sentence.
        The second sentence is long enough that the whole paragraph needs to be reflowed to fit.
    "};
    let expected = indoc! {"
        A short first sentence.  The second sentence is long enough that the whole
        paragraph needs to be reflowed to fit.
    "};
    assert_eq!(fmt(dir.path(), input, ON), expected);
}

#[test]
fn test_other_gaps_still_become_one_space() {
    let dir = TempDir::new().unwrap();
    // A break after an abbreviation, two spaces mid-sentence, and two spaces
    // after an abbreviation are no sentence ends
    let input = indoc! {"
        Ask e.g.
        the maintainers about the  plan, and write  Dr.  Smith a note that runs well past the limit.
    "};
    let expected = indoc! {"
        Ask e.g. the maintainers about the plan, and write Dr. Smith a note that runs
        well past the limit.
    "};
    assert_eq!(fmt(dir.path(), input, ON), expected);
}

#[test]
fn test_every_gap_becomes_one_space_without_the_option() {
    let dir = TempDir::new().unwrap();
    let input = indoc! {"
        A short first sentence.
        The second sentence is long enough that the whole paragraph needs to be reflowed.  To fit.
    "};
    let expected = indoc! {"
        A short first sentence. The second sentence is long enough that the whole
        paragraph needs to be reflowed. To fit.
    "};
    assert_eq!(fmt(dir.path(), input, &["MD013.reflow=true"]), expected);
}

#[test]
fn test_a_sentence_that_ends_at_the_wrap_point_leaves_no_trailing_spaces() {
    let dir = TempDir::new().unwrap();
    // Trailing spaces would turn the line break into a hard break
    let input = "This line is built so that its first sentence ends right at the limit here.  Then a second sentence starts and keeps going for a good while longer.\n";
    let expected = indoc! {"
        This line is built so that its first sentence ends right at the limit here.
        Then a second sentence starts and keeps going for a good while longer.
    "};
    let formatted = fmt(dir.path(), input, ON);
    assert_eq!(formatted, expected);
    assert!(formatted.lines().all(|line| !line.ends_with(' ')));
}

#[test]
fn test_two_spaces_stay_before_a_link_that_follows_a_sentence() {
    let dir = TempDir::new().unwrap();
    let input = "Read the guide first.  [The guide](https://example.com/guide) explains everything you need to know about it.\n";
    let expected = indoc! {"
        Read the guide first.  [The guide](https://example.com/guide) explains
        everything you need to know about it.
    "};
    assert_eq!(fmt(dir.path(), input, ON), expected);
}

#[test]
fn test_sentence_pack_separates_packed_sentences_with_two_spaces() {
    let dir = TempDir::new().unwrap();
    let input = indoc! {"
        First sentence here.
        Second one.
        Third sentence that makes the paragraph longer than eighty columns, surely.
    "};
    let expected = indoc! {"
        First sentence here.  Second one.
        Third sentence that makes the paragraph longer than eighty columns, surely.
    "};
    let settings = [ON, &["MD013.reflow-mode=\"sentence-pack\""]].concat();
    assert_eq!(fmt(dir.path(), input, &settings), expected);
}

#[test]
fn test_normalize_mode_joins_sentences_with_two_spaces() {
    let dir = TempDir::new().unwrap();
    // Normalize joins short paragraphs and list items even when they fit, so
    // every sentence break is joined: kept two-space gaps stay two, broken
    // sentence ends become two, and a gap that ends no sentence stays one
    let input = indoc! {"
        Short one.  Short two.
        Short three, with e.g.
        an abbreviation.

        - First item sentence.
          Second item sentence.
    "};
    let expected = indoc! {"
        Short one.  Short two.  Short three, with e.g. an abbreviation.

        - First item sentence.  Second item sentence.
    "};
    let settings = [ON, &["MD013.reflow-mode=\"normalize\""]].concat();
    assert_eq!(fmt(dir.path(), input, &settings), expected);
}
