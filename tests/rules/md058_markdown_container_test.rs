//! The `markdown` attribute decides what an HTML element holds: `1` and `block`
//! open a Markdown block, whose first and last rows need no blank line against
//! the element's own tags, and `span` holds inline text, where pipe rows are not
//! a table at all.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD055TablePipeStyle, MD056TableColumnCount, MD058BlanksAroundTables};

const FLAVORS: [MarkdownFlavor; 2] = [MarkdownFlavor::MkDocs, MarkdownFlavor::Standard];

fn md058(content: &str, flavor: MarkdownFlavor) -> Vec<String> {
    let ctx = LintContext::new(content, flavor, None);
    MD058BlanksAroundTables::default()
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|w| format!("{}:{}", w.line, w.message))
        .collect()
}

#[test]
fn a_table_filling_a_markdown_block_element_needs_no_blank_lines() {
    for flavor in FLAVORS {
        for value in ["1", "block"] {
            let doc = format!("<div markdown=\"{value}\">\n|a|b|\n|-|-|\n|1|2|\n</div>\n");
            assert_eq!(md058(&doc, flavor), Vec::<String>::new(), "{flavor:?} {value}");
        }
        assert_eq!(md058("<div markdown>\n|a|b|\n|-|-|\n|1|2|\n</div>\n", flavor).len(), 0);
    }
}

#[test]
fn a_table_after_text_in_a_markdown_block_element_still_needs_a_blank_line() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\ntext\n|a|b|\n|-|-|\n|1|2|\n</div>\n";
        assert_eq!(
            md058(doc, flavor),
            vec!["3:Missing blank line before table".to_string()],
            "{flavor:?}"
        );
    }
}

#[test]
fn a_plain_div_holds_raw_html_not_a_table() {
    let doc = "<div>\n|a|b|\n|-|-|\n|1|2|\n</div>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    let tables = ctx.table_blocks.iter().filter(|b| b.parser_confirmed).count();
    assert_eq!(tables, 0, "a plain div holds raw HTML, not a table");
}

#[test]
fn pipe_rows_in_a_span_element_are_not_a_table() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"span\">\n|a|b|\n|-|-|\n|1|\n</div>\n";
        let ctx = LintContext::new(doc, flavor, None);
        assert!(ctx.table_blocks.is_empty(), "{flavor:?}: {:?}", ctx.table_blocks);
        assert!(MD056TableColumnCount.check(&ctx).unwrap().is_empty());
        assert!(MD055TablePipeStyle::default().check(&ctx).unwrap().is_empty());
        assert!(MD058BlanksAroundTables::default().check(&ctx).unwrap().is_empty());
    }
}

#[test]
fn a_short_row_in_a_markdown_block_element_is_still_reported() {
    // Positive control: the table is recognised there.
    let doc = "<div markdown=\"1\">\n|a|b|\n|-|-|\n|1|\n</div>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    let lines: Vec<usize> = MD056TableColumnCount
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|w| w.line)
        .collect();
    assert_eq!(lines, vec![4]);
}

#[test]
fn a_table_right_after_text_on_the_opening_tag_line_still_needs_a_blank_line() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">intro\n|a|b|\n|-|-|\n|1|2|\n</div>\n";
        assert_eq!(
            md058(doc, flavor),
            vec!["2:Missing blank line before table".to_string()],
            "{flavor:?}"
        );
    }
}

#[test]
fn mdx_keeps_the_table_in_an_element_whose_markdown_prop_says_span() {
    let doc = "<div markdown=\"span\">\n\n|a|b|\n|-|-|\n|1|\n\n</div>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MDX, None);
    let lines: Vec<usize> = MD056TableColumnCount
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|w| w.line)
        .collect();
    assert_eq!(lines, vec![5]);
}

#[test]
fn mdx_treats_the_markdown_prop_as_an_ordinary_prop_for_table_boundaries() {
    let with_prop = md058(
        "<div markdown=\"1\">\n|a|b|\n|-|-|\n|1|2|\n</div>\n",
        MarkdownFlavor::MDX,
    );
    let without_prop = md058("<div>\n|a|b|\n|-|-|\n|1|2|\n</div>\n", MarkdownFlavor::MDX);
    assert_eq!(with_prop, without_prop);
    assert_eq!(with_prop.len(), 2, "{with_prop:?}");
}

#[test]
fn a_span_tag_in_a_comment_or_fence_does_not_hide_the_tables_after_it() {
    for (doc, short_row) in [
        ("<!--\n<div markdown=\"span\">\n-->\n\n|a|b|\n|-|-|\n|1|\n", 7),
        ("```html\n<div markdown=\"span\">\n```\n\n|a|b|\n|-|-|\n|1|\n", 7),
        ("    <div markdown=\"span\">\n\n|a|b|\n|-|-|\n|1|\n", 5),
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        let lines: Vec<usize> = MD056TableColumnCount
            .check(&ctx)
            .unwrap()
            .iter()
            .map(|w| w.line)
            .collect();
        assert_eq!(lines, vec![short_row], "{doc:?}");
    }
}

#[test]
fn a_commented_uppercase_span_tag_does_not_hide_a_real_table() {
    for flavor in FLAVORS {
        let doc = "<!--\n<div MARKDOWN=\"span\">\n-->\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![7],
            "{flavor:?}"
        );
    }
}

#[test]
fn a_span_tag_in_front_matter_does_not_hide_a_real_table() {
    for flavor in FLAVORS {
        let doc = "---\nexample: |\n  <div markdown=\"span\">\n---\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![8],
            "{flavor:?}"
        );
    }
}

#[test]
fn an_unrelated_closing_tag_still_needs_a_blank_line_after_a_table() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n\n|a|b|\n|-|-|\n|1|2|\n</section>\n\n</div>\n";
        let after: Vec<String> = md058(doc, flavor)
            .into_iter()
            .filter(|w| w.contains("after table"))
            .collect();
        assert_eq!(after.len(), 1, "{flavor:?} {after:?}");
    }
}

#[test]
fn a_commented_span_tag_after_a_fenced_comment_opener_does_not_hide_a_real_table() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n```html\n<!--\n```\n</div>\n\n<!--\n<div markdown=\"span\">\n-->\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![13],
            "{flavor:?}"
        );
    }
}

#[test]
fn a_fence_marker_in_a_comment_does_not_hide_the_closing_tag() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"span\">\n<!--\n```\n-->\n</div>\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![9],
            "{flavor:?}"
        );
    }
}

#[test]
fn a_span_tag_after_a_multiline_code_span_does_not_hide_a_real_table() {
    for flavor in FLAVORS {
        let doc = "text `code\nmore` <video markdown=\"span\">\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![6],
            "{flavor:?}"
        );
    }
}

#[test]
fn a_span_element_holding_a_backtick_run_still_closes() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"span\">\n```\n</div>\n\n|a|b|\n|-|-|\n|1|\n";
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
            vec![7],
            "{flavor:?}"
        );
    }
}
