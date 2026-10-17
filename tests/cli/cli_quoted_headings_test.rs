//! Heading rules read headings written inside blockquotes (nested quotes, quotes
//! in list items, list items in quotes) the way they read top-level headings,
//! and their fixes keep every quote marker.
//!
//! The oracle parses the document before and after `rumdl check --fix` with
//! pulldown-cmark and lists every leaf block with the containers around it and
//! the heading level, so a fix that drops a `>`, splits a quote or changes a
//! heading's level shows as a different list.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Every leaf block as `containers > kind: text`, in document order.
fn skeleton(content: &str) -> Vec<String> {
    let mut path: Vec<&'static str> = Vec::new();
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut open: Option<String> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Rule => blocks.push(format!("{} > rule", path.join(" "))),
            Event::Start(Tag::Heading { level, .. }) => open = Some(format!("heading{}", level as usize)),
            Event::Start(Tag::CodeBlock(_)) => open = Some("code".to_string()),
            Event::Start(Tag::HtmlBlock) => open = Some("html".to_string()),
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => text.push_str(&t),
            Event::SoftBreak => text.push(' '),
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
                if let Some(kind) = open.take() {
                    blocks.push(format!("{} > {kind}: {}", path.join(" "), text.trim()));
                    text.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                blocks.push(format!("{} > text: {}", path.join(" "), text.trim()));
                text.clear();
            }
            _ => {}
        }
    }
    blocks
}

