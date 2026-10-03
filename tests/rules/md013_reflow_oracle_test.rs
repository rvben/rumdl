//! Tests for the reflow semantics oracle shared with `fuzz_reflow_semantics`
//! and the corpus sweep in `md013_reflow_sweep`.
//!
//! An oracle that never fires is indistinguishable from a clean result, so
//! these tests prove each property can be observed failing, that the settings
//! reach MD013, and that correct reflow passes.

use super::reflow_semantics::{
    Mode, Outcome, ReflowSettings, Renderer, ViolationKind, check, check_with, normalize_html, reflow,
};

fn rendered(markdown: &str, cjk_join: bool) -> Vec<String> {
    Renderer::ALL
        .iter()
        .map(|renderer| normalize_html(&renderer.render(markdown), cjk_join))
        .collect()
}

#[test]
fn a_moved_line_break_renders_the_same() {
    assert_eq!(
        rendered("one two three\nfour five\n", false),
        rendered("one two\nthree four five\n", false)
    );
}

#[test]
fn a_hard_break_is_not_a_soft_break() {
    assert_ne!(
        rendered("one two  \nthree\n", false),
        rendered("one two\nthree\n", false)
    );
}

#[test]
fn wrapping_onto_a_heading_marker_changes_the_rendering() {
    assert_ne!(
        rendered("see issue # 5 here\n", false),
        rendered("see issue\n# 5 here\n", false)
    );
}

#[test]
fn wrapping_onto_a_list_marker_changes_the_rendering() {
    assert_ne!(
        rendered("the answer is\n1. not 3\n", false),
        rendered("the answer is 1.\nnot 3\n", false)
    );
    assert_ne!(rendered("a b - c\n", false), rendered("a b\n- c\n", false));
}

#[test]
fn a_changed_emphasis_span_changes_the_rendering() {
    assert_ne!(
        rendered("*one. two* three\n", false),
        rendered("*one.*\n*two* three\n", false)
    );
}

#[test]
fn a_lost_word_changes_the_rendering() {
    assert_ne!(rendered("one two three\n", false), rendered("one three\n", false));
}

#[test]
fn whitespace_inside_a_code_block_is_significant() {
    assert_ne!(rendered("```\na  b\n```\n", false), rendered("```\na b\n```\n", false));
}

#[test]
fn whitespace_inside_a_code_span_is_significant() {
    assert_ne!(rendered("run `a  b` now\n", false), rendered("run `a b` now\n", false));
    assert_eq!(rendered("run `a b` now\n", false), rendered("run `a\nb` now\n", false));
}

/// A paragraph drops the indentation of its continuation lines, and so does a
/// code span crossing into one under the spec, as comrak and cmark render it.
/// markdown-rs and pulldown-cmark keep it in the code. Joining the two lines
/// changes the code for one side or the other, so the oracle must hear both.
#[test]
fn renderers_disagree_on_the_indentation_a_code_span_keeps() {
    let indented = "run `a\n   b` now\n";
    let keeps = |renderer: Renderer| normalize_html(&renderer.render(indented), false).contains("<code>a    b</code>");
    assert!(keeps(Renderer::MarkdownRs));
    assert!(keeps(Renderer::PulldownCmark));
    assert!(normalize_html(&Renderer::Comrak.render(indented), false).contains("<code>a b</code>"));
    for joined in ["run `a    b` now\n", "run `a b` now\n"] {
        assert!(
            matches!(
                check_with(indented, &ReflowSettings::with_mode(Mode::Normalize, 80), |_, _| Ok(joined.to_string())),
                Err(violation) if matches!(violation.kind, ViolationKind::RenderChanged { .. })
            ),
            "{joined:?}"
        );
    }
}

