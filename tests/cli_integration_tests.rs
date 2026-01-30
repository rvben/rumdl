use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[path = "common/fixtures.rs"]
mod fixtures;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    fixtures::create_test_files(temp_dir.path(), "basic").unwrap();
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
    let (success_incl, stdout_incl, _) = run_cmd(&["check", ".", "--include", "docs/doc1.md", "--verbose"]);
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
    let (success_excl, stdout_excl, _) = run_cmd(&["check", ".", "--exclude", "docs/temp", "--verbose"]);
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
    let (success_comb, stdout_comb, _) = run_cmd(&[
        "check",
        ".",
        "--include",
        "docs/*.md",
        "--exclude",
        "docs/temp",
        "--verbose",
    ]);
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

    let (success_incl, stdout_incl, _) = run_cmd(&["check", ".", "--verbose"]);
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

    let (success_comb, stdout_comb, _) = run_cmd(&["check", ".", "--verbose"]);
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
    let (success, stdout, _) = run_cmd(&["check", ".", "--include", "docs/doc1.md", "--verbose"]);
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

    let (success, stdout, _) = run_cmd(&["check", ".", "--verbose"]);
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

    // Create test structure using fixtures
    fixtures::create_test_files(base_path, "basic")?;

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
    let (success1, stdout1, stderr1) = run_cmd(&["check", ".", "--exclude", "docs/temp", "--verbose"]);
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
    let (success2, stdout2, stderr2) = run_cmd(&["check", ".", "--include", "docs/doc1.md", "--verbose"]);
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
    let (success3, stdout3, stderr3) = run_cmd(&["check", ".", "--exclude", "**/README.md", "--verbose"]);
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
    let (success4, stdout4, stderr4) = run_cmd(&["check", ".", "--include", "docs/*.md", "--verbose"]);
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
        "check",
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
    let (success6, stdout6, stderr6) = run_cmd(&[
        "check",
        ".",
        "--include",
        "subfolder/*.md",
        "--exclude",
        "subfolder/README.md",
    ]); // Pass only the args slice
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
    let (success7, stdout7, stderr7) = run_cmd(&["check", ".", "--exclude", "README.md", "--verbose"]); // No globstar
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
    let (success8, stdout8, stderr8) = run_cmd(&["check", ".", "--exclude", "**/*", "--verbose"]);
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
    let (success9, stdout9, stderr9) = run_cmd(&["check", ".", "--exclude", "README.md,src/*", "--verbose"]);
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
    let (success10, stdout10, stderr10) = run_cmd(&["check", ".", "--include", "README.md,src/*", "--verbose"]);
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
    let (success11, stdout11, _) = run_cmd(&["check", "docs/doc1.md", "--verbose"]);
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
    let (success12, stdout12, _) = run_cmd(&["check", "docs", "--verbose"]); // Process everything in docs/
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
    let (success13, stdout13, _) = run_cmd(&["check", "docs", "--verbose"]); // Process docs/, exclude temp via config
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
    let (success14, stdout14, _) = run_cmd(&["check", "docs", "--exclude", "docs/temp", "--verbose"]); // Process docs/, exclude temp via CLI
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
    let (success15, stdout15, _) = run_cmd(&["check", "docs/doc1.md", "src/test.md", "--verbose"]); // Process specific files
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
    let (success16, stdout16, _) = run_cmd(&["check", ".", "--exclude", "docs/temp/temp.md", "--verbose"]); // Discover ., exclude specific file via CLI
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
        &["check", ".", "--include", "docs/doc1.md", "--verbose"], // ADDED "." path for discovery mode
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
fn test_force_exclude() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let dir_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create test files
    fs::create_dir_all(dir_path.join("excluded"))?;
    fs::write(dir_path.join("included.md"), "# Included\n")?;
    fs::write(dir_path.join("excluded.md"), "# Should be excluded\n")?;
    fs::write(dir_path.join("excluded/test.md"), "# In excluded dir\n")?;

    // Helper to run command
    let run_cmd = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(rumdl_exe)
            .current_dir(dir_path)
            .args(args)
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stdout, stderr)
    };
    let normalize = |s: &str| s.replace(r"\", "/");

    // Create config with exclude pattern
    let config = r#"[global]
exclude = ["excluded.md", "excluded/**"]
"#;
    fs::write(dir_path.join(".rumdl.toml"), config)?;

    // Test 1: Default behavior - explicitly provided files ARE excluded (new behavior as of v0.0.156)
    println!("--- Test 1: Default behavior (always respect excludes) ---");
    let (success1, stdout1, stderr1) = run_cmd(&["check", "excluded.md", "--verbose"]);
    assert!(success1, "Test 1 failed");
    let norm_stdout1 = normalize(&stdout1);
    let norm_stderr1 = normalize(&stderr1);
    assert!(
        norm_stderr1.contains("warning:")
            && norm_stderr1.contains("excluded.md")
            && norm_stderr1.contains("ignored because of exclude pattern"),
        "Default behavior: excluded.md should show warning about exclusion. stderr: {norm_stderr1}"
    );
    assert!(
        !norm_stdout1.contains("Processing file: excluded.md"),
        "Default behavior: excluded.md should NOT be processed"
    );

    // Test 2: included.md should still be processed
    println!("--- Test 2: Non-excluded files are processed ---");
    let (success2, stdout2, _) = run_cmd(&["check", "included.md", "--verbose"]);
    assert!(success2, "Test 2 failed");
    let norm_stdout2 = normalize(&stdout2);
    assert!(
        norm_stdout2.contains("Processing file: included.md"),
        "included.md should be processed"
    );

    // Test 3: Multiple files - only non-excluded are processed
    println!("--- Test 3: Multiple files with excludes ---");
    let (success3, stdout3, stderr3) = run_cmd(&["check", "included.md", "excluded.md", "--verbose"]);
    assert!(success3, "Test 3 failed");
    let norm_stdout3 = normalize(&stdout3);
    let norm_stderr3 = normalize(&stderr3);
    assert!(
        norm_stdout3.contains("Processing file: included.md"),
        "included.md should be processed"
    );
    assert!(
        norm_stderr3.contains("warning:")
            && norm_stderr3.contains("excluded.md")
            && norm_stderr3.contains("ignored because of exclude pattern"),
        "excluded.md should show warning about exclusion"
    );
    assert!(
        !norm_stdout3.contains("Processing file: excluded.md"),
        "excluded.md should NOT be processed"
    );

    // Test 4: Directory patterns work
    println!("--- Test 4: Directory patterns with excludes ---");
    let (success4, stdout4, stderr4) = run_cmd(&["check", "excluded/test.md", "--verbose"]);
    assert!(success4, "Test 4 failed");
    let norm_stdout4 = normalize(&stdout4);
    let norm_stderr4 = normalize(&stderr4);
    assert!(
        norm_stderr4.contains("warning:")
            && norm_stderr4.contains("excluded/test.md")
            && norm_stderr4.contains("ignored because of exclude pattern"),
        "Files in excluded dir should show warning about exclusion"
    );
    assert!(
        !norm_stdout4.contains("Processing file: excluded/test.md"),
        "excluded/test.md should NOT be processed"
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

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg(".")
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
    let mut cmd1 = cargo_bin_cmd!("rumdl");
    cmd1.arg("check").arg(".").arg("--verbose").current_dir(dir_path);
    cmd1.assert()
        .success()
        .stdout(predicates::str::contains("Processing file: test.md"))
        .stdout(predicates::str::contains("Processing file: test.markdown"))
        .stdout(predicates::str::contains("Processing file: other.txt").not());

    // Test 2: Explicit include for .markdown should only find that file
    let mut cmd2 = cargo_bin_cmd!("rumdl");
    cmd2.arg("check")
        .arg(".")
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

    // Test 1: --include allows checking non-markdown files (e.g., .txt)
    let mut cmd1 = cargo_bin_cmd!("rumdl");
    cmd1.arg("check")
        .arg(".")
        .arg("--include")
        .arg("*.txt")
        .arg("--verbose")
        .current_dir(dir_path);
    cmd1.assert()
        .code(1) // Should fail because test.txt has linting issues
        .stdout(predicates::str::contains("Processing file: test.txt"))
        .stdout(predicates::str::contains("MD041")) // First line should be heading
        .stdout(predicates::str::contains("MD047")); // Should end with newline

    // Test 2: Excluding all .md files when only .md files exist
    let mut cmd2 = cargo_bin_cmd!("rumdl");
    cmd2.arg("check")
        .arg(".")
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
    let mut cmd3 = cargo_bin_cmd!("rumdl");
    cmd3.arg("check")
        .arg(".")
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

    // Test that direct file path doesn't work anymore
    let output = std::process::Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["README.md"])
        .output()
        .expect("Failed to execute command");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Should fail and show help because "README.md" is not a valid subcommand
    assert!(!output.status.success(), "legacy CLI should fail");
    assert!(
        stderr.contains("error:") || stderr.contains("Usage:"),
        "Should show error or usage for invalid subcommand"
    );

    // Test that new syntax with 'check' works
    let output = std::process::Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "README.md"])
        .output()
        .expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(output.status.success(), "new CLI with check should work");
    assert!(
        stdout.contains("Success:") || stdout.contains("Issues:"),
        "Output missing summary"
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
    // Updated to match new output format
    assert!(
        stdout.contains("Name:") || stdout.contains("Description"),
        "Output missing expected field"
    );
}

