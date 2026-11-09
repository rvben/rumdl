//! Inline directive comments against the real binary.
//!
//! - A warning reported on an enabled line survives a disabled region further
//!   down its range; only its fix is withheld.
//! - MD044 recognises the Vale and remark-lint directive comments by their
//!   syntax, whatever whitespace follows `<!--`, and never rewrites one.
//! - One whitespace class serves every comment helper.
//! - A directive comment may span lines.
//! - MD048 exempts a block whose opening fence is disabled: its closing fence
//!   is neither reported nor rewritten.
//!
//! Each case has a control that must behave the other way: the same document
//! without the directive, or a near-miss comment that is not one.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const MD013_NORMALIZE: &str =
    "[global]\nenable = [\"MD013\"]\n[MD013]\nline-length = 30\nreflow = true\nreflow-mode = \"normalize\"\n";
const MD013_PLAIN: &str = "[global]\nenable = [\"MD013\"]\n[MD013]\nline-length = 20\n";
const MD044: &str = "[global]\nenable = [\"MD044\"]\n[MD044]\nnames = [\"JavaScript\"]\n";

/// Run `rumdl <command> --config <file> --no-cache doc.md`; returns the output and
/// the document as it is afterwards.
fn run(config: &str, command: &str, doc: &str) -> (Output, String) {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("rumdl.toml");
    fs::write(&config_path, config).unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .arg(command)
        .args(["--config", config_path.to_str().unwrap(), "--no-cache"])
        .arg(&path)
        .output()
        .expect("run rumdl");
    let code = output.status.code();
    assert!(
        matches!(code, Some(0 | 1)),
        "rumdl {command} failed ({code:?}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&path).unwrap();
    (output, after)
}

/// The `check` output lines that report `rule`, as `(line, is_fixable)`.
fn findings(config: &str, rule: &str, doc: &str) -> Vec<(usize, bool)> {
    let (output, _) = run(config, "check", doc);
    let tag = format!("[{rule}]");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&tag))
        .map(|line| {
            let rest = line.split(".md:").nth(1).expect("file:line:col prefix");
            let number = rest.split(':').next().unwrap().parse().unwrap();
            (number, line.trim_end().ends_with("[*]"))
        })
        .collect()
}

fn fmt(config: &str, doc: &str) -> String {
    run(config, "fmt", doc).1
}

// ── 19: a disabled region inside a list item ────────────────────────────────

const ITEM: &str = "- aaaa bbbb cccc dddd eeee ffff gggg hhhh\n";
const REGION: &str = "  <!-- rumdl-disable MD013 -->\n  iiii jjjj kkkk\n  <!-- rumdl-enable MD013 -->\n";

#[test]
fn normalize_warning_on_an_enabled_line_survives_a_disabled_region_below() {
    let doc = format!("{ITEM}{REGION}");
    let found = findings(MD013_NORMALIZE, "MD013", &doc);
    assert_eq!(found.len(), 1, "line 1 is over the limit and enabled: {found:?}");
    assert_eq!(found[0].0, 1);
    assert!(!found[0].1, "the fix would rewrite a disabled line, so none is offered");
}

#[test]
fn the_withheld_fix_leaves_the_disabled_region_alone() {
    let doc = format!("{ITEM}{REGION}\nxxxx yyyy zzzz xxxx yyyy zzzz xxxx yyyy zzzz\n");
    let after = fmt(MD013_NORMALIZE, &doc);
    assert!(
        after.starts_with(&format!("{ITEM}{REGION}")),
        "list item untouched: {after:?}"
    );
    assert_eq!(
        after.trim_end().rsplit('\n').take(2).collect::<Vec<_>>(),
        ["xxxx yyyy zzzz", "xxxx yyyy zzzz xxxx yyyy zzzz"],
        "the paragraph after the item is still normalized: {after:?}"
    );
    assert_eq!(fmt(MD013_NORMALIZE, &after), after, "second pass changes nothing");
}

#[test]
fn control_without_the_region_the_item_is_reported_and_fixable() {
    let doc = format!("{ITEM}  iiii jjjj kkkk\n");
    let found = findings(MD013_NORMALIZE, "MD013", &doc);
    assert_eq!(found, [(1, true)]);
    assert_ne!(fmt(MD013_NORMALIZE, &doc), doc);
}

#[test]
fn a_plain_paragraph_with_a_disabled_line_keeps_its_over_long_enabled_line() {
    let doc = "aaaa bbbb cccc dddd eeee ffff gggg hhhh\niiii jjjj kkkk <!-- rumdl-disable-line MD013 -->\n";
    let found = findings(MD013_NORMALIZE, "MD013", doc);
    assert_eq!(found, [(1, false)]);
}

