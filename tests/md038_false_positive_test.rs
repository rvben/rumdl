use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::rules;

#[test]
fn test_md038_no_false_positive_for_commands() {
    // Test cases from README.md that are incorrectly flagged
    let test_cases = vec![
        // Line 606: pyproject.toml has no trailing spaces
        "3. `pyproject.toml` (must contain `[tool.rumdl]` section)",
        // Line 733: rumdl config has no trailing spaces
        "#### Effective Configuration (`rumdl config`)",
        // Line 755: .rumdl.toml has no trailing spaces
        "- Blue: `.rumdl.toml`",
        // Line 760: rumdl config --defaults has no trailing spaces
        "### Defaults Only (`rumdl config --defaults`)",
    ];

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md038_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD038").collect();

    for content in test_cases {
        let warnings = rumdl_lib::lint(content, &md038_rules, false, MarkdownFlavor::Standard, None).unwrap();

        // These should NOT produce warnings - the code spans have no leading/trailing spaces
        assert_eq!(
            warnings.len(),
            0,
            "MD038 should not flag code spans without leading/trailing spaces. Content: '{}'. Warnings: {:?}",
            content,
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_md038_correctly_flags_actual_spaces() {
    // CommonMark: Single space at BOTH ends is valid (spaces are stripped)
    // Space at only ONE end should be flagged
    let test_cases = vec![
        ("` pyproject.toml`", 1),  // Leading space only - SHOULD be flagged
        ("`pyproject.toml `", 1),  // Trailing space only - SHOULD be flagged
        ("` pyproject.toml `", 0), // Both leading and trailing - valid CommonMark (stripped)
    ];

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md038_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD038").collect();

    for (content, expected_warnings) in test_cases {
        let warnings = rumdl_lib::lint(content, &md038_rules, false, MarkdownFlavor::Standard, None).unwrap();

        assert_eq!(
            warnings.len(),
            expected_warnings,
            "MD038 CommonMark space-stripping behavior. Content: '{content}'"
        );
    }
}
