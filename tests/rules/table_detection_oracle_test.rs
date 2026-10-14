//! Differential tests of table confirmation against comrak (GFM as GitHub
//! renders it), over every document of 2 or 3 lines drawn from a vocabulary of
//! table-like shapes.
//!
//! A rewriting rule must never change the block structure comrak reads, and
//! every table block rumdl confirms must be a table comrak reads, apart from
//! shapes in which the table's own header or rows consist only of dashes and
//! pipes: there a row can be read as header, delimiter or body, and parsers
//! pick differently.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD055TablePipeStyle, MD058BlanksAroundTables, MD060TableFormat};

const VOCAB: &[&str] = &[
    "",
    "m",
    "a|b",
    "a|b|",
    "|a|b|",
    "|a|",
    "||",
    "|-|",
    "|-|-|",
    "-|-",
    "|---|---|",
    "-",
    "- x",
    "1. x",
    "> a|b",
    "> |-|",
    ">|-|",
    "> ",
    "  |a|",
    "  |-|",
    "    |a|",
    "    |-|",
    "\t|a|",
    "\t|-|",
    "  | a |",
    "  |---|",
    "|",
    "o|r  ",
    "x|",
    " |a|b|",
    "   |a|b|",
    " |-|-|",
    "   |-|-|",
    "2. |a|b|",
    ":warning:",
    "term",
];

fn documents() -> impl Iterator<Item = (Vec<&'static str>, String)> {
    let n = VOCAB.len();
    (2..=3u32).flat_map(move |len| {
        (0..n.pow(len)).map(move |k| {
            let mut index = k;
            let mut lines = Vec::new();
            for _ in 0..len {
                lines.push(VOCAB[index % n]);
                index /= n;
            }
            let doc = lines.join("\n") + "\n";
            (lines, doc)
        })
    })
}

fn comrak_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options
}

fn html(doc: &str) -> String {
    let out = comrak::markdown_to_html(doc, &comrak_options());
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn strip_paragraphs(html: &str) -> String {
    html.replace("<p>", "").replace("</p>", "").replace(" <", "<")
}

fn comrak_headers(doc: &str) -> Vec<usize> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, doc, &comrak_options());
    root.descendants()
        .filter(|node| matches!(node.data.borrow().value, comrak::nodes::NodeValue::Table(_)))
        .map(|node| node.data.borrow().sourcepos.start.line - 1)
        .collect()
}

/// A line of only pipes, dashes, colons, blockquote markers and whitespace
/// holding a dash, which reads as a delimiter row as well as a header or body.
fn is_dashes_and_pipes(line: &str) -> bool {
    line.contains('-') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | '>' | ' ' | '\t'))
}

fn is_one_column(delimiter: &str) -> bool {
    delimiter
        .trim_start_matches(['>', ' ', '\t'])
        .trim_matches('|')
        .matches('|')
        .count()
        == 0
}

/// Whether a confirmed block of `doc` has a row that MD055's pipe-less styles
/// turn into something else (an empty row ends the table, a lone dash is a list
/// marker), or that comrak can read as header, delimiter or body.
fn ambiguous(doc: &str, ctx: &LintContext) -> bool {
    let lines: Vec<&str> = doc.lines().collect();
    ctx.table_blocks
        .iter()
        .filter(|block| block.parser_confirmed)
        .any(|block| {
            is_dashes_and_pipes(lines[block.header_line])
                || block
                    .content_lines
                    .iter()
                    .any(|&row| lines[row].trim_matches(['|', ' ']).is_empty() || is_dashes_and_pipes(lines[row]))
                || is_one_column(lines[block.delimiter_line])
        })
}

#[test]
fn confirmed_blocks_are_tables_to_comrak() {
    let mut wrong = Vec::new();
    for (_, doc) in documents() {
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        if ambiguous(&doc, &ctx) {
            continue;
        }
        let comrak = comrak_headers(&doc);
        for block in ctx.table_blocks.iter().filter(|block| block.parser_confirmed) {
            if !comrak.contains(&block.header_line) {
                wrong.push(format!("{doc:?} header line {}", block.header_line));
            }
        }
    }
    // Shapes where pulldown-cmark reads a table and comrak does not: a list
    // item followed by rows indented with a tab or four spaces.
    assert!(
        wrong.is_empty(),
        "{} confirmed non-tables, e.g. {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
}

#[test]
fn unconfirmed_blocks_are_kept_for_lenient_rules() {
    // MD056 reports a delimiter row with the wrong cell count, so the scanner's
    // block stays in `table_blocks`, flagged as not confirmed.
    let ctx = LintContext::new("|a|b|c|\n|-|-|\n|1|2|3|\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.table_blocks.len(), 1);
    assert!(!ctx.table_blocks[0].parser_confirmed);
}

fn rewriting_rules() -> Vec<(String, Box<dyn Rule>)> {
    let mut rules: Vec<(String, Box<dyn Rule>)> = Vec::new();
    for style in [
        "leading_and_trailing",
        "no_leading_or_trailing",
        "leading_only",
        "trailing_only",
    ] {
        rules.push((
            format!("MD055/{style}"),
            Box::new(MD055TablePipeStyle::new(style.to_string())),
        ));
    }
    rules.push(("MD058".to_string(), Box::new(MD058BlanksAroundTables::default())));
    for style in ["aligned", "compact", "tight"] {
        rules.push((
            format!("MD060/{style}"),
            Box::new(MD060TableFormat::new(true, style.to_string())),
        ));
    }
    rules
}

#[test]
fn fixes_keep_the_block_structure() {
    let rules = rewriting_rules();
    let mut corrupted = Vec::new();
    for (_, doc) in documents() {
        let before = html(&doc);
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        if ambiguous(&doc, &ctx) {
            continue;
        }
        for (name, rule) in &rules {
            let fixed = rule.fix(&ctx).unwrap();
            // A blank line added inside a list item makes the list loose, which
            // wraps item text in paragraphs; that is the requested spacing, so
            // MD058 is compared without paragraph tags.
            let same = if name == "MD058" {
                strip_paragraphs(&html(&fixed)) == strip_paragraphs(&before)
            } else {
                html(&fixed) == before
            };
            if fixed != doc && !same {
                corrupted.push(format!("{name} {doc:?} -> {fixed:?}"));
            }
        }
    }
    // Shapes where pulldown-cmark reads a table and comrak does not.
    assert!(
        corrupted.is_empty(),
        "{} corrupting fixes, e.g. {:?}",
        corrupted.len(),
        &corrupted[..corrupted.len().min(8)]
    );
}
