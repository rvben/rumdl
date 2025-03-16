use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rumdl::rules::md053_link_image_reference_definitions::MD053LinkImageReferenceDefinitions;
use rumdl::utils::range_utils::line_col_to_byte_range;

fn bench_md053_legacy(c: &mut Criterion) {
    let content = include_str!("../tests/fixtures/md053_large.md");
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);

    c.bench_function("MD053 Legacy (line/col)", |b| {
        b.iter(|| {
            let warnings = rule.check(black_box(content)).unwrap();
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
    let content = include_str!("../tests/fixtures/md053_large.md");
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);

    c.bench_function("MD053 Byte Ranges", |b| {
        b.iter(|| {
            rule.check(black_box(content)).unwrap();
        })
    });
}

criterion_group!(benches, bench_md053_legacy, bench_md053_range_based);
criterion_main!(benches);
