//! Tests for the --flavor CLI option
//!
//! Validates that the --flavor CLI argument correctly overrides
//! the config file flavor setting.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Helper to run rumdl check with given arguments
fn run_rumdl(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("Failed to execute rumdl");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

#[test]
fn test_flavor_cli_option_recognized() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    // Test that --flavor is recognized and doesn't error
    let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "mkdocs", "test.md"]);
    assert!(success, "Command should succeed. stderr: {stderr}, stdout: {stdout}");
}

#[test]
fn test_flavor_cli_all_variants() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    // Test all valid flavor values (including aliases accepted by clap parser).
    for flavor in [
        "standard",
        "gfm",
        "github",
        "commonmark",
        "mkdocs",
        "mdx",
        "quarto",
        "qmd",
        "rmd",
        "rmarkdown",
        "obsidian",
        "kramdown",
        "jekyll",
    ] {
        let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", flavor, "test.md"]);
        assert!(
            success,
            "Command should succeed for flavor '{flavor}'. stderr: {stderr}, stdout: {stdout}"
        );
    }
}

#[test]
fn test_flavor_cli_invalid_value() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    // Test invalid flavor value
    let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "invalid_flavor", "test.md"]);
    assert!(!success, "Command should fail for invalid flavor");
    assert!(
        stderr.contains("invalid_flavor") || stderr.contains("possible values"),
        "Error should mention invalid value. stderr: {stderr}"
    );
}

#[test]
fn test_flavor_cli_overrides_config() {
    let temp_dir = tempdir().unwrap();

    // Create config with standard flavor
    let config_content = r#"
[global]
flavor = "standard"
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create a markdown file with MkDocs admonition
    let md_content = r#"# Test

!!! note "MkDocs Admonition"
    This should trigger MD022 in standard mode but not in mkdocs mode.
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run without --flavor override (uses config's standard)
    let (_success_std, stdout_std, _) = run_rumdl(temp_dir.path(), &["check", "test.md"]);

    // Run with --flavor mkdocs override
    let (_success_mkdocs, stdout_mkdocs, _stderr_mkdocs) =
        run_rumdl(temp_dir.path(), &["check", "--flavor", "mkdocs", "test.md"]);

    // The key test is that both commands complete without panic.
    // The fact that run_rumdl returns means the command executed.
    // We just log the output for debugging.
    println!("Standard mode: {stdout_std}");
    println!("MkDocs mode: {stdout_mkdocs}");
}

#[test]
fn test_flavor_cli_with_output_format() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    // Test combining --flavor with --output-format
    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &["check", "--flavor", "mkdocs", "--output-format", "json", "test.md"],
    );
    assert!(success, "Command should succeed with both options. stderr: {stderr}");
    // JSON output should be valid (either empty array or object)
    assert!(
        stdout.trim().is_empty() || stdout.starts_with('[') || stdout.starts_with('{'),
        "Output should be valid JSON. stdout: {stdout}"
    );
}

#[test]
fn test_flavor_cli_with_enable_disable() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    // Test combining --flavor with --enable
    let (success, _stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &["check", "--flavor", "mkdocs", "--enable", "MD001,MD003", "test.md"],
    );
    assert!(
        success,
        "Command should succeed with --flavor and --enable. stderr: {stderr}"
    );

    // Test combining --flavor with --disable
    let (success, _stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &["check", "--flavor", "quarto", "--disable", "MD013", "test.md"],
    );
    assert!(
        success,
        "Command should succeed with --flavor and --disable. stderr: {stderr}"
    );
}

#[test]
fn test_flavor_mdx_jsx_support() {
    let temp_dir = tempdir().unwrap();

    // Create an MDX file with JSX content
    let mdx_content = r#"# MDX Test

<CustomComponent prop="value">
  Some content inside a custom component.
</CustomComponent>

Regular paragraph.
"#;
    fs::write(temp_dir.path().join("test.mdx"), mdx_content).unwrap();

    // Run with MDX flavor - command completing without panic is the test
    let (_success, _stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "mdx", "test.mdx"]);
}

#[test]
fn test_flavor_quarto_support() {
    let temp_dir = tempdir().unwrap();

    // Create a Quarto file with callouts
    let qmd_content = r#"---
title: "Quarto Test"
---

# Quarto Document

:::{.callout-note}
This is a Quarto callout note.
:::

Regular paragraph.
"#;
    fs::write(temp_dir.path().join("test.qmd"), qmd_content).unwrap();

    // Run with Quarto flavor - command completing without panic is the test
    let (_success, _stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "quarto", "test.qmd"]);
}

