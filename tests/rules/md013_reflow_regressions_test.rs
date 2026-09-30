//! MD013 reflow defects found by the reflow semantics oracle
//! (`fuzz/oracle/reflow_semantics.rs`).
//!
//! Each test pins the exact output of the production fix path and also runs
//! the oracle over the input, so a regression fails with the rendered
//! difference as well as the text one.

use super::reflow_semantics::{Mode, Outcome, ReflowSettings, check, reflow};

const REFLOW_MODES: [Mode; 3] = [Mode::Normalize, Mode::SentencePerLine, Mode::SemanticLineBreaks];

fn assert_reflows_to(input: &str, line_length: u64, modes: &[Mode], expected: &str) {
    for &mode in modes {
        let settings = ReflowSettings::with_mode(mode, line_length);
        let output = reflow(input, &settings).expect("reflow runs");
        assert_eq!(
            output, expected,
            "{mode:?} at line length {line_length}, input {input:?}"
        );
        if let Err(violation) = check(input, &settings) {
            panic!("{mode:?}: {}", violation.label());
        }
    }
}

/// A list item whose marker line holds nothing is empty unless the next line
/// is indented to its content column. An unindented line after it starts a
/// paragraph outside the list, and joining it onto the marker moved it into
/// the list.
#[test]
fn an_empty_list_item_keeps_the_paragraph_after_it_out_of_the_list() {
    for input in [
        "Tasks:\n\n-\nFollow-up notes go here.\n",
        "Tasks:\n\n*\nFollow-up notes go here.\n",
        "Tasks:\n\n1.\nFollow-up notes go here.\n",
        "Tasks:\n\n3)\nFollow-up notes go here.\n",
        "Tasks:\n\n-\t\nFollow-up notes go here.\n",
        "Tasks:\n\n- \nFollow-up notes go here.\n",
        "- first\n-\nFollow-up notes go here.\n",
        "- first\n\n  -\n  Follow-up notes go here.\n",
    ] {
        assert_reflows_to(input, 80, &REFLOW_MODES, input);
    }
}

/// The paragraph after an empty item is still reflowed, as a paragraph of
/// its own.
#[test]
fn the_paragraph_after_an_empty_list_item_is_still_reflowed() {
    assert_reflows_to(
        "-\nOne two three four five six seven eight.\n",
        20,
        &[Mode::Normalize],
        "-\nOne two three four\nfive six seven\neight.\n",
    );
}

/// Content indented to the content column belongs to the item and stays in
/// it.
#[test]
fn content_under_an_empty_marker_line_stays_in_the_item() {
    for input in ["-\n  one\n  two\n", "1.\n   one\n   two\n"] {
        for &mode in &REFLOW_MODES {
            let settings = ReflowSettings::with_mode(mode, 80);
            match check(input, &settings) {
                Ok(Outcome::Unchanged | Outcome::Rewritten) => {}
                Err(violation) => panic!("{mode:?} on {input:?}: {}", violation.label()),
            }
        }
    }
}

// CommonMark strips only spaces and tabs at the edges of a line, and HTML
// collapses only spaces, tabs and line endings. Every other whitespace
// character (an ideographic space, an em space, a vertical tab) is text a
// renderer shows, so reflow must keep it and must never break a line there.

#[test]
fn leading_ideographic_space_on_a_paragraph_is_kept() {
    assert_reflows_to(
        "\u{3000}First line of text\nsecond line.\n",
        12,
        &[Mode::Normalize],
        "\u{3000}First line\nof text\nsecond line.\n",
    );
}

/// Inside a line it binds the words on either side like a letter would.
#[test]
fn ideographic_space_between_words_is_not_a_break_opportunity() {
    assert_reflows_to(
        "Alpha\u{3000}beta gamma delta epsilon zeta.\n",
        12,
        &[Mode::Normalize],
        "Alpha\u{3000}beta\ngamma delta\nepsilon\nzeta.\n",
    );
}

/// Glued to a CJK sentence end it separates the sentences, and a break beside
/// it would render as a second space, so the sentences stay joined.
#[test]
fn ideographic_space_after_a_cjk_sentence_end_keeps_the_sentences_joined() {
    let cjk = "文です。\u{3000}次の文です。\n";
    assert_reflows_to(cjk, 12, &[Mode::SentencePerLine], cjk);
    let settings = ReflowSettings {
        cjk_join: true,
        ..ReflowSettings::with_mode(Mode::SentencePerLine, 12)
    };
    if let Err(violation) = check(cjk, &settings) {
        panic!("cjk-soft-break = join: {}", violation.label());
    }
}

