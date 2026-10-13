//! `rumdl check --fix` must not change what a document means when a blockquote's
//! paragraph continues on a lazy line that is indented four or more columns past
//! the `>` marker.
//!
//! Inside a blockquote, a line indented four or more columns past the marker
//! cannot start a block (CommonMark 5.2, 4.4): after a paragraph it is plain
//! continuation text, so `>     ---` is text, not a thematic break or a setext
//! underline, and `>     |a|b|` is not a table row. Stripping that indent turns
//! the same characters into a heading, list, fence, break or table.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares the block structure, including thematic breaks and tables.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn structure(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for event in Parser::new_ext(content, Options::ENABLE_TABLES) {
        match event {
            Event::Start(Tag::List(first)) => tokens.push(format!("list:{first:?}")),
            Event::Start(Tag::Item) => tokens.push("item".into()),
            Event::Start(Tag::BlockQuote(_)) => tokens.push("quote".into()),
            Event::Start(Tag::Paragraph) => tokens.push("para".into()),
            Event::Start(Tag::Heading { level, .. }) => tokens.push(format!("heading:{}", level as u8)),
            Event::Start(Tag::Table(_)) => tokens.push("table".into()),
            Event::Start(Tag::CodeBlock(kind)) => {
                tokens.push(format!("code:fenced={}", matches!(kind, CodeBlockKind::Fenced(_))));
            }
            Event::Rule => tokens.push("rule".into()),
            Event::Html(_) => tokens.push("html".into()),
            Event::End(
                TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::BlockQuote(_)
                | TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Table
                | TagEnd::CodeBlock,
            ) => tokens.push("end".into()),
            Event::SoftBreak => tokens.push("soft".into()),
            _ => {}
        }
    }
    tokens
}

