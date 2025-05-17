use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rumdl::rule::Rule;
use rumdl::rules::{
    MD013LineLength, MD033NoInlineHtml, MD037NoSpaceInEmphasis, MD044ProperNames, MD051LinkFragments,
    MD053LinkImageReferenceDefinitions,
};

/// Benchmark MD013 rule on a large content with long lines
fn bench_md013(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Generate 100 lines each with 120 characters (above default line length limit)
    for i in 0..100 {
        content.push_str(&format!("Line {:03} with a very long text that exceeds the default line length limit of 80 characters. This line is exactly 120 characters.{}\n", i, " ".repeat(5)));
    }

    let rule = MD013LineLength::default();

    c.bench_function("MD013 check 100 long lines", |b| {
        b.iter(|| rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });

    c.bench_function("MD013 fix 100 long lines", |b| {
        b.iter(|| rule.fix(&rumdl::lint_context::LintContext::new(&content)))
    });
}

/// Benchmark MD033 rule on a large content with HTML tags
fn bench_md033(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Generate 500 lines with HTML tags
    for i in 0..500 {
        content.push_str(&format!("Line {} with <span class=\"highlight\">HTML</span> and <div>nested <em>tags</em></div>\n", i));
    }

    let rule = MD033NoInlineHtml::default();

    c.bench_function("MD033 check 500 HTML tags", |b| {
        b.iter(|| rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });
}

/// Benchmark MD037 rule on a large content with emphasis
fn bench_md037(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Add correct and incorrect emphasis usage (spaces around emphasis)
    for i in 0..500 {
        if i % 3 == 0 {
            // Incorrect: no spaces around emphasis
            content.push_str(&format!("Line {} with*incorrect emphasis*markers\n", i));
        } else if i % 3 == 1 {
            // Incorrect: spaces inside emphasis
            content.push_str(&format!("Line {} with *incorrect emphasis *markers\n", i));
        } else {
            // Correct: proper spaces around emphasis
            content.push_str(&format!("Line {} with *correct emphasis* markers\n", i));
        }
    }

    let rule = MD037NoSpaceInEmphasis;

    c.bench_function("MD037 check 500 emphasis markers", |b| {
        b.iter(|| rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });

    c.bench_function("MD037 fix 500 emphasis markers", |b| {
        b.iter(|| rule.fix(&rumdl::lint_context::LintContext::new(&content)))
    });
}

/// Benchmark MD044 proper names rule (regex-intensive)
fn bench_md044(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Generate content with names that should be consistently capitalized
    let proper_names = vec![
        "JavaScript".to_string(),
        "TypeScript".to_string(),
        "GitHub".to_string(),
        "VS Code".to_string(),
        "Docker".to_string(),
        "Kubernetes".to_string(),
    ];
    let incorrect_names = [
        "javascript",
        "typescript",
        "github",
        "vs code",
        "docker",
        "kubernetes",
    ];

    for i in 0..500 {
        if i % 2 == 0 {
            let name_idx = i % proper_names.len();
            content.push_str(&format!(
                "Line {} mentions {} correctly\n",
                i, proper_names[name_idx]
            ));
        } else {
            let name_idx = i % incorrect_names.len();
            content.push_str(&format!(
                "Line {} mentions {} incorrectly\n",
                i, incorrect_names[name_idx]
            ));
        }
    }

    // Create a rule with the proper names to check
    let rule = MD044ProperNames::new(proper_names, true); // true = exclude code blocks

    c.bench_function("MD044 check 500 proper name occurrences", |b| {
        b.iter(|| rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });

    c.bench_function("MD044 fix 500 proper name occurrences", |b| {
        b.iter(|| rule.fix(&rumdl::lint_context::LintContext::new(&content)))
    });
}

/// Benchmark MD051 link fragments rule
fn bench_md051(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);

    // Add 100 headings
    for i in 1..101 {
        content.push_str(&format!("## Heading {}\n\n", i));
    }

    // Add 500 links, some with valid and some with invalid fragments
    for i in 0..500 {
        if i % 3 == 0 {
            // Valid link
            let heading_number = (i % 100) + 1;
            content.push_str(&format!(
                "This is a [valid link](#heading-{})\n",
                heading_number
            ));
        } else {
            // Invalid link
            content.push_str(&format!(
                "This is an [invalid link](#non-existent-heading-{})\n",
                i
            ));
        }
    }

    let rule = MD051LinkFragments::new();

    c.bench_function("MD051 check 500 link fragments", |b| {
        b.iter(|| rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });
}

/// Benchmark MD053 link image reference definitions with caching
fn bench_md053(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);

    // Add reference definitions
    content.push_str("## Reference Definitions\n\n");
    for i in 0..200 {
        if i < 100 {
            // Used references
            content.push_str(&format!("[ref{}]: https://example.com/ref{}\n", i, i));
        } else {
            // Unused references
            content.push_str(&format!("[unused{}]: https://example.com/unused{}\n", i, i));
        }
    }

    // Add reference usages
    content.push_str("\n## Content with References\n\n");
    for i in 0..100 {
        content.push_str(&format!(
            "This is a paragraph with a [link][ref{}] reference.\n",
            i
        ));
    }

    let rule = MD053LinkImageReferenceDefinitions::default();

    // First call to benchmark cold cache
    c.bench_function("MD053 check cold cache", |b| {
        b.iter_with_setup(
            MD053LinkImageReferenceDefinitions::default, // Create a new instance for each iteration
            |r| r.check(&rumdl::lint_context::LintContext::new(&content)),
        )
    });

    // Using the same instance to benchmark warm cache
    c.bench_function("MD053 check warm cache", |b| {
        // First, prime the cache
        let primed_rule = rule.clone();
        let _ = primed_rule.check(&rumdl::lint_context::LintContext::new(&content));

        // Then benchmark with warm cache
        b.iter(|| primed_rule.check(&rumdl::lint_context::LintContext::new(&content)))
    });

    c.bench_function("MD053 fix unused references", |b| {
        b.iter(|| rule.fix(&rumdl::lint_context::LintContext::new(&content)))
    });
}

criterion_group!(
    benches,
    bench_md013,
    bench_md033,
    bench_md037,
    bench_md044,
    bench_md051,
    bench_md053
);
criterion_main!(benches);
