//! MD013 reflow defects found by the reflow semantics oracle
//! (`fuzz/oracle/reflow_semantics.rs`).
//!
//! Each test pins the exact output of the production fix path and also runs
//! the oracle over the input, so a regression fails with the rendered
//! difference as well as the text one.

use super::reflow_semantics::{Mode, Outcome, ReflowSettings, check, reflow};

const REFLOW_MODES: [Mode; 4] = [
    Mode::Normalize,
    Mode::SentencePerLine,
    Mode::SentencePack,
    Mode::SemanticLineBreaks,
];

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
    assert_reflows_to(
        vertical_tab,
        12,
        &[Mode::Normalize, Mode::SentencePerLine, Mode::SemanticLineBreaks],
        vertical_tab,
    );
    assert_reflows_to(vertical_tab, 12, &[Mode::SentencePack], "\u{b}First line second.\n");
}

/// An MkDocs admonition body is reflowed by a path of its own, which leaves a
/// code span running into indentation as written too.
#[test]
fn a_code_span_running_into_indentation_in_an_admonition_is_left_as_written() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    });
    let fix = |content: &str| {
        rule.fix(&LintContext::new(content, MarkdownFlavor::MkDocs, None))
            .unwrap()
    };
    let indented = "!!! note\n\n    Run `cargo\n      test` before pushing.\n";
    assert_eq!(fix(indented), indented);
    assert_eq!(
        fix("!!! note\n\n    Run `cargo\n    test` before pushing.\n"),
        "!!! note\n\n    Run `cargo test` before pushing.\n"
    );
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

/// Python-Markdown expands a tab in an admonition's indentation to the next
/// multiple of four columns, so a body indented with tabs is re-indented with
/// the spaces that reach the same column, and wraps within what is left of
/// the line after it.
#[test]
fn a_tab_indented_admonition_body_stays_in_the_admonition() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
    use rumdl_lib::types::LineLength;

    let fix = |content: &str, line_length: usize| {
        let rule = MD013LineLength::from_config_struct(MD013Config {
            line_length: LineLength::new(line_length),
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        });
        rule.fix(&LintContext::new(content, MarkdownFlavor::MkDocs, None))
            .unwrap()
    };
    let cases = [
        (
            "!!! note\n\tbody text more words\n",
            13,
            "!!! note\n    body text\n    more\n    words\n",
        ),
        (
            "!!! note\n\tbody text\n    more words\n",
            13,
            "!!! note\n    body text\n    more\n    words\n",
        ),
        (
            "!!! note\n\t!!! tip\n\t\tbody text more words\n",
            17,
            "!!! note\n\t!!! tip\n        body text\n        more\n        words\n",
        ),
        (
            "- a\n\n  \t!!! note\n  \t    body text more words\n",
            17,
            "- a\n\n  \t!!! note\n        body text\n        more\n        words\n",
        ),
        (
            "- a\n\n  \t!!! note\n  \t\tbody text more words\n",
            17,
            "- a\n\n  \t!!! note\n        body text\n        more\n        words\n",
        ),
    ];
    for (input, line_length, expected) in cases {
        assert_eq!(fix(input, line_length), expected, "input {input:?} at {line_length}");
    }
    // The spaces past the tab sit inside the code span, so the paragraph is
    // left as written.
    let code_span = "!!! note\n\tRun `cargo\n\t  test` before pushing.\n";
    assert_eq!(fix(code_span, 80), code_span);
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

/// A code span crossing into an indented line holds that indentation as code
/// for markdown-rs and pulldown-cmark, while cmark, commonmark.js and comrak
/// (cmark-gfm, as GitHub renders) strip it, so no join renders the same
/// everywhere and the paragraph is left as written. With no indentation the
/// line break is one space in the code for all of them, and the line joins.
#[test]
fn a_code_span_running_into_indentation_leaves_the_paragraph_as_written() {
    for input in [
        "Run `cargo\n     test` before pushing.\n",
        "Run `cargo\n\ttest` before pushing.\n",
        "Run `a \n b    c` now.\n",
    ] {
        assert_reflows_to(input, 10, &REFLOW_MODES, input);
    }
    assert_reflows_to(
        "Run `cargo\ntest` before pushing.\n",
        80,
        &REFLOW_MODES,
        "Run `cargo test` before pushing.\n",
    );
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

/// Each shape twice: with whitespace past the content column of the container
/// holding the span, which renderers disagree on, and with none, which every
/// renderer reads as one space in the code.
const SPANS_INTO_CONTAINER_INDENTATION: [(&str, &str, &str); 8] = [
    (
        "- item with `a code\n    span` tail.\n",
        "- item with `a code\n  span` tail.\n",
        "- item with `a code span` tail.\n",
    ),
    (
        "1. item with `a code\n      span` tail.\n",
        "1. item with `a code\n   span` tail.\n",
        "1. item with `a code span` tail.\n",
    ),
    (
        "- [ ] task `a code\n    span` tail.\n",
        "- [ ] task `a code\n  span` tail.\n",
        "- [ ] task `a code span` tail.\n",
    ),
    (
        "- outer\n  - inner `a code\n      span` tail.\n",
        "- outer\n  - inner `a code\n    span` tail.\n",
        "- outer\n  - inner `a code span` tail.\n",
    ),
    (
        "> quoted `a code\n>     span` tail.\n",
        "> quoted `a code\n> span` tail.\n",
        "> quoted `a code span` tail.\n",
    ),
    (
        "> - item `a code\n>     span` tail.\n",
        "> - item `a code\n>   span` tail.\n",
        "> - item `a code span` tail.\n",
    ),
    (
        "> - [ ] task `a code\n>     span` tail.\n",
        "> - [ ] task `a code\n>   span` tail.\n",
        "> - [ ] task `a code span` tail.\n",
    ),
    (
        "Text.[^1]\n\n[^1]: a note that is long enough to wrap `a code\n      span` tail.\n",
        "Text.[^1]\n\n[^1]: a note that is long enough to wrap `a code\n    span` tail.\n",
        "Text.[^1]\n\n[^1]: a note that is long enough to wrap\n    `a code span` tail.\n",
    ),
];

#[test]
fn a_code_span_running_into_container_indentation_leaves_the_paragraph_as_written() {
    for (indented, _, _) in SPANS_INTO_CONTAINER_INDENTATION {
        assert_reflows_to(indented, 40, &REFLOW_MODES, indented);
    }
}

#[test]
fn a_code_span_running_to_the_content_column_joins() {
    for (_, at_column, joined) in SPANS_INTO_CONTAINER_INDENTATION {
        assert_reflows_to(at_column, 40, &[Mode::Normalize], joined);
    }
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

/// A lazy line falls short of the innermost item but still matches every
/// outer item it reaches, and their indentation renders as nothing even inside
/// a code span. Only the whitespace past the deepest matched item is code.
#[test]
fn a_lazy_line_in_a_nested_item_loses_the_indentation_of_the_items_it_matches() {
    assert_reflows_to(
        "- a\n  - `x y\n  `z` w\n",
        60,
        &[Mode::Normalize],
        "- a\n  - `x y `z` w\n",
    );
    for input in [
        "- a\n  - `x y\n  `z` w\n",
        "1. a\n   - `x y\n   `z` w and more words to wrap\n",
        "- a\n  - b\n    - `x y\n  `z` w\n",
        "- a\n  - b\n    - `x y\n    `z` w\n",
    ] {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 60] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{input:?} {settings:?}: {}", violation.label());
                }
            }
        }
    }
}