#[test]
fn test_rule_command_json_output_all_rules() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule --output-format json'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "'rumdl rule --output-format json' did not exit successfully"
    );

    // Parse the JSON output
    let rules: serde_json::Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert!(rules.is_array(), "Expected JSON array");
    let rules_array = rules.as_array().unwrap();
    assert!(!rules_array.is_empty(), "Expected at least one rule");

    // Check structure of first rule
    let first_rule = &rules_array[0];
    assert!(first_rule.get("code").is_some(), "Missing 'code' field");
    assert!(first_rule.get("name").is_some(), "Missing 'name' field");
    assert!(first_rule.get("aliases").is_some(), "Missing 'aliases' field");
    assert!(first_rule.get("summary").is_some(), "Missing 'summary' field");
    assert!(first_rule.get("category").is_some(), "Missing 'category' field");
    assert!(first_rule.get("fix").is_some(), "Missing 'fix' field");
    assert!(
        first_rule.get("fix_availability").is_some(),
        "Missing 'fix_availability' field"
    );
    assert!(first_rule.get("url").is_some(), "Missing 'url' field");

    // Verify MD001 is present
    let md001 = rules_array
        .iter()
        .find(|r| r.get("code").and_then(|c| c.as_str()) == Some("MD001"));
    assert!(md001.is_some(), "MD001 not found in rules");
    let md001 = md001.unwrap();
    assert_eq!(md001.get("name").and_then(|n| n.as_str()), Some("heading-increment"));
    assert_eq!(md001.get("category").and_then(|c| c.as_str()), Some("heading"));
    assert!(
        md001.get("url").and_then(|u| u.as_str()).unwrap().contains("rumdl.dev"),
        "URL should contain rumdl.dev"
    );
}

