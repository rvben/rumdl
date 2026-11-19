//! A table indented under a list item belongs to that item whatever kind of
//! line the item's paragraph ended with. A line that continues the paragraph
//! without the item's indent (a lazy continuation) keeps the item open, so a
//! table after it is still a list table and MD056 checks its rows.
//!
//! The oracle is pulldown-cmark with tables enabled: a document is expected to
//! be flagged by MD056 exactly when the parser reads a table in it. Every
//! document ends with a row of three cells under a two-cell header, so a
//! detected table is always flagged.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;

const TABLE: &str = "| a | b |\n| - | - |\n| 1 | 2 | 3 |\n";

fn indented(table: &str, indent: &str) -> String {
    table.lines().map(|line| format!("{indent}{line}\n")).collect()
}

/// Whether pulldown-cmark reads a table anywhere in `content`.
fn parser_reads_a_table(content: &str) -> bool {
    Parser::new_ext(content, Options::ENABLE_TABLES).any(|event| matches!(event, Event::Start(Tag::Table(_))))
}

/// The lines MD056 flags in `content`.
fn md056_lines(content: &str) -> Vec<usize> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "check",
            "--no-cache",
            "--isolated",
            "--disable",
            "MD041",
            "--enable",
            "MD056",
            "doc.md",
        ])
        .output()
        .expect("run rumdl check");
    // 0 is a clean run and 1 reports findings; anything else (a panic, a usage
    // error) would read as a document with nothing to flag.
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "rumdl check failed on {content:?}: {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("[MD056]"))
        .filter_map(|line| line.split(':').nth(1)?.parse().ok())
        .collect()
}

/// A document that holds a list item, a line that ends its paragraph in some
/// way, and a table.
struct Case {
    name: &'static str,
    content: String,
}

fn case(name: &'static str, head: &str, indent: &str) -> Case {
    Case {
        name,
        content: format!("{head}{}", indented(TABLE, indent)),
    }
}

/// Tables that belong to a list item (or, for the last group, to no item).
fn cases() -> Vec<Case> {
    vec![
        // The lazy line is the case under test: it is text, a code span, or text
        // that is indented but short of the content column.
        case("lazy plain line", "- Item\ncode\n\n", "  "),
        case("lazy code span line", "- Item\n`code`\n\n", "  "),
        case("aligned code span line", "- Item\n  `code`\n\n", "  "),
        case("lazy line short of the content column", "- Item\n code\n\n", "  "),
        case("two lazy lines", "- Item\nlazy one\nlazy two\n\n", "  "),
        case("lazy line, table right below", "- Item\nlazy\n", "  "),
        case("lazy line in a nested item", "- a\n  - b\nlazy\n\n", "    "),
        case("lazy line in an ordered item", "1. Item\nlazy\n\n", "   "),
        case("lazy line in a quoted list", "> - Item\n> lazy\n>\n", "  "),
        case(
            "lazy line after an item's own lazy text",
            "- Item\n  more\nlazy\n\n",
            "  ",
        ),
        // A line that ends the list: the table after it is not the item's, and
        // the parser decides whether it is a table at all (an indented code
        // block, or a table in the document).
        case(
            "paragraph after a blank line ends the list",
            "- Item\n\ntext\n\n",
            "    ",
        ),
        case(
            "paragraph after a blank line, table at the margin",
            "- Item\n\ntext\n\n",
            "",
        ),
        case(
            "lazy line, then a blank, then a paragraph",
            "- Item\nlazy\n\ntext\n\n",
            "    ",
        ),
        case("thematic break ends the list", "- Item\n***\n\n", "    "),
        case("heading ends the list", "- Item\n# Title\n\n", "    "),
        case("table too deep to be a table", "- Item\nlazy\n\n", "      "),
    ]
}

/// Prefixes a quoted case's table rows with the quote marker.
fn quoted(case: Case) -> Case {
    if case.name != "lazy line in a quoted list" {
        return case;
    }
    let content = case
        .content
        .lines()
        .map(|line| {
            if line.starts_with('>') {
                format!("{line}\n")
            } else {
                format!("> {line}\n")
            }
        })
        .collect();
    Case { content, ..case }
}

