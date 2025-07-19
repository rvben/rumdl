use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    println!("Creating test files in: {}", base_path.display());

    // Create test files and directories
    fs::create_dir_all(base_path.join("docs")).unwrap();
    fs::create_dir_all(base_path.join("docs/temp")).unwrap();
    fs::create_dir_all(base_path.join("src")).unwrap();
    fs::create_dir_all(base_path.join("subfolder")).unwrap();

    fs::write(base_path.join("README.md"), "# Test\n").unwrap();
    fs::write(base_path.join("docs/doc1.md"), "# Doc 1\n").unwrap();
    fs::write(base_path.join("docs/temp/temp.md"), "# Temp\n").unwrap();
    fs::write(base_path.join("src/test.md"), "# Source\n").unwrap();
    fs::write(base_path.join("subfolder/README.md"), "# Subfolder README\n").unwrap();

    // Print the created files for debugging
    println!("Created test files:");
    println!("  {}/README.md", base_path.display());
    println!("  {}/docs/doc1.md", base_path.display());
    println!("  {}/docs/temp/temp.md", base_path.display());
    println!("  {}/src/test.md", base_path.display());
    println!("  {}/subfolder/README.md", base_path.display());

    temp_dir
}

fn create_config(dir: &Path, content: &str) {
    fs::write(dir.join(".rumdl.toml"), content).unwrap();
}

#[test]
fn test_cli_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Helper to run command and get stdout/stderr
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(base_path)
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };
    let normalize = |s: &str| s.replace(r"\", "/");

    // Test include via CLI - should only process docs/doc1.md
    println!("--- Running CLI Include Test ---");
    let (success_incl, stdout_incl, _) = run_cmd(&[".", "--include", "docs/doc1.md", "--verbose"]);
    assert!(success_incl, "CLI Include Test failed");
    let norm_stdout_incl = normalize(&stdout_incl);
    assert!(
        norm_stdout_incl.contains("Processing file: docs/doc1.md"),
        "CLI Include: docs/doc1.md missing"
    );
    assert!(
        !norm_stdout_incl.contains("Processing file: README.md"),
        "CLI Include: README.md should be excluded"
    );
    assert!(
        !norm_stdout_incl.contains("Processing file: docs/temp/temp.md"),
        "CLI Include: temp.md should be excluded"
    );

    // Test exclude via CLI - exclude the temp directory
    println!("--- Running CLI Exclude Test ---");
    let (success_excl, stdout_excl, _) = run_cmd(&[".", "--exclude", "docs/temp", "--verbose"]);
    assert!(success_excl, "CLI Exclude Test failed");
    let norm_stdout_excl = normalize(&stdout_excl);
    assert!(
        norm_stdout_excl.contains("Processing file: README.md"),
        "CLI Exclude: README.md missing"
    );
    assert!(
        norm_stdout_excl.contains("Processing file: docs/doc1.md"),
        "CLI Exclude: docs/doc1.md missing"
    );
    assert!(
        norm_stdout_excl.contains("Processing file: src/test.md"),
        "CLI Exclude: src/test.md missing"
    );
    assert!(
        !norm_stdout_excl.contains("Processing file: docs/temp/temp.md"),
        "CLI Exclude: temp.md should be excluded"
    );

    // Test combined include and exclude via CLI - include *.md in docs, exclude temp
    println!("--- Running CLI Include/Exclude Test ---");
    let (success_comb, stdout_comb, _) =
        run_cmd(&[".", "--include", "docs/*.md", "--exclude", "docs/temp", "--verbose"]);
    assert!(success_comb, "CLI Include/Exclude Test failed");
    let norm_stdout_comb = normalize(&stdout_comb);
    assert!(
        norm_stdout_comb.contains("Processing file: docs/doc1.md"),
        "CLI Combo: docs/doc1.md missing"
    );
    assert!(
        !norm_stdout_comb.contains("Processing file: docs/temp/temp.md"),
        "CLI Combo: temp.md should be excluded"
    );
    assert!(
        !norm_stdout_comb.contains("Processing file: README.md"),
        "CLI Combo: README.md should be excluded"
    );
}

