//! Comprehensive algorithmic complexity tests for all line-processing rules
//!
//! These tests verify that rules maintain O(n) complexity by measuring how
//! execution time scales with input size. They catch O(n²) regressions like
//! issue #148 where MD007's `has_mixed_list_nesting()` scanned the full
//! document per line.
//!
//! ## Key Design Principles
//!
//! 1. **Test growth ratios, not absolute times** - Avoids flaky CI failures
//! 2. **Warm-up run + median** - Statistical rigor, reduces noise
//! 3. **Large input sizes** - 500/1000/2000 entries minimizes jitter impact
//! 4. **6x threshold** - Allows variance while catching O(n²) (which shows 4x+)
//!
//! ## When These Tests Run
//!
//! These tests run in the `performance` profile with serial execution to
//! minimize system noise. They are excluded from `ci` and `dev` profiles.
//!
//! To run manually:
//! ```sh
//! cargo nextest run --profile performance -E 'test(linear_complexity)'
//! # Or use the make target:
//! make test-complexity
//! ```

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_block_utils::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::*;
use std::time::{Duration, Instant};

// =============================================================================
// Measurement Infrastructure
// =============================================================================

/// Run with warm-up and multiple iterations for stable measurements
fn measure_rule_time<R: Rule>(rule: &R, content: &str, iterations: usize) -> Duration {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Warm-up run (discard - avoids cold cache artifacts)
    let _ = rule.check(&ctx);

    // Collect multiple measurements
    let mut times: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            let _ = rule.check(&ctx);
            start.elapsed()
        })
        .collect();

    // Use median (robust to outliers from system jitter)
    times.sort();
    times[iterations / 2]
}

/// Measure LintContext construction time
fn measure_context_time(content: &str, iterations: usize) -> Duration {
    // Warm-up
    let _ = LintContext::new(content, MarkdownFlavor::Standard, None);

    let mut times: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            let _ = LintContext::new(content, MarkdownFlavor::Standard, None);
            start.elapsed()
        })
        .collect();

    times.sort();
    times[iterations / 2]
}

/// Assert linear complexity by checking growth ratios
fn assert_linear_complexity(name: &str, durations: &[Duration], threshold: f64) {
    assert!(durations.len() >= 3, "Need at least 3 measurements for ratio check");

    let ratio_1 = durations[1].as_secs_f64() / durations[0].as_secs_f64();
    let ratio_2 = durations[2].as_secs_f64() / durations[1].as_secs_f64();

    println!("{name} scaling: 500->1000: {ratio_1:.2}x, 1000->2000: {ratio_2:.2}x");

    assert!(
        ratio_1 < threshold,
        "{name} shows non-linear scaling: 500->1000 took {ratio_1:.2}x (threshold: {threshold}x)"
    );
    assert!(
        ratio_2 < threshold,
        "{name} shows non-linear scaling: 1000->2000 took {ratio_2:.2}x (threshold: {threshold}x)"
    );
}

// =============================================================================
// Content Generators
// =============================================================================

/// Generate document with nested lists (tests MD004, MD005, MD007, MD030, MD032)
fn generate_list_document(num_entries: usize) -> String {
    let mut content = String::with_capacity(num_entries * 100);
    content.push_str("# List Document\n\n");

    for i in 0..num_entries {
        // Mix of ordered and unordered lists with varying nesting
        if i % 3 == 0 {
            content.push_str(&format!("- Item {i}\n"));
            content.push_str(&format!("  - Nested item {i}.1\n"));
            content.push_str(&format!("    - Deeply nested {i}.1.1\n"));
        } else if i % 3 == 1 {
            content.push_str(&format!("1. Ordered item {i}\n"));
            content.push_str(&format!("   1. Nested ordered {i}.1\n"));
        } else {
            content.push_str(&format!("* Unordered {i}\n"));
            content.push_str(&format!("  * Sub-item {i}.1\n"));
        }
    }

    content.push_str("\nEnd of lists.\n");
    content
}

/// Generate document with many headings (tests MD001, MD003, MD018-MD023, MD025, MD026)
fn generate_heading_document(num_headings: usize) -> String {
    let mut content = String::with_capacity(num_headings * 60);
    content.push_str("# Main Title\n\n");

    for i in 1..num_headings {
        let level = (i % 5) + 1; // Cycle through h1-h5
        let hashes = "#".repeat(level);

        // Mix of ATX styles
        if i % 4 == 0 {
            content.push_str(&format!("{hashes} Heading {i} {hashes}\n\n"));
        } else if i % 4 == 1 {
            content.push_str(&format!("{hashes} Heading {i}\n\n"));
        } else if i % 4 == 2 {
            // Closed ATX with potential issues
            content.push_str(&format!("{hashes}  Heading {i}  {hashes}\n\n"));
        } else {
            content.push_str(&format!("{hashes} Heading {i}:\n\n"));
        }

        content.push_str("Some paragraph content.\n\n");
    }

    content
}

