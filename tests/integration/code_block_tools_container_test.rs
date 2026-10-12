//! Code blocks inside blockquotes, list items and footnote definitions,
//! through the real binary.
//!
//! A tool gets the code CommonMark sees, with the containers' prefixes removed,
//! and `fmt` writes the formatted code back inside the same containers. The
//! tools are shell one-liners so nothing here depends on an installed formatter.
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn setup(config: &str, body: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("t.md"), body).unwrap();
    dir
}

fn run(dir: &Path, command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args([command, "--no-cache", "--disable", "ALL", "t.md"])
        .output()
        .unwrap()
}

/// A linter that appends what it read on stdin, then a separator, to `received.txt`.
const RECORDER_CONFIG: &str = r#"[code-block-tools]
enabled = true
normalize-language = "exact"

[code-block-tools.tools.recorder]
command = ["sh", "-c", "cat >> received.txt; echo ==== >> received.txt"]
stdin = true
stdout = false

[code-block-tools.languages.mermaid]
lint = ["recorder"]
"#;

#[test]
fn lint_tools_receive_code_without_container_prefixes() {
    let body = "# Diagrams\n\n\
                > ```mermaid\n> graph LR\n>   X --> Y\n> ```\n\n\
                > > ```mermaid\n> > graph TD\n> >\n> > A\n> > ```\n\n\
                - ```mermaid\n  graph LR\n    A --> B\n  ```\n\n\
                > - ```mermaid\n>   graph LR\n>   ```\n\n\
                See the note.[^diagram]\n\n\
                [^diagram]: ```mermaid\n    graph LR\n      Z --> W\n    ```\n";
    let dir = setup(RECORDER_CONFIG, body);

    let output = run(dir.path(), "check");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let received = fs::read_to_string(dir.path().join("received.txt")).unwrap();
    assert_eq!(
        received,
        "graph LR\n  X --> Y\n====\n\
         graph TD\n\nA\n====\n\
         graph LR\n  A --> B\n====\n\
         graph LR\n====\n\
         graph LR\n  Z --> W\n====\n"
    );
}

/// A formatter that uppercases, adds a blank line after the first line and
/// indents the rest, so the write-back has indentation and blank lines to place.
const RESHAPE_CONFIG: &str = r#"[code-block-tools]
enabled = true
normalize-language = "exact"

[code-block-tools.tools.reshape]
command = ["awk", "NR == 1 { print toupper($0); print \"\"; next } { print \"  \" toupper($0) }"]
stdin = true
stdout = true

[code-block-tools.languages.json]
format = ["reshape"]
"#;

#[test]
fn fmt_writes_formatted_code_back_inside_list_items_and_blockquotes() {
    let body = "# Data\n\n\
                - ```json\n  {\"a\": 1,\n  \"b\": 2}\n  ```\n- next item\n\n\
                > ```json\n> {\"a\": 1,\n> \"b\": 2}\n> ```\n\n\
                > - ```json\n>   {\"a\": 1,\n>   \"b\": 2}\n>   ```\n\n\
                See the note.[^data]\n\n\
                [^data]: ```json\n    {\"a\": 1,\n    \"b\": 2}\n    ```\n\n\
                After.\n";
    let dir = setup(RESHAPE_CONFIG, body);

    let output = run(dir.path(), "fmt");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let formatted = fs::read_to_string(dir.path().join("t.md")).unwrap();
    assert_eq!(
        formatted,
        "# Data\n\n\
         - ```json\n  {\"A\": 1,\n\n    \"B\": 2}\n  ```\n- next item\n\n\
         > ```json\n> {\"A\": 1,\n>\n>   \"B\": 2}\n> ```\n\n\
         > - ```json\n>   {\"A\": 1,\n>\n>     \"B\": 2}\n>   ```\n\n\
         See the note.[^data]\n\n\
         [^data]: ```json\n    {\"A\": 1,\n\n      \"B\": 2}\n    ```\n\n\
         After.\n"
    );
}

/// A formatter whose output contains a line that closes the fence.
const FENCE_CONFIG: &str = r#"[code-block-tools]
enabled = true
normalize-language = "exact"

[code-block-tools.tools.fence]
command = ["awk", "{ print \"```\"; print toupper($0) }"]
stdin = true
stdout = true

[code-block-tools.languages.json]
format = ["fence"]
"#;

#[test]
fn fmt_refuses_formatted_code_that_would_end_its_block() {
    let body = "# Data\n\n```json\n{\"a\": 1}\n```\n\nAfter.\n";
    let dir = setup(FENCE_CONFIG, body);

    let output = run(dir.path(), "fmt");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The block was not formatted, so the run is incomplete: a tool error.
    assert_eq!(output.status.code(), Some(2), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        format!("{stdout}{stderr}").contains("cannot be written back without changing the document's structure"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(fs::read_to_string(dir.path().join("t.md")).unwrap(), body);
}
