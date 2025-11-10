use std::env;
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

    // Create a file with an unfixable issue - MD013 (line length)
    fs::write(
        base_path.join("unfixable_issue.md"),
        "# Unfixable Issue\n\nThis is a very long line that exceeds the default line length limit of 80 characters which cannot be automatically fixed by the linter.\n",
    )
    .unwrap();

    // Create a file with both fixable and unfixable issues
    fs::write(
        base_path.join("mixed_issues.md"),
        "# Mixed Issues\nThis line has trailing spaces.  \n\nThis paragraph contains * spaced emphasis * that should be fixable.\nThis line should have a newline at the end but doesn't",
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
        .args(["check", "single_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Single file output:\n{stdout}");

    // Check for singular "file" in the output
    assert!(
        stdout.contains("issues in 1 file"),
        "Expected output to contain 'issues in 1 file', but got:\n{stdout}"
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
        .args(["check", "single_file.md", "second_file.md"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Multiple files output:\n{stdout}");

    // Check for plural "files" in the output
    assert!(
        stdout.contains("issues in 2 files"),
        "Expected output to contain 'issues in 2 files', but got:\n{stdout}"
    );
}

#[test]
fn test_output_format_fix_mode_singular() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Run the linter on a single file with fix mode
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "single_file.md", "--fix"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Fix mode single file output:\n{stdout}");

    // Check for singular "file" in the fixed output
    assert!(
        stdout.contains("issues in 1 file"),
        "Expected output to contain 'issues in 1 file', but got:\n{stdout}"
    );

    // Make sure it doesn't use plural for single file
    assert!(
        !stdout.contains("issues in 1 files"),
        "Output should not contain 'issues in 1 files'"
    );

    // Verify only the Fixed line is shown, not the Issues line
    assert!(stdout.contains("Fixed:"), "Output should contain 'Fixed:' line");
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
        .args(["check", "single_file.md", "second_file.md", "--fix"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Fix mode multiple files output:\n{stdout}");

    // Check for plural "files" in the fixed output
    assert!(
        stdout.contains("issues in 2 files"),
        "Expected output to contain 'issues in 2 files', but got:\n{stdout}"
    );

    // Verify only the Fixed line is shown, not the Issues line
    assert!(stdout.contains("Fixed:"), "Output should contain 'Fixed:' line");
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
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout_normal = String::from_utf8_lossy(&output_normal.stdout);

    // Check that fixable issues have [*] label
    assert!(
        stdout_normal.contains("[*]"),
        "Normal mode should show [*] for fixable issues"
    );
    assert!(
        !stdout_normal.contains("[fixed]"),
        "Normal mode should not show [fixed] labels"
    );

    // Now run in fix mode
    let output_fix = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");

    let stdout_fix = String::from_utf8_lossy(&output_fix.stdout);

    // Check that fixed issues have [fixed] label
    assert!(
        stdout_fix.contains("[fixed]"),
        "Fix mode should show [fixed] for fixed issues"
    );
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
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout_normal = String::from_utf8_lossy(&output_normal.stdout);
    println!("Multi-issue normal mode output:\n{stdout_normal}");

    // Verify each type of expected issue is detected and marked as fixable
    assert!(
        stdout_normal.contains("[MD022]") && stdout_normal.contains("blank line"),
        "Should detect heading blank line issue"
    );
    assert!(
        stdout_normal.contains("[MD025]") && stdout_normal.contains("Multiple top-level headings"),
        "Should detect duplicate level 1 heading issue"
    );
    assert!(
        stdout_normal.contains("[MD012]") && stdout_normal.contains("Multiple consecutive blank lines"),
        "Should detect multiple blank lines issue"
    );
    assert!(
        stdout_normal.contains("[MD040]") && stdout_normal.contains("Code block (```) missing language"),
        "Should detect code block language issue"
    );
    assert!(
        stdout_normal.contains("[MD047]") && stdout_normal.contains("File should end with a single newline"),
        "Should detect file ending newline issue"
    );

    // Verify each fixable issue is properly labeled in normal mode
    let normal_mode_fixable_issues = stdout_normal.matches("[*]").count();
    assert!(
        normal_mode_fixable_issues >= 5,
        "Expected at least 5 fixable issues marked with [*], found {normal_mode_fixable_issues}"
    );

    // Now run in fix mode to check fixed issue labeling
    let output_fix = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");

    let stdout_fix = String::from_utf8_lossy(&output_fix.stdout);
    println!("Multi-issue fix mode output:\n{stdout_fix}");

    // Verify each fixable issue is labeled as fixed
    let fix_mode_fixed_issues = stdout_fix.matches("[fixed]").count();
    assert!(
        fix_mode_fixed_issues >= 5,
        "Expected at least 5 issues marked as [fixed], found {fix_mode_fixed_issues}"
    );

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

        assert_eq!(
            fix_mode_fixed_issues, summary_fixed_count,
            "The count of [fixed] labels ({fix_mode_fixed_issues}) should match the summary count ({summary_fixed_count})"
        );
    }
}

#[test]
fn test_fixable_issues_labeling() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a file with a fixable issue - MD037 (spaces inside emphasis markers)
    let test_file = temp_dir.path().join("fixable_issue.md");
    fs::write(
        &test_file,
        "# Fixable Issue\n\nThis paragraph contains * spaced emphasis *.\n",
    )
    .unwrap();

    let test_file_path = test_file.to_str().unwrap();

    // Run the linter
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Fixable issue output:\n{stdout}");

    // Verify MD037 emphasis spaces issue is reported
    assert!(stdout.contains("[MD037]"), "Should detect spaces around emphasis issue");

    // Verify issue has a [*] label, indicating it's fixable
    if stdout.contains("[MD037]") {
        let md037_line = stdout.lines().find(|line| line.contains("[MD037]")).unwrap_or("");
        assert!(
            md037_line.contains("[*]"),
            "MD037 should have [*] label indicating it's fixable"
        );
    }

    // Run with fix mode and ensure the issue is fixed
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    println!("Fixable issue with --fix output:\n{fix_stdout}");

    // Verify the issue is marked as fixed
    assert!(
        fix_stdout.contains("[MD037]"),
        "Spaces around emphasis issue should be reported with fix"
    );

    if fix_stdout.contains("[MD037]") {
        let md037_line = fix_stdout.lines().find(|line| line.contains("[MD037]")).unwrap_or("");
        assert!(
            md037_line.contains("[fixed]"),
            "Fixed issue (MD037) should have [fixed] label"
        );
    }

    // Verify the content was actually fixed
    let content = fs::read_to_string(test_file_path).expect("Failed to read file");
    assert!(
        content.contains("*spaced emphasis*"),
        "Spaces inside emphasis should be fixed in the file content"
    );
}

#[test]
fn test_truly_unfixable_issues_labeling() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a file with an unfixable issue - MD013 (line length)
    let test_file = temp_dir.path().join("unfixable_issue.md");
    // Create content with a very long line that exceeds default line length (80 chars)
    fs::write(
        &test_file,
        "# Truly Unfixable Issue\n\nThis paragraph contains a very long line that definitely exceeds the maximum line length limit and cannot be automatically fixed by the linter because line wrapping requires manual intervention.\n",
    ).unwrap();

    // Create a custom config file with a small line_length to ensure MD013 is triggered
    let config_file = temp_dir.path().join("custom_rumdl.toml");
    fs::write(&config_file, "[MD013]\nline_length = 20\n").unwrap();

    let test_file_path = test_file.to_str().unwrap();
    let config_file_path = config_file.to_str().unwrap();

    // Run the linter with custom config
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--config", config_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Truly unfixable issue output:\n{stdout}");

    // Verify MD013 line length issue is reported
    assert!(
        stdout.contains("[MD013]") && stdout.contains("Line length"),
        "Should detect line length issue"
    );

    // Verify issue does NOT have a [*] label, indicating it's NOT fixable
    if stdout.contains("[MD013]") {
        let md013_line = stdout.lines().find(|line| line.contains("[MD013]")).unwrap_or("");
        assert!(
            !md013_line.contains("[*]"),
            "MD013 should NOT have [*] label since it's not fixable"
        );
    }

    // Run with fix mode and ensure the issue is NOT fixed
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix", "--config", config_file_path])
        .output()
        .expect("Failed to execute command");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    println!("Truly unfixable issue with --fix output:\n{fix_stdout}");

    // Verify the issue is still reported and NOT marked as fixed
    assert!(
        fix_stdout.contains("[MD013]") && fix_stdout.contains("Line length"),
        "Line length issue should still be reported after fix attempt"
    );

    if fix_stdout.contains("[MD013]") {
        let md013_line = fix_stdout.lines().find(|line| line.contains("[MD013]")).unwrap_or("");
        assert!(
            !md013_line.contains("[fixed]"),
            "Unfixable issue (MD013) should NOT have [fixed] label"
        );
    }

    // Verify the content was NOT fixed
    let content = fs::read_to_string(test_file_path).expect("Failed to read file");
    // The long line should still be long
    assert!(
        content.contains("This paragraph contains a very long line"),
        "Line length should NOT be fixed in the file content"
    );
}

#[test]
fn test_mixed_fixable_unfixable_issues() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a file with both fixable and unfixable issues
    let test_file = temp_dir.path().join("mixed_issues.md");
    fs::write(
        &test_file,
        "# Mixed Issues\nThis line has trailing spaces.  \n\nThis paragraph contains * spaced emphasis * that should be fixable.\nThis line is extremely long and exceeds the maximum line length which cannot be automatically fixed because line wrapping requires manual intervention by the user.\nThis line should have a newline at the end but doesn't",
    ).unwrap();

    // Create a custom config file with a small line_length to ensure MD013 is triggered
    let config_file = temp_dir.path().join("custom_rumdl.toml");
    fs::write(&config_file, "[MD013]\nline_length = 20\n").unwrap();

    let test_file_path = test_file.to_str().unwrap();
    let config_file_path = config_file.to_str().unwrap();

    // Run the linter in normal mode with custom config
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--config", config_file_path])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Mixed issues output:\n{stdout}");

    // Check for fixable issues
    assert!(
        stdout.contains("[MD022]"),
        "Should detect heading blank line issue (fixable)"
    );
    assert!(
        stdout.contains("[MD047]"),
        "Should detect missing newline issue (fixable)"
    );
    assert!(
        stdout.contains("[MD037]"),
        "Should detect spaces around emphasis issue (fixable)"
    );

    // Check for unfixable issues
    assert!(
        stdout.contains("[MD013]"),
        "Should detect line length issue (unfixable)"
    );

    // Check that fixable issues have [*] label
    assert!(
        stdout.contains("[*]"),
        "Should detect at least one fixable issue with [*] label"
    );

    // Check that MD013 doesn't have [*] label
    if stdout.contains("[MD013]") {
        let md013_line = stdout.lines().find(|line| line.contains("[MD013]")).unwrap_or("");
        assert!(
            !md013_line.contains("[*]"),
            "MD013 should NOT have [*] label since it's not fixable"
        );
    }

    // Run with fix mode and custom config
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix", "--config", config_file_path])
        .output()
        .expect("Failed to execute command");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    println!("Mixed issues with --fix output:\n{fix_stdout}");

    // Check that fixable issues are fixed
    assert!(
        fix_stdout.contains("[fixed]"),
        "Fixable issues should show [fixed] label"
    );

    // Verify MD037 is marked as fixed
    if fix_stdout.contains("[MD037]") {
        let md037_line = fix_stdout.lines().find(|line| line.contains("[MD037]")).unwrap_or("");
        assert!(
            md037_line.contains("[fixed]"),
            "MD037 should have [fixed] label after applying fixes"
        );
    }

    // Verify MD013 is NOT marked as fixed
    if fix_stdout.contains("[MD013]") {
        let md013_line = fix_stdout.lines().find(|line| line.contains("[MD013]")).unwrap_or("");
        assert!(
            !md013_line.contains("[fixed]"),
            "MD013 should NOT have [fixed] label as it cannot be fixed automatically"
        );
    }

    // Verify the content was actually fixed for fixable issues only
    let content = fs::read_to_string(test_file_path).expect("Failed to read file");

    // Check that the heading has a blank line below it now
    assert!(
        content.contains("# Mixed Issues\n\n"),
        "Heading should have a blank line below it after fixing"
    );

    // Check that emphasis spaces were fixed
    assert!(
        content.contains("*spaced emphasis*"),
        "Spaces inside emphasis should be fixed in the file content"
    );

    // Check that file ends with newline
    assert!(
        content.ends_with('\n'),
        "Missing newline should be fixed in the file content"
    );

    // Check that long line is still long (unfixed)
    assert!(
        content.contains("This line is extremely long"),
        "Long line should remain unfixed in the content"
    );
}

