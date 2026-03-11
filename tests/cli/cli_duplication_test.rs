use std::fs;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test file for emphasis conversion
    fs::write(base_path.join("emphasis_heading.md"), "**This should be a heading**")
        .expect("Failed to write test file");

    temp_dir
}

#[test]
fn test_heading_emphasis_detection() {
    // This test verifies that emphasis-only lines are detected but NOT automatically fixed
    // MD036 no longer provides automatic fixes to prevent document corruption

    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter with --fix but only apply the MD036 rule
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "--enable=MD036", "emphasis_heading.md"])
        .output()
        .unwrap();

    // Check that MD036 detected the issue (might be in stderr or stdout)
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{stderr}{stdout}");
    assert!(
        combined_output.contains("MD036"),
        "MD036 should detect emphasis used as heading. Output: {combined_output}"
    );

    // Run with --fix
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "--enable=MD036", "emphasis_heading.md"])
        .output()
        .unwrap();

    // Verify the emphasis was NOT converted (no automatic fix)
    let fixed_content = fs::read_to_string(base_path.join("emphasis_heading.md")).expect("Could not read fixed file");

    // The emphasis should remain unchanged since MD036 no longer provides automatic fixes
    assert!(
        fixed_content.contains("**This should be a heading**"),
        "Emphasis should remain unchanged (no automatic fix)"
    );
    assert!(
        !fixed_content.contains("## This should be a heading"),
        "Emphasis should NOT be converted to a heading automatically"
    );
}