/// A browser shows no whitespace at the start of a block. markdown-rs keeps
/// the tab indenting a later paragraph of a list item as the paragraph's first
/// character, where the other renderers drop it, and no reflow can write that
/// tab back.
#[test]
fn whitespace_at_the_edge_of_a_block_is_not_significant() {
    let tab_indented = "- one\n\n\ttwo\n";
    assert!(Renderer::MarkdownRs.render(tab_indented).contains("<p>\ttwo"));
    assert_eq!(rendered(tab_indented, false), rendered("- one\n\n  two\n", false));
    assert_eq!(
        normalize_html("<p> one <code> a </code> </p>\n<li> two </li>", false),
        "<p>one <code> a </code></p><li>two</li>"
    );
    assert_ne!(rendered("one two\n", false), rendered("onetwo\n", false));
}

/// CommonMark reads a tab after a container marker as reaching the next tab
/// stop, as cmark, pulldown-cmark, comrak and micromark do, so `-\t` puts the
/// item's content at column 4. markdown-rs 1.0 counts the tab as one column,
/// so the oracle gives it the tab as spaces.
#[test]
fn markdown_rs_reads_a_tab_after_a_container_marker_as_reaching_the_tab_stop() {
    for md in [
        "-\tw\n      > H j\n",
        "-\tfoo\n\n  bar\n",
        "1.\tw\n  # H\n",
        "-\t>\tone `a\n    two  b`\n",
        "- -\tw\n    > H\n",
    ] {
        let [markdown_rs, pulldown_cmark, comrak] =
            Renderer::ALL.map(|renderer| normalize_html(&renderer.render(md), false));
        assert_eq!(markdown_rs, pulldown_cmark, "{md:?}");
        assert_eq!(markdown_rs, comrak, "{md:?}");
    }
    // A tab inside a line's text is content, and stays one.
    assert!(Renderer::MarkdownRs.render("- a `\tb`\n").contains("<code>\tb</code>"));
}

#[test]
fn a_line_break_inside_raw_inline_code_html_is_a_space() {
    assert_eq!(
        rendered("see <code>impl\n!Unpin</code> now\n", false),
        rendered("see <code>impl !Unpin</code> now\n", false)
    );
    assert_eq!(
        rendered("see <code\nclass=\"x\">a</code> now\n", false),
        rendered("see <code class=\"x\">a</code> now\n", false)
    );
    assert_ne!(
        rendered("see <code>impl\n!Unpin</code> now\n", false),
        rendered("see <code>impl  !Unpin</code> now\n", false)
    );
}

#[test]
fn a_no_break_space_is_content() {
    assert_ne!(
        rendered("10\u{a0}000 euros\n", false),
        rendered("10 000 euros\n", false)
    );
}

#[test]
fn a_cjk_soft_break_joins_only_in_join_mode() {
    let broken = "日本語の\n文章です\n";
    let joined = "日本語の文章です\n";
    assert_eq!(rendered(broken, true), rendered(joined, true));
    assert_ne!(rendered(broken, false), rendered(joined, false));
}

#[test]
fn join_mode_keeps_a_space_the_author_wrote() {
    assert_ne!(
        rendered("日本語の 文章です\n", true),
        rendered("日本語の文章です\n", true)
    );
}

#[test]
fn the_settings_reach_md013() {
    let input = "First sentence here. Second sentence here.\n";
    let split = reflow(input, &ReflowSettings::with_mode(Mode::SentencePerLine, 0)).unwrap();
    assert_eq!(split, "First sentence here.\nSecond sentence here.\n");

    let long = "one two three four five six seven eight nine ten eleven twelve\n";
    let wrapped = reflow(long, &ReflowSettings::with_mode(Mode::Default, 20)).unwrap();
    assert!(
        wrapped.lines().count() > 1 && wrapped.lines().all(|line| line.len() <= 20),
        "line-length 20 should wrap the paragraph: {wrapped:?}"
    );
}

#[test]
fn correct_reflow_passes_in_every_mode() {
    let input = "This paragraph is long enough to be wrapped at forty columns. It has two \
                 sentences, a [link](https://example.com) and some *emphasis* in it.\n";
    for mode in Mode::ALL {
        let settings = ReflowSettings::with_mode(mode, 40);
        match check(input, &settings) {
            Ok(Outcome::Rewritten) => {}
            other => panic!("{mode:?}: expected a rewrite that preserves the document, got {other:?}"),
        }
    }
}

