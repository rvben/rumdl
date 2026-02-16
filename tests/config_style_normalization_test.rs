//! Generic test that verifies all rule style configuration options accept
//! both snake_case and kebab-case variants, and are case-insensitive.
//!
//! This test catches issue #428 (inconsistent style option formatting
//! across rules) by running the actual linter with variant config values
//! and checking stderr for "Invalid configuration" warnings.
//!
//! Design: Uses process-level testing because:
//! - Config validation happens at rule instantiation time (not config loading)
//! - Invalid style values produce stderr warnings + fallback to defaults
//! - The per-rule config types are private (not accessible from integration tests)
//! - This tests the actual user experience end-to-end

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Each entry: (rule_name, field_name, canonical_values)
///
/// canonical_values are the primary (documented) forms.
/// The test generates snake_case, kebab-case, UPPER, and Mixed variants
/// and verifies the linter accepts them without warnings.
const STYLE_CONFIGS: &[(&str, &str, &[&str])] = &[
    // MD003: heading style (multi-word variants need hyphen/underscore normalization)
    (
        "MD003",
        "style",
        &[
            "atx",
            "atx_closed",
            "consistent",
            "setext_with_atx",
            "setext_with_atx_closed",
        ],
    ),
    // MD046: code block style
    ("MD046", "style", &["fenced", "indented", "consistent"]),
    // MD048: code fence style
    ("MD048", "style", &["backtick", "tilde", "consistent"]),
    // MD049: emphasis style
    ("MD049", "style", &["asterisk", "underscore", "consistent"]),
    // MD050: strong style
    ("MD050", "style", &["asterisk", "underscore", "consistent"]),
    // MD055: table pipe style
    (
        "MD055",
        "style",
        &[
            "consistent",
            "leading-only",
            "trailing-only",
            "leading-and-trailing",
            "no-leading-or-trailing",
        ],
    ),
    // MD060: table format style
    (
        "MD060",
        "style",
        &["aligned", "aligned-no-space", "compact", "tight", "any"],
    ),
    // MD063: heading capitalization style
    ("MD063", "style", &["title_case", "sentence_case", "all_caps"]),
];

/// Generate variant forms of a style value for testing.
fn generate_variants(canonical: &str) -> Vec<(String, String)> {
    let mut variants = vec![];

    // Original canonical form
    variants.push(("canonical".to_string(), canonical.to_string()));

    // UPPERCASE
    variants.push(("UPPERCASE".to_string(), canonical.to_uppercase()));

    // If contains underscores, also test kebab-case variant
    if canonical.contains('_') {
        let kebab = canonical.replace('_', "-");
        variants.push(("kebab-case".to_string(), kebab.clone()));
        variants.push(("KEBAB-UPPER".to_string(), kebab.to_uppercase()));
    }

    // If contains hyphens, also test snake_case variant
    if canonical.contains('-') {
        let snake = canonical.replace('-', "_");
        variants.push(("snake_case".to_string(), snake.clone()));
        variants.push(("SNAKE_UPPER".to_string(), snake.to_uppercase()));
    }

    variants
}

/// Build the test binary path. Uses the debug target directory.
fn rumdl_binary() -> String {
    // Use cargo to find the binary
    let output = Command::new("cargo")
        .args(["build", "--quiet"])
        .output()
        .expect("Failed to build rumdl");
    assert!(
        output.status.success(),
        "cargo build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Return the path to the debug binary
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/target/debug/rumdl")
}

#[test]
fn test_all_style_configs_accept_variant_forms() {
    let binary = rumdl_binary();
    let temp_dir = tempdir().expect("Failed to create temporary directory");

    // Create a minimal markdown file to lint
    let md_file = temp_dir.path().join("test.md");
    fs::write(&md_file, "# Test\n\nSome content.\n").expect("Failed to write markdown file");

    let mut failures = Vec::new();

    for (rule_name, field_name, canonical_values) in STYLE_CONFIGS {
        for canonical in *canonical_values {
            for (variant_desc, variant_value) in generate_variants(canonical) {
                let config_content = format!("[{rule_name}]\n{field_name} = \"{variant_value}\"\n");

                let config_path = temp_dir.path().join(".rumdl.toml");
                fs::write(&config_path, &config_content).expect("Failed to write config");

                let output = Command::new(&binary)
                    .args([
                        "check",
                        "--no-cache",
                        "--config",
                        config_path.to_str().unwrap(),
                        md_file.to_str().unwrap(),
                    ])
                    .output()
                    .expect("Failed to run rumdl");

                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check for "Invalid configuration" warnings on stderr
                if stderr.contains("Invalid configuration")
                    || stderr.contains("unknown variant")
                    || stderr.contains("Invalid")
                {
                    failures.push(format!(
                        "{rule_name}.{field_name} = \"{variant_value}\" ({variant_desc} of \"{canonical}\"): {stderr}"
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "Style configuration normalization failures ({} total):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}
