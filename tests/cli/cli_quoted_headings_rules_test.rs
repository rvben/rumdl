//! MD001, MD024, MD025 and MD043 treat a heading inside a blockquote as a heading
//! of the document, like a heading inside a list item, against the real binary.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Per heading in document order: its level, its text, and the list, item and
/// blockquote events it is nested in, outermost first.
type Headings = Vec<(u8, String, Vec<&'static str>)>;

fn headings(content: &str) -> Headings {
    let mut path: Vec<&'static str> = Vec::new();
    let mut out = Vec::new();
    let mut current: Option<(u8, String, Vec<&'static str>)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::Heading { level, .. }) => current = Some((level as u8, String::new(), path.clone())),
            Event::Text(text) | Event::Code(text) => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => out.extend(current.take()),
            _ => {}
        }
    }
    out
}

/// Runs `rumdl check` on `content` with only `rule` enabled and `config` appended
/// to the config file. Returns the output and the document afterwards.
fn run(content: &str, rule: &str, config: &str, fix: bool) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config_path = dir.path().join("rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(&config_path, format!("[global]\nenable = [\"{rule}\"]\n\n{config}\n")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["check", "--no-cache", "--config"]).arg(&config_path);
    if fix {
        command.arg("--fix");
    }
    let output = command.arg(&path).output().expect("run rumdl check");
    (output, fs::read_to_string(&path).unwrap())
}

fn lines(output: &Output, rule: &str) -> Vec<usize> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&format!("[{rule}]")))
        .filter_map(|line| line.split(':').nth(1)?.trim().parse().ok())
        .collect()
}

mod md024 {
    use super::*;

    #[test]
    fn quoted_duplicate_is_reported() {
        for (src, line) in [
            ("# Same\n\n> # Same\n", 3),
            ("> # Same\n\n> # Same\n", 3),
            ("# Same\n\n> > # Same\n", 3),
            ("# Same\n\n- x\n\n  > # Same\n", 5),
            ("# Same\n\n> - # Same\n", 3),
        ] {
            let (output, _) = run(src, "MD024", "", false);
            assert_eq!(lines(&output, "MD024"), vec![line], "{src:?}: {output:?}");
        }
    }

    #[test]
    fn distinct_quoted_headings_are_clean() {
        let (output, _) = run("# A\n\n> # B\n", "MD024", "", false);
        assert!(lines(&output, "MD024").is_empty(), "{output:?}");
    }

    #[test]
    fn fix_changes_nothing() {
        let src = "# Same\n\n> # Same\n";
        let (_, fixed) = run(src, "MD024", "", true);
        assert_eq!(fixed, src);
    }
}

mod md043 {
    use super::*;

    const CONFIG: &str = "[MD043]\nheadings = [\"# A\", \"## B\"]\n";

    #[test]
    fn unexpected_quoted_heading_is_reported() {
        for (src, line) in [
            ("# A\n\n## B\n\n> ## Extra\n", 5),
            ("# A\n\n## B\n\n> > ## Extra\n", 5),
            ("# A\n\n## B\n\n- x\n\n  > ## Extra\n", 7),
            ("# A\n\n## B\n\n> - ## Extra\n", 5),
        ] {
            let (output, _) = run(src, "MD043", CONFIG, false);
            assert_eq!(lines(&output, "MD043"), vec![line], "{src:?}: {output:?}");
        }
    }

    #[test]
    fn matching_quoted_heading_is_clean() {
        let (output, _) = run("# A\n\n> ## B\n", "MD043", CONFIG, false);
        assert!(lines(&output, "MD043").is_empty(), "{output:?}");
    }

    #[test]
    fn quote_only_document_is_checked() {
        let (output, _) = run("> ## Wrong\n", "MD043", CONFIG, false);
        assert!(!lines(&output, "MD043").is_empty(), "{output:?}");
    }
}

mod md001 {
    use super::*;

    /// Wraps `body` (one or more lines) in each container shape.
    fn shapes(body: &[&str]) -> Vec<(&'static str, String)> {
        let quote = |prefix: &str| body.iter().map(|l| format!("{prefix}{l}\n")).collect::<String>();
        vec![
            ("top", quote("")),
            ("quote", quote("> ")),
            ("nested", quote("> > ")),
            ("quote in list", format!("- x\n\n{}", quote("  > "))),
            ("list in quote", format!("> - x\n>\n{}", quote(">   "))),
            ("quote in quote list", format!("> - x\n>\n{}", quote(">   > "))),
        ]
    }

    #[test]
    fn skipped_level_is_reported_in_every_shape() {
        for (name, src) in shapes(&["# A", "", "### C"]) {
            let (output, _) = run(&src, "MD001", "", false);
            assert_eq!(lines(&output, "MD001").len(), 1, "{name}: {src:?}: {output:?}");
        }
    }

    #[test]
    fn quoted_heading_after_unquoted_is_checked_in_source_order() {
        let (output, _) = run("# A\n\n> ### C\n", "MD001", "", false);
        assert_eq!(lines(&output, "MD001"), vec![3], "{output:?}");
        let (output, _) = run("> # A\n\n### C\n", "MD001", "", false);
        assert_eq!(lines(&output, "MD001"), vec![3], "{output:?}");
    }

    #[test]
    fn quote_only_document_is_not_skipped() {
        let (output, _) = run("> # A\n>\n> ### C\n", "MD001", "", false);
        assert_eq!(lines(&output, "MD001"), vec![3], "{output:?}");
    }

    #[test]
    fn atx_fix_only_changes_levels_and_keeps_every_container() {
        for (name, src) in shapes(&["# A", "", "#### C", "", "###### E", ""]) {
            let before = headings(&src);
            let (_, fixed) = run(&src, "MD001", "", true);
            let after = headings(&fixed);
            assert_eq!(before.len(), after.len(), "{name}: {fixed:?}");
            let levels: Vec<u8> = after.iter().map(|h| h.0).collect();
            assert_eq!(levels, vec![1, 2, 3], "{name}: {fixed:?}");
            for (b, a) in before.iter().zip(&after) {
                assert_eq!((&b.1, &b.2), (&a.1, &a.2), "{name}: structure changed: {fixed:?}");
            }
            let (output, _) = run(&fixed, "MD001", "", false);
            assert!(lines(&output, "MD001").is_empty(), "{name}: not idempotent: {fixed:?}");
        }
    }

    #[test]
    fn atx_fix_keeps_closing_sequence_and_indent() {
        let (_, fixed) = run("> # A\n>\n>   ### C ###\n", "MD001", "", true);
        assert_eq!(fixed, "> # A\n>\n>   ## C ###\n");
    }

    #[test]
    fn quoted_setext_headings_never_skip_a_level_and_are_left_alone() {
        // A setext heading is level 1 or 2, so it can never jump past the previous
        // level and there is nothing to report or rewrite.
        for (name, src) in shapes(&["Title", "=====", "", "Sub", "---", ""]) {
            let (output, fixed) = run(&src, "MD001", "", true);
            assert!(lines(&output, "MD001").is_empty(), "{name}: {output:?}");
            assert_eq!(fixed, src, "{name}");
        }
    }

    #[test]
    fn quoted_setext_counts_as_the_previous_level() {
        let src = "> Sub\n> ---\n\n#### C\n";
        let (output, fixed) = run(src, "MD001", "", true);
        assert_eq!(lines(&output, "MD001"), vec![4], "{output:?}");
        assert_eq!(fixed, "> Sub\n> ---\n\n### C\n");
    }
}