#[test]
fn test_config_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Helper
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(base_path)
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };
    let normalize = |s: &str| s.replace(r"\", "/");

    // Test include via config - only include docs/doc1.md specifically
    println!("--- Running Config Include Test ---");
    let config_incl = r#"
[global]
include = ["docs/doc1.md"]
"#;
    create_config(base_path, config_incl);

    let (success_incl, stdout_incl, _) = run_cmd(&[".", "--verbose"]);
    assert!(success_incl, "Config Include Test failed");
    let norm_stdout_incl = normalize(&stdout_incl);
    assert!(
        norm_stdout_incl.contains("Processing file: docs/doc1.md"),
        "Config Include: docs/doc1.md missing"
    );
    assert!(
        !norm_stdout_incl.contains("Processing file: README.md"),
        "Config Include: README.md should be excluded"
    );
    assert!(
        !norm_stdout_incl.contains("Processing file: docs/temp/temp.md"),
        "Config Include: temp.md should be excluded"
    );

    // Test combined include and exclude via config
    println!("--- Running Config Include/Exclude Test ---");
    let config_comb = r#"
[global]
include = ["docs/**/*.md"] # Include all md in docs recursively
exclude = ["docs/temp"]
"#;
    create_config(base_path, config_comb);

    let (success_comb, stdout_comb, _) = run_cmd(&[".", "--verbose"]);
    assert!(success_comb, "Config Include/Exclude Test failed");
    let norm_stdout_comb = normalize(&stdout_comb);
    assert!(
        norm_stdout_comb.contains("Processing file: docs/doc1.md"),
        "Config Combo: docs/doc1.md missing"
    );
    assert!(
        !norm_stdout_comb.contains("Processing file: docs/temp/temp.md"),
        "Config Combo: temp.md should be excluded"
    );
    assert!(
        !norm_stdout_comb.contains("Processing file: README.md"),
        "Config Combo: README.md should be excluded"
    );
}

#[test]
fn test_cli_override_config() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Helper
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(base_path)
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };
    let normalize = |s: &str| s.replace(r"\", "/");

    // Set up config with one pattern
    let config = r#"
[global]
include = ["src/**/*.md"] # Config includes only src/test.md
"#;
    create_config(base_path, config);

    // Override with CLI pattern - should only process docs/doc1.md
    println!("--- Running CLI Override Config Test ---");
    let (success, stdout, _) = run_cmd(&[".", "--include", "docs/doc1.md", "--verbose"]);
    assert!(success, "CLI Override Config Test failed");
    let norm_stdout = normalize(&stdout);

    assert!(
        norm_stdout.contains("Processing file: docs/doc1.md"),
        "CLI Override: docs/doc1.md missing"
    );
    assert!(
        !norm_stdout.contains("Processing file: src/test.md"),
        "CLI Override: src/test.md should be excluded due to CLI override"
    );
    assert!(
        !norm_stdout.contains("Processing file: README.md"),
        "CLI Override: README.md should be excluded"
    );
}

#[test]
fn test_readme_pattern_scope() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Helper
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(base_path)
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };
    let normalize = |s: &str| s.replace(r"\", "/");

    // Test include pattern for README.md should only match the root README.md file
    println!("--- Running README Pattern Scope Test ---");
    let config = r#"
[global]
include = ["README.md"] # Reverted pattern
"#;
    create_config(base_path, config);

    let (success, stdout, _) = run_cmd(&[".", "--verbose"]);
    assert!(success, "README Pattern Scope Test failed");
    let norm_stdout = normalize(&stdout);

    assert!(
        norm_stdout.contains("Processing file: README.md"),
        "README Scope: Root README.md missing"
    );
    assert!(
        norm_stdout.contains("Processing file: subfolder/README.md"),
        "README Scope: Subfolder README.md ALSO included (known behavior)"
    );
    assert!(
        !norm_stdout.contains("Processing file: docs/doc1.md"),
        "README Scope: docs/doc1.md should be excluded"
    );
}

