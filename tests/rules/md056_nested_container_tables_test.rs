//! A table inside several containers at once is judged like one inside a single
//! container: a short row is reported, and a bare list marker above a table does
//! not hide it from the blank-line rule.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD056TableColumnCount, MD058BlanksAroundTables};

fn short_row_lines(content: &str) -> Vec<usize> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    MD056TableColumnCount
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|w| w.line)
        .collect()
}

fn missing_blank_before_lines(content: &str) -> Vec<usize> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    MD058BlanksAroundTables::default()
        .check(&ctx)
        .unwrap()
        .iter()
        .filter(|w| w.message.contains("before table"))
        .map(|w| w.line)
        .collect()
}

#[test]
fn a_table_after_a_quote_in_the_same_list_item_is_still_in_the_item() {
    let doc = "- item\n\n  > quote\n\n  |a|b|\n  |-|-|\n  |1|2|\n  |3|\n";
    assert_eq!(short_row_lines(doc), vec![8]);
    // Control: the same item without the quote.
    assert_eq!(short_row_lines("- item\n\n  |a|b|\n  |-|-|\n  |1|2|\n  |3|\n"), vec![6]);
}

#[test]
fn a_table_after_a_quote_in_an_ordered_list_item_is_still_in_the_item() {
    let doc = "1. item\n\n   > quote\n\n   |a|b|\n   |-|-|\n   |1|2|\n   |3|\n";
    assert_eq!(short_row_lines(doc), vec![8]);
}

#[test]
fn a_quote_outside_the_item_ends_it() {
    // The quote is not indented into the item, so the item ends there and the
    // unindented rows form a table outside any list.
    let doc = "- item\n\n> quote\n\n|a|b|\n|-|-|\n|1|2|\n|3|\n";
    assert_eq!(short_row_lines(doc), vec![8]);
}

#[test]
fn a_quoted_table_under_two_markers_on_one_line() {
    let doc = "- - > |a|b|\n    > |-|-|\n    > |1|2|\n    > |3|\n";
    assert_eq!(short_row_lines(doc), vec![4]);
    // Controls: one marker, and no marker.
    assert_eq!(short_row_lines("- > |a|b|\n  > |-|-|\n  > |1|2|\n  > |3|\n"), vec![4]);
    assert_eq!(short_row_lines("> |a|b|\n> |-|-|\n> |1|2|\n> |3|\n"), vec![4]);
}

#[test]
fn a_table_under_two_markers_on_one_line() {
    assert_eq!(short_row_lines("- - |a|b|\n    |-|-|\n    |1|2|\n    |3|\n"), vec![4]);
    assert_eq!(
        short_row_lines("1. 2. |a|b|\n      |-|-|\n      |1|2|\n      |3|\n"),
        vec![4]
    );
}

#[test]
fn a_table_under_a_bare_marker_that_underlines_a_paragraph_follows_a_heading() {
    // An empty item cannot interrupt a paragraph, so the nested `-` underlines
    // `paragraph` and the table follows that heading directly.
    assert_eq!(
        missing_blank_before_lines("- - paragraph\n    -\n    |a|b|\n    |-|-|\n"),
        vec![3]
    );
    // Control: one level.
    assert_eq!(
        missing_blank_before_lines("- paragraph\n  -\n  |a|b|\n  |-|-|\n"),
        vec![3]
    );
}

#[test]
fn a_quoted_table_inside_a_list_item_is_measured_from_its_quote() {
    // The table sits in the quote, not in the item's own content, so the item's
    // indent says nothing about it.
    let doc = "- item\n\n  > quote\n  >\n  >    |a|b|\n  >    |-|-|\n  >    |1|\n";
    assert_eq!(short_row_lines(doc), vec![7]);
}

#[test]
fn a_list_ended_inside_a_quote_does_not_indent_the_table_after_it() {
    let doc = "> - item\n\n  > quote\n\n   |a|b|\n   |-|-|\n   |1|\n";
    assert_eq!(short_row_lines(doc), vec![7]);
}

#[test]
fn a_sibling_item_closes_the_inner_items_of_the_one_before() {
    let doc = "1. - item\n\n    - item\n\n     |a|b|\n     |-|-|\n     |1|\n";
    assert_eq!(short_row_lines(doc), vec![7]);
}

#[test]
fn a_thematic_break_made_of_markers_opens_no_list() {
    let doc = "- - -\n\n  > quote\n\n   |a|b|\n   |-|-|\n   |1|\n";
    assert_eq!(short_row_lines(doc), vec![7]);
}

#[test]
fn an_item_holding_a_thematic_break_is_one_list_level() {
    let doc = "- * * *\n\n    |a|b|\n    |-|-|\n    |1|\n";
    assert_eq!(short_row_lines(doc), vec![5]);
}

#[test]
fn lists_opened_in_a_blockquote_end_with_it() {
    let doc = "- item\n\n  > - - child\n\n    |a|b|\n    |-|-|\n    |1|\n";
    assert_eq!(short_row_lines(doc), vec![7]);
}
