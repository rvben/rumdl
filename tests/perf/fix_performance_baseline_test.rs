/// Performance baseline test for fix functionality
/// This test captures the current behavior and performance characteristics
/// of the fix system before refactoring to batch text edits
///
/// This will help ensure our refactor:
/// 1. Maintains the same fix output
/// 2. Actually improves performance
/// 3. Doesn't break any edge cases
use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use std::time::Instant;

/// Test data structure to capture baseline behavior
#[derive(Debug)]
struct FixBaseline {
    rule_name: &'static str,
    original_content: &'static str,
    fixed_content: String,
    warnings_before: usize,
    warnings_after: usize,
    fix_time_ms: u128,
    context_creations: usize,
}

/// Helper to count LintContext creations (simulated)
fn count_context_creations_for_fix(rules: &[Box<dyn Rule>], content: &str, config: &Config) -> (String, usize) {
    let mut current_content = content.to_string();
    let mut context_creations = 0;

    // Simulate current behavior: one context per rule that fixes
    for rule in rules {
        // Create context for this rule
        let ctx = LintContext::new(&current_content, config.markdown_flavor(), None);
        context_creations += 1;

        let warnings = rule.check(&ctx).unwrap_or_default();
        // A rule is considered fixable if it has a fix() implementation that works
        if !warnings.is_empty() {
            // Try to apply the fix
            match rule.fix(&ctx) {
                Ok(fixed) if fixed != current_content => {
                    current_content = fixed;
                }
                _ => {
                    // Fix not implemented or didn't change content
                }
            }
        }
    }

    // Final context for verification (in real code)
    context_creations += 1;

    (current_content, context_creations)
}

#[test]
fn test_fix_performance_baseline() {
    let test_cases = vec![
        // Simple case: single rule violation
        ("simple_trailing_spaces", "Line with trailing spaces   \n"),
        // Multiple rules: trailing spaces + list style
        (
            "multiple_rules",
            "# Heading\n\nLine with spaces   \n\n* Item 1\n+ Item 2\n- Item 3\n",
        ),
        // Complex case: many rules with violations
        (
            "complex_document",
            r#"# Document with many issues

##No space after heading marker
###  Too many spaces

Line with trailing spaces

* Inconsistent
+ list
- markers

**Bold__text**

[Empty link]()

`code with spaces `

| Very very very very very very very very very very very very very very very very very long table header | Col2 |
|----|----|
| A | B |

http://bare-url.com
"#,
        ),
    ];

    let config = Config::default();
    let mut baselines = Vec::new();

    println!("\n=== Capturing Fix Performance Baseline ===\n");

    for (test_name, content) in test_cases {
        // Get all rules (we'll check fixability by testing if fix() works)
        let all_rules = all_rules(&config);
        let fixable_rules = all_rules;

        // Measure current fix performance
        let start = Instant::now();
        let (fixed_content, context_creations) = count_context_creations_for_fix(&fixable_rules, content, &config);
        let elapsed = start.elapsed().as_millis();

        // Count warnings before and after
        let ctx_before = LintContext::new(content, config.markdown_flavor(), None);
        let ctx_after = LintContext::new(&fixed_content, config.markdown_flavor(), None);

        let mut warnings_before = 0;
        let mut warnings_after = 0;

        for rule in &fixable_rules {
            warnings_before += rule.check(&ctx_before).unwrap_or_default().len();
            warnings_after += rule.check(&ctx_after).unwrap_or_default().len();
        }

        let baseline = FixBaseline {
            rule_name: test_name,
            original_content: content,
            fixed_content: fixed_content.clone(),
            warnings_before,
            warnings_after,
            fix_time_ms: elapsed,
            context_creations,
        };

        println!("Test: {test_name}");
        println!("  Content length: {} chars", content.len());
        println!("  Warnings: {warnings_before} -> {warnings_after}");
        println!("  Context creations: {context_creations}");
        println!("  Time: {elapsed}ms");
        println!("  Fixed: {}", content != fixed_content);
        println!();

        baselines.push(baseline);
    }

    // Store baselines for future comparison
    println!("=== Baseline Summary ===");
    println!("Total test cases: {}", baselines.len());
    println!(
        "Average context creations: {:.1}",
        baselines.iter().map(|b| b.context_creations as f64).sum::<f64>() / baselines.len() as f64
    );
    println!(
        "Total fix time: {}ms",
        baselines.iter().map(|b| b.fix_time_ms).sum::<u128>()
    );

    // Verify all fixes are deterministic
    for baseline in &baselines {
        // Re-run the same fix
        let all_rules = all_rules(&config);
        let fixable_rules = all_rules;

        let (second_fix, _) = count_context_creations_for_fix(&fixable_rules, baseline.original_content, &config);

        assert_eq!(
            baseline.fixed_content, second_fix,
            "Fix for '{}' should be deterministic",
            baseline.rule_name
        );

        // Verify fixes reduced or maintained warning count (never increased)
        assert!(
            baseline.warnings_after <= baseline.warnings_before,
            "Fix for '{}' should not increase warnings ({} -> {})",
            baseline.rule_name,
            baseline.warnings_before,
            baseline.warnings_after
        );
    }
}

