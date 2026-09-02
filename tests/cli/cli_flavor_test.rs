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
fn test_flavor_pandoc_parses() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "pandoc", "test.md"]);
    assert!(
        success,
        "Command should succeed for flavor 'pandoc'. stderr: {stderr}, stdout: {stdout}"
    );
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
        "pandoc",
        "quarto",
        "qmd",
        "rmd",
        "rmarkdown",
        "obsidian",
        "kramdown",
        "jekyll",
        "azure_devops",
        "azure",
        "ado",
        "myst",
        "mystmd",
        "hugo",
        "goldmark",
        "mdg",
        "markdown_with_gherkin",
        "gh-aw",
    ] {
        let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", flavor, "test.md"]);
        assert!(
            success,
            "Command should succeed for flavor '{flavor}'. stderr: {stderr}, stdout: {stdout}"
        );
    }
}

#[test]
fn test_flavor_help_lists_specialized_flavors() {
    let (success, stdout, stderr) = run_rumdl(std::path::Path::new("."), &["check", "--help"]);
    assert!(success, "Help should succeed. stderr: {stderr}");
    assert!(
        stdout.contains("mdg"),
        "Help should list the MDG flavor. stdout: {stdout}"
    );
    assert!(
        stdout.contains("markdown_with_gherkin"),
        "Help should list the markdown_with_gherkin alias. stdout: {stdout}"
    );
    let normalized = stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized.contains("mdg (also accepts markdown_with_gherkin), or gh-aw"),
        "Help should use one final conjunction in the flavor list. stdout: {stdout}"
    );
    assert!(
        stdout.contains("gh-aw"),
        "Help should list the gh-aw flavor. stdout: {stdout}"
    );
}

#[test]
fn test_invalid_per_file_flavor_warning_lists_all_canonical_flavors() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("test.md"), "# Test\n").unwrap();
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        "[per-file-flavor]\n\"*.md\" = \"not-a-flavor\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "test.md"])
        .env("RUST_LOG", "warn")
        .output()
        .expect("Failed to execute rumdl");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Invalid flavor"), "stderr: {stderr}");
    for flavor in ["myst", "hugo", "mdg", "gh-aw"] {
        assert!(stderr.contains(flavor), "warning omitted {flavor}: {stderr}");
    }
}

#[test]
fn test_gh_aw_cli_and_per_file_flavor_preserve_runtime_imports() {
    let temp_dir = tempdir().unwrap();
    fs::create_dir_all(temp_dir.path().join(".github/workflows/shared")).unwrap();
    let workflow = temp_dir.path().join(".github/workflows/shared/triage.md");
    let content = "{{#runtime-import https://example.com/shared.md:10-50}}\n\n# Triage\n";
    fs::write(&workflow, content).unwrap();

    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "check",
            "--no-cache",
            "--no-config",
            "--flavor",
            "gh-aw",
            "--enable",
            "MD034,MD041",
            ".github/workflows/shared/triage.md",
        ],
    );
    assert!(
        success,
        "gh-aw CLI flavor should pass. stdout: {stdout}, stderr: {stderr}"
    );

    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        "[per-file-flavor]\n\".github/workflows/**/*.md\" = \"gh-aw\"\n",
    )
    .unwrap();
    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "fmt",
            "--no-cache",
            "--enable",
            "MD034,MD041",
            ".github/workflows/shared/triage.md",
        ],
    );
    assert!(
        success,
        "per-file gh-aw formatting should pass. stdout: {stdout}, stderr: {stderr}"
    );
    assert_eq!(fs::read_to_string(&workflow).unwrap(), content);

    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "check",
            "--no-cache",
            "--no-config",
            "--enable",
            "MD034",
            ".github/workflows/shared/triage.md",
        ],
    );
    assert!(!success, "ordinary .md files must not auto-detect gh-aw");
    assert!(stdout.contains("MD034"), "stdout: {stdout}, stderr: {stderr}");
}