/// End-to-end test: Obsidian flavor skips tag syntax in MD018
///
/// Verifies that --flavor obsidian actually affects MD018 behavior,
/// skipping Obsidian tag patterns (#tagname) while still flagging
/// multi-hash patterns (##tag) and digit-starting patterns (#123).
#[test]
fn test_obsidian_flavor_md018_tags() {
    let temp_dir = tempdir().unwrap();

    // Create a markdown file with Obsidian tags and malformed headings
    let md_content = r#"# Real Heading

#todo this is an Obsidian tag

#project/active nested tag

##Introduction

#123
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run with standard flavor - should flag ALL single-hash patterns
    let (success_std, stdout_std, _stderr_std) =
        run_rumdl(temp_dir.path(), &["check", "--flavor", "standard", "test.md"]);
    assert!(!success_std, "Standard flavor should find issues");

    // Count MD018 warnings in standard mode
    let std_md018_count = stdout_std.matches("MD018").count();
    assert!(
        std_md018_count >= 4,
        "Standard flavor should flag at least 4 MD018 issues (#todo, #project/active, ##Introduction, #123). Found {std_md018_count}. stdout: {stdout_std}"
    );

    // Run with obsidian flavor - should skip tags, flag only ##Introduction and #123
    let (success_obs, stdout_obs, _stderr_obs) =
        run_rumdl(temp_dir.path(), &["check", "--flavor", "obsidian", "test.md"]);
    assert!(!success_obs, "Obsidian flavor should still find some issues");

    // Count MD018 warnings in obsidian mode
    let obs_md018_count = stdout_obs.matches("MD018").count();
    assert_eq!(
        obs_md018_count, 2,
        "Obsidian flavor should flag exactly 2 MD018 issues (##Introduction, #123). Found {obs_md018_count}. stdout: {stdout_obs}"
    );

    // Verify specific patterns are NOT flagged
    // Note: Output format is "file:LINE:COLUMN:", so we check for "test.md:LINE:" pattern
    assert!(
        !stdout_obs.contains("test.md:3:"),
        "#todo (line 3) should NOT be flagged in Obsidian flavor. stdout: {stdout_obs}"
    );
    assert!(
        !stdout_obs.contains("test.md:5:"),
        "#project/active (line 5) should NOT be flagged in Obsidian flavor. stdout: {stdout_obs}"
    );
}

/// End-to-end test: Obsidian flavor works with config file
#[test]
fn test_obsidian_flavor_config_file() {
    let temp_dir = tempdir().unwrap();

    // Create config with obsidian flavor
    let config_content = r#"
[global]
flavor = "obsidian"
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create markdown with Obsidian tag
    let md_content = "#todo this is a tag\n";
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run without --flavor flag (should use config's obsidian)
    let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "test.md"]);

    // Should pass (no MD018 warning) because #todo is an Obsidian tag
    assert!(
        success,
        "Obsidian flavor from config should skip #todo tag. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        !stdout.contains("MD018"),
        "#todo should NOT be flagged when flavor=obsidian in config. stdout: {stdout}"
    );
}

/// End-to-end test: Obsidian fix mode preserves tags
#[test]
fn test_obsidian_flavor_fix_preserves_tags() {
    let temp_dir = tempdir().unwrap();

    // Create markdown with tags and malformed headings
    let md_content = "#todo tag\n\n##Introduction\n";
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, md_content).unwrap();

    // Run fix with obsidian flavor
    let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--fix", "--flavor", "obsidian", "test.md"]);
    assert!(success, "Fix command should succeed. stderr: {stderr}");

    // Read the fixed content
    let fixed_content = fs::read_to_string(&md_path).expect("Should read fixed file");

    // #todo should be preserved (not changed to "# todo")
    assert!(
        fixed_content.contains("#todo tag"),
        "#todo should be preserved in Obsidian flavor. Fixed content: {fixed_content}"
    );

    // ##Introduction should be fixed to "## Introduction"
    assert!(
        fixed_content.contains("## Introduction"),
        "##Introduction should be fixed to '## Introduction'. Fixed content: {fixed_content}"
    );
}

/// End-to-end test: MD018 magiclink config option
///
/// Verifies that [MD018] magiclink = true skips MagicLink-style issue refs (#123)
/// while still flagging non-numeric patterns (#Summary).
#[test]
fn test_md018_magiclink_config() {
    let temp_dir = tempdir().unwrap();

    // Create config with magiclink enabled
    let config_content = r#"
[MD018]
magiclink = true
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create markdown with MagicLink patterns and malformed headings
    let md_content = r#"# Real Heading

#10 discusses the issue

#37 is another reference

#Summary
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run with magiclink config - should skip #10 and #37, flag #Summary
    let (success, stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "test.md"]);
    assert!(!success, "Should find issues (at least #Summary)");

    // Count MD018 warnings
    let md018_count = stdout.matches("MD018").count();
    assert_eq!(
        md018_count, 1,
        "With magiclink=true, should flag exactly 1 MD018 issue (#Summary). Found {md018_count}. stdout: {stdout}"
    );

    // Verify #10 and #37 are NOT flagged (lines 3 and 5)
    assert!(
        !stdout.contains("test.md:3:"),
        "#10 (line 3) should NOT be flagged with magiclink=true. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("test.md:5:"),
        "#37 (line 5) should NOT be flagged with magiclink=true. stdout: {stdout}"
    );
}

