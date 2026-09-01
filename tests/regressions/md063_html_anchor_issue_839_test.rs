//! Regression tests for issue #839: MD063 applied heading capitalization to
//! raw HTML embedded in a heading, changing attribute names and values and
//! thereby breaking stable anchors.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Run the public formatter with only MD063 enabled and return the rewritten
/// document. The test deliberately exercises the CLI rather than MD063's
/// segmentation helpers: users care that bytes inside an HTML tag survive the
/// complete formatting pipeline.
fn format_heading(content: &str) -> String {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("heading.md");
    fs::write(&path, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-config", "--no-cache", "--enable", "MD063"])
        .arg(&path)
        .output()
        .expect("rumdl fmt should run");

    assert!(
        output.status.success(),
        "rumdl fmt failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(path).unwrap()
}

#[test]
fn title_case_preserves_an_html_anchor_byte_for_byte() {
    let input = "## Goals and scope<a id=\"goals-and-scope\"></a>\n";
    let expected = "## Goals and Scope<a id=\"goals-and-scope\"></a>\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn html_tag_matching_is_case_insensitive_without_normalizing_source_bytes() {
    let input = "## Goals and scope<A id=\"stable-anchor\"></A>\n";
    let expected = "## Goals and Scope<A id=\"stable-anchor\"></A>\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn an_already_correct_heading_does_not_produce_an_anchor_only_rewrite() {
    let input = "### Trademarks<a id=\"trademarks\" data-owner=\"docs-team\"></a>\n";

    assert_eq!(format_heading(input), input);
}

#[test]
fn every_html_tag_keeps_its_attribute_bytes() {
    // The rule recases prose, and the bytes inside a tag are never prose, whatever
    // the element is called.
    for (input, expected) in [
        (
            "## goals and scope <img src=\"goals-and-scope.png\" alt=\"goals and scope\">\n",
            "## Goals and Scope <img src=\"goals-and-scope.png\" alt=\"goals and scope\">\n",
        ),
        (
            "## the <del>old-name-here</del> is gone\n",
            "## The <del>old-name-here</del> Is Gone\n",
        ),
        (
            "## released <time datetime=\"2024-01-01\">last year</time>\n",
            "## Released <time datetime=\"2024-01-01\">last year</time>\n",
        ),
        (
            "## see <img alt=\"a > b\" src=\"compare.png\"> now\n",
            "## See <img alt=\"a > b\" src=\"compare.png\"> Now\n",
        ),
    ] {
        assert_eq!(format_heading(input), expected, "input: {input}");
    }
}

#[test]
fn a_void_element_does_not_swallow_the_prose_after_it() {
    let input = "## line one<br>line two here and <b>bold</b>\n";
    let expected = "## Line One<br>Line Two Here and <b>bold</b>\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn nested_elements_are_paired_by_name() {
    let input = "## <b>bold <i>inner</i> more</b> stuff\n";
    let expected = "## <b>bold <i>inner</i> more</b> Stuff\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_self_closing_tag_does_not_open_an_element() {
    // `<span/>` holds no content, so the closing tag pairs with the outer `<span>`
    // and every word between the two stays verbatim.
    let input = "## <span>old words <span/> more words</span> tail\n";
    let expected = "## <span>old words <span/> more words</span> Tail\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_void_element_never_pairs_with_a_closing_tag() {
    // `<br>` has no content, so a stray `</br>` closes nothing and the words
    // between the two are prose.
    let input = "## one<br>two words</br> tail\n";
    let expected = "## One<br>Two Words</br> Tail\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_tag_inside_a_code_span_is_code_not_markup() {
    // Were the `<b>` in the code span read as an open tag, the `</b>` after the
    // keyboard element would close it and the element in between would be lost
    // together with its protection.
    let input = "## use `<b>` and <kbd>ctrl</kbd> then </b> ends\n";
    let expected = "## Use `<b>` and <kbd>ctrl</kbd> Then </b> Ends\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn an_html_comment_is_preserved_verbatim() {
    let input = "## title <!-- keep: this text --> here\n";
    let expected = "## Title <!-- keep: this text --> Here\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_trailing_empty_anchor_does_not_move_the_last_word() {
    // The anchor renders nothing, so `to` is still the heading's last word.
    let input = "## where to<a id=\"where-to\"></a>\n";
    let expected = "## Where To<a id=\"where-to\"></a>\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_trailing_comment_does_not_move_the_last_word() {
    let input = "## where to <!-- anchor -->\n";
    let expected = "## Where To <!-- anchor -->\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn an_element_with_visible_text_is_the_last_word() {
    // `<kbd>ctrl</kbd>` renders text, so `to` is not the last word and stays lowercase.
    let input = "## go to <kbd>ctrl</kbd>\n";
    let expected = "## Go to <kbd>ctrl</kbd>\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_visible_element_without_text_is_the_last_word() {
    // `<img>` paints something even though it holds no text, so like the Markdown
    // image it is the heading's last element and `to` stays lowercase.
    let html = "## go to <img src=\"icon.png\" alt=\"settings\">\n";
    let markdown = "## go to ![settings](icon.png)\n";

    assert_eq!(
        format_heading(html),
        "## Go to <img src=\"icon.png\" alt=\"settings\">\n"
    );
    assert_eq!(format_heading(markdown), "## Go to ![settings](icon.png)\n");
}

#[test]
fn a_backslash_escaped_tag_is_heading_text() {
    // `\<time>` is a literal `<time>`, so nothing pairs it with the later
    // `\</time>` and the words between are capitalized like any other prose.
    let input = "## before \\<time> old words \\</time> after\n";
    let expected = "## Before \\<Time> Old Words \\</Time> After\n";

    assert_eq!(format_heading(input), expected);
}

#[test]
fn a_degenerate_comment_is_complete() {
    // `<!-->` and `<!--->` are whole comments, so the words after them are prose
    // and the later `-->` is text.
    assert_eq!(
        format_heading("# before <!--> old words --> after\n"),
        "# Before <!--> Old Words --> After\n"
    );
    assert_eq!(
        format_heading("# before <!---> old words --> after\n"),
        "# Before <!---> Old Words --> After\n"
    );
    assert_eq!(
        format_heading("# before <!-- real comment --> after words\n"),
        "# Before <!-- real comment --> After Words\n"
    );
}

#[test]
fn a_param_element_is_void() {
    // `<param>` is complete in its start tag, so `</param>` closes nothing and
    // the words between are prose.
    assert_eq!(
        format_heading("# before <param> old words </param> after\n"),
        "# Before <param> Old Words </param> After\n"
    );
}