#[test]
fn a_short_paragraph_is_left_alone() {
    let settings = ReflowSettings::with_mode(Mode::Default, 80);
    assert_eq!(check("Short.\n", &settings), Ok(Outcome::Unchanged));
}

#[test]
fn every_byte_pair_decodes_to_a_configuration() {
    let mut modes = std::collections::HashSet::new();
    let mut line_lengths = std::collections::HashSet::new();
    for a in 0..=u8::MAX {
        for b in 0..8 {
            let settings = ReflowSettings::from_bytes(a, b);
            modes.insert(format!("{:?}", settings.mode));
            line_lengths.insert(settings.line_length);
        }
    }
    assert_eq!(modes.len(), Mode::ALL.len());
    assert_eq!(line_lengths.len(), 8);
}

/// markdown-rs opens a list at an ordered marker not numbered 1 that continues
/// a paragraph, when the paragraph is the first block after a list. CommonMark
/// keeps such a line in the paragraph, as cmark, pulldown-cmark and micromark
/// do, so the oracle renders it as paragraph text for markdown-rs too, while
/// real lists stay as they are.
#[test]
fn markdown_rs_reads_an_ordered_marker_continuing_a_paragraph_as_text() {
    let render = |md: &str| normalize_html(&Renderer::MarkdownRs.render(md), false);
    for (md, joined) in [
        (
            "* a\n\nWe see Chapter\n18. For now.\n",
            "* a\n\nWe see Chapter 18. For now.\n",
        ),
        (
            "1. a\n\nWe see Chapter\n2) For now.\n",
            "1. a\n\nWe see Chapter 2) For now.\n",
        ),
        (
            "> * a\n>\n> We see Chapter\n> 18. For now.\n",
            "> * a\n>\n> We see Chapter 18. For now.\n",
        ),
        (
            "- a\n\n  We see Chapter\n  18. For now.\n",
            "- a\n\n  We see Chapter 18. For now.\n",
        ),
    ] {
        assert_eq!(render(md), render(joined), "{md:?}");
        assert!(render(md).contains("<p>We see Chapter "), "{md:?}");
    }
    // Lists, and markers on a paragraph's first line, are untouched.
    assert!(render("* a\n\n18. item\n").contains("<ol start=\"18\">"));
    assert!(render("Text.\n\n18. item\n").contains("<ol start=\"18\">"));
    // A marker inside a code span is code, so it is left to markdown-rs: an
    // escape there would be a backslash in the code.
    assert!(!render("* a\n\nx `y\n18. z`\n").contains('\\'));
}

/// markdown-rs ends a list item before a lazy continuation line that starts
/// with `<` when that line ends the document without a line ending, and reads
/// the line as a paragraph of its own. CommonMark keeps it in the item's
/// paragraph, as cmark, pulldown-cmark and micromark do, so the oracle gives
/// markdown-rs a final line ending, which no renderer reads as content.
#[test]
fn markdown_rs_keeps_a_lazy_tag_line_at_the_end_of_the_document_in_its_item() {
    for md in ["- a\n<b", "- a\n<b c", "> - a\n> <b", "- a\nb\n<c"] {
        let [markdown_rs, pulldown_cmark, comrak] =
            Renderer::ALL.map(|renderer| normalize_html(&renderer.render(md), false));
        assert_eq!(markdown_rs, pulldown_cmark, "{md:?}");
        assert_eq!(markdown_rs, comrak, "{md:?}");
        assert!(!markdown_rs.contains("<p>"), "{md:?}: {markdown_rs}");
    }
    // The document's content is unchanged: an unclosed fence still holds
    // exactly its own lines.
    assert_eq!(
        normalize_html(&Renderer::MarkdownRs.render("```\nx"), false),
        normalize_html(&Renderer::MarkdownRs.render("```\nx\n"), false),
    );
}
