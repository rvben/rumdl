//! Regression tests: a setext underline cannot be a lazy continuation line.
//!
//! CommonMark 4.3 states it outright, and the consequence is structural. In
//!
//! ```markdown
//! > quote
//! Setup
//! =====
//! ```
//!
//! the blockquote's paragraph is still open, so `Setup` and `=====` are lazy
//! continuation lines and the whole thing renders as one blockquoted paragraph.
//! rumdl's parser read `Setup\n=====` as a heading, MD022 then inserted blank
//! lines around that phantom heading, and `rumdl fmt` split one blockquote into
//! a blockquote plus an `<h1>` plus a paragraph. The document rendered
//! differently after formatting, with no rule reporting anything unusual.
//!
//! The same shape hits every container that supports lazy continuation: nested
//! blockquotes, unordered and ordered list items, and a paragraph a blockquote
//! interrupts. The mirror image matters just as much: once the container's
//! paragraph is CLOSED - by a blank line, an empty `>`, a heading, a thematic
//! break, a fence, an HTML block or a table - the underline is not lazy and the
//! heading is real. Every "no heading" case here is paired with a closed-container
//! case that must still be a heading, so a parser that simply stopped seeing
//! setext headings fails instead of passing.
//!
//! Expectations were taken from pulldown-cmark, the parser rumdl itself uses.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::{HeadingStyle, LintContext};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Does the parser see a setext heading anywhere in `content`?
fn has_setext_heading(content: &str) -> bool {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    ctx.lines.iter().any(|line| {
        line.heading
            .as_ref()
            .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
    })
}