fn run(content: &str, args: &[&str]) -> (String, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
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

/// The document after `rumdl check --fix --enable <rule>`.
fn fixed(rule: &str, content: &str) -> String {
    run(content, &["--fix", "--enable", rule]).0
}

/// The 1-indexed lines `rumdl check --enable <rule>` reports `rule` on.
fn warned_lines(rule: &str, content: &str) -> Vec<usize> {
    reported_lines(rule, &run(content, &["--enable", rule, "--output-format=json"]).1)
}

/// Read structured locations without interpreting separators in file names.
fn reported_lines(rule: &str, output: &str) -> Vec<usize> {
    let warnings: Vec<serde_json::Value> = serde_json::from_str(output).expect("valid JSON findings");
    warnings
        .iter()
        .filter(|warning| warning["rule"] == rule)
        .map(|warning| warning["line"].as_u64().expect("finding line number") as usize)
        .collect()
}

/// The skeleton with letter case ignored, for rules that recase text.
fn folded_skeleton(content: &str) -> Vec<String> {
    skeleton(content).iter().map(|block| block.to_lowercase()).collect()
}

/// The fix of `input` is exactly `expected`, keeps the block structure of the
/// document, and fixing again changes nothing.
fn assert_fix(rule: &str, input: &str, expected: &str) {
    let once = fixed(rule, input);
    assert_eq!(once, expected, "{rule}: fix of {input:?}");
    assert_eq!(
        folded_skeleton(&once),
        folded_skeleton(input),
        "{rule}: structure of {input:?}"
    );
    assert_eq!(fixed(rule, &once), once, "{rule}: not idempotent on {input:?}");
}

/// The rule reports nothing and the fix leaves the document as written.
fn assert_untouched(rule: &str, input: &str) {
    assert_eq!(
        warned_lines(rule, input),
        Vec::<usize>::new(),
        "{rule}: warns on {input:?}"
    );
    assert_eq!(fixed(rule, input), input, "{rule}: fix changed {input:?}");
}

mod md018 {
    use super::*;

    /// The fix of `input` is exactly `expected`, which parses to `headings`
    /// (the paragraph became a heading in the same containers), and fixing
    /// again changes nothing.
    fn assert_becomes_heading(input: &str, expected: &str, headings: &[&str]) {
        assert!(
            skeleton(input).iter().all(|block| !block.contains("> heading")),
            "{input:?} must start as plain text, got {:?}",
            skeleton(input)
        );
        let once = fixed("MD018", input);
        assert_eq!(once, expected, "MD018: fix of {input:?}");
        assert_eq!(skeleton(&once), headings, "MD018: structure of {input:?}");
        assert_eq!(fixed("MD018", &once), once, "MD018: not idempotent on {input:?}");
    }

    #[test]
    fn quoted_text_missing_the_space_is_fixed_keeping_the_quote() {
        assert_becomes_heading("> #Bad\n", "> # Bad\n", &["quote > heading1: Bad"]);
        assert_becomes_heading("> > ##Bad\n", "> > ## Bad\n", &["quote quote > heading2: Bad"]);
        assert_becomes_heading(">#Bad\n", "># Bad\n", &["quote > heading1: Bad"]);
        assert_becomes_heading(
            "> text\n> ###Bad\n",
            "> text\n> ### Bad\n",
            &["quote > text: text", "quote > heading3: Bad"],
        );
    }

    #[test]
    fn unicode_whitespace_after_the_hashes_is_replaced_by_one_space() {
        // CommonMark strips only ASCII spaces and tabs, so a space inserted in front of
        // the Unicode space would leave it as the first character of the heading text.
        for space in ['\u{a0}', '\u{2003}', '\u{3000}'] {
            for (prefix, hashes) in [("", "#"), ("> ", "#"), ("", "###"), ("> ", "##")] {
                let input = format!("{prefix}{hashes}{space}Heading\n");
                let expected = format!("{prefix}{hashes} Heading\n");
                assert_eq!(fixed("MD018", &input), expected, "MD018: fix of {input:?}");
                assert_eq!(
                    fixed("MD018", &expected),
                    expected,
                    "MD018: not idempotent on {input:?}"
                );
            }
        }
    }

    #[test]
    fn a_zero_width_space_is_not_whitespace_and_stays_in_the_text() {
        for (input, expected) in [
            ("#\u{200b}Heading\n", "# \u{200b}Heading\n"),
            ("> #\u{200b}Heading\n", "> # \u{200b}Heading\n"),
        ] {
            assert_eq!(fixed("MD018", input), expected, "MD018: fix of {input:?}");
            assert_eq!(fixed("MD018", expected), expected, "MD018: not idempotent on {input:?}");
        }
    }

    #[test]
    fn a_quote_in_a_footnote_is_a_quote() {
        // The skeleton parser has no footnotes, so the result is compared as text.
        let input = "[^1]: note\n\n    > #Bad\n";
        assert_eq!(warned_lines("MD018", input), vec![3]);
        assert_eq!(fixed("MD018", input), "[^1]: note\n\n    > # Bad\n");
    }

    #[test]
    fn a_quote_in_a_wide_list_item_is_a_quote() {
        assert_becomes_heading(
            "-   item\n\n    > #Bad\n",
            "-   item\n\n    > # Bad\n",
            &["list item > text: item", "list item quote > heading1: Bad"],
        );
    }

    #[test]
    fn quoted_text_missing_the_space_is_reported_on_its_line() {
        assert_eq!(warned_lines("MD018", "# Top\n\n> #Bad\n>\n> > ##Deep\n"), vec![3, 5]);
    }

    #[test]
    fn text_that_is_not_a_heading_candidate_is_left_alone() {
        assert_untouched("MD018", "> # Good\n");
        assert_untouched("MD018", "> #1\n");
        assert_untouched("MD018", "> >  #Bad\n");
        assert_untouched("MD018", "> text\n>     #Bad\n");
        assert_untouched("MD018", "> o\n>     > #Bad\n");
        assert_untouched("MD018", "> text\n    > #Bad\n");
        assert_untouched("MD018", "text\n    > #Bad\n");
        assert_untouched("MD018", "text\n\n    > #Bad\n");
        // pandoc: the line is text of the footnote paragraph.
        assert_untouched("MD018", "[^1]: note\n    > #Bad\n");
        assert_untouched("MD018", "> > o\n> >     > #Bad\n");
        assert_untouched("MD018", "> ```\n> #Bad\n> ```\n");
        assert_untouched("MD018", "> Title\n> ===\n");
        assert_untouched("MD018", "> #Bad\n> ===\n");
    }

    #[test]
    fn a_tab_between_quote_markers_stops_at_the_next_tab_stop() {
        assert_becomes_heading("> \t> #Bad\n", "> \t> # Bad\n", &["quote quote > heading1: Bad"]);
        assert_becomes_heading(">\t> #Bad\n", ">\t> # Bad\n", &["quote quote > heading1: Bad"]);
        assert_untouched("MD018", ">\t\t> #Bad\n");
    }
}

mod md019 {
    use super::*;

    #[test]
    fn quoted_headings_with_extra_spaces_are_fixed_keeping_the_quote() {
        assert_fix("MD019", "> ##  Two\n", "> ## Two\n");
        assert_fix("MD019", "> > ###   Three\n", "> > ### Three\n");
        assert_fix("MD019", ">##  Tight\n", ">## Tight\n");
        assert_fix("MD019", "> - ##  In item\n", "> - ## In item\n");
        assert_fix("MD019", "- > ##  In quote\n", "- > ## In quote\n");
        assert_fix(
            "MD019",
            "# Top\n\n> ##  Two\n> text\n\n##  Plain\n",
            "# Top\n\n> ## Two\n> text\n\n## Plain\n",
        );
    }

    #[test]
    fn quoted_headings_are_reported_on_their_lines() {
        assert_eq!(
            warned_lines("MD019", "# Top\n\n> ##  Two\n>\n> > #  Deep\n"),
            vec![3, 5]
        );
    }

    #[test]
    fn text_that_is_not_a_heading_is_left_alone() {
        // Indented code, a lazy continuation, fenced code and a missing space are no heading.
        assert_untouched("MD019", "> text\n>     ##  B\n");
        assert_untouched("MD019", ">     ##  Code\n");
        assert_untouched("MD019", "> ```\n> ##  fenced\n> ```\n");
        assert_untouched("MD019", "> ## Single\n");
    }

    #[test]
    fn a_quoted_heading_inside_template_code_is_left_alone() {
        assert_untouched("MD019", "> {% set x = \"\n> ##  y\" %}\n");
        assert_untouched("MD019", "> {{ \"\n> ##  y\" }}\n");
        assert_fix("MD019", "> {% if x %}\n>\n> ##  y\n", "> {% if x %}\n>\n> ## y\n");
    }
}

mod md020 {
    use super::*;

    /// The distinct container paths that hold blocks, without the blocks.
    fn containers(content: &str) -> std::collections::BTreeSet<String> {
        skeleton(content)
            .iter()
            .map(|block| {
                block
                    .rsplit_once(" > ")
                    .map_or(String::new(), |(path, _)| path.to_string())
            })
            .collect()
    }

    /// The fix of `input` is exactly `expected`, stays in the same
    /// containers, and fixing again changes nothing. The fix may change the
    /// heading text itself (`Bad#` becomes `Bad`), so blocks are not compared.
    fn assert_closed(input: &str, expected: &str) {
        let once = fixed("MD020", input);
        assert_eq!(once, expected, "MD020: fix of {input:?}");
        assert_eq!(containers(&once), containers(input), "MD020: containers of {input:?}");
        assert_eq!(fixed("MD020", &once), once, "MD020: not idempotent on {input:?}");
    }

    #[test]
    fn quoted_closed_headings_are_fixed_keeping_the_quote() {
        assert_closed("> #Bad#\n", "> # Bad #\n");
        assert_closed("> ##Bad ##\n", "> ## Bad ##\n");
        assert_closed("> ## Bad##\n", "> ## Bad ##\n");
        assert_closed("> > ##Deep##\n", "> > ## Deep ##\n");
        assert_closed(">#Tight#\n", "># Tight #\n");
        assert_closed(
            "# Top #\n\n> #Bad#\n> text\n\n#Plain#\n",
            "# Top #\n\n> # Bad #\n> text\n\n# Plain #\n",
        );
    }

    #[test]
    fn quoted_text_in_an_mdx_comment_is_left_alone() {
        let input = "{/*\n> #Bad#\n*/}\n";
        let (out, report) = run(input, &["--enable", "MD020", "--flavor", "mdx", "--fix"]);
        assert_eq!(out, input);
        assert!(!report.contains("MD020"), "{report}");
    }

    #[test]
    fn quoted_text_in_a_jsx_attribute_is_left_alone() {
        let input = "<Component value=\"\n> #Bad#\n\" />\n";
        let (out, report) = run(input, &["--enable", "MD020", "--flavor", "mdx", "--fix"]);
        assert_eq!(out, input);
        assert!(!report.contains("MD020"), "{report}");
    }

    #[test]
    fn a_quote_in_a_footnote_is_a_quote() {
        let input = "[^1]: note\n\n    > #Bad#\n";
        assert_eq!(warned_lines("MD020", input), vec![3]);
        assert_eq!(fixed("MD020", input), "[^1]: note\n\n    > # Bad #\n");
    }

    #[test]
    fn a_quote_in_a_wide_list_item_is_a_quote() {
        assert_closed("-   item\n\n    > #Bad#\n", "-   item\n\n    > # Bad #\n");
    }

    #[test]
    fn quoted_closed_headings_are_reported_on_their_lines_and_columns() {
        assert_eq!(
            warned_lines("MD020", "# Top #\n\n> #Bad#\n>\n> > ##Deep##\n"),
            vec![3, 5]
        );
        let (_, out) = run("> #Bad#\n", &["--enable", "MD020"]);
        assert!(out.contains("doc.md:1:4:"), "column after the opening hash: {out}");
    }

    #[test]
    fn a_tab_after_the_quote_marker_counts_from_the_line_column() {
        assert_closed("> \t# Bad#\n", "> \t# Bad #\n");
        assert_closed(">\t# Bad#\n", ">\t# Bad #\n");
        assert_closed(">    # Bad#\n", ">    # Bad #\n");
        assert_untouched("MD020", "> \t\t# Bad#\n");
    }

    #[test]
    fn text_that_is_not_a_heading_is_left_alone() {
        assert_untouched("MD020", ">     #Bad#\n");
        assert_untouched("MD020", ">     > # Bad#\n");
        assert_untouched("MD020", "> text\n    > # Bad#\n");
        assert_untouched("MD020", "text\n    > # Bad#\n");
        assert_untouched("MD020", "text\n\n    > # Bad#\n");
        assert_untouched("MD020", "[^1]: note\n    > #Bad#\n");
        assert_untouched("MD020", "> ####### Text#\n");
        assert_untouched("MD020", "> text\n>     > # Bad#\n");
        assert_untouched("MD020", "> text\n>     #Bad#\n");
        assert_untouched("MD020", "> ```\n> #Bad#\n> ```\n");
        assert_untouched("MD020", "> text\n> #Bad#\n> ===\n");
        assert_untouched("MD020", "> # Good #\n");
        // A line inside a multiline link is link text, not a heading.
        assert_untouched("MD020", "> [link\n> #Bad#\n> ](url)\n");
        // Text of the list item paragraph, eight columns in under a two column item.
        assert_untouched("MD020", "- item\n        > #Bad#\n");
        assert_untouched("MD018", "- item\n        > #Bad\n");
        assert_untouched("MD020", "> - #Item#\n");
        assert_untouched("MD020", "> C\\#\n> #Bad\\#\n");
    }
}

mod md021 {
    use super::*;

    #[test]
    fn quoted_closed_headings_are_fixed_keeping_the_quote() {
        assert_fix("MD021", "> ##  Two  ##\n", "> ## Two ##\n");
        assert_fix("MD021", "> > #  Deep #\n", "> > # Deep #\n");
        assert_fix("MD021", ">##  Tight  ##\n", ">## Tight ##\n");
        assert_fix("MD021", "> - ##  Item  ##\n", "> - ## Item ##\n");
        assert_fix("MD021", "- > ##  In quote  ##\n", "- > ## In quote ##\n");
        assert_fix(
            "MD021",
            "# Top #\n\n> ##  Two  ##\n> text\n\n##  Plain  ##\n",
            "# Top #\n\n> ## Two ##\n> text\n\n## Plain ##\n",
        );
    }

    #[test]
    fn quoted_closed_headings_are_reported_on_their_lines() {
        assert_eq!(
            warned_lines("MD021", "# Top #\n\n> ##  Two  ##\n>\n> > #  Deep #\n"),
            vec![3, 5]
        );
    }

    #[test]
    fn text_that_is_not_a_heading_is_left_alone() {
        assert_untouched("MD021", "> text\n>     ##  B  ##\n");
        assert_untouched("MD021", ">     ##  Code  ##\n");
        assert_untouched("MD021", "> ```\n> ##  fenced  ##\n> ```\n");
        assert_untouched("MD021", "> ## Single ##\n");
    }

    #[test]
    fn a_quoted_heading_in_a_wide_list_item_is_fixed() {
        assert_fix(
            "MD021",
            "-   item\n\n    > ##  Heading  ##\n",
            "-   item\n\n    > ## Heading ##\n",
        );
    }

    #[test]
    fn a_quoted_heading_inside_template_code_is_left_alone() {
        assert_untouched("MD021", "> {% set x = \"\n> ##  y  ##\n> \" %}\n");
        assert_untouched("MD021", "> {{ \"\n> ##  y  ##\n> \" }}\n");
        assert_fix(
            "MD021",
            "> {% if x %}\n>\n> ##  y  ##\n",
            "> {% if x %}\n>\n> ## y ##\n",
        );
    }
}

mod md023 {
    use super::*;

    #[test]
    fn code_in_a_quote_opened_on_a_list_item_line_is_left_alone() {
        // pandoc: BulletList [BlockQuote [CodeBlock "## Code"]]
        for rule in ["MD018", "MD019", "MD023", "MD064"] {
            assert_untouched(rule, "- >     ## Code\n");
            assert_untouched(rule, "- - >     ##  Code\n");
        }
    }

    #[test]
    fn indented_quoted_headings_are_fixed_keeping_the_quote() {
        assert_fix("MD023", ">   ## Two\n", "> ## Two\n");
        assert_fix("MD023", "> >  # Deep\n", "> > # Deep\n");
        assert_fix("MD023", ">  ## Closed ##\n", "> ## Closed ##\n");
        assert_fix("MD023", "  >  ## Indented quote\n", "  > ## Indented quote\n");
        assert_fix("MD023", ">    Setext\n>    ---\n", "> Setext\n> ---\n");
        assert_fix("MD023", "> text\n>  more\n> ---\n", "> text\n> more\n> ---\n");
        assert_fix(
            "MD023",
            "# Top\n\n>  ## Two\n> text\n\n  ## Plain\n",
            "# Top\n\n> ## Two\n> text\n\n## Plain\n",
        );
    }

    #[test]
    fn indented_quoted_headings_are_reported_on_their_lines() {
        assert_eq!(
            warned_lines("MD023", "# Top\n\n>  ## Two\n>\n> >   # Deep\n"),
            vec![3, 5]
        );
        assert_eq!(warned_lines("MD023", ">  Setext\n>  ---\n"), vec![1, 2]);
    }

    #[test]
    fn headings_at_the_quote_margin_and_non_headings_are_left_alone() {
        assert_untouched("MD023", "> ## Two\n");
        assert_untouched("MD023", ">## Tight\n");
        assert_untouched("MD023", "> > # Deep\n");
        assert_untouched("MD023", "> text\n>     ## B\n");
        assert_untouched("MD023", ">     ## Code\n");
        assert_untouched("MD023", "> ```\n>   ## fenced\n> ```\n");
    }

    #[test]
    fn indentation_inside_a_list_item_of_the_quote_is_the_items_own() {
        // Removing it would take the heading out of the item.
        assert_untouched("MD023", "> - x\n>\n>   ## a\n");
        assert_untouched("MD023", "> - a\n>   ---\n");
        assert_untouched("MD023", "> 1. a\n>    ---\n");
        assert_untouched("MD023", "> - a\n>\n>   - b\n>\n>     ## c\n");
    }

    #[test]
    fn indentation_after_a_list_that_ended_is_still_reported() {
        assert_fix(
            "MD023",
            "> - item\n>\n> ## Outside\n>\n>   ## Indented\n",
            "> - item\n>\n> ## Outside\n>\n> ## Indented\n",
        );
        // Flush text after a blank line closes the list; flush text right under
        // an item may still be part of it.
        assert_fix(
            "MD023",
            "> - item\n>\n> Text\n>\n>   ## Indented\n",
            "> - item\n>\n> Text\n>\n> ## Indented\n",
        );
        assert_untouched("MD023", "> - item\n> lazy\n>   ## In item\n");
        assert_untouched("MD023", "> - item\nlazy\n>   ## In item\n");
        assert_untouched("MD023", "- > - item\n  >   ## In item\n");
        assert_untouched("MD023", "> - item\n> #tag\n>   ## In item\n");
        assert_untouched("MD023", "> - item\n>   > # Nested\n>   ## In item\n");
    }

    #[test]
    fn a_flush_heading_or_a_fenced_list_does_not_leave_a_list_open() {
        assert_fix(
            "MD023",
            "> - item\n> ## Outside\n>   ## Indented\n",
            "> - item\n> ## Outside\n> ## Indented\n",
        );
        assert_fix(
            "MD023",
            "> ```\n> - x\n> ```\n>\n>   ## Indented\n",
            "> ```\n> - x\n> ```\n>\n> ## Indented\n",
        );
    }

    #[test]
    fn markers_that_are_text_of_a_setext_paragraph_are_not_stripped() {
        // The literal `>` is inline code content, so no fix may touch the spaces after it.
        let input = "> Text `code\n>     >  hi`\n> ---\n";
        assert_eq!(warned_lines("MD023", input), vec![2]);
        assert_eq!(fixed("MD023", input), input);
    }

    #[test]
    fn a_tab_after_the_quote_marker_is_reported_but_not_rewritten() {
        let input = ">  \t## Tab\n";
        assert_eq!(warned_lines("MD023", input), vec![1]);
        assert_eq!(fixed("MD023", input), input);
    }

    #[test]
    fn a_tab_that_is_the_space_after_the_quote_marker_leaves_indentation_to_report() {
        // The tab spans to column 4; one column of it is the marker's own space.
        for input in [">\t## Tab\n", "-  >\t## Tab\n", ">  \t## Tab\n"] {
            assert_eq!(warned_lines("MD023", input), vec![1], "{input:?}");
            assert_eq!(fixed("MD023", input), input, "{input:?}");
        }
        assert_untouched("MD023", ">\t\t## Code\n");
    }

    #[test]
    fn a_quote_inside_a_list_item_is_judged_by_its_own_margin() {
        assert_fix("MD023", "- >  ## Bad\n", "- > ## Bad\n");
        assert_fix("MD023", "-   >   ## Bad\n", "-   > ## Bad\n");
        assert_fix("MD023", "1. >  ## Bad\n", "1. > ## Bad\n");
        assert_fix("MD023", "- - >   ## Bad\n", "- - > ## Bad\n");
        assert_fix("MD023", "> - >  ## Bad\n", "> - > ## Bad\n");
        assert_fix("MD023", "- >  Bad\n  >  ===\n", "- > Bad\n  > ===\n");
        assert_untouched("MD023", "- > ## Ok\n");
        assert_untouched("MD023", "- >     ## Code\n");
    }

    #[test]
    fn deep_continuation_of_quoted_setext_text_is_reported_but_not_rewritten() {
        // Unindented, the fence line would open a code fence and end the heading.
        let input = "> text\n>      ```\n> ---\n";
        assert_eq!(skeleton(input).len(), 1, "one setext heading: {:?}", skeleton(input));
        assert_eq!(warned_lines("MD023", input), vec![2]);
        assert_eq!(fixed("MD023", input), input);
    }
}

mod md063 {
    use super::*;

    #[test]
    fn a_quoted_heading_in_a_wide_list_item_is_recased() {
        assert_fix(
            "MD063",
            "-   item\n\n    > ## lower heading\n",
            "-   item\n\n    > ## Lower Heading\n",
        );
        assert_untouched("MD063", "> text\n>     ## lower heading\n");
    }

    #[test]
    fn a_marker_shaped_word_in_a_setext_continuation_stays_text() {
        assert_eq!(
            fixed("MD063", "> lower text\n>     >word\n> ---\n"),
            "> Lower Text\n>     >Word\n> ---\n"
        );
    }

    #[test]
    fn quoted_headings_are_recased_keeping_the_quote() {
        assert_fix("MD063", "> ## the quick brown fox\n", "> ## The Quick Brown Fox\n");
        assert_fix("MD063", "> > # lower deep\n", "> > # Lower Deep\n");
        assert_fix(
            "MD063",
            "> ## closed heading here ##\n",
            "> ## Closed Heading Here ##\n",
        );
        assert_fix("MD063", "> - ## in item here\n", "> - ## In Item Here\n");
        assert_fix("MD063", "- > ## quote in item here\n", "- > ## Quote in Item Here\n");
        assert_fix("MD063", "> the quick fox\n> ---\n", "> The Quick Fox\n> ---\n");
        assert_fix(
            "MD063",
            ">   indented setext words\n>   ---\n",
            ">   Indented Setext Words\n>   ---\n",
        );
        assert_fix(
            "MD063",
            "# Top Level\n\n> ## lower case here\n> text\n\n## lower case plain\n",
            "# Top Level\n\n> ## Lower Case Here\n> text\n\n## Lower Case Plain\n",
        );
    }

    #[test]
    fn a_quoted_setext_heading_over_several_lines_keeps_every_quote_marker() {
        assert_fix(
            "MD063",
            "> the quick\n> brown fox\n> ===\n",
            "> The Quick\n> Brown Fox\n> ===\n",
        );
        assert_fix(
            "MD063",
            "> > the quick\\\n> > brown fox\n> > ---\n",
            "> > The Quick\\\n> > Brown Fox\n> > ---\n",
        );
    }

    #[test]
    fn quoted_headings_are_reported_on_their_lines() {
        assert_eq!(
            warned_lines("MD063", "# Top Level\n\n> ## lower here\n>\n> > # lower deep\n"),
            vec![3, 5]
        );
        assert_eq!(warned_lines("MD063", "> the quick\n> brown fox\n> ===\n"), vec![1]);
    }

    #[test]
    fn text_that_is_not_a_heading_is_left_alone() {
        assert_untouched("MD063", "> text\n>     ## not heading here\n");
        assert_untouched("MD063", ">     ## code here\n");
        assert_untouched("MD063", "> ```\n> ## fenced here\n> ```\n");
        assert_untouched("MD063", "> ## Already Correct\n");
    }
}

mod md013 {
    use super::*;

    /// `rumdl check --config <config> --fix` over `content`, and the lines
    /// MD013 reports before the fix.
    fn with_config(config: &str, content: &str) -> (Vec<usize>, String) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        let config_path = dir.path().join("rumdl.toml");
        fs::write(&path, content).unwrap();
        fs::write(&config_path, config).unwrap();
        let rumdl = |extra: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(["check", "--no-cache", "--enable", "MD013", "--config"])
                .arg(&config_path)
                .args(extra)
                .arg(&path)
                .output()
                .expect("run rumdl check")
        };
        let checked = rumdl(&["--output-format=json"]);
        let lines = reported_lines("MD013", &String::from_utf8_lossy(&checked.stdout));
        rumdl(&["--fix"]);
        (lines, fs::read_to_string(&path).unwrap())
    }

    const SHORT: &str = "[MD013]\nline-length = 20\n";
    const NO_HEADINGS: &str = "[MD013]\nline-length = 20\nheadings = false\n";
    const HEADING_LIMIT: &str = "[MD013]\nline-length = 20\nheading-line-length = 60\n";
    const REFLOW: &str = "[MD013]\nline-length = 20\nreflow = true\nreflow-mode = \"normalize\"\n";

    const LONG: &str = "a long heading well past twenty";

    #[test]
    fn a_quoted_heading_is_a_heading_for_the_headings_option() {
        for quoted in [
            format!("> ## {LONG}\n"),
            format!("> > # {LONG}\n"),
            format!("> {LONG}\n> ---\n"),
            format!("> {LONG}\n> more text\n> ===\n"),
            format!("- > ## {LONG}\n"),
        ] {
            let (reported, _) = with_config(SHORT, &quoted);
            assert!(!reported.is_empty(), "positive control: {quoted:?} is too long");
            let (reported, _) = with_config(NO_HEADINGS, &quoted);
            assert_eq!(reported, Vec::<usize>::new(), "headings = false: {quoted:?}");
        }
    }

    #[test]
    fn a_quoted_heading_takes_the_heading_line_length() {
        for quoted in [format!("> ## {LONG}\n"), format!("> {LONG}\n> ---\n")] {
            let (reported, _) = with_config(HEADING_LIMIT, &quoted);
            assert_eq!(reported, Vec::<usize>::new(), "heading-line-length: {quoted:?}");
        }
        // The same text as a paragraph keeps the paragraph limit.
        let (reported, _) = with_config(HEADING_LIMIT, &format!("> {LONG}\n"));
        assert_eq!(reported, vec![1]);
    }

    #[test]
    fn text_that_is_not_a_heading_keeps_the_paragraph_limit() {
        let (reported, _) = with_config(NO_HEADINGS, &format!("> text\n>     ## {LONG}\n"));
        assert_eq!(reported, vec![2]);
        let (reported, _) = with_config(NO_HEADINGS, &format!(">     ## {LONG}\n"));
        assert_eq!(reported, vec![1]);
    }

    #[test]
    fn a_quoted_setext_heading_is_not_reflowed() {
        let heading = format!("> {LONG}\n> ---\n");
        assert_eq!(with_config(REFLOW, &heading).1, heading);
        let heading = format!("> > {LONG}\n> > ===\n");
        assert_eq!(with_config(REFLOW, &heading).1, heading);
        // The paragraph after it is reflowed: the skip ends with the heading.
        let (_, fixed) = with_config(REFLOW, &format!("> {LONG}\n> ---\n>\n> {LONG}\n"));
        assert_eq!(
            fixed,
            format!("> {LONG}\n> ---\n>\n> a long heading\n> well past twenty\n")
        );
        // Positive control: the same text without an underline wraps.
        let (_, fixed) = with_config(REFLOW, &format!("> {LONG}\n"));
        assert_eq!(fixed, "> a long heading\n> well past twenty\n");
    }
}

