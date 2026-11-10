//! MD046 in `consistent` mode picks the style most blocks use, so the tally
//! has to count the blocks the parser sees. A fence line inside a longer or
//! different-character fence is the fence's text, not a delimiter: it must not
//! end the block early and hide the indented blocks that follow. The oracle is
//! pulldown-cmark: fixing must not change the text of any code block.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Stdio};
use tempfile::tempdir;

const HEAD: &str = "# T\n\nPara.\n\n    indented one\n\nPara.\n\n    indented two\n\nPara.\n\n";

struct Case {
    name: &'static str,
    input: String,
    /// 1-based lines of the warnings, with their message.
    warnings: Vec<(usize, &'static str)>,
    fixed: String,
}

fn cases() -> Vec<Case> {
    let indented = "Use indented code blocks";
    let fenced = "Use fenced code blocks";
    let with_head = |tail: &str| format!("{HEAD}{tail}");
    let fixed_head = "# T\n\nPara.\n\n    indented one\n\nPara.\n\n    indented two\n\nPara.\n\n";
    vec![
        Case {
            name: "tilde fence holding a backtick fence line",
            input: with_head("~~~\n```\n~~~\n"),
            warnings: vec![(13, indented)],
            fixed: format!("{fixed_head}    ```\n"),
        },
        Case {
            name: "four backticks holding a three backtick line",
            input: with_head("````\n```\n````\n"),
            warnings: vec![(13, indented)],
            fixed: format!("{fixed_head}    ```\n"),
        },
        Case {
            name: "four tildes holding a three tilde line",
            input: with_head("~~~~\n~~~\n~~~~\n"),
            warnings: vec![(13, indented)],
            fixed: format!("{fixed_head}    ~~~\n"),
        },
        Case {
            name: "fence inside a list item",
            input: with_head("- item\n\n  ~~~\n  ```\n  ~~~\n"),
            warnings: vec![(15, indented)],
            fixed: format!("{fixed_head}- item\n\n      ```\n"),
        },
        Case {
            name: "negative control: plain body, fenced is the minority",
            input: with_head("~~~\nplain\n~~~\n"),
            warnings: vec![(13, indented)],
            fixed: format!("{fixed_head}    plain\n"),
        },
        Case {
            name: "fence-like lines inside an HTML comment are not blocks",
            input: with_head("<!--\n~~~\na\n~~~\n\n~~~\nb\n~~~\n\n~~~\nc\n~~~\n-->\n"),
            warnings: vec![],
            fixed: with_head("<!--\n~~~\na\n~~~\n\n~~~\nb\n~~~\n\n~~~\nc\n~~~\n-->\n"),
        },
        Case {
            name: "fenced blocks outnumber the indented block",
            input: "# T\n\nPara.\n\n    indented one\n\nPara.\n\n~~~\n```\n~~~\n\nPara.\n\n~~~\n```\n~~~\n".to_string(),
            warnings: vec![(5, fenced)],
            fixed: "# T\n\nPara.\n\n```\nindented one\n```\n\nPara.\n\n~~~\n```\n~~~\n\nPara.\n\n~~~\n```\n~~~\n"
                .to_string(),
        },
        Case {
            name: "unrelated closer after an unclosed fence is not a second block",
            input: with_head("~~~\n```\n"),
            warnings: vec![],
            fixed: String::new(),
        },
    ]
}

fn rumdl(args: &[&str], path: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rumdl");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn code_block_texts(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => open = Some(String::new()),
            Event::Text(text) => {
                if let Some(block) = &mut open {
                    block.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => blocks.extend(open.take()),
            _ => {}
        }
    }
    blocks
}

fn warnings(content: &str) -> Vec<(usize, String)> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    rumdl(&["check", "--no-cache", "--isolated", "--enable", "MD046"], &path)
        .lines()
        .filter_map(|line| {
            // The path can itself contain colons (a Windows drive letter), so
            // the row and column are read from the end of the location.
            let (location, message) = line.split_once(": [MD046] ")?;
            let mut fields = location.rsplitn(3, ':');
            fields.next()?;
            let row = fields
                .next()
                .and_then(|row| row.parse().ok())
                .unwrap_or_else(|| panic!("no row in {line:?}"));
            Some((row, message.trim().trim_end_matches(" [*]").to_string()))
        })
        .collect()
}

fn fix(content: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    rumdl(
        &["check", "--no-cache", "--fix", "--isolated", "--enable", "MD046"],
        &path,
    );
    fs::read_to_string(&path).unwrap()
}

#[test]
fn consistent_style_counts_the_blocks_the_parser_sees() {
    for case in cases().iter().filter(|case| !case.fixed.is_empty()) {
        let got = warnings(&case.input);
        let want: Vec<(usize, String)> = case.warnings.iter().map(|(l, m)| (*l, (*m).to_string())).collect();
        assert_eq!(got, want, "{}: warnings", case.name);
    }
}

#[test]
fn fix_converts_toward_the_majority_style() {
    for case in cases().iter().filter(|case| !case.fixed.is_empty()) {
        let fixed = fix(&case.input);
        assert_eq!(fixed, case.fixed, "{}: fixed output", case.name);
        assert_eq!(fix(&fixed), fixed, "{}: second pass must change nothing", case.name);
        assert_eq!(
            code_block_texts(&fixed),
            code_block_texts(&case.input),
            "{}: fixing must not change any code block's text",
            case.name
        );
    }
}

#[test]
fn an_unclosed_fence_reports_only_the_unclosed_block() {
    let case = cases().into_iter().find(|c| c.fixed.is_empty()).unwrap();
    let got = warnings(&case.input);
    assert_eq!(got.len(), 1, "{}: {got:?}", case.name);
    assert!(got[0].1.contains("never closed"), "{}: {got:?}", case.name);
}
