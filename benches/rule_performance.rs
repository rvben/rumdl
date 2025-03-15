use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rumdl::rules::{MD015NoMissingSpaceAfterListMarker, MD053LinkImageReferenceDefinitions};

fn bench_md015(c: &mut Criterion) {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "-Item 1\n*Item 2\n+Item 3".repeat(1000);
    
    c.bench_function("MD015 fix 1000 items", |b| {
        b.iter(|| rule.fix(black_box(&content)))
    });
}

fn bench_md053(c: &mut Criterion) {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[ref]: url\n\nText [ref]".repeat(500);
    
    c.bench_function("MD053 check 500 refs", |b| {
        b.iter(|| rule.check(black_box(&content)))
    });
}

criterion_group!(benches, bench_md015, bench_md053);
criterion_main!(benches);