#[test]
fn control_a_disabled_first_line_still_drops_the_warning() {
    let doc = format!("<!-- rumdl-disable MD013 -->\n{ITEM}  iiii jjjj kkkk\n<!-- rumdl-enable MD013 -->\n");
    assert!(findings(MD013_NORMALIZE, "MD013", &doc).is_empty());
}

// ── default-mode reflow: one over-long line, one paragraph-wide fix ──────────
//
// Default mode reports each over-long line on its own, and every such warning
// carries the fix for the whole paragraph. The fix is withheld when it would
// rewrite a disabled line, whichever line the warning is reported on.

const MD013_REFLOW: &str = "[global]\nenable = [\"MD013\"]\n[MD013]\nline-length = 30\nreflow = true\n";

const LONG_A: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh";
const LONG_B: &str = "iiii jjjj kkkk llll mmmm nnnn oooo pppp";

/// Paragraphs holding a disabled line beside an enabled over-long one, with the
/// 1-indexed line the enabled one is on.
fn paragraphs_with_a_disabled_line() -> Vec<(String, usize)> {
    vec![
        (
            format!("<!-- rumdl-disable-next-line MD013 -->\n{LONG_A}\n{LONG_B}\n"),
            3,
        ),
        (
            format!("{LONG_A}\niiii jjjj kkkk <!-- rumdl-disable-line MD013 -->\n"),
            1,
        ),
        (
            format!("{LONG_A} <!-- markdownlint-disable-line MD013 -->\n{LONG_B}\n"),
            2,
        ),
        (
            format!("> {LONG_A}\n> iiii jjjj kkkk <!-- rumdl-disable-line MD013 -->\n"),
            1,
        ),
        (format!("> {LONG_A} <!-- rumdl-disable-line MD013 -->\n> {LONG_B}\n"), 2),
    ]
}

#[test]
fn default_reflow_reports_the_enabled_line_of_a_paragraph_with_a_disabled_line_without_a_fix() {
    for (doc, enabled_line) in paragraphs_with_a_disabled_line() {
        assert_eq!(
            findings(MD013_REFLOW, "MD013", &doc),
            [(enabled_line, false)],
            "the fix would rewrite a disabled line, so none is offered: {doc:?}"
        );
    }
}

#[test]
fn default_reflow_leaves_a_paragraph_with_a_disabled_line_as_written() {
    for (doc, _) in paragraphs_with_a_disabled_line() {
        assert_eq!(fmt(MD013_REFLOW, &doc), doc, "{doc:?}");
    }
}

#[test]
fn default_reflow_still_rewraps_the_paragraph_after_one_with_a_disabled_line() {
    let kept = format!("<!-- rumdl-disable-next-line MD013 -->\n{LONG_A}\n{LONG_B}\n");
    let after = fmt(MD013_REFLOW, &format!("{kept}\n{LONG_A}\n"));
    assert_eq!(after, format!("{kept}\naaaa bbbb cccc dddd eeee ffff\ngggg hhhh\n"));
    assert_eq!(fmt(MD013_REFLOW, &after), after, "second pass changes nothing");
}

/// The fix covers whole lines up to, not including, the line after the
/// paragraph: a disabled line on either side of it is no reason to withhold it.
#[test]
fn default_reflow_fixes_a_paragraph_beside_a_disabled_line_it_does_not_hold() {
    let wrapped = "aaaa bbbb cccc dddd eeee ffff\ngggg hhhh";
    let heading = "# iiii jjjj kkkk llll <!-- rumdl-disable-line MD013 -->";
    for (doc, expected, paragraph_line) in [
        // The disabled line starts where the fix ends.
        (format!("{LONG_A}\n{heading}\n"), format!("{wrapped}\n{heading}\n"), 1),
        (format!("{LONG_A}\n{heading}"), format!("{wrapped}\n{heading}"), 1),
        // The disabled line ends where the fix starts.
        (format!("{heading}\n{LONG_A}\n"), format!("{heading}\n{wrapped}\n"), 2),
        (format!("{heading}\n{LONG_A}"), format!("{heading}\n{wrapped}"), 2),
    ] {
        assert_eq!(
            findings(MD013_REFLOW, "MD013", &doc),
            [(paragraph_line, true)],
            "{doc:?}"
        );
        let after = fmt(MD013_REFLOW, &doc);
        assert_eq!(after, expected, "{doc:?}");
        assert_eq!(fmt(MD013_REFLOW, &after), after, "second pass: {doc:?}");
    }
}