/// Whitespace a code span keeps at the start of a lazy line is code to
/// CommonMark, while pulldown-cmark strips a different amount of it, so the
/// item renders differently by renderer and no reflow can keep all of them.
/// The item is left as written.
#[test]
fn an_item_whose_lazy_line_starts_with_code_span_whitespace_is_left_as_written() {
    for input in [
        "- `x y\n `z` w and more words to wrap\n",
        "- a\n  - `x y\n   `z` w and more words to wrap\n",
        "1. a\n   - `x y\n    `z` w and more words to wrap\n",
        "- a\n  - b\n    - `x y\n   `z` w and more words\n",
        "- p\n\ntext\n  - `x y\n  `z` w and more words to wrap\n",
    ] {
        assert_reflows_to(input, 10, &REFLOW_MODES, input);
    }
}

/// An item whose content opens an HTML block holds HTML, not a paragraph, so
/// a line indented less than the item's content ends the list instead of
/// continuing the block lazily. Joined onto the block's last line, it moves
/// into the item.
#[test]
fn a_line_after_an_item_html_block_is_not_a_lazy_continuation() {
    for input in [
        "- <div>\n  b\n}\n",
        "- <div>\n  b c d e f g h i j k l m n o p q r s t u v w x y z\n}\n",
        "- <!-- a\n  b\n  c\n}\n",
        "- <!-- a\n- <!-- a\n        b\n    c\n}\n",
        "- x\n- <div>\n  b\n}\n",
        "1. <div>\n   b\n}\n",
    ] {
        assert_reflows_to(input, 20, &REFLOW_MODES, input);
    }
}

