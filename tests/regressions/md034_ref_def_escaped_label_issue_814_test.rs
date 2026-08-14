//! Tests for reference definitions whose link label contains an escaped
//! closing bracket (Issue #814)
//!
//! CommonMark §6.3: a link label ends at the first `]` that is **not**
//! backslash-escaped. `[ref\[\]]: url` is therefore a reference definition
//! with the label `ref\[\]`, but the label group of `REF_DEF_PATTERN` did not
//! read escapes, so the line was not recognised as a definition at all and
//! MD034 saw its destination as a bare URL.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD034NoBareUrls, MD053LinkImageReferenceDefinitions};

/// The reporter's document: only the escaped-closing-bracket label misbehaves,
/// so the three definitions must agree with each other.
#[test]
fn test_md034_not_flagged_for_escaped_bracket_label() {
    let content = "# Reference Example\n\n\
                   * [this is a link to example.com][ref1\\[\\]]\n\
                   * [this is also a link to example.com][ref2\\[]\n\
                   * [this is also a link to example.com][ref3]\n\n\
                   [ref1\\[\\]]: https://example.com/ref1\n\
                   [ref2\\[]: https://example.com/ref2\n\
                   [ref3]: https://example.com/ref3\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = MD034NoBareUrls.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "A reference definition destination is not a bare URL: {:?}",
        warnings.iter().map(|w| (w.line, &w.message)).collect::<Vec<_>>()
    );
}

/// The parse itself, not just MD034's view of it: an escaped `]` must not end
/// the label, so all three definitions reach `ctx.reference_definitions()`.
/// Every rule that reads reference definitions (MD052/MD053/MD057) depends on
/// this, so assert the parse directly rather than only its MD034 symptom.
#[test]
fn test_escaped_bracket_label_parsed_as_reference_def() {
    let content = "[ref1\\[\\]]: https://example.com/ref1\n\
                   [ref2\\[]: https://example.com/ref2\n\
                   [ref3]: https://example.com/ref3\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let ids: Vec<&str> = ctx.reference_definitions().iter().map(|d| d.id.as_str()).collect();

    assert_eq!(ids, vec!["ref1\\[\\]", "ref2\\[", "ref3"]);
}

/// The same rule seen from the other side, and the one line whose behavior this
/// change reverses: an escaped `]` does not close the label, so `[ref\]:` never
/// closes one and the line is a paragraph rather than a definition — which
/// leaves its destination a genuinely bare URL for MD034 to report.
///
/// Both halves are asserted together because they are one fact. This is also
/// the input that separates the label pattern from a looser `(?:[^\]]|\\.)+`,
/// which fixes the reported bug just as well but lets a `\` be read as either a
/// literal or an escape, whichever makes the line match.
#[test]
fn test_label_left_open_by_escaped_bracket_is_not_a_reference_def() {
    let content = "[ref\\]: https://example.com/ref\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        ctx.reference_definitions().is_empty(),
        "`[ref\\]:` leaves the label unclosed, so it is not a reference definition: {:?}",
        ctx.reference_definitions()
    );
    assert_eq!(
        MD034NoBareUrls.check(&ctx).unwrap().len(),
        1,
        "and so its destination is a bare URL"
    );
}

/// The fix's real blast radius: definitions that were invisible now reach every
/// rule reading `ctx`, MD053 ("unused reference") among them. A *used* escaped
/// label must not be reported as unused — which holds only because MD053
/// unescapes the definition side and the usage side alike. Assert it rather
/// than rely on the two staying in step.
#[test]
fn test_used_escaped_bracket_label_is_not_reported_unused() {
    let content = "[a][used\\[\\]]\n\n[used\\[\\]]: https://example.com/u\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = MD053LinkImageReferenceDefinitions::default().check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "the definition is used, so it is not unused: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}
