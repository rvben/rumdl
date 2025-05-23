use rumdl::lint_context::LintContext;
use rumdl::{
    rule::Rule, MD033NoInlineHtml, MD037NoSpaceInEmphasis, MD053LinkImageReferenceDefinitions,
};
use std::time::Instant;

#[test]
fn test_optimized_rules_performance() {
    // Generate a large markdown content with HTML and emphasis
    let mut content = String::with_capacity(100_000);
    for i in 0..1000 {
        content.push_str(&format!(
            "Line {} with <span>HTML</span> and *emphasis*\n",
            i
        ));
    }

    // Add reference definitions
    content.push_str("\n\n## Reference Definitions\n\n");
    for i in 0..200 {
        // 100 used references
        if i < 100 {
            content.push_str(&format!("[ref{}]: https://example.com/ref{}\n", i, i));
            // Add usages for these references
            content.push_str(&format!("Here is a [link][ref{}] to example {}\n", i, i));
        } else {
            // 100 unused references (should be detected by MD053)
            content.push_str(&format!("[unused{}]: https://example.com/unused{}\n", i, i));
        }
    }

    println!("Generated test content of {} bytes", content.len());

    let ctx = LintContext::new(&content);
    // Test MD033 (HTML rule)
    let html_rule = MD033NoInlineHtml::default();
    let start = Instant::now();
    let html_warnings = html_rule.check(&ctx).unwrap();
    let html_duration = start.elapsed();
    println!(
        "MD033 Rule check took: {:?}, found: {} issues",
        html_duration,
        html_warnings.len()
    );

    // Test MD037 (emphasis rule)
    let emphasis_rule = MD037NoSpaceInEmphasis;
    let start = Instant::now();
    let emphasis_warnings = emphasis_rule.check(&ctx).unwrap();
    let emphasis_duration = start.elapsed();
    println!(
        "MD037 Rule check took: {:?}, found: {} issues",
        emphasis_duration,
        emphasis_warnings.len()
    );

    // Test MD053 with caching (first run)
    let start_time = Instant::now();
    let reference_rule = MD053LinkImageReferenceDefinitions::default();
    let ref_warnings = reference_rule.check(&ctx).unwrap();
    let ref_duration = start_time.elapsed();
    println!(
        "MD053 Rule first check (cold cache) took: {:?}, found: {} issues",
        ref_duration,
        ref_warnings.len()
    );

    // Test MD053 with caching (second run - should be faster)
    let start = Instant::now();
    let ref_warnings_cached = reference_rule.check(&ctx).unwrap();
    let ref_cached_duration = start.elapsed();
    println!(
        "MD053 Rule second check (warm cache) took: {:?}, found: {} issues",
        ref_cached_duration,
        ref_warnings_cached.len()
    );

    // Verify results
    assert_eq!(
        ref_warnings.len(),
        ref_warnings_cached.len(),
        "Cached and non-cached runs should return the same number of warnings"
    );
    assert!(
        ref_warnings.len() <= 100,
        "Should find at most 100 unused references"
    );
    assert!(
        ref_cached_duration < ref_duration,
        "Cached run should be faster than initial run"
    );
    assert_eq!(
        html_warnings.len(),
        2000,
        "Should detect HTML tags (1000 <span> + 1000 </span> = 2000 total)"
    );
    assert_eq!(
        emphasis_warnings.len(),
        0,
        "Should not have detected emphasis issues"
    );
}
