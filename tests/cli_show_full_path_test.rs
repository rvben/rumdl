//! Tests for the --show-full-path CLI flag
//!
//! This flag controls whether file paths in output are absolute or relative.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Create a test directory with a markdown file that has linting issues
fn create_test_structure() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    // Create a file with MD012 violations (multiple blank lines)
    let content = "# Test\n\n\n\nContent here";
    fs::write(docs_dir.join("guide.md"), content).expect("Failed to write test file");

    // Create a config file to set project root
    fs::write(temp_dir.path().join(".rumdl.toml"), "").expect("Failed to write config");

    temp_dir
}

#[test]
fn test_default_shows_relative_paths() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should show relative path
    assert!(
        combined.contains("docs/guide.md:"),
        "Expected relative path 'docs/guide.md', got:\n{combined}"
    );

    // Should NOT show absolute path
    assert!(
        !combined.contains(temp_dir.path().to_str().unwrap()),
        "Should not contain absolute path prefix:\n{combined}"
    );
}

#[test]
fn test_show_full_path_flag_shows_absolute_paths() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");
    let canonical_path = file_path.canonicalize().expect("Failed to canonicalize");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--show-full-path")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should show absolute/canonical path
    let expected_path = format!("{}:", canonical_path.display());
    assert!(
        combined.contains(&expected_path),
        "Expected absolute path '{}' in output:\n{combined}",
        canonical_path.display()
    );
}

#[test]
fn test_relative_paths_work_without_config_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    // Create a file with violations but NO config file
    let content = "# Test\n\n\n\nContent";
    fs::write(docs_dir.join("test.md"), content).expect("Failed to write");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("docs/test.md")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should show CWD-relative path
    assert!(
        combined.contains("docs/test.md:"),
        "Expected relative path 'docs/test.md' in output:\n{combined}"
    );
}

#[test]
fn test_relative_paths_with_nested_directories() {
    let temp_dir = create_test_structure();
    let nested_dir = temp_dir.path().join("a/b/c");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested dirs");

    let content = "# Deep\n\n\n\nNested content";
    fs::write(nested_dir.join("deep.md"), content).expect("Failed to write");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("a/b/c/deep.md")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("a/b/c/deep.md:"),
        "Expected nested relative path in output:\n{combined}"
    );
}

#[test]
fn test_show_full_path_with_different_output_formats() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");
    let canonical_path = file_path.canonicalize().expect("Failed to canonicalize");

    // Test with JSON format
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--show-full-path")
        .arg("--output-format")
        .arg("json")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // JSON should contain the full path
    assert!(
        stdout.contains(&canonical_path.to_string_lossy().to_string()),
        "JSON output should contain absolute path:\n{stdout}"
    );
}

#[test]
fn test_relative_paths_with_spaces_in_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let spaced_dir = temp_dir.path().join("my docs/sub folder");
    fs::create_dir_all(&spaced_dir).expect("Failed to create dir with spaces");

    let content = "# Spaced\n\n\n\nContent";
    fs::write(spaced_dir.join("my file.md"), content).expect("Failed to write");
    fs::write(temp_dir.path().join(".rumdl.toml"), "").expect("Failed to write config");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("my docs/sub folder/my file.md")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("my docs/sub folder/my file.md:"),
        "Expected path with spaces in output:\n{combined}"
    );
}

#[test]
fn test_help_shows_show_full_path_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--help")
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--show-full-path"),
        "Help should mention --show-full-path flag:\n{stdout}"
    );
    assert!(
        stdout.contains("absolute") || stdout.contains("relative"),
        "Help should describe path behavior:\n{stdout}"
    );
}

#[test]
fn test_relative_paths_in_sarif_format() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--output-format")
        .arg("sarif")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // SARIF uses artifactLocation.uri which should have relative path
    assert!(
        stdout.contains("docs/guide.md"),
        "SARIF output should contain relative path:\n{stdout}"
    );
    // Should not contain the temp dir absolute path
    assert!(
        !stdout.contains(temp_dir.path().to_str().unwrap()),
        "SARIF should not contain absolute path prefix:\n{stdout}"
    );
}

#[test]
fn test_relative_paths_in_github_format() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--output-format")
        .arg("github")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // GitHub format uses ::warning file=path pattern
    assert!(
        combined.contains("file=docs/guide.md"),
        "GitHub format should use relative path:\n{combined}"
    );
}

#[test]
fn test_show_full_path_in_sarif_format() {
    let temp_dir = create_test_structure();
    let file_path = temp_dir.path().join("docs/guide.md");
    let canonical_path = file_path.canonicalize().expect("Failed to canonicalize");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--show-full-path")
        .arg("--output-format")
        .arg("sarif")
        .arg(&file_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // SARIF should contain the full canonical path
    assert!(
        stdout.contains(&canonical_path.to_string_lossy().to_string()),
        "SARIF with --show-full-path should contain absolute path:\n{stdout}"
    );
}

#[test]
fn test_fix_mode_shows_relative_paths() {
    let temp_dir = create_test_structure();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-cache")
        .arg("docs/guide.md")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Fix mode output should show relative path
    if combined.contains("guide.md") {
        assert!(
            combined.contains("docs/guide.md"),
            "Fix mode should show relative path:\n{combined}"
        );
        assert!(
            !combined.contains(temp_dir.path().to_str().unwrap()),
            "Fix mode should not show absolute path prefix:\n{combined}"
        );
    }
}

#[test]
fn test_stdin_uses_dash_as_path() {
    // When reading from stdin, the path should be "-"
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn rumdl");

    // Write content with violations to stdin
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(b"# Test\n\n\n\nContent")
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // stdin should show as "-" in output
    assert!(
        combined.contains("-:") || combined.contains("<stdin>"),
        "Stdin input should show as '-' or '<stdin>' in output:\n{combined}"
    );
}
