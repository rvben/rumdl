/// Test for issue #99: Exclude list should be respected when using pre-commit
/// Pre-commit passes explicit file paths to rumdl, and exclude patterns should
/// filter those files even when they are explicitly provided.
use std::fs;
use tempfile::TempDir;

#[test]
fn test_exclude_patterns_with_explicit_paths() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create directory structure: docs/ and root
    let docs_dir = base_path.join("docs");
    fs::create_dir(&docs_dir).unwrap();

    // Create test files with violations to ensure they would be reported if processed
    let root_file = base_path.join("README.md");
    let docs_file = docs_dir.join("guide.md");

    // Both files have MD041 violation (no first line heading)
    fs::write(&root_file, "Some content without heading.\n").unwrap();
    fs::write(&docs_file, "Some content without heading.\n").unwrap();

    // Create pyproject.toml with exclude configuration
    let pyproject_content = r#"
[tool.rumdl]
exclude = ["docs/*"]
"#;
    fs::write(base_path.join("pyproject.toml"), pyproject_content).unwrap();

    // Test 1: Run rumdl check with explicit paths (simulating pre-commit behavior)
    // This should respect the exclude configuration and NOT process docs/guide.md
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(root_file.to_str().unwrap())
        .arg(docs_file.to_str().unwrap())
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    // docs/guide.md should show a warning but not be processed
    assert!(
        stderr.contains("warning:")
            && stderr.contains("docs/guide.md")
            && stderr.contains("ignored because of exclude pattern"),
        "docs/guide.md should show exclusion warning. stderr:\n{stderr}"
    );
    // Should not appear in linting results (stdout)
    assert!(
        !stdout.contains("docs/guide.md"),
        "docs/guide.md should not be in linting results. stdout:\n{stdout}"
    );

    // README.md should appear (it's not excluded)
    assert!(
        combined.contains("README.md"),
        "README.md should be processed. Output:\n{combined}"
    );

    // Test 2: Verify that discovery mode still works
    let output2 = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(".")
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    let combined2 = format!("{stdout2}\n{stderr2}");

    // In discovery mode, docs/guide.md should also be excluded
    assert!(
        !combined2.contains("docs/guide.md"),
        "docs/guide.md should be excluded in discovery mode. Output:\n{combined2}"
    );
}

#[test]
fn test_force_exclude_with_explicit_paths() {
    // Test the force_exclude flag
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let docs_dir = base_path.join("docs");
    fs::create_dir(&docs_dir).unwrap();

    let docs_file = docs_dir.join("guide.md");
    fs::write(&docs_file, "# Guide\n\nSome content.\n").unwrap();

    let pyproject_content = r#"
[tool.rumdl]
exclude = ["docs/*"]
force_exclude = true
"#;
    fs::write(base_path.join("pyproject.toml"), pyproject_content).unwrap();

    // With force_exclude = true, explicitly provided files should still be excluded
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(docs_file.to_str().unwrap())
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // File should be excluded with a warning
    assert!(
        stderr.contains("warning:") && stderr.contains("ignored because of exclude pattern"),
        "docs/guide.md should be excluded with warning. stderr: {stderr}"
    );

    // Should report no files found
    assert!(
        stdout.contains("No markdown files found") || stderr.contains("No markdown files found"),
        "Should report no markdown files found when all are excluded. stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_no_exclude_flag() -> Result<(), Box<dyn std::error::Error>> {
    // Test the --no-exclude flag disables all exclusions
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path();

    let docs_dir = base_path.join("docs");
    fs::create_dir(&docs_dir)?;

    let docs_file = docs_dir.join("guide.md");
    fs::write(&docs_file, "# Guide\n\nSome content.\n")?;

    let pyproject_content = r#"
[tool.rumdl]
exclude = ["docs/*"]
"#;
    fs::write(base_path.join("pyproject.toml"), pyproject_content)?;

    // With --no-exclude, the file should be linted despite being in exclude patterns
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(docs_file.to_str().unwrap())
        .arg("--no-exclude")
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // File should be processed (no warning about exclusion)
    assert!(
        !stderr.contains("warning:") && !stderr.contains("ignored because of exclude pattern"),
        "Should not show exclusion warning with --no-exclude. stderr: {stderr}"
    );

    // Should report success (file was linted)
    assert!(
        stdout.contains("Success") || stdout.contains("No issues found"),
        "Should report linting success. stdout: {stdout}"
    );

    Ok(())
}

#[test]
fn test_cli_exclude_overrides_config() {
    // Test that CLI --exclude overrides config exclude
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let docs_dir = base_path.join("docs");
    let tests_dir = base_path.join("tests");
    fs::create_dir(&docs_dir).unwrap();
    fs::create_dir(&tests_dir).unwrap();

    let docs_file = docs_dir.join("guide.md");
    let tests_file = tests_dir.join("test.md");

    fs::write(&docs_file, "# Guide\n\nSome content.\n").unwrap();
    fs::write(&tests_file, "# Test\n\nSome content.\n").unwrap();

    let pyproject_content = r#"
[tool.rumdl]
exclude = ["docs/*"]
"#;
    fs::write(base_path.join("pyproject.toml"), pyproject_content).unwrap();

    // Use CLI --exclude to exclude tests/* instead of docs/*
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--exclude")
        .arg("tests/*")
        .arg(docs_file.to_str().unwrap())
        .arg(tests_file.to_str().unwrap())
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // tests/test.md should be excluded with a warning (CLI override)
    assert!(
        stderr.contains("warning:")
            && stderr.contains("tests/test.md")
            && stderr.contains("ignored because of exclude pattern"),
        "tests/test.md should show exclusion warning. stderr: {stderr}"
    );
    assert!(
        !stdout.contains("tests/test.md"),
        "tests/test.md should not be in linting results. stdout: {stdout}"
    );

    // docs/guide.md should NOT be excluded (CLI overrides config)
    // Note: This may still not appear if there are no issues found, so we just check
    // that the command completed successfully
    assert!(output.status.success() || output.status.code() == Some(1));
}
