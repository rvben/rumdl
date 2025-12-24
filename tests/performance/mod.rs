/// Performance tests using real-world markdown documents
mod fixtures;

use fixtures::{Fixture, FIXTURES};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::rule::Rule;
use std::time::Instant;

/// Performance test configuration
const PERF_TIMEOUT_MS: u128 = 5000; // 5 seconds max per rule

/// Test all rules against real-world documents
#[test]
#[ignore] // Run with --ignored or via CI performance workflow
fn test_real_world_performance() {
    println!("\n=== Real-World Performance Test ===\n");

    for fixture in FIXTURES {
        println!("Testing with: {} - {}", fixture.name, fixture.description);

        let content = match fixture.download() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  ⚠ Skipping (download failed): {}", e);
                continue;
            }
        };

        println!("  Document size: {} bytes", content.len());

        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

        // Test a representative sample of rules
        test_rule(&ctx, "MD001", || {
            use rumdl_lib::MD001HeadingIncrement::default();
            MD001HeadingIncrement.check(&ctx)
        });

        test_rule(&ctx, "MD003", || {
            use rumdl_lib::MD003HeadingStyle;
            MD003HeadingStyle::default().check(&ctx)
        });

        test_rule(&ctx, "MD013", || {
            use rumdl_lib::MD013LineTooLong;
            MD013LineTooLong::default().check(&ctx)
        });

        test_rule(&ctx, "MD033", || {
            use rumdl_lib::MD033NoInlineHtml;
            MD033NoInlineHtml::default().check(&ctx)
        });

        test_rule(&ctx, "MD034", || {
            use rumdl_lib::MD034BareUrls;
            MD034BareUrls.check(&ctx)
        });

        test_rule(&ctx, "MD053", || {
            use rumdl_lib::MD053LinkImageReferenceDefinitions;
            MD053LinkImageReferenceDefinitions::default().check(&ctx)
        });

        println!();
    }
}

/// Test a single rule with timing and assertions
fn test_rule<F>(ctx: &LintContext, rule_name: &str, check_fn: F)
where
    F: FnOnce() -> Result<Vec<rumdl_lib::Warning>, Box<dyn std::error::Error>>,
{
    let start = Instant::now();
    let result = check_fn();
    let duration = start.elapsed();

    match result {
        Ok(warnings) => {
            let duration_ms = duration.as_millis();
            println!(
                "  {} - {:4}ms - {} warnings",
                rule_name,
                duration_ms,
                warnings.len()
            );

            // Assert performance is reasonable
            if duration_ms > PERF_TIMEOUT_MS {
                panic!(
                    "{} took {}ms (> {}ms timeout)",
                    rule_name, duration_ms, PERF_TIMEOUT_MS
                );
            }
        }
        Err(e) => {
            eprintln!("  {} - ERROR: {}", rule_name, e);
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn test_fixture_cache_reuse() {
        // Test that caching works by downloading twice
        let fixture = &FIXTURES[0];

        // First download
        let start1 = Instant::now();
        let content1 = fixture.download();
        let duration1 = start1.elapsed();

        if content1.is_err() {
            println!("Skipping cache test - network unavailable");
            return;
        }

        // Second download (should be from cache)
        let start2 = Instant::now();
        let content2 = fixture.download();
        let duration2 = start2.elapsed();

        assert!(content2.is_ok());
        assert_eq!(content1.unwrap(), content2.unwrap());

        // Cache should be significantly faster
        println!("First: {:?}, Second: {:?}", duration1, duration2);
        // Note: Don't assert timing in tests as it's environment-dependent
    }
}