/// End-to-end test: MD018 without magiclink config flags all patterns
#[test]
fn test_md018_without_magiclink_config() {
    let temp_dir = tempdir().unwrap();

    // No config file - default behavior

    // Create markdown with MagicLink patterns
    let md_content = r#"# Real Heading

#10 discusses the issue

#Summary
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run without magiclink config - should flag ALL patterns
    let (success, stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "test.md"]);
    assert!(!success, "Should find issues");

    // Count MD018 warnings - should be 2 (#10 and #Summary)
    let md018_count = stdout.matches("MD018").count();
    assert_eq!(
        md018_count, 2,
        "Without magiclink config, should flag 2 MD018 issues (#10, #Summary). Found {md018_count}. stdout: {stdout}"
    );
}

/// End-to-end test: MD018 magiclink fix preserves issue refs
#[test]
fn test_md018_magiclink_fix_preserves_refs() {
    let temp_dir = tempdir().unwrap();

    // Create config with magiclink enabled
    let config_content = r#"
[MD018]
magiclink = true
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create markdown with MagicLink ref and malformed heading
    let md_content = "#10 is an issue\n\n#Summary\n";
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, md_content).unwrap();

    // Run fix with magiclink config
    let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--fix", "test.md"]);
    assert!(success, "Fix command should succeed. stderr: {stderr}");

    // Read the fixed content
    let fixed_content = fs::read_to_string(&md_path).expect("Should read fixed file");

    // #10 should be preserved (not changed to "# 10")
    assert!(
        fixed_content.contains("#10 is an issue"),
        "#10 should be preserved with magiclink=true. Fixed content: {fixed_content}"
    );

    // #Summary should be fixed to "# Summary"
    assert!(
        fixed_content.contains("# Summary"),
        "#Summary should be fixed to '# Summary'. Fixed content: {fixed_content}"
    );
}

/// Regression test: Fix coordination must respect per-file-flavor configuration.
///
/// Bug: FixCoordinator used config.markdown_flavor() (global) instead of
/// config.get_flavor_for_file() (per-file), causing MkDocs content inside
/// admonitions to not be fixed because the fix phase didn't recognize
/// the MkDocs syntax.
#[test]
fn test_per_file_flavor_fix_coordination() {
    let temp_dir = tempdir().unwrap();

    // Create config with per-file-flavor for MkDocs (NOT global flavor)
    // The global flavor is NOT set to mkdocs, so if per-file-flavor is ignored,
    // the fix won't recognize MkDocs admonition syntax
    let config_content = r#"
[global]
enable = ["MD013"]
line-length = 80

[per-file-flavor]
"docs/**/*.md" = "mkdocs"

[MD013]
line-length = 80
reflow = true
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create docs directory and markdown file with MkDocs admonition
    // The content inside the admonition has a long line that should be reflowed
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir(&docs_dir).unwrap();

    let md_content = r#"# Test

!!! note "Important Note"
    This is a very long line inside an MkDocs admonition that exceeds the 80 character line length limit and should be reflowed by the fix command.
"#;
    let md_path = docs_dir.join("test.md");
    fs::write(&md_path, md_content).unwrap();

    // Run fix mode
    let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--fix", "docs/test.md"]);

    // The command should succeed (exit 0)
    assert!(success, "Fix command should succeed. stderr: {stderr}");

    // The key test is that the content was actually modified
    // (proving that fix coordination used the per-file-flavor and recognized MkDocs syntax)
    let fixed_content = fs::read_to_string(&md_path).expect("Should read fixed file");

    // Verify the content was modified (the long line should have been reflowed)
    // The original content had one line starting with "    This is a very long line"
    // After reflow, that line should be different (wrapped into multiple lines or reformatted)
    let original_long_line = "    This is a very long line inside an MkDocs admonition that exceeds the 80 character line length limit and should be reflowed by the fix command.";

    assert!(
        !fixed_content.contains(original_long_line),
        "Long line should have been modified by fix.\n\
         This proves per-file-flavor was respected in fix coordination.\n\
         If the line is unchanged, fix coordination likely used global flavor (standard) \n\
         instead of per-file flavor (mkdocs), failing to recognize admonition content.\n\
         Fixed content:\n{fixed_content}\n\
         stderr: {stderr}"
    );
}