/// With breakable whitespace after the ender, the sentences split there and
/// the ideographic space opens the next line.
#[test]
fn ideographic_space_after_a_spaced_cjk_sentence_end_opens_the_next_line() {
    assert_reflows_to(
        "文です。 \u{3000}次の文です。\n",
        12,
        &[Mode::SentencePerLine],
        "文です。\n\u{3000}次の文です。\n",
    );
}

#[test]
fn ideographic_space_opening_a_list_item_is_kept() {
    assert_reflows_to(
        "- \u{3000}item text\n  more.\n",
        12,
        &[Mode::SemanticLineBreaks],
        "- \u{3000}item\n  text more.\n",
    );
}

#[test]
fn ideographic_space_opening_a_blockquote_is_kept() {
    let quoted = "> \u{3000}quoted\n> more.\n";
    assert_reflows_to(quoted, 12, &[Mode::Normalize], quoted);
}

#[test]
fn leading_vertical_tab_is_kept() {
    let vertical_tab = "\u{b}First line\nsecond.\n";
    assert_reflows_to(vertical_tab, 12, &REFLOW_MODES, vertical_tab);
}

/// An ideographic space inside an MkDocs admonition, whose body is reflowed by a path of
/// its own.
#[test]
fn ideographic_space_opening_an_admonition_body_is_kept() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(12),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    });
    let content = "!!! note\n\n    \u{3000}body text here\n    more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "!!! note\n\n    \u{3000}body\n    text\n    here\n    more.\n"
    );
}

/// A line's indentation is its leading spaces and tabs; an ideographic space
/// is the line's first character of content.
#[test]
fn line_indent_counts_only_spaces_and_tabs() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;

    let ctx = LintContext::new("\u{3000}text\n \t\u{3000}text\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.lines[0].indent, 0);
    assert_eq!(ctx.lines[1].indent, 2);
}

/// A `markdown` attribute written inside fenced code is code. Taking it for a
/// real `<div markdown>` made the fence's lines container content, and reflow
/// rewrapped the code.
#[test]
fn a_markdown_attribute_inside_fenced_code_does_not_open_a_container() {
    let input = "```text\n<div markdown>\n\nOne two three four five six seven eight nine ten.\n\n</div>\n```\n";
    assert_reflows_to(input, 20, &REFLOW_MODES, input);
}

#[test]
fn fenced_code_lines_are_code_whatever_tags_they_hold() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;

    let content = "```text\n<div markdown>\n\nOne two.\n\n</div>\n```\n\nAfter.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    for (i, line) in ctx.lines.iter().enumerate().take(7) {
        assert!(line.in_code_block, "line {i} is code");
        assert!(!line.in_mkdocs_html_markdown, "line {i} is outside any container");
    }
    assert!(!ctx.lines[8].in_mkdocs_html_markdown);
}

/// A closing tag inside a fence in a real body leaves the body open.
#[test]
fn a_closing_tag_inside_fenced_code_does_not_close_the_body() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;

    let content = "<div markdown>\n\n```text\n</div>\n```\n\nStill inside.\n\n</div>\n\nOutside.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(ctx.lines[3].in_code_block);
    assert!(
        ctx.lines[6].in_mkdocs_html_markdown,
        "the paragraph after the fence is in the body"
    );
    assert!(
        !ctx.lines[10].in_mkdocs_html_markdown,
        "the real closing tag ends the body"
    );
}

/// An inline HTML comment is one unit: a break inside it rewrites what the
/// comment holds, which breaks tools that match marker comments exactly.
#[test]
fn an_inline_html_comment_is_never_broken() {
    assert_reflows_to(
        "Some words here <!-- MARKER_NAME -->35<!-- /MARKER_NAME --> more words here.\n",
        30,
        &[Mode::Normalize, Mode::SemanticLineBreaks],
        "Some words\nhere <!-- MARKER_NAME -->35<!-- /MARKER_NAME -->\nmore words here.\n",
    );
    // The empty forms end at their own `>`.
    assert_reflows_to(
        "Odd <!--> and <!---> forms then words words words.\n",
        30,
        &[Mode::Normalize],
        "Odd <!--> and <!---> forms\nthen words words words.\n",
    );
}

