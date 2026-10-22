//! MD070 judges fences relative to their container.
//!
//! A fence inside a list item or blockquote is measured from the container's
//! content, not from the start of the raw line: a closer indented five raw
//! columns inside a two-column item is a valid three-column closer, and an
//! outer fence inside a quote closes early at an inner bare fence exactly as it
//! does at the top level. The oracle is pulldown-cmark: after `--fix` the outer
//! fence must hold the inner fences as its literal text.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

/// (name, input, expected output after fixing)
const BROKEN: &[(&str, &str, &str)] = &[
    (
        "blockquote",
        "> ```markdown\n> ```bash\n> x\n> ```\n> ```\n",
        "> ````markdown\n> ```bash\n> x\n> ```\n> ````\n",
    ),
    (
        "wide ordered item",
        "10. a\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
        "10. a\n\n    ````markdown\n    ```bash\n    x\n    ```\n    ````\n",
    ),
    (
        "nested list item",
        "- a\n  - b\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
        "- a\n  - b\n\n    ````markdown\n    ```bash\n    x\n    ```\n    ````\n",
    ),
    (
        "top level control",
        "# T\n\n```markdown\nOuter\n\n```bash\nx\n```\n\n```\n",
        "# T\n\n````markdown\nOuter\n\n```bash\nx\n```\n\n````\n",
    ),
    (
        "list item control",
        "# T\n\n- Item\n\n  ```markdown\n  ```bash\n  x\n  ```\n  ```\n",
        "# T\n\n- Item\n\n  ````markdown\n  ```bash\n  x\n  ```\n  ````\n",
    ),
    (
        "quote inside list item",
        "- a\n\n  > ```markdown\n  > ```bash\n  > x\n  > ```\n  > ```\n",
        "- a\n\n  > ````markdown\n  > ```bash\n  > x\n  > ```\n  > ````\n",
    ),
    (
        "varying quote indentation",
        "   > - a\n   >\n   >   ```md\n>   ```python\n>   x\n>   ```\n>   ```\n",
        "   > - a\n   >\n   >   ````md\n>   ```python\n>   x\n>   ```\n>   ````\n",
    ),
    (
        "task list item",
        "-  [ ] a\n\n   ```markdown\n   ```python\n   x\n   ```\n   ```\n",
        "-  [ ] a\n\n   ````markdown\n   ```python\n   x\n   ```\n   ````\n",
    ),
    (
        "closer indented past the raw limit inside an item with an empty first line",
        "-\n  ```md\n  ```python\n  x\n  ```\n     ```\n",
        "-\n  ````md\n  ```python\n  x\n  ```\n     ````\n",
    ),
    (
        "list item, quote and list item opening on one line",
        "- > - ```md\n  >   ```python\n  >   x\n  >   ```\n  >   ```\n",
        "- > - ````md\n  >   ```python\n  >   x\n  >   ```\n  >   ````\n",
    ),
    (
        "quote, list item, quote and list item opening on one line",
        "> - > - ```md\n>   >   ```python\n>   >   x\n>   >   ```\n>   >   ```\n",
        "> - > - ````md\n>   >   ```python\n>   >   x\n>   >   ```\n>   >   ````\n",
    ),
    (
        "ordered item, quote and list item opening on one line",
        "1. > - ```md\n   >   ```python\n   >   x\n   >   ```\n   >   ```\n",
        "1. > - ````md\n   >   ```python\n   >   x\n   >   ```\n   >   ````\n",
    ),
    (
        "search does not enter the next sibling item",
        "- a\n  - b\n\n    ```markdown\n    ```bash\n    x\n    ``` \n- c\n\n  ```\n",
        "- a\n  - b\n\n    ````markdown\n    ```bash\n    x\n    ```` \n- c\n\n  ```\n",
    ),
    (
        "search resumes after a list item that holds its own block",
        "```markdown\n```python\nx\n```\n\n- example\n\n  ```js\n  y\n  ```\n\n```\n",
        "````markdown\n```python\nx\n```\n\n- example\n\n  ```js\n  y\n  ```\n\n````\n",
    ),
];

/// (name, input, expected output after fixing) for blocks followed by a list
/// item or quote nested inside their own container. The nested container's
/// fences belong to its own blocks, so each block is fixed up to its own
/// closer and the rendered code blocks stay exactly as they were.
const NESTED: &[(&str, &str, &str)] = &[
    (
        "nested list items keep their own closers",
        "- item\n\n  ```markdown\n  ```python\n  x\n  ```\n\n  - item\n\n    ```markdown\n    ```python\n    x\n    ```\n",
        "- item\n\n  ````markdown\n  ```python\n  x\n  ````\n\n  - item\n\n    ````markdown\n    ```python\n    x\n    ````\n",
    ),
    (
        "top level block does not take a list item's closer",
        "```markdown\n```python\nx\n```\n\n- item\n\n  ```js\n  y\n  ```\n",
        "````markdown\n```python\nx\n````\n\n- item\n\n  ```js\n  y\n  ```\n",
    ),
    (
        "nested quotes keep their own closers",
        "> ```markdown\n> ```python\n> x\n> ```\n> > ```markdown\n> > ```python\n> > x\n> > ```\n",
        "> ````markdown\n> ```python\n> x\n> ````\n> > ````markdown\n> > ```python\n> > x\n> > ````\n",
    ),
    (
        "a lazy line in a nested quote ends the search",
        "> ```md\n> ```py\n> x\n> ```\n> > ```js\n> > y\n> > ```\n> > p\nlazy\n> ```\n> tail\n> ```\n",
        "> ````md\n> ```py\n> x\n> ````\n> > ```js\n> > y\n> > ```\n> > p\nlazy\n> ```\n> tail\n> ```\n",
    ),
];

