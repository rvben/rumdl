//! A tag written in code or in a comment opens no `markdown` element, so the
//! lines after it are ordinary document lines in every flavor.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

const FLAVORS: [MarkdownFlavor; 2] = [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs];

/// 1-indexed lines the document reads as inside a `markdown` element.
fn container_lines(doc: &str, flavor: MarkdownFlavor) -> Vec<usize> {
    let ctx = LintContext::new(doc, flavor, None);
    (1..=doc.lines().count())
        .filter(|&n| ctx.line_info(n).is_some_and(|l| l.in_mkdocs_html_markdown))
        .collect()
}

#[test]
fn a_tag_in_an_html_comment_opens_nothing() {
    for flavor in FLAVORS {
        for value in ["1", "span"] {
            let doc = format!("<!--\n<div markdown=\"{value}\">\n-->\n\nafter\n");
            assert_eq!(container_lines(&doc, flavor), Vec::<usize>::new(), "{flavor:?} {value}");
        }
    }
}

#[test]
fn a_tag_in_an_indented_code_block_opens_nothing() {
    for flavor in FLAVORS {
        let doc = "para\n\n    <div markdown=\"1\">\n\nafter\n";
        assert_eq!(container_lines(doc, flavor), Vec::<usize>::new(), "{flavor:?}");
    }
}

#[test]
fn a_real_tag_next_to_a_comment_still_opens_its_element() {
    // Positive control: the same shapes without the code context do open one.
    for flavor in FLAVORS {
        let doc = "<!-- note -->\n<div markdown=\"1\">\ninside\n</div>\n\nafter\n";
        assert_eq!(container_lines(doc, flavor), vec![2, 3, 4], "{flavor:?}");
    }
}

#[test]
fn a_nested_tag_indented_inside_an_open_element_is_still_a_tag() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n\n    <div markdown=\"1\">\n    inner\n    </div>\n\n</div>\n\nafter\n";
        let lines = container_lines(doc, flavor);
        assert!(
            lines.contains(&3) && lines.contains(&4) && !lines.contains(&9),
            "{flavor:?} {lines:?}"
        );
    }
}

#[test]
fn a_closing_tag_after_a_comment_on_the_same_line_still_closes_the_element() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n<!-- note --> </div>\n\nafter\n";
        assert_eq!(container_lines(doc, flavor), vec![1, 2], "{flavor:?}");
    }
}

#[test]
fn a_tag_in_a_code_span_that_starts_the_line_opens_nothing() {
    // An inline element can start a line inside a paragraph, and a code span
    // can run across the line break in front of it.
    for flavor in FLAVORS {
        let doc = "text `\n<video markdown=\"1\">\n` more\n\nafter\n";
        assert_eq!(container_lines(doc, flavor), Vec::<usize>::new(), "{flavor:?}");
    }
}

#[test]
fn a_comment_indented_inside_an_element_hides_its_tags() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n\n    <!--\n    <div markdown=\"span\">\n    -->\n\n</div>\n\nafter\n";
        let ctx = LintContext::new(doc, flavor, None);
        assert!(
            (1..=doc.lines().count()).all(|n| !ctx.line_info(n).unwrap().in_markdown_span),
            "{flavor:?}"
        );
    }
}

#[test]
fn a_comment_opener_in_fenced_code_inside_an_element_hides_nothing() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n\n```html\n<!--\n```\n\n</div>\n\nafter\n\n<!-- end -->\n";
        let lines = container_lines(doc, flavor);
        assert!(
            lines.contains(&5) && lines.contains(&7) && !lines.contains(&9),
            "{flavor:?} {lines:?}"
        );
    }
}

