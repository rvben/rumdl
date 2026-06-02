use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::fs;

#[test]
fn test_cli_output_json_is_valid() {
    // Markdown with known violations: a closed-ATX heading (MD003) and trailing
    // whitespace (MD009). This guarantees the JSON output is non-empty so the
    // documented field contract is actually asserted on a real warning object.
    let md_content = "# Title\n\n## Closed Heading ##\n\ntrailing spaces here   \n";
    let tmp_dir = tempfile::tempdir().unwrap();
    let md_path = tmp_dir.path().join("test.md");
    fs::write(&md_path, md_content).unwrap();

    let output = cargo_bin_cmd!("rumdl")
        .args([
            "check",
            md_path.to_str().unwrap(),
            "--output-format",
            "json",
            "--no-cache",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("Output is not valid JSON");

    // The JSON reporter emits a flat array of warning objects.
    let warnings = parsed.as_array().expect("JSON output should be an array");
    assert!(!warnings.is_empty(), "expected at least one violation in output");

    // Each warning object must carry the documented fields.
    for warning in warnings {
        for field in ["file", "line", "column", "rule", "message", "severity", "fixable"] {
            assert!(
                warning.get(field).is_some(),
                "warning object missing documented field {field:?}: {warning}"
            );
        }
    }
}
