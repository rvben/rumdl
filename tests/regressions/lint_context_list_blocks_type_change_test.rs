// Regression tests: a list block that ends because a list of a *different type*
// follows must keep its properly indented continuation lines. Only genuinely
// lazy continuations (indented to column 0) are trimmed back.
//
// When the block was truncated to its last marker line, MD032 saw the item as
// ending at the marker and inserted a blank line *inside* the item, splitting
// it into a list plus a stray paragraph.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

fn blocks(content: &str) -> Vec<(usize, usize)> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    ctx.list_blocks.iter().map(|b| (b.start_line, b.end_line)).collect()
}

#[test]
fn test_bullet_continuation_kept_when_ordered_list_follows() {
    // Line 2 is indented to the bullet's content column, so it belongs to item 1.
    assert_eq!(
        blocks("- alpha beta\n  aligned\n1. ordered item\n"),
        vec![(1, 2), (3, 3)]
    );
}

#[test]
fn test_ordered_continuation_kept_when_bullet_list_follows() {
    assert_eq!(
        blocks("1. alpha beta\n   aligned\n- bullet item\n"),
        vec![(1, 2), (3, 3)]
    );
}

#[test]
fn test_multiple_continuation_lines_kept() {
    assert_eq!(
        blocks("- alpha beta\n  second\n  third\n1. ordered item\n"),
        vec![(1, 3), (4, 4)]
    );
}

#[test]
fn test_lazy_continuation_still_trimmed_when_different_type_follows() {
    // A column-0 continuation is what the original trim was written for. This
    // pins that behavior as the boundary of the fix above; whether MD032 should
    // then split the lazy line off into its own paragraph is a separate question.
    assert_eq!(blocks("- alpha beta\nlazy\n1. ordered item\n"), vec![(1, 1), (3, 3)]);
}

#[test]
fn test_same_type_list_still_merges_continuations() {
    // A same-type marker never triggers the trim; the whole run is one block.
    assert_eq!(blocks("- alpha beta\n  aligned\n- second item\n"), vec![(1, 3)]);
}

#[test]
fn test_no_continuation_is_unaffected() {
    assert_eq!(blocks("- alpha beta\n1. ordered item\n"), vec![(1, 1), (2, 2)]);
}

#[test]
fn test_blockquote_continuation_kept_when_different_type_follows() {
    // Inside a blockquote the raw indent is 0 because of the `>` marker. The
    // indentation that matters is the one after the blockquote prefix.
    assert_eq!(
        blocks("> - alpha beta\n>   aligned\n> 1. ordered item\n"),
        vec![(1, 2), (3, 3)]
    );
}

#[test]
fn test_blockquote_lazy_continuation_still_trimmed() {
    // No indentation after the `>`, so this line is lazy, as at the root level.
    assert_eq!(
        blocks("> - alpha beta\n> lazy\n> 1. ordered item\n"),
        vec![(1, 1), (3, 3)]
    );
}
