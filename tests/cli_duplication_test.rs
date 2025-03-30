use std::fs;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test files with various duplication patterns
    let test_cases = [
        // Test case 1: Simple duplication with space
        ("space_duplication.md", "## Heading ## Heading"),
        
        // Test case 2: Multiple occurrences of duplications
        ("multiple.md", "# Title\n\n## Section 1## Section 1\n\nContent\n\n## Section 2**Section 2**\n\nMore content"),
    ];
    
    for (filename, content) in test_cases {
        fs::write(base_path.join(filename), content).unwrap();
    }

    temp_dir
}

#[test]
fn test_heading_duplication_fix() {
    // This test verifies that the linter now properly fixes duplicated headings
    
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter with --fix on a file with duplicated headings
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "--verbose", "space_duplication.md"])
        .output()
        .unwrap();

    // Let's read the fixed file to see what state it's in
    let fixed_content = fs::read_to_string(base_path.join("space_duplication.md"))
        .expect("Could not read fixed file");
    
    println!("=== Heading Duplication Fix ===");
    println!("The linter now properly handles duplicated headings.");
    println!("For example:");
    println!("Input: '## Heading ## Heading'");
    println!("New output: '{}'", fixed_content.trim());
    
    // Verify the duplicated heading is now properly fixed
    assert_eq!(fixed_content.trim(), "# Heading", 
        "Heading duplication should be fixed and the heading level should be 1 (due to MD002/MD041)");
    
    // Also check multiple duplications
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "--verbose", "multiple.md"])
        .output()
        .unwrap();
    
    let multiple_content = fs::read_to_string(base_path.join("multiple.md"))
        .expect("Could not read fixed file");
    
    // Verify all duplications are fixed
    assert!(!multiple_content.contains("## Section 1## Section 1"),
        "Section 1 duplication should be fixed");
    assert!(!multiple_content.contains("## Section 2**Section 2**"),
        "Section 2 emphasis duplication should be fixed");
}

#[test]
fn test_heading_emphasis_conversion() {
    // This test verifies that emphasis-only lines are properly converted to headings
    // which is something the linter does handle well
    
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();
    
    // Create a test file with emphasis that should be converted to a heading
    let content = "**This should be a heading**";
    fs::write(base_path.join("emphasis_heading.md"), content).expect("Failed to write test file");
    
    // Run the linter with --fix
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "emphasis_heading.md"])
        .output()
        .unwrap();
    
    // Verify the emphasis was converted to a heading
    let fixed_content = fs::read_to_string(base_path.join("emphasis_heading.md"))
        .expect("Could not read fixed file");
    
    // The emphasis should be converted to a heading (by MD036)
    // But first MD041 will add a title, so we check if our heading is in the file
    assert!(fixed_content.contains("# This should be a heading"), 
            "Emphasis should be converted to a heading");
} 