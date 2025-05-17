use assert_cmd::Command;
use serde_json::Value;
use std::fs;

#[test]
fn test_cli_output_json_is_valid() {
    // Prepare a minimal markdown file
    let md_content = "# Title\n\nSome text.\n";
    let tmp_dir = tempfile::tempdir().unwrap();
    let md_path = tmp_dir.path().join("test.md");
    fs::write(&md_path, md_content).unwrap();

    // Run rumdl with --output json
    let output = Command::cargo_bin("rumdl")
        .unwrap()
        .args(&["check", md_path.to_str().unwrap(), "--output", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse as JSON
    let parsed: Value = serde_json::from_str(&stdout).expect("Output is not valid JSON");

    // Optionally: check that it's an array and has expected fields
    assert!(parsed.is_array());
    if let Some(first) = parsed.as_array().and_then(|arr| arr.get(0)) {
        assert!(first.get("rule_name").is_some());
        assert!(first.get("line").is_some());
        assert!(first.get("column").is_some());
        assert!(first.get("message").is_some());
    }
}