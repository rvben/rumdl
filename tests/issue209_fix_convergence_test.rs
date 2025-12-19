// Test for Issue #209: Fix should converge in a single pass for mixed ordered/unordered lists
//
// The issue was that MD007 and MD005 would oscillate when fixing mixed lists:
// 1. MD007 fixed bullet indentation but skipped ordered items
// 2. MD005 then saw inconsistent indentation and "fixed" it
// 3. This changed the structure, triggering MD007 again
// 4. Required 4 passes to converge
//
// Root cause: MD007 auto-switched to "fixed" style when indent was explicitly set,
// but this created conflicts with MD005's consistency checks.
//
// Fix: Removed the auto-switch to fixed style. Text-aligned style (default)
// correctly handles mixed ordered/unordered lists.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Test the exact scenario from issue #209
/// Mixed ordered/unordered list with indent=3 should converge in one pass
#[test]
fn test_issue209_mixed_list_single_pass_convergence() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let config_file = temp_dir.path().join(".rumdl.toml");

    // Exact content from issue #209
    let content = r#"# Header 1

- **First item**:
  - First subitem
  - Second subitem
  - Third subitem
- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
       - Second subpoint
       - Third subpoint
    2. **Second point**
       - First subpoint
       - Second subpoint
       - Third subpoint
"#;

    // Config from issue #209
    let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
"#;

    fs::write(&test_file, content).unwrap();
    fs::write(&config_file, config).unwrap();

    // Run fmt once
    let output1 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);

    // Run fmt a second time - should find no issues (convergence)
    let output2 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // The second run should show "No issues found" - single pass convergence
    assert!(
        stdout2.contains("No issues found"),
        "Fix should converge in single pass.\n\
         First run output:\n{stdout1}\n\
         Second run output:\n{stdout2}"
    );
}

/// Test that check --fix also converges in one pass
#[test]
fn test_issue209_check_fix_single_pass() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let config_file = temp_dir.path().join(".rumdl.toml");

    let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
    2. **Second point**
       - First subpoint
"#;

    let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
"#;

    fs::write(&test_file, content).unwrap();
    fs::write(&config_file, config).unwrap();

    // Run check --fix
    let output1 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    // Run check (no fix) - should find no issues
    let output2 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let exit_code = output2.status.code().unwrap_or(-1);

    assert!(
        stdout2.contains("No issues found") && exit_code == 0,
        "After check --fix, no issues should remain.\n\
         First run: {:?}\n\
         Second run stdout: {stdout2}\n\
         Exit code: {exit_code}",
        String::from_utf8_lossy(&output1.stdout)
    );
}

/// Test that explicit style=text-aligned works correctly
#[test]
fn test_issue209_explicit_text_aligned_no_issues() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let config_file = temp_dir.path().join(".rumdl.toml");

    // This content should have NO issues with text-aligned style
    let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
"#;

    let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
style = "text-aligned"
"#;

    fs::write(&test_file, content).unwrap();
    fs::write(&config_file, config).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output.status.code().unwrap_or(-1);

    assert!(
        stdout.contains("No issues found") && exit_code == 0,
        "With explicit text-aligned style, mixed lists should have no issues.\n\
         stdout: {stdout}\n\
         exit code: {exit_code}"
    );
}

/// Test that without explicit style, text-aligned is used (default)
/// This is the key behavioral change - we no longer auto-switch to fixed
#[test]
fn test_issue209_default_style_is_text_aligned() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let config_file = temp_dir.path().join(".rumdl.toml");

    // Content matching the exact issue 209 scenario - this should have no issues
    // with text-aligned style (default) but would oscillate with fixed style
    let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
"#;

    // indent=3 but NO explicit style - should default to text-aligned
    // Previously this would auto-switch to fixed style and cause oscillation
    let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
"#;

    fs::write(&test_file, content).unwrap();
    fs::write(&config_file, config).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output.status.code().unwrap_or(-1);

    // With text-aligned (default), this structure should be valid
    // With the old auto-switch to fixed, MD007 would flag the sub-bullets
    // expecting 9 spaces instead of 7
    assert!(
        stdout.contains("No issues found") && exit_code == 0,
        "Default style should be text-aligned, not auto-switching to fixed.\n\
         stdout: {stdout}\n\
         exit code: {exit_code}\n\
         (If this fails, the auto-switch to fixed style may still be active)"
    );
}
