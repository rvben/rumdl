use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::MD043RequiredHeadings;

fn assert_warning_message(result: &[LintWarning], expected: &str) {
    assert!(!result.is_empty(), "Expected at least one warning");
    assert!(
        result.iter().all(|warning| warning.message == expected),
        "Expected all warnings to have message {expected:?}, got: {result:?}"
    );
}

fn warning_messages(result: &[LintWarning]) -> Vec<&str> {
    result.iter().map(|warning| warning.message.as_str()).collect()
}

#[test]
fn test_reports_each_literal_alignment_edit() {
    let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# C".into()]);

    let substitutions = rule
        .check(&rumdl_lib::lint_context::LintContext::new(
            "# A\n# X\n# Y",
            rumdl_lib::config::MarkdownFlavor::Standard,
            None,
        ))
        .unwrap();
    assert_eq!(
        warning_messages(&substitutions),
        [
            "Heading structure does not match required structure. Expected heading '# B', but found '# X'",
            "Heading structure does not match required structure. Expected heading '# C', but found '# Y'",
        ]
    );

    let ambiguous = rule
        .check(&rumdl_lib::lint_context::LintContext::new(
            "# A\n# C\n# D",
            rumdl_lib::config::MarkdownFlavor::Standard,
            None,
        ))
        .unwrap();
    assert_eq!(
        warning_messages(&ambiguous),
        [
            "Heading structure does not match required structure. Missing required heading '# B'",
            "Heading structure does not match required structure. Unexpected heading '# D' at position 3",
        ]
    );
}

#[test]
fn test_reports_reordering_without_hiding_other_extras() {
    let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# C".into()]);
    let ctx = rumdl_lib::lint_context::LintContext::new(
        "# A\n# X\n# Y\n# C\n# D\n# B",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        warning_messages(&result),
        [
            "Heading structure does not match required structure. Unexpected heading '# X' at position 2",
            "Heading structure does not match required structure. Unexpected heading '# Y' at position 3",
            "Heading structure does not match required structure. Unexpected heading '# D' at position 5",
            "Heading structure does not match required structure. Heading '# B' is out of order; expected between '# A' and '# C'",
        ]
    );
    assert_eq!(
        result.iter().map(|warning| warning.line).collect::<Vec<_>>(),
        [2, 3, 5, 6]
    );
}

#[test]
fn test_crossed_substitutions_are_reported_as_reorders() {
    let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# C".into()]);
    let ctx =
        rumdl_lib::lint_context::LintContext::new("# C\n# B\n# A", rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        warning_messages(&result),
        [
            "Heading structure does not match required structure. Heading '# C' is out of order; expected after '# B'",
            "Heading structure does not match required structure. Heading '# A' is out of order; expected before '# B'",
        ]
    );
    assert_eq!(result.iter().map(|warning| warning.line).collect::<Vec<_>>(), [1, 3]);
}

#[test]
fn test_multiple_reorders_pair_duplicate_occurrences_once() {
    let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# A".into(), "# C".into()]);
    let ctx = rumdl_lib::lint_context::LintContext::new(
        "# B\n# C\n# A\n# A",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        warning_messages(&result),
        [
            "Heading structure does not match required structure. Heading '# C' is out of order; expected after '# A'",
            "Heading structure does not match required structure. Heading '# A' is out of order; expected before '# B'",
        ]
    );
    assert_eq!(result.iter().map(|warning| warning.line).collect::<Vec<_>>(), [2, 4]);
}

