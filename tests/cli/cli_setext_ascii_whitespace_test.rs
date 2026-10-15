//! CommonMark 4.3 allows only spaces and tabs around a setext underline. A line
//! of `---` or `===` with any other whitespace (an em space, a no-break space)
//! is paragraph text, so no rule may read a heading there and `--fix` must not
//! rewrite the paragraph into one. Structure is judged by pulldown-cmark.

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// (level, line) of every heading in `content`, per pulldown-cmark.
fn headings(content: &str) -> Vec<(HeadingLevel, usize)> {
    Parser::new(content)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Heading { level, .. }) => Some((level, content[..range.start].matches('\n').count())),
            _ => None,
        })
        .collect()
}

/// Run `rumdl check` on `content` with `args`; returns (file after, stdout).
fn run(content: &str, args: &[&str]) -> (String, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache"])
        .args(args)
        .arg(&path)
        .output()
        .expect("run rumdl check");
    (
        fs::read_to_string(&path).unwrap(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn fix(content: &str, args: &[&str]) -> String {
    let mut full = vec!["--fix"];
    full.extend_from_slice(args);
    run(content, &full).0
}

/// Documents with a Unicode-whitespace underline, at top level, in a list item
/// and in a blockquote.
fn documents() -> Vec<String> {
    let mut docs = Vec::new();
    for ws in ['\u{2003}', '\u{a0}', '\u{3000}', '\u{2009}'] {
        for marker in ["---", "==="] {
            docs.push(format!("# T\n\nTitle2\n{marker}{ws}\n\nText\n"));
            docs.push(format!("# T\n\nTitle2\n{ws}{marker}\n\nText\n"));
            docs.push(format!("# T\n\n- Title2\n  {marker}{ws}\n\n  Text\n"));
            docs.push(format!("# T\n\n> Title2\n> {marker}{ws}\n>\n> Text\n"));
        }
    }
    docs
}

#[test]
fn unicode_whitespace_around_an_underline_is_not_a_heading_for_any_rule() {
    for doc in documents() {
        // The oracle agrees with the premise: only `# T` is a heading.
        assert_eq!(headings(&doc).len(), 1, "oracle on {doc:?}");
        for args in [&[][..], &["--enable", "MD003,MD025,MD022,MD024,MD041"][..]] {
            let (_, out) = run(&doc, args);
            assert!(!out.contains("MD003"), "{doc:?} {args:?}: {out}");
            assert!(!out.contains("MD025"), "{doc:?} {args:?}: {out}");
            let fixed = fix(&doc, args);
            assert_eq!(headings(&fixed), headings(&doc), "{doc:?} {args:?} -> {fixed:?}");
            assert_eq!(fix(&fixed, args), fixed, "idempotent on {doc:?} {args:?}");
        }
    }
}

/// The reported repro: `--fix` used to turn the paragraph into `## Title2`.
#[test]
fn fix_leaves_the_paragraph_with_an_em_space_underline_byte_for_byte() {
    let doc = "# T\n\nTitle2\n---\u{2003}\n\nText\n";
    assert_eq!(fix(doc, &[]), doc);
    assert_eq!(fix(doc, &["--enable", "MD003"]), doc);
}

/// Controls: spaces and tabs after the run, and up to three leading spaces, keep
/// the heading; the heading is really seen by a rule (MD003 atx style rewrites it).
#[test]
fn ascii_space_and_tab_underlines_remain_headings() {
    let args = ["--enable", "MD003", "--config", "MD003.style=\"atx\""];
    for underline in ["---  ", "---\t", "   ---"] {
        let doc = format!("# T\n\nTitle2\n{underline}\n\nText\n");
        assert_eq!(headings(&doc).len(), 2, "oracle on {doc:?}");
        assert_eq!(fix(&doc, &args), "# T\n\n## Title2\n\nText\n", "{doc:?}");
    }
    let doc = "# T\n\nTitle1\n===\t\n\nText\n";
    assert_eq!(fix(doc, &args), "# T\n\n# Title1\n\nText\n");
}

fn html(content: &str) -> String {
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, Parser::new(content));
    out
}

#[test]
fn unicode_whitespace_after_a_dash_run_is_not_a_horizontal_rule() {
    // `---` followed by an em space is paragraph text, so MD065 must not
    // split the paragraph by inserting a blank line before it.
    for doc in documents() {
        let fixed = fix(&doc, &["--enable", "MD065"]);
        assert_eq!(html(&fixed), html(&doc), "{doc:?} -> {fixed:?}");
    }
}

#[test]
fn unicode_whitespace_after_a_marker_run_in_a_list_item_is_not_a_thematic_break() {
    // The line is text inside the item; MD035 --fix must not split the list.
    for ws in ['\u{2003}', '\u{a0}', '\u{3000}'] {
        for marker in ["***", "---", "___", "* * *", "- - -"] {
            let doc = format!("- Intro\n  {marker}{ws}\n  more\n");
            // MD069 deliberately removes a repeated list marker, which is not
            // about whitespace and is excluded here.
            for args in [
                &["--disable", "MD069"][..],
                &["--enable", "MD035,MD065,MD032", "--disable", "MD069"][..],
            ] {
                let fixed = fix(&doc, args);
                assert_eq!(html(&fixed), html(&doc), "{doc:?} {args:?} -> {fixed:?}");
            }
        }
    }
}
