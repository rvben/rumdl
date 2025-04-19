use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn run_rumdl_command(args: &[&str], current_dir: &Path) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(args)
        .current_dir(current_dir)
        .output()
        .expect("Failed to run rumdl command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (success, stdout, stderr)
}

#[test]
fn test_pyproject_toml_init_command() {
    let temp_dir = tempdir().unwrap();

    // Run the init command with --pyproject flag
    let (success, stdout, _stderr) = run_rumdl_command(&["init", "--pyproject"], temp_dir.path());

    assert!(success, "Command failed");
    assert!(
        stdout.contains("Created pyproject.toml with rumdl configuration"),
        "Expected success message not found in stdout: {}",
        stdout
    );

    // Verify the file was created
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    assert!(pyproject_path.exists(), "pyproject.toml was not created");

    // Check file contents
    let content = fs::read_to_string(pyproject_path).unwrap();
    assert!(
        content.contains("[tool.rumdl]"),
        "Missing [tool.rumdl] section"
    );
    assert!(
        content.contains("line-length = 100"),
        "Missing line-length setting"
    );
    assert!(
        content.contains("[build-system]"),
        "Missing build-system section"
    );
}

#[test]
fn test_append_to_existing_pyproject_toml() {
    let temp_dir = tempdir().unwrap();
    let pyproject_path = temp_dir.path().join("pyproject.toml");

    // Create existing pyproject.toml with minimal content
    fs::write(
        &pyproject_path,
        r#"[build-system]
requires = ["setuptools>=42", "wheel"]
build-backend = "setuptools.build_meta"
"#,
    )
    .unwrap();

    // Create a test file to simulate input (answering "y" to the prompt)
    let input_file = temp_dir.path().join("input.txt");
    fs::write(&input_file, "y\n").unwrap();

    // Run the init command with --pyproject flag, redirecting input from our file
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["init", "--pyproject"])
        .current_dir(temp_dir.path())
        .stdin(fs::File::open(&input_file).unwrap())
        .output()
        .expect("Failed to run rumdl command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(output.status.success(), "Command failed");
    assert!(
        stdout.contains("Would you like to append rumdl configuration?"),
        "Expected prompt not found in stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("Added rumdl configuration to pyproject.toml"),
        "Expected success message not found in stdout: {}",
        stdout
    );

    // Check file contents
    let content = fs::read_to_string(pyproject_path).unwrap();
    assert!(
        content.contains("[tool.rumdl]"),
        "Missing [tool.rumdl] section"
    );
    assert!(
        content.contains("line-length = 100"),
        "Missing line-length setting"
    );
    assert!(
        content.contains("[build-system]"),
        "Missing build-system section"
    );
}

#[test]
fn test_pyproject_toml_config_loading() {
    let temp_dir = tempdir().unwrap();
    let pyproject_path = temp_dir.path().join("pyproject.toml");

    // Create test.md with a line that exceeds the default length but is under custom length
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        "# Test File\n\nThis line is 85 characters long which exceeds the default limit of 80 characters.\n",
    ).unwrap();

    // First run without config (should detect line length issue with default 80 chars)
    let (success, stdout, _stderr) =
        run_rumdl_command(&[test_file.to_str().unwrap()], temp_dir.path());

    assert!(!success, "Command should fail with line length issues");
    assert!(
        stdout.contains("MD013"),
        "MD013 rule warning not found in stdout"
    );

    // Create pyproject.toml with custom line length of 100
    fs::write(
        &pyproject_path,
        r#"[build-system]
requires = ["setuptools>=42", "wheel"]
build-backend = "setuptools.build_meta"

[tool.rumdl]
line-length = 100
"#,
    )
    .unwrap();

    // Run again with the config (should not detect line length issue now)
    let (success, stdout, _stderr) =
        run_rumdl_command(&[test_file.to_str().unwrap()], temp_dir.path());

    assert!(success, "Command should succeed with custom line length");
    assert!(
        !stdout.contains("MD013"),
        "MD013 rule warning should not be present"
    );
    assert!(
        stdout.contains("No issues found"),
        "Expected 'No issues found' message"
    );
}