#[test]
fn test_adjacent_wildcard_runs_have_additive_minimums() {
    for (patterns, passing_count, failing_count) in [
        (vec!["*", "?"], 1, 0),
        (vec!["?", "+"], 2, 1),
        (vec!["+", "+"], 2, 1),
        (vec!["+", "*"], 1, 0),
        (vec!["?", "?"], 2, 1),
    ] {
        let repeats = patterns.iter().any(|pattern| matches!(*pattern, "*" | "+"));
        let mut required = vec!["# Start".to_string()];
        required.extend(patterns.into_iter().map(str::to_string));
        required.push("# End".to_string());
        let rule = MD043RequiredHeadings::new(required);
        let document = |count: usize| {
            let middle = (0..count).map(|i| format!("## {i}")).collect::<Vec<_>>().join("\n");
            if middle.is_empty() {
                "# Start\n# End".to_string()
            } else {
                format!("# Start\n{middle}\n# End")
            }
        };

        let passing = document(passing_count);
        let passing_ctx =
            rumdl_lib::lint_context::LintContext::new(&passing, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&passing_ctx).unwrap().is_empty());

        let failing = document(failing_count);
        let failing_ctx =
            rumdl_lib::lint_context::LintContext::new(&failing, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&failing_ctx).unwrap().len(), 1);

        if repeats {
            let above_minimum = document(passing_count + 2);
            let above_minimum_ctx = rumdl_lib::lint_context::LintContext::new(
                &above_minimum,
                rumdl_lib::config::MarkdownFlavor::Standard,
                None,
            );
            assert!(
                rule.check(&above_minimum_ctx).unwrap().is_empty(),
                "a repeating wildcard in an anchored run should absorb headings above the minimum"
            );
        }
    }
}

#[test]
fn test_warning_ranges_and_omission_order() {
    let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# C".into()]);
    let ctx =
        rumdl_lib::lint_context::LintContext::new("preamble\n\n# C", rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        warning_messages(&result),
        [
            "Heading structure does not match required structure. Missing required heading '# A'",
            "Heading structure does not match required structure. Missing required heading '# B'",
        ]
    );
    assert_eq!(
        result
            .iter()
            .map(|warning| (warning.line, warning.column, warning.end_line, warning.end_column))
            .collect::<Vec<_>>(),
        [(3, 1, 3, 4), (3, 1, 3, 4)]
    );

    let headingless =
        rumdl_lib::lint_context::LintContext::new("plain text", rumdl_lib::config::MarkdownFlavor::Standard, None);
    let headingless_result = rule.check(&headingless).unwrap();
    assert_eq!(headingless_result.len(), 3);
    assert!(
        headingless_result
            .iter()
            .all(|warning| { (warning.line, warning.column, warning.end_line, warning.end_column) == (1, 1, 1, 2) })
    );
}

#[test]
fn test_exact_ranges_for_each_alignment_edit() {
    let cases = [
        (
            vec!["## Right"],
            "intro\n\n### Wrong ###",
            "Expected heading '## Right', but found '### Wrong'",
            (3, 1, 3, 14),
        ),
        (
            vec!["# A"],
            "# A\n\n## Extra ##",
            "Unexpected heading '## Extra' at position 2",
            (3, 1, 3, 12),
        ),
        (
            vec!["# A", "# B", "# C"],
            "# A\n# C\n# B",
            "Heading '# B' is out of order; expected between '# A' and '# C'",
            (3, 1, 3, 4),
        ),
        (
            vec!["# A", "+", "# B"],
            "# A\n\n# B ##",
            "Wildcard '+' at position 2 requires one or more headings, but none was available",
            (3, 1, 3, 7),
        ),
        (
            vec!["====== Expected"],
            "Actual\n------",
            "Expected heading '====== Expected', but found '------ Actual'",
            (1, 1, 1, 7),
        ),
    ];

    for (headings, content, message_suffix, expected_range) in cases {
        let rule = MD043RequiredHeadings::new(headings.into_iter().map(str::to_string).collect());
        let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "content={content:?}");
        assert!(result[0].message.ends_with(message_suffix), "warning={:?}", result[0]);
        assert_eq!(
            (
                result[0].line,
                result[0].column,
                result[0].end_line,
                result[0].end_column,
            ),
            expected_range,
            "warning={:?}",
            result[0]
        );
    }
}