/// Run `rumdl fmt` with the default rule set and return the rewritten file.
fn fmt_default(dir: &Path, content: &str) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// The underline is lazy: no heading exists, in any container that supports
/// lazy continuation.
#[test]
fn lazy_underline_is_not_a_heading() {
    let cases = [
        ("blockquote, `=` underline", "> quote\nSetup\n=====\n"),
        ("blockquote, `---` underline", "> quote\nSetup\n---\n"),
        ("blockquote, `--` underline", "> quote\nSetup\n--\n"),
        (
            "blockquote, prose after the underline",
            "> quote\nSetup\n=====\nalpha bravo\n",
        ),
        ("blockquote, three lazy lines", "> quote\nmore\nSetup\n=====\n"),
        ("nested blockquote", "> > deep\nSetup\n=====\n"),
        ("unordered list item", "- item\nSetup\n=====\n"),
        ("ordered list item", "1. item\nSetup\n=====\n"),
        ("nested list item", "- a\n  - b\nSetup\n=====\n"),
        (
            "list item, indented text but lazy underline",
            "- item\n  Setup\n=====\n",
        ),
        ("list inside a blockquote", "> - item\nSetup\n=====\n"),
        ("blockquote interrupting a paragraph", "text\n> quote\nSetup\n=====\n"),
        ("delimiter row with no header above it", "> --- | ---\nSetup\n=====\n"),
        (
            "delimiter row written under prose",
            "> intro\n> --- | ---\nSetup\n=====\n",
        ),
        (
            "paragraph after a table a heading ended",
            "> a | b\n> --- | ---\n> # h\n> para\nSetup\n=====\n",
        ),
        (
            "paragraph after a table a blank line ended",
            "> a | b\n> --- | ---\n>\n> para\nSetup\n=====\n",
        ),
        (
            "paragraph reopened under a setext heading",
            "> quote\n> Setup\n> =====\n> Another\nUnder\n=====\n",
        ),
        ("inline tag opening no HTML block", "> text\n> <span>\nSetup\n=====\n"),
        (
            "inline tag mid-paragraph",
            "> text\n> <span>x</span> more\nSetup\n=====\n",
        ),
        // Containers nest in either order, and a paragraph inside one is open
        // whichever marker came first.
        ("list holding a quoted paragraph", "* > quote\nSetup\n=====\n"),
        ("quote holding a listed paragraph", "> * item\nSetup\n=====\n"),
        // Nine digits is the longest ordered marker CommonMark accepts, so this
        // is a list item and its paragraph is open.
        ("nine-digit ordered marker", "999999999. item\nSetup\n=====\n"),
        ("paren-delimited ordered marker", "10) item\nSetup\n=====\n"),
        // Indented code cannot interrupt a paragraph, so this line continues it.
        ("indented code under a paragraph", "> text\n>     code\nSetup\n=====\n"),
        // CommonMark 5.2 lets an ordered list interrupt a paragraph when it
        // starts at 1, and the start number is what counts: leading zeros and
        // the `)` delimiter do not change it. Each of these opens an item whose
        // paragraph then swallows the underline.
        ("ordered list at the document start", "2. item\nSetup\n=====\n"),
        ("`1.` under prose", "text\n1. item\nSetup\n=====\n"),
        ("`1)` under prose", "text\n1) item\nSetup\n=====\n"),
        ("zero-padded `01.` under prose", "text\n01. item\nSetup\n=====\n"),
        (
            "nine-digit start of 1 under prose",
            "text\n000000001. item\nSetup\n=====\n",
        ),
        // A bullet may interrupt a paragraph whatever its marker, since only the
        // ordered form carries a start number.
        ("bullet under prose", "text\n- item\nSetup\n=====\n"),
        // The interruption question is asked where the paragraph's own text
        // would be written, and this `2.` is outdented from the item holding
        // that paragraph. It interrupts nothing, so it opens a list of its own
        // (pulldown-cmark renders `<ol start="2">`) and that item's paragraph
        // swallows the run and the underline below it.
        (
            "marker outdented from the item holding the paragraph",
            "- item\n2. item\n  ===\nSetup\n=====\n",
        ),
        // Only the OUTERMOST marker faces the interruption question. `2.` opens
        // nothing under prose, but inside a container it sits at a block start
        // and opens an item whose paragraph swallows the underline. Paired with
        // the thematic-break rows in the test below, which are the same shape
        // with content that holds no paragraph.
        ("prose in a quoted item", "text\n> 2. item\nSetup\n=====\n"),
        ("prose in a nested item", "text\n- 2. item\nSetup\n=====\n"),
        ("prose in a quoted `1.` item", "text\n> 1. item\nSetup\n=====\n"),
        // A `=` or `-` run is the one line whose reading depends on what is open
        // above it: it underlines a paragraph it is written inside, and is
        // paragraph text of its own anywhere else. None of these runs is written
        // inside the container holding the open paragraph, so each is ordinary
        // lazy text that the paragraph runs straight through.
        ("`=` run above the lazy underline", "> quote\n=====\nSetup\n=====\n"),
        ("`=` run then more lazy prose", "> quote\n===\ntext\nSetup\n=====\n"),
        ("two stacked `=` runs", "> quote\n=\n=\nSetup\n=====\n"),
        ("short `--` run under a quoted paragraph", "> quote\n--\nSetup\n=====\n"),
        ("`=` run outdented from an item", "- item\n===\nSetup\n=====\n"),
        ("`=` run above a blockquote", "===\n> quote\nSetup\n=====\n"),
        // A run that opens a container starts that body's first paragraph rather
        // than closing an outer one, so the item it opens is what holds the
        // underline. These are the `prose in a ... item` rows above with the
        // item's content swapped for the ambiguous run, and the answer is the
        // same: whatever the run means, the item's paragraph is open.
        ("`=` run opening a quoted item", "text\n> 2. ===\nSetup\n=====\n"),
        ("`=` run opening a nested item", "text\n- 2. ===\nSetup\n=====\n"),
        ("`=` run opening a quoted `1.` item", "text\n> 1. ===\nSetup\n=====\n"),
        // Containers nest in either order, and it is the INNERMOST one a
        // continuation has to re-enter. Each run below reaches the list item's
        // content column while carrying no `>`, so it is written outside the
        // blockquote whose paragraph is open and underlines nothing.
        (
            "run inside an item but outside its quote",
            "- > quote\n  ===\nSetup\n=====\n",
        ),
        (
            "run outside a quote, indented text below",
            "- > quote\n  ===\n  Setup\n=====\n",
        ),
        (
            "run outside a quote, lazy text below",
            "- > quote\n  ===\ntext\nSetup\n=====\n",
        ),
        (
            "run outdented from a quoted item",
            "> quote\n> - item\n  ===\nSetup\n=====\n",
        ),
        // Here the run does underline the item's paragraph, and the text below
        // opens another one in the same item. The bare underline cannot reach
        // the item's content column, so it is lazy again.
        (
            "paragraph reopened inside a list item",
            "- item\n  ===\n  Setup\n=====\n",
        ),
        (
            "run at a nested item's parent column",
            "- item\n  - nested\n  ===\nSetup\n=====\n",
        ),
        // A blank line ends the paragraph, not the item: CommonMark 5.2 lets one
        // hold several blocks, so text indented to its content column below the
        // blank opens a second paragraph inside the item and the bare underline
        // is lazy again. Paired with `list closed by a blank line` in the test
        // below, whose text is NOT indented and so closes the list instead.
        (
            "paragraph reopened after a blank line",
            "- item\n\n  para\nSetup\n=====\n",
        ),
        (
            "two lines reopened after a blank line",
            "- item\n\n  para\n  more\nSetup\n=====\n",
        ),
        (
            "run reopening the paragraph after a blank line",
            "- item\n\n  ===\nSetup\n=====\n",
        ),
        (
            "run reopening a quoted item's paragraph",
            "- > quote\n\n  ===\nSetup\n=====\n",
        ),
        (
            "indented text under a reopening run",
            "- item\n\n  ===\n  Setup\n=====\n",
        ),
        (
            "ordered item reopened after a blank line",
            "1. item\n\n   text\nSetup\n=====\n",
        ),
        // The two containers are re-entered by different means, so the order
        // they nest in decides what a line carrying one of them re-enters. A
        // `>` written at column 0 does not reach a list item's content column,
        // so it closes the item and opens a blockquote of its own; the
        // paragraph THAT quote holds is what runs on into the underline.
        (
            "quote closing the item that held it",
            "- > quote\n> more\nSetup\n=====\n",
        ),
        (
            "run outdented from a quote that closed an item",
            "- > a\n> b\n  ===\nSetup\n=====\n",
        ),
        (
            "item closing the quote it is written under",
            "> quote\n- item\n\n  ===\nSetup\n=====\n",
        ),
        // Indenting re-enters a list item and nothing else, so a line indented
        // under `> - item` is outside the blockquote and lazily continues the
        // item's paragraph; repeating the `>` as well re-enters both.
        (
            "item indented under a quote it never re-enters",
            "> - item\n  para\nSetup\n=====\n",
        ),
        (
            "quoted item continued inside its quote",
            "> - item\n>   para\nSetup\n=====\n",
        ),
        (
            "quote re-entered inside the item holding it",
            "- > quote\n  > more\nSetup\n=====\n",
        ),
        // No row of a table is paragraph text, so an underline written as one
        // underlines nothing: the whole construct is the table's own body.
        ("table body row above the underline", "a | b\n--- | ---\nSetup\n=====\n"),
        ("prose row in a table", "a | b\n--- | ---\ntext\nSetup\n=====\n"),
        ("indented row in a table", "a | b\n--- | ---\n  Setup\n=====\n"),
        (
            "table whose header row is a delimiter row",
            "--- | ---\n--- | ---\nSetup\n=====\n",
        ),
        // GFM breaks a table at a line that opens a container, and the paragraph
        // that container holds then swallows the underline. Paired with the
        // table rows in the test below, which break it on a line holding no
        // paragraph at all and leave nothing open.
        (
            "table broken by a blockquote",
            "a | b\n--- | ---\n> quote\nSetup\n=====\n",
        ),
        (
            "table broken by a list item",
            "a | b\n--- | ---\n- item\nSetup\n=====\n",
        ),
        // A delimiter row cannot interrupt a paragraph, so this table is inside
        // the item's lazily continued paragraph rather than under it.
        (
            "delimiter row lazily continuing an item",
            "- item\na | b\n--- | ---\nSetup\n=====\n",
        ),
        // The line that opens the item starts a paragraph there whatever ran
        // into it, so the table opens inside the item and the run is a cell of
        // its body. Paired with `delimiter row lazily continuing an item` above,
        // where the same three lines open no item and no table opens either.
        (
            "table opened by the line that opens the item",
            "text\n- a | b\n  --- | ---\n  c | d\n  =====\n",
        ),
        // The same bytes as the myst row in
        // `a_structural_block_closes_the_container_paragraph`. Standard flavor
        // does not know the construct, so the line is lazy continuation text
        // and the quoted paragraph runs straight through it.
        ("myst comment, standard flavor", "> quote\n% a comment\nSetup\n=====\n"),
    ];

    for (label, content) in cases {
        assert!(
            !has_setext_heading(content),
            "{label}: the underline is a lazy continuation line, so no heading exists"
        );
    }
}

