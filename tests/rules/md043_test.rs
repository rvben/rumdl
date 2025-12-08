use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD043RequiredHeadings;

#[test]
fn test_matching_headings() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Methods\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_heading() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // MD043 now preserves original content to prevent data loss
    assert_eq!(fixed, content);
}

#[test]
fn test_extra_heading() {
    let required = vec!["# Introduction".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Methods\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // MD043 now preserves original content to prevent data loss
    assert_eq!(fixed, content);
}

#[test]
fn test_wrong_order() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results\n\n# Methods";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // MD043 now preserves original content to prevent data loss
    assert_eq!(fixed, content);
}

#[test]
fn test_empty_required_headings() {
    let required = vec![];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Any heading\n\n# Another heading";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_case_sensitive() {
    let required = vec!["# Introduction".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# INTRODUCTION";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should match because match_case is false by default
}

#[test]
fn test_mixed_heading_styles() {
    let required = vec!["# Introduction".to_string(), "======= Methods".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\nContent\nMethods\n=======";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

// Integration tests for wildcard patterns

#[test]
fn test_asterisk_wildcard_integration() {
    let required = vec!["# README".to_string(), "*".to_string(), "## License".to_string()];
    let rule = MD043RequiredHeadings::new(required);

    // Should pass with optional sections between README and License
    let content = "# README\n\n## Installation\n\n## Usage\n\n### Examples\n\n## License";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Should pass with no sections between README and License
    let content = "# README\n\n## License";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_plus_wildcard_integration() {
    let required = vec![
        "# Documentation".to_string(),
        "+".to_string(),
        "## Contributing".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);

    // Should pass with one or more sections
    let content = "# Documentation\n\n## API\n\n## Contributing";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Should fail with zero sections
    let content = "# Documentation\n\n## Contributing";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_question_wildcard_integration() {
    let required = vec![
        "?".to_string(),
        "## Description".to_string(),
        "## Installation".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);

    // Should pass with variable project title
    let content = "# My Awesome Project\n\n## Description\n\n## Installation";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Should pass with different title
    let content = "# Another Project\n\n## Description\n\n## Installation";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Should fail without any title
    let content = "## Description\n\n## Installation";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_real_world_documentation_pattern() {
    // Typical open source project documentation structure
    let required = vec![
        "?".to_string(),               // Project name (variable)
        "## Overview".to_string(),     // Required
        "*".to_string(),               // Optional sections (badges, screenshots, etc.)
        "## Installation".to_string(), // Required
        "*".to_string(),               // Optional sections (usage, examples, etc.)
        "## License".to_string(),      // Required
    ];
    let rule = MD043RequiredHeadings::new(required);

    // Minimal documentation
    let content = "# MyLib\n\n## Overview\n\n## Installation\n\n## License";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Full documentation with all optional sections
    let content = "# MyLib\n\n## Overview\n\n## Features\n\n## Screenshots\n\n## Installation\n\n## Usage\n\n## API Reference\n\n## Examples\n\n## License";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Missing required section should fail
    let content = "# MyLib\n\n## Overview\n\n## Features\n\n## License";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}