#[test]
fn test_required_wildcards_may_consume_literal_text_before_anchor() {
    let question = MD043RequiredHeadings::new(vec!["?".into(), "# A".into()]);
    let duplicate_literal =
        rumdl_lib::lint_context::LintContext::new("# A\n# A", rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(question.check(&duplicate_literal).unwrap().is_empty());

    let plus = MD043RequiredHeadings::new(vec!["+".into(), "# B".into()]);
    let duplicate_anchor =
        rumdl_lib::lint_context::LintContext::new("# B\n# X\n# B", rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(plus.check(&duplicate_anchor).unwrap().is_empty());
}

#[test]
fn test_repeating_wildcard_consumption_can_be_a_reorder_candidate() {
    let reordered = MD043RequiredHeadings::new(vec!["# A".into(), "*".into(), "# B".into(), "# C".into()]);
    let ctx =
        rumdl_lib::lint_context::LintContext::new("# A\n# C\n# B", rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = reordered.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Heading structure does not match required structure. Heading '# C' is out of order; expected after '# B'"
    );
}

#[test]
fn test_required_wildcard_consumption_is_not_reused_for_reordering() {
    for wildcard in ["?", "+"] {
        let rule = MD043RequiredHeadings::new(vec!["# A".into(), wildcard.into(), "# B".into(), "# C".into()]);
        let ctx = rumdl_lib::lint_context::LintContext::new(
            "# A\n# C\n# B",
            rumdl_lib::config::MarkdownFlavor::Standard,
            None,
        );

        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            warning_messages(&result),
            ["Heading structure does not match required structure. Missing required heading '# C'"],
            "required wildcard {wildcard} must retain ownership of the consumed heading"
        );
    }
}

#[test]
fn test_wrong_heading_level_is_a_substitution() {
    let rule = MD043RequiredHeadings::new(vec!["## A".into()]);
    let ctx = rumdl_lib::lint_context::LintContext::new("### A", rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].message,
        "Heading structure does not match required structure. Expected heading '## A', but found '### A'"
    );
}

#[test]
fn test_headingless_document_runs_through_lint_pipeline() {
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD043RequiredHeadings::new(vec!["# Required".into()]))];
    let result = rumdl_lib::lint(
        "plain text",
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);

    let empty = rumdl_lib::lint(
        "",
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();
    assert!(empty.is_empty());

    for (wildcard, expected_count) in [("?", 1), ("+", 1), ("*", 0)] {
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD043RequiredHeadings::new(vec![wildcard.into()]))];
        let result = rumdl_lib::lint(
            "plain text",
            &rules,
            false,
            rumdl_lib::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.len(), expected_count, "wildcard={wildcard}");
    }
}

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
fn test_structure_mismatch_message_reports_missing_heading() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Missing required heading '# Methods'",
    );
}

#[test]
fn test_structure_mismatch_message_reports_missing_trailing_heading() {
    let required = vec!["# Introduction".to_string(), "# Methods".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Missing required heading '# Methods'",
    );
}

#[test]
fn test_structure_mismatch_message_reports_extra_trailing_heading() {
    let required = vec!["# Introduction".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n## Extra";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Unexpected heading '## Extra' at position 2",
    );
}

#[test]
fn test_structure_mismatch_message_reports_extra_trailing_heading_after_wildcard() {
    let required = vec!["# Introduction".to_string(), "*".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n## Optional 1\n\n## Optional 2\n\n# Results\n\n## Extra";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Unexpected heading '## Extra' at position 5",
    );
}

#[test]
fn test_structure_mismatch_message_handles_plus_before_required_heading() {
    let required = vec!["# Introduction".to_string(), "+".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Wildcard '+' at position 2 requires one or more headings, but none was available",
    );
}

#[test]
fn test_structure_mismatch_message_preserves_actual_case_after_wildcard() {
    let required = vec!["+".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# RESULTS";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Wildcard '+' at position 1 requires one or more headings, but none was available",
    );
}

#[test]
fn test_structure_mismatch_message_handles_question_before_required_heading() {
    let required = vec!["?".to_string(), "## Description".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "## Description";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Wildcard '?' at position 1 requires one heading, but none was available",
    );
}

#[test]
fn test_structure_mismatch_message_handles_asterisk_missing_required_heading() {
    let required = vec!["# Introduction".to_string(), "*".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n## Optional 1\n\n## Optional 2";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    assert_warning_message(
        &result,
        "Heading structure does not match required structure. Missing required heading '# Results'",
    );
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
