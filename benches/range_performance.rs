use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rumdl::rule::Rule;
use rumdl::rules::md053_link_image_reference_definitions::MD053LinkImageReferenceDefinitions;

fn create_test_content() -> String {
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
        content.push_str(&format!("This is a paragraph with a [link][ref{}] reference.\n", i));
    }
    
    content
}

fn bench_md053_legacy(c: &mut Criterion) {
    let content = create_test_content();
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Legacy (line/col)", |b| {
        b.iter(|| {
            let warnings = rule.check(black_box(&content)).unwrap();
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
    let content = create_test_content();
    let rule = MD053LinkImageReferenceDefinitions::default();

    c.bench_function("MD053 Byte Ranges", |b| {
        b.iter(|| {
            rule.check(black_box(&content)).unwrap();
        })
    });
}

criterion_group!(benches, bench_md053_legacy, bench_md053_range_based);
criterion_main!(benches);