#[test]
fn test_rule_command_json_output_single_rule() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "MD041", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule MD041 --output-format json'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "'rumdl rule MD041 --output-format json' did not exit successfully"
    );

    // Parse the JSON output (single object, not array)
    let rule: serde_json::Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert!(rule.is_object(), "Expected JSON object for single rule");

    assert_eq!(rule.get("code").and_then(|c| c.as_str()), Some("MD041"));
    assert_eq!(rule.get("name").and_then(|n| n.as_str()), Some("first-line-h1"));
    // MD041 has "first-line-heading" as an alias
    let aliases = rule.get("aliases").and_then(|a| a.as_array()).unwrap();
    assert!(aliases.iter().any(|a| a.as_str() == Some("first-line-heading")));
    assert_eq!(
        rule.get("url").and_then(|u| u.as_str()),
        Some("https://rumdl.dev/md041/")
    );
}

#[test]
fn test_rule_command_json_fix_availability_values() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule --output-format json'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rules: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    // Verify fix_availability values are one of the expected values
    for rule in &rules {
        let fix_avail = rule.get("fix_availability").and_then(|f| f.as_str()).unwrap();
        assert!(
            matches!(fix_avail, "Always" | "Sometimes" | "None"),
            "Unexpected fix_availability value: {} for rule {}",
            fix_avail,
            rule.get("code").and_then(|c| c.as_str()).unwrap_or("unknown")
        );
    }

    // Verify at least one unfixable rule exists (MD033 - no-inline-html)
    let md033 = rules
        .iter()
        .find(|r| r.get("code").and_then(|c| c.as_str()) == Some("MD033"));
    assert!(md033.is_some(), "MD033 not found");
    assert_eq!(
        md033.unwrap().get("fix_availability").and_then(|f| f.as_str()),
        Some("None")
    );
}

#[test]
fn test_rule_command_fixable_filter() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--fixable", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule --fixable'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rules: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    // All returned rules should be fixable (Always or Sometimes)
    for rule in &rules {
        let fix_avail = rule.get("fix_availability").and_then(|f| f.as_str()).unwrap();
        assert!(
            matches!(fix_avail, "Always" | "Sometimes"),
            "Non-fixable rule {} returned with --fixable filter",
            rule.get("code").and_then(|c| c.as_str()).unwrap_or("unknown")
        );
    }

    // Should not include MD033 (no-inline-html) which has fix_availability = None
    let has_md033 = rules
        .iter()
        .any(|r| r.get("code").and_then(|c| c.as_str()) == Some("MD033"));
    assert!(!has_md033, "MD033 should not be included with --fixable filter");
}

#[test]
fn test_rule_command_category_filter() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--category", "heading", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule --category heading'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rules: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    assert!(!rules.is_empty(), "Should return at least one heading rule");

    // All returned rules should have category "heading"
    for rule in &rules {
        let category = rule.get("category").and_then(|c| c.as_str()).unwrap();
        assert_eq!(
            category,
            "heading",
            "Rule {} has category {} instead of heading",
            rule.get("code").and_then(|c| c.as_str()).unwrap_or("unknown"),
            category
        );
    }

    // Should include MD001 (heading-increment)
    let has_md001 = rules
        .iter()
        .any(|r| r.get("code").and_then(|c| c.as_str()) == Some("MD001"));
    assert!(has_md001, "MD001 should be included with --category heading");
}

#[test]
fn test_rule_command_combined_filters() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--fixable", "--category", "heading", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule --fixable --category heading'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rules: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    assert!(!rules.is_empty(), "Should return at least one fixable heading rule");

    // All returned rules should be fixable AND have category heading
    for rule in &rules {
        let fix_avail = rule.get("fix_availability").and_then(|f| f.as_str()).unwrap();
        let category = rule.get("category").and_then(|c| c.as_str()).unwrap();

        assert!(
            matches!(fix_avail, "Always" | "Sometimes"),
            "Rule {} should be fixable",
            rule.get("code").and_then(|c| c.as_str()).unwrap_or("unknown")
        );
        assert_eq!(
            category,
            "heading",
            "Rule {} should have category heading",
            rule.get("code").and_then(|c| c.as_str()).unwrap_or("unknown")
        );
    }
}

#[test]
fn test_rule_command_json_lines_format() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--output-format", "json-lines"])
        .output()
        .expect("Failed to execute 'rumdl rule --output-format json-lines'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Each line should be valid JSON
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "Should output at least one line");

    for (i, line) in lines.iter().enumerate() {
        let rule: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("Line {i} is not valid JSON: {e}"));
        assert!(rule.get("code").is_some(), "Line {i} missing 'code' field");
        assert!(rule.get("name").is_some(), "Line {i} missing 'name' field");
    }

    // First line should be MD001
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(
        first.get("code").and_then(|c| c.as_str()),
        Some("MD001"),
        "First line should be MD001"
    );
}