#[test]
fn the_markdown_attribute_is_matched_in_any_letter_case() {
    for flavor in FLAVORS {
        for attr in ["MARKDOWN", "Markdown"] {
            let commented = format!("<!--\n<div {attr}=\"1\">\n-->\n\nafter\n");
            assert_eq!(
                container_lines(&commented, flavor),
                Vec::<usize>::new(),
                "{flavor:?} {attr}"
            );
            let real = format!("<div {attr}=\"1\">\n\ninner\n\n</div>\n");
            assert!(!container_lines(&real, flavor).is_empty(), "{flavor:?} {attr} real tag");
        }
    }
}

#[test]
fn a_comment_opener_in_a_fence_the_parser_did_not_see_hides_nothing() {
    for flavor in FLAVORS {
        // The fence follows the tag with no blank line, so the parser reads raw HTML.
        let doc = "<div markdown=\"1\">\n```html\n<!--\n```\n</div>\n\nafter\n\n<!-- later -->\n";
        let lines = container_lines(doc, flavor);
        assert!(!lines.contains(&7), "{flavor:?} {lines:?}");
    }
}

#[test]
fn a_comment_closed_by_the_dashes_of_its_own_opener_hides_nothing_after_it() {
    for flavor in FLAVORS {
        for opener in ["<!-->", "<!--->"] {
            let doc = format!("<div markdown=\"1\">\n\n    {opener}\n</div>\n\nafter\n\n<!-- later -->\n");
            let lines = container_lines(&doc, flavor);
            assert!(!lines.contains(&6), "{flavor:?} {opener} {lines:?}");
        }
    }
}

#[test]
fn a_span_tag_in_a_fence_the_parser_did_not_see_opens_nothing() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n```html\n<section markdown=\"span\">\n```\n</div>\n\nafter\n";
        let ctx = LintContext::new(doc, flavor, None);
        assert!(
            (1..=doc.lines().count()).all(|n| !ctx.line_info(n).unwrap().in_markdown_span),
            "{flavor:?}"
        );
    }
}

#[test]
fn a_tag_in_front_matter_opens_nothing() {
    for flavor in FLAVORS {
        for value in ["1", "span"] {
            let doc = format!("---\nexample: |\n  <div markdown=\"{value}\">\n---\n\nafter\n");
            assert_eq!(container_lines(&doc, flavor), Vec::<usize>::new(), "{flavor:?} {value}");
        }
    }
}

#[test]
fn a_fence_marker_in_a_comment_inside_an_element_starts_no_fence() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n<!--\n```\n-->\n</div>\n\nafter\n\n<!-- later -->\n";
        let lines = container_lines(doc, flavor);
        assert!(lines.contains(&4) && !lines.contains(&7), "{flavor:?} {lines:?}");
    }
}

#[test]
fn a_real_comment_after_a_fence_comment_opener_still_hides_its_tags() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n```html\n<!--\n```\n</div>\n\n<!--\n<div markdown=\"span\">\n-->\n\nafter\n";
        let ctx = LintContext::new(doc, flavor, None);
        assert!(
            (1..=doc.lines().count()).all(|n| !ctx.line_info(n).unwrap().in_markdown_span),
            "{flavor:?}"
        );
    }
}

#[test]
fn a_tag_after_the_end_of_a_multiline_code_span_is_inline_html() {
    for flavor in FLAVORS {
        let doc = "text `code\nmore` <video markdown=\"span\">\n\nafter\n";
        assert_eq!(container_lines(doc, flavor), Vec::<usize>::new(), "{flavor:?}");
    }
}

#[test]
fn a_closing_tag_after_a_multiline_code_span_still_closes_the_element() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"1\">\n\n`one\ntwo`</div>\n\nafter\n";
        let lines = container_lines(doc, flavor);
        assert!(lines.contains(&4) && !lines.contains(&6), "{flavor:?} {lines:?}");
    }
}

#[test]
fn a_backtick_run_in_span_content_is_not_a_fence() {
    for flavor in FLAVORS {
        let doc = "<div markdown=\"span\">\n```\n</div>\n\nafter\n\n<!-- end -->\n";
        let lines = container_lines(doc, flavor);
        assert!(!lines.contains(&5), "{flavor:?} {lines:?}");
    }
}
