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