fn run_fix(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--fix", "--isolated"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

/// Lazy continuation lines (indent of 4 or more after the marker) whose text
/// would open a block if the indent were removed.
const LAZY_ROWS: &[&str] = &[
    // Setext underline, heading, list, fence, break, table, html.
    "> o\n>     ---\n",
    "> o\n>     ***\n",
    "> o\n>     ___\n",
    "> o\n>     - - -\n",
    "> o\n>     * * *\n",
    ">o\n>     ---\n",
    ">o\n>     ***\n",
    "> o\n>     ===\n",
    "> o\n>     x\n>     ---\n",
    "> o\n>     x\n>     ===\n",
    "> o\n>     # x\n",
    "> o\n>     #  x\n",
    "> o\n>     ## x ##\n",
    "> o\n>     -  x\n",
    "> o\n>     -\n",
    "> o\n>     1.  x\n",
    "> o\n>     * x\n",
    "> o\n>     - [ ] x\n",
    "> o\n>     ```\n",
    "> o\n>     ```\n>     code\n>     ```\n",
    "> o\n>     <!-- c -->\n",
    "> o\n>     |a|b|\n>     |-|-|\n",
    "> o\n>     |a|b|\n>     |-|-|\n>     |1|2|\n",
    ">o\n>     |a|b|\n>     |-|-|\n",
    "> |a|b|\n>     |-|-|\n>     ---\n",
    "> |a|b|\n>     |-|-|\n>     # x\n",
    "> o\n>     |a|b|\n>     |-|-|\n>     ---\n",
    "> o\n>     ---\n>     x\n",
    // Tabs and deeper indents.
    "> o\n>\t\t---\n",
    "> o\n>\t\t```\n",
    "> o\n>\t\t|a|b|\n>\t\t|-|-|\n",
    "> o\n>      ---\n",
    "> o\n>        ***\n",
    // A quote marker inside the indent is text, not a nested quote.
    "> o\n>     > ---\n",
    "> o\n>     > # x\n",
    // Nested quotes.
    "> > o\n> >     ---\n",
    "> > o\n> >     # x\n",
    "> > o\n> >     |a|b|\n> >     |-|-|\n",
    "> > o\n> >     -  x\n",
    // Lazy line of a list item inside a quote.
    "> - a\n>     ---\n",
    "> - a\n>       ---\n",
    "> - a\n>       |a|b|\n>       |-|-|\n",
    "> - a\n>       # x\n",
    // A quote inside a list item.
    "- > o\n>     ---\n",
    "- > o\n  >     ---\n",
    "- > o\n  >     # x\n",
    "- > o\n  >     |a|b|\n  >     |-|-|\n",
    // A real block after the lazy lines.
    "> o\n>     ---\n>\n> # real\n",
    "> o\n>     # x\n>\n> ---\n",
];

/// The same rows under every rule that can touch a quote line.
const EXTRA_RULES: &[&[&str]] = &[
    &[],
    &["--enable", "MD027"],
    &["--enable", "MD065"],
    &["--enable", "MD058"],
    &["--enable", "MD027,MD065,MD058"],
    &["--enable", "MD040"],
];

#[test]
fn lazy_indented_lines_keep_their_structure_under_every_rule() {
    let mut failures = Vec::new();
    for extra in EXTRA_RULES {
        for source in LAZY_ROWS {
            let fixed = run_fix(source, extra);
            if structure(source) != structure(&fixed) {
                failures.push(format!("{extra:?} {source:?} -> {fixed:?}"));
            }
            let again = run_fix(&fixed, extra);
            if again != fixed {
                failures.push(format!(
                    "{extra:?} not idempotent: {source:?} -> {fixed:?} -> {again:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Rows the fix leaves byte for byte unchanged: the only thing a fix could do to
/// them is change the structure, and no blank line belongs around a lazy line.
#[test]
fn lazy_indented_thematic_break_and_table_text_is_left_alone() {
    for source in [
        "> o\n>     ---\n",
        "> o\n>     ***\n",
        ">o\n>     ***\n",
        "> o\n>     # x\n",
        "> o\n>     ```\n",
        "> o\n>     |a|b|\n>     |-|-|\n",
        "> o\n>     |a|b|\n>     |-|-|\n>     ---\n",
        "> > o\n> >     ---\n",
        "> - a\n>       ---\n",
    ] {
        assert_eq!(run_fix(source, &[]), source, "default rules changed {source:?}");
        assert_eq!(
            run_fix(source, &["--enable", "MD065,MD058"]),
            source,
            "MD065 and MD058 changed {source:?}"
        );
    }
}

#[test]
fn lazy_indented_prose_is_still_reduced_to_one_space() {
    assert_eq!(run_fix("> o\n>     more text\n", &[]), "> o\n> more text\n");
    assert_eq!(run_fix("> o\n>     a\n>     b\n", &[]), "> o\n> a\n> b\n");
    assert_eq!(run_fix("> > o\n> >     more\n", &[]), "> > o\n> > more\n");
}

#[test]
fn lazy_indented_line_is_still_reported() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, "> o\n>     ---\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated"])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("MD027"), "MD027 must still warn: {stdout}");
}

#[test]
fn real_thematic_breaks_in_quotes_are_still_found() {
    for (source, expected) in [
        ("> a\n>\n> ---\n> b\n", "> a\n>\n> ---\n>\n> b\n"),
        ("> a\n>\n>   ---\n> b\n", "> a\n>\n>   ---\n>\n> b\n"),
        ("> a\n>\n> ***\n> b\n", "> a\n>\n> ***\n>\n> b\n"),
        ("> > a\n> >\n> > ---\n> > b\n", "> > a\n> >\n> > ---\n> >\n> > b\n"),
    ] {
        assert_eq!(run_fix(source, &["--enable", "MD065"]), expected, "{source:?}");
    }
}

#[test]
fn real_tables_in_quotes_are_still_found() {
    for (source, expected) in [
        (
            "> intro\n> |a|b|\n> |-|-|\n> |1|2|\n> after\n",
            "> intro\n>\n> |a|b|\n> |-|-|\n> |1|2|\n> after\n",
        ),
        // The indented rows are text, and the table after them is a real one.
        (
            "> o\n>     |a|b|\n>     |-|-|\n> |c|d|\n> |-|-|\n",
            "> o\n>     |a|b|\n>     |-|-|\n>\n> |c|d|\n> |-|-|\n",
        ),
        // A delimiter row that is lazy text does not complete a header row.
        ("> o\n> |a|b|\n>     |-|-|\n", "> o\n> |a|b|\n>     |-|-|\n"),
    ] {
        let fixed = run_fix(source, &["--enable", "MD058"]);
        assert_eq!(fixed, expected, "{source:?}");
        assert_eq!(structure(&fixed), structure(source), "{source:?}");
    }
}

/// The MDX syntax tree with the source positions removed.
fn mdx_tree(content: &str) -> String {
    fn strip(node: &mut markdown::mdast::Node) {
        if let Some(position) = node.position_mut() {
            *position = markdown::unist::Position::new(1, 1, 0, 1, 1, 0);
        }
        if let Some(children) = node.children_mut() {
            children.iter_mut().for_each(strip);
        }
    }
    let mut options = markdown::ParseOptions::mdx();
    options.constructs.gfm_table = true;
    let mut root = markdown::to_mdast(content, &options).expect("MDX source");
    strip(&mut root);
    format!("{root:?}")
}

/// MDX has no indented code, so the lazy shapes keep their structure when the
/// indent goes, and MD027 fixes them as it did before the CommonMark guard.
#[test]
fn mdx_lazy_lines_are_fixed_when_the_mdx_structure_is_kept() {
    let shapes = [
        ("hr", "o\n    ***\nafter\n"),
        ("dash", "o\n    ---\nafter\n"),
        ("bullet", "o\n    -\n"),
        ("ordered", "o\n    1. x\n"),
        ("atx", "o\n    # x\n"),
        ("table", "o\n    |a|b|\n    |-|-|\n"),
        ("prose", "o\n    more\n"),
        ("fence", "o\n    ```\n    x\n    ```\n"),
    ];
    for (name, text) in shapes {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let quote: String = lines.iter().map(|line| format!("> {line}")).collect();
        let item: String = lines
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}{line}", if i == 0 { "- " } else { "  " }))
            .collect();
        for (place, source) in [("top", text.to_string()), ("quote", quote), ("item", item)] {
            let fixed = run_fix(&source, &["--flavor", "mdx", "--enable", "MD027"]);
            assert_eq!(mdx_tree(&fixed), mdx_tree(&source), "{name} {place}: {source:?}");
            // A quoted shape other than the fence is reported and fixed.
            if place == "quote" && name != "fence" {
                assert_ne!(fixed, source, "{name} {place}: the fix must apply to {source:?}");
            }
        }
    }
}

#[test]
fn mdx_fixes_that_change_the_mdx_structure_are_still_withheld() {
    // Inside a code span the continuation's indent is part of the value.
    for source in ["> `a\n>      b`\n", "> o `a\n>      b` c\n", "> > `a\n> >      b`\n"] {
        let fixed = run_fix(source, &["--flavor", "mdx", "--enable", "MD027"]);
        assert_eq!(mdx_tree(&fixed), mdx_tree(source), "{source:?}");
    }
}

/// Fixes are judged per innermost block, so one block quote holding many
/// withheld fixes and many kept ones resolves each fix on its own.
#[test]
fn mdx_judges_each_fix_in_a_large_block_quote_on_its_own() {
    let source = "> o\n>     ***\n> after\n>\n> `a\n>      b`\n>\n".repeat(60);
    let fixed = run_fix(&source, &["--flavor", "mdx", "--enable", "MD027"]);
    assert_eq!(mdx_tree(&fixed), mdx_tree(&source));
    assert_eq!(fixed.matches(">     ***").count(), 0, "the preserving fixes apply");
    assert_eq!(
        fixed.matches(">      b`").count(),
        60,
        "the structure-changing fixes are withheld"
    );
}

/// A paragraph holding more fixes than are told apart is kept or dropped whole,
/// so a run of withheld fixes in one paragraph stays cheap and unfixed.
#[test]
fn mdx_drops_a_paragraph_with_many_structure_changing_fixes_whole() {
    let source = "> `a\n>      b`\n".repeat(200);
    let fixed = run_fix(&source, &["--flavor", "mdx", "--enable", "MD027"]);
    assert_eq!(fixed, source);
}