mod md022 {
    use super::*;

    #[test]
    fn a_lazy_line_above_a_quoted_heading_gets_a_blank_line_in_the_quote() {
        assert_fix("MD022", "> text\nlazy\n> ## H\n", "> text\nlazy\n>\n> ## H\n");
        assert_fix("MD022", "> > text\nlazy\n> > ## H\n", "> > text\nlazy\n> >\n> > ## H\n");
        assert_fix(
            "MD022",
            "> > text\n> lazy\n> > ## H\n",
            "> > text\n> lazy\n> >\n> > ## H\n",
        );
        assert_fix("MD022", "> a\n> > ## H\n", "> a\n>\n> > ## H\n");
    }

    #[test]
    fn a_bare_marker_is_compared_to_the_heading_inside_each_lines_own_quote_margin() {
        assert_untouched("MD022", "> > -\n>>   ## a\n");
        assert_untouched("MD022", "> > -\n> >   ## a\n");
        assert_eq!(fixed("MD022", "> > -\n>> ## a\n"), "> > -\n>>\n>> ## a\n");
    }

    #[test]
    fn quoted_headings_get_blank_lines_that_keep_the_quote() {
        assert_fix("MD022", "> text\n> ## H\n> more\n", "> text\n>\n> ## H\n>\n> more\n");
        assert_fix("MD022", "> # A\n> text\n", "> # A\n>\n> text\n");
        assert_fix("MD022", "> text\n> # A\n", "> text\n>\n> # A\n");
        assert_fix(
            "MD022",
            "> > text\n> > ## H\n> > more\n",
            "> > text\n> >\n> > ## H\n> >\n> > more\n",
        );
        assert_fix(
            "MD022",
            "- > text\n  > ## H\n  > more\n",
            "- > text\n  >\n  > ## H\n  >\n  > more\n",
        );
        assert_fix(
            "MD022",
            "> text\n> ```\n> x\n> ```\n> ## H\n",
            "> text\n> ```\n> x\n> ```\n>\n> ## H\n",
        );
        assert_fix(
            "MD022",
            "> text\n>\n> H\n> ---\n> more\n",
            "> text\n>\n> H\n> ---\n>\n> more\n",
        );
    }