#[test]
fn test_color_output_disabled() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();
    let test_file = temp_dir.path().join("single_file.md");
    let test_file_path = test_file.to_str().unwrap();

    // Run with NO_COLOR environment variable to disable colored output
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .env("NO_COLOR", "1")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("No color output:\n{stdout}");

    // ANSI color codes start with ESC character (27) followed by [
    // In Rust strings, this looks like \x1b[
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI color codes when NO_COLOR is set"
    );
}

#[test]
fn test_quiet_mode_output() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();
    let test_file = temp_dir.path().join("single_file.md");
    let test_file_path = test_file.to_str().unwrap();

    // Run with --silent flag
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--silent"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Silent mode output:\n{stdout}");

    // Verify output is suppressed
    assert!(
        stdout.is_empty(),
        "Silent mode should suppress standard output, got: {stdout}"
    );

    // Run with --silent and --fix to ensure it still fixes issues but doesn't output
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--silent", "--fix"])
        .output()
        .expect("Failed to execute command");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    assert!(
        fix_stdout.is_empty(),
        "Quiet mode with fix should suppress standard output, got: {fix_stdout}"
    );

    // Verify that fix was still applied by checking the content of the fixed file
    let content = fs::read_to_string(test_file_path).expect("Failed to read file");
    assert!(
        content.ends_with('\n'),
        "File should have been fixed (newline added) even in quiet mode"
    );
}

