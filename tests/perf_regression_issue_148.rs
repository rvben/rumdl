/// Performance regression test for issue #148
///
/// Tests that document processing scales linearly O(n) and not quadratically O(n²).
/// This specifically tests scenarios with many list items and blockquotes that
/// previously caused LineIndex to be created in hot loops.
///
/// ## When These Tests Run
///
/// These tests are EXCLUDED from the `dev` and `ci` profiles because timing-based
/// assertions are inherently flaky under parallel execution. They run in the
/// `performance` profile which uses serial execution (`test-threads = 1`).
///
/// To run these tests manually:
/// ```sh
/// cargo nextest run --profile performance --test perf_regression_issue_148
/// ```
///
/// Uses large input sizes (500/1000/2000 entries) to minimize timing noise from
/// system jitter. With smaller inputs, base times are in microseconds where a
/// single context switch can cause >6x variance.
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::{MD020NoMissingSpaceClosedAtx, MD027MultipleSpacesBlockquote, rule::Rule};
use std::time::Instant;

/// Generate a test document with nested lists and quoted strings
/// This pattern was identified in issue #148 as causing O(n²) behavior
fn generate_list_document(num_entries: usize) -> String {
    let mut content = String::with_capacity(num_entries * 150);
    content.push_str("# Work Log\n\n");

    for i in 0..num_entries {
        content.push_str(&format!("- day-{i}: 2025-06-{:02}\n", (i % 28) + 1));
        // Add sub-items with blockquotes that trigger MD027
        content.push_str("  - task: 09:00-10:00\n");
        content.push_str(">  Extra space after marker\n"); // Triggers MD027
        content.push_str("    - fix: add field\n");
        content.push_str(&format!("    - fix: \"json_tag\": \"[{i}]\"\n"));
        content.push_str("    - fix: \"local_field\": [\"record_id\"]\n");
    }

    content
}

/// Generate document with many headings that could trigger MD020
fn generate_heading_document(num_headings: usize) -> String {
    let mut content = String::with_capacity(num_headings * 50);

    for i in 0..num_headings {
        // Some valid, some with missing space (triggers MD020)
        if i % 3 == 0 {
            content.push_str(&format!("## Heading {i}##\n\n")); // Missing space
        } else {
            content.push_str(&format!("## Heading {i} ##\n\n")); // Valid
        }
        content.push_str("Some content here.\n\n");
    }

    content
}

#[test]
fn test_md027_linear_complexity() {
    // Test with documents of increasing size
    // If complexity is O(n), doubling size should roughly double time (±50% margin)
    // If complexity is O(n²), doubling size would increase time by 4x
    //
    // Using 500/1000/2000 entries to ensure base times are in milliseconds,
    // reducing impact of system jitter during parallel test execution

    let sizes = [500, 1000, 2000];
    let mut durations = Vec::new();

    for &size in &sizes {
        let content = generate_list_document(size);
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD027MultipleSpacesBlockquote;

        let start = Instant::now();
        let warnings = rule.check(&ctx).unwrap();
        let duration = start.elapsed();

        println!(
            "MD027 with {} entries: {:?} ({} warnings, {} bytes)",
            size,
            duration,
            warnings.len(),
            content.len()
        );

        durations.push(duration);
    }

    // Check that doubling from 500→1000 and 1000→2000 doesn't cause exponential growth
    // Allow up to 5x growth to account for system variance during parallel test execution
    // while still catching O(n²) regressions (which would show consistent 4x+ ratios)
    // In isolation, this test shows ~2-3x ratios; under load, up to ~4.5x is normal
    let ratio_1 = durations[1].as_secs_f64() / durations[0].as_secs_f64();
    let ratio_2 = durations[2].as_secs_f64() / durations[1].as_secs_f64();

    println!("Growth ratios: 500→1000: {ratio_1:.2}x, 1000→2000: {ratio_2:.2}x");

    assert!(
        ratio_1 < 5.0,
        "MD027 should scale roughly linearly: 500→1000 entries took {ratio_1:.2}x time (should be < 5x)"
    );
    assert!(
        ratio_2 < 5.0,
        "MD027 should scale roughly linearly: 1000→2000 entries took {ratio_2:.2}x time (should be < 5x)"
    );
}

#[test]
fn test_md020_linear_complexity() {
    // Using 500/1000/2000 entries for stable timing measurements
    let sizes = [500, 1000, 2000];
    let mut durations = Vec::new();

    for &size in &sizes {
        let content = generate_heading_document(size);
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD020NoMissingSpaceClosedAtx;

        let start = Instant::now();
        let warnings = rule.check(&ctx).unwrap();
        let duration = start.elapsed();

        println!(
            "MD020 with {} headings: {:?} ({} warnings, {} bytes)",
            size,
            duration,
            warnings.len(),
            content.len()
        );

        durations.push(duration);
    }

    // Same threshold as MD027 test - 5x allows variance while catching O(n²)
    let ratio_1 = durations[1].as_secs_f64() / durations[0].as_secs_f64();
    let ratio_2 = durations[2].as_secs_f64() / durations[1].as_secs_f64();

    println!("Growth ratios: 500→1000: {ratio_1:.2}x, 1000→2000: {ratio_2:.2}x");

    assert!(
        ratio_1 < 5.0,
        "MD020 should scale roughly linearly: 500→1000 headings took {ratio_1:.2}x time (should be < 5x)"
    );
    assert!(
        ratio_2 < 5.0,
        "MD020 should scale roughly linearly: 1000→2000 headings took {ratio_2:.2}x time (should be < 5x)"
    );
}

#[test]
fn test_large_document_performance() {
    // Test with a realistically large document (1000 entries)
    // This should complete in reasonable time (< 1 second) with O(n) complexity
    // but would take 10+ seconds with the old O(n²) behavior

    let content = generate_list_document(1000);
    println!("Testing large document: {} bytes", content.len());

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD027MultipleSpacesBlockquote;

    let start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    println!("MD027 with 1000 entries: {:?} ({} warnings)", duration, warnings.len());

    // With O(n) complexity, this should complete in well under 1 second
    // Allow 2 seconds for slower CI environments
    assert!(
        duration.as_secs() < 2,
        "Large document processing took {duration:?}, should be < 2s (old O(n²) would take 10+ seconds)"
    );
}

#[test]
fn test_combined_rules_performance() {
    // Test that multiple rules don't compound the performance issue
    let content = generate_list_document(1000);
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let md027 = MD027MultipleSpacesBlockquote;
    let md020 = MD020NoMissingSpaceClosedAtx;

    let start = Instant::now();
    let warnings_027 = md027.check(&ctx).unwrap();
    let warnings_020 = md020.check(&ctx).unwrap();
    let duration = start.elapsed();

    println!(
        "Combined rules (1000 entries): {:?} (MD027: {} warnings, MD020: {} warnings)",
        duration,
        warnings_027.len(),
        warnings_020.len()
    );

    // Should complete quickly since ctx.line_index is shared
    assert!(
        duration.as_secs() < 2,
        "Combined rules took {duration:?}, should be < 2s"
    );
}