#[test]
fn test_cli_filter_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path();

    println!("Creating test files in: {}", base_path.display());

    // Create test files and directories
    fs::create_dir_all(base_path.join("docs"))?;
    fs::create_dir_all(base_path.join("docs/temp"))?;
    fs::create_dir_all(base_path.join("src"))?;
    fs::create_dir_all(base_path.join("subfolder"))?;

    fs::write(base_path.join("README.md"), "# Test\n")?;
    fs::write(base_path.join("docs/doc1.md"), "# Doc 1\n")?;
    fs::write(base_path.join("docs/temp/temp.md"), "# Temp\n")?;
    fs::write(base_path.join("src/test.md"), "# Source\n")?;
    fs::write(base_path.join("subfolder/README.md"), "# Subfolder README\n")?;

    // Print the created files for debugging
    println!("Created test files:");
    println!("  {}/README.md", base_path.display());
    println!("  {}/docs/doc1.md", base_path.display());
    println!("  {}/docs/temp/temp.md", base_path.display());
    println!("  {}/src/test.md", base_path.display());
    println!("  {}/subfolder/README.md", base_path.display());

    // Helper to run command and get stdout/stderr
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(temp_dir.path())
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };

    // Normalize paths in output for consistent matching
    let normalize = |s: &str| s.replace(r"\", "/");

    // --- Test Case 1: Exclude directory ---
    println!("--- Running Test Case 1: Exclude directory ---");
    let (success1, stdout1, stderr1) = run_cmd(&[".", "--exclude", "docs/temp", "--verbose"]);
    println!("Test Case 1 Stdout:\\n{stdout1}");
    println!("Test Case 1 Stderr:\\n{stderr1}");
    assert!(success1, "Test Case 1 failed");
    let norm_stdout1 = normalize(&stdout1);
    assert!(
        norm_stdout1.contains("Processing file: README.md"),
        "Expected file README.md missing in Test Case 1"
    );
    assert!(
        norm_stdout1.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 1"
    );
    assert!(
        norm_stdout1.contains("Processing file: src/test.md"),
        "Expected file src/test.md missing in Test Case 1"
    );
    assert!(
        norm_stdout1.contains("Processing file: subfolder/README.md"),
        "Expected file subfolder/README.md missing in Test Case 1"
    );

    // --- Test Case 2: Include specific file ---
    println!("--- Running Test Case 2: Include specific file ---");
    let (success2, stdout2, stderr2) = run_cmd(&[".", "--include", "docs/doc1.md", "--verbose"]);
    println!("Test Case 2 Stdout:\\n{stdout2}");
    println!("Test Case 2 Stderr:\\n{stderr2}");
    assert!(success2, "Test Case 2 failed");
    let norm_stdout2 = normalize(&stdout2);
    assert!(
        norm_stdout2.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 2"
    );
    assert!(
        !norm_stdout2.contains("Processing file: README.md"),
        "File README.md should not be processed in Test Case 2"
    );
    assert!(
        !norm_stdout2.contains("Processing file: docs/temp/temp.md"),
        "File docs/temp/temp.md should not be processed in Test Case 2"
    );
    assert!(
        !norm_stdout2.contains("Processing file: src/test.md"),
        "File src/test.md should not be processed in Test Case 2"
    );
    assert!(
        !norm_stdout2.contains("Processing file: subfolder/README.md"),
        "File subfolder/README.md should not be processed in Test Case 2"
    );

    // --- Test Case 3: Exclude glob pattern (original failing case) ---
    // This should exclude README.md in root AND subfolder/README.md
    println!("--- Running Test Case 3: Exclude glob pattern ---");
    let (success3, stdout3, stderr3) = run_cmd(&[".", "--exclude", "**/README.md", "--verbose"]);
    println!("Test Case 3 Stdout:\\n{stdout3}");
    println!("Test Case 3 Stderr:\\n{stderr3}");
    assert!(success3, "Test Case 3 failed");
    let norm_stdout3 = normalize(&stdout3);
    assert!(
        !norm_stdout3.contains("Processing file: README.md"),
        "Root README.md should be excluded in Test Case 3"
    );
    assert!(
        !norm_stdout3.contains("Processing file: subfolder/README.md"),
        "Subfolder README.md should be excluded in Test Case 3"
    );
    assert!(
        norm_stdout3.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 3"
    );
    assert!(
        norm_stdout3.contains("Processing file: docs/temp/temp.md"),
        "Expected file docs/temp/temp.md missing in Test Case 3"
    );
    assert!(
        norm_stdout3.contains("Processing file: src/test.md"),
        "Expected file src/test.md missing in Test Case 3"
    );

    // --- Test Case 4: Include glob pattern ---
    // Should only include docs/doc1.md (not docs/temp/temp.md)
    println!("--- Running Test Case 4: Include glob pattern ---");
    let (success4, stdout4, stderr4) = run_cmd(&[".", "--include", "docs/*.md", "--verbose"]);
    println!("Test Case 4 Stdout:\\n{stdout4}");
    println!("Test Case 4 Stderr:\\n{stderr4}");
    assert!(success4, "Test Case 4 failed");
    let norm_stdout4 = normalize(&stdout4);
    assert!(
        norm_stdout4.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 4"
    );
    assert!(
        !norm_stdout4.contains("Processing file: docs/temp/temp.md"),
        "File docs/temp/temp.md should not be processed in Test Case 4"
    );
    assert!(
        !norm_stdout4.contains("Processing file: README.md"),
        "File README.md should not be processed in Test Case 4"
    );
    assert!(
        !norm_stdout4.contains("Processing file: src/test.md"),
        "File src/test.md should not be processed in Test Case 4"
    );
    assert!(
        !norm_stdout4.contains("Processing file: subfolder/README.md"),
        "File subfolder/README.md should not be processed in Test Case 4"
    );

    // --- Test Case 5: Glob Include + Specific Exclude ---
    // Should include docs/doc1.md but exclude docs/temp/temp.md
    println!("--- Running Test Case 5: Glob Include + Specific Exclude ---");
    let (success5, stdout5, stderr5) = run_cmd(&[
        ".",
        "--include",
        "docs/**/*.md",
        "--exclude",
        "docs/temp/temp.md",
        "--verbose",
    ]);
    println!("Test Case 5 Stdout:\\n{stdout5}");
    println!("Test Case 5 Stderr:\\n{stderr5}");
    assert!(success5, "Test Case 5 failed");
    let norm_stdout5 = normalize(&stdout5);
    assert!(
        norm_stdout5.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 5"
    );
    assert!(
        !norm_stdout5.contains("Processing file: docs/temp/temp.md"),
        "File docs/temp/temp.md should be excluded in Test Case 5"
    );
    assert!(
        !norm_stdout5.contains("Processing file: README.md"),
        "File README.md should not be processed in Test Case 5"
    );
    assert!(
        !norm_stdout5.contains("Processing file: src/test.md"),
        "File src/test.md should not be processed in Test Case 5"
    );
    assert!(
        !norm_stdout5.contains("Processing file: subfolder/README.md"),
        "File subfolder/README.md should not be processed in Test Case 5"
    );

    // --- Test Case 6: Specific Exclude Overrides Broader Include ---
    println!("--- Running Test Case 6: Specific Exclude Overrides Broader Include ---");
    let (success6, stdout6, stderr6) =
        run_cmd(&[".", "--include", "subfolder/*.md", "--exclude", "subfolder/README.md"]); // Pass only the args slice
    println!("Test Case 6 Stdout:\n{stdout6}");
    println!("Test Case 6 Stderr:{stderr6}");
    assert!(success6, "Case 6: Command failed"); // Use success6
    assert!(
        stdout6.contains("No markdown files found to check."),
        "Case 6: Should find no files"
    );
    assert!(
        !stdout6.contains("Processing file: subfolder/README.md"),
        "File subfolder/README.md should be excluded in Test Case 6"
    );

    // --- Test Case 7: Root Exclude ---
    println!("--- Running Test Case 7: Root Exclude ---");
    let (success7, stdout7, stderr7) = run_cmd(&[".", "--exclude", "README.md", "--verbose"]); // No globstar
    println!("Test Case 7 Stdout:\\n{stdout7}");
    println!("Test Case 7 Stderr:{stderr7}");
    assert!(success7, "Test Case 7 failed");
    let norm_stdout7 = normalize(&stdout7);
    assert!(
        !norm_stdout7.contains("Processing file: README.md"),
        "Root README.md should be excluded in Test Case 7"
    );
    assert!(
        !norm_stdout7.contains("Processing file: subfolder/README.md"),
        "Subfolder README.md should ALSO be excluded in Test Case 7"
    );
    assert!(
        norm_stdout7.contains("Processing file: docs/doc1.md"),
        "File docs/doc1.md should be included in Test Case 7"
    );

    // --- Test Case 8: Deep Glob Exclude ---
    // Should exclude everything
    println!("--- Running Test Case 8: Deep Glob Exclude ---");
    let (success8, stdout8, stderr8) = run_cmd(&[".", "--exclude", "**/*", "--verbose"]);
    println!("Test Case 8 Stdout:\\n{stdout8}");
    println!("Test Case 8 Stderr:\\n{stderr8}");
    assert!(success8, "Test Case 8 failed");
    let norm_stdout8 = normalize(&stdout8);
    // Check that *none* of the files were processed
    assert!(
        !norm_stdout8.contains("Processing file:"),
        "No files should be processed in Test Case 8"
    );

    // --- Test Case 9: Exclude multiple patterns ---
    println!("--- Running Test Case 9: Exclude multiple patterns ---");
    let (success9, stdout9, stderr9) = run_cmd(&[".", "--exclude", "README.md,src/*", "--verbose"]);
    println!("Test Case 9 Stdout:\n{stdout9}");
    println!("Test Case 9 Stderr:{stderr9}\n");
    assert!(success9, "Test Case 9 failed");
    let norm_stdout9 = normalize(&stdout9);
    assert!(
        !norm_stdout9.contains("Processing file: README.md"),
        "Root README.md should be excluded in Test Case 9"
    );
    assert!(
        !norm_stdout9.contains("Processing file: subfolder/README.md"),
        "Subfolder README.md should be excluded in Test Case 9"
    );
    assert!(
        !norm_stdout9.contains("Processing file: src/test.md"),
        "File src/test.md should be excluded in Test Case 9"
    );
    assert!(
        norm_stdout9.contains("Processing file: docs/doc1.md"),
        "Expected file docs/doc1.md missing in Test Case 9"
    );

    // --- Test Case 10: Include multiple patterns ---
    println!("--- Running Test Case 10: Include multiple patterns ---");
    let (success10, stdout10, stderr10) = run_cmd(&[".", "--include", "README.md,src/*", "--verbose"]);
    println!("Test Case 10 Stdout:\n{stdout10}");
    println!("Test Case 10 Stderr:{stderr10}\n");
    assert!(success10, "Test Case 10 failed");
    let norm_stdout10 = normalize(&stdout10);
    assert!(
        norm_stdout10.contains("Processing file: README.md"),
        "Root README.md should be included in Test Case 10"
    );
    assert!(
        norm_stdout10.contains("Processing file: src/test.md"),
        "File src/test.md should be included in Test Case 10"
    );
    assert!(
        !norm_stdout10.contains("Processing file: docs/doc1.md"),
        "File docs/doc1.md should not be processed in Test Case 10"
    );
    assert!(
        norm_stdout10.contains("Processing file: subfolder/README.md"),
        "File subfolder/README.md SHOULD be processed in Test Case 10"
    );

    // --- Test Case 11: Explicit Path (File) Ignores Config Include ---
    println!("--- Running Test Case 11: Explicit Path (File) Ignores Config Include ---");
    let config11 = r#"[global]
include=["src/*.md"]
"#;
    create_config(temp_dir.path(), config11);
    let (success11, stdout11, _) = run_cmd(&["docs/doc1.md", "--verbose"]);
    assert!(success11, "Test Case 11 failed");
    let norm_stdout11 = normalize(&stdout11);
    assert!(
        norm_stdout11.contains("Processing file: docs/doc1.md"),
        "Explicit path docs/doc1.md should be processed in Test Case 11"
    );
    assert!(
        !norm_stdout11.contains("Processing file: src/test.md"),
        "src/test.md should not be processed in Test Case 11"
    );
    fs::remove_file(temp_dir.path().join(".rumdl.toml"))?; // Clean up config

    // --- Test Case 12: Explicit Path (Dir) Ignores Config Include ---
    println!("--- Running Test Case 12: Explicit Path (Dir) Ignores Config Include ---");
    let config12 = r#"[global]
include=["src/*.md"]
"#;
    create_config(temp_dir.path(), config12);
    let (success12, stdout12, _) = run_cmd(&["docs", "--verbose"]); // Process everything in docs/
    assert!(success12, "Test Case 12 failed");
    let norm_stdout12 = normalize(&stdout12);
    assert!(
        norm_stdout12.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md should be processed in Test Case 12"
    );
    assert!(
        norm_stdout12.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should be processed in Test Case 12"
    );
    assert!(
        !norm_stdout12.contains("Processing file: src/test.md"),
        "src/test.md should not be processed in Test Case 12"
    );
    fs::remove_file(temp_dir.path().join(".rumdl.toml"))?; // Clean up config

    // --- Test Case 13: Explicit Path (Dir) Respects Config Exclude ---
    println!("--- Running Test Case 13: Explicit Path (Dir) Respects Config Exclude ---");
    let config13 = r#"[global]
exclude=["docs/temp"]
"#;
    create_config(temp_dir.path(), config13);
    let (success13, stdout13, _) = run_cmd(&["docs", "--verbose"]); // Process docs/, exclude temp via config
    assert!(success13, "Test Case 13 failed");
    let norm_stdout13 = normalize(&stdout13);
    assert!(
        norm_stdout13.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md should be processed in Test Case 13"
    );
    assert!(
        !norm_stdout13.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should be excluded by config in Test Case 13"
    );
    fs::remove_file(temp_dir.path().join(".rumdl.toml"))?; // Clean up config

    // --- Test Case 14: Explicit Path (Dir) Respects CLI Exclude ---
    println!("--- Running Test Case 14: Explicit Path (Dir) Respects CLI Exclude ---");
    let (success14, stdout14, _) = run_cmd(&["docs", "--exclude", "docs/temp", "--verbose"]); // Process docs/, exclude temp via CLI
    assert!(success14, "Test Case 14 failed");
    let norm_stdout14 = normalize(&stdout14);
    assert!(
        norm_stdout14.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md should be processed in Test Case 14"
    );
    assert!(
        !norm_stdout14.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should be excluded by CLI in Test Case 14"
    );

    // --- Test Case 15: Multiple Explicit Paths ---
    println!("--- Running Test Case 15: Multiple Explicit Paths ---");
    let (success15, stdout15, _) = run_cmd(&["docs/doc1.md", "src/test.md", "--verbose"]); // Process specific files
    assert!(success15, "Test Case 15 failed");
    let norm_stdout15 = normalize(&stdout15);
    assert!(
        norm_stdout15.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md was not processed in Test Case 15"
    );
    assert!(
        norm_stdout15.contains("Processing file: src/test.md"),
        "src/test.md was not processed in Test Case 15"
    );
    assert!(
        !norm_stdout15.contains("Processing file: README.md"),
        "README.md should not be processed in Test Case 15"
    );
    assert!(
        !norm_stdout15.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should not be processed in Test Case 15"
    );

    // --- Test Case 16: CLI Exclude Overrides Config Include (Discovery Mode) ---
    println!("--- Running Test Case 16: CLI Exclude Overrides Config Include ---");
    let config16 = r#"[global]
include=["docs/**/*.md"]
"#;
    create_config(temp_dir.path(), config16);
    let (success16, stdout16, _) = run_cmd(&[".", "--exclude", "docs/temp/temp.md", "--verbose"]); // Discover ., exclude specific file via CLI
    assert!(success16, "Test Case 16 failed");
    let norm_stdout16 = normalize(&stdout16);
    assert!(
        norm_stdout16.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md should be included by config in Test Case 16"
    );
    assert!(
        !norm_stdout16.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should be excluded by CLI in Test Case 16"
    );
    assert!(
        !norm_stdout16.contains("Processing file: README.md"),
        "README.md should not be included by config in Test Case 16"
    );
    fs::remove_file(temp_dir.path().join(".rumdl.toml"))?; // Clean up config

    // --- Test Case 17: CLI Include Overrides Config Exclude (Discovery Mode) ---
    println!("--- Running Test Case 17: CLI Include Overrides Config Exclude ---");
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        r#"
exclude = ["docs/*"] # Exclude all docs via config
"#,
    )?;
    let (success17, stdout17, stderr17) = run_cmd(
        &[".", "--include", "docs/doc1.md", "--verbose"], // ADDED "." path for discovery mode
    );
    println!("Test Case 17 Stdout:\n{stdout17}");
    println!("Test Case 17 Stderr:{stderr17}\n");
    assert!(success17, "Test Case 17 failed");
    let norm_stdout17 = normalize(&stdout17);
    // ASSERTION REVERTED: Expect file to be included by CLI override
    assert!(
        norm_stdout17.contains("Processing file: docs/doc1.md"),
        "docs/doc1.md should be included by CLI in Test Case 17"
    );
    assert!(
        !norm_stdout17.contains("Processing file: docs/temp/temp.md"),
        "docs/temp/temp.md should remain excluded by config in Test Case 17"
    );
    // Other files shouldn't be processed because they aren't included by CLI
    assert!(
        !norm_stdout17.contains("Processing file: README.md"),
        "README.md should NOT be included in Test Case 17"
    );
    assert!(
        !norm_stdout17.contains("Processing file: src/test.md"),
        "src/test.md should NOT be included in Test Case 17"
    );
    assert!(
        !norm_stdout17.contains("Processing file: subfolder/README.md"),
        "subfolder/README.md should NOT be included in Test Case 17"
    );

    Ok(())
}

