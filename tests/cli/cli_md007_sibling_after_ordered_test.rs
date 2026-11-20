//! A bullet left of a preceding ordered item's content column is a separate list.
//!
//! The oracle renders the document with pulldown-cmark and compares the list
//! tree before and after `rumdl check --fix`: every fix must keep each item at
//! the same depth under the same kind of list, so a sibling is never rewritten
//! into a child (or the reverse).

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Every list item in document order: its list nesting depth, whether its list
/// is ordered, and its first text.
fn list_tree(content: &str) -> Vec<(usize, bool, String)> {
    let mut stack: Vec<bool> = Vec::new();
    let mut items: Vec<(usize, bool, String)> = Vec::new();
    let mut pending: Option<usize> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(start)) => stack.push(start.is_some()),
            Event::End(TagEnd::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                items.push((stack.len(), *stack.last().unwrap(), String::new()));
                pending = Some(items.len() - 1);
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some(index) = pending.take() {
                    items[index].2 = text.to_string();
                }
            }
            _ => {}
        }
    }
    items
}

struct Run {
    output: Output,
    fixed: String,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).to_string()
    }
}

fn run(content: &str, config: Option<&str>, fix: bool) -> Run {
    let args: &[&str] = if fix { &["check", "--fix"] } else { &["check"] };
    run_command(content, config, args)
}

fn run_command(content: &str, config: Option<&str>, args: &[&str]) -> Run {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(args).args(["--no-cache", "--enable", "MD007"]);
    if let Some(config) = config {
        let config_path = dir.path().join("rumdl.toml");
        fs::write(&config_path, config).unwrap();
        command.arg("--config").arg(&config_path);
    } else {
        command.arg("--isolated");
    }
    let output = command.arg(&path).output().expect("run rumdl");
    let fixed = fs::read_to_string(&path).unwrap();
    Run { output, fixed }
}

const REPORT: &str = "1. a\n  - b\n\n10. c\n   - d\n\n10. e\n\nText.\n\n   - f\n";
const FIXED_STYLE: &str = "[MD007]\nindent = 2\nstyle = \"fixed\"\n";
const INDENT_ALONE: &str = "[MD007]\nindent = 2\n";

fn diagnostics(run: &Run) -> Vec<String> {
    run.stdout()
        .lines()
        .filter(|line| line.contains("[MD007]"))
        .map(|line| {
            // Drop the temp-dir prefix the CLI prints before `doc.md`.
            line.rsplit_once("doc.md:").map_or(line, |(_, rest)| rest).to_string()
        })
        .collect()
}

#[test]
fn default_style_reports_siblings_without_a_fix_and_fixes_the_paragraph_bullet() {
    let before = run(REPORT, None, false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2",
            "5:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 4 spaces to nest under the preceding ordered item, found 3",
            "11:1: [MD007] Expected 0 spaces for indent depth 0, found 3 [*]",
        ]
    );

    let fixed = run(REPORT, None, true);
    assert_eq!(fixed.fixed, "1. a\n  - b\n\n10. c\n   - d\n\n10. e\n\nText.\n\n- f\n");
    assert_eq!(
        list_tree(&fixed.fixed),
        list_tree(REPORT),
        "the fix keeps the list tree"
    );

    let again = run(&fixed.fixed, None, true);
    assert_eq!(again.fixed, fixed.fixed, "a second pass changes nothing");
}

#[test]
fn explicit_indent_alone_agrees_with_the_default() {
    let default = run(REPORT, None, false);
    let explicit = run(REPORT, Some(INDENT_ALONE), false);
    assert_eq!(diagnostics(&explicit), diagnostics(&default));
}

#[test]
fn fixed_style_moves_every_sibling_to_column_zero_without_changing_the_tree() {
    let before = run(REPORT, Some(FIXED_STYLE), false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for indent depth 0, found 2 [*]",
            "5:1: [MD007] Expected 0 spaces for indent depth 0, found 3 [*]",
            "11:1: [MD007] Expected 0 spaces for indent depth 0, found 3 [*]",
        ]
    );
    let fixed = run(REPORT, Some(FIXED_STYLE), true);
    assert_eq!(fixed.fixed, "1. a\n- b\n\n10. c\n- d\n\n10. e\n\nText.\n\n- f\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(REPORT));
    assert!(
        fixed.output.status.success(),
        "nothing is left to report: {:?}",
        fixed.output
    );
    assert_eq!(run(&fixed.fixed, Some(FIXED_STYLE), true).fixed, fixed.fixed);
}

