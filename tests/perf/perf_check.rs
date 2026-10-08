use rumdl_lib::lint_context::LintContext;
use rumdl_lib::{MD033NoInlineHtml, MD037NoSpaceInEmphasis, MD053LinkImageReferenceDefinitions, rule::Rule};
use std::time::Instant;

#[test]
fn test_optimized_rules_performance() {
    // Generate a large markdown content with HTML and emphasis
    let mut content = String::with_capacity(100_000);
    for i in 0..1000 {
        content.push_str(&format!("Line {i} with <span>HTML</span> and *emphasis*\n"));
    }

    // Add reference definitions
    content.push_str("\n\n## Reference Definitions\n\n");
    for i in 0..200 {
        let label = if i < 100 {
            format!("ref{i}")
        } else {
            format!("unused{i}")
        };
        content.push_str(&format!("[{label}]: https://example.com/{label}\n"));
    }
    content.push('\n');
    for i in 0..100 {
        content.push_str(&format!("Here is a [link][ref{i}] to example {i}\n\n"));
    }

    println!("Generated test content of {} bytes", content.len());

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    // Check MD053 against the shared parsed context.
    let start_time = Instant::now();
    let reference_rule = MD053LinkImageReferenceDefinitions::default();
    let mut ref_warnings = reference_rule.check(&ctx).unwrap();
    let ref_duration = start_time.elapsed();
    println!(
        "MD053 Rule first check took: {:?}, found: {} issues",
        ref_duration,
        ref_warnings.len()
    );

    // Repeated checks must reuse context and return identical diagnostics.
    // A single elapsed-time comparison cannot establish a cache speedup:
    // scheduling and already-initialized shared caches can reverse the order.
    let definitions = ctx.reference_definitions().as_ptr();
    let links = ctx.links().as_ptr();
    let start = Instant::now();
    let mut ref_warnings_cached = reference_rule.check(&ctx).unwrap();
    let ref_cached_duration = start.elapsed();
    println!(
        "MD053 Rule repeated check took: {:?}, found: {} issues",
        ref_cached_duration,
        ref_warnings_cached.len()
    );

    // HashMap iteration order is not a diagnostic ordering contract.
    let sort_warnings = |warnings: &mut Vec<rumdl_lib::rule::LintWarning>| {
        warnings.sort_by(|a, b| (a.line, a.column, &a.message).cmp(&(b.line, b.column, &b.message)));
    };
    sort_warnings(&mut ref_warnings);
    sort_warnings(&mut ref_warnings_cached);
    assert_eq!(
        ref_warnings, ref_warnings_cached,
        "Repeated checks must return identical warnings"
    );
    assert_eq!(ref_warnings.len(), 100, "Should find every unused reference");
    assert_eq!(
        ctx.reference_definitions().as_ptr(),
        definitions,
        "Parsed definitions must be reused"
    );
    assert_eq!(ctx.links().as_ptr(), links, "Parsed links must be reused");
    assert_eq!(
        html_warnings.len(),
        1000,
        "Should detect 1000 opening HTML tags (MD033 only reports opening tags, not closing tags)"
    );
    assert_eq!(emphasis_warnings.len(), 0, "Should not have detected emphasis issues");
}