#[test]
fn test_rule_command_explain_flag() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "MD001", "--output-format", "json", "--explain"])
        .output()
        .expect("Failed to execute 'rumdl rule MD001 --explain'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rule: serde_json::Value = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    // Should have explanation field
    let explanation = rule.get("explanation").and_then(|e| e.as_str());
    assert!(explanation.is_some(), "Should have explanation field with --explain");
    assert!(
        explanation.unwrap().contains("heading"),
        "Explanation should contain 'heading'"
    );

    // Without --explain, should not have explanation field
    let output_no_explain = Command::new(rumdl_exe)
        .args(["rule", "MD001", "--output-format", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule MD001'");
    let stdout_no_explain = String::from_utf8_lossy(&output_no_explain.stdout).to_string();
    let rule_no_explain: serde_json::Value = serde_json::from_str(&stdout_no_explain).expect("Failed to parse JSON");

    assert!(
        rule_no_explain.get("explanation").is_none(),
        "Should not have explanation field without --explain"
    );
}

#[test]
fn test_rule_command_text_output_with_filters() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--fixable", "--category", "heading"])
        .output()
        .expect("Failed to execute 'rumdl rule --fixable --category heading'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should show filter info in header
    assert!(stdout.contains("fixable"), "Output should mention fixable filter");
    assert!(stdout.contains("heading"), "Output should mention category filter");

    // Should show total count
    assert!(stdout.contains("Total:"), "Output should show total count");

    // Should include MD001
    assert!(stdout.contains("MD001"), "Should include MD001 in output");
}

#[test]
fn test_rule_command_list_categories() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--list-categories"])
        .output()
        .expect("Failed to execute 'rumdl rule --list-categories'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(output.status.success(), "Should exit successfully");
    assert!(stdout.contains("Available categories:"), "Should show header");
    assert!(stdout.contains("heading"), "Should list heading category");
    assert!(stdout.contains("whitespace"), "Should list whitespace category");
    assert!(stdout.contains("rules)"), "Should show rule counts");
}

#[test]
fn test_rule_command_invalid_category_error() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["rule", "--category", "nonexistent"])
        .output()
        .expect("Failed to execute 'rumdl rule --category nonexistent'");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(!output.status.success(), "Should exit with error");
    assert!(stderr.contains("Invalid category"), "Should mention invalid category");
    assert!(stderr.contains("Valid categories:"), "Should list valid categories");
    assert!(stderr.contains("heading"), "Should show heading as valid option");
}

#[test]
fn test_rule_command_short_flags() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test -f (fixable) and -c (category) short flags
    let output = Command::new(rumdl_exe)
        .args(["rule", "-f", "-c", "heading", "-o", "json"])
        .output()
        .expect("Failed to execute 'rumdl rule -f -c heading -o json'");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let rules: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    assert!(!rules.is_empty(), "Should return at least one rule");

    // All rules should be fixable and in heading category
    for rule in &rules {
        let fix_avail = rule.get("fix_availability").and_then(|f| f.as_str()).unwrap();
        let category = rule.get("category").and_then(|c| c.as_str()).unwrap();

        assert!(matches!(fix_avail, "Always" | "Sometimes"), "Rule should be fixable");
        assert_eq!(category, "heading", "Rule should be in heading category");
    }
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
        stdout.contains("global.exclude = [\"docs/temp\", \"node_modules\"] [from project config]"),
        "Unexpected output: {stdout}. Stderr: {stderr}"
    );

    // Test MD013.line_length
    let (success, stdout, stderr) = run_cmd(&["config", "get", "MD013.line_length"]);
    assert!(success, "config get MD013.line_length should succeed, stderr: {stderr}");
    assert!(
        stdout.contains("MD013.line-length = 123 [from project config]"),
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

#[test]
fn test_stdin_formatting() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test case 1: Format markdown with trailing spaces
    let input = "# Test   \n\nTest paragraph   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("--stdin").arg("--fix").arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Fixed content should be on stdout
    // Note: MD009 removes all trailing spaces from headings, but preserves 2 spaces
    // for line breaks in regular text (br_spaces: 2)
    // Note: Output will have a trailing newline even if input doesn't
    assert_eq!(stdout, "# Test\n\nTest paragraph\n");
    // No errors should be on stderr in quiet mode
    assert_eq!(stderr, "");
    assert!(output.status.success());
}

#[test]
fn test_stdin_formatting_with_remaining_issues() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test case with fixable issues (trailing spaces) and unfixable issues (duplicate heading)
    let input = "# Test   \n## Test\n# Test";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("--stdin").arg("--fix");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Fixed content should be on stdout
    // Note: MD009 removes all trailing spaces from headings
    // MD001 and MD003 add blank lines around headings
    // Note: Output will have a trailing newline even if input doesn't
    assert_eq!(stdout, "# Test\n\n## Test\n\n## Test\n");
    // Should report remaining issues on stderr
    assert!(stderr.contains("MD024"));
    assert!(stderr.contains("remaining"));
    // Should exit with error due to remaining issues
    assert!(!output.status.success());
}

#[test]
fn test_stdin_check_without_fix() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that check mode without --fix reports issues but doesn't output fixed content
    let input = "# Test   \n\nTest   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("--stdin");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not output content to stdout in check mode
    assert_eq!(stdout, "");
    // Should report issues on stderr
    assert!(stderr.contains("MD009"));
    assert!(stderr.contains("trailing spaces"));
    // MD047 is also triggered since input doesn't end with newline
    assert!(stderr.contains("Found 3 issue(s)"));
    // Should exit with error due to issues
    assert!(!output.status.success());
}