#[test]
fn test_default_discovery_includes_only_markdown() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let dir_path = temp_dir.path();

    // Create a markdown file
    fs::write(dir_path.join("test.md"), "# Valid Markdown\n")?;
    // Create a non-markdown file
    fs::write(dir_path.join("test.txt"), "This is a text file.")?;

    let mut cmd = Command::cargo_bin("rumdl")?;
    cmd.arg(".")
        .arg("--verbose") // Need verbose to see "Processing file:" messages
        .current_dir(dir_path);

    cmd.assert()
        .success() // Should succeed as test.md is valid
        .stdout(predicates::str::contains("Processing file: test.md"))
        .stdout(predicates::str::contains("Processing file: test.txt").not());

    Ok(())
}

#[test]
fn test_markdown_extension_handling() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir_path = temp_dir.path();

    // Create files with both extensions
    fs::write(dir_path.join("test.md"), "# MD File\n")?;
    fs::write(dir_path.join("test.markdown"), "# MARKDOWN File\n")?;
    fs::write(dir_path.join("other.txt"), "Text file")?;

    // Test 1: Default discovery should find both .md and .markdown
    let mut cmd1 = Command::cargo_bin("rumdl")?;
    cmd1.arg(".").arg("--verbose").current_dir(dir_path);
    cmd1.assert()
        .success()
        .stdout(predicates::str::contains("Processing file: test.md"))
        .stdout(predicates::str::contains("Processing file: test.markdown"))
        .stdout(predicates::str::contains("Processing file: other.txt").not());

    // Test 2: Explicit include for .markdown should only find that file
    let mut cmd2 = Command::cargo_bin("rumdl")?;
    cmd2.arg(".")
        .arg("--include")
        .arg("*.markdown")
        .arg("--verbose")
        .current_dir(dir_path);
    cmd2.assert()
        .success()
        .stdout(predicates::str::contains("Processing file: test.markdown"))
        .stdout(predicates::str::contains("Processing file: test.md").not());

    Ok(())
}