/// A paragraph of a list item can start on a line of its own, after an empty
/// marker line or an HTML block on the marker line. A lazy line under it does
/// not reach the item's content, so its indentation is no measure of where the
/// paragraph's reflowed lines belong.
#[test]
fn an_item_paragraph_keeps_its_indentation_over_a_lazy_line() {
    for (input, expected) in [
        ("-\n  b\nc. D e.\n", "-\n  b c.\n  D e.\n"),
        ("- <!-- a -->\n  b\nc. D e.\n", "- <!-- a -->\n  b c.\n  D e.\n"),
        (
            "- a\n  - <!-- a -->\n    b\nc. D e.\n",
            "- a\n  - <!-- a -->\n    b c.\n    D e.\n",
        ),
    ] {
        assert_reflows_to(input, 60, &[Mode::SentencePerLine], expected);
    }
    for input in [
        "-\n  b\nc. D e.\n",
        "- <!-- a -->\n  b\nc. D e.\n",
        "- a\n  - <!-- a -->\n    b\nc. D e.\n",
    ] {
        for mode in REFLOW_MODES {
            for line_length in [10, 60] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
}

/// A task checkbox alone on a marker line opens no paragraph for
/// pulldown-cmark, so the line under it leaves the list there, while
/// markdown-rs continues the checkbox's paragraph lazily. Reflowed in place,
/// the line renders the same for both; joined onto the checkbox, it moves
/// into the item for pulldown-cmark.
#[test]
fn a_line_after_a_lone_task_checkbox_stays_out_of_the_item() {
    for checkbox in ["[ ]", "[x]", "[X]"] {
        let input = format!("- {checkbox}\nfoo bar baz qux quux corge\n");
        let expected = format!("- {checkbox}\nfoo bar baz qux quux\ncorge\n");
        assert_reflows_to(&input, 20, &[Mode::Normalize], &expected);
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(&input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
}

/// A closing tag opens an HTML block that interrupts the item's paragraph, and
/// any HTML block but a raw-text one runs on past its closing tag to a blank
/// line. Neither the tag nor the text under it is prose to reflow.
#[test]
fn an_html_block_in_an_item_runs_to_a_blank_line() {
    for (input, expected) in [
        (
            "- a b c\n  d e\n  </div>\n  <div>\n",
            "- a b c d e\n  </div>\n  <div>\n",
        ),
        ("- a b c\nd e\n  </div>\n  <div>\n", "- a b c d e\n  </div>\n  <div>\n"),
        (
            "- x\n  <div>\n  a\n  </div>\n  bar baz qux quux corge grault garply\n",
            "- x\n  <div>\n  a\n  </div>\n  bar baz qux quux corge grault garply\n",
        ),
        (
            "- x\n  <div>\n  a\n  </div>\n\n  bar baz qux quux corge grault garply\n",
            "- x\n  <div>\n  a\n  </div>\n\n  bar baz qux quux corge\n  grault garply\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 30, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
}

/// HTML text in an item keeps its indentation past the item's content column,
/// which inside a `<pre>` is visible text. Reflow moves it with the marker.
#[test]
fn an_html_block_in_an_item_keeps_its_indentation() {
    for (input, expected) in [
        (
            "- aaa bbb ccc ddd eee fff ggg hhh\n  <pre>\n    code  x\n  y\n  </pre>\n",
            "- aaa bbb ccc ddd eee fff ggg\n  hhh\n  <pre>\n    code  x\n  y\n  </pre>\n",
        ),
        (
            "-   aaa bbb ccc ddd eee fff ggg hhh\n    <pre>\n      code\n    </pre>\n",
            "- aaa bbb ccc ddd eee fff ggg\n  hhh\n  <pre>\n    code\n  </pre>\n",
        ),
        (
            "- aaa bbb ccc ddd eee fff ggg hhh\n    <div>\n    x\n",
            "- aaa bbb ccc ddd eee fff ggg\n  hhh\n    <div>\n    x\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 30, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
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

/// A wrapped line made of a table's delimiter cells (`------ | - |`) under a
/// line with as many cells opens a GFM table, which may interrupt a paragraph,
/// so the line before it became a table header. The wrap moves instead. The
/// top-level and blockquote rows are controls: those paths leave a line with
/// pipes unwrapped.
#[test]
fn reflow_never_starts_a_line_with_a_table_delimiter_row() {
    assert_reflows_to(
        "- Some words here a | b | ------ | - |\n  morewordsafter\n",
        20,
        &[Mode::Normalize],
        "- Some words here\n  a |\n  b | ------ | - |\n  morewordsafter\n",
    );
    for input in [
        "- Some words here a | b | ------ | - |\n  morewordsafter\n",
        "Some words here a | b | ------ | - | morewordsafter and more words\n",
        "> Some words here a | b | --- | :-: | morewordsafter and more\n",
        "1. Some words a | b | c | --: | --- | :-- | and more words after it\n",
    ] {
        for mode in REFLOW_MODES {
            for line_length in [10, 15, 20, 25, 30] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
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

/// An ATX heading interrupts a paragraph wherever it is written, so a heading
/// inside a list item ends the item's paragraph and keeps its line.
#[test]
fn a_heading_in_a_list_item_keeps_its_line() {
    for (input, expected) in [
        (
            "- Search the tree with grep and list the files that use it.\n  # Usage\nz.\n",
            "- Search the tree with grep\n  and list the files that use\n  it.\n  # Usage\nz.\n",
        ),
        (
            "1. Search the tree with grep and list the files that use it.\n   ### Usage notes\n",
            "1. Search the tree with grep\n   and list the files that use\n   it.\n   ### Usage notes\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 30, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
}

/// Lines a code span runs on into, each spelled like the start of a block the
/// collectors stop at: a heading, a link reference definition, an ordered item
/// that may not interrupt a paragraph, a table row, an HTML tag. The last two
/// carry whitespace of the span past the line start, which a reflow of the
/// span's tail on its own would rewrite.
const SPAN_CONTINUATIONS: [&str; 10] = [
    "#b`",
    "#`",
    "#### b c`",
    "[a]: b`",
    "7. b`",
    "2) b`",
    "|b`",
    "<span>`",
    "#b  c` d",
    "7. b  c` d",
];

/// Run the oracle over a code span opened on the first line of each container
/// and closed on the next, spelled as each of `SPAN_CONTINUATIONS`. The span
/// holds double spaces, which any reflow that splits it collapses. A last line
/// follows both lazily and after a blank line: a quoted item with a lazy line
/// is left as written, so only the blank line lets its reflow run.
fn assert_span_continuations_keep_rendering(containers: &[(&str, &str)]) {
    let mut violations = Vec::new();
    for (first, continuation) in containers {
        for (next, end) in SPAN_CONTINUATIONS
            .iter()
            .flat_map(|next| ["\nz.\n", "\n\nz.\n"].map(|end| (next, end)))
        {
            let input = format!("{first}w w w w w w w w `x.  E  F  G  H  I  J  K  L\n{continuation}{next}{end}");
            for mode in REFLOW_MODES {
                for line_length in [10, 20, 40, 80] {
                    let settings = ReflowSettings::with_mode(mode, line_length);
                    if let Err(violation) = check(&input, &settings) {
                        violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// A line a code span runs on into continues the span's paragraph however it
/// is spelled, and the span is reflowed whole.
#[test]
fn a_code_span_runs_on_into_a_line_that_looks_like_a_block_start() {
    assert_reflows_to(
        "Search the tree with `grep -n  -w\n#include` to list the files that use it.\n",
        30,
        &[Mode::Normalize],
        "Search the tree with\n`grep -n  -w #include` to list\nthe files that use it.\n",
    );
    assert_span_continuations_keep_rendering(&[("", "")]);
}

/// Without the span, the same heading still ends the paragraph.
#[test]
fn a_heading_without_a_code_span_still_ends_the_paragraph() {
    assert_reflows_to(
        "Search the tree with grep and list the files that use it.\n#### Usage\nz.\n",
        30,
        &[Mode::Normalize],
        "Search the tree with grep and\nlist the files that use it.\n#### Usage\nz.\n",
    );
}

#[test]
fn a_code_span_runs_on_inside_a_list_item() {
    assert_reflows_to(
        "- Search the tree with `grep -n  -w\n  7. step` to list the files that use it.\n",
        30,
        &[Mode::Normalize],
        "- Search the tree with\n  `grep -n  -w 7. step` to\n  list the files that use it.\n",
    );
    assert_span_continuations_keep_rendering(&[("- ", "  "), ("1. ", "   "), ("- ", " "), ("- ", "")]);
}

#[test]
fn a_code_span_runs_on_inside_a_blockquote() {
    assert_reflows_to(
        "> Search the tree with `grep -n  -w\n> [a]: b` to list the files that use it.\n",
        30,
        &[Mode::Normalize],
        "> Search the tree with\n> `grep -n  -w [a]: b` to list\n> the files that use it.\n",
    );
    assert_span_continuations_keep_rendering(&[("> ", "> "), ("> > ", "> > "), ("> ", "")]);
}

/// The continuation line is part of the item however far it is indented: to
/// the content column, short of it, or not at all.
#[test]
fn a_code_span_runs_on_inside_a_list_item_in_a_blockquote() {
    assert_reflows_to(
        "> - Search the tree with `grep -n  -w\n> #include` to list the files that use it.\n",
        30,
        &[Mode::Normalize],
        "> - Search the tree with\n>   `grep -n  -w #include` to\n>   list the files that use\n>   it.\n",
    );
    assert_span_continuations_keep_rendering(&[
        ("> - ", ">   "),
        ("> - ", ">  "),
        ("> - ", "> "),
        ("> - ", ""),
        ("> -   ", ">   "),
    ]);
}

#[test]
fn a_code_span_runs_on_inside_a_footnote() {
    assert_reflows_to(
        "[^1]: Search the tree with `grep -n  -w\n    #include` to list the files that use it.\n",
        30,
        &[Mode::Normalize],
        "[^1]: Search the tree with\n    `grep -n  -w #include`\n    to list the files that\n    use it.\n",
    );
    assert_span_continuations_keep_rendering(&[
        ("[^1]: ", "    "),
        ("[^1]: ", "   "),
        ("[^1]: ", "  "),
        ("[^1]: ", ""),
    ]);
}

/// A marker line that opens a second container hands the item collector a
/// paragraph it cannot measure: a quote's continuation ends the item, and the
/// inner item's content column is not in the line info. With a code span
/// crossing one of its breaks, the item is left as written, and so is the
/// rest of the paragraph after it.
#[test]
fn a_code_span_runs_on_from_a_marker_line_that_opens_a_container() {
    for input in [
        "- > Search the tree with `grep -n  -w\n  > #include  <x>` to list the files that use it.\n",
        "- - Search the tree with `grep -n  -w\n    #include` to list the files that use it.\n",
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], input);
    }
    // The inner item's paragraph after the blank line keeps its indentation.
    for input in [
        "- -\t`x.  E\n  H` j\n\n    k\n",
        "- -\tw w w\n    v `x.  E\nH` j\n\n    k\n",
    ] {
        assert_reflows_to(input, 80, &[Mode::Normalize, Mode::SemanticLineBreaks], input);
    }
    assert_span_continuations_keep_rendering(&[
        ("- > ", "  > "),
        ("- > ", "  "),
        ("- > ", ""),
        ("- - ", ""),
        ("- - ", "  "),
        ("- - ", "   "),
        ("- - ", "    "),
        ("- - ", "      "),
        ("1. - ", "     "),
        ("- 1. ", "  "),
    ]);
}

/// An ordered marker closes with `.` or `)`. A `2)` line after a quote opens
/// a list outside it, so the quote's paragraph ends above it.
#[test]
fn a_parenthesis_list_after_a_quote_stays_out_of_it() {
    assert_reflows_to(
        "> Search the tree with grep to list the files that use it.\n2) Then edit them.\n",
        30,
        &[Mode::Normalize],
        "> Search the tree with grep to\n> list the files that use it.\n2) Then edit them.\n",
    );
}

/// The positive control: an item opened with `)` is a list item, reflowed
/// under its own marker.
#[test]
fn a_parenthesis_list_item_is_reflowed_as_one() {
    for (input, expected) in [
        (
            "1) Search the tree with grep to list the files that use it.\n",
            "1) Search the tree with grep\n   to list the files that use\n   it.\n",
        ),
        (
            "> 1) Search the tree with grep to list the files that use it.\n",
            "> 1) Search the tree with grep\n>    to list the files that\n>    use it.\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
    }
}

/// Each `)` item after a paragraph is its own one-sentence item, the same as
/// with `.`. Read as one paragraph, the second marker was joined onto the
/// first line and the list collapsed into a single item.
#[test]
fn parenthesis_list_items_stay_one_per_line() {
    for input in [
        "Two checks:\n\n1) Before fetching\n2) Before sending\n",
        "Two checks:\n\n1. Before fetching\n2. Before sending\n",
    ] {
        assert_reflows_to(input, 0, &SENTENCE_MODES, input);
    }
}

/// Reflow rewrites only the item's paragraph, so a later line of the quote
/// keeps its indentation. Narrowing the marker's padding would move the
/// item's content column onto that line and nest it in the item.
#[test]
fn a_quoted_item_keeps_its_marker_where_a_later_line_sits_short_of_its_content() {
    for (input, expected) in [
        (
            "> -   Search the tree with grep to list the files that use it.\n>   7. Then edit them.\n",
            "> -   Search the tree with\n>     grep to list the files\n>     that use it.\n>   7. Then edit them.\n",
        ),
        (
            "> 1)   Search the tree with grep to list the files that use it.\n>    7. Then edit them.\n",
            "> 1)   Search the tree with\n>      grep to list the files\n>      that use it.\n>    7. Then edit them.\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
    }
}

/// A hard break ends a line of the footnote wherever reflow puts it.
#[test]
fn a_footnote_keeps_its_hard_breaks() {
    for (input, expected) in [
        (
            "[^1]: Search the tree with grep to list  \n    the files that use it and edit them.\n",
            "[^1]: Search the tree with\n    grep to list  \n    the files that use it\n    and edit them.\n",
        ),
        (
            "[^1]: Search the tree with grep to list\\\n    the files that use it and edit them.\n",
            "[^1]: Search the tree with\n    grep to list\\\n    the files that use it\n    and edit them.\n",
        ),
        (
            "[^1]: Search the tree\n    with grep to list  \n    the files that use it and edit them.\n",
            "[^1]: Search the tree with\n    grep to list  \n    the files that use it\n    and edit them.\n",
        ),
    ] {
        assert_reflows_to(input, 30, &[Mode::Normalize], expected);
    }
}

/// A line that cannot interrupt a paragraph continues the footnote's text
/// however it starts (a definition, an ordered item not opening at 1, a table
/// row with no delimiter row), and a blank line put before it would cut it
/// off. One that does open a block keeps the blank line it has in the source.
#[test]
fn a_footnote_line_that_cannot_interrupt_its_paragraph_stays_in_it() {
    for next in ["[a]: b", "7. b", "| a | b |"] {
        let input =
            format!("[^1]: Search the tree with grep to list the files that use it.\n    {next}\n    Then edit.\n");
        let expected = format!(
            "[^1]: Search the tree with\n    grep to list the files\n    that use it.\n    {next}\n    Then edit.\n"
        );
        assert_reflows_to(&input, 30, &[Mode::Normalize], &expected);
    }
    assert_reflows_to(
        "[^1]: Search the tree with grep to list the files that use it.\n\n    - b\n",
        30,
        &[Mode::Normalize],
        "[^1]: Search the tree with\n    grep to list the files\n    that use it.\n\n    - b\n",
    );
}

/// Assert `input` comes back unchanged in every reflow mode, at several line
/// lengths, with code spans and links held atomic and not.
fn assert_left_as_written(input: &str) {
    for atomic_spans in [true, false] {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 40] {
                let settings = ReflowSettings {
                    atomic_spans,
                    ..ReflowSettings::with_mode(mode, line_length)
                };
                assert_reflows_with(input, &settings, input);
            }
        }
    }
}

/// A line that could be a table row is passed over, so the paragraph is
/// collected from the line after it, here indented four columns. Its code
/// span runs on into indentation, and was missed because the text was parsed
/// as an indented code block, so the span's tail was joined and lost the
/// whitespace that renders inside it.
#[test]
fn a_code_span_after_a_table_lookalike_running_into_indentation_is_left_as_written() {
    assert_left_as_written(
        "Fall back with a || b, then run\n    `cargo test\n  --all` to check the whole workspace.\n",
    );
}

/// Without the span, the indented line is prose and is reflowed.
#[test]
fn an_indented_line_after_a_table_lookalike_is_still_reflowed() {
    assert_reflows_to(
        "Fall back with a || b, then run\n    cargo test --all to check the whole workspace.\n",
        20,
        &[Mode::Normalize],
        "Fall back with a || b, then run\ncargo test --all to\ncheck the whole\nworkspace.\n",
    );
}

/// A math span holding a backtick, followed on the next line by each of
/// `SPAN_CONTINUATIONS` with the math's closing delimiter before its last
/// backtick, in each container. rumdl reads the math, and a reader without
/// math reads a code span from the first backtick to the last, which the
/// reflow has to keep whole for the rendering to stay the same.
fn assert_math_continuations_keep_rendering(delimiter: &str) {
    let containers: &[(&str, &str)] = &[
        ("", ""),
        ("- ", "  "),
        ("1. ", "   "),
        ("- ", " "),
        ("- ", ""),
        ("> ", "> "),
        ("> > ", "> > "),
        ("> ", ""),
        ("> - ", ">   "),
        ("> - ", ">  "),
        ("> - ", "> "),
        ("> - ", ""),
        ("> -   ", ">   "),
        ("[^1]: ", "    "),
        ("[^1]: ", "   "),
        ("[^1]: ", "  "),
        ("[^1]: ", ""),
        ("- > ", "  > "),
        ("- > ", "  "),
        ("- > ", ""),
        ("- - ", ""),
        ("- - ", "  "),
        ("- - ", "   "),
        ("- - ", "    "),
        ("- - ", "      "),
        ("1. - ", "     "),
        ("- 1. ", "  "),
    ];
    let mut violations = Vec::new();
    for (first, continuation) in containers {
        // The second head is a line that could be a table row, which the
        // paragraph collectors pass over.
        for head in ["w w w w w w w w", "w | w w w w w w"] {
            for next in SPAN_CONTINUATIONS {
                let (code, rest) = next.rsplit_once('`').expect("each continuation closes the span");
                for end in ["\nz.\n", "\n\nz.\n"] {
                    for lead in ["", " "] {
                        let input = format!(
                            "{first}{head} {delimiter}y `x.  E  F  G  H  I  J  K  L\n{continuation}{lead}{code}{delimiter}`{rest}{end}"
                        );
                        for mode in REFLOW_MODES {
                            let settings = ReflowSettings::with_mode(mode, 20);
                            if let Err(violation) = check(&input, &settings) {
                                violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// rumdl reads `$...$` as math, and a backtick inside it opens no code span,
/// while a reader without math reads a code span from that backtick, here
/// running on into the next line. The paragraph collectors asked only the
/// math reading where code spans run, so a line the span runs on into could
/// start a paragraph of its own, and its reflow rewrote the whitespace the
/// span holds. A line inside a code span under either reading continues it.
#[test]
fn a_code_span_inside_inline_math_runs_on_as_code() {
    // The line the span runs on into would start a paragraph of its own
    // after a table lookalike, so the paragraph cannot be rewritten.
    assert_left_as_written("Prices | start at $x and `cost\n c$` per unit, shipped the same day.\n");
    // A heading lookalike the span runs on into continues the paragraph, and
    // the span is kept whole.
    assert_reflows_to(
        "Prices start at $x and `cost  more\n#b$` per unit, shipped the same day.\n",
        20,
        &[Mode::Normalize, Mode::SemanticLineBreaks],
        "Prices start at\n$x and `cost  more #b$`\nper unit, shipped\nthe same day.\n",
    );
    // The math closes on the first line, the span its backtick opens on the
    // second.
    assert_left_as_written("Prices | start at $x and `cost$\n  more` per unit, shipped the same day.\n");
    assert_math_continuations_keep_rendering("$");
}

/// The same for `$$...$$` written inside a paragraph. A list item re-emits a
/// line inside multi-line display math as written and reflowed the prose
/// above it on its own, cutting the code span a reader without math sees
/// crossing between them.
#[test]
fn a_code_span_inside_display_math_runs_on_as_code() {
    assert_left_as_written("- Prices start at $$x and `cost  more\n  #b$$` per unit, shipped the same day.\n");
    assert_math_continuations_keep_rendering("$$");
}

/// Math without a backtick reads the same either way, and a span a backtick
/// in math opens ends with its paragraph, so neither keeps a paragraph from
/// being reflowed.
#[test]
fn math_that_reads_the_same_without_math_is_still_reflowed() {
    assert_reflows_to(
        "Sums like $x + y$ stay, and the paragraph runs on for a while and is long.\n",
        20,
        &[Mode::Normalize],
        "Sums like $x + y$\nstay, and the\nparagraph runs on\nfor a while and is\nlong.\n",
    );
    assert_reflows_to(
        "Sums like $x + y$ and $a`b$ stay.\n\nThe next paragraph runs on for a while ` and is long.\n",
        20,
        &[Mode::Normalize],
        "Sums like $x + y$\nand $a`b$ stay.\n\nThe next paragraph\nruns on for a while\n` and is long.\n",
    );
}

/// Containers a table can sit in, as the prefix of the line opening the
/// container and the prefix of the lines continuing it.
const TABLE_CONTAINERS: &[(&str, &str)] = &[
    ("", ""),
    ("- ", "  "),
    ("1. ", "   "),
    ("> ", "> "),
    ("> > ", "> > "),
    ("> - ", ">   "),
    ("- > ", "  > "),
    ("- - ", "    "),
    ("[^1]: ", "    "),
];

/// A GFM table needs no pipes: a line over `---:` is a one-cell table, and a
/// header line without a pipe is still a header over `|---|`. rumdl's own
/// table detection wants pipes, so the paragraph collectors took such a
/// table for prose, joined its delimiter row into the line above or rewrapped
/// its header, and the table became a paragraph.
#[test]
fn a_table_without_pipes_is_left_as_written() {
    let delimiters = [
        "---:",
        ":---",
        ":-:",
        " --: ",
        "--- |",
        "| ---:",
        "-:|",
        "|---|",
        "--- | :-:",
    ];
    let mut violations = Vec::new();
    for (first, continuation) in TABLE_CONTAINERS {
        for delimiter in delimiters {
            for shape in [
                format!("{first}head h h h h h h h h h h\n{continuation}{delimiter}\n"),
                format!("{first}w w w w w w w w w w\n{continuation}head h h h h h h h h\n{continuation}{delimiter}\n"),
                format!("{first}head h h\n{continuation}{delimiter}\n{continuation}body b b b b b b b b b b b\n"),
            ] {
                for mode in REFLOW_MODES {
                    for line_length in [0, 20] {
                        let settings = ReflowSettings::with_mode(mode, line_length);
                        if let Err(violation) = check(&shape, &settings) {
                            violations.push(format!("{settings:?} {shape:?}: {}", violation.label()));
                        }
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A line made of a delimiter row is a table delimiter under the line above
/// it, so a wrap may not end with one on a line of its own.
#[test]
fn a_wrap_never_leaves_a_table_delimiter_on_a_line_of_its_own() {
    assert_reflows_to(
        "w w w w w w w w w w ---:\n",
        20,
        &[Mode::Normalize],
        "w w w w w w w w w\nw ---:\n",
    );
    let mut violations = Vec::new();
    for (first, _) in TABLE_CONTAINERS {
        for input in [
            format!("{first}w w w w w w w w w w ---:\n"),
            format!("{first}w w w w w w w w w w :-:\n"),
            format!("{first}w w w w w w w w w w -: | --\n"),
        ] {
            for mode in REFLOW_MODES {
                for line_length in [10, 20, 30] {
                    let settings = ReflowSettings::with_mode(mode, line_length);
                    if let Err(violation) = check(&input, &settings) {
                        violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A table ends at a blank line or at the next block, and the block after
/// it is reflowed as before. A blank line, a heading or a code block is no
/// table header.
#[test]
fn the_blocks_around_a_table_without_pipes_are_still_reflowed() {
    for (input, expected) in [
        (
            "head\n---:\n> quoted q q q q q q q q q q\n",
            "head\n---:\n> quoted q q q q q q\n> q q q q\n",
        ),
        (
            "- head\n  ---:\n- next n n n n n n n n n n\n",
            "- head\n  ---:\n- next n n n n n n n\n  n n n\n",
        ),
        (
            "head\n---:\n- next n n n n n n n n n n\n",
            "head\n---:\n- next n n n n n n n\n  n n n\n",
        ),
        (
            "head\n---:\n# Title\nprose p p p p p p p p p p\n",
            "head\n---:\n# Title\nprose p p p p p p p\np p p\n",
        ),
        (
            "head\n---:\n```\nc\n```\nprose p p p p p p p p p p\n",
            "head\n---:\n```\nc\n```\nprose p p p p p p p\np p p\n",
        ),
        (
            "a\n\n---:\nprose p p p p p p p p p p\n",
            "a\n\n---: prose p p p p p\np p p p p\n",
        ),
        (
            "```\nc\n```\n---:\nprose p p p p p p p p p p\n",
            "```\nc\n```\n---: prose p p p p p\np p p p p\n",
        ),
        (
            "# Title\n---:\nprose p p p p p p p p p p\n",
            "# Title\n---: prose p p p p p\np p p p p\n",
        ),
    ] {
        assert_reflows_to(input, 20, &[Mode::Normalize], expected);
    }
}

/// A table rumdl's table detection sees is kept as written by the
/// collectors themselves, and the prose around it is reflowed.
#[test]
fn the_prose_around_a_table_with_pipes_in_a_list_item_is_still_reflowed() {
    assert_reflows_to(
        "- prose p p p p p p p p p p\n\n  | a | b |\n  | - | - |\n",
        20,
        &[Mode::Normalize],
        "- prose p p p p p p\n  p p p p\n\n  | a | b |\n  | - | - |\n",
    );
}

/// Prose over a line of bare dashes is a setext heading rather than a table,
/// and prose with no delimiter row under it is reflowed as before.
#[test]
fn prose_without_a_delimiter_row_under_it_is_still_reflowed() {
    assert_reflows_to(
        "head h h h h h h h h h h\nnext n n: n -- n\n",
        20,
        &[Mode::Normalize],
        "head h h h h h h h h\nh h next n n: n -- n\n",
    );
    assert_reflows_to(
        "- head h h h h h h h h h h\n  next n n n n\n",
        20,
        &[Mode::Normalize],
        "- head h h h h h h h\n  h h h next n n n n\n",
    );
}

/// An underline under a list item's paragraph makes the paragraph a setext
/// heading. The list collector took the heading's lines for prose: it joined
/// the underline into the text, or the text after the heading into the
/// underline.
#[test]
fn a_setext_heading_in_a_list_item_keeps_its_lines() {
    let mut violations = Vec::new();
    for (first, continuation) in TABLE_CONTAINERS {
        for input in [
            format!("{first}head h h h h h h h h h h\n{continuation}===\n"),
            format!("{first}w w w w w w w w w w\n{continuation}head h h h h h h h h\n{continuation}---\n"),
            format!(
                "{first}a\n\n{continuation}head h h h h h h h h h h\n{continuation}===\n{continuation}tail t t t t t t t t t t t t\n"
            ),
            format!(
                "{first}a\n\n{continuation}w w w w w w w w w w w w\n{continuation}head h h h h h h h h\n{continuation}---\n{continuation}tail t t t t t t t t t t t t\n"
            ),
        ] {
            for mode in REFLOW_MODES {
                for line_length in [0, 10, 20, 30] {
                    let settings = ReflowSettings::with_mode(mode, line_length);
                    if let Err(violation) = check(&input, &settings) {
                        violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The prose around a heading in a list item is still reflowed, and the
/// heading's own lines are not, the same as for a heading outside a list.
#[test]
fn the_prose_around_a_setext_heading_in_a_list_item_is_still_reflowed() {
    assert_reflows_to(
        "- a\n\n  w w w w w w w w w w w w\n  head\n  ---\n",
        20,
        &[Mode::Normalize],
        "- a\n\n  w w w w w w w w w w w w\n  head\n  ---\n",
    );
    assert_reflows_to(
        "- a\n\n  head\n  ===\n\n  tail t t t t t t t t t t t t\n",
        20,
        &[Mode::Normalize],
        "- a\n\n  head\n  ===\n\n  tail t t t t t t t\n  t t t t t\n",
    );
}

/// A marker line can open a second item (`- - a`), and a paragraph after a
/// blank line at that inner item's content column belongs to the inner item.
/// The list collector re-indented it to the outer item's column, which moved
/// it out of the inner item.
#[test]
fn a_paragraph_of_an_item_opened_on_its_parents_marker_line_stays_in_it() {
    // The marker line, and the content columns of the outer and inner items.
    let containers = [
        ("- - ", 2, 4),
        ("1. - ", 3, 5),
        ("- 1. ", 2, 5),
        ("* + ", 2, 4),
        ("-   - ", 4, 6),
        ("> - - ", 2, 4),
    ];
    let mut violations = Vec::new();
    for (first, outer_col, inner_col) in containers {
        let quote = if first.starts_with('>') { "> " } else { "" };
        let blank = if quote.is_empty() { "" } else { ">" };
        for extra in [0, 1] {
            let pad = format!("{quote}{}", " ".repeat(inner_col + extra));
            let outer = format!("{quote}{}", " ".repeat(outer_col));
            let inputs = [
                format!("{first}a\n{blank}\n{pad}tail t t t t t t t t t t t t\n"),
                format!("{first}a a a a a a a a a a a a\n{blank}\n{pad}tail t t t t t t t t t t t t\n"),
                format!("{first}a\n{blank}\n{pad}w w\n{blank}\n{pad}tail t t t t t t t t t t t t\n"),
                format!("{first}a\n{pad}b\n{blank}\n{pad}tail t t t t t t t t t t t t\n"),
                format!("{first}a\n{outer}b\n{blank}\n{pad}tail t t t t t t t t t t t t\n"),
                format!(
                    "{first}a\n{blank}\n{pad}w w w w w w w w w w w w\n{blank}\n{outer}tail t t t t t t t t t t t t\n"
                ),
            ];
            for input in inputs {
                for mode in REFLOW_MODES {
                    for line_length in [0, 10, 20, 30] {
                        let settings = ReflowSettings::with_mode(mode, line_length);
                        if let Err(violation) = check(&input, &settings) {
                            violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                        }
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A paragraph of the inner item is reflowed at the inner item's column, and
/// one of the outer item at the outer item's. An item holding paragraphs of
/// both is left as written.
#[test]
fn the_paragraphs_after_a_nested_marker_line_are_reflowed_in_their_own_item() {
    assert_reflows_to(
        "- - a\n\n    tail t t t t t t t t t t t t\n",
        20,
        &[Mode::Normalize],
        "- - a\n\n    tail t t t t t t\n    t t t t t t\n",
    );
    assert_reflows_to(
        "- - a\n  b\n\n     tail t t t t t t t t t t t t\n",
        20,
        &[Mode::SemanticLineBreaks],
        "- - a b\n\n     tail t t t t t\n     t t t t t t t\n",
    );
    assert_reflows_to(
        "- - a\n\n    w w w w w w w w w w w w\n\n  tail t t t t t t t t t t t t\n",
        20,
        &REFLOW_MODES,
        "- - a\n\n    w w w w w w w w w w w w\n\n  tail t t t t t t t t t t t t\n",
    );
    assert_reflows_to(
        "- - a\n\n  tail t t t t t t t t t t t t\n",
        20,
        &[Mode::Normalize],
        "- - a\n\n  tail t t t t t t t\n  t t t t t\n",
    );
}

/// A list item's content starts past all the padding after its marker, and a
/// line short of that column after a blank line is not in the item. The list
/// collector measured against a marker of one padding space, and so took such
/// a line into the item.
///
/// Normalize also re-spaces a marker to the spacing MD030 wants, which moves
/// the item's content column. Moved left, the column reached a line after the
/// item that stopped short of the old one, and that line joined the item: a
/// paragraph after the list moved into it, and a sibling item became a
/// nested one.
#[test]
fn re_spacing_a_marker_never_pulls_a_later_line_into_the_item() {
    // The item's first line, the lines continuing it, and the columns between
    // the re-spaced item's content column and the written one.
    let items = [
        ("-   a a a a a a a a a a a a", "", 2..4),
        ("1.  a a a a a a a a a a a a", "", 3..4),
        ("*    a a a a a a a a a a a a", "", 2..5),
        ("- a\n\n  -   b b b b b b b b b b b b", "", 4..6),
        ("-   a a a a a a a a a a a a\n    c c", "", 2..4),
    ];
    let mut violations = Vec::new();
    for (item, _, columns) in items {
        for column in columns {
            let pad = " ".repeat(column);
            for after in ["tail t t t t t t t t t t t t", "- b", "# h", "```\ncode\n```", "> q"] {
                let after = after.replace('\n', &format!("\n{pad}"));
                for separator in ["\n", "\n\n"] {
                    let input = format!("{item}{separator}{pad}{after}\n");
                    for mode in REFLOW_MODES {
                        for line_length in [0, 10, 20, 30] {
                            let settings = ReflowSettings::with_mode(mode, line_length);
                            if let Err(violation) = check(&input, &settings) {
                                violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A line after the item that the re-spaced column does not reach leaves the
/// marker free to be re-spaced, and an item whose next line it would reach
/// keeps its marker as written.
#[test]
fn a_marker_is_still_re_spaced_when_no_later_line_reaches_its_new_column() {
    assert_reflows_to(
        "-   a a a a a a a a a a a a\n\n  tail t t t t t t t t t t t t\n",
        20,
        &[Mode::Normalize],
        "-   a a a a a a a a\n    a a a a\n\n  tail t t t t t t t\n  t t t t t\n",
    );
    assert_reflows_to(
        "-   a a a a a a a a a a a a\n\ntail\n",
        20,
        &[Mode::Normalize],
        "- a a a a a a a a a\n  a a a\n\ntail\n",
    );
    assert_reflows_to(
        "- a\n\n  -   b b b b b b b b b b b b\n\n   tail\n",
        20,
        &[Mode::Normalize],
        "- a\n\n  - b b b b b b b b\n    b b b b\n\n   tail\n",
    );
}

/// A paragraph in a list item that follows another block with no blank line
/// between them keeps it that way. The list reflow wrote a blank line before
/// every paragraph after another block, so a paragraph the table detection
/// ended early, after rows a GFM reader takes for prose, split in two.
#[test]
fn a_paragraph_after_another_block_in_a_list_item_gets_no_blank_line_before_it() {
    let items = [
        "- : u|- ---- ||\n| les### 3. ocumen\n",
        "- :\n  u|-\n  ---- || |\n  les\n  3. tail t t t t t t t t t t\n",
        "- a\n  u|-\n  ---- || |\n  tail t t t t t t t t t t t t\n",
        "- a\n  <pre>\n  x\n  </pre>\n  tail t t t t t t t t t t t t\n",
        "- a\n  <!-- c\n  -->\n  tail t t t t t t t t t t t t\n",
    ];
    let blank_lines = |text: &str| text.lines().filter(|line| line.trim().is_empty()).count();
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 30] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                let output = reflow(input, &settings).expect("reflow runs");
                if blank_lines(&output) != blank_lines(input) {
                    violations.push(format!("{settings:?} {input:?}: blank line added, {output:?}"));
                }
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The paragraph is still reflowed, under the block it follows.
#[test]
fn a_paragraph_after_another_block_in_a_list_item_is_still_reflowed() {
    assert_reflows_to(
        "- a\n  <pre>\n  x\n  </pre>\n  tail t t t t t t t t t t t t\n",
        20,
        &[Mode::Normalize],
        "- a\n  <pre>\n  x\n  </pre>\n  tail t t t t t t t\n  t t t t t\n",
    );
    assert_reflows_to(
        "- : u|- ---- ||\n| les### 3. ocumen\n",
        10,
        &[Mode::Normalize],
        "- :\n  u|-\n  ---- || |\n  les###\n  3.\n  ocumen\n",
    );
}

/// A lazy continuation line that is one whole tag would open a type-7 HTML
/// block outside the list item. CommonMark keeps the line in the item's
/// paragraph, since such a block cannot interrupt one, but markdown-rs and
/// comrak end the item before it. Indenting the line into the item or joining
/// it into the paragraph changed what one of the two readings rendered, so the
/// item is left as written.
#[test]
fn a_list_item_with_a_lazy_line_that_is_one_whole_tag_is_left_as_written() {
    let items = [
        "  - A.svg\">\n<i a=\"\" s=\"\">",
        "  - A.svg\">\n<i a=\"\" s=\"\">\n",
        "- one two three four five six seven eight nine ten eleven twelve\n<i a=\"\">\ntail words here\n",
        "- one two three four five six seven eight nine ten eleven twelve\n</i>\n",
        "- a\n  - one two three four five six seven eight nine ten eleven twelve\n<span>\n",
    ];
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 40] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                let output = reflow(input, &settings).expect("reflow runs");
                if output != input {
                    violations.push(format!("{settings:?} {input:?}: rewritten to {output:?}"));
                }
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A lazy line holding text after its tag opens no HTML block, so every
/// reader keeps it in the item and the item is reflowed. In a blockquote the
/// item is reflowed too, and a lazy tag line is kept as written below it.
#[test]
fn a_list_item_with_a_lazy_line_that_is_more_than_a_tag_is_still_reflowed() {
    assert_reflows_to(
        "- one two three four five six seven eight nine ten eleven twelve\n<i a=\"\"> text\n",
        20,
        &[Mode::Normalize],
        "- one two three four\n  five six seven\n  eight nine ten\n  eleven twelve\n  <i a=\"\"> text\n",
    );
    assert_reflows_to(
        "> - one two three four five six seven eight nine ten eleven twelve\n> <i a=\"\">\n",
        20,
        &[Mode::Normalize],
        "> - one two three\n>   four five six\n>   seven eight nine\n>   ten eleven\n>   twelve\n> <i a=\"\">\n",
    );
}

/// A blank line before or after a div marker (`:::`) or a snippet delimiter
/// (`--8<--`) in a list item is kept. The list reflow wrote neither, so the
/// paragraphs on either side of the line joined it into one paragraph.
#[test]
fn a_div_marker_or_snippet_line_in_a_list_item_keeps_its_blank_lines() {
    let items = [
        "- {\n  :::\n\n  `",
        "- a\n  :::\n\n  x\n",
        "- a\n\n  :::\n\n  x\n",
        "- a\n\n  --8<--\n\n  x\n",
        "- a\n\n  ::: note\n  x\n  :::\n\n  y\n",
        "- a\n\n  ```\n  c\n  ```\n\n  :::\n\n  y\n",
        "- a\n\n  NOTE:\n\n  :::\n\n  y\n",
        "- a\n\n  :::\n  :::\n\n  y\n",
    ];
    let blank_lines = |text: &str| text.lines().filter(|line| line.trim().is_empty()).count();
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                let output = reflow(input, &settings).expect("reflow runs");
                if blank_lines(&output) != blank_lines(input) {
                    violations.push(format!("{settings:?} {input:?}: blank line dropped, {output:?}"));
                }
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The paragraphs around the marker are still reflowed.
#[test]
fn a_list_item_holding_a_div_is_still_reflowed() {
    assert_reflows_to(
        "- a\n\n  ::: note\n  x\n  :::\n\n  one two three four five six seven eight\n",
        20,
        &[Mode::Normalize],
        "- a\n\n  ::: note\n  x\n  :::\n\n  one two three four\n  five six seven\n  eight\n",
    );
}

/// A tab after a list marker reaches the next tab stop, so `-\t` puts the
/// item's content at column four. A line after a blank indented less than
/// that is not in the item, and markdown-rs, which starts the content one
/// column past the marker, reads the item differently again. Re-spacing the
/// marker to `- ` moved such a line into the item for every renderer.
#[test]
fn a_tab_padded_list_marker_keeps_lines_short_of_its_content_out_of_the_item() {
    let items = [
        "-\ta\n\n  x\n",
        "-\ta\n  b\n\n  x\n",
        "-\t{\n  :::\n\n  `",
        " -\ta\n\n   x\n",
        "1.\ta\n\n   x\n",
        "-\t`\n  `\n",
        "-\ta\n\n\t\tcode\n",
    ];
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A tab-padded item is still reflowed, its continuation lines placed at the
/// column its content starts on. A line short of that column continues the
/// paragraph lazily and is reflowed with it.
#[test]
fn a_tab_padded_list_item_is_still_reflowed_at_its_content_column() {
    assert_reflows_to(
        "-\ta b c d e f g h i j\n    k l m n o p\n",
        10,
        &[Mode::Normalize],
        "-\ta b c\n    d e f\n    g h i\n    j k l\n    m n o\n    p\n",
    );
    assert_reflows_to(
        "-\ta b c d e f g h i j\n  k l m\n",
        10,
        &[Mode::Normalize],
        "-\ta b c\n    d e f\n    g h i\n    j k l\n    m\n",
    );
}

/// A line kept on its own in a list item is re-indented with spaces, and
/// implementations disagree on how much of a tab the item's indentation
/// consumes. Re-indenting one written with a tab moved code out of the item
/// or changed what a fence or table holds, so such an item is left as written.
#[test]
fn a_list_item_holding_a_line_indented_with_a_tab_is_left_as_written() {
    let items = [
        "- a b c d e f g h i j\n\n\t\tcode\n",
        "-\ta b c d e f g h i j\n\n\t\tcode\n",
        "- a b c d e f g h i j\n\n  \t```\n  \tx\n  \t```\n",
        "- a b c d e f g h i j\n\n  \t| a | b |\n  \t| - | - |\n",
        "- a b c d e f g h i j\n      \tk\n",
    ];
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                let output = reflow(input, &settings).expect("reflow runs");
                if output != input {
                    violations.push(format!("{settings:?} {input:?}: rewritten to {output:?}"));
                }
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A code span can run across a `:::` line or a `NOTE:` line, which makes the
/// line text of the paragraph the span is in. Keeping such a line on its own
/// as a div marker or a semantic line re-indented the lines after it, which
/// changed the span's text. pulldown-cmark, which rumdl parses with, reads a
/// colon touching its text after a paragraph line as a definition, ending the
/// span there, so the context reports no span at all for those; the item is
/// left as written.
#[test]
fn a_code_span_crossing_a_div_marker_or_semantic_line_keeps_its_text() {
    let items = [
        "-\tD\n    :::`\n     nge}`;\n",
        "- D\n  :::`\n     b` c\n",
        "- D\n  a `b\n  :::\n     c` d\n",
        "-\tD\n    a `b\n    ::: c\n     d` e\n",
        "- D\n  a `b\n  :x\n     c` d\n",
        "a `b\n:::\n c` d\n",
        "a `b\n:x\n c` d\n",
        "- D\n  a `b\n  NOTE: c\n     d` e\n",
        "-\tD\n    a `b\n    NOTE: c` d\n       e\n",
        "- D\n  x\n  NOTE: `a\n     b` c\n",
        "- D\n  NOTE: `a\n   b\n     c` d\n",
    ];
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// A `:::` line no code span crosses still separates the paragraphs around
/// it, which are reflowed on their own.
#[test]
fn a_div_marker_no_code_span_crosses_still_separates_reflowed_paragraphs() {
    assert_reflows_to(
        "::: note\none two three four five six seven eight\n:::\n",
        20,
        &[Mode::Normalize],
        "::: note\none two three four\nfive six seven eight\n:::\n",
    );
    assert_reflows_to(
        "- a\n\n  ::: note\n  one two three four five six seven eight\n  :::\n",
        20,
        &[Mode::Normalize],
        "- a\n\n  ::: note\n  one two three four\n  five six seven\n  eight\n  :::\n",
    );
}

/// A `:::` line a code span runs out of is the first line of a paragraph,
/// not a fence, so the paragraph is reflowed like any other.
#[test]
fn a_div_marker_a_code_span_runs_out_of_is_reflowed_as_paragraph_text() {
    assert_reflows_to(
        "::: x `a\nb` c d e f g h i j\n",
        80,
        &[Mode::Normalize, Mode::SemanticLineBreaks],
        "::: x `a b` c d e f g h i j\n",
    );
    assert_reflows_to(
        "::: x `a\nb` c d e f g h i j\n",
        20,
        &[Mode::Normalize],
        "::: x `a b` c d e f\ng h i j\n",
    );
}

/// In Quarto a `:::` line is a fence that ends the paragraph before it, as
/// the parser's definition does, so a backtick before it opens no span
/// across it and the paragraphs on both sides are reflowed.
#[test]
fn a_quarto_div_fence_after_a_backtick_still_separates_reflowed_paragraphs() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::new(20),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    });
    let content = "::: note\none two `three\n:::\nfour` five six seven eight\n";
    assert_eq!(
        rule.fix(&LintContext::new(content, MarkdownFlavor::Quarto, None))
            .unwrap(),
        "::: note\none two `three\n:::\nfour` five six seven\neight\n"
    );
}

/// An indented code block cannot interrupt a paragraph, so a line under an
/// item's prose indented four or more columns past its content continues
/// the paragraph. Kept on its own as code, it hid a code span crossing into
/// it, and the lines above were joined while it kept its indentation, which
/// changed the span's text.
#[test]
fn a_deeply_indented_line_under_item_prose_continues_its_paragraph() {
    let items = [
        "-\tD\n    `a\n     b\n       c` d\n",
        "-\tD\n    x `a\n     b\n       c` d\n",
        "-\tD\n    a `b\n    c\n       d` e\n",
        "-\tD\n    a `b\n    NOTE: c\n       d` e\n",
        "-\tD\n    NOTE: `a\n     b\n       c` d\n",
        "-   D\n    `a\n     b\n            c` d\n",
        "- D\n  `a\n   b\n         c` d\n",
    ];
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
    assert_reflows_to(
        "-\tD\n    one two three four five\n       six seven\n",
        80,
        &[Mode::Normalize],
        "-\tD one two three four five six seven\n",
    );
    assert_reflows_to(
        "- one two three\n          four five\n",
        80,
        &[Mode::Normalize],
        "- one two three four five\n",
    );
}

/// A line a code span crosses into is text of the paragraph that span is
/// in, whatever block its text alone would open. Kept on its own as a
/// table row, heading or fence, it held text the indentation strip had
/// only partly removed and was written back past the item's full
/// indentation, so the line moved further right on every pass.
#[test]
fn a_line_a_code_span_crosses_into_is_paragraph_text_whatever_it_would_open() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD013LineLength;

    let items = [
        "- > a `b\n   | c` | d |\n",
        "- > a `b\n      # c` d\n",
        "- > a `b\n   ```c` d\n",
        "- > a `b\n   NOTE: c` d\n",
        "- > `b\n  ```c` d\n",
        "- > `b\n  | c` | d |\n",
    ];
    for input in items {
        for mode in [Mode::Normalize, Mode::SemanticLineBreaks] {
            let config = ReflowSettings::with_mode(mode, 10).config();
            let rule = MD013LineLength::from_config(&config);
            let once = rule
                .fix(&LintContext::new(input, MarkdownFlavor::Standard, None))
                .unwrap();
            let twice = rule
                .fix(&LintContext::new(&once, MarkdownFlavor::Standard, None))
                .unwrap();
            assert_eq!(once, twice, "{mode:?}, input {input:?}");
        }
    }
    let mut violations = Vec::new();
    for input in items {
        for mode in REFLOW_MODES {
            for line_length in [0, 10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    violations.push(format!("{settings:?} {input:?}: {}", violation.label()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "{} violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// An item's text can open containers of its own (`- > quote`, `- - item`).
/// When the first word after their markers did not fit, the line broke
/// right after the markers, which opened the containers empty and left
/// the text outside them.
#[test]
fn the_first_word_of_an_item_stays_with_the_container_markers_before_it() {
    const WRAPPING_MODES: [Mode; 2] = [Mode::Normalize, Mode::SemanticLineBreaks];
    assert_reflows_to("- > abcdefghijkl mn\n", 10, &WRAPPING_MODES, "- > abcdefghijkl\n  mn\n");
    assert_reflows_to("- - abcdefghijkl mn\n", 10, &WRAPPING_MODES, "- - abcdefghijkl\n  mn\n");
    assert_reflows_to(
        "1. > abcdefghijkl mn\n",
        10,
        &WRAPPING_MODES,
        "1. > abcdefghijkl\n   mn\n",
    );
    assert_reflows_to(
        "- > - abcdefghijkl mn\n",
        10,
        &WRAPPING_MODES,
        "- > - abcdefghijkl\n  mn\n",
    );
    assert_reflows_to(
        "- > ab cdefghijklmnop\n",
        10,
        &WRAPPING_MODES,
        "- > ab\n  cdefghijklmnop\n",
    );
}

/// A code span crossing out of an item's last line runs on into a line the
/// item does not collect, so its paragraph continues past where the item
/// ends. Reflowing the part inside the item joined the lines the span
/// crosses and rewrote the whitespace it holds.
#[test]
fn a_code_span_running_on_past_the_end_of_an_item_leaves_the_item_as_written() {
    for input in [
        "- > a `b\n    x\n::: c` d\n",
        "- > a `b\n    x\n ::: c` d\n",
        "- > a `b\n> c` d\n",
    ] {
        assert_reflows_to(input, 80, &REFLOW_MODES, input);
    }
    assert_reflows_to(
        "- > a b\n    x\n::: c d\n",
        80,
        &[Mode::Normalize],
        "- > a b x\n::: c d\n",
    );
}

/// A tab after a list marker reaches the next tab stop, so `-\t` puts the
/// item's content at column 4, and a line indented past it under a paragraph
/// is a block of the item (a quote, a heading) when it opens one.
#[test]
fn a_tab_padded_item_keeps_the_blocks_a_later_line_opens() {
    for input in [
        "-\t`x.  E\n      > H` j\n",
        "1.\t`x.  E\n       > H` j\n",
        "-\t- w w w w `x.  E  F  G\n        > H` j\n",
        "-\t>\tw w w w `x.  E  F  G\nH` j\n",
    ] {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?}: {}", violation.label());
                }
            }
        }
    }
    assert_reflows_to(
        "-\t`x.  E` w w w w w w w\n      > H j\n",
        10,
        &[Mode::SemanticLineBreaks],
        "-\t`x.  E`\n  w w w w\n  w w w\n      > H\n      > j\n",
    );
}

/// Semantic line breaks keep the indent the author gave an item's paragraph
/// text, and write every paragraph of the item at it. A continuation line can
/// sit any distance past the content, but a paragraph after a blank line
/// indented four or more past it is a code block, so such a line sets no
/// indent and the paragraph after the blank line stays a paragraph.
#[test]
fn a_deep_continuation_line_sets_no_indent_for_the_paragraphs_after_it() {
    for input in [
        "- a b c d e f g\n      h i\n\n  j k l m n o\n",
        "- a b c d e f g\n        h i\n\n    j k l m n o\n",
        "- **a b** c d e f g\n        h i\n\n    j k l m n o\n",
        "1. a b c d e f g\n          h i\n\n   j k l m n o\n",
        "-\ta b c d e f g\n          h i\n\n    j k l m n o\n",
        "> - a b c d e f g\n>         h i\n>\n>   j k l m n o\n",
    ] {
        for mode in REFLOW_MODES {
            for line_length in [10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, line_length);
                if let Err(violation) = check(input, &settings) {
                    panic!("{settings:?} {input:?}: {}", violation.label());
                }
            }
        }
    }
    // The later paragraph's own indent is kept for the whole item.
    assert_reflows_to(
        "- a b c d e f g\n        h i\n\n    j k l m n o\n",
        10,
        &[Mode::SemanticLineBreaks],
        "- a b c\n    d e f\n    g h i\n\n    j k l\n    m n o\n",
    );
    // An indent short of the code threshold is still kept.
    assert_reflows_to(
        "- a b c d e f g\n     h i\n\n  j k l m n o\n",
        10,
        &[Mode::SemanticLineBreaks],
        "- a b c\n     d e f\n     g h i\n\n     j k l\n     m n o\n",
    );
}