#[test]
fn fixed_style_children_of_a_sibling_bullet_keep_their_nesting() {
    let content = "1. a\n  - b\n    - c\n      - d\n";
    let fixed = run(content, Some(FIXED_STYLE), true);
    assert_eq!(fixed.fixed, "1. a\n- b\n  - c\n    - d\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
    assert_eq!(run(&fixed.fixed, Some(FIXED_STYLE), true).fixed, fixed.fixed);
}

#[test]
fn default_style_children_of_a_sibling_bullet_are_not_reported() {
    let content = "1. a\n  - b\n    - c\n      - d\n";
    let before = run(content, None, false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2"
        ]
    );
    assert_eq!(
        list_tree(content),
        vec![
            (1, true, "a".to_string()),
            (1, false, "b".to_string()),
            (2, false, "c".to_string()),
            (3, false, "d".to_string()),
        ],
        "the oracle agrees that b starts a separate list with c and d below it"
    );
}

/// Runs `rumdl check --fix` with every rule enabled and returns the MD007
/// diagnostics of the original document and the rewritten document.
fn fix_with_all_rules(content: &str, config: Option<&str>) -> (Vec<String>, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let check = |fix: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command.args(["check", "--no-cache"]);
        if let Some(config) = config {
            let config_path = dir.path().join("rumdl.toml");
            fs::write(&config_path, config).unwrap();
            command.arg("--config").arg(&config_path);
        } else {
            command.arg("--isolated");
        }
        if fix {
            command.arg("--fix");
        }
        command.arg(&path).output().expect("run rumdl")
    };
    fs::write(&path, content).unwrap();
    let before = Run {
        output: check(false),
        fixed: String::new(),
    };
    check(true);
    (diagnostics(&before), fs::read_to_string(&path).unwrap())
}

const SIBLING_WARNING: &str = "4:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2";

#[test]
fn later_siblings_of_an_unfixed_bullet_stay_aligned_with_it() {
    // The list after `1. a` is reported once, at its first bullet. Its other
    // bullets are aligned with that one, and moving any of them alone would
    // leave a ragged list.
    let indent_four = "[MD007]\nindent = 4\n";
    for (content, config) in [
        ("# T\n\n1. a\n  - b\n  - c\n  - d\n", None),
        ("# T\n\n1. a\n  - b\n  - c\n  - d\n", Some(INDENT_ALONE)),
        ("# T\n\n1. a\n  - b\n  - c\n  - d\n", Some(indent_four)),
        ("# T\n\n1. a\n  - b\n    - child\n  - c\n", None),
        ("# T\n\n1. a\n  - b\n    - child\n  - c\n", Some(INDENT_ALONE)),
    ] {
        let (before, fixed) = fix_with_all_rules(content, config);
        assert_eq!(before, [SIBLING_WARNING], "{content:?}, {config:?}");
        assert_eq!(fixed, content, "{content:?}, {config:?}: nothing is rewritten");
    }
}

#[test]
fn a_later_sibling_at_the_margin_is_accepted() {
    let content = "# T\n\n1. a\n  - b\n\n- c\n";
    let (before, fixed) = fix_with_all_rules(content, None);
    assert_eq!(before, [SIBLING_WARNING]);
    assert_eq!(fixed, content);
}

#[test]
fn a_misaligned_later_sibling_is_moved_to_the_first_ones_column() {
    let content = "# T\n\n1. a\n  - b\n  - c\n   - d\n";
    let (before, fixed) = fix_with_all_rules(content, None);
    assert_eq!(
        before,
        [
            SIBLING_WARNING,
            "6:1: [MD007] Expected 2 spaces for indent depth 0, found 3 [*]"
        ]
    );
    assert_eq!(fixed, "# T\n\n1. a\n  - b\n  - c\n  - d\n");
    assert_eq!(list_tree(&fixed), list_tree(content));
    assert_eq!(
        fix_with_all_rules(&fixed, None).1,
        fixed,
        "a second pass changes nothing"
    );
}

#[test]
fn fixed_style_moves_later_siblings_together_under_every_rule() {
    // Positive control for the three tests above: with an explicit fixed style
    // the whole list does move, so an unchanged document there is a verdict
    // and not a run that fixed nothing.
    let content = "# T\n\n1. a\n  - b\n  - c\n  - d\n";
    let (_, fixed) = fix_with_all_rules(content, Some(FIXED_STYLE));
    assert_eq!(fixed, "# T\n\n1. a\n\n- b\n- c\n- d\n");
    assert_eq!(list_tree(&fixed), list_tree(content));
}

#[test]
fn genuine_children_are_neither_reported_nor_moved() {
    // Positive control for the sweeps above: bullets at or right of the ordered
    // content column are children, and no configuration touches them.
    let content = "1. a\n   - b\n\n10. c\n    - d\n";
    for config in [None, Some(INDENT_ALONE), Some(FIXED_STYLE)] {
        let result = run(content, config, true);
        assert!(result.output.status.success(), "{config:?}: {:?}", result.output);
        assert_eq!(result.fixed, content, "{config:?}");
    }
    assert_eq!(list_tree(content)[1].0, 2, "the oracle sees b as a child");
}