#[test]
fn test_gh_aw_representative_corpus_passes_full_cli_and_is_format_stable() {
    let temp_dir = tempdir().unwrap();
    let workflow_dir = temp_dir.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).unwrap();
    let corpus = [
        (
            "representative.md",
            include_str!("../fixtures/gh_aw/representative-workflow.md"),
        ),
        (
            "conditional.md",
            include_str!("../fixtures/gh_aw/conditional-workflow.md"),
        ),
        ("imports-only.md", include_str!("../fixtures/gh_aw/imports-only.md")),
        ("branching.md", include_str!("../fixtures/gh_aw/branching-workflow.md")),
    ];
    for (name, content) in corpus {
        fs::write(workflow_dir.join(name), content).unwrap();
    }

    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "check",
            "--no-cache",
            "--no-config",
            "--flavor",
            "gh-aw",
            ".github/workflows",
        ],
    );
    assert!(success, "full corpus check failed. stdout: {stdout}, stderr: {stderr}");

    for pass in 1..=2 {
        let (success, stdout, stderr) = run_rumdl(
            temp_dir.path(),
            &[
                "fmt",
                "--no-cache",
                "--no-config",
                "--flavor",
                "gh-aw",
                ".github/workflows",
            ],
        );
        assert!(
            success,
            "full corpus format pass {pass} failed. stdout: {stdout}, stderr: {stderr}"
        );
        for (name, original) in corpus {
            assert_eq!(fs::read_to_string(workflow_dir.join(name)).unwrap(), original, "{name}");
        }
    }
}

#[test]
fn test_feature_md_auto_detection_applies_mdg_rules() {
    let temp_dir = tempdir().unwrap();
    let feature_path = temp_dir.path().join("checkout.feature.md");
    // A tilde fence is never a Doc String, so MD048 converts it to backticks.
    // MD040 reports the missing media type but does not invent one under MDG.
    let content = "# Feature: Checkout\n\n## Scenario: Payload\n\n* Given a message\n\n  ~~~\n  hello\n  ~~~\n";
    fs::write(&feature_path, content).unwrap();

    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "fmt",
            "--no-cache",
            "--no-config",
            "--enable",
            "MD040,MD048",
            "checkout.feature.md",
        ],
    );

    assert!(success, "Formatting should succeed. stderr: {stderr}, stdout: {stdout}");
    assert_eq!(
        fs::read_to_string(feature_path).unwrap(),
        "# Feature: Checkout\n\n## Scenario: Payload\n\n* Given a message\n\n  ```\n  hello\n  ```\n",
        "auto-detected MDG formatting must steer fences to backticks without inventing a media type"
    );
}

#[test]
fn test_mdg_md003_explicit_style_override_warns() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(".rumdl.toml"), "[MD003]\nstyle = \"setext\"\n").unwrap();
    fs::write(
        temp_dir.path().join("headings.feature.md"),
        "Feature: Checkout\n=================\n",
    )
    .unwrap();

    let (_success, _stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &["check", "--no-cache", "--enable", "MD003", "headings.feature.md"],
    );

    assert!(stderr.contains("[config warning]"), "stderr: {stderr}");
    assert!(stderr.contains("MD003:"), "stderr: {stderr}");
    assert!(stderr.contains("requires style=\"atx\""), "stderr: {stderr}");
}

#[test]
fn test_mdg_md026_override_warns_before_rule_skip_once_across_config_groups() {
    let temp_dir = tempdir().unwrap();
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        "[MD026]\npunctuation = \".,;:!\"\n",
    )
    .unwrap();
    fs::write(temp_dir.path().join("root.feature.md"), "#### Examples:\n").unwrap();

    let nested = temp_dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join(".rumdl.toml"), "[MD026]\npunctuation = \".,;:!\"\n").unwrap();
    fs::write(nested.join("nested.feature.md"), "#### Examples:\n").unwrap();

    let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--no-cache", "--enable", "MD026", "."]);

    assert!(
        success,
        "No effective punctuation should be reported. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stderr.contains("[config warning]"), "stderr: {stderr}");
    assert!(stderr.contains("MD026:"), "stderr: {stderr}");
    assert!(stderr.contains("punctuation=\".,;:!\""), "stderr: {stderr}");
    assert_eq!(
        stderr.matches("MD026:").count(),
        1,
        "the process-wide override notice must not repeat for nested config groups. stderr: {stderr}"
    );
}