#[test]
fn test_verbose_mode_output() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();
    let test_file = temp_dir.path().join("single_file.md");
    let test_file_path = test_file.to_str().unwrap();

    // Run with --verbose flag
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Verbose mode output:\n{stdout}");

    // Verify verbose output includes file processing messages
    assert!(
        stdout.contains("Processing file:"),
        "Verbose mode should include 'Processing file:' messages"
    );

    // Verify verbose output includes list of rules
    assert!(
        stdout.contains("Enabled rules:"),
        "Verbose mode should include list of enabled rules"
    );

    // Verify the basic issues are still present in output
    // The test file should trigger at least some warnings
    assert!(
        stdout.contains("[MD"),
        "Verbose mode should still show lint warnings in the output"
    );
}

#[test]
fn test_exit_code_validation() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a clean file with no issues
    let clean_file = temp_dir.path().join("clean_file.md");
    fs::write(&clean_file, "# Clean File\n\nThis file has no issues.\n").unwrap();

    // Create a file with issues
    let issue_file = temp_dir.path().join("single_file.md");

    // Run linter on file with issues
    let output_with_issues = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", issue_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    // Run linter on clean file
    let output_no_issues = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", clean_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    // Verify exit code is non-zero when issues found
    assert_ne!(
        output_with_issues.status.code(),
        Some(0),
        "Exit code should be non-zero when issues are found"
    );

    // Verify exit code is zero when no issues found
    assert_eq!(
        output_no_issues.status.code(),
        Some(0),
        "Exit code should be zero when no issues are found"
    );

    // Verify fix mode results in exit code 0 if all issues fixed
    let output_fix = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", issue_file.to_str().unwrap(), "--fix"])
        .output()
        .expect("Failed to execute command");

    // If exit code is 0, all issues were fixed. Otherwise, some were unfixable.
    let fix_stdout = String::from_utf8_lossy(&output_fix.stdout);
    if !fix_stdout.contains("issues") || fix_stdout.contains("Fixed: 0/") {
        // No issues or no issues fixed
        assert_eq!(
            output_fix.status.code(),
            Some(0),
            "Exit code should be 0 when no issues remain after fix"
        );
    }
}