/// The container's paragraph is closed, so the underline is not lazy and the
/// heading is real. Positive control for the test above.
#[test]
fn underline_after_a_closed_container_is_a_heading() {
    let cases = [
        ("closed by a blank line", "> quote\n\nSetup\n=====\n"),
        ("closed by an empty `>` line", "> quote\n>\nSetup\n=====\n"),
        ("list closed by a blank line", "- item\n\nSetup\n=====\n"),
        // A blockquote asks a continuation to repeat its marker rather than to
        // reach a column, so indenting under one re-enters nothing: the run
        // starts a paragraph of its own outside the quote and underlines it.
        // The rows in the test above indent the same way under a list item,
        // where the marker does set a column.
        (
            "indented run under a closed blockquote",
            "> quote\n\n  ===\nSetup\n=====\n",
        ),
        // A blank line closes every blockquote and everything written inside
        // one, so an item the quote holds goes with it while an item holding
        // the quote survives. These are the `- > quote` rows in the test above
        // with the two containers written the other way round, and the answer
        // flips: nothing is left for the indented lines below to be inside.
        (
            "quoted item closed by a blank line",
            "> - item\n\n  para\nSetup\n=====\n",
        ),
        ("run under a closed quoted item", "> - item\n\n  ===\nSetup\n=====\n"),
        // The `>` on the second line closed the item, so the blank line below
        // leaves nothing at all open. Paired with `run reopening a quoted
        // item's paragraph` in the test above, the same document without that
        // second line, where the item does survive the blank.
        (
            "blank after a quote that closed its item",
            "- > quote\n> quote\n\n  para\nSetup\n=====\n",
        ),
        (
            "run after a quote that closed its item",
            "- > quote\n> quote\n\n  ===\nSetup\n=====\n",
        ),
        ("ATX heading in the blockquote", "> # heading\nSetup\n=====\n"),
        ("thematic break in the blockquote", "> ---\nSetup\n=====\n"),
        ("`***` thematic break in the blockquote", "> ***\nSetup\n=====\n"),
        (
            "`___` thematic break under a paragraph",
            "> text\n> ___\nSetup\n=====\n",
        ),
        ("fenced code in the blockquote", "> ```\n> code\n> ```\nSetup\n=====\n"),
        ("HTML block in the blockquote", "> <div>\nSetup\n=====\n"),
        (
            "block-level tag interrupting a paragraph",
            "> text\n> <div>\nSetup\n=====\n",
        ),
        // CommonMark start conditions 2 to 5: no tag names an HTML block, and
        // each of these interrupts a paragraph like a block-level tag does.
        ("HTML comment", "> text\n> <!-- c -->\nSetup\n=====\n"),
        ("document type declaration", "> text\n> <!DOCTYPE html>\nSetup\n=====\n"),
        ("processing instruction", "> text\n> <?php ?>\nSetup\n=====\n"),
        ("CDATA section", "> text\n> <![CDATA[x]]>\nSetup\n=====\n"),
        ("table header in the blockquote", "> a | b\n> --- | ---\nSetup\n=====\n"),
        ("table with a body row", "> a | b\n> --- | ---\n> c | d\nSetup\n=====\n"),
        (
            "table body row holding no pipe",
            "> a | b\n> --- | ---\n> text\n> c | d\nSetup\n=====\n",
        ),
        ("table in a list item", "- a | b\n  --- | ---\n  c | d\nSetup\n=====\n"),
        // A blank line ends a table as it ends a paragraph, so the lines below
        // it are a paragraph of their own and the underline completes a heading
        // of that. Paired with the table rows in the test above, where nothing
        // ends the table and the run is read as one more of its cells.
        ("blank line after a delimiter row", "a | b\n--- | ---\n\nSetup\n=====\n"),
        (
            "blank line after a body row",
            "a | b\n--- | ---\na | b\n\nSetup\n=====\n",
        ),
        // GFM matches the two opening rows cell for cell, so a delimiter row
        // holding a different number of cells opens no table at all: every line
        // here is one paragraph, and the run underlines the whole of it.
        (
            "delimiter row narrower than its header row",
            "a | b\n\na | b | c\n--- | ---\nSetup\n=====\n",
        ),
        (
            "body rows under a delimiter row that opened nothing",
            "a | b | c\n--- | ---\na | b\na | b | c\nSetup\n=====\n",
        ),
        // A table's header row has to start its own paragraph: a paragraph
        // already running into the row holds it as text, so no table opens and
        // the underline belongs to that paragraph.
        ("table header row under prose", "text\na | b\n--- | ---\nSetup\n=====\n"),
        (
            "delimiter rows under prose",
            "text\n--- | ---\n--- | ---\nSetup\n=====\n",
        ),
        // A blank line closes every blockquote, so the quoted lines below it
        // open a fresh one. Its item is gone with it, which leaves the run
        // written in the quote's own body rather than lazily outside an item,
        // and the quoted paragraph it underlines ends there.
        (
            "quoted item closed by a blank line, run in the quote",
            "> - a\n\n>    b\n> ===\ntext\n=====\n",
        ),
        // GFM breaks a table at the first line that starts another block-level
        // structure, and neither of these holds a paragraph, so nothing is left
        // open. The rows in the test above break the same table on a container,
        // whose paragraph then swallows the underline.
        (
            "table broken by an ATX heading",
            "a | b\n--- | ---\n# head\nSetup\n=====\n",
        ),
        (
            "table broken by a thematic break",
            "a | b\n--- | ---\n---\nSetup\n=====\n",
        ),
        // Indentation opens no container, so nothing here asks the underline
        // below to reach a column. The paired rows in the test above indent
        // exactly as far under a list item, where a marker does ask.
        ("indented paragraph", "  Setup\n=====\n"),
        ("indented paragraph of two lines", "  para\n  more\nSetup\n=====\n"),
        ("indented `=` run over an indented line", "  ===\n  ===\nSetup\n=====\n"),
        // A blockquote marker is re-entry however little it is indented, and the
        // run under it underlines the quoted paragraph and closes it.
        ("unspaced blockquote marker", ">a\n>===\nSetup\n=====\n"),
        // The item's paragraph runs on through a line indented past its content
        // column, and the run reaching that column underlines the pair.
        (
            "underline over an indented continuation",
            "- item\n    text\n  ===\nSetup\n=====\n",
        ),
        (
            "quoted underline over an indented continuation",
            "> a\n>     b\n> ===\nSetup\n=====\n",
        ),
        // `---` under prose is a setext underline, not the delimiter row of a
        // one-column table, so the blockquote's paragraph is closed either way.
        (
            "`---` under prose that resembles a table row",
            "> a | b\n> ---\nSetup\n=====\n",
        ),
        // The quoted run repeats the blockquote's marker, so it underlines the
        // quoted paragraph and closes it. Nothing is left for `Another` to
        // continue, and its own underline completes a real heading. The row
        // below it in the test above is the same document with one more quoted
        // line, which reopens the paragraph and makes the underline lazy again.
        (
            "setext heading in the blockquote",
            "> quote\n> Setup\n> =====\nAnother\n=====\n",
        ),
        // The ambiguous run again, now written where it does underline the
        // paragraph above it. The run closes that paragraph, so nothing is left
        // open for the underline below to continue. Paired with the runs in the
        // test above, which are the identical construct written outside the
        // container whose paragraph is open.
        ("`=` run under prose", "text\n===\nSetup\n=====\n"),
        (
            "underline in an item, outdented text below",
            "- item\n  text\n  ===\nSetup\n=====\n",
        ),
        (
            "underline in an item, indented text below",
            "- item\n  text\n  ===\n  Setup\n  =====\n",
        ),
        // `---` is long enough to be a thematic break, which outranks the lazy
        // underline reading and interrupts the quoted paragraph. `--` is too
        // short for a break, so under prose it underlines the line above.
        ("`---` outdented from a blockquote", "> quote\n---\nSetup\n=====\n"),
        ("short `--` under prose", "text\n--\nSetup\n=====\n"),
        // The run opens a paragraph of its own, `2.` cannot interrupt one so it
        // continues that paragraph as prose, and the underline completes a
        // heading of the whole thing.
        (
            "`=` run above a non-interrupting marker",
            "===\n2. item\nSetup\n=====\n",
        ),
        ("ATX heading in the list item", "- # heading\nSetup\n=====\n"),
        // An empty container holds no paragraph, so there is nothing to continue.
        ("empty unordered item", "* \nSetup\n=====\n"),
        ("empty ordered item", "1. \nSetup\n=====\n"),
        ("empty item after a full one", "- item\n- \nSetup\n=====\n"),
        ("empty nested container", "> - \nSetup\n=====\n"),
        // Containers nest in either order, so the marker that comes first says
        // nothing about what the innermost one holds.
        ("list holding a quoted heading", "* > # nested\nSetup\n=====\n"),
        ("list holding a nested list heading", "* - # nested\nSetup\n=====\n"),
        ("quote holding a listed heading", "> * # nested\nSetup\n=====\n"),
        ("list holding a quoted thematic break", "- > ---\nSetup\n=====\n"),
        // CommonMark 5.2: a thematic break outranks a list item when a line
        // reads as both, so these close the paragraph rather than holding it.
        ("spaced `* * *` break", "> text\n> * * *\nSetup\n=====\n"),
        ("spaced `- - -` break", "> text\n> - - -\nSetup\n=====\n"),
        ("spaced `_ _ _` break", "> text\n> _ _ _\nSetup\n=====\n"),
        ("`* * *` opening the blockquote", "> * * *\nSetup\n=====\n"),
        // Ten digits is too many for an ordered marker, so the line is prose and
        // the underline belongs to it. rumdl reports the heading on its last
        // text line, where CommonMark's heading text spans both lines.
        ("ten-digit ordered marker", "9999999999. item\nSetup\n=====\n"),
        // CommonMark 5.2: an ordered list interrupts a paragraph only when it
        // starts at 1. Written under prose these markers open no item at all,
        // so the paragraph runs on and the underline completes a heading of it.
        // Paired with the interrupting markers in the test above.
        ("`2.` under prose", "text\n2. item\nSetup\n=====\n"),
        ("`3)` under prose", "text\n3) item\nSetup\n=====\n"),
        ("two non-interrupting markers", "text\n2. a\n3. b\nSetup\n=====\n"),
        ("start number zero", "text\n0. item\nSetup\n=====\n"),
        ("zero-padded start number zero", "text\n00. item\nSetup\n=====\n"),
        // A container's body is a fresh block start, so the `2.` inside one does
        // open an item, and the item holds a thematic break rather than a
        // paragraph. Nothing is left open and the heading is real. The same rows
        // written with prose inside are lazy, in the test above: swap the content
        // and the answer flips, which is what shows the inner marker is read.
        ("thematic break in a quoted item", "text\n> 2. ---\nSetup\n=====\n"),
        ("thematic break in a nested item", "text\n- 2. ---\nSetup\n=====\n"),
        ("thematic break in a quoted `1.` item", "text\n> 1. ---\nSetup\n=====\n"),
        ("thematic break in a nested `1.` item", "text\n- 1. ---\nSetup\n=====\n"),
        ("indented code opening the blockquote", ">     code\nSetup\n=====\n"),
        ("list item, underline indented to content", "- item\n  Setup\n  =====\n"),
        ("thematic break above", "---\nSetup\n=====\n"),
        ("plain paragraph above", "para\nSetup\n=====\n"),
        ("no container at all", "Setup\n=====\n\ntext\n"),
        ("no container, `---` underline", "Setup\n---\n\ntext\n"),
    ];

    for (label, content) in cases {
        assert!(
            has_setext_heading(content),
            "{label}: the paragraph above is closed, so this is a real setext heading"
        );
    }
}