#[test]
fn test_stdin_formatting_no_issues() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that formatting mode outputs the original content when there are no issues
    // This was a bug where it would output "No issues found in stdin" instead
    let input = "# Clean Markdown\n\nThis markdown has no issues.\n";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("fmt").arg("-").arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should output the original content unchanged
    assert_eq!(stdout, input, "fmt should output original content when no issues found");
    // No errors should be on stderr in quiet mode
    assert_eq!(stderr, "");
    assert!(output.status.success());
}

#[test]
fn test_stdin_dash_syntax() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that '-' works as stdin indicator
    let input = "# Test   \n\nTest   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("-");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not output content to stdout in check mode
    assert_eq!(stdout, "");
    // Should report issues on stderr
    assert!(stderr.contains("MD009"));
    assert!(stderr.contains("trailing spaces"));
    assert!(stderr.contains("Found 3 issue(s)"));
}

#[test]
fn test_stdin_filename_flag() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that --stdin-filename changes the displayed filename in error messages
    let input = "# Test   \n\nTest paragraph   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("-").arg("--stdin-filename").arg("test-file.md");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not output content to stdout in check mode
    assert_eq!(stdout, "");
    // Should show the custom filename in error messages
    assert!(
        stderr.contains("test-file.md:1:"),
        "Error message should contain custom filename"
    );
    assert!(
        stderr.contains("test-file.md:3:"),
        "Error message should contain custom filename for line 3"
    );
    assert!(stderr.contains("in test-file.md"), "Summary should use custom filename");
    // Should still detect the issues
    assert!(stderr.contains("MD009"));
}

#[test]
fn test_stdin_filename_in_fmt_mode() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that --stdin-filename also works in fmt mode
    let input = "# Clean Markdown\n\nNo issues here.\n";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("fmt")
        .arg("-")
        .arg("--stdin-filename")
        .arg("custom-file.md")
        .arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // In fmt mode with no issues, should output the original content
    assert_eq!(stdout, input, "Should output original content");
    // No errors should be on stderr in quiet mode
    assert_eq!(stderr, "");
    assert!(output.status.success());
}

#[test]
fn test_fmt_dash_syntax() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that 'fmt -' works for formatting
    let input = "# Test   \n\nTest   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("fmt").arg("-");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should output formatted content
    assert_eq!(stdout, "# Test\n\nTest\n");
    // Should exit successfully
    assert!(output.status.success());
}

#[test]
fn test_fmt_command() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test that fmt command works as an alias for check --fix
    let input = "# Test   \n\nTest paragraph   ";
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("fmt").arg("--stdin").arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn command");

    // Write input to stdin
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should output formatted content to stdout (same as check --fix)
    assert_eq!(stdout, "# Test\n\nTest paragraph\n");
    // No errors in quiet mode
    assert_eq!(stderr, "");
    assert!(output.status.success());
}

#[test]
fn test_fmt_vs_check_fix_exit_codes() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test content with an unfixable violation (MD041 - first line heading)
    // and fixable violations (missing blank line before heading - MD022)
    let input = "Some text\n# Title\n";

    // Test 1: fmt should exit 0 even if unfixable violations remain
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("fmt").arg("-").arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn fmt command");
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait for fmt command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should output formatted content (blank line added before heading)
    assert_eq!(stdout, "Some text\n\n# Title\n");
    // fmt should exit 0 even though MD041 violation remains
    assert!(output.status.success(), "fmt should exit 0 on successful formatting");

    // Test 2: check --fix should exit 1 if unfixable violations remain
    let mut cmd = Command::new(rumdl_exe);
    cmd.arg("check").arg("--fix").arg("-").arg("--quiet");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn check --fix command");
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("Failed to wait for check --fix command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should output formatted content (same as fmt)
    assert_eq!(stdout, "Some text\n\n# Title\n");
    // check --fix should exit 1 because MD041 violation remains
    assert!(
        !output.status.success(),
        "check --fix should exit 1 when unfixable violations remain"
    );
    assert_eq!(output.status.code(), Some(1), "check --fix should exit with code 1");
}

/// Test that --include allows checking files with non-standard extensions (issue #127)
#[test]
fn test_include_nonstandard_extensions() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir_path = temp_dir.path();

    // Create files with both standard and non-standard extensions
    fs::write(
        dir_path.join("template.md.jinja"),
        "# Template\n\nThis is a Jinja2 template.\n",
    )?;
    fs::write(
        dir_path.join("regular.md"),
        "# Regular\n\nThis is a regular markdown file.\n",
    )?;
    fs::write(dir_path.join("config.yml.j2"), "# Not markdown\n\nThis is YAML.\n")?;

    // Test 1: Default behavior should only find regular.md
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg(".").arg("--verbose").current_dir(dir_path);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Processing file: regular.md"))
        .stdout(predicates::str::contains("template.md.jinja").not())
        .stdout(predicates::str::contains("config.yml.j2").not());

    // Test 2: --include with *.md.jinja should find template.md.jinja
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg(".")
        .arg("--include")
        .arg("**/*.md.jinja")
        .arg("--verbose")
        .current_dir(dir_path);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Processing file: template.md.jinja"));

    // Test 3: --include should still respect patterns (not find yml.j2)
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg(".")
        .arg("--include")
        .arg("**/*.md.jinja")
        .arg("--verbose")
        .current_dir(dir_path);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("config.yml.j2").not());

    Ok(())
}

