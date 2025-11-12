use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::Rng;
use rand::rng;
use rumdl_lib::utils::range_utils::LineIndex;

// Naive implementation that doesn't use the optimized function
fn naive_line_col_to_byte_range(content: &str, line: usize, column: usize) -> std::ops::Range<usize> {
    let lines: Vec<&str> = content.lines().collect();

    // Handle out of bounds
    if line == 0 || line > lines.len() {
        return content.len()..content.len();
    }

    // Manually iterate through each line to find the byte position
    let mut current_pos = 0;
    for i in 0..(line - 1) {
        if i < lines.len() {
            current_pos += lines[i].len() + 1; // +1 for newline
        }
    }

    // Adjust column
    let current_line = lines[line - 1];
    let col = if column == 0 {
        1
    } else if column > current_line.len() + 1 {
        current_line.len() + 1
    } else {
        column
    };

    let start = current_pos + col - 1;
    let safe_start = std::cmp::min(start, content.len());

    safe_start..safe_start
}

fn generate_test_content(line_count: usize) -> String {
    (0..line_count)
        .map(|i| format!("Line {i}: This is a test line with some content."))
        .collect::<Vec<String>>()
        .join("\n")
}

fn bench_range_utils(c: &mut Criterion) {
    let line_count = 10_000;
    let content = generate_test_content(line_count);

    // New cached implementation
    let line_index = LineIndex::new(&content);
    c.bench_function("cached_line_col_to_byte_range", |b| {
        b.iter(|| {
            for line in [1, 100, 1000, 5000, 9999].iter() {
                for col in [1, 10, 20, 40].iter() {
                    black_box(line_index.line_col_to_byte_range(*line, *col));
                }
            }
        })
    });

    // Benchmark the naive implementation
    c.bench_function("naive_line_col_to_byte_range", |b| {
        b.iter(|| {
            // Access the same positions
            for line in [1, 100, 1000, 5000, 9999].iter() {
                for col in [1, 10, 20, 40].iter() {
                    black_box(naive_line_col_to_byte_range(black_box(&content), *line, *col));
                }
            }
        })
    });

    // Benchmark with random accesses
    let mut rng = rng();

    c.bench_function("random_access_optimized", |b| {
        b.iter(|| {
            for _ in 0..20 {
                let line = rng.random_range(1..=line_count);
                let col = rng.random_range(1..=40);
                black_box(line_index.line_col_to_byte_range(line, col));
            }
        })
    });

    c.bench_function("random_access_naive", |b| {
        b.iter(|| {
            for _ in 0..20 {
                let line = rng.random_range(1..=line_count);
                let col = rng.random_range(1..=40);
                black_box(naive_line_col_to_byte_range(black_box(&content), line, col));
            }
        })
    });
}

criterion_group!(benches, bench_range_utils);
criterion_main!(benches);