/// A line starting with `<!--` (or `<?`, `<!X`, `<![CDATA[`) opens an HTML
/// block, which interrupts a paragraph, so reflow must not move one there.
#[test]
fn an_html_block_opener_is_not_moved_to_the_start_of_a_line() {
    for input in [
        "First one. <!-- A note. Then more. --> Second one here.\n",
        "Open <!-- never closed words words words words.\n",
        "Some words here now <?php echo 1; ?> more words here.\n",
        "Some words here now <!DOCTYPE html> more words here.\n",
        "Some words here now <![CDATA[ x ]]> more words here.\n",
    ] {
        for &mode in &REFLOW_MODES {
            for line_length in [10, 20, 30] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                let output = reflow(input, &settings).expect("reflow runs");
                for line in output.lines().skip(1) {
                    assert!(
                        !line.starts_with('<'),
                        "{mode:?} at {line_length} put {line:?} at a line start in {output:?}"
                    );
                }
                if let Err(violation) = check(input, &settings) {
                    panic!("{mode:?} at {line_length} on {input:?}: {}", violation.label());
                }
            }
        }
    }
}

/// Sentence punctuation with an element glued to it (`Done.`x``) ends no
/// sentence: a break needs whitespace to replace, and one inserted between
/// the two renders as a space.
#[test]
fn an_element_glued_to_sentence_punctuation_stays_on_its_line() {
    let modes = [Mode::SentencePerLine, Mode::SemanticLineBreaks];
    assert_reflows_to(
        "Done.`x` then more. Next one here.\n",
        0,
        &modes,
        "Done.`x` then more.\nNext one here.\n",
    );
    assert_reflows_to(
        "See this.**Bold** follows. Next.\n",
        0,
        &modes,
        "See this.**Bold** follows.\nNext.\n",
    );
    assert_reflows_to(
        "It works!`cargo test` passes and more words keep coming along here until we are well past eighty.\n",
        80,
        &[Mode::SemanticLineBreaks],
        "It works!`cargo test` passes and more words keep coming along here\nuntil we are well past eighty.\n",
    );
}

/// The control: with a space after the punctuation the element opens the
/// next sentence, and the space is where the line breaks.
#[test]
fn an_element_after_spaced_sentence_punctuation_opens_the_next_line() {
    assert_reflows_to(
        "Done. `x` then more. Next one.\n",
        0,
        &[Mode::SentencePerLine, Mode::SemanticLineBreaks],
        "Done.\n`x` then more.\nNext one.\n",
    );
}

/// Punctuation that ends or continues a clause opens no sentence, so a CJK
/// ender followed by one is no boundary. The last ender in a run is one, and
/// breaks at the space after its closer.
#[test]
fn a_cjk_sentence_end_followed_by_punctuation_is_no_boundary() {
    let modes = [Mode::SentencePerLine, Mode::SemanticLineBreaks];
    for (input, expected) in [
        (
            "Marks (。, ！, ？) listed. Next.\n",
            "Marks (。, ！, ？)\nlisted.\nNext.\n",
        ),
        ("項目。、次です。\n", "項目。、次です。\n"),
        ("A (。; ！) b.\n", "A (。; ！)\nb.\n"),
    ] {
        assert_reflows_to(input, 0, &modes, expected);
    }
}

/// A doubled ender is one sentence end: the break goes after the last one.
#[test]
fn a_doubled_cjk_ender_is_one_sentence_end() {
    let input = "本当！！次です。\n";
    for mode in [Mode::SentencePerLine, Mode::SemanticLineBreaks] {
        let output = reflow(input, &ReflowSettings::with_mode(mode, 0)).expect("reflow runs");
        assert_eq!(output, "本当！！\n次です。\n", "{mode:?}");
    }
}

/// Outside a code span the spaces and tabs starting a continuation line
/// render as nothing, so joining the line does not carry them into the
/// middle of the joined one.
#[test]
fn joining_a_line_drops_its_indentation() {
    assert_reflows_to("Word here\n    more.\n", 80, &REFLOW_MODES, "Word here more.\n");
    assert_reflows_to("> Word here\n>     more.\n", 80, &REFLOW_MODES, "> Word here more.\n");
}

/// Inside a code span every character of a continuation line is code, its
/// indentation included, and the line break itself shows as one space.
#[test]
fn joining_a_line_keeps_its_indentation_inside_a_code_span() {
    assert_reflows_to(
        "Run `cargo\n     test` before pushing.\n",
        80,
        &REFLOW_MODES,
        "Run `cargo      test` before pushing.\n",
    );
    assert_reflows_to("Run `a \n b    c` now.\n", 80, &REFLOW_MODES, "Run `a   b    c` now.\n");
}