#[test]
fn a_misindented_bullet_list_is_still_fixed() {
    // Negative control: the rule still rewrites a real misindentation.
    let content = "- a\n   - b\n";
    let result = run(content, None, true);
    assert_eq!(result.fixed, "- a\n  - b\n");
    assert_eq!(list_tree(&result.fixed), list_tree(content));
}

#[test]
fn a_wide_unordered_root_does_not_change_the_separate_list_target() {
    let content = "-   root\n  1. a\n   - b\n";
    let before = run(content, None, false);
    assert_eq!(
        diagnostics(&before),
        [
            "3:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 5 spaces to nest under the preceding ordered item, found 3"
        ]
    );
    let fixed = run(content, None, true);
    assert_eq!(fixed.fixed, content, "no autofix is offered");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn indent_four_leaves_children_of_a_sibling_bullet_where_they_are() {
    let content = "1. a\n  - b\n      - c\n          - d\n";
    let config = "[MD007]\nindent = 4\n";
    let before = run(content, Some(config), false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2"
        ]
    );
    assert_eq!(run(content, Some(config), true).fixed, content);
}

#[test]
fn fixed_style_with_an_unusable_start_indent_names_the_nesting_position() {
    let content = "1. a\n  - b\n";
    let config = "[MD007]\nstyle = \"fixed\"\nindent = 4\nstart-indented = true\nstart-indent = 3\n";
    let before = run(content, Some(config), false);
    assert_eq!(
        diagnostics(&before),
        ["2:1: [MD007] Expected 4 spaces to nest under the preceding ordered item, found 2"]
    );
    assert_eq!(run(content, Some(config), true).fixed, content);
    assert!(
        diagnostics(&run("1. a\n    - b\n", Some(config), false)).is_empty(),
        "the named position is accepted"
    );
}

