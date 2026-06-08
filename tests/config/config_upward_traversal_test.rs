use std::fs;
// Only the Unix-gated symlink test below uses this.
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_config_upward_traversal() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("subdir").join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create config at project root
    let config_content = r#"
[global]
line-length = 120
disable = ["MD013", "MD033"]
"#;
    let config_path = project_dir.join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters but should not trigger MD013 due to parent config.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();

    // MD013 should be disabled by parent config
    assert!(!stderr.contains("MD013"), "MD013 should be disabled by parent config");
    assert!(!stderr.contains("Line length"), "Line length warning should not appear");
}

#[test]
fn test_config_stops_at_git_boundary() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure with .git in middle
    let subdir = project_dir.join("subdir");
    let nested_dir = subdir.join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .git directory in subdir (boundary)
    fs::create_dir(subdir.join(".git")).unwrap();

    // Create config at project root (should not be found)
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 because config is not found.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger because config is not found (stopped at .git)
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger because traversal stopped at .git boundary. Output: {combined}"
    );
}

#[test]
fn test_isolated_flag_ignores_all_configs() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create config that disables MD013
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 when using --isolated flag.\n").unwrap();

    // Run with --isolated flag
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md", "--isolated"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger despite config because --isolated is used
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger with --isolated flag. Output: {combined}"
    );
}

#[test]
fn test_config_precedence_order() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create both pyproject.toml and .rumdl.toml
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    fs::write(project_dir.join("pyproject.toml"), pyproject_content).unwrap();

    let rumdl_content = r#"
[global]
line-length = 100
"#;
    fs::write(project_dir.join(".rumdl.toml"), rumdl_content).unwrap();

    // Check which config is loaded
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();

    // .rumdl.toml should take precedence
    assert!(
        stdout.contains(".rumdl.toml"),
        ".rumdl.toml should take precedence over pyproject.toml"
    );
    assert!(
        !stdout.contains("pyproject.toml"),
        "pyproject.toml should not be loaded when .rumdl.toml exists"
    );
}

#[test]
#[cfg(unix)]
fn test_symlinked_config_is_followed() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create a real config file
    let real_config_content = r#"
[global]
disable = ["MD013", "MD033"]
"#;
    let real_config_path = project_dir.join("real-config.toml");
    fs::write(&real_config_path, real_config_content).unwrap();

    // Create a symlink to it
    let symlink_path = project_dir.join(".rumdl.toml");
    unix_fs::symlink(&real_config_path, &symlink_path).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and MD013 should be disabled by symlinked config.\n").unwrap();

    // Run rumdl - should follow the symlink
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should be disabled by symlinked config (following Ruff's behavior)
    assert!(
        !combined.contains("MD013"),
        "MD013 should be disabled by symlinked config"
    );
    assert!(
        !combined.contains("Line length"),
        "Line length warning should not appear"
    );
}

#[test]
fn test_markdownlint_yaml_upward_traversal() {
    // Issue #193: .markdownlint.yaml should be discovered via upward traversal
    // just like .rumdl.toml
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("path").join("to");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .markdownlint.yaml at project root (not in nested dir)
    let config_content = r#"
MD013:
  line_length: 200
  code_blocks: false
"#;
    fs::write(project_dir.join(".markdownlint.yaml"), config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("file.md");
    fs::write(
        &test_file,
        "# Test\n\nThis is a line that is about 100 characters long and should not trigger MD013 due to parent config setting line_length to 200.\n",
    )
    .unwrap();

    // Run rumdl from nested directory, checking the file
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "file.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should NOT trigger because line_length=200 from parent config
    assert!(
        !combined.contains("MD013"),
        "MD013 should not trigger - .markdownlint.yaml at repo root should be discovered. Output: {combined}"
    );
}

#[test]
fn test_multi_path_global_config_not_seeded_from_first_path() {
    // Regression: with multiple paths spanning several config scopes, the
    // *global* config must be discovered from the project root (cwd), not from
    // the first path's directory. Otherwise whichever file sorts first decides
    // the baseline for every file, so a nested `extend-disable` silently
    // disables that rule for files in other directories that inherit the root
    // config (e.g. `rumdl check .claude/a.md d/b.md` would drop MD013 on d/b.md
    // because `.claude/.rumdl.toml` extend-disables it).
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // `.git` marks the project root so config discovery treats `project_dir`
    // (not `dir_a`) as the project root for the inheriting files.
    fs::create_dir(project_dir.join(".git")).unwrap();

    // Project-root config: MD013 enabled.
    fs::write(project_dir.join(".rumdl.toml"), "[MD013]\nline-length = 120\n").unwrap();

    // dir_a: nested config that extend-disables MD013.
    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nextend-disable = [\"MD013\"]\n",
    )
    .unwrap();
    fs::write(dir_a.join("a.md"), "# A\n\nshort line.\n").unwrap();

    // dir_b: no own config, inherits the root config, so its long line must fire MD013.
    let dir_b = project_dir.join("dir_b");
    fs::create_dir_all(&dir_b).unwrap();
    let long_line = "This is a deliberately long line in dir_b which inherits the root config and clearly exceeds one hundred twenty characters so MD013 must fire.";
    fs::write(dir_b.join("b.md"), format!("# B\n\n{long_line}\n")).unwrap();

    // Pass the dir_a file FIRST: a regression seeds the global config from
    // dir_a (MD013 disabled) and wrongly suppresses MD013 for dir_b/b.md.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_a/a.md", "dir_b/b.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // Only dir_b/b.md can produce MD013 (dir_a disables it and a.md is short),
    // so the rule firing at all proves dir_b kept the root config's MD013.
    assert!(
        combined.contains("MD013"),
        "MD013 must fire for dir_b/b.md (inherits root config) even though the \
         first path is in dir_a whose config extend-disables MD013. Output: {combined}"
    );
}