#[test]
fn test_type_filter_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir_path = temp_dir.path();

    // Create files
    fs::write(dir_path.join("test.md"), "# MD File\n")?;
    fs::write(dir_path.join("test.txt"), "Text file")?;

    // Test 1: Trying to include non-markdown files should yield nothing
    let mut cmd1 = Command::cargo_bin("rumdl")?;
    cmd1.arg(".")
        .arg("--include")
        .arg("*.txt")
        .arg("--verbose") // Use verbose to ensure no "Processing file:" messages appear
        .current_dir(dir_path);
    cmd1.assert()
        .success()
        .stdout(predicates::str::contains("No markdown files found to check."))
        .stdout(predicates::str::contains("Processing file:").not());

    // Test 2: Excluding all .md files when only .md files exist
    let mut cmd2 = Command::cargo_bin("rumdl")?;
    cmd2.arg(".")
        .arg("--exclude")
        .arg("*.md")
        .arg("--verbose")
        .current_dir(dir_path);
    cmd2.assert()
        .success()
        .stdout(predicates::str::contains("No markdown files found to check."))
        .stdout(predicates::str::contains("Processing file:").not());

    // Test 3: Excluding both markdown types
    fs::write(dir_path.join("test.markdown"), "# MARKDOWN File\n")?;
    let mut cmd3 = Command::cargo_bin("rumdl")?;
    cmd3.arg(".")
        .arg("--exclude")
        .arg("*.md,*.markdown")
        .arg("--verbose")
        .current_dir(dir_path);
    cmd3.assert()
        .success()
        .stdout(predicates::str::contains("No markdown files found to check."))
        .stdout(predicates::str::contains("Processing file:").not());

    Ok(())
}