/// A structural block written between the container and the underline closes
/// the container's paragraph, so the heading below it is real.
#[test]
fn a_structural_block_closes_the_container_paragraph() {
    let cases = [
        // rumdl recognises a `$$` display-math block in every flavor
        // (`compute_math_block_line_map` takes no flavor), so this reads the
        // same under standard as under the flavors that document math support.
        (
            "math block",
            "> quote\n$$\nx\n$$\nSetup\n=====\n",
            MarkdownFlavor::Standard,
        ),
        (
            "math block, quarto",
            "> quote\n$$\nx\n$$\nSetup\n=====\n",
            MarkdownFlavor::Quarto,
        ),
        (
            "pandoc div",
            "> quote\n::: note\nx\n:::\nSetup\n=====\n",
            MarkdownFlavor::Pandoc,
        ),
        (
            "mkdocs admonition",
            "> quote\n!!! note\n    x\nSetup\n=====\n",
            MarkdownFlavor::MkDocs,
        ),
        (
            "myst directive",
            "> quote\n```{note}\nx\n```\nSetup\n=====\n",
            MarkdownFlavor::MyST,
        ),
        (
            "front matter",
            "---\ntitle: x\n---\nSetup\n=====\n",
            MarkdownFlavor::Standard,
        ),
        (
            "fenced code",
            "> quote\n```\nx\n```\nSetup\n=====\n",
            MarkdownFlavor::Quarto,
        ),
        // myst-parser ends the quoted paragraph on a `%` comment and reports an
        // h1 here, so this one is a real heading. The standard-flavor row in
        // `lazy_underline_is_not_a_heading` runs the identical bytes through a
        // parser that does not know the construct, which is what shows the
        // flavor is doing the work rather than the shape.
        (
            "myst comment",
            "> quote\n% a comment\nSetup\n=====\n",
            MarkdownFlavor::MyST,
        ),
    ];

    for (label, content, flavor) in cases {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            ctx.lines.iter().any(|line| {
                line.heading
                    .as_ref()
                    .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
            }),
            "{label}: the block closed the container's paragraph, so this is a real heading"
        );
    }
}