#[test]
fn test_fix_application_order_independence() {
    // Test that applying fixes in different orders produces the same result
    // This is important for the batch fix refactor

    let content = "# Heading\n\nLine with spaces   \n\n* Item\n+ Item\n";
    let config = Config::default();

    // Apply MD009 (trailing spaces) first, then MD004 (list style)
    let mut content1 = content.to_string();
    let md009 = MD009TrailingSpaces::default();
    let md004 = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);

    let ctx = LintContext::new(&content1, config.markdown_flavor(), None);
    if let Ok(fixed) = md009.fix(&ctx) {
        content1 = fixed;
    }
    let ctx = LintContext::new(&content1, config.markdown_flavor(), None);
    if let Ok(fixed) = md004.fix(&ctx) {
        content1 = fixed;
    }

    // Apply MD004 first, then MD009
    let mut content2 = content.to_string();
    let ctx = LintContext::new(&content2, config.markdown_flavor(), None);
    if let Ok(fixed) = md004.fix(&ctx) {
        content2 = fixed;
    }
    let ctx = LintContext::new(&content2, config.markdown_flavor(), None);
    if let Ok(fixed) = md009.fix(&ctx) {
        content2 = fixed;
    }

    // Both orders should produce the same result
    assert_eq!(
        content1, content2,
        "Fix order should not affect final result\nOrder 1: {content1:?}\nOrder 2: {content2:?}"
    );
}

#[test]
fn test_overlapping_fixes() {
    // Test cases where multiple rules might fix the same or overlapping regions
    let test_cases = vec![
        // Multiple issues on same line
        (
            "same_line_multiple",
            "#  Heading with spaces   \n", // MD019 (multiple spaces) + MD009 (trailing)
        ),
        // Overlapping regions
        (
            "overlapping",
            "**bold __text**__\n", // MD050 (strong style) might overlap
        ),
        // Adjacent fixes
        (
            "adjacent",
            "Word1  Word2\n", // Multiple spaces between words
        ),
    ];

    let config = Config::default();
    let all_rules = all_rules(&config);

    for (name, content) in test_cases {
        let ctx = LintContext::new(content, config.markdown_flavor(), None);

        // Collect all fixes that would be applied
        let mut fixes_applied = Vec::new();
        for rule in &all_rules {
            if let Ok(warnings) = rule.check(&ctx)
                && !warnings.is_empty()
                && let Ok(fixed) = rule.fix(&ctx)
                && fixed != content
            {
                fixes_applied.push(rule.name());
            }
        }

        println!("Test '{}': {} rules would fix", name, fixes_applied.len());
        for rule_name in &fixes_applied {
            println!("  - {rule_name}");
        }

        // Ensure sequential application works
        let mut sequential_content = content.to_string();
        for rule in &all_rules {
            let ctx = LintContext::new(&sequential_content, config.markdown_flavor(), None);
            if let Ok(warnings) = rule.check(&ctx)
                && !warnings.is_empty()
                && let Ok(fixed) = rule.fix(&ctx)
            {
                sequential_content = fixed;
            }
        }

        // Final content should be valid
        let final_ctx = LintContext::new(&sequential_content, config.markdown_flavor(), None);
        let mut remaining_warnings = 0;
        for rule in &all_rules {
            if let Ok(warnings) = rule.check(&final_ctx) {
                remaining_warnings += warnings.len();
            }
        }

        println!("  Final: {remaining_warnings} remaining warnings\n");
    }
}
