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
        assert_reflows_with(input, &ReflowSettings::with_mode(mode, line_length), expected);
    }
}

fn assert_reflows_with(input: &str, settings: &ReflowSettings, expected: &str) {
    let output = reflow(input, settings).expect("reflow runs");
    assert_eq!(output, expected, "{settings:?}, input {input:?}");
    if let Err(violation) = check(input, settings) {
        panic!("{settings:?}: {}", violation.label());
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
    for mode in SENTENCE_MODES {
        assert_reflows_with(
            "本当！！ 次です。\n",
            &ReflowSettings::with_mode(mode, 0),
            "本当！！\n次です。\n",
        );
        assert_reflows_with("本当！！次です。\n", &cjk_join(mode), "本当！！\n次です。\n");
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

// A line break between two CJK characters renders as a space unless
// `cjk-soft-break = "join"` drops it, so a CJK sentence ending is a line break
// site only where the break renders as what it replaces.

const SENTENCE_MODES: [Mode; 2] = [Mode::SentencePerLine, Mode::SemanticLineBreaks];

fn cjk_join(mode: Mode) -> ReflowSettings {
    ReflowSettings {
        cjk_join: true,
        ..ReflowSettings::with_mode(mode, 80)
    }
}

/// Under the default the break would render as a space the source never had.
#[test]
fn unspaced_cjk_sentences_stay_on_one_line_when_a_break_renders_as_a_space() {
    let cjk = "文です。次の文です。\n";
    assert_reflows_to(cjk, 80, &SENTENCE_MODES, cjk);
}

#[test]
fn spaced_cjk_sentences_split_when_a_break_renders_as_a_space() {
    assert_reflows_to(
        "文です。 次の文です。\n",
        80,
        &SENTENCE_MODES,
        "文です。\n次の文です。\n",
    );
}

#[test]
fn unspaced_cjk_sentences_split_when_breaks_are_joined() {
    for mode in SENTENCE_MODES {
        assert_reflows_with("文です。次の文です。\n", &cjk_join(mode), "文です。\n次の文です。\n");
    }
}

/// The joined break would erase the space the author typed.
#[test]
fn spaced_cjk_sentences_stay_on_one_line_when_breaks_are_joined() {
    let cjk = "文です。 次の文です。\n";
    for mode in SENTENCE_MODES {
        assert_reflows_with(cjk, &cjk_join(mode), cjk);
    }
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

// A code span crossing a line break keeps every character of the next line
// past the content column of the container holding it, and the whitespace
// ending the line before the break. Only the container's own indentation is
// markup there, and trailing spaces inside the span are code, not a hard break.

#[test]
fn a_code_span_keeps_indentation_past_a_list_item_content_column() {
    assert_reflows_to(
        "- item with `a code\n    span` tail.\n",
        80,
        &[Mode::Normalize],
        "- item with `a code   span` tail.\n",
    );
    assert_reflows_to(
        "1. item with `a code\n      span` tail.\n",
        80,
        &[Mode::Normalize],
        "1. item with `a code    span` tail.\n",
    );
    assert_reflows_to(
        "- [ ] task `a code\n    span` tail.\n",
        80,
        &[Mode::Normalize],
        "- [ ] task `a code   span` tail.\n",
    );
    assert_reflows_to(
        "- outer\n  - inner `a code\n      span` tail.\n",
        80,
        &[Mode::Normalize],
        "- outer\n  - inner `a code   span` tail.\n",
    );
}

#[test]
fn a_code_span_keeps_indentation_past_a_blockquote_marker() {
    assert_reflows_to(
        "> quoted `a code\n>     span` tail.\n",
        80,
        &[Mode::Normalize],
        "> quoted `a code     span` tail.\n",
    );
    assert_reflows_to(
        "> - item `a code\n>     span` tail.\n",
        80,
        &[Mode::Normalize],
        "> - item `a code   span` tail.\n",
    );
    assert_reflows_to(
        "> - [ ] task `a code\n>     span` tail.\n",
        80,
        &[Mode::Normalize],
        "> - [ ] task `a code   span` tail.\n",
    );
}

#[test]
fn a_code_span_keeps_indentation_past_a_footnote_content_column() {
    assert_reflows_to(
        "Text.[^1]\n\n[^1]: a note that is long enough to wrap `a code\n      span` tail.\n",
        40,
        &[Mode::Normalize],
        "Text.[^1]\n\n[^1]: a note that is long enough to wrap\n    `a code   span` tail.\n",
    );
}

#[test]
fn trailing_spaces_inside_a_code_span_are_code_and_no_hard_break() {
    for (input, expected) in [
        ("- item `a code   \n  span` tail.\n", "- item `a code    span` tail.\n"),
        ("- item `a code \n  span` tail.\n", "- item `a code  span` tail.\n"),
        (
            "> quoted `a code   \n> span` tail.\n",
            "> quoted `a code    span` tail.\n",
        ),
        (
            "> - item `a code   \n>   span` tail.\n",
            "> - item `a code    span` tail.\n",
        ),
        (
            "Text.[^1]\n\n[^1]: a note that is long enough to wrap `a code   \n    span` tail.\n",
            "Text.[^1]\n\n[^1]: a note that is long enough to wrap\n    `a code    span` tail.\n",
        ),
    ] {
        assert_reflows_to(input, 40, &[Mode::Normalize], expected);
    }
}

/// Every mode, width and span setting, with a tab for indentation.
#[test]
fn a_code_span_across_a_container_line_break_renders_unchanged() {
    let inputs = [
        "- item with `a code\n    span` and some more words after the span.\n",
        "1. item with `a code\n      span` and some more words after the span.\n",
        "- outer\n  - inner `a code\n      span` and some more words after it.\n",
        "- item with `a code\n\tspan` and some more words after the span.\n",
        "- item `a code   \n  span` and some more words after the span.\n",
        "> quoted `a code\n>     span` and some more words after the span.\n",
        "> quoted `a code\n    span` and some more words after the span.\n",
        "> quoted `a code   \n> span` and some more words after the span.\n",
        "> - item `a code\n>     span` and some more words after the span.\n",
        "> - item `a code   \n>   span` and some more words after the span.\n",
        "Text.[^1]\n\n[^1]: note `a code\n      span` and some more words after it.\n",
        "Text.[^1]\n\n[^1]: note `a code   \n    span` and some more words after it.\n",
    ];
    for input in inputs {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 40, 80] {
                for atomic_spans in [true, false] {
                    let settings = ReflowSettings {
                        atomic_spans,
                        ..ReflowSettings::with_mode(mode, line_length)
                    };
                    if let Err(violation) = check(input, &settings) {
                        panic!("{settings:?} on {input:?}: {}", violation.label());
                    }
                }
            }
        }
    }
}

/// Reflow re-spaces a list marker only when it rewrites every line the item
/// holds. A nested list and the paragraph after it are left where they are, so
/// narrowing the parent's `*   ` to `* ` would leave the paragraph inside the
/// nested item instead of the parent.
#[test]
fn a_parent_keeps_its_marker_spacing_over_a_nested_list_and_paragraph() {
    assert_reflows_to(
        "*   aaa bbb ccc ddd eee\n\n    *   fff ggg hhh iii jjj\n\n    After fff ggg\n",
        20,
        &[Mode::Normalize],
        "*   aaa bbb ccc ddd\n    eee\n\n    * fff ggg hhh\n      iii jjj\n\n    After fff ggg\n",
    );
}

/// The sibling the reflow leaves alone stays a sibling: narrowing the parent
/// would have put it at the reflowed sibling's content column, nesting it.
#[test]
fn an_unreflowed_nested_sibling_stays_a_sibling() {
    assert_reflows_to(
        "*   aaa bbb ccc ddd eee\n\n    *   fff ggg hhh iii jjj\n    *   kkk\n",
        20,
        &[Mode::Normalize],
        "*   aaa bbb ccc ddd\n    eee\n\n    * fff ggg hhh\n      iii jjj\n    *   kkk\n",
    );
}

/// A lazy continuation line continues the item's paragraph, so the item owns
/// it and everything it holds after it: the whole item is rewritten and
/// re-spaced, and the indented code moves with it.
#[test]
fn a_lazy_line_is_reflowed_with_its_item() {
    assert_reflows_to(
        "*   aaa bbb ccc ddd eee\nlazy fff ggg\n\n        code\n",
        20,
        &[Mode::Normalize],
        "* aaa bbb ccc ddd\n  eee lazy fff ggg\n\n      code\n",
    );
}

/// Outside a paragraph the line continues, an ordered marker numbered other
/// than 1 opens a list. Reflowed on its own at column 0, a lazy line could
/// wrap to start a line with `2)`, which then closed the item and opened a
/// list; reflowed with its item, the line is indented under it.
#[test]
fn a_lazy_line_never_wraps_to_start_with_an_ordered_marker() {
    let input = "- Item text here:\nsome lazy words in Python 2) and more words to wrap here.\n";
    assert_reflows_to(
        input,
        26,
        &[Mode::Normalize],
        "- Item text here: some\n  lazy words in Python 2)\n  and more words to wrap\n  here.\n",
    );
    assert_reflows_to(
        input,
        26,
        &[Mode::SemanticLineBreaks],
        "- Item text here:\n  some lazy words in\n  Python 2) and more words\n  to wrap here.\n",
    );
}

/// A tab-indented line continues the item, but falls short of its content
/// column counted in bytes, so reflow leaves it and what follows to the outer
/// loop. The item still holds the code after it, so it keeps its marker.
#[test]
fn a_parent_keeps_its_marker_spacing_over_code_past_a_tab_indented_line() {
    let input = "*   aaa bbb ccc ddd eee\n\tfff\n\n        code\n";
    assert_reflows_to(
        input,
        20,
        &[Mode::Normalize],
        "*   aaa bbb ccc ddd\n    eee\n\tfff\n\n        code\n",
    );
    for mode in REFLOW_MODES {
        for line_length in [10, 20, 40] {
            let settings = ReflowSettings::with_mode(mode, line_length);
            if let Err(violation) = check(input, &settings) {
                panic!("{settings:?}: {}", violation.label());
            }
        }
    }
}

/// Only a paragraph has lazy continuation lines. An unindented line after a
/// table the item holds ends the item, so it is not reflowed into it.
#[test]
fn a_line_after_an_item_table_is_not_a_lazy_continuation() {
    assert_reflows_to(
        "- item text\n\n  | a | b |\n  | - | - |\n  | 1 | 2 |\nafter the table words here\n",
        20,
        &[Mode::Normalize],
        "- item text\n\n  | a | b |\n  | - | - |\n  | 1 | 2 |\nafter the table\nwords here\n",
    );
}

/// A link reference definition can only open a paragraph, and it ends at its
/// line: text joined onto the destination's line turns the definition into
/// visible text and breaks every link that uses it. Reflow keeps the leading
/// definitions of an item's paragraph as written and reflows what follows.
#[test]
fn reflow_keeps_a_link_reference_definition_opening_an_item_paragraph() {
    assert_reflows_to(
        "- [a]: https://example.com/a\n  one two three four five six seven eight nine ten\n",
        40,
        &[Mode::Normalize],
        "- [a]: https://example.com/a\n  one two three four five six seven\n  eight nine ten\n",
    );
    let prose = "one two three four five six seven eight nine ten eleven twelve thirteen";
    for input in [
        format!("- [a]: https://example.com/a\n  {prose}\n"),
        format!("- [a]: https://example.com/a\n{prose}\n"),
        format!("> - [a]: https://example.com/a\n>   {prose}\n"),
        format!("1. intro\n\n   [a]: https://example.com/a\n   [b]: https://example.com/b \"Bee\"\n   {prose}\n"),
        format!("- [a]: https://example.com/a\n[b]: https://example.com/b\u{4} {prose}\n"),
    ] {
        for mode in REFLOW_MODES {
            for line_length in [20, 40, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(&input, &settings) {
                    panic!("{input:?} {settings:?}: {}", violation.label());
                }
            }
        }
    }
}

/// Padding after a list marker beyond its first space belongs to the item's
/// first line, and a line kept as written still sits on the marker. Re-emitting
/// that padding after a marker that already carries it moved the content
/// column: an item's text five spaces in became indented code, and a re-spaced
/// item left its later paragraphs outside it. The blockquoted rows take the
/// blockquote list path, which builds its own first line, and pin it too.
#[test]
fn reflow_drops_extra_marker_padding_before_a_line_kept_as_written() {
    let prose = "one two three four five six seven eight nine ten eleven twelve thirteen";
    assert_reflows_to(
        &format!("-   [a]: https://example.com/a\n\n    {prose}\n"),
        40,
        &[Mode::Normalize],
        "- [a]: https://example.com/a\n\n  one two three four five six seven\n  eight nine ten eleven twelve thirteen\n",
    );
    for input in [
        format!("-   [a]: https://example.com/a\n\n    {prose}\n"),
        format!("-   [a]:\n\n    {prose}\n"),
        format!("-   [a]: https://example.com/a\n    {prose}\n"),
        format!("1.  [a]: https://example.com/a\n\n    {prose}\n"),
        format!("> -   [a]: https://example.com/a\n>\n>     {prose}\n"),
        format!("> -   [a]:\n>\n>     {prose}\n"),
    ] {
        for mode in REFLOW_MODES {
            for line_length in [20, 40, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(&input, &settings) {
                    panic!("{input:?} {settings:?}: {}", violation.label());
                }
            }
        }
    }
}

/// Control: an item whose lines reflow rewrites in full is re-spaced, and its
/// fenced code moves with it.
#[test]
fn an_item_reflow_rewrites_in_full_is_re_spaced() {
    assert_reflows_to(
        "*   aaa bbb ccc ddd eee\n\n    ```\n    x\n    ```\n",
        20,
        &[Mode::Normalize],
        "* aaa bbb ccc ddd\n  eee\n\n  ```\n  x\n  ```\n",
    );
}

/// A complete tag alone on a paragraph's first line opens an HTML block, so
/// wrapping right after an inline `<img>` turned the image into raw HTML
/// outside any paragraph. The line after the tag stays on the tag's line.
#[test]
fn a_wrapped_line_never_leaves_an_inline_tag_alone_on_the_first_line() {
    assert_reflows_to(
        "<img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it here\n",
        30,
        &[Mode::Normalize],
        "<img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it here\n",
    );
    assert_reflows_to(
        "- <img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it\n",
        30,
        &[Mode::Normalize],
        "- <img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it\n",
    );
    assert_reflows_to(
        "<img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it here that keeps going on for a while longer\n",
        30,
        &[Mode::Normalize],
        "<img alt=\"a b c d e f g h i j k\" src=\"x.png\" /> and more text after it here\nthat keeps going on for a\nwhile longer\n",
    );
}

/// A tag the source wrapped between its attributes is a paragraph holding an
/// image. Joining it into one line would make it an HTML block, so it stays
/// broken, between attributes.
#[test]
fn a_paragraph_that_is_one_wrapped_tag_stays_wrapped() {
    assert_reflows_to(
        "<img alt=\"a b\nc\" src=\"x\" />\n",
        30,
        &[Mode::Normalize],
        "<img alt=\"a b c\" src=\"x\"\n/>\n",
    );
}