/// Test that explicit file paths work with non-standard extensions (issue #127)
#[test]
fn test_explicit_path_nonstandard_extensions() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir_path = temp_dir.path();

    // Create a file with non-standard extension
    let jinja_file = dir_path.join("template.md.jinja");
    fs::write(&jinja_file, "# Jinja Template\n\nThis should be checked.\n")?;

    // Test: Explicitly providing the file path should work
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg(&jinja_file).arg("--verbose");

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Processing file:"))
        .stdout(predicates::str::contains("template.md.jinja"));

    Ok(())
}

/// Test that --include works with multiple non-standard extensions
#[test]
fn test_include_multiple_nonstandard_extensions() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir_path = temp_dir.path();

    // Create files with various extensions
    fs::write(dir_path.join("template.md.jinja"), "# Jinja\n")?;
    fs::write(dir_path.join("readme.md.tmpl"), "# Template\n")?;
    fs::write(dir_path.join("doc.md.erb"), "# ERB\n")?;
    fs::write(dir_path.join("regular.md"), "# Regular\n")?;

    // Test: Include multiple non-standard extensions
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg(".")
        .arg("--include")
        .arg("**/*.md.jinja,**/*.md.tmpl,**/*.md.erb")
        .arg("--verbose")
        .current_dir(dir_path);

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should find all three non-standard extension files
    assert!(stdout.contains("template.md.jinja"), "Should find .md.jinja file");
    assert!(stdout.contains("readme.md.tmpl"), "Should find .md.tmpl file");
    assert!(stdout.contains("doc.md.erb"), "Should find .md.erb file");
    // Should NOT find regular.md (not in include pattern)
    assert!(
        !stdout.contains("regular.md"),
        "Should not find regular.md when using specific --include"
    );

    Ok(())
}

// Tests for Issue #197: Exit code behavior with --fix
// These tests verify that rumdl check --fix returns the correct exit code
mod issue197_exit_code {
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_exit_code_after_all_fixes() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create a file with a fixable issue (MD007 - list indentation)
        // This will be fixed by --fix
        fs::write(
            &test_file,
            "# Heading\n\n- list item\n    - nested item (4 spaces, should be 2)\n",
        )
        .unwrap();

        // Create config to set MD007 indent to 2
        let config_file = temp_dir.path().join(".rumdl.toml");
        fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

        // Run rumdl check --fix
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--fix")
            .arg(test_file.to_str().unwrap())
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        // Verify the fix was applied
        assert!(
            stdout.contains("[fixed]") || stdout.contains("Fixed:"),
            "Should show that issues were fixed. stdout: {stdout}\nstderr: {stderr}"
        );

        // Verify exit code is 0 when all issues are fixed
        assert_eq!(
            exit_code, 0,
            "Exit code should be 0 when all issues are fixed. stdout: {stdout}\nstderr: {stderr}\nexit_code: {exit_code}"
        );

        // Verify the message shows all issues were fixed
        assert!(
            stdout.contains("Fixed:") && (stdout.contains("Fixed 1/1") || stdout.contains("Fixed: 1/1")),
            "Should show 'Fixed: Fixed 1/1 issues' message. stdout: {stdout}"
        );
    }

    #[test]
    fn test_exit_code_with_remaining_issues() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create a file with both fixable and unfixable issues
        // MD007 (fixable) and MD041 (unfixable - first line must be heading)
        fs::write(
            &test_file,
            "This is not a heading (MD041 violation - unfixable)\n\n- list item\n    - nested item (MD007 violation - fixable)\n",
        )
        .unwrap();

        // Create config to set MD007 indent to 2
        let config_file = temp_dir.path().join(".rumdl.toml");
        fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

        // Run rumdl check --fix
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--fix")
            .arg(test_file.to_str().unwrap())
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        // Verify exit code is 1 when some issues remain (unfixable)
        assert_eq!(
            exit_code, 1,
            "Exit code should be 1 when unfixable issues remain. stdout: {stdout}\nstderr: {stderr}\nexit_code: {exit_code}"
        );
    }

    /// Test that verifies the fix implementation re-lints after applying fixes.
    ///
    /// This addresses a concern raised by @martimlobao on issue #197:
    /// If --fix creates NEW issues while fixing existing ones (e.g., MD005/MD007 conflict),
    /// the exit code should still be 1.
    ///
    /// The implementation in file_processor.rs:668-740 handles this by:
    /// 1. Applying all fixes to the content
    /// 2. Re-linting the fixed content with all rules
    /// 3. Returning exit code based on remaining_warnings (which includes ANY issues, new or old)
    ///
    /// This test verifies that behavior by checking that:
    /// - The fix is applied (issue count decreases)
    /// - But exit code is 1 if ANY issues remain after fixing
    #[test]
    fn test_relint_after_fix_catches_remaining_issues() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create a file where:
        // - MD007 will fix the indentation
        // - But MD041 (first line not heading) remains unfixable
        // This verifies the re-lint catches issues that weren't part of the original fix
        fs::write(&test_file, "Not a heading\n\n- item\n    - nested\n").unwrap();

        let config_file = temp_dir.path().join(".rumdl.toml");
        fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

        // First, verify the file has multiple issues
        let check_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg(test_file.to_str().unwrap())
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let check_stdout = String::from_utf8_lossy(&check_output.stdout);
        assert!(
            check_stdout.contains("MD007") && check_stdout.contains("MD041"),
            "File should have both MD007 and MD041 issues. stdout: {check_stdout}"
        );

        // Now run --fix
        let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--fix")
            .arg(test_file.to_str().unwrap())
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
        let fix_stderr = String::from_utf8_lossy(&fix_output.stderr);
        let exit_code = fix_output.status.code().unwrap_or(-1);

        // Verify MD007 was fixed
        assert!(
            fix_stdout.contains("[fixed]"),
            "MD007 should be fixed. stdout: {fix_stdout}"
        );

        // Verify exit code is 1 because MD041 still remains
        // This proves the implementation re-lints after fixing and catches remaining issues
        assert_eq!(
            exit_code, 1,
            "Exit code should be 1 when issues remain after fix (re-lint catches them). \
             stdout: {fix_stdout}\nstderr: {fix_stderr}"
        );

        // Verify the content was actually modified (fix was applied)
        let fixed_content = fs::read_to_string(&test_file).unwrap();
        assert!(
            fixed_content.contains("  - nested"),
            "Content should be fixed (2 spaces). Got: {fixed_content}"
        );
    }
}

