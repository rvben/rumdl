use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD001HeadingIncrement;

#[test]
pub fn test_md001_valid() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_invalid() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Expected heading level 2, but found heading level 3");
}

#[test]
pub fn test_md001_multiple_violations() {
    let rule = MD001HeadingIncrement::default();
    // H1 → H3 → H4: check() tracks the fixed level for idempotent fixes.
    // H3 is flagged (expected H2), H4 is flagged (expected H3 after fixed H2).
    let content = "# Heading 1\n### Heading 3\n#### Heading 4\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
pub fn test_md001_fix() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 3\n");
}

#[test]
pub fn test_md001_no_headings() {
    let rule = MD001HeadingIncrement::default();
    let content = "This is a paragraph\nwith no headings.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_single_heading() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Single Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_atx_and_setext() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\nHeading 2\n---------\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_ignores_headings_in_html_comments() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Real Heading 1\n\n<!--\n## This heading is in a comment\n### This one too\n-->\n\n### This should trigger MD001\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should get exactly one warning for the level 3 heading that comes after level 1
    assert_eq!(result.len(), 1, "Should have one MD001 violation, but got: {result:?}");
    assert_eq!(result[0].line, 8, "MD001 violation should be on line 8");
    assert_eq!(result[0].message, "Expected heading level 2, but found heading level 3");
}

#[test]
pub fn test_md001_html_comments_dont_affect_heading_sequence() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n\n<!--\n#### Random comment heading\n-->\n\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no violations - the comment heading shouldn't affect the sequence
    assert!(
        result.is_empty(),
        "Should have no violations when HTML comment headings don't interfere, but got: {result:?}"
    );
}

/// Setext H1 followed by a deep ATX heading: the ATX heading is fixed to H2.
#[test]
pub fn test_md001_setext_h1_followed_by_deep_atx() {
    let rule = MD001HeadingIncrement::default();
    let content = "Title\n=====\n\n#### Fourth Level\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "H4 after Setext H1 should be flagged");
    assert!(warnings[0].message.contains("Expected heading level 2"));

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("## Fourth Level"),
        "H4 should become H2 (ATX), got: {fixed}"
    );

    // Verify fix is idempotent
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert_eq!(fixed, fixed2, "fix() must be idempotent");
}

/// Parametric test: for every warning, check()'s fix replacement matches fix()'s output line.
#[test]
pub fn test_md001_check_fix_consistency() {
    let rule = MD001HeadingIncrement::default();

    let inputs = [
        "# H1\n### H3\n",
        "# H1\n#### H4\n##### H5\n",
        "## H2\n##### H5\n",
        "# A\n### B\n# C\n#### D\n",
        "Title\n=====\n\n#### Deep\n",
        "---\ntitle: T\n---\n\n#### Deep\n",
        "# H1\n  ### Indented H3\n",
    ];

    for input in &inputs {
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();
        let fixed_lines: Vec<&str> = fixed.lines().collect();

        for w in &warnings {
            if let Some(ref fix) = w.fix {
                let idx = w.line - 1;
                assert!(
                    idx < fixed_lines.len(),
                    "Line {} out of range for input: {input:?}",
                    w.line
                );
                assert_eq!(
                    fix.replacement, fixed_lines[idx],
                    "check()/fix() diverge at line {} for input: {input:?}",
                    w.line
                );
            }
        }
    }
}
