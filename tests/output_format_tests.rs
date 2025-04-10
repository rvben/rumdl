use std::fs;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test files with known issues
    fs::write(
        base_path.join("single_file.md"),
        "# Test Heading!\nThis line has trailing spaces.  \nNo newline at end",
    )
    .unwrap();

    fs::write(
        base_path.join("second_file.md"),
        "# Another Heading\n\n## Missing space after ##heading\n\nSome content.",
    )
    .unwrap();

    // Create a file with multiple different issue types for more thorough testing
    fs::write(
        base_path.join("multi_issue.md"),
        "# Heading 1\n# Heading 1 duplicate\nThis line has trailing spaces.  \n\n\nMultiple blank lines above.\n```\nCode block with no language\n```\nNo newline at end",
    )
    .unwrap();

    temp_dir
}

#[test]
fn test_output_format_singular() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter on a single file without fixes
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["single_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Single file output:\n{}", stdout);

    // Check for singular "file" in the output
    assert!(
        stdout.contains("issues in 1 file"),
        "Expected output to contain 'issues in 1 file', but got:\n{}",
        stdout
    );

    // Make sure it doesn't use plural for single file
    assert!(
        !stdout.contains("issues in 1 files"),
        "Output should not contain 'issues in 1 files'"
    );
}

#[test]
fn test_output_format_plural() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter on multiple files without fixes
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["single_file.md", "second_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Multiple files output:\n{}", stdout);

    // Check for plural "files" in the output
    assert!(
        stdout.contains("issues in 2 files"),
        "Expected output to contain 'issues in 2 files', but got:\n{}",
        stdout
    );
}

#[test]
fn test_output_format_fix_mode_singular() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter on a single file with fix mode
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "single_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Fix mode single file output:\n{}", stdout);

    // Check for singular "file" in the fixed output
    assert!(
        stdout.contains("issues in 1 file"),
        "Expected output to contain 'issues in 1 file', but got:\n{}",
        stdout
    );

    // Make sure it doesn't use plural for single file
    assert!(
        !stdout.contains("issues in 1 files"),
        "Output should not contain 'issues in 1 files'"
    );

    // Verify only the Fixed line is shown, not the Issues line
    assert!(
        stdout.contains("Fixed:"),
        "Output should contain 'Fixed:' line"
    );
    assert!(
        !stdout.contains("Issues:"),
        "Output should not contain 'Issues:' line when in fix mode"
    );
}

#[test]
fn test_output_format_fix_mode_plural() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter on multiple files with fix mode
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--fix", "single_file.md", "second_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Fix mode multiple files output:\n{}", stdout);

    // Check for plural "files" in the fixed output
    assert!(
        stdout.contains("issues in 2 files"),
        "Expected output to contain 'issues in 2 files', but got:\n{}",
        stdout
    );

    // Verify only the Fixed line is shown, not the Issues line
    assert!(
        stdout.contains("Fixed:"),
        "Output should contain 'Fixed:' line"
    );
    assert!(
        !stdout.contains("Issues:"),
        "Output should not contain 'Issues:' line when in fix mode"
    );
}

#[test]
fn test_output_format_fix_mode_label() {
    // Create temporary markdown files with known issues
    let temp_dir = setup_test_files();

    // Get path to the first test file (use existing single_file.md)
    let test_file = temp_dir.path().join("single_file.md");
    let test_file_path = test_file.to_str().unwrap();

    // Run in normal mode first
    let output_normal = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(&[test_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout_normal = String::from_utf8_lossy(&output_normal.stdout);
    
    // Check that fixable issues have [*] label
    assert!(stdout_normal.contains("[*]"), "Normal mode should show [*] for fixable issues");
    assert!(!stdout_normal.contains("[fixed]"), "Normal mode should not show [fixed] labels");

    // Now run in fix mode
    let output_fix = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(&[test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");

    let stdout_fix = String::from_utf8_lossy(&output_fix.stdout);
    
    // Check that fixed issues have [fixed] label
    assert!(stdout_fix.contains("[fixed]"), "Fix mode should show [fixed] for fixed issues");
    assert!(!stdout_fix.contains("[*]"), "Fix mode should not show [*] labels");
}

#[test]
fn test_multi_issue_output_format() {
    // Create temporary markdown files with different issue types
    let temp_dir = setup_test_files();
    
    // Get path to the multi-issue test file
    let test_file = temp_dir.path().join("multi_issue.md");
    let test_file_path = test_file.to_str().unwrap();
    
    // First run in normal mode to check issue labeling
    let output_normal = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(&[test_file_path])
        .output()
        .expect("Failed to execute command");
        
    let stdout_normal = String::from_utf8_lossy(&output_normal.stdout);
    println!("Multi-issue normal mode output:\n{}", stdout_normal);
    
    // Verify each type of expected issue is detected and marked as fixable
    assert!(stdout_normal.contains("[MD022]") && stdout_normal.contains("Heading should have"), 
            "Should detect heading blank line issue");
    assert!(stdout_normal.contains("[MD025]") && stdout_normal.contains("Multiple top-level headings"), 
            "Should detect duplicate level 1 heading issue");
    assert!(stdout_normal.contains("[MD012]") && stdout_normal.contains("Multiple consecutive blank lines"), 
            "Should detect multiple blank lines issue");
    assert!(stdout_normal.contains("[MD040]") && stdout_normal.contains("Fenced code blocks should have a language"), 
            "Should detect code block language issue");
    assert!(stdout_normal.contains("[MD047]") && stdout_normal.contains("File should end with a single newline"), 
            "Should detect file ending newline issue");
    
    // Verify each fixable issue is properly labeled in normal mode
    let normal_mode_fixable_issues = stdout_normal.matches("[*]").count();
    assert!(normal_mode_fixable_issues >= 5, 
            "Expected at least 5 fixable issues marked with [*], found {}", normal_mode_fixable_issues);
    
    // Now run in fix mode to check fixed issue labeling
    let output_fix = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(&[test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");
        
    let stdout_fix = String::from_utf8_lossy(&output_fix.stdout);
    println!("Multi-issue fix mode output:\n{}", stdout_fix);
    
    // Verify each fixable issue is labeled as fixed
    let fix_mode_fixed_issues = stdout_fix.matches("[fixed]").count();
    assert!(fix_mode_fixed_issues >= 5, 
            "Expected at least 5 issues marked as [fixed], found {}", fix_mode_fixed_issues);
    
    // Verify all the fixable issues are marked as fixed
    assert!(!stdout_fix.contains("[*]"), "Fix mode should not have any [*] labels");
    
    // Verify the summary counts match
    if stdout_fix.contains("Fixed:") {
        // Extract the fix count from the summary line
        let fixed_count_start = stdout_fix.find("Fixed:").unwrap();
        let fixed_line = &stdout_fix[fixed_count_start..];
        let fixed_line = if let Some(end) = fixed_line.find('\n') {
            &fixed_line[..end]
        } else {
            fixed_line
        };
        
        // Ensure the count of [fixed] labels matches the summary count
        let summary_fixed_count = fixed_line
            .split_whitespace()
            .nth(2)
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
            
        assert_eq!(fix_mode_fixed_issues, summary_fixed_count, 
            "The count of [fixed] labels ({}) should match the summary count ({})",
            fix_mode_fixed_issues, summary_fixed_count);
    }
}