/// Generate document with paragraphs and emphasis (tests MD013, MD036, MD037, MD049, MD050, MD064)
fn generate_paragraph_document(num_paragraphs: usize) -> String {
    let mut content = String::with_capacity(num_paragraphs * 200);
    content.push_str("# Text Document\n\n");

    for i in 0..num_paragraphs {
        // Vary emphasis styles
        if i % 4 == 0 {
            content.push_str(&format!(
                "This is paragraph {i} with *emphasis* and **strong** text.\n\n"
            ));
        } else if i % 4 == 1 {
            content.push_str(&format!(
                "Paragraph {i} uses _underscores_ for _emphasis_ and __strong__.\n\n"
            ));
        } else if i % 4 == 2 {
            // Potential MD036 trigger (emphasis-only line)
            content.push_str(&format!("**Bold heading style {i}**\n\n"));
            content.push_str("Normal content follows.\n\n");
        } else {
            // Long line for MD013
            content.push_str(&format!(
                "This is a longer paragraph number {i} that contains multiple words and might trigger line length checks if it exceeds the configured maximum line length threshold.\n\n"
            ));
        }
    }

    content
}

/// Generate document with links (tests MD034, MD039, MD042, MD051-MD054, MD059, MD062)
fn generate_link_document(num_links: usize) -> String {
    let mut content = String::with_capacity(num_links * 100);
    content.push_str("# Links Document\n\n");

    for i in 0..num_links {
        if i % 5 == 0 {
            // Standard link
            content.push_str(&format!("- [Link {i}](https://example.com/{i})\n"));
        } else if i % 5 == 1 {
            // Reference link
            content.push_str(&format!("- [Link {i}][ref{i}]\n"));
        } else if i % 5 == 2 {
            // Image
            content.push_str(&format!("- ![Image {i}](https://example.com/img{i}.png)\n"));
        } else if i % 5 == 3 {
            // Bare URL (MD034)
            content.push_str(&format!("- https://example.com/bare{i}\n"));
        } else {
            // Fragment link
            content.push_str(&format!("- [Section {i}](#section-{i})\n"));
        }
    }

    // Add reference definitions
    content.push('\n');
    for i in 0..num_links {
        if i % 5 == 1 {
            content.push_str(&format!("[ref{i}]: https://example.com/ref{i}\n"));
        }
    }

    // Add target sections for fragment links
    content.push('\n');
    for i in 0..num_links {
        if i % 5 == 4 {
            content.push_str(&format!("## Section {i}\n\nContent for section {i}.\n\n"));
        }
    }

    content
}

/// Generate document with code blocks (tests MD014, MD031, MD038, MD040, MD046, MD048)
fn generate_code_document(num_blocks: usize) -> String {
    let mut content = String::with_capacity(num_blocks * 150);
    content.push_str("# Code Document\n\n");

    for i in 0..num_blocks {
        content.push_str(&format!("## Code Example {i}\n\n"));

        if i % 3 == 0 {
            // Fenced code with language
            content.push_str("```rust\n");
            content.push_str(&format!("fn example_{i}() {{\n"));
            content.push_str(&format!("    println!(\"Hello {i}\");\n"));
            content.push_str("}\n");
            content.push_str("```\n\n");
        } else if i % 3 == 1 {
            // Fenced code without language (MD040)
            content.push_str("```\n");
            content.push_str(&format!("$ echo \"Command {i}\"\n"));
            content.push_str(&format!("Output {i}\n"));
            content.push_str("```\n\n");
        } else {
            // Inline code (MD038)
            content.push_str(&format!(
                "Use the `function_{i}()` method with ` spaced ` arguments.\n\n"
            ));
        }
    }

    content
}

/// Generate mixed document for structural rules (tests MD012, MD041, MD047)
fn generate_mixed_document(num_sections: usize) -> String {
    let mut content = String::with_capacity(num_sections * 300);
    // Note: No leading heading to test MD041

    for i in 0..num_sections {
        content.push_str(&format!("## Section {i}\n\n"));

        // Content with varying blank lines (MD012)
        content.push_str(&format!("Paragraph one in section {i}.\n"));
        if i % 3 == 0 {
            content.push_str("\n\n\n"); // Multiple blanks
        } else {
            content.push('\n');
        }
        content.push_str(&format!("Paragraph two in section {i}.\n\n"));

        // List
        content.push_str(&format!("- Item {i}.1\n"));
        content.push_str(&format!("- Item {i}.2\n\n"));

        // Code
        content.push_str("```\n");
        content.push_str(&format!("code {i}\n"));
        content.push_str("```\n\n");
    }

    // MD047: Ensure trailing newline
    content.push_str("Final content.\n");
    content
}

/// Generate the EXACT document structure that caused issue #148
/// This is a "regression anchor" - if this test fails, issue #148 has regressed
fn generate_issue_148_document(num_entries: usize) -> String {
    let mut content = String::with_capacity(num_entries * 150);
    content.push_str("# Work Log\n\n");

    for i in 0..num_entries {
        content.push_str(&format!("- day-{i}: 2025-06-{:02}\n", (i % 28) + 1));
        content.push_str("  - task: 09:00-10:00\n");
        content.push_str(">  Extra space after marker\n"); // Triggers MD027
        content.push_str("    - fix: add field\n");
        content.push_str(&format!("    - fix: \"json_tag\": \"[{i}]\"\n"));
        content.push_str("    - fix: \"local_field\": [\"record_id\"]\n");
    }

    content
}