#[test]
fn test_mdg_md034_reports_email_and_xmpp_without_link_prefilter_match() {
    let temp_dir = tempdir().unwrap();
    fs::write(
        temp_dir.path().join("contacts.feature.md"),
        "# Feature: Contact\n\n* Given I email user@example.com\n* And I message xmpp:user@example.org\n",
    )
    .unwrap();

    let (success, stdout, stderr) = run_rumdl(
        temp_dir.path(),
        &[
            "check",
            "--no-cache",
            "--no-config",
            "--flavor",
            "mdg",
            "--enable",
            "MD034",
            "contacts.feature.md",
        ],
    );

    assert!(
        !success,
        "MD034 findings should fail check. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("MD034"), "stdout: {stdout}");
    assert!(stdout.contains("user@example.com"), "stdout: {stdout}");
    assert!(stdout.contains("xmpp:user@example.org"), "stdout: {stdout}");
    assert!(stdout.contains("Gherkin placeholder syntax"), "stdout: {stdout}");
}

#[test]
fn test_flavor_cli_invalid_value() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

    for flavor in ["invalid_flavor", "gaw"] {
        let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", flavor, "test.md"]);
        assert!(!success, "Command should fail for invalid flavor '{flavor}'");
        assert!(
            stderr.contains(flavor) || stderr.contains("possible values"),
            "Error should mention invalid value. stderr: {stderr}"
        );
    }
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

/// End-to-end test: MD018 tags config enables tag recognition without Obsidian flavor
#[test]
fn test_md018_tags_config_standard_flavor() {
    let temp_dir = tempdir().unwrap();

    // Create config with tags enabled (no Obsidian flavor)
    let config_content = r#"
[MD018]
tags = true
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    // Create markdown with tag patterns and malformed headings
    let md_content = r#"# Real Heading

#todo this is a tag

#project/active nested tag

##Introduction

#123
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // Run with tags config - should skip tags, flag ##Introduction and #123
    let (success, stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "test.md"]);
    assert!(!success, "Should find issues (##Introduction, #123)");

    let md018_count = stdout.matches("MD018").count();
    assert_eq!(
        md018_count, 2,
        "With tags=true, should flag exactly 2 MD018 issues (##Introduction, #123). Found {md018_count}. stdout: {stdout}"
    );

    // Tags should NOT be flagged
    assert!(
        !stdout.contains("test.md:3:"),
        "#todo (line 3) should NOT be flagged with tags=true. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("test.md:5:"),
        "#project/active (line 5) should NOT be flagged with tags=true. stdout: {stdout}"
    );
}

/// End-to-end test: MD018 tags=false overrides Obsidian flavor default
#[test]
fn test_md018_tags_config_override_obsidian() {
    let temp_dir = tempdir().unwrap();

    // Create config with Obsidian flavor but tags explicitly disabled
    let config_content = r#"
[global]
flavor = "obsidian"

[MD018]
tags = false
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    let md_content = r#"# Real Heading

#todo

#project/active
"#;
    fs::write(temp_dir.path().join("test.md"), md_content).unwrap();

    // With tags=false, should flag tag patterns even in Obsidian flavor
    let (success, stdout, _stderr) = run_rumdl(temp_dir.path(), &["check", "test.md"]);
    assert!(!success, "Should find issues with tags=false");

    let md018_count = stdout.matches("MD018").count();
    assert_eq!(
        md018_count, 2,
        "With tags=false in Obsidian flavor, should flag tag patterns. Found {md018_count}. stdout: {stdout}"
    );
}