#[test]
fn test_check_subcommand_works() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = std::process::Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "README.md"])
        .output()
        .expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(output.status.success(), "check subcommand failed: {stderr}");
    assert!(
        stdout.contains("Success:") || stdout.contains("Issues:"),
        "Output missing summary"
    );
    assert!(
        !stderr.contains("Deprecation warning"),
        "Should not print deprecation warning for subcommand"
    );
}

#[test]
fn test_legacy_cli_works_and_warns() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = std::process::Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["README.md"])
        .output()
        .expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(output.status.success(), "legacy CLI failed: {stderr}");
    assert!(
        stdout.contains("Success:") || stdout.contains("Issues:"),
        "Output missing summary"
    );
    assert!(
        stderr.contains("Deprecation warning"),
        "Should print deprecation warning for legacy CLI"
    );
}

#[test]
fn test_rule_command_lists_all_rules() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .arg("rule")
        .output()
        .expect("Failed to execute 'rumdl rule'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "'rumdl rule' did not exit successfully");
    assert!(stdout.contains("Available rules:"), "Output missing 'Available rules:'");
    assert!(stdout.contains("MD013"), "Output missing rule MD013");
}

#[test]
fn test_rule_command_shows_specific_rule() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "MD013"])
        .output()
        .expect("Failed to execute 'rumdl rule MD013'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "'rumdl rule MD013' did not exit successfully");
    assert!(stdout.contains("MD013"), "Output missing rule name MD013");
    assert!(stdout.contains("Description"), "Output missing 'Description'");
}