#[test]
fn control_default_reflow_without_a_directive_offers_the_fix_and_rewraps() {
    for doc in [format!("{LONG_A}\n{LONG_B}\n"), format!("> {LONG_A}\n> {LONG_B}\n")] {
        let found = findings(MD013_REFLOW, "MD013", &doc);
        assert!(found.iter().any(|&(_, fixable)| fixable), "{doc:?}: {found:?}");
        assert_ne!(fmt(MD013_REFLOW, &doc), doc, "{doc:?}");
    }
}

// ── 20, 21: MD044 and tool comments ─────────────────────────────────────────

fn md044_lines(doc: &str) -> Vec<usize> {
    findings(MD044, "MD044", doc)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn prose_comments_that_merely_start_like_a_vale_directive_are_checked() {
    let doc =
        "<!-- vale offset javascript -->\n<!-- vale onboarding javascript -->\n<!-- vale style guide javascript -->\n";
    assert_eq!(md044_lines(doc), [1, 2, 3]);
    let after = fmt(MD044, doc);
    assert_eq!(after.matches("JavaScript").count(), 3, "{after:?}");
}

#[test]
fn directive_comments_in_any_spelling_are_skipped_and_never_rewritten() {
    let doc = "<!-- lint disable no-javascript -->\n\
               <!--lint disable no-javascript-->\n\
               <!--  lint disable no-javascript -->\n\
               <!-- vale off -->\n\
               <!--vale off-->\n\
               <!--  vale on  -->\n\
               <!-- vale Style.javascript = NO -->\n\
               <!--\tlint ignore no-javascript\t-->\n";
    assert!(md044_lines(doc).is_empty());
    assert_eq!(fmt(MD044, doc), doc);
}

#[test]
fn prose_around_a_directive_comment_is_still_checked() {
    let doc = "javascript <!-- vale off -->\njavascript <!--\nlint disable no-javascript\n--> javascript\n";
    assert_eq!(md044_lines(doc), [1, 2, 4]);
    let after = fmt(MD044, doc);
    assert!(
        after.contains("lint disable no-javascript\n"),
        "directive text untouched: {after:?}"
    );
}

#[test]
fn a_multi_line_directive_comment_is_not_rewritten() {
    let doc = "<!--\nlint disable no-javascript\n-->\n\njavascript\n";
    assert_eq!(md044_lines(doc), [5], "only the prose line is reported");
    assert_eq!(fmt(MD044, doc), "<!--\nlint disable no-javascript\n-->\n\nJavaScript\n");
}

// ── 22: one whitespace class ────────────────────────────────────────────────

const LONG: &str = "This line is much longer than twenty columns.";

#[test]
fn feff_around_a_directive_counts_as_comment_whitespace() {
    let doc = format!("<!--\u{feff}rumdl-disable MD013\u{feff}-->\n\n{LONG}\n");
    assert!(findings(MD013_PLAIN, "MD013", &doc).is_empty());
}

#[test]
fn control_zero_width_space_is_not_whitespace() {
    let doc = format!("<!--\u{200b}rumdl-disable MD013\u{200b}-->\n\n{LONG}\n");
    assert_eq!(findings(MD013_PLAIN, "MD013", &doc).len(), 1);
}

#[test]
fn control_the_same_document_without_a_directive_is_reported() {
    let doc = format!("{LONG}\n");
    assert_eq!(findings(MD013_PLAIN, "MD013", &doc).len(), 1);
}

// ── 23: directive comments spanning lines ───────────────────────────────────

fn md013_lines(doc: &str) -> Vec<usize> {
    findings(MD013_PLAIN, "MD013", doc)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn keyword_on_the_line_after_the_opener_disables() {
    for tool in ["rumdl", "markdownlint"] {
        let doc = format!("<!--\n{tool}-disable MD013\n-->\n\n{LONG}\n");
        assert!(md013_lines(&doc).is_empty(), "{tool}");
    }
}

#[test]
fn every_multi_line_layout_disables() {
    for open in [
        "<!--\nrumdl-disable MD013\n-->",
        "<!-- \nrumdl-disable MD013 -->",
        "<!-- rumdl-disable\nMD013 -->",
        "<!-- rumdl-disable MD013\n-->",
        "<!--\n\nrumdl-disable\n MD013 \n\n-->",
    ] {
        let doc = format!("{open}\n\n{LONG}\n");
        assert!(md013_lines(&doc).is_empty(), "{open:?}");
    }
}

#[test]
fn control_without_the_comment_the_line_is_reported() {
    assert_eq!(md013_lines(&format!("\n\n{LONG}\n")), [3]);
}

#[test]
fn multi_line_disable_next_line_covers_the_line_after_the_comment() {
    let doc = format!("<!--\nrumdl-disable-next-line MD013\n-->\n{LONG}\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [6]);
}

#[test]
fn multi_line_enable_ends_a_region() {
    let doc = format!("<!-- rumdl-disable MD013 -->\n{LONG}\n<!--\nrumdl-enable MD013\n-->\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [6]);
}

#[test]
fn multi_line_capture_and_restore() {
    let doc =
        format!("<!--\nrumdl-capture\n-->\n<!-- rumdl-disable MD013 -->\n{LONG}\n<!--\nrumdl-restore\n-->\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [9]);
}

#[test]
fn a_multi_line_directive_in_a_code_block_configures_nothing() {
    let doc = format!("```\n<!--\nrumdl-disable MD013\n-->\n```\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [7]);
}

#[test]
fn a_comment_that_merely_mentions_a_keyword_mid_text_is_not_a_directive() {
    let doc = format!("<!--\nnote: rumdl-disable MD013\n-->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
}

#[test]
fn unknown_rule_in_a_multi_line_directive_is_reported_on_the_opening_line() {
    let doc = "\n<!--\nrumdl-disable MD9999\n-->\n";
    let (output, _) = run(MD013_PLAIN, "check", doc);
    let text = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("MD9999"), "{text}");
    assert!(text.contains("doc.md:2"), "reported on the opening line: {text}");
}

#[test]
fn an_unterminated_opener_in_a_code_block_does_not_hide_the_directive_after_it() {
    let doc = format!("```\n<!-- rumdl-disable\n```\n\n<!--\nrumdl-disable MD013\n-->\n\n{LONG}\n");
    assert!(md013_lines(&doc).is_empty());
}

#[test]
fn prose_after_a_single_line_directive_comment_is_still_checked() {
    let doc = "<!--vale off--> javascript\n<!-- rumdl-disable MD044 --> javascript\n";
    assert_eq!(md044_lines(doc), [1, 2]);
    assert_eq!(
        fmt(MD044, doc),
        "<!--vale off--> JavaScript\n<!-- rumdl-disable MD044 --> JavaScript\n"
    );
}

#[test]
fn an_unclosed_tool_comment_example_in_code_hides_no_prose() {
    let doc = "```\n<!-- vale off\n```\n\njavascript\n\n<!-- note -->\n";
    assert_eq!(md044_lines(doc), [5]);
}

#[test]
fn a_multi_line_directive_after_a_code_example_reports_its_unknown_rule() {
    let doc = "```\n<!-- rumdl-disable\n```\n\n<!--\nrumdl-disable MD9999\n-->\n";
    let (output, _) = run(MD013_PLAIN, "check", doc);
    let text = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("MD9999") && text.contains("doc.md:5"), "{text}");
}

#[test]
fn a_multi_line_directive_inside_a_blockquote_disables() {
    for open in [
        "> <!--\n> rumdl-disable MD013\n> -->",
        "> <!-- rumdl-disable\n> MD013 -->",
        ">> <!--\n>> rumdl-disable MD013\n>> -->",
        "- > <!--\n  > rumdl-disable MD013\n  > -->",
    ] {
        let doc = format!("{open}\n\n{LONG}\n");
        assert!(md013_lines(&doc).is_empty(), "{open:?}");
    }
}

#[test]
fn control_a_blockquote_comment_that_only_mentions_a_keyword_is_not_a_directive() {
    let doc = format!("> <!--\n> note: rumdl-disable MD013\n> -->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
}

#[test]
fn a_greater_than_sign_in_a_top_level_comment_is_text_not_a_quote_marker() {
    let doc = format!("<!--\n> rumdl-disable MD013\n-->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
    let doc = format!("<!-- rumdl-disable\n> MD009\n-->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5], "a literal `>` is not a marker, MD013 stays on");
    let doc = format!("text a > b <!--\n> rumdl-disable MD013\n-->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
    let doc = format!("1 > 0 <!--\n> rumdl-disable MD013\n-->\n\n{LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
}

#[test]
fn a_blockquote_marker_between_the_words_of_a_tool_comment_is_not_directive_text_to_rewrite() {
    for doc in [
        "> <!-- lint\n> disable no-javascript\n> -->\n",
        "> <!-- vale\n> off javascript\n> -->\n",
        "> <!-- vale\n> javascript\n> = NO\n> -->\n",
    ] {
        assert!(md044_lines(doc).is_empty(), "{doc:?}");
    }
}

#[test]
fn a_marker_deeper_than_the_enclosing_blockquote_is_comment_text() {
    let doc = format!("> <!--\n> > rumdl-disable MD013\n> -->\n\n> {LONG}\n");
    assert_eq!(md013_lines(&doc), [5]);
    let doc = format!(">> <!--\n>> rumdl-disable MD013\n>> -->\n\n>> {LONG}\n");
    assert!(md013_lines(&doc).is_empty(), "control: matching depth is a directive");
}

#[test]
fn prose_before_a_comment_in_a_blockquote_does_not_hide_the_quote_markers() {
    let doc = format!("> prose <!--\n> rumdl-disable MD013\n> -->\n\n> {LONG}\n");
    assert!(md013_lines(&doc).is_empty());
    for doc in [
        "> prose <!-- lint\n> disable no-javascript\n> -->\n",
        "- > note <!-- vale\n  > off javascript\n  > -->\n",
    ] {
        assert!(md044_lines(doc).is_empty(), "{doc:?}");
    }
}

#[test]
fn an_unclosed_configure_file_example_in_code_does_not_hide_the_directive_after_it() {
    let doc = format!("```\n<!-- rumdl-configure-file\n```\n\n<!--\nrumdl-disable MD013\n-->\n\n{LONG}\n");
    assert!(md013_lines(&doc).is_empty());
}

// ── MD048: a directive on the opening fence exempts the whole block ─────────
//
// MD048 reports the two fences of a block on their own lines, each with a fix
// for that line alone. Only the opener can carry a directive, because nothing
// between the fences is one, and a closing fence converted on its own no longer
// closes its opener: the block then swallows the rest of the document.

const MD048_BACKTICK: &str = "[global]\nenable = [\"MD048\"]\n[MD048]\nstyle = \"backtick\"\n";
const TILDE_BLOCK: &str = "~~~markdown\n```rust\nfn main() {}\n```\n~~~\n\nAfter the block.\n";

/// The tilde block with MD048 disabled on its opening fence, in each spelling.
fn tilde_blocks_disabled_at_the_opening_fence() -> Vec<String> {
    vec![
        format!("<!-- rumdl-disable-next-line MD048 -->\n{TILDE_BLOCK}"),
        format!("<!-- markdownlint-disable-next-line MD048 -->\n{TILDE_BLOCK}"),
        format!("<!-- rumdl-disable-next-line -->\n{TILDE_BLOCK}"),
        format!("<!-- rumdl-disable-next-line MD048 -->\n{TILDE_BLOCK}").replace('\n', "\r\n"),
    ]
}

#[test]
fn a_block_disabled_at_its_opening_fence_is_not_reported_at_its_closing_fence() {
    for doc in tilde_blocks_disabled_at_the_opening_fence() {
        assert!(findings(MD048_BACKTICK, "MD048", &doc).is_empty(), "{doc:?}");
    }
}

#[test]
fn a_block_disabled_at_its_opening_fence_keeps_its_closing_fence() {
    for doc in tilde_blocks_disabled_at_the_opening_fence() {
        assert_eq!(fmt(MD048_BACKTICK, &doc), doc);
    }
}

#[test]
fn the_default_rule_set_keeps_a_block_disabled_at_its_opening_fence_closed() {
    let doc = format!("# Title\n\n```text\nfirst\n```\n\n<!-- rumdl-disable-next-line MD048 -->\n{TILDE_BLOCK}");
    assert_eq!(fmt("", &doc), doc);
}

#[test]
fn a_block_after_one_disabled_at_its_opening_fence_is_still_converted() {
    let doc = format!("<!-- rumdl-disable-next-line MD048 -->\n{TILDE_BLOCK}\n~~~\nplain\n~~~\n");
    assert_eq!(findings(MD048_BACKTICK, "MD048", &doc), [(10, true), (12, true)]);
    assert_eq!(
        fmt(MD048_BACKTICK, &doc),
        format!("<!-- rumdl-disable-next-line MD048 -->\n{TILDE_BLOCK}\n```\nplain\n```\n")
    );
}

#[test]
fn control_without_the_directive_both_fences_are_reported_and_converted() {
    assert_eq!(findings(MD048_BACKTICK, "MD048", TILDE_BLOCK), [(1, true), (5, true)]);
    assert_eq!(
        fmt(MD048_BACKTICK, TILDE_BLOCK),
        "````markdown\n```rust\nfn main() {}\n```\n````\n\nAfter the block.\n"
    );
}
