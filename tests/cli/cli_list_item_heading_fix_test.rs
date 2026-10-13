//! `rumdl check --fix` must not move a heading out of the list item holding it.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares, per heading, the containers it is nested in. Counts alone would
//! pass `- x\n\n  ## a` becoming `- x\n\n## a`, which has the same headings, items
//! and code blocks with the heading in another place.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// What a rendering holds that a fix must not change.
#[derive(Debug, PartialEq, Eq)]
struct Structure {
    items: usize,
    code_blocks: usize,
    /// Per heading in document order: its level, its text, and the list, item and
    /// blockquote events it is nested in, outermost first.
    headings: Vec<(u8, String, Vec<&'static str>)>,
}

impl Structure {
    /// The same, without the heading levels and text that other heading rules
    /// may rewrite on purpose.
    fn shape(&self) -> (usize, usize, Vec<&[&'static str]>) {
        (
            self.items,
            self.code_blocks,
            self.headings.iter().map(|heading| heading.2.as_slice()).collect(),
        )
    }
}

fn structure(content: &str) -> Structure {
    let mut items = 0;
    let mut code_blocks = 0;
    let mut path: Vec<&'static str> = Vec::new();
    let mut headings = Vec::new();
    let mut current: Option<(u8, String, Vec<&'static str>)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => {
                items += 1;
                path.push("item");
            }
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::CodeBlock(_)) => code_blocks += 1,
            Event::Start(Tag::Heading { level, .. }) => current = Some((level as u8, String::new(), path.clone())),
            Event::Text(text) | Event::Code(text) => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push_str(&text);
                }
            }
            Event::SoftBreak => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => headings.extend(current.take()),
            _ => {}
        }
    }
    Structure {
        items,
        code_blocks,
        headings,
    }
}