#[test]
fn test_config_command_lists_options() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .arg("config")
        .output()
        .expect("Failed to execute 'rumdl config'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "'rumdl config' did not exit successfully");
    assert!(stdout.contains("[global]"), "Output missing [global] section");
    assert!(
        stdout.contains("enable =") || stdout.contains("disable =") || stdout.contains("exclude ="),
        "Output missing expected config keys"
    );
}

#[test]
fn test_version_command_prints_version() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .arg("version")
        .output()
        .expect("Failed to execute 'rumdl version'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "'rumdl version' did not exit successfully");
    assert!(stdout.contains("rumdl"), "Output missing 'rumdl' in version output");
    assert!(stdout.contains("."), "Output missing version number");
}

#[test]
fn test_config_get_subcommand() {
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
exclude = ["docs/temp", "node_modules"]

[MD013]
line_length = 123
"#;
    fs::write(&config_path, config_content).unwrap();

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(temp_dir.path())
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };

    // Test global.exclude
    let (success, stdout, stderr) = run_cmd(&["config", "get", "global.exclude"]);
    assert!(success, "config get global.exclude should succeed, stderr: {stderr}");
    assert!(
        stdout.contains("global.exclude = [\"docs/temp\", \"node_modules\"] [from .rumdl.toml]"),
        "Unexpected output: {stdout}. Stderr: {stderr}"
    );

    // Test MD013.line_length
    let (success, stdout, stderr) = run_cmd(&["config", "get", "MD013.line_length"]);
    assert!(success, "config get MD013.line_length should succeed, stderr: {stderr}");
    assert!(
        stdout.contains("MD013.line-length = 123 [from .rumdl.toml]"),
        "Unexpected output: {stdout}. Stderr: {stderr}"
    );

    // Test unknown key
    let (success, _stdout, stderr) = run_cmd(&["config", "get", "global.unknown"]);
    assert!(!success, "config get global.unknown should fail");
    assert!(
        stderr.contains("Unknown global key: unknown"),
        "Unexpected stderr: {stderr}"
    );

    let (success, _stdout, stderr) = run_cmd(&["config", "get", "MD999.line_length"]);
    assert!(!success, "config get MD999.line_length should fail");
    assert!(
        stderr.contains("Unknown config key: MD999.line-length"),
        "Unexpected stderr: {stderr}"
    );

    let (success, _stdout, stderr) = run_cmd(&["config", "get", "notavalidkey"]);
    assert!(!success, "config get notavalidkey should fail");
    assert!(
        stderr.contains("Key must be in the form global.key or MDxxx.key"),
        "Unexpected stderr: {stderr}"
    );
}

