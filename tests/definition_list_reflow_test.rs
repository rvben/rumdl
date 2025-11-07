use std::fs;
use tempfile::tempdir;

#[test]
fn test_definition_list_via_cli() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"# Documentation

This is a paragraph explaining concepts. It has multiple sentences that should be reflowed.

## Terms

Keyword
: Definition of the keyword, potentially spanning a few more lines. A lot of words. Lorem ipsum.

Another Term
: Another definition.
: Second definition for same term.

## More Content

Regular paragraph after definition lists.
"#;

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 0
reflow = true
reflow_mode = "sentence-per-line"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Verify definition list structure is preserved
    assert!(
        fixed_content.contains("Keyword\n: Definition"),
        "Term and definition should remain on separate lines"
    );

    // Verify the definition line starts with ": "
    let lines: Vec<&str> = fixed_content.lines().collect();
    let def_line = lines
        .iter()
        .find(|l| l.trim_start().starts_with(": Definition"))
        .expect("Should find definition line");
    assert!(
        def_line.trim_start().starts_with(": "),
        "Definition should start with ': '"
    );

    // Verify regular paragraphs were reflowed
    assert!(
        fixed_content.contains("This is a paragraph explaining concepts.\nIt has multiple sentences"),
        "Regular paragraphs should still be reflowed"
    );
}

#[test]
fn test_definition_list_issue_136_reproduction() {
    // Exact reproduction of the issue reported in #136
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content =
        "Keyword\n: Definition of the keyword, potentially spanning a few more lines.\nA lot of words.\nLorem ipsum.";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
code-blocks = false
line-length = 0
tables = false
reflow_mode = "sentence-per-line"
reflow = true
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // The bug was: "Keyword : Definition..." (joined with space before colon)
    assert!(
        !fixed_content.contains("Keyword : "),
        "Should NOT join term with definition (bug from #136). Got: {fixed_content:?}"
    );

    // Should preserve the structure
    assert!(
        fixed_content.starts_with("Keyword\n: "),
        "Term and definition should remain on separate lines"
    );
}