// =============================================================================
// LintContext Construction Tests
// =============================================================================

#[test]
fn test_lint_context_construction_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            println!("Testing LintContext with {size} sections ({} bytes)", content.len());
            measure_context_time(&content, iterations)
        })
        .collect();

    assert_linear_complexity("LintContext::new", &durations, 6.0);
}

// =============================================================================
// Issue #148 Regression Anchor
// =============================================================================

#[test]
fn test_issue_148_exact_pattern_linear_complexity() {
    // This test uses the EXACT document structure from issue #148
    // If this fails, we've reintroduced the O(n²) regression

    let sizes = [300, 600, 1200]; // Matches the ~890 row original report
    let iterations = 5;
    let rule = MD007ULIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_issue_148_document(size);
            println!(
                "Testing issue #148 pattern with {size} entries ({} bytes)",
                content.len()
            );
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // Extra strict threshold for this specific regression
    assert_linear_complexity("MD007 (issue #148 pattern)", &durations, 4.0);
}

// =============================================================================
// List Rules (MD004, MD005, MD007, MD030, MD032)
// =============================================================================

#[test]
fn test_md004_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD004UnorderedListStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD004", &durations, 6.0);
}

#[test]
fn test_md005_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD005ListIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD005", &durations, 6.0);
}

#[test]
fn test_md007_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD007ULIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD007", &durations, 6.0);
}

#[test]
fn test_md030_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD030ListMarkerSpace::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD030", &durations, 6.0);
}

#[test]
fn test_md032_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD032BlanksAroundLists::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD032", &durations, 6.0);
}

// =============================================================================
// Heading Rules (MD001, MD003, MD018-MD023, MD025, MD026)
// =============================================================================

#[test]
fn test_md001_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD001HeadingIncrement::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD001", &durations, 6.0);
}

#[test]
fn test_md003_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD003HeadingStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD003", &durations, 6.0);
}

#[test]
fn test_md018_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD018NoMissingSpaceAtx;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD018", &durations, 6.0);
}

#[test]
fn test_md019_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD019NoMultipleSpaceAtx;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD019", &durations, 6.0);
}

#[test]
fn test_md020_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD020NoMissingSpaceClosedAtx;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD020", &durations, 6.0);
}

#[test]
fn test_md022_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD022BlanksAroundHeadings::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD022", &durations, 6.0);
}

#[test]
fn test_md025_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD025SingleTitle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD025", &durations, 6.0);
}

#[test]
fn test_md026_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD026NoTrailingPunctuation::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD026", &durations, 6.0);
}

// =============================================================================
// Text Rules (MD013, MD036, MD037, MD049, MD050, MD064)
// =============================================================================

#[test]
fn test_md013_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD013LineLength::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD013", &durations, 6.0);
}

#[test]
fn test_md036_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD036NoEmphasisAsHeading::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD036", &durations, 6.0);
}

#[test]
fn test_md037_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD037NoSpaceInEmphasis;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD037", &durations, 6.0);
}

#[test]
fn test_md049_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD049EmphasisStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD049", &durations, 6.0);
}

#[test]
fn test_md050_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD050StrongStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD050", &durations, 6.0);
}

// =============================================================================
// Link Rules (MD034, MD039, MD042, MD051-MD054, MD059)
// =============================================================================

#[test]
fn test_md034_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD034NoBareUrls;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD034", &durations, 6.0);
}

#[test]
fn test_md039_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD039NoSpaceInLinks;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD039", &durations, 6.0);
}

#[test]
fn test_md042_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD042NoEmptyLinks::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD042", &durations, 6.0);
}

#[test]
fn test_md051_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD051LinkFragments::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD051", &durations, 6.0);
}

#[test]
fn test_md053_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD053LinkImageReferenceDefinitions::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD053", &durations, 6.0);
}

// =============================================================================
// Code Rules (MD014, MD031, MD038, MD040, MD046, MD048)
// =============================================================================

#[test]
fn test_md014_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD014CommandsShowOutput::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD014", &durations, 6.0);
}

#[test]
fn test_md031_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD031BlanksAroundFences::new(true);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD031", &durations, 6.0);
}

#[test]
fn test_md038_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD038NoSpaceInCode::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD038", &durations, 6.0);
}

#[test]
fn test_md040_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD040FencedCodeLanguage;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD040", &durations, 6.0);
}

#[test]
fn test_md046_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD046", &durations, 6.0);
}

#[test]
fn test_md048_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD048", &durations, 6.0);
}

// =============================================================================
// Structural Rules (MD012, MD041, MD047)
// =============================================================================

#[test]
fn test_md012_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD012NoMultipleBlanks::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD012", &durations, 6.0);
}

#[test]
fn test_md041_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD041FirstLineHeading::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD041", &durations, 6.0);
}

#[test]
fn test_md047_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD047SingleTrailingNewline;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD047", &durations, 6.0);
}
