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

    // docs/guide.md is excluded and must not be processed. By default (no
    // --verbose) the exclusion is silent: no per-file notice floods the output
    // (issue #686 - pre-commit passes excluded files on every run).
    assert!(
        !stderr.contains("ignored because of exclude pattern"),
        "Default run should not emit a per-file exclusion notice. stderr:\n{stderr}"
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

    // Test 1b: the same run with --verbose DOES surface the exclusion notice,
    // so users debugging "why wasn't my file linted?" can still find out.
    let output_v = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--verbose")
        .arg(root_file.to_str().unwrap())
        .arg(docs_file.to_str().unwrap())
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");
    let stderr_v = String::from_utf8_lossy(&output_v.stderr);
    assert!(
        stderr_v.contains("docs/guide.md") && stderr_v.contains("ignored because of exclude pattern"),
        "--verbose should surface the exclusion notice. stderr:\n{stderr_v}"
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

    // File is excluded; by default (no --verbose) no per-file exclusion notice
    // is printed. Assert specifically on the notice text, not a generic
    // "warning:" token - the deprecated force_exclude path emits its own,
    // unrelated warning line that is not affected by this change.
    assert!(
        !stderr.contains("ignored because of exclude pattern"),
        "Default run should not emit a per-file exclusion notice. stderr: {stderr}"
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

    // tests/test.md is excluded by the CLI --exclude override, but the default
    // run (no --verbose) stays quiet about it.
    assert!(
        !stderr.contains("ignored because of exclude pattern"),
        "Default run should not emit a per-file exclusion notice. stderr: {stderr}"
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

/// Issue #686 regression: a pre-commit-style invocation that passes a
/// deliberately-excluded file alongside a normal one must not flood stderr with
/// a per-file exclusion notice, while still linting the non-excluded file.
#[test]
fn test_excluded_file_notice_suppressed_by_default() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Mirror the reporter's setup: an auto-generated file excluded via config.
    fs::write(base_path.join("CHANGELOG.md"), "Auto-generated, no heading.\n").unwrap();
    fs::write(base_path.join("README.md"), "Some content without heading.\n").unwrap();
    fs::write(
        base_path.join("pyproject.toml"),
        "[tool.rumdl]\nexclude = [\"CHANGELOG.md\"]\n",
    )
    .unwrap();

    // pre-commit passes the full staged-file list explicitly.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("CHANGELOG.md")
        .arg("README.md")
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("ignored because of exclude pattern"),
        "Excluded file must not emit a per-file notice by default. stderr:\n{stderr}"
    );
    // The non-excluded file is still linted (its MD041 violation is reported).
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("README.md"),
        "README.md should still be linted. Output:\n{combined}"
    );
    assert!(
        !combined.contains("CHANGELOG.md"),
        "CHANGELOG.md should be silently excluded. Output:\n{combined}"
    );
}

/// `rumdl fmt` routes through the same discovery path as `check`
/// (`FmtArgs -> CheckArgs`), so the exclusion notice must be verbose-only there
/// too: silent by default, surfaced under --verbose.
#[test]
fn test_fmt_excluded_file_notice_verbose_only() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("CHANGELOG.md"), "# Changelog\n\nEntry.\n").unwrap();
    fs::write(
        base_path.join("pyproject.toml"),
        "[tool.rumdl]\nexclude = [\"CHANGELOG.md\"]\n",
    )
    .unwrap();

    let run = |extra: &[&str]| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
        cmd.arg("fmt").arg("--no-cache");
        for a in extra {
            cmd.arg(a);
        }
        cmd.arg("CHANGELOG.md")
            .current_dir(base_path)
            .output()
            .expect("Failed to execute rumdl")
    };

    // Default fmt: silent about the exclusion.
    let default_stderr = String::from_utf8_lossy(&run(&[]).stderr).into_owned();
    assert!(
        !default_stderr.contains("ignored because of exclude pattern"),
        "fmt should not emit a per-file exclusion notice by default. stderr:\n{default_stderr}"
    );

    // fmt --verbose: surfaces the exclusion notice.
    let verbose_stderr = String::from_utf8_lossy(&run(&["--verbose"]).stderr).into_owned();
    assert!(
        verbose_stderr.contains("CHANGELOG.md") && verbose_stderr.contains("ignored because of exclude pattern"),
        "fmt --verbose should surface the exclusion notice. stderr:\n{verbose_stderr}"
    );
}

/// JSON output is on stdout; a stray per-file notice on stderr is noise for
/// consumers. By default it must not appear.
#[test]
fn test_excluded_file_notice_absent_in_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("CHANGELOG.md"), "no heading\n").unwrap();
    fs::write(
        base_path.join("pyproject.toml"),
        "[tool.rumdl]\nexclude = [\"CHANGELOG.md\"]\n",
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg("--output-format")
        .arg("json")
        .arg("CHANGELOG.md")
        .current_dir(base_path)
        .output()
        .expect("Failed to execute rumdl");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("ignored because of exclude pattern"),
        "JSON run should not emit a per-file exclusion notice on stderr. stderr:\n{stderr}"
    );
}