/// Test that `rumdl fmt` correctly reports the number of files that were actually fixed
/// (GitHub issue #347: summary underreported changed files)
///
/// This test verifies that when all issues in a file are fixed (leaving no remaining issues),
/// the file is still counted in the "Fixed X issues in Y files" summary.
#[test]
fn test_fmt_files_fixed_count_reports_actual_modified_files() {
    let temp_dir = tempdir().unwrap();

    // Create file A with fixable issues (no space after # in heading)
    let file_a = temp_dir.path().join("file_a.md");
    fs::write(
        &file_a,
        r#"# Heading A

#Bad heading A1

#Bad heading A2
"#,
    )
    .unwrap();

    // Create file B with fixable issues
    let file_b = temp_dir.path().join("file_b.md");
    fs::write(
        &file_b,
        r#"# Heading B

#Bad heading B1
"#,
    )
    .unwrap();

    // Create file C with NO issues (clean file)
    let file_c = temp_dir.path().join("file_c.md");
    fs::write(
        &file_c,
        r#"# Clean file

This file has no issues.
"#,
    )
    .unwrap();

    // Create file D with fixable issues
    let file_d = temp_dir.path().join("file_d.md");
    fs::write(
        &file_d,
        r#"# Heading D

#Bad heading D1

#Bad heading D2

#Bad heading D3
"#,
    )
    .unwrap();

    // Run rumdl fmt
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The summary should report 3 files fixed (A, B, D), NOT 0 or 4
    // Before the fix, it would show "in 0 files" because no remaining issues
    assert!(
        stdout.contains("in 3 files") || stdout.contains("in 3 file"),
        "Summary should report 3 files fixed (not 0 or 4). Got:\n{stdout}"
    );

    // Verify files were actually modified
    let content_a = fs::read_to_string(&file_a).unwrap();
    assert!(
        content_a.contains("## Bad heading A1"),
        "File A should be modified. Got:\n{content_a}"
    );

    let content_c = fs::read_to_string(&file_c).unwrap();
    assert!(
        content_c.contains("# Clean file"),
        "File C should not be modified. Got:\n{content_c}"
    );
}

/// Test that files_fixed count is correct when some files have unfixable issues
#[test]
fn test_fmt_files_fixed_count_with_unfixable_issues() {
    let temp_dir = tempdir().unwrap();

    // Create file A with fixable issues only
    let file_a = temp_dir.path().join("file_a.md");
    fs::write(
        &file_a,
        r#"# Heading A

#Bad heading A1
"#,
    )
    .unwrap();

    // Create file B with unfixable issue (MD041 - first line should be heading)
    let file_b = temp_dir.path().join("file_b.md");
    fs::write(
        &file_b,
        r#"This file starts with text, not a heading.

# Later heading
"#,
    )
    .unwrap();

    // Create file C with NO issues
    let file_c = temp_dir.path().join("file_c.md");
    fs::write(
        &file_c,
        r#"# Clean file

This file has no issues.
"#,
    )
    .unwrap();

    // Run rumdl fmt
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Only file A should be counted as fixed (file B has unfixable issues)
    assert!(
        stdout.contains("in 1 file"),
        "Summary should report 1 file fixed (only file_a). Got:\n{stdout}"
    );

    // Verify file A was modified
    let content_a = fs::read_to_string(&file_a).unwrap();
    assert!(
        content_a.contains("## Bad heading A1"),
        "File A should be modified. Got:\n{content_a}"
    );
}

/// Test that files_fixed is 0 when no files are actually modified
#[test]
fn test_fmt_files_fixed_count_zero_when_no_changes() {
    let temp_dir = tempdir().unwrap();

    // Create clean files only
    let file_a = temp_dir.path().join("file_a.md");
    fs::write(
        &file_a,
        r#"# Clean file A

This file has no issues.
"#,
    )
    .unwrap();

    let file_b = temp_dir.path().join("file_b.md");
    fs::write(
        &file_b,
        r#"# Clean file B

This file also has no issues.
"#,
    )
    .unwrap();

    // Run rumdl fmt
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show success with no issues found
    assert!(
        stdout.contains("No issues found") || stdout.contains("Success"),
        "Summary should indicate no issues found. Got:\n{stdout}"
    );
}