/// The document after `rumdl check --fix`, with the given extra arguments.
fn fix(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--fix"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

const STRICT: &[&str] = &["--enable", "MD022,MD023"];

/// Documents with a heading inside a list item, in the shapes that corrupted it,
/// and headings outside every item beside them.
const CORPUS: &[&str] = &[
    "text\n    ---\n    - z\n===\n",
    "- x\n\n  text\n      ---\n      - z\n  ===\n",
    "-\n  ## a\n  text\n",
    "- x\n\n  ## a\n\n  text\n",
    "- x\n  ## a\n  text\n",
    "1. x\n\n   ## a\n\n   text\n",
    "- x\n  - y\n\n    ## a\n\n    text\n",
    "- x\n\n  ## a ##\n\n  text\n",
    "- x\n\n  ##  a\n\n  text\n",
    "- x\n\n    ## a\n\n  text\n",
    "- x\n\n  a\n  ===\n\n  text\n",
    "- x\n\n  a\n  ---\n\n  text\n",
    "1. x\n\n   a\n   ---\n",
    "-\n  a\n  ---\n  text\n",
    "-\n  a\n  b\n  ---\n  text\n",
    "- x\n  - y\n\n    a\n    ---\n\n    text\n",
    "10. x\n\n    ## a\n\n    text\n",
    "- x\n\n\t## a\n\n\ttext\n",
    "- x\n\n\t\t## a\n",
    "-\n  ## a\n",
    "1.\n   ## a\n   text\n",
    "-   \n  ## a\n  text\n",
    "-\n  ##  a\n  text\n",
    "-\n  -\n    ## a\n    text\n",
    "-\n\n  ## a\n\n  text\n",
    "-\n ## a\n",
    "- x\n  text\n    ## a\n",
    "- x\nlazy\n  ## a\n",
    "- x\n\n  a\nlazy\n  ---\n",
    "-\n  -\n\n    ## a\n",
    "- x\n  - y\n\n  paragraph\n\n    ## a\n",
    "-     code\n\n  ## a\n",
    "- x\n\n  > ## a\n",
    "> - x\n>\n>   ## a\n",
    "> -\n>   ## a\n>   text\n",
    "- x\n\n  a\n      ```\n  ---\n",
    "a\n    ```\n---\n",
    "- x\n\n      ## a\n",
    "-\n        ## a\n",
    "- -\n    ## a\n",
    "- -\n    a\n    ---\n",
    "1. -\n     ## a\n",
    "> x\n    ## a\n",
    "> > x\n    ## a\n",
    "- > x\n  ## a\n",
    "> - x\n    ## a\n",
    "para\n    ## a\n",
    "> x\n    a\n    ===\n",
    "- x\n\n  ## a\n\n- y\n\n  ## b\n",
    "# T\n\n- x\n\n  ### a\n\n  text\n",
    "1. x\n\n  ## a\n",
    "- x\n\nplain\n\n  ## a\n",
    "- x\n      ## A\n",
    "- x\n      - y\n        ## a\n",
    "- > x\n      ## a\n",
];

#[test]
fn no_default_fix_moves_a_heading_out_of_its_container() {
    for content in CORPUS {
        let before = structure(content);
        let after = structure(&fix(content, &[]));
        assert_eq!(before.shape(), after.shape(), "{content:?}");
    }
}

#[test]
fn the_heading_rules_alone_keep_every_heading_where_it_is() {
    for content in CORPUS {
        let before = structure(content);
        let after = structure(&fix(content, STRICT));
        assert_eq!(before, after, "{content:?}");
    }
}

#[test]
fn fixing_twice_changes_nothing_more() {
    for content in CORPUS {
        for extra in [&[][..], STRICT] {
            let once = fix(content, extra);
            let twice = fix(&once, extra);
            assert_eq!(once, twice, "{content:?} with {extra:?}");
        }
    }
}

/// Exact results for the shapes that corrupted a heading: the container's indent
/// stays and only the indentation past it goes.
#[test]
fn the_heading_rules_write_these_exact_results() {
    for (content, expected) in [
        ("- x\n\n    ## a\n\n  text\n", "- x\n\n  ## a\n\n  text\n"),
        ("- x\n\n    a\n    ---\n", "- x\n\n  a\n  ---\n"),
        ("-\n  ## a\n  text\n", "-\n  ## a\n\n  text\n"),
        ("- x\n  ## a\n  text\n", "- x\n\n  ## a\n\n  text\n"),
        ("-\n  a\n  ---\n  text\n", "-\n  a\n  ---\n\n  text\n"),
        ("-\n  -\n\n    ## a\n", "-\n  -\n\n  ## a\n"),
        (
            "- x\n  - y\n\n    ## a\n\n    text\n",
            "- x\n  - y\n\n    ## a\n\n    text\n",
        ),
        // Outside every item the heading still goes to the margin.
        ("-\n\n  ## a\n", "-\n\n## a\n"),
        ("- x\n\nplain\n\n  ## a\n", "- x\n\nplain\n\n## a\n"),
    ] {
        assert_eq!(fix(content, STRICT), expected, "{content:?}");
    }
}

/// Whitespace other than a space or tab is part of a heading's text, so a style
/// conversion writes its marker before that whitespace.
#[test]
fn style_conversion_keeps_unicode_whitespace_in_the_text() {
    for ws in ["\u{3000}", "\u{a0}"] {
        let content = format!("# T\n\n- {ws}Title\n    ---\n");
        let fixed = fix(&content, &["--enable", "MD003"]);
        assert_eq!(fixed, format!("# T\n\n- ## {ws}Title\n"), "{ws:?}");
    }
    // A numeral outside ASCII opens no list item, so this is plain text.
    let content = "# T\n\n\u{661}. a\n    ---\n";
    assert_eq!(fix(content, &["--enable", "MD003"]), content);
}

/// Positive control: the harness sees a fix when one changes the rendering, so a
/// pass above is not a harness that observes nothing.
#[test]
fn the_oracle_sees_headings_that_fixes_change() {
    let content = "Text\n## a\n";
    let fixed = fix(content, STRICT);
    assert_eq!(fixed, "Text\n\n## a\n");
    assert_ne!(content, fixed);
    let content = "  ## a\ntext\n";
    assert_eq!(fix(content, STRICT), "## a\n\ntext\n");
}

/// Negative control: the oracle tells a heading that left its item from one that
/// stayed, although both documents hold the same headings, items and code.
#[test]
fn the_oracle_detects_a_heading_leaving_its_item() {
    let inside = structure("- x\n\n  ## a\n\n  text\n");
    let outside = structure("- x\n\n## a\n\n  text\n");
    assert_eq!(inside.headings.len(), outside.headings.len());
    assert_eq!(inside.items, outside.items);
    assert_ne!(inside.shape(), outside.shape());
    assert_ne!(inside, outside);
    // And the corruption that shipped: `text` becoming indented code.
    let corrupted = structure("- x\n  - y\n\n## a\n\n    text\n");
    assert_ne!(
        structure("- x\n  - y\n\n    ## a\n\n    text\n").shape(),
        corrupted.shape()
    );
}

/// The document after `rumdl check --fix` with a config file, so a rule can be
/// set to a given style.
fn fix_with_config(content: &str, config: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config_path = dir.path().join("rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(&config_path, config).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--fix", "--config"])
        .arg(&config_path)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

/// Headings whose first line carries the list marker, in every container shape
/// the parser accepts them in, beside controls that are no headings.
const MARKER_LINE_CORPUS: &[&str] = &[
    "- a\n  ---\n",
    "- a\n  ===\n",
    "- a\n  ---\n  text\n",
    "- a\n  ---\n\n  text\n",
    "- a\n  ---\n- b\n  ---\n",
    "* a\n  ===\n* b\n  ===\n",
    "1. a\n   ---\n",
    "1) a\n   ===\n   text\n",
    "10. a\n    ---\n",
    "- - a\n    ---\n",
    "- - a\n    ===\n    text\n",
    "- a\n  b\n  ---\n",
    "- a\n  b\n  ---\n  text\n",
    "- a\n  b\n  c\n  ===\n",
    "- > a\n  > ---\n",
    "- > a\n  > ---\n  > text\n",
    "> - a\n>   ---\n",
    "> - a\n>   ===\n>   text\n",
    "> > - a\n> >   ---\n",
    "> - > a\n>   > ---\n",
    "> - > a\n>   > b\n>   > ===\n",
    "> > - > a\n> >   > ---\n",
    "- a\n  ---\n  - b\n    ===\n",
    "# T\n\n- a\n  ---\n\n## U\n",
    "text\n\n- a\n  ---\n",
    "- ## a\n",
    "- ## a\n  text\n",
    "1. # a\n",
    "1. # a\n   text\n",
    "1) ### a\n",
    "- - ## a\n",
    "- - ## a\n    text\n",
    "- ## a ##\n",
    "- ##  a\n",
    "-   ## a\n",
    "- > ## a\n",
    "> - ## a\n",
    "> 1. # a\n",
    "# T\n\n- ## a\n\n  - ### b\n",
    "# T\n\n- ## a\n  text\n- ## b\n  text\n",
    "- # a\n- # b\n",
    "- ## a\n  ---\n",
    "- ## a\n  b\n  ---\n",
    "- ##\ta\n",
    "- #\ta\n",
    "- a\n   ---\n",
    "- ## a\n  text\n  ---\n",
    // Controls: not headings.
    "- a\n---\n",
    "- a\n===\n",
    "- ##a\n",
    "- x\n  -      ## a\n",
    "1.      ## a\n",
    "- -     ## a\n",
    "> -     ## a\n",
    "-     ## a\n",
    "- a\n\n  ---\n",
    "1. a\n  ---\n",
];

#[test]
fn no_default_fix_corrupts_a_heading_on_a_marker_line() {
    for content in MARKER_LINE_CORPUS {
        // MD069 flattens the second marker of `- - a` whatever follows it.
        if content.contains("- - ") {
            continue;
        }
        let before = structure(content);
        let after = structure(&fix(content, &[]));
        assert_eq!(before.shape(), after.shape(), "{content:?}");
    }
}

/// Every rule that reads or writes a heading, alone. Level and text may change
/// on purpose in MD001, MD025 and MD063, so those compare the containers only.
#[test]
fn each_heading_rule_alone_keeps_a_marker_line_heading_in_its_item() {
    for rule in [
        "MD001", "MD003", "MD018", "MD019", "MD020", "MD021", "MD022", "MD023", "MD024", "MD025", "MD026", "MD036",
        "MD041", "MD043", "MD063", "MD065", "MD066", "MD069", "MD077",
    ] {
        for content in MARKER_LINE_CORPUS {
            let before = structure(content);
            let after = structure(&fix(content, &["--enable", rule]));
            if matches!(rule, "MD001" | "MD025" | "MD063") {
                assert_eq!(before.shape(), after.shape(), "{rule} {content:?}");
            } else if rule == "MD069" || rule == "MD077" {
                // Both flatten the second marker of `- - a`, which is unrelated
                // to the heading and identical on documents without one.
                if content.contains("- - ") {
                    continue;
                }
                assert_eq!(before, after, "{rule} {content:?}");
            } else {
                assert_eq!(before, after, "{rule} {content:?}");
            }
        }
    }
}

#[test]
fn md003_styles_keep_a_marker_line_heading_in_its_item() {
    for style in ["atx", "atx_closed", "setext", "setext_with_atx", "consistent"] {
        let config = format!("[global]\nenable = [\"MD003\"]\n\n[MD003]\nstyle = \"{style}\"\n");
        for content in MARKER_LINE_CORPUS {
            let before = structure(content);
            let fixed = fix_with_config(content, &config);
            assert_eq!(before, structure(&fixed), "{style} {content:?} -> {fixed:?}");
            assert_eq!(fixed, fix_with_config(&fixed, &config), "{style} {content:?}");
        }
    }
}

#[test]
fn fixing_a_marker_line_heading_twice_changes_nothing_more() {
    for content in MARKER_LINE_CORPUS {
        for extra in [&[][..], STRICT] {
            let once = fix(content, extra);
            let twice = fix(&once, extra);
            assert_eq!(once, twice, "{content:?} with {extra:?}");
        }
    }
}

/// Exact results: the marker stays in front of whatever the rule writes.
#[test]
fn marker_line_headings_are_rewritten_behind_their_marker() {
    let config = |rule: &str, body: &str| format!("[global]\nenable = [\"{rule}\"]\n\n{body}");
    for (rule, body, content, expected) in [
        ("MD003", "[MD003]\nstyle = \"atx\"\n", "- a\n  ---\n", "- ## a\n"),
        ("MD003", "[MD003]\nstyle = \"atx\"\n", "- a\n  ===\n", "- # a\n"),
        ("MD003", "[MD003]\nstyle = \"atx\"\n", "1. a\n   ---\n", "1. ## a\n"),
        ("MD003", "[MD003]\nstyle = \"atx\"\n", "- a\n  b\n  ---\n", "- ## a b\n"),
        (
            "MD003",
            "[MD003]\nstyle = \"atx_closed\"\n",
            "- a\n  ---\n",
            "- ## a ##\n",
        ),
        // Headings inside a blockquote are not rewritten by MD003.
        (
            "MD003",
            "[MD003]\nstyle = \"atx\"\n",
            "> - a\n>   ---\n",
            "> - a\n>   ---\n",
        ),
        // ATX to setext on a marker line cannot be written safely, so it is left.
        ("MD003", "[MD003]\nstyle = \"setext\"\n", "- ## a\n", "- ## a\n"),
        ("MD019", "", "-  ##  a\n", "-  ## a\n"),
        ("MD019", "", "- ##  a\n", "- ## a\n"),
        ("MD021", "", "- ##  a  ##\n", "- ## a ##\n"),
        ("MD022", "", "- ## a\n  text\n", "- ## a\n\n  text\n"),
        ("MD022", "", "text\n- ## a\n", "text\n- ## a\n"),
        ("MD025", "", "# T\n\n- # b\n", "# T\n\n- ## b\n"),
        ("MD063", "", "- the quick fox\n  ---\n", "- The Quick Fox\n  ---\n"),
        ("MD063", "", "- ## the quick fox\n", "- ## The Quick Fox\n"),
        (
            "MD063",
            "",
            "- the quick\n  brown fox\n  ---\n",
            "- The Quick\n  Brown Fox\n  ---\n",
        ),
    ] {
        let config = config(rule, body);
        assert_eq!(fix_with_config(content, &config), expected, "{rule} {content:?}");
    }
}

/// The `check` output (stdout and stderr) for a document under one rule.
fn check(content: &str, rule: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--enable", rule])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A link to the anchor of a heading that shares its line with a marker is a
/// link to a heading that exists.
#[test]
fn links_to_marker_line_headings_resolve() {
    for content in [
        "[x](#a)\n\n- a\n  ---\n",
        "[x](#a)\n\n- a\n  ===\n",
        "[x](#a)\n\n1. a\n   ---\n",
        "[x](#a)\n\n- - a\n    ---\n",
        "[x](#a)\n\n- ## a\n",
        "[x](#a)\n\n1. # a\n",
        "[x](#a)\n\n- > a\n  > ---\n",
        "[x](#a)\n\n> - a\n>   ---\n",
        "[x](#a-b)\n\n- a\n  b\n  ---\n",
    ] {
        let output = check(content, "MD051");
        assert!(
            output.contains("No issues found") || !output.contains("MD051"),
            "{content:?}: {output}"
        );
    }
    // Negative control: an anchor that no heading has is still reported.
    assert!(check("[x](#missing)\n\n- a\n  ---\n", "MD051").contains("MD051"));
}

/// A heading that opens a list item is container content, not the first line's
/// heading, as a quoted heading already is.
#[test]
fn md041_does_not_accept_a_heading_that_opens_a_list_item() {
    for content in ["- # a\n", "1. # a\n", "- a\n  ===\n"] {
        assert!(check(content, "MD041").contains("MD041"), "{content:?}");
    }
    // Controls: a real first-line heading, and one after the first line's item.
    assert!(!check("# a\n\n- b\n", "MD041").contains("MD041"));
}

/// Rules that report on heading text see a marker-line heading, with the text
/// read after the marker.
#[test]
fn heading_text_rules_report_marker_line_headings() {
    // MD026: trailing punctuation.
    for content in ["- ## a.\n", "- a.\n  ---\n"] {
        assert!(check(content, "MD026").contains("MD026"), "{content:?}");
    }
    // MD024: duplicate headings.
    for content in ["# T\n\n- a\n  ---\n\n- a\n  ---\n", "- ## a\n- ## a\n"] {
        assert!(check(content, "MD024").contains("MD024"), "{content:?}");
    }
    // MD001: a level jump through a marker-line heading.
    assert!(check("# T\n\n- ### a\n", "MD001").contains("MD001"));
    // MD025: a second H1.
    assert!(check("# T\n\n- # a\n", "MD025").contains("MD025"));
    // MD019: extra spaces after the hashes.
    assert!(check("- ##  a\n", "MD019").contains("MD019"));
    assert_eq!(fix("- ## a.\n", &["--enable", "MD026"]), "- ## a\n");
    // Negative control: clean headings stay clean.
    assert!(!check("- ## a\n", "MD026").contains("MD026"));
    assert!(!check("- ## a\n- ## b\n", "MD024").contains("MD024"));
}

/// MD065 puts a blank line above a thematic break, which would split a heading's
/// text from its underline; a marker-line heading is a heading, so it is left.
#[test]
fn md065_leaves_marker_line_setext_headings_alone() {
    for content in [
        "- a\n  ---\n",
        "1. a\n   ---\n",
        "- > a\n  > ---\n",
        "> - a\n>   ---\n",
        "> - > a\n>   > ---\n",
        "> > - > a\n> >   > ---\n",
        "- a\n  b\n  ---\n",
    ] {
        assert_eq!(fix(content, &["--enable", "MD065"]), content, "{content:?}");
    }
    // Control: a real thematic break after a paragraph still gets its blank line.
    assert_eq!(
        fix("- a\n\n  b\n  ***\n", &["--enable", "MD065"]),
        "- a\n\n  b\n\n  ***\n"
    );
    assert_eq!(fix("> a\n> ***\n", &["--enable", "MD065"]), "> a\n>\n> ***\n");
}