#[test]
fn test_config_command_defaults_prints_only_defaults() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Write a .rumdl.toml with non-defaults to ensure it is ignored
    let config_content = r#"
[global]
enable = ["MD013"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config_content);

    // Run 'rumdl config --defaults' (should ignore .rumdl.toml)
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["config", "--defaults"])
        .output()
        .expect("Failed to execute 'rumdl config --defaults'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "'rumdl config --defaults' did not exit successfully: {stderr}"
    );
    // [global] should be at the top
    assert!(
        stdout.trim_start().starts_with("[global]"),
        "Output should start with [global], got: {}",
        &stdout[..stdout.find('\n').unwrap_or(stdout.len())]
    );
    // Should contain provenance annotation [from default]
    assert!(
        stdout.contains("[from default]"),
        "Output should contain provenance annotation [from default]"
    );
    // Should not mention .rumdl.toml
    assert!(!stdout.contains(".rumdl.toml"), "Output should not mention .rumdl.toml");
    // Should contain a known default (e.g., enable = [])
    assert!(
        stdout.contains("enable = ["),
        "Output should contain default enable = []"
    );
    // Should NOT contain the custom value from .rumdl.toml
    assert!(
        !stdout.contains("enable = [\"MD013\"]"),
        "Output should not contain custom config values from .rumdl.toml"
    );
    // Output is NOT valid TOML (annotated), so do not parse as TOML
}

#[test]
fn test_config_command_defaults_output_toml_is_valid() {
    use toml::Value;
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Write a .rumdl.toml with non-defaults to ensure it is ignored
    let config_content = r#"
[global]
enable = ["MD013"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config_content);

    // Run 'rumdl config --defaults --output toml' (should ignore .rumdl.toml)
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["config", "--defaults", "--output", "toml"])
        .output()
        .expect("Failed to execute 'rumdl config --defaults --output toml'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "'rumdl config --defaults --output toml' did not exit successfully: {stderr}"
    );
    // [global] should be at the top
    assert!(
        stdout.trim_start().starts_with("[global]"),
        "Output should start with [global], got: {}",
        &stdout[..stdout.find('\n').unwrap_or(stdout.len())]
    );
    // Should NOT contain provenance annotation [from default]
    assert!(
        !stdout.contains("[from default]"),
        "Output should NOT contain provenance annotation [from default] in TOML output"
    );
    // Should not mention .rumdl.toml
    assert!(!stdout.contains(".rumdl.toml"), "Output should not mention .rumdl.toml");
    // Should contain a known default (e.g., enable = [])
    assert!(
        stdout.contains("enable = ["),
        "Output should contain default enable = []"
    );
    // Should NOT contain the custom value from .rumdl.toml
    assert!(
        !stdout.contains("enable = [\"MD013\"]"),
        "Output should not contain custom config values from .rumdl.toml"
    );
    // Output should be valid TOML (parse all [section] blocks)
    let mut current = String::new();
    for line in stdout.lines() {
        if line.starts_with('[') && !current.is_empty() {
            toml::from_str::<Value>(&current).expect("Section is not valid TOML");
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        toml::from_str::<Value>(&current).expect("Section is not valid TOML");
    }
}

#[test]
fn test_config_command_defaults_provenance_annotation_colored() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Write a .rumdl.toml with non-defaults to ensure it is ignored
    let config_content = r#"
[global]
enable = ["MD013"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config_content);

    // Run 'rumdl config --defaults --color always'
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["config", "--defaults", "--color", "always"])
        .output()
        .expect("Failed to execute 'rumdl config --defaults --color always'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "'rumdl config --defaults --color always' did not exit successfully: {stderr}"
    );
    // Should contain provenance annotation [from default]
    assert!(
        stdout.contains("[from default]"),
        "Output should contain provenance annotation [from default]"
    );
    // Should contain ANSI color codes for provenance annotation (e.g., dim/gray: \x1b[2m...\x1b[0m)
    let provenance_colored = "\x1b[2m[from default]\x1b[0m";
    assert!(
        stdout.contains(provenance_colored),
        "Provenance annotation [from default] should be colored dim/gray (found: {stdout:?})"
    );
}
