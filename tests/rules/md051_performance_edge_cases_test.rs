// Performance edge case tests for MD051 rule
// These tests ensure the algorithm performs well under extreme conditions

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;
use std::time::{Duration, Instant};

/// Test regex performance with patterns that could cause catastrophic backtracking
#[test]
fn test_regex_catastrophic_backtracking_prevention() {
    let rule = MD051LinkFragments::new();

    // These patterns are known to cause exponential time in poorly written regex engines
    let catastrophic_patterns = vec![
        // Classic ReDoS pattern: nested quantifiers
        ("a".repeat(30) + "(a+)+$"),
        ("a".repeat(30) + "(a|a)*$"),
        ("a".repeat(30) + "(a*)*$"),
        // Alternation with repetition
        ("(a|b)*".repeat(10) + &"c".repeat(20)),
        // Complex nested groups
        ("((a*)*)*".to_string() + &"b".repeat(20)),
        // Mixed with actual markdown-like content
        ("*".repeat(50) + "text" + &"*".repeat(50)),
        ("`".repeat(30) + "code" + &"`".repeat(30)),
        ("[".repeat(20) + "link" + &"]".repeat(20)),
        // Unicode that might stress character classification
        ("🎉".repeat(50) + &"a".repeat(50)),
        // Combining characters that might stress normalization
        ("a\u{0301}".repeat(100)),
    ];

    for pattern in catastrophic_patterns {
        let content = format!("# {pattern}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        // Should complete quickly even with pathological regex patterns
        assert!(
            result.is_ok(),
            "ReDoS pattern caused panic: {}",
            pattern.chars().take(50).collect::<String>()
        );
        assert!(
            duration < Duration::from_secs(2),
            "Potential ReDoS vulnerability: pattern took {:?} (pattern: {})",
            duration,
            pattern.chars().take(50).collect::<String>()
        );

        println!(
            "✓ ReDoS pattern handled in {:?}: {}",
            duration,
            pattern.chars().take(20).collect::<String>()
        );
    }
}

/// Test memory allocation patterns under stress
#[test]
fn test_memory_allocation_under_stress() {
    let rule = MD051LinkFragments::new();

    // Test patterns that could cause memory allocation spikes
    let memory_stress_patterns = vec![
        // Very wide strings
        ("word ".repeat(10000), "Wide string 10K words"),
        // Many small allocations pattern
        ("a-".repeat(10000), "Alternating pattern 10K"),
        // Unicode that requires complex processing
        ("🎉".repeat(1000), "1K emoji"),
        ("中".repeat(5000), "5K CJK chars"),
        // Complex punctuation
        ("!@#$%^&*()".repeat(1000), "1K punctuation"),
        // Mixing patterns that might stress different code paths
        ("word🎉-中文".repeat(1000), "Mixed pattern 1K"),
        // Very long single "word" (no spaces)
        ("a".repeat(50000), "50K char single word"),
        // Many hyphens (stress hyphen processing)
        ("-".repeat(20000), "20K hyphens"),
        // Alternating hyphens and chars (stress context detection)
        ("a-".repeat(10000), "10K alternating"),
    ];

    for (pattern, description) in memory_stress_patterns {
        let content = format!("# {pattern}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Monitor timing and assume memory usage correlates
        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "Memory stress pattern failed: {description}");

        // Should not take excessive time (indicating memory thrashing)
        assert!(
            duration < Duration::from_secs(5),
            "Memory stress pattern too slow: {description} took {duration:?}"
        );

        println!("✓ Memory stress pattern: {description} in {duration:?}");
    }
}

/// Test algorithmic complexity scaling
#[test]
fn test_algorithmic_complexity_scaling() {
    let rule = MD051LinkFragments::new();

    // Test that processing time scales reasonably with input size
    let sizes = vec![100, 500, 1000, 5000, 10000];
    let mut previous_time = Duration::from_nanos(1);

    for size in sizes {
        let heading = "word ".repeat(size);
        let content = format!("# {heading}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "Scaling test failed at size {size}");

        // Check that time doesn't grow exponentially
        if previous_time > Duration::from_millis(1) {
            let time_ratio = duration.as_nanos() as f64 / previous_time.as_nanos() as f64;
            let size_ratio = if size == 100 {
                1.0
            } else {
                size as f64 / (size / 5) as f64 // Approximate ratio
            };

            // Time growth should be roughly linear (allow 2x overhead for complexity)
            assert!(
                time_ratio < size_ratio * 2.0,
                "Algorithm may have poor complexity: size {size} took {duration:?}, ratio {time_ratio:.2}"
            );
        }

        previous_time = duration;
        println!("✓ Size {size} processed in {duration:?}");
    }
}

/// Test concurrent access patterns (if applicable)
#[test]
fn test_concurrent_processing() {
    use std::sync::Arc;
    use std::thread;

    let rule = Arc::new(MD051LinkFragments::new());

    // Test concurrent processing of different documents
    let test_cases = vec![
        "# Heading One\n\n[Link](#heading-one)",
        "# Different Heading\n\n[Link](#different-heading)",
        "# Third Heading\n\n[Link](#third-heading)",
        "# Complex: (Pattern) & More\n\n[Link](#complex-pattern-more)",
        "# Unicode: Café & 中文\n\n[Link](#unicode-café-中文)",
    ];

    let handles: Vec<_> = test_cases
        .into_iter()
        .map(|content| {
            let rule = Arc::clone(&rule);
            let content = content.to_string();

            thread::spawn(move || {
                let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let start = Instant::now();
                let result = rule.check(&ctx);
                let duration = start.elapsed();

                (result.is_ok(), duration)
            })
        })
        .collect();

    // Wait for all threads and check results
    for (i, handle) in handles.into_iter().enumerate() {
        let (success, duration) = handle.join().expect("Thread panicked");

        assert!(success, "Concurrent test {i} failed");
        assert!(
            duration < Duration::from_secs(1),
            "Concurrent test {i} took too long: {duration:?}"
        );

        println!("✓ Concurrent test {i} completed in {duration:?}");
    }
}

/// Test performance with many links in single document
#[test]
fn test_many_links_performance() {
    let rule = MD051LinkFragments::new();

    // Create document with many links to test link processing performance
    let mut content = String::from("# Main Heading\n\n## Sub Heading\n\n");

    // Add many valid links
    for i in 0..1000 {
        content.push_str(&format!("[Link {i}](#main-heading)\n"));
    }

    // Add some invalid links
    for i in 0..100 {
        content.push_str(&format!("[Invalid {i}](#missing-{i})\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Many links test failed");

    let warnings = result.unwrap();

    // Should find the 100 invalid links
    assert_eq!(warnings.len(), 100, "Should find exactly 100 invalid links");

    // Should process efficiently
    assert!(
        duration < Duration::from_secs(3),
        "Many links processing too slow: {duration:?}"
    );

    println!("✓ Many links test: 1100 links processed in {duration:?}");
}

/// Test performance with deeply nested markdown structures
#[test]
fn test_deeply_nested_markdown_performance() {
    let rule = MD051LinkFragments::new();

    // Create deeply nested markdown that might stress the parser
    let mut content = String::from("# Main\n\n");

    // Create nested formatting that might stress the markdown stripping
    let nested_depth = 50;
    let mut opening = String::new();
    let mut closing = String::new();

    for i in 0..nested_depth {
        opening.push_str(match i % 4 {
            0 => "**",
            1 => "*",
            2 => "`",
            _ => "~~",
        });
        closing.insert_str(
            0,
            match i % 4 {
                0 => "**",
                1 => "*",
                2 => "`",
                _ => "~~",
            },
        );
    }

    let complex_heading = format!("# {opening}Deeply Nested{closing}");
    content.push_str(&complex_heading);
    content.push_str("\n\n[Link](#deeply-nested)\n");

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Deeply nested test failed");

    // Should handle complex nesting efficiently
    assert!(
        duration < Duration::from_secs(2),
        "Deeply nested processing too slow: {duration:?}"
    );

    println!("✓ Deeply nested test: depth {nested_depth} processed in {duration:?}");
}

/// Test performance with large documents containing many headings
#[test]
fn test_large_document_performance() {
    let rule = MD051LinkFragments::new();

    // Create a large document with many headings
    let mut content = String::new();
    let heading_count = 1000;

    // Add many headings
    for i in 0..heading_count {
        content.push_str(&format!("# Heading {i}\n\nSome content here.\n\n"));
        content.push_str(&format!("## Sub Heading {i}\n\nMore content.\n\n"));
    }

    // Add links to some headings
    for i in 0..100 {
        content.push_str(&format!("[Link {}](#heading-{})\n", i, i * 10));
    }

    // Add some invalid links
    for i in 0..50 {
        content.push_str(&format!("[Invalid {i}](#missing-{i})\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Large document test failed");

    let warnings = result.unwrap();

    // Should find the invalid links
    assert!(warnings.len() >= 50, "Should find invalid links");

    // Should process large document efficiently
    assert!(
        duration < Duration::from_secs(10),
        "Large document processing too slow: {:?} for {} headings",
        duration,
        heading_count * 2
    );

    println!(
        "✓ Large document: {} headings processed in {:?}",
        heading_count * 2,
        duration
    );
}

/// Test performance regression prevention
#[test]
fn test_performance_regression_prevention() {
    let rule = MD051LinkFragments::new();

    // Known patterns that have caused performance issues in the past
    let regression_patterns = vec![
        // Issue #39 related patterns that were slow
        (
            "cbrown --> sbrown: --unsafe-paths ".repeat(100),
            "Issue 39 pattern x100",
        ),
        // Patterns with many consecutive hyphens
        (format!("test{}end", "-".repeat(1000)), "1K consecutive hyphens"),
        // Complex punctuation clusters
        ("!@#$%^&*()".repeat(500), "Complex punctuation x500"),
        // Unicode intensive patterns
        ("🎉中文Café".repeat(1000), "Unicode intensive x1000"),
        // Mixed patterns that stress multiple code paths
        ("Test & More: (Complex) --> End".repeat(200), "Mixed complexity x200"),
    ];

    for (pattern, description) in regression_patterns {
        let content = format!("# {pattern}\n\n[Link](#test)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "Performance regression test failed: {description}");

        // Should complete in reasonable time
        assert!(
            duration < Duration::from_secs(3),
            "Performance regression detected: {description} took {duration:?}"
        );

        println!("✓ Performance regression test: {description} in {duration:?}");
    }
}

/// Benchmark fragment generation specifically
#[test]
fn test_fragment_generation_performance() {
    let rule = MD051LinkFragments::new();

    // Test fragment generation with various input types
    let long_heading = format!("Long {}", "word ".repeat(1000));
    let punctuation_heavy = format!("Punctuation {}", "!@#$%^&*() ".repeat(100));
    let hyphen_heavy = format!("Hyphens {}", "-".repeat(500));
    let mixed_complex = "Mixed 🎉 café & more --> test".repeat(100);
    let fragment_test_cases = [
        ("Simple Heading", "simple case"),
        ("Complex: (Pattern) & More!!!", "complex punctuation"),
        ("Unicode: Café & 中文 & Русский", "unicode case"),
        (&long_heading, "long heading"),
        (&punctuation_heavy, "punctuation heavy"),
        (&hyphen_heavy, "hyphen heavy"),
        (&mixed_complex, "mixed complexity"),
    ];

    for (heading, description) in fragment_test_cases.iter() {
        let content = format!("# {heading}\n\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Time the fragment generation (indirectly via rule check)
        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "Fragment generation test failed: {description}");

        // Fragment generation should be fast (allow 200ms for CI, locally ~10-50ms)
        assert!(
            duration < Duration::from_millis(200),
            "Fragment generation too slow: {description} took {duration:?}"
        );

        println!("✓ Fragment generation: {description} in {duration:?}");
    }
}

/// Test performance under memory pressure simulation
#[test]
fn test_performance_under_memory_pressure() {
    let rule = MD051LinkFragments::new();

    // Simulate memory pressure by processing many large documents sequentially
    let document_count = 10;
    let document_size = 1000; // words per document

    for doc_num in 0..document_count {
        let heading = "Large Document ".to_string() + &"word ".repeat(document_size);
        let content = format!("# {heading}\n\n[Link](#large-document)");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = Instant::now();
        let result = rule.check(&ctx);
        let duration = start.elapsed();

        assert!(result.is_ok(), "Memory pressure test failed at document {doc_num}");

        // Performance should remain consistent under memory pressure
        assert!(
            duration < Duration::from_secs(2),
            "Performance degraded under memory pressure: doc {doc_num} took {duration:?}"
        );

        if doc_num % 3 == 0 {
            println!("✓ Memory pressure test: document {doc_num} processed in {duration:?}");
        }
    }

    println!("✓ All {document_count} documents processed under memory pressure");
}