#[test]
fn test_rule_with_configuration() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a file with customizable issues (line length can be configured)
    let test_file = temp_dir.path().join("configurable_issue.md");
    fs::write(
        &test_file,
        "# Configurable Issue\n\nThis line is exactly 70 characters long which is within default limits.\nThis line is a bit longer and has exactly 75 characters which exceeds 70 chars.\nThis line is much longer and definitely exceeds the default limit of 80 characters by a substantial margin including many extra words that ensure it is well over the limit of 80 characters making it extremely long and definitely triggering the MD013 rule with its default configuration.\n",
    ).unwrap();

    let test_file_path = test_file.to_str().unwrap();

    // First run with default configuration (line length 80)
    let default_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let default_stdout = String::from_utf8_lossy(&default_output.stdout);
    println!("Default configuration output:\n{default_stdout}");

    // Check that line length issues are reported with default config (limit 80)
    assert!(
        default_stdout.contains("[MD013]"),
        "Should detect line length issues over 80 characters with default configuration"
    );

    // Now create a custom config file with lower limit
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &config_file,
        r#"
[MD013]
line_length = 70
"#,
    )
    .unwrap();

    // Run with custom configuration (line length 70)
    let custom_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .current_dir(temp_dir.path()) // Important: set working directory to where config file is
        .output()
        .expect("Failed to execute command");

    let custom_stdout = String::from_utf8_lossy(&custom_output.stdout);
    println!("Custom configuration output:\n{custom_stdout}");

    // Custom config should detect more issues (since limit is lower)
    assert!(
        custom_stdout.contains("[MD013]"),
        "Should detect line length issues over 70 characters with custom configuration"
    );

    // The custom configuration should detect the second line as an issue (over 70 chars)
    // which the default configuration wouldn't detect (since it's under 80)
    let default_issue_count = default_stdout.matches("[MD013]").count();
    let custom_issue_count = custom_stdout.matches("[MD013]").count();

    assert!(
        custom_issue_count > default_issue_count,
        "Custom configuration should detect more issues than default configuration"
    );
}

