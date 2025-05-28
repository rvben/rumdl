use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::rng;
use rand::Rng;
use rumdl::rule::Rule;
use rumdl::LintContext;
use rumdl::MD053LinkImageReferenceDefinitions;

fn create_test_content(size: usize, ratio: f64) -> String {
    let mut content = String::with_capacity(size);
    let mut rng = rng();
    let line_length = 80;
    let words_per_line = 10;
    let word_length = line_length / words_per_line;

    // Add some references at the beginning
    content.push_str("# Test Document with References\n\n");

    for i in 0..100 {
        if rng.random::<f64>() < 0.5 {
            // 50% chance to add a reference
            if rng.random::<f64>() < 0.7 {
                // 70% chance to be a link, 30% to be an image
                content.push_str(&format!("[Link {}][ref-{}]\n", i, i));
            } else {
                content.push_str(&format!("![Image {}][ref-{}]\n", i, i));
            }
        }
    }

    content.push('\n');

    // Create paragraphs
    let num_paragraphs = size / (line_length * 5);
    for _ in 0..num_paragraphs {
        let num_lines = rng.random_range(3..7);
        for _ in 0..num_lines {
            for _ in 0..words_per_line {
                let word_size = rng.random_range(3..word_length);
                for _ in 0..word_size {
                    let c = (b'a' + rng.random_range(0..26)) as char;
                    content.push(c);
                }
                content.push(' ');
            }
            content.push('\n');
        }
        content.push('\n');
    }

    // Add reference definitions at the end
    for i in 0..100 {
        if rng.random::<f64>() < ratio {
            // Only add a portion based on ratio
            content.push_str(&format!("[ref-{}]: https://example.com/ref-{}\n", i, i));
        }
    }

    content
}

fn create_test_content_legacy() -> String {
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

    content
}

fn bench_md053_legacy(c: &mut Criterion) {
    let content = create_test_content_legacy();
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Legacy (line/col)", |b| {
        b.iter(|| {
            let ctx = LintContext::new(black_box(&content));
            let warnings = rule.check(&ctx).unwrap();
            warnings.iter().for_each(|w| {
                if let Some(fix) = &w.fix {
                    // Simulate old line/column calculation overhead
                    let line = content[..fix.range.start].lines().count();
                    let col = fix.range.start - content[..fix.range.start].rfind('\n').unwrap_or(0);
                    let _ = (line, col); // Prevent optimization
                }
            });
        })
    });
}

fn bench_md053_range_based(c: &mut Criterion) {
    let content = create_test_content_legacy();
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Byte Ranges", |b| {
        b.iter(|| {
            let ctx = LintContext::new(black_box(&content));
            rule.check(&ctx).unwrap();
        })
    });
}

fn bench_md053_many_references(c: &mut Criterion) {
    let content = create_test_content(50000, 0.8); // 80% of references are defined
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Many References", |b| {
        b.iter(|| {
            let ctx = LintContext::new(&content);
            let _ = rule.check(&ctx);
        });
    });
}

fn bench_md053_fix_many_references(c: &mut Criterion) {
    let content = create_test_content(50000, 0.4); // Only 40% of refs defined, so lots of unused
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Fix Many References", |b| {
        b.iter(|| {
            let ctx = LintContext::new(&content);
            let _ = rule.fix(&ctx);
        });
    });
}

criterion_group!(
    benches,
    bench_md053_legacy,
    bench_md053_range_based,
    bench_md053_many_references,
    bench_md053_fix_many_references
);
criterion_main!(benches);
