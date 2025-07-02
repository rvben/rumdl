use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::*;

/// Generate test content with common markdown issues
fn generate_test_content() -> String {
    let mut content = String::with_capacity(50_000);

    for i in 0..500 {
        // MD009 - Trailing spaces (very common)
        content.push_str(&format!("Line {i} with trailing spaces   \n"));

        // MD012 - Multiple blank lines
        if i % 10 == 0 {
            content.push_str("\n\n\n");
        }

        // MD018 - No space after hash
        if i % 15 == 0 {
            content.push_str(&format!("#Heading without space {i}\n\n"));
        }

        // MD026 - Trailing punctuation
        if i % 20 == 0 {
            content.push_str(&format!("# Heading with punctuation {i}!\n\n"));
        }

        // MD037 - Spaces inside emphasis
        if i % 8 == 0 {
            content.push_str(&format!("Text with * bad emphasis * number {i}\n"));
        }

        // Regular content
        content.push_str(&format!("Regular paragraph {i} with some content.\n\n"));
    }

    content
}

/// Benchmark the most commonly used fix rules
fn bench_common_fixes(c: &mut Criterion) {
    let content = generate_test_content();
    let ctx = LintContext::new(&content);

    // MD009 - Trailing spaces (most common fix)
    c.bench_function("MD009 trailing spaces fix", |b| {
        let rule = MD009TrailingSpaces::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD012 - Multiple blank lines
    c.bench_function("MD012 multiple blanks fix", |b| {
        let rule = MD012NoMultipleBlanks::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD018 - No space after hash
    c.bench_function("MD018 missing space atx fix", |b| {
        let rule = MD018NoMissingSpaceAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD026 - Trailing punctuation
    c.bench_function("MD026 trailing punctuation fix", |b| {
        let rule = MD026NoTrailingPunctuation::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD037 - Spaces inside emphasis
    c.bench_function("MD037 emphasis spaces fix", |b| {
        let rule = MD037NoSpaceInEmphasis;
        b.iter(|| rule.fix(black_box(&ctx)))
    });
}

/// Benchmark string manipulation approaches
fn bench_string_approaches(c: &mut Criterion) {
    let content = "Line with trailing spaces   \n".repeat(1000);

    // Approach 1: trim_end + collect
    c.bench_function("trim_end_collect", |b| {
        b.iter(|| {
            content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n")
        })
    });

    // Approach 2: manual iteration with capacity
    c.bench_function("manual_with_capacity", |b| {
        b.iter(|| {
            let mut result = String::with_capacity(content.len());
            for line in content.lines() {
                result.push_str(line.trim_end());
                result.push('\n');
            }
            result
        })
    });

    // Approach 3: regex replace
    c.bench_function("regex_replace", |b| {
        use regex::Regex;
        let re = Regex::new(r" +$").unwrap();
        b.iter(|| re.replace_all(black_box(&content), "").to_string())
    });
}

/// Test fix vs check performance ratio
fn bench_fix_vs_check(c: &mut Criterion) {
    let content = generate_test_content();
    let ctx = LintContext::new(&content);

    let rule = MD009TrailingSpaces::default();

    c.bench_function("MD009 check", |b| b.iter(|| rule.check(black_box(&ctx))));

    c.bench_function("MD009 fix", |b| b.iter(|| rule.fix(black_box(&ctx))));
}

criterion_group!(benches, bench_common_fixes, bench_string_approaches, bench_fix_vs_check);
criterion_main!(benches);
