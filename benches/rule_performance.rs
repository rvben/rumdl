use criterion::{Criterion, criterion_group, criterion_main};
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD013LineLength, MD033NoInlineHtml, MD034NoBareUrls, MD037NoSpaceInEmphasis, MD044ProperNames, MD050StrongStyle,
    MD051LinkFragments, MD053LinkImageReferenceDefinitions, MD054LinkImageStyle,
};
use std::hint::black_box;

fn make_ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::Standard, None)
}

/// Long generated paragraphs exercise construct lookups and diagnostic columns.
/// Keep the context warm to measure the rule; CLI comparisons include parsing.
fn bench_md034_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("md034_scaling");
    let rule = MD034NoBareUrls;
    for count in [16, 256, 4096, 16384] {
        for (name, unit, warnings_per_unit) in [
            ("formatted", "[link](https://example.com) <hello@example.com> ", 0),
            ("bare", "https://example.com hello@example.com ", 2),
            (
                "mixed_unicode",
                "é🦀 [link](https://example.com) https://bare.example hello@example.com ",
                2,
            ),
        ] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = make_ctx(&content);
            assert_eq!(rule.check(&ctx).unwrap().len(), count * warnings_per_unit);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

/// Benchmark MD013 rule on a large content with long lines
fn bench_md013(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Generate 100 lines each with 120 characters (above default line length limit)
    for i in 0..100 {
        content.push_str(&format!("Line {:03} with a very long text that exceeds the default line length limit of 80 characters. This line is exactly 120 characters.{}\n", i, " ".repeat(5)));
    }

    let rule = MD013LineLength::default();
    let ctx = make_ctx(&content);

    c.bench_function("MD013 check 100 long lines", |b| b.iter(|| rule.check(black_box(&ctx))));

    c.bench_function("MD013 fix 100 long lines", |b| b.iter(|| rule.fix(black_box(&ctx))));
}

/// Benchmark MD033 rule on a large content with HTML tags
fn bench_md033(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Generate 500 lines with HTML tags
    for i in 0..500 {
        content.push_str(&format!(
            "Line {i} with <span class=\"highlight\">HTML</span> and <div>nested <em>tags</em></div>\n"
        ));
    }

    let rule = MD033NoInlineHtml::default();
    let ctx = make_ctx(&content);

    c.bench_function("MD033 check 500 HTML tags", |b| b.iter(|| rule.check(black_box(&ctx))));
}

/// Repeated tags on one line exercise diagnostic end-column calculation.
fn bench_md033_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("md033_scaling");
    group.sample_size(10);
    let rule = MD033NoInlineHtml::default();
    for count in [128, 2048, 8192, 32768] {
        for (name, unit) in [
            ("ascii", "text <span>x</span> "),
            ("unicode", "é日本🙂 <span>x</span> "),
        ] {
            let content = unit.repeat(count);
            let ctx = make_ctx(&content);
            assert_eq!(rule.check(&ctx).unwrap().len(), count);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

/// Many permitted regions exercise lookups both inside and outside each region.
fn bench_md033_allowed_inside(c: &mut Criterion) {
    let mut group = c.benchmark_group("md033_allowed_inside");
    group.sample_size(10);
    let config = toml::from_str("allowed-inside = ['details', 'section']").unwrap();
    let rule = MD033NoInlineHtml::from_config_struct(config);
    for count in [128, 2048, 8192, 32768] {
        for (name, unit) in [
            ("siblings", "<details><b>inside</b></details><i>outside</i>\n"),
            (
                "nested",
                "<details><section><b>inside</b></section><i>inside</i></details><b>outside</b>\n",
            ),
        ] {
            let content = unit.repeat(count);
            let ctx = make_ctx(&content);
            assert_eq!(rule.check(&ctx).unwrap().len(), count);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

/// Benchmark MD037 rule on a large content with emphasis
fn bench_md037(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);
    // Add correct and incorrect emphasis usage (spaces around emphasis)
    for i in 0..500 {
        if i % 3 == 0 {
            // Incorrect: no spaces around emphasis
            content.push_str(&format!("Line {i} with*incorrect emphasis*markers\n"));
        } else if i % 3 == 1 {
            // Incorrect: spaces inside emphasis
            content.push_str(&format!("Line {i} with *incorrect emphasis *markers\n"));
        } else {
            // Correct: proper spaces around emphasis
            content.push_str(&format!("Line {i} with *correct emphasis* markers\n"));
        }
    }

    let rule = MD037NoSpaceInEmphasis;
    let ctx = make_ctx(&content);

    c.bench_function("MD037 check 500 emphasis markers", |b| {
        b.iter(|| rule.check(black_box(&ctx)))
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
    let incorrect_names = ["javascript", "typescript", "github", "vs code", "docker", "kubernetes"];

    for i in 0..500 {
        if i % 2 == 0 {
            let name_idx = i % proper_names.len();
            content.push_str(&format!("Line {} mentions {} correctly\n", i, proper_names[name_idx]));
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
    let ctx = make_ctx(&content);

    c.bench_function("MD044 check 500 proper name occurrences", |b| {
        b.iter(|| rule.check(black_box(&ctx)))
    });

    c.bench_function("MD044 fix 500 proper name occurrences", |b| {
        b.iter(|| rule.fix(black_box(&ctx)))
    });
}

/// Benchmark MD051 link fragments rule
fn bench_md051(c: &mut Criterion) {
    let mut content = String::with_capacity(50_000);

    // Add 100 headings
    for i in 1..101 {
        content.push_str(&format!("## Heading {i}\n\n"));
    }

    // Add 500 links, some with valid and some with invalid fragments
    for i in 0..500 {
        if i % 3 == 0 {
            // Valid link
            let heading_number = (i % 100) + 1;
            content.push_str(&format!("This is a [valid link](#heading-{heading_number})\n"));
        } else {
            // Invalid link
            content.push_str(&format!("This is an [invalid link](#non-existent-heading-{i})\n"));
        }
    }

    let rule = MD051LinkFragments::new();
    let ctx = make_ctx(&content);

    c.bench_function("MD051 check 500 link fragments", |b| {
        b.iter(|| rule.check(black_box(&ctx)))
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
            content.push_str(&format!("[ref{i}]: https://example.com/ref{i}\n"));
        } else {
            // Unused references
            content.push_str(&format!("[unused{i}]: https://example.com/unused{i}\n"));
        }
    }

    // Add reference usages
    content.push_str("\n## Content with References\n\n");
    for i in 0..100 {
        content.push_str(&format!("This is a paragraph with a [link][ref{i}] reference.\n"));
    }

    let rule = MD053LinkImageReferenceDefinitions::default();
    let ctx = make_ctx(&content);

    // Cold cache: fresh rule instance each iteration, pre-built context
    c.bench_function("MD053 check cold cache", |b| {
        b.iter_with_setup(MD053LinkImageReferenceDefinitions::default, |r| {
            r.check(black_box(&ctx))
        })
    });

    // Warm cache: prime the rule's internal cache once, then measure
    let primed_rule = rule.clone();
    let _ = primed_rule.check(&ctx);
    c.bench_function("MD053 check warm cache", |b| {
        b.iter(|| primed_rule.check(black_box(&ctx)))
    });

    c.bench_function("MD053 fix unused references", |b| b.iter(|| rule.fix(black_box(&ctx))));
}

/// Separate lines keep column calculation cheap while exercising fix conflicts.
fn bench_md054_fix_planning(c: &mut Criterion) {
    let mut group = c.benchmark_group("md054_fix_planning");
    group.sample_size(10);
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
    for count in [128, 1024, 4096] {
        for (name, unit, warnings_per_unit) in [
            ("links", "[label](https://example.com)\n", 1),
            (
                "nested",
                "[![alt](image.png)](https://example.com) [plain](https://example.com)\n",
                3,
            ),
        ] {
            let content = unit.repeat(count);
            let ctx = make_ctx(&content);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), count * warnings_per_unit);
            assert_eq!(warnings.iter().filter(|warning| warning.fix.is_some()).count(), count);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

/// Repeated diagnostics on one line must reuse the parser's character columns.
fn bench_md054_long_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("md054_long_lines");
    group.sample_size(10);
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
    for count in [128, 1024, 4096, 16384] {
        for (name, unit) in [
            ("ascii", "[label](target) ![alt](image.png) "),
            ("unicode", "é日本🙂 [标签](target) ![图](image.png) "),
        ] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = make_ctx(&content);
            assert_eq!(rule.check(&ctx).unwrap().len(), count * 2);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

/// Consistency detection and diagnostics share indexed skip/position facts.
fn bench_md050_long_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("md050_long_lines");
    group.sample_size(10);
    let rule = MD050StrongStyle::default();
    for count in [128, 1024, 4096] {
        for (name, unit, flavor) in [
            ("ascii", "**good** __other__ ", MarkdownFlavor::Standard),
            ("unicode", "é日本🙂 **good** __other__ ", MarkdownFlavor::Standard),
            (
                "code_markup",
                "`**code**` :smile: **good** __other__ ",
                MarkdownFlavor::Standard,
            ),
            (
                "mkdocs",
                "++ctrl+alt++ ^^insert^^ ~sub~ ==mark== :smile: `**code**` **good** __other__ ",
                MarkdownFlavor::MkDocs,
            ),
            ("math", "$x$ **good** __other__ ", MarkdownFlavor::Standard),
        ] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), count);
            group.bench_function(format!("{name}/{count}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_md034_scaling,
    bench_md013,
    bench_md033,
    bench_md033_scaling,
    bench_md033_allowed_inside,
    bench_md054_fix_planning,
    bench_md054_long_lines,
    bench_md050_long_lines,
    bench_md037,
    bench_md044,
    bench_md051,
    bench_md053
);
criterion_main!(benches);
