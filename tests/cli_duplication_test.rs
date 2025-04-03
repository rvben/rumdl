use std::fs;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test file for emphasis conversion
    fs::write(
        base_path.join("emphasis_heading.md"),
        "**This should be a heading**",
    )
    .expect("Failed to write test file");

    temp_dir
}

#[test]
fn test_heading_emphasis_conversion() {
    // This test verifies that emphasis-only lines are properly converted to headings
    // which is something the linter does handle well

    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter with --fix but only apply the MD036 rule
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "--enable=MD036", "emphasis_heading.md"])
        .output()
        .unwrap();

    // Verify the emphasis was converted to a heading
    let fixed_content = fs::read_to_string(base_path.join("emphasis_heading.md"))
        .expect("Could not read fixed file");

    // The emphasis should be converted to a heading (by MD036)
    // Level 2 for double asterisks, as that's what the rule does
    assert!(
        fixed_content.contains("## This should be a heading"),
        "Emphasis should be converted to a heading"
    );
}