/// Test that MD033 warnings are NOT counted as fixable (issue #349)
/// MD033 has LSP-only fixes (for VS Code quick actions) but declares FixCapability::Unfixable
#[test]
fn test_md033_not_counted_as_fixable() {
    let temp_dir = tempdir().unwrap();

    // Create file with MD033 violation (inline HTML)
    let file = temp_dir.path().join("test.md");
    fs::write(
        &file,
        r#"# Test

This has <b>inline HTML</b> which triggers MD033.
"#,
    )
    .unwrap();

    // Run rumdl check - should report issue but NOT as fixable
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should find MD033 issue
    assert!(
        stdout.contains("MD033") || stderr.contains("MD033"),
        "Should detect MD033 violation. Got stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Should NOT show fixable count (MD033 is not CLI-fixable)
    assert!(
        !stdout.contains("fixable"),
        "MD033 should NOT be counted as fixable. Got:\n{stdout}"
    );

    // Run rumdl fmt - should report 0 fixes
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should NOT report any fixes
    assert!(!stdout.contains("Fixed"), "MD033 should NOT be fixed. Got:\n{stdout}");

    // File content should be unchanged
    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("<b>inline HTML</b>"),
        "File should not be modified. Got:\n{content}"
    );
}

/// Test that capability-based fix counting works correctly with mixed rule types
/// Tests that only truly CLI-fixable rules are counted as fixable
#[test]
fn test_capability_based_fixable_count() {
    let temp_dir = tempdir().unwrap();

    // Create file with both fixable (MD018) and unfixable (MD033) issues
    let file = temp_dir.path().join("test.md");
    fs::write(
        &file,
        r#"#Missing space after hash

This has <b>inline HTML</b> which triggers MD033.
"#,
    )
    .unwrap();

    // Run rumdl check
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should detect both issues
    assert!(
        stdout.contains("MD018") || stdout.contains("no-missing-space-atx"),
        "Should detect MD018 violation. Got:\n{stdout}"
    );
    assert!(
        stdout.contains("MD033") || stdout.contains("no-inline-html"),
        "Should detect MD033 violation. Got:\n{stdout}"
    );

    // Should show 1 fixable (MD018 only, not MD033)
    // Output format: "Run `rumdl fmt` to automatically fix 1 of the 2 issues"
    assert!(
        stdout.contains("fix 1 of the 2 issues") || stdout.contains("1 fixable"),
        "Should report 1 fixable issue (MD018 only). Got:\n{stdout}"
    );

    // Run rumdl fmt
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "."])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should report 1 fix (MD018 only)
    // Output format: "Fixed 1/2 issues in 1 file"
    assert!(
        stdout.contains("Fixed 1/2 issues") || stdout.contains("Fixed 1 issue"),
        "Should report 1 issue fixed. Got:\n{stdout}"
    );

    // Verify MD018 was fixed but HTML remains
    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("# Missing space"),
        "MD018 should be fixed. Got:\n{content}"
    );
    assert!(
        content.contains("<b>inline HTML</b>"),
        "HTML should remain unchanged. Got:\n{content}"
    );
}

// =============================================================================
// Shell completions tests
// =============================================================================

#[test]
fn test_completions_list_shells() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "--list"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(stdout.contains("bash"), "Should list bash");
    assert!(stdout.contains("zsh"), "Should list zsh");
    assert!(stdout.contains("fish"), "Should list fish");
    assert!(stdout.contains("powershell"), "Should list powershell");
    assert!(stdout.contains("elvish"), "Should list elvish");
}

#[test]
fn test_completions_bash_generates_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "bash"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(stdout.contains("_rumdl()"), "Should generate bash completion function");
    assert!(stdout.contains("COMPREPLY"), "Should use COMPREPLY for completions");
}

#[test]
fn test_completions_zsh_generates_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "zsh"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(stdout.contains("#compdef rumdl"), "Should have zsh compdef directive");
    assert!(stdout.contains("_rumdl()"), "Should generate zsh completion function");
}

#[test]
fn test_completions_fish_generates_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "fish"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("complete -c rumdl"),
        "Should generate fish complete commands"
    );
}

#[test]
fn test_completions_powershell_generates_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "powershell"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("Register-ArgumentCompleter"),
        "Should register argument completer"
    );
}

#[test]
fn test_completions_elvish_generates_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "elvish"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("set edit:completion:arg-completer[rumdl]"),
        "Should set elvish completion handler"
    );
}

#[test]
fn test_completions_auto_detect_from_shell_env() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions"])
        .env("SHELL", "/bin/bash")
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed with SHELL=bash");
    assert!(
        stdout.contains("_rumdl()"),
        "Should auto-detect bash and generate bash completions"
    );
}

#[test]
fn test_completions_unknown_shell_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions"])
        .env("SHELL", "/bin/unknown")
        .output()
        .expect("Failed to execute rumdl");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Command should fail with unknown shell");
    assert!(
        stderr.contains("Could not detect shell"),
        "Should show helpful error message"
    );
    assert!(
        stderr.contains("rumdl completions bash"),
        "Should suggest explicit shell argument"
    );
}

#[test]
fn test_completions_short_list_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "-l"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Command should succeed with -l flag");
    assert!(stdout.contains("bash"), "Should list shells with short flag");
}

#[test]
fn test_completions_clean_piping_stdout_only_has_script() {
    // Verify stdout contains only the script (no extra output)
    // This ensures `eval "$(rumdl completions zsh)"` works cleanly
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["completions", "zsh"])
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should have the script only
    assert!(
        stdout.contains("#compdef rumdl"),
        "stdout should contain the zsh script"
    );
    assert!(
        !stdout.contains("Installation"),
        "stdout should NOT contain installation instructions"
    );

    // stderr should be empty (no noise for eval usage)
    assert!(stderr.is_empty(), "stderr should be empty for clean eval usage");
}
