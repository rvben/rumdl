/// Performance tests using real-world markdown documents
use super::perf_fixtures::FIXTURES;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{LintError, LintWarning, Rule};
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
                eprintln!("  Skipping (download failed): {}", e);
                continue;
            }
        };

        println!("  Document size: {} bytes", content.len());

        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

        test_rule(&ctx, "MD001", || {
            rumdl_lib::MD001HeadingIncrement::default().check(&ctx)
        });

        test_rule(&ctx, "MD003", || rumdl_lib::MD003HeadingStyle::default().check(&ctx));

        test_rule(&ctx, "MD013", || rumdl_lib::MD013LineLength::default().check(&ctx));

        test_rule(&ctx, "MD033", || rumdl_lib::MD033NoInlineHtml::default().check(&ctx));

        test_rule(&ctx, "MD034", || rumdl_lib::MD034NoBareUrls.check(&ctx));

        test_rule(&ctx, "MD053", || {
            rumdl_lib::MD053LinkImageReferenceDefinitions::default().check(&ctx)
        });

        println!();
    }
}

/// Test a single rule with timing and assertions
fn test_rule<F>(_ctx: &LintContext, rule_name: &str, check_fn: F)
where
    F: FnOnce() -> Result<Vec<LintWarning>, LintError>,
{
    let start = Instant::now();
    let result = check_fn();
    let duration = start.elapsed();

    match result {
        Ok(warnings) => {
            let duration_ms = duration.as_millis();
            println!("  {} - {:4}ms - {} warnings", rule_name, duration_ms, warnings.len());

            if duration_ms > PERF_TIMEOUT_MS {
                panic!("{} took {}ms (> {}ms timeout)", rule_name, duration_ms, PERF_TIMEOUT_MS);
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
        let fixture = &FIXTURES[0];

        let start1 = Instant::now();
        let content1 = fixture.download();
        let duration1 = start1.elapsed();

        if content1.is_err() {
            println!("Skipping cache test - network unavailable");
            return;
        }

        let start2 = Instant::now();
        let content2 = fixture.download();
        let duration2 = start2.elapsed();

        assert!(content2.is_ok());
        assert_eq!(content1.unwrap(), content2.unwrap());

        println!("First: {:?}, Second: {:?}", duration1, duration2);
    }
}