#[test]
fn a_table_is_found_exactly_when_the_parser_reads_one() {
    let mut failures = Vec::new();
    for case in cases().into_iter().map(quoted) {
        let expected = parser_reads_a_table(&case.content);
        let flagged = !md056_lines(&case.content).is_empty();
        if flagged != expected {
            failures.push(format!(
                "{}: parser reads a table: {expected}, MD056 flags it: {flagged}\n{}",
                case.name, case.content
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
fn the_cases_include_documents_with_and_without_a_table() {
    // Positive and negative controls for the oracle itself.
    let all: Vec<_> = cases().into_iter().map(quoted).collect();
    let with = all.iter().filter(|c| parser_reads_a_table(&c.content)).count();
    assert!(
        with >= 10 && with < all.len(),
        "{with} of {} cases hold a table",
        all.len()
    );
}

#[test]
fn the_issue_row_is_flagged_for_all_three_twins() {
    for head in ["- Item\n`code`\n\n", "- Item\ncode\n\n", "- Item\n  `code`\n\n"] {
        let content = format!("{head}{}", indented(TABLE, "  "));
        assert_eq!(md056_lines(&content), vec![6], "{content:?}");
    }
}

/// The parser's reading of `content` with tables, whitespace ignored: every
/// event, with text trimmed and empty text dropped, so reformatting a table's
/// padding does not count as a change. Paragraph events are dropped too: the
/// blank line MD058 puts before a table makes a tight item's text a paragraph,
/// which changes how the item renders but not what it holds.
fn outline(content: &str) -> Vec<String> {
    Parser::new_ext(content, Options::ENABLE_TABLES)
        .filter_map(|event| match event {
            Event::Text(text) => {
                let text = text.trim();
                (!text.is_empty()).then(|| format!("text {text}"))
            }
            // Inline code keeps its literal content: only padding around text
            // may change, never what a span holds or whether one exists.
            Event::Code(text) => Some(format!("code {text}")),
            Event::SoftBreak | Event::HardBreak => None,
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => None,
            other => Some(format!("{other:?}")),
        })
        .collect()
}

/// One `rumdl fmt` run with every table rule on, over the default rules.
fn format_tables(content: &str) -> String {
    format_tables_with(content, &[])
}

/// `format_tables` with more command-line arguments.
fn format_tables_with(content: &str, extra: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "fmt",
            "--no-cache",
            "--isolated",
            "--disable",
            "MD041",
            "--enable",
            "MD055,MD056,MD058,MD060,MD075",
        ])
        .args(extra)
        .arg("doc.md")
        .output()
        .expect("run rumdl fmt");
    // 0 is a clean run and 1 reports what could not be fixed; anything else
    // (a panic, a usage error) would leave the file as it was and read as a
    // formatter that has nothing to change.
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "rumdl fmt failed on {content:?}: {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(&path).unwrap()
}

#[test]
fn formatting_keeps_the_documents_structure_and_reaches_a_fixed_point() {
    for case in cases().into_iter().map(quoted) {
        let once = format_tables(&case.content);
        assert_eq!(
            outline(&case.content),
            outline(&once),
            "{}: formatting changed the structure\n{:?}\n{once:?}",
            case.name,
            case.content
        );
        assert_eq!(once, format_tables(&once), "{}: not a fixed point", case.name);
    }
}

/// MD060 rewrites a well-formed table that is not padded; the rewrite keeps the
/// table in its item, and a second run leaves it alone.
#[test]
fn a_table_reached_through_a_lazy_line_is_rewritten_in_place() {
    let compact = "| a | b |\n| - | - |\n| long | 2 |\n";
    let aligned = ["--config", "MD060.style=\"aligned\""];
    for (head, indent) in [
        ("- Item\nlazy\n\n", "  "),
        ("- Item\n`code`\n\n", "  "),
        ("- Item\nlazy\n", "  "),
        ("- a\n  - b\nlazy\n\n", "    "),
        ("1. Item\nlazy\n\n", "   "),
        ("- Item\nlazy one\nlazy two\n\n", "  "),
    ] {
        let content = format!("{head}{}", indented(compact, indent));
        let once = format_tables_with(&content, &aligned);
        assert_ne!(once, content, "MD060 should pad the table: {content:?}");
        assert_eq!(outline(&content), outline(&once), "{content:?} became {once:?}");
        assert_eq!(
            once,
            format_tables_with(&once, &aligned),
            "{content:?} is not a fixed point"
        );
        assert!(
            once.lines()
                .any(|line| line.starts_with(indent) && line.contains("| a    |")),
            "the table left its indent: {once:?}"
        );
    }
}

/// The line right after a list item's table is not a lazy continuation of
/// anything: a table is not a paragraph, so an unindented line there ends the
/// item. The table after it is then a table of its own, whose unindented row
/// MD056 still reads and flags.
#[test]
fn a_line_after_a_list_table_ends_the_item() {
    let content = "- | a | b |\n  | - | - |\noutside\n\n  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    assert!(parser_reads_a_table(content));
    assert_eq!(md056_lines(content), vec![7], "{content}");
    // Control: the same second table after an item that ended without a table.
    let control = "- Item\n\noutside\n\n  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    assert_eq!(md056_lines(control), vec![7]);
}

/// An item whose marker line opens a block that is not a paragraph (a heading,
/// a thematic break, indented code) cannot be continued lazily, so an
/// unindented line below it ends the item and the table after it belongs to
/// the document.
#[test]
fn a_marker_line_that_opens_another_block_ends_the_item_at_the_next_line() {
    let marker_lines = [
        "- # Heading",
        "1. # Heading",
        "- > # Heading",
        "- - # Heading",
        "- ***",
        "-     indented code",
        "- >",
        "- *** **",
    ];
    for marker_line in marker_lines {
        let content = format!("{marker_line}\noutside\n\n  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n");
        assert!(parser_reads_a_table(&content), "{content:?}");
        assert_eq!(md056_lines(&content), vec![6], "{content}");
    }
    // Control: a marker line holding a paragraph keeps the item open, so the
    // same table is the item's and the row is checked in it as well.
    let control = "- text\noutside\n\n  | a | b |\n  | - | - |\n  | 1 | 2 | 3 |\n";
    assert_eq!(md056_lines(control), vec![6]);
}

/// Lines the paragraph reading takes for paragraph text but the parser does not
/// keep in the item: a Setext underline closes the heading it underlines, and
/// a numbered line that cannot interrupt a paragraph is text of an earlier
/// paragraph, not an item of the list above. The unindented line after either
/// ends the item, so the table below it belongs to the document.
#[test]
fn a_line_that_only_looks_like_paragraph_text_still_ends_the_item() {
    let table = "  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    let setext = format!("- text\n  ===\noutside\n\n{table}");
    assert!(parser_reads_a_table(&setext));
    assert_eq!(md056_lines(&setext), vec![7], "{setext}");
    let invented = "- real\n\npara\n2. text\noutside\n\n   | a | b |\n   | - | - |\n| 1 | 2 | 3 |\n";
    assert!(parser_reads_a_table(invented));
    assert_eq!(md056_lines(invented), vec![9], "{invented}");
}

/// A lazy line may omit the blockquote marker as well as the item's indent; the
/// quoted item stays open, and the table below it is the item's.
#[test]
fn a_lazy_line_without_the_quote_marker_keeps_the_quoted_item_open() {
    let table = ">   | a | b |\n>   | - | - |\n>   | 1 | 2 | 3 |\n";
    for head in ["> - Item\nlazy\n>\n", "> - Item\n> lazy\n>\n", "> 1. Item\nlazy\n>\n"] {
        let indent = if head.contains("1.") { ">    " } else { ">   " };
        let table = table.replace(">   ", indent);
        let content = format!("{head}{table}");
        assert!(parser_reads_a_table(&content), "{content:?}");
        assert_eq!(md056_lines(&content), vec![6], "{content}");
    }
    // Control: a blank line without the marker ends the quote and its list.
    let ended = "> - Item\n\n  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    assert_eq!(md056_lines(ended), vec![5], "{ended}");
}

/// A block that ends the item without being a line of the table scanner's (a
/// comment, a fence) leaves nothing of the item behind: the indented table after
/// it is the document's, and its unindented row is flagged.
#[test]
fn a_comment_or_fence_after_a_lazy_line_ends_the_item() {
    let table = "  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    for block in ["<!-- hi -->\n", "```\nx\n```\n"] {
        let content = format!("- item\nlazy\n\n{block}\n{table}");
        let row = content.lines().count();
        assert!(parser_reads_a_table(&content), "{content:?}");
        assert_eq!(md056_lines(&content), vec![row], "{content}");
    }
}

/// The same after-a-table line, written as a code span: it opens with a
/// backtick, which is still paragraph text and not the start of a table.
#[test]
fn a_code_span_line_after_a_list_table_ends_the_item() {
    let content = "- | a | b |\n  | - | - |\n`outside`\n\n  | a | b |\n  | - | - |\n| 1 | 2 | 3 |\n";
    assert!(parser_reads_a_table(content));
    assert_eq!(md056_lines(content), vec![7], "{content}");
}