#[test]
fn test_fixed_content_validation() {
    // Create temporary markdown files
    let temp_dir = setup_test_files();

    // Create a simple test file with specific fixable issues
    let test_file = temp_dir.path().join("fixable_issues.md");
    fs::write(&test_file, "# Missing Newline\nThis line does not end with a newline").unwrap();

    let test_file_path = test_file.to_str().unwrap();

    // Save original content for comparison
    let original_content = fs::read_to_string(&test_file).unwrap();

    // Run the linter to verify the issue exists
    let check_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let check_stdout = String::from_utf8_lossy(&check_output.stdout);
    println!("Pre-fix output:\n{check_stdout}");

    // Verify the file has the MD047 issue (missing newline)
    assert!(
        check_stdout.contains("[MD047]") && check_stdout.contains("newline"),
        "Should detect missing newline issue"
    );

    // Fix the file
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path, "--fix"])
        .output()
        .expect("Failed to execute command");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    println!("Fix output:\n{fix_stdout}");

    // Check that content was actually modified
    let fixed_content = fs::read_to_string(&test_file).unwrap();
    assert_ne!(
        original_content, fixed_content,
        "Content should be modified after fixing"
    );

    // Missing newline should be added
    assert!(fixed_content.ends_with('\n'), "Fixed content should end with a newline");

    // Run linter again to verify no issues remain
    let recheck_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", test_file_path])
        .output()
        .expect("Failed to execute command");

    let recheck_stdout = String::from_utf8_lossy(&recheck_output.stdout);
    println!("Post-fix check output:\n{recheck_stdout}");

    // The specific fixed issue should not be reported again
    assert!(!recheck_stdout.contains("[MD047]"), "MD047 issue should be fixed");
}