    #[test]
    fn a_heading_under_a_bare_marker_in_a_quote_stays_in_its_item() {
        // A blank line above would leave the item empty and end it.
        assert_fix("MD022", "> -\n>   ## a\n", "> -\n>   ## a\n");
        // Text below the heading makes the tight item loose, as it does unquoted.
        assert_eq!(
            fixed("MD022", "> -\n>   ## a\n>   text\n"),
            "> -\n>   ## a\n>\n>   text\n"
        );
        // A quote opened inside the item keeps the heading in it too.
        assert_untouched("MD022", "> -\n>   > ## a\n");
        // Less indented than the item's content, the heading is outside the item.
        assert_fix("MD022", "> -\n> ## a\n", "> -\n>\n> ## a\n");
    }

    #[test]
    fn quoted_headings_are_reported_on_their_lines() {
        assert_eq!(warned_lines("MD022", "# T\n\n> text\n> ## H\n> more\n"), vec![4, 4]);
        assert_eq!(warned_lines("MD022", "> # A\n> text\n"), vec![1]);
    }

    #[test]
    fn a_quoted_heading_at_the_start_gets_no_blank_line_above() {
        assert_eq!(fixed("MD022", "> ## H\n> text\n"), "> ## H\n>\n> text\n");
    }

    #[test]
    fn separated_headings_and_non_headings_are_left_alone() {
        assert_untouched("MD022", "> text\n>\n> ## H\n>\n> more\n");
        assert_untouched("MD022", "> > text\n> >\n> > ## H\n> >\n> > more\n");
        assert_untouched("MD022", "> paragraph\n>     ## B\n");
        assert_untouched("MD022", "> text\n> ```\n> ## H\n> ```\n");
    }
}