#[test]
fn a_one_space_sibling_is_still_moved_to_the_margin() {
    let content = "1. a\n - b\n";
    let fixed = run(content, None, true);
    assert_eq!(fixed.fixed, "1. a\n- b\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn a_bullet_left_of_a_sibling_bullets_content_is_judged_as_its_sibling() {
    let content = "1. a\n  - b\n   - c\n";
    let before = run(content, None, false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2",
            "3:1: [MD007] Expected 2 spaces for indent depth 0, found 3 [*]",
        ]
    );
    let fixed = run(content, None, true);
    assert_eq!(fixed.fixed, "1. a\n  - b\n  - c\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn fixed_style_moves_adjacent_siblings_after_an_ordered_item_together() {
    let content = "1. a\n  - b\n   - c\n";
    let fixed = run(content, Some(FIXED_STYLE), true);
    assert_eq!(fixed.fixed, "1. a\n- b\n- c\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn grandchildren_of_a_sibling_bullet_settle_after_one_fix() {
    let content = "1. a\n  - b\n       - c\n          - d\n";
    let config = "[MD007]\nindent = 4\n";
    let fixed = run(content, Some(config), true);
    assert_eq!(fixed.fixed, "1. a\n  - b\n      - c\n          - d\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
    let again = run(&fixed.fixed, Some(config), false);
    assert_eq!(
        diagnostics(&again),
        [
            "2:1: [MD007] Expected 0 spaces for a separate list at indent depth 0, or 3 spaces to nest under the preceding ordered item, found 2"
        ]
    );
}

#[test]
fn fixed_style_keeps_children_of_a_disabled_parent_in_place() {
    let content = "1. a\n  - b <!-- rumdl-disable-line MD007 -->\n    - c\n";
    let fixed = run_command(content, Some(FIXED_STYLE), &["fmt"]);
    assert!(fixed.output.status.success(), "{:?}", fixed.output);
    assert_eq!(fixed.fixed, content);
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));

    let misaligned = content.replace("    - c", "     - c");
    let fixed = run_command(&misaligned, Some(FIXED_STYLE), &["fmt"]);
    assert_eq!(fixed.fixed, content);
    assert_eq!(list_tree(&fixed.fixed), list_tree(&misaligned));
}

#[test]
fn fixed_style_keeps_children_and_siblings_aligned_when_the_root_cannot_move() {
    for indent in [4, 5] {
        let config =
            format!("[MD007]\nstyle = \"fixed\"\nindent = {indent}\nstart-indented = true\nstart-indent = 3\n");
        let content = "1. a\n  - b\n    - c\n  - d\n";
        assert_eq!(
            diagnostics(&run(content, Some(&config), false)),
            [format!(
                "2:1: [MD007] Expected {indent} spaces to nest under the preceding ordered item, found 2"
            )]
        );
        assert_eq!(run(content, Some(&config), true).fixed, content);
    }
}

#[test]
fn a_different_bullet_marker_starts_a_new_list() {
    for (prefix, expected) in [("", ""), ("> ", "> ")] {
        for indent in ["  ", "   "] {
            let content = format!("{prefix}1. a\n{prefix}  - b\n{prefix}{indent}+ c\n");
            let fixed = run(&content, None, true);
            assert_eq!(fixed.fixed, format!("{expected}1. a\n{expected}  - b\n{expected}+ c\n"));
            assert_eq!(list_tree(&fixed.fixed), list_tree(&content));
            assert_eq!(run(&fixed.fixed, None, true).fixed, fixed.fixed);
        }
    }
}

#[test]
fn fixed_style_avoids_advising_indent_that_cannot_start_a_list() {
    let config = "[MD007]\nstyle = \"fixed\"\nindent = 8\nstart-indented = true\nstart-indent = 3\n";
    let content = "1. a\n  - b\n    - c\n";
    assert_eq!(
        diagnostics(&run(content, Some(config), false)),
        [
            "2:1: [MD007] The MD007 configuration has no valid indent for this separate list or a nested bullet, found 2 spaces"
        ]
    );
    assert_eq!(run(content, Some(config), true).fixed, content);
}

#[test]
fn a_fixed_style_quick_fix_keeps_adjacent_lists_separate() {
    let content = "1. a\n  - b\n   + c\n";
    let before = run(content, Some(FIXED_STYLE), false);
    assert_eq!(
        diagnostics(&before),
        [
            "2:1: [MD007] Expected 0 spaces for indent depth 0, found 2 [*]",
            "3:1: [MD007] Expected 0 spaces for indent depth 0, found 3",
        ]
    );
    let fixed = run(content, Some(FIXED_STYLE), true);
    assert_eq!(fixed.fixed, "1. a\n- b\n+ c\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn a_nested_separate_list_keeps_other_items_quick_fixes_independent() {
    let content = "- x\n   - y\n- w\n   - z\n- root\n  1. a\n    - b\n";
    for config in [None, Some(FIXED_STYLE)] {
        let before = run(content, config, false);
        let lines = diagnostics(&before);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(
            lines[0],
            "2:1: [MD007] Expected 2 spaces for indent depth 1, found 3 [*]"
        );
        assert_eq!(
            lines[1],
            "4:1: [MD007] Expected 2 spaces for indent depth 1, found 3 [*]"
        );
        assert_eq!(lines[2].ends_with("[*]"), config.is_some(), "{lines:?}");
    }
}

#[test]
fn a_disabled_sibling_can_block_a_separate_lists_move() {
    let content = "1. a\n  - b\n  - c <!-- rumdl-disable-line MD007 -->\n";
    let before = run(content, Some(FIXED_STYLE), false);
    assert_eq!(
        diagnostics(&before),
        ["2:1: [MD007] Moving this separate list to column 0 would require other edits to preserve its structure"]
    );
    let fixed = run_command(content, Some(FIXED_STYLE), &["fmt"]);
    assert_eq!(fixed.fixed, content);
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn a_separate_lists_quick_fix_includes_its_moving_ancestor() {
    let content = "  - root\n    1. a\n      - b\n";
    let before = run(content, Some(FIXED_STYLE), false);
    assert_eq!(
        diagnostics(&before),
        [
            "1:1: [MD007] Expected 0 spaces for indent depth 0, found 2 [*]",
            "3:1: [MD007] Expected 2 spaces for indent depth 1, found 6",
        ]
    );
    let fixed = run(content, Some(FIXED_STYLE), true);
    assert_eq!(fixed.fixed, "- root\n    1. a\n  - b\n");
    assert_eq!(list_tree(&fixed.fixed), list_tree(content));
}

#[test]
fn an_inline_disable_keeps_either_misaligned_sibling_at_its_original_column() {
    let disable = " <!-- rumdl-disable-line MD007 -->";
    for (first, second) in [(disable, ""), ("", disable)] {
        let content = format!("10. x\n   - b\n  - c{first}\n  - d{second}\n");
        let expected_c = if first.is_empty() { "   - c" } else { "  - c" };
        let expected_d = if second.is_empty() { "   - d" } else { "  - d" };
        let expected = format!("10. x\n   - b\n{expected_c}{first}\n{expected_d}{second}\n");
        assert_eq!(run(&content, None, true).fixed, expected);
        assert_eq!(run_command(&content, None, &["fmt"]).fixed, expected);
        assert_eq!(list_tree(&expected), list_tree(&content));
    }
}
