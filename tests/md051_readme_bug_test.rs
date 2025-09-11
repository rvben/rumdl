use rumdl_lib::config::{Config, MarkdownFlavor};
/// Test for MD051 false positives with the actual README
/// This test verifies that MD051 correctly finds headings in README.md
use rumdl_lib::rules;
use std::fs;

#[test]
fn test_md051_readme_headings() {
    // Read the actual README
    let content = fs::read_to_string("README.md").expect("Failed to read README.md");

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md051_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD051").collect();

    let warnings = rumdl_lib::lint(&content, &md051_rules, false, MarkdownFlavor::Standard).unwrap();

    // Check specific false positives that shouldn't exist
    let false_positives = vec![
        "#markdownlint-migration",
        "#configuration-file-example",
        "#initializing-configuration",
        "#configuration-in-pyprojecttoml",
        "#configuration-output",
    ];

    for fp in &false_positives {
        let has_warning = warnings.iter().any(|w| w.message.contains(fp));
        assert!(
            !has_warning,
            "MD051 should not report '{fp}' as missing - this heading exists in the document"
        );
    }
}
