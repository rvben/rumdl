//! MD029 `style = "zero"` against the real binary: every item is `0.`, a
//! markdownlint configuration that sets the style imports and is honored, and
//! the rewrite keeps the list structure pulldown-cmark sees.

use pulldown_cmark::{Event, Parser, Tag};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const ZERO_CONFIG: &str = "[global]\nenable = [\"MD029\"]\n[MD029]\nstyle = \"zero\"\n";

fn run(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .arg("--no-cache")
        .output()
        .expect("run rumdl")
}

/// The `check` output and `fix` result of `doc` under `config`.
fn check_and_fix(config: &str, doc: &str) -> (String, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), config).unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let checked = run(&["check", "--config", "rumdl.toml", "doc.md"], dir.path());
    assert!(matches!(checked.status.code(), Some(0 | 1)), "{checked:?}");
    let mut text = String::from_utf8_lossy(&checked.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&checked.stderr));
    let fixed = run(&["check", "--fix", "--config", "rumdl.toml", "doc.md"], dir.path());
    assert!(matches!(fixed.status.code(), Some(0 | 1)), "{fixed:?}");
    (text, fs::read_to_string(&path).unwrap())
}

/// Start number, nesting depth and item count of every ordered list, in
/// document order, so a list that merges into its parent shows up as a change.
fn lists(doc: &str) -> Vec<(u64, usize, usize)> {
    let mut out: Vec<(u64, usize, usize)> = Vec::new();
    let mut stack: Vec<Option<usize>> = Vec::new();
    for event in Parser::new(doc) {
        match event {
            Event::Start(Tag::List(start)) => {
                let index = start.map(|start| {
                    out.push((start, stack.len(), 0));
                    out.len() - 1
                });
                stack.push(index);
            }
            Event::End(pulldown_cmark::TagEnd::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                if let Some(&Some(i)) = stack.last() {
                    out[i].2 += 1;
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn zero_style_is_accepted_and_all_zero_list_is_clean() {
    let (out, fixed) = check_and_fix(ZERO_CONFIG, "0. a\n0. b\n0. c\n");
    assert!(!out.contains("MD029"), "{out}");
    assert!(
        !out.to_lowercase().contains("unknown") && !out.contains("Invalid"),
        "{out}"
    );
    assert_eq!(fixed, "0. a\n0. b\n0. c\n");
}

#[test]
fn default_style_still_flags_an_all_zero_list() {
    let (out, _) = check_and_fix("[global]\nenable = [\"MD029\"]\n", "0. a\n0. b\n0. c\n");
    assert_eq!(out.matches("[MD029]").count(), 2, "{out}");
}

#[test]
fn zero_style_renumbers_and_keeps_structure() {
    for (doc, expected) in [
        ("1. a\n2. b\n3. c\n", "0. a\n0. b\n0. c\n"),
        ("- x\n\n  1. a\n  2. b\n", "- x\n\n  0. a\n  0. b\n"),
        (
            "1. a\n\n   1. x\n   2. y\n\n2. b\n",
            "0. a\n\n   0. x\n   0. y\n\n0. b\n",
        ),
        ("5. a\n6. b\n", "0. a\n0. b\n"),
    ] {
        let (out, fixed) = check_and_fix(ZERO_CONFIG, doc);
        assert!(out.contains("configured style 'zero'"), "{doc:?}: {out}");
        assert_eq!(fixed, expected, "{doc:?}");
        // Same lists, depths and item counts; only start numbers differ.
        let before: Vec<_> = lists(doc).iter().map(|l| (l.1, l.2)).collect();
        let after: Vec<_> = lists(&fixed).iter().map(|l| (l.1, l.2)).collect();
        assert_eq!(before, after, "{doc:?}");
        assert!(lists(&fixed).iter().all(|l| l.0 == 0), "{doc:?}");
        let (again, refixed) = check_and_fix(ZERO_CONFIG, &fixed);
        assert!(!again.contains("[MD029]"), "{again}");
        assert_eq!(refixed, fixed);
    }
}

#[test]
fn imported_markdownlint_zero_style_is_honored() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("in.json"), r#"{"MD029": {"style": "zero"}}"#).unwrap();
    let imported = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["import", "in.json", "--output", "out.toml"])
        .output()
        .expect("run rumdl");
    assert!(imported.status.success(), "{imported:?}");
    let toml = fs::read_to_string(dir.path().join("out.toml")).unwrap();
    assert!(toml.contains("[MD029]") && toml.contains("style = \"zero\""), "{toml}");

    let config = format!("[global]\nenable = [\"MD029\"]\n{toml}");
    let (out, _) = check_and_fix(&config, "0. a\n0. b\n");
    assert!(!out.contains("[MD029]"), "{out}");
    let (out, fixed) = check_and_fix(&config, "1. a\n2. b\n");
    assert_eq!(out.matches("[MD029]").count(), 2, "{out}");
    assert_eq!(fixed, "0. a\n0. b\n");
}

#[test]
fn zero_style_does_not_turn_a_nested_list_into_paragraph_text() {
    // `0.` cannot interrupt a paragraph, so renumbering the nested list would
    // merge it into its parent item. The oracle shows why the fix is withheld.
    let doc = "1. a\n   1. x\n   2. y\n2. b\n";
    assert_eq!(lists(doc), vec![(1, 0, 2), (1, 1, 2)]);
    assert_eq!(lists("0. a\n   0. x\n   0. y\n0. b\n"), vec![(0, 0, 2)]);
    let (out, fixed) = check_and_fix(ZERO_CONFIG, doc);
    assert!(out.contains("[MD029]"), "{out}");
    assert_eq!(fixed, doc);
}

#[test]
fn zero_style_does_not_turn_a_list_after_a_paragraph_into_text() {
    // Same guard for a top-level list: after paragraph text only a list that
    // starts at 1 interrupts it, so the fix is withheld.
    let doc = "intro\n1. a\n2. b\n";
    assert_eq!(lists(doc), vec![(1, 0, 2)]);
    assert!(lists("intro\n0. a\n0. b\n").is_empty());
    let (out, fixed) = check_and_fix(ZERO_CONFIG, doc);
    assert!(out.contains("[MD029]"), "{out}");
    assert_eq!(fixed, doc);
}