/// Positive control for the test above: the walk compares the blocks the two
/// lines sit inside rather than testing one of them, so a container written
/// INSIDE such a block still holds its paragraph open across the underline.
#[test]
fn a_container_inside_a_structural_block_still_holds_its_paragraph() {
    let cases = [
        (
            "pandoc div",
            "::: note\n> quote\nSetup\n=====\n:::\n",
            MarkdownFlavor::Pandoc,
        ),
        (
            "mkdocs admonition",
            "!!! note\n    > quote\n    Setup\n    =====\n",
            MarkdownFlavor::MkDocs,
        ),
    ];

    for (label, content, flavor) in cases {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            ctx.lines.iter().all(|line| line.heading.is_none()),
            "{label}: the container's paragraph is open inside the block, so the underline is lazy"
        );
    }
}

/// A construct belongs on the boundary list only if the parser that defines it
/// ends the paragraph there, and kramdown's two extension constructs do not.
///
/// Both were added to the list on the strength of their absence from
/// `is_paragraph_context`, and kramdown itself refuses both readings. Its
/// extension block cannot interrupt a paragraph at all: it renders
/// `para\n{::comment}\nx\n{:/comment}\nSetup\n=====` as one paragraph holding
/// every line. Its block IAL does close the blockquote, and kramdown still
/// renders the pair below as a paragraph rather than a heading. Either way the
/// underline is not a heading, which is what this pins.
#[test]
fn kramdown_extension_constructs_do_not_end_the_quoted_paragraph() {
    let cases = [
        (
            "extension block",
            "> quote\n{::comment}\nx\n{:/comment}\nSetup\n=====\n",
        ),
        ("block IAL", "> quote\n{:.cls}\nSetup\n=====\n"),
    ];

    for (label, content) in cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);
        assert!(
            ctx.lines.iter().all(|line| line.heading.is_none()),
            "{label}: kramdown renders no heading here, so neither does rumdl"
        );
    }

    // Paired control: a blank line ends the quoted paragraph in kramdown too,
    // and there the heading is real. Without this the test above would pass on
    // a parser that had simply stopped seeing setext headings in kramdown.
    let ctx = LintContext::new("> quote\n{:.cls}\n\nSetup\n=====\n", MarkdownFlavor::Kramdown, None);
    assert!(
        ctx.lines.iter().any(|line| {
            line.heading
                .as_ref()
                .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
        }),
        "a blank line closes the paragraph, so this is a real heading"
    );
}