/// Documents MD070 must leave alone.
const VALID: &[(&str, &str)] = &[
    (
        "closer indented past the raw limit but within three columns of the item",
        "- a\n\n  ```md\n  x\n     ```\n\n```text\ny\n```\n",
    ),
    (
        "inner fence indented four columns inside a quote is code",
        "> ```markdown\n>     ```bash\n> ```\n",
    ),
    (
        "tab indented fence is an indented code block",
        "\t```markdown\n```python\ncode()\n```\n```\n",
    ),
    (
        "definition list body is not rewritten against an unrelated fence",
        "Term\n: ```md\n  ```python\n  x\n  ```\n\nOutside\n\n```\n",
    ),
];

fn rumdl(args: &[&str], stdin: Option<&str>, path: Option<&std::path::Path>) -> (String, i32) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(args);
    if let Some(path) = path {
        command.arg(path);
    }
    command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn rumdl");
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

fn file_after(args: &[&str], content: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    rumdl(args, None, Some(&path));
    fs::read_to_string(&path).unwrap()
}

fn fenced_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => open = Some(String::new()),
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

#[test]
fn check_fix_rewrites_only_the_two_fence_runs() {
    for (name, input, expected) in BROKEN {
        let fixed = file_after(
            &["check", "--no-cache", "--fix", "--isolated", "--enable", "MD070"],
            input,
        );
        assert_eq!(&fixed, expected, "{name}: check --fix output");
        let again = file_after(
            &["check", "--no-cache", "--fix", "--isolated", "--enable", "MD070"],
            &fixed,
        );
        assert_eq!(again, fixed, "{name}: second pass must change nothing");
    }
}

#[test]
fn fixed_document_holds_the_inner_fences_as_literal_text() {
    for (name, input, expected) in BROKEN {
        if name.starts_with("search does not enter") {
            continue;
        }
        let blocks = fenced_blocks(expected);
        assert_eq!(
            blocks.len(),
            1,
            "{name}: the fixed document is one fenced block, got {blocks:?}"
        );
        assert!(
            blocks[0].contains("```") && blocks[0].contains('x'),
            "{name}: the outer fence must hold the inner fence as text, got {blocks:?}"
        );
        // The input is broken: the outer fence closes early, leaving a second block.
        let broken = fenced_blocks(input);
        assert!(
            broken.len() > 1,
            "{name}: the input is expected to be misparsed, got {broken:?}"
        );
    }
}

#[test]
fn nested_container_fences_are_not_taken_as_the_closer() {
    for (name, input, expected) in NESTED {
        let fixed = file_after(
            &["check", "--no-cache", "--fix", "--isolated", "--enable", "MD070"],
            input,
        );
        assert_eq!(&fixed, expected, "{name}: check --fix output");
        assert_eq!(
            fenced_blocks(&fixed),
            fenced_blocks(input),
            "{name}: fixing must not change the rendered code blocks"
        );
        let again = file_after(
            &["check", "--no-cache", "--fix", "--isolated", "--enable", "MD070"],
            &fixed,
        );
        assert_eq!(again, fixed, "{name}: second pass must change nothing");
        let (stdin, _) = rumdl(&["fmt", "--isolated", "--enable", "MD070", "-"], Some(input), None);
        let file = file_after(&["fmt", "--no-cache", "--isolated", "--enable", "MD070"], input);
        assert_eq!(stdin, file, "{name}: `fmt -` and `fmt file` must agree");
    }
}

#[test]
fn stdin_and_file_agree() {
    for (name, input, _) in BROKEN {
        let file = file_after(&["fmt", "--no-cache", "--isolated", "--enable", "MD070"], input);
        let (stdin, _) = rumdl(&["fmt", "--isolated", "--enable", "MD070", "-"], Some(input), None);
        assert_eq!(stdin, file, "{name}: `fmt -` and `fmt file` must agree");
    }
    for (name, input) in VALID {
        let file = file_after(&["fmt", "--no-cache", "--isolated", "--enable", "MD070"], input);
        let (stdin, _) = rumdl(&["fmt", "--isolated", "--enable", "MD070", "-"], Some(input), None);
        assert_eq!(stdin, file, "{name}: `fmt -` and `fmt file` must agree");
    }
}

#[test]
fn valid_documents_are_not_flagged_or_rewritten() {
    for (name, input) in VALID {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let (out, _) = rumdl(
            &[
                "check",
                "--no-cache",
                "--isolated",
                "--enable",
                "MD070",
                "--output-format",
                "concise",
            ],
            None,
            Some(&path),
        );
        assert!(!out.contains("MD070"), "{name}: unexpected MD070 warning:\n{out}");
        let fixed = file_after(&["fmt", "--no-cache", "--isolated", "--enable", "MD070"], input);
        assert_eq!(&fixed, input, "{name}: fmt must not change the document");
    }
}

#[test]
fn broken_documents_are_flagged() {
    for (name, input, _) in BROKEN {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let (out, code) = rumdl(
            &[
                "check",
                "--no-cache",
                "--isolated",
                "--enable",
                "MD070",
                "--output-format",
                "concise",
            ],
            None,
            Some(&path),
        );
        assert!(out.contains("MD070"), "{name}: expected an MD070 warning:\n{out}");
        assert_ne!(code, 0, "{name}: exit code must report the warning");
    }
}