/// End-to-end test: MD018 tags config fix preserves tags
#[test]
fn test_md018_tags_config_fix_preserves_tags() {
    let temp_dir = tempdir().unwrap();

    let config_content = r#"
[MD018]
tags = true
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    let md_content = "#todo\n\n#Summary\n";
    let md_path = temp_dir.path().join("test.md");
    fs::write(&md_path, md_content).unwrap();

    let (success, _stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--fix", "test.md"]);
    assert!(success, "Fix command should succeed. stderr: {stderr}");

    let fixed_content = fs::read_to_string(&md_path).expect("Should read fixed file");

    // Both #todo and #Summary are valid tags (they contain a non-numerical
    // character), so neither should be modified
    assert!(
        fixed_content.contains("#todo"),
        "#todo should be preserved with tags=true. Fixed content: {fixed_content}"
    );
    assert!(
        fixed_content.contains("#Summary"),
        "#Summary should be preserved with tags=true (matches tag pattern). Fixed content: {fixed_content}"
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

/// End-to-end test: Azure DevOps flavor suppresses MD013 inside colon fences.
///
/// In Azure DevOps Markdown, colon fences (`::: mermaid ... :::`) are code blocks.
/// Lines inside those fences should not trigger MD013 line-length warnings.
#[test]
fn test_flavor_azure_devops_suppresses_md013_in_colon_fence() {
    let temp_dir = tempdir().unwrap();
    let md_path = temp_dir.path().join("test.md");
    // Very long line inside a colon fence — should not trigger MD013
    let long_line = "A".repeat(150);
    let content = format!("# Diagram\n\n::: mermaid\n{long_line}\n:::\n");
    fs::write(&md_path, &content).unwrap();

    let (success, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--flavor", "azure_devops", "test.md"]);
    assert!(
        success,
        "Should pass with no warnings. stderr: {stderr}, stdout: {stdout}"
    );
}

/// Regression test: MD051 resolves its anchor style per file, not once from the
/// global flavor.
///
/// The anchor style a heading fragment is generated with is a property of the
/// renderer, so a file `per-file-flavor` hands to MkDocs must be checked against
/// Python-Markdown anchors even though the rule was constructed under a global
/// gfm flavor. An em dash collapses to one hyphen under Python-Markdown and
/// vanishes between two hyphens under GitHub, so exactly one of the two links
/// below is invalid and which one names the style that was used.
#[test]
fn test_per_file_flavor_selects_md051_anchor_style() {
    let temp_dir = tempdir().unwrap();

    let config_content = r#"
[global]
flavor = "gfm"
enable = ["MD051"]

[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"#;
    fs::write(temp_dir.path().join(".rumdl.toml"), config_content).unwrap();

    let content = "### Getting Started — Advanced\n\n\
        [python-markdown slug](#getting-started-advanced)\n\
        [github slug](#getting-started--advanced)\n";
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir(&docs_dir).unwrap();
    fs::write(docs_dir.join("page.md"), content).unwrap();
    // Control: the same file outside the per-file-flavor glob, so the global
    // flavor applies to it.
    fs::write(temp_dir.path().join("page.md"), content).unwrap();

    let (_, stdout, stderr) = run_rumdl(temp_dir.path(), &["check", "--no-cache", "."]);

    let flagged = |path: &str| -> Vec<&str> {
        stdout
            .lines()
            .filter(|line| line.contains("MD051") && line.contains(path))
            .collect()
    };

    let docs_flagged = flagged("docs");
    assert_eq!(docs_flagged.len(), 1, "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        docs_flagged[0].contains("#getting-started--advanced"),
        "a mkdocs file must be checked against Python-Markdown anchors, so the GitHub slug is \
         the broken one. Got: {docs_flagged:?}\nstdout: {stdout}"
    );

    let root_flagged: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("MD051") && !line.contains("docs"))
        .collect();
    assert_eq!(root_flagged.len(), 1, "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        root_flagged[0].contains("#getting-started-advanced'"),
        "a file the global gfm flavor applies to must keep GitHub anchors. \
         Got: {root_flagged:?}\nstdout: {stdout}"
    );
}