/// A lazy underline is paragraph text in every flavor: `>` and list markers mean
/// the same thing everywhere, so this is not a flavor-specific rule.
#[test]
fn lazy_underline_is_not_a_heading_in_any_flavor() {
    let content = "> quote\nSetup\n=====\n";
    for flavor in [
        MarkdownFlavor::Standard,
        MarkdownFlavor::MkDocs,
        MarkdownFlavor::MDX,
        MarkdownFlavor::Pandoc,
        MarkdownFlavor::Quarto,
        MarkdownFlavor::Obsidian,
        MarkdownFlavor::Kramdown,
        MarkdownFlavor::AzureDevOps,
        MarkdownFlavor::MyST,
        MarkdownFlavor::Hugo,
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            ctx.lines.iter().all(|line| line.heading.is_none()),
            "{flavor:?}: the underline is a lazy continuation line, so no heading exists"
        );
    }
}

/// The user-visible defect: `fmt` rewrote the document into a different one.
#[test]
fn fmt_preserves_a_lazy_underline() {
    let temp_dir = TempDir::new().unwrap();
    let cases = [
        ("blockquote", "> quote\nSetup\n=====\nalpha bravo\n"),
        ("nested blockquote", "> > deep\nSetup\n=====\n"),
        ("unordered list item", "- item\nSetup\n=====\n"),
        ("ordered list item", "1. item\nSetup\n=====\n"),
        ("list inside a blockquote", "> - item\nSetup\n=====\n"),
        ("blockquote interrupting a paragraph", "text\n> quote\nSetup\n=====\n"),
    ];

    for (label, content) in cases {
        assert_eq!(
            fmt_default(temp_dir.path(), content),
            content,
            "{label}: fmt must leave a lazy continuation line alone"
        );
    }
}

/// Positive control for the test above: `fmt` is running and does rewrite the
/// same shape once the blank line makes the heading real.
#[test]
fn fmt_still_formats_a_real_heading_in_the_same_shape() {
    let temp_dir = TempDir::new().unwrap();
    assert_eq!(
        fmt_default(temp_dir.path(), "> quote\n\nSetup\n=====\nalpha bravo\n"),
        "> quote\n\nSetup\n=====\n\nalpha bravo\n",
        "MD022 must still add the blank line under a real setext heading"
    );
}