#[test]
fn test_kebab_vs_snake_case_config() {
    // No need for this unused variable
    // let temp_dir = tempdir().unwrap();

    // Test with snake_case
    let snake_case_dir = tempdir().unwrap();
    let snake_pyproject_path = snake_case_dir.path().join("pyproject.toml");

    fs::write(
        &snake_pyproject_path,
        r#"[tool.rumdl]
line_length = 100
ignore_gitignore = true
"#,
    )
    .unwrap();

    // Test with kebab-case
    let kebab_case_dir = tempdir().unwrap();
    let kebab_pyproject_path = kebab_case_dir.path().join("pyproject.toml");

    fs::write(
        &kebab_pyproject_path,
        r#"[tool.rumdl]
line-length = 100
ignore-gitignore = true
"#,
    )
    .unwrap();

    // Create test file with long line
    let test_file_content = "# Test File\n\nThis line is 85 characters long which exceeds the default limit of 80 characters.\n";

    let snake_test_file = snake_case_dir.path().join("test.md");
    fs::write(&snake_test_file, test_file_content).unwrap();

    let kebab_test_file = kebab_case_dir.path().join("test.md");
    fs::write(&kebab_test_file, test_file_content).unwrap();

    // Test snake_case config
    let (snake_success, snake_stdout, _snake_stderr) =
        run_rumdl_command(&[snake_test_file.to_str().unwrap()], snake_case_dir.path());

    // Test kebab-case config
    let (kebab_success, kebab_stdout, _kebab_stderr) =
        run_rumdl_command(&[kebab_test_file.to_str().unwrap()], kebab_case_dir.path());

    // Both should succeed with custom line length
    assert!(
        snake_success,
        "Command should succeed with snake_case config"
    );
    assert!(
        kebab_success,
        "Command should succeed with kebab-case config"
    );

    assert!(
        !snake_stdout.contains("MD013"),
        "MD013 rule warning should not be present with snake_case"
    );
    assert!(
        !kebab_stdout.contains("MD013"),
        "MD013 rule warning should not be present with kebab-case"
    );
}

#[test]
fn test_explicit_config_precedence() {
    let temp_dir = tempdir().unwrap();

    // Create pyproject.toml with line-length=100
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    fs::write(
        &pyproject_path,
        r#"[tool.rumdl]
line-length = 100
"#,
    )
    .unwrap();

    // Create .rumdl.toml with line_length=60
    let rumdl_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &rumdl_path,
        r#"[global]

[MD013]
line_length = 60
"#,
    )
    .unwrap();

    // Create a custom config with line_length=120
    let custom_config_path = temp_dir.path().join("custom-config.toml");
    fs::write(
        &custom_config_path,
        r#"[global]

[MD013]
line_length = 120
"#,
    )
    .unwrap();

    // Create test file with lines of different lengths
    let test_file = temp_dir.path().join("test.md");
    fs::write(
        &test_file,
        r#"# Test File

This line is 65 characters long, which exceeds the limit of 60 chars.
This line is a bit longer at 85 characters, which exceeds 80 but is less than 100 chars.
This line is much longer at 130 characters, which exceeds all three limits: 60 chars from .rumdl.toml, 80 chars default, 100 from pyproject.toml, and 120 from custom.
"#,
    ).unwrap();

    // Run with explicit config path (should use line_length=120)
    let (explicit_success, explicit_stdout, _) = run_rumdl_command(
        &[
            test_file.to_str().unwrap(),
            "--config",
            custom_config_path.to_str().unwrap(),
        ],
        temp_dir.path(),
    );

    // Run without explicit config (.rumdl.toml should take precedence over pyproject.toml)
    let (default_success, default_stdout, _) =
        run_rumdl_command(&[test_file.to_str().unwrap()], temp_dir.path());

    // Both should fail but with different patterns
    assert!(
        !explicit_success,
        "Command should fail with explicit config"
    );
    assert!(
        !default_success,
        "Command should fail with default config search"
    );

    // Only line > 120 chars should fail with explicit config
    assert!(
        explicit_stdout.contains("MD013"),
        "MD013 warning should be present"
    );
    assert_eq!(
        explicit_stdout.matches("MD013").count(),
        1,
        "Should only have one MD013 warning with explicit config"
    );

    // Both lines > 60 chars should fail with .rumdl.toml (which takes precedence over pyproject.toml)
    assert!(
        default_stdout.contains("MD013"),
        "MD013 warning should be present"
    );
    assert!(
        default_stdout.matches("MD013").count() >= 3,
        "Should have multiple MD013 warnings with .rumdl.toml config"
    );
}