/// A marker line (`NOTE:`, `WARNING:`, ...) inside a list item keeps its own
/// line when the item is reflowed, but with no blank line around it in the
/// source it continues the item's paragraph. A blank line written before or
/// after it would split that paragraph in two.
#[test]
fn a_marker_line_in_a_list_item_stays_in_its_paragraph() {
    let input = "- Install the package that provides the command line tool\n  NOTE: use version 2.0\n  or higher.\n";
    assert_reflows_to(
        input,
        40,
        &[Mode::Default, Mode::Normalize],
        "- Install the package that provides the\n  command line tool\n  NOTE: use version 2.0\n  or higher.\n",
    );
    assert_reflows_to(
        input,
        40,
        &[Mode::SemanticLineBreaks],
        "- Install the package\n  that provides the command line tool\n  NOTE: use version 2.0\n  or higher.\n",
    );
}

/// Wrapping can move a marker to the start of a line; the next pass must
/// read that line as the same paragraph, not as a new one.
#[test]
fn a_marker_moved_to_a_line_start_by_wrapping_stays_in_its_paragraph() {
    assert_reflows_to(
        "- Configure the server first. NOTE: the port must be free before you start it.\n",
        30,
        &[Mode::Default, Mode::Normalize, Mode::SemanticLineBreaks],
        "- Configure the server first.\n  NOTE: the port must be free\n  before you start it.\n",
    );
}

/// Blank lines the source puts around a marker line stay where they are.
#[test]
fn a_marker_line_keeps_the_blank_lines_around_it() {
    assert_reflows_to(
        "- Install the package that provides the command line tool\n\n  NOTE: use version 2.0.\n\n  More text here.\n",
        40,
        &[Mode::Default, Mode::Normalize],
        "- Install the package that provides the\n  command line tool\n\n  NOTE: use version 2.0.\n\n  More text here.\n",
    );
}

/// A backslash before a line ending is a hard line break, so wrapping must not
/// leave a literal `\` from the prose at the end of a line.
#[test]
fn wrapping_never_ends_a_line_in_a_backslash() {
    assert_reflows_to(
        "Press the backslash key \\ to escape the next character in the shell prompt.\n",
        25,
        &[Mode::Default, Mode::Normalize],
        "Press the backslash key\n\\ to escape the next\ncharacter in the shell\nprompt.\n",
    );
}

/// An HTML block can interrupt a paragraph, so a list item's paragraph and the
/// block right after it are two children with no blank line between them.
/// Writing one there makes the list loose, which wraps the paragraph in `<p>`.
#[test]
fn an_html_block_after_a_list_paragraph_keeps_its_spacing() {
    assert_reflows_to(
        "- Item text here that is long enough to need wrapping at a width\n  <details>\n  <summary>More</summary>\n  </details>\n",
        30,
        &[Mode::Default, Mode::Normalize],
        "- Item text here that is long\n  enough to need wrapping at a\n  width\n  <details>\n  <summary>More</summary>\n  </details>\n",
    );
    assert_reflows_to(
        "- Item text here that is long enough to need wrapping at a width\n\n  <details>\n  <summary>More</summary>\n  </details>\n",
        30,
        &[Mode::Default, Mode::Normalize],
        "- Item text here that is long\n  enough to need wrapping at a\n  width\n\n  <details>\n  <summary>More</summary>\n  </details>\n",
    );
}

/// Every CommonMark type-6 tag interrupts a paragraph, `<option>` and
/// `<title>` included, so wrapping must not move one to the start of a line.
#[test]
fn wrapping_never_starts_a_line_with_a_block_level_tag() {
    assert_reflows_to(
        "Choose a value from the list <option value=\"a\">A</option> here.\n",
        30,
        &[Mode::Default, Mode::Normalize],
        "Choose a value from the\nlist <option value=\"a\">A</option>\nhere.\n",
    );
}

// GFM ends a table only at a blank line or where another block starts, so a
// line after a table's rows is one more row, with or without pipes.

#[test]
fn a_line_after_a_table_is_a_row_and_is_not_reflowed() {
    for input in [
        "| a | b |\n|---|---|\n| 1 | 2 |\nplain text line that is long\n",
        "> | a | b |\n> |---|---|\n> | 1 | 2 |\n> plain text line that is long\n",
        "- item\n\n  | a | b |\n  |---|---|\n  | 1 | 2 |\n  plain text line that is long\n",
    ] {
        assert_reflows_to(input, 20, &REFLOW_MODES, input);
    }
}
