use criterion::{Criterion, criterion_group, criterion_main};
use rumdl_lib::LintContext;
use rumdl_lib::MD053LinkImageReferenceDefinitions;
use rumdl_lib::rule::Rule;
use std::hint::black_box;

/// Build a deterministic test document with a predictable mix of link/image
/// references and plain paragraphs. `defined_ratio` controls what fraction of
/// the 100 referenced labels get a matching definition (the rest are unused).
fn create_test_content(size: usize, defined_ratio: f64) -> String {
    let mut content = String::with_capacity(size);
    let line_length = 80;
    let words_per_line = 10;
    let word_length = line_length / words_per_line;

    content.push_str("# Test Document with References\n\n");

    // Deterministic link/image reference usages: every other index, alternating link vs image.
    for i in 0..100 {
        if i % 2 == 0 {
            if i % 10 != 0 {
                content.push_str(&format!("[Link {i}][ref-{i}]\n"));
            } else {
                content.push_str(&format!("![Image {i}][ref-{i}]\n"));
            }
        }
    }

    content.push('\n');

    let num_paragraphs = size / (line_length * 5);
    for p in 0..num_paragraphs {
        let num_lines = 3 + p % 4;
        for l in 0..num_lines {
            for w in 0..words_per_line {
                let word_size = 3 + ((p + l + w) % (word_length.saturating_sub(3).max(1)));
                for c_idx in 0..word_size {
                    let c = (b'a' + ((p + l + w + c_idx) % 26) as u8) as char;
                    content.push(c);
                }
                content.push(' ');
            }
            content.push('\n');
        }
        content.push('\n');
    }

    // Deterministic reference definitions. Keep the first `threshold` labels;
    // matches the behavior of the original ratio-based sampling.
    let threshold = (100.0 * defined_ratio) as usize;
    for i in 0..threshold.min(100) {
        content.push_str(&format!("[ref-{i}]: https://example.com/ref-{i}\n"));
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

    content
}

fn bench_md053_legacy(c: &mut Criterion) {
    let content = create_test_content_legacy();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Legacy (line/col)", |b| {
        b.iter(|| {
            let warnings = rule.check(black_box(&ctx)).unwrap();
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
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Byte Ranges", |b| {
        b.iter(|| {
            rule.check(black_box(&ctx)).unwrap();
        })
    });
}

fn bench_md053_many_references(c: &mut Criterion) {
    let content = create_test_content(50000, 0.8); // 80% of references are defined
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Many References", |b| {
        b.iter(|| {
            let _ = rule.check(black_box(&ctx));
        });
    });
}

fn bench_md053_fix_many_references(c: &mut Criterion) {
    let content = create_test_content(50000, 0.4); // Only 40% of refs defined, so lots of unused
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Fix Many References", |b| {
        b.iter(|| {
            let _ = rule.fix(black_box(&ctx));
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
