//! Regression tests for MD037 on emphasis that spans more than one line.
//!
//! Markers are paired one line at a time. On a line that continues a span, the
//! first marker on that line is really the span's *closing* delimiter, but the
//! line-local pairing read it as an opener, so every marker after it paired one
//! position off. Ordinary words then sat between what looked like delimiters,
//! MD037 reported "spaces inside emphasis markers", and `rumdl fmt` deleted
//! those spaces: `_ and _` became `_and_`, joining two words into one.
//!
//! The document-level parse settles it: a marker already spent closing a span
//! cannot also open one.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD037NoSpaceInEmphasis;

fn check(content: &str) -> Vec<String> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    MD037NoSpaceInEmphasis
        .check(&ctx)
        .unwrap()
        .into_iter()
        .map(|w| format!("{}:{} {}", w.line, w.column, w.message))
        .collect()
}

fn fix(content: &str) -> String {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    MD037NoSpaceInEmphasis.fix(&ctx).unwrap()
}

/// The failing case: two quoted spans, the first continuing onto a second line.
/// The `and` between the spans was reported as emphasized-with-spaces.
#[test]
fn test_text_between_spans_is_not_flagged_when_a_span_wraps() {
    let content = "_\"She said \"yes.\"\nThe vote carried.\"_ and _\"He said \"no.\" The motion failed.\"_\n";

    assert!(
        check(content).is_empty(),
        "text between two spans is not emphasis: {:?}",
        check(content)
    );
    assert_eq!(fix(content), content, "fix must not touch the spaces around 'and'");
}

/// Control: the same text on a single line was always correct, so the bug is
/// specific to a span crossing a line boundary.
#[test]
fn test_single_line_control_is_unaffected() {
    let content = "_\"She said \"yes.\" The vote carried.\"_ and _\"He said \"no.\" The motion failed.\"_\n";
    assert!(check(content).is_empty(), "{:?}", check(content));
}

/// Control: a wrapped span without interior quotes was always correct.
#[test]
fn test_wrapped_span_without_quotes_control() {
    let content = "_She said yes.\nThe vote carried._ and _She said no. The motion failed._\n";
    assert!(check(content).is_empty(), "{:?}", check(content));
}

/// A genuine violation must still be reported, including on a line that also
/// continues a real span. Suppressing the false positive must not blind the rule.
#[test]
fn test_real_violations_still_reported() {
    let plain = "This is * spaced emphasis * text.\n";
    assert_eq!(
        check(plain).len(),
        1,
        "plain violation must be flagged: {:?}",
        check(plain)
    );

    let after_wrapped_span = "_alpha\nbeta._ then * spaced * here\n";
    let warnings = check(after_wrapped_span);
    assert_eq!(
        warnings.len(),
        1,
        "a violation after a wrapped span must still be flagged: {warnings:?}"
    );
    assert!(warnings[0].starts_with("2:"), "on the second line: {warnings:?}");
    assert_eq!(fix(after_wrapped_span), "_alpha\nbeta._ then *spaced* here\n");
}

/// A span that both opens and closes inside the flagged run is a different
/// shape from a span that only closes inside it, and must stay reported:
/// `* _real_ *` really is emphasis with spaces in it.
#[test]
fn test_span_nested_inside_the_run_is_still_a_violation() {
    let content = "Text * _real_ * more.\n";
    assert_eq!(
        check(content).len(),
        1,
        "spaces inside the outer markers are a violation: {:?}",
        check(content)
    );
    assert_eq!(fix(content), "Text *_real_* more.\n");
}
