use rumdl_lib::config::Config;
use rumdl_lib::parallel::{FileParallelProcessor, ParallelConfig, ParallelPerformanceComparison};
/// File-Level Parallel Processing Benchmark for rumdl
///
/// This binary tests file-level parallelization, which should be much more
/// effective than rule-level parallelization for markdown linting.
use rumdl_lib::performance::ContentGenerator;
use rumdl_lib::rules::all_rules;
use std::env;
use std::time::Instant;

fn main() {
    // Initialize logging
    env_logger::init();

    println!("📁 RUMDL File-Level Parallel Benchmark");
    println!("======================================\n");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let file_count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let content_size = args.get(2).map(|s| s.as_str()).unwrap_or("medium");
    let thread_count: Option<usize> = args.get(3).and_then(|s| s.parse().ok());

    // Create all rules with default configuration
    let config = Config::default();
    let rules = all_rules(&config);

    println!("📋 Benchmark Configuration:");
    println!("   Rules to test: {}", rules.len());
    println!("   File count: {file_count}");
    println!("   Content size: {content_size}");
    println!("   CPU cores: {}", num_cpus::get());
    if let Some(threads) = thread_count {
        println!("   Thread override: {threads}");
    }
    println!();

    // Generate test files
    let test_files = generate_test_files(file_count, content_size);
    let total_content_size: usize = test_files.iter().map(|(_, content)| content.len()).sum();
    let total_lines: usize = test_files.iter().map(|(_, content)| content.lines().count()).sum();

    println!("📄 Generated Test Files:");
    println!("   Total files: {}", test_files.len());
    println!("   Total content: {total_content_size} bytes ({total_lines} lines)");
    println!("   Average file size: {} bytes", total_content_size / test_files.len());
    println!();

    // Create file parallel processor
    let parallel_config = ParallelConfig {
        enabled: true,
        thread_count,
        min_file_count: 0, // Test all file counts
    };
    let file_processor = FileParallelProcessor::new(parallel_config);

    // Run comparison test
    run_file_parallel_comparison(&test_files, &rules, &file_processor);

    println!("✅ File-level parallel benchmark completed!");
}

fn generate_test_files(count: usize, size_type: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();

    for i in 0..count {
        let content = match size_type {
            "small" => ContentGenerator::small_content(),
            "medium" => ContentGenerator::medium_content(),
            "large" => ContentGenerator::large_content(),
            "huge" => ContentGenerator::huge_content(),
            _ => ContentGenerator::medium_content(),
        };

        let filename = format!("test_file_{}.md", i + 1);
        files.push((filename, content));
    }

    files
}

fn run_file_parallel_comparison(
    files: &[(String, String)],
    rules: &[Box<dyn rumdl_lib::rule::Rule>],
    file_processor: &FileParallelProcessor,
) {
    println!("🔄 Testing File-Level Parallel Processing:");

    // Warm up both approaches
    let _ = process_files_sequential(files, rules);
    let _ = file_processor.process_files(files, rules);

    // Test sequential processing (multiple runs for accuracy)
    let mut sequential_times = Vec::new();
    let mut sequential_total_warnings = 0;

    for _ in 0..3 {
        let start = Instant::now();
        let results = process_files_sequential(files, rules);
        sequential_times.push(start.elapsed());

        // Count total warnings across all files
        sequential_total_warnings = results
            .iter()
            .map(|(_, result)| result.as_ref().map(|w| w.len()).unwrap_or(0))
            .sum();
    }
    let sequential_time = sequential_times.iter().sum::<std::time::Duration>() / sequential_times.len() as u32;

    // Test parallel processing (multiple runs for accuracy)
    let mut parallel_times = Vec::new();
    let mut parallel_total_warnings = 0;

    for _ in 0..3 {
        let start = Instant::now();
        let results = file_processor.process_files(files, rules).unwrap();
        parallel_times.push(start.elapsed());

        // Count total warnings across all files
        parallel_total_warnings = results
            .iter()
            .map(|(_, result)| result.as_ref().map(|w| w.len()).unwrap_or(0))
            .sum();
    }
    let parallel_time = parallel_times.iter().sum::<std::time::Duration>() / parallel_times.len() as u32;

    // Verify correctness (warnings should be the same)
    if sequential_total_warnings != parallel_total_warnings {
        println!(
            "⚠️  WARNING: Different warning counts! Sequential: {sequential_total_warnings}, Parallel: {parallel_total_warnings}"
        );
    }

    // Create and print comparison
    let comparison = ParallelPerformanceComparison::new(sequential_time, parallel_time);

    println!("   Files processed: {}", files.len());
    println!(
        "   Sequential time: {:.3}ms ({} total warnings)",
        sequential_time.as_secs_f64() * 1000.0,
        sequential_total_warnings
    );
    println!(
        "   Parallel time: {:.3}ms ({} total warnings)",
        parallel_time.as_secs_f64() * 1000.0,
        parallel_total_warnings
    );
    println!("   Speedup factor: {:.2}x", comparison.speedup_factor);

    if comparison.speedup_factor > 1.0 {
        let improvement = (comparison.speedup_factor - 1.0) * 100.0;
        println!("   ✅ Performance improvement: {improvement:.1}%");
    } else {
        let degradation = (1.0 - comparison.speedup_factor) * 100.0;
        println!("   ❌ Performance degradation: {degradation:.1}%");
        if comparison.parallel_overhead > std::time::Duration::ZERO {
            println!(
                "   Parallel overhead: {:.3}ms",
                comparison.parallel_overhead.as_secs_f64() * 1000.0
            );
        }
    }

    // Calculate throughput
    let total_content_size: usize = files.iter().map(|(_, content)| content.len()).sum();
    let sequential_throughput = total_content_size as f64 / sequential_time.as_secs_f64() / 1024.0 / 1024.0; // MB/s
    let parallel_throughput = total_content_size as f64 / parallel_time.as_secs_f64() / 1024.0 / 1024.0; // MB/s

    println!();
    println!("📊 Throughput Analysis:");
    println!("   Sequential: {sequential_throughput:.2} MB/s");
    println!("   Parallel: {parallel_throughput:.2} MB/s");
    println!(
        "   Throughput improvement: {:.1}%",
        (parallel_throughput / sequential_throughput - 1.0) * 100.0
    );
}

fn process_files_sequential(
    files: &[(String, String)],
    rules: &[Box<dyn rumdl_lib::rule::Rule>],
) -> Vec<(String, rumdl_lib::rule::LintResult)> {
    files
        .iter()
        .map(|(path, content)| {
            let result = rumdl_lib::lint(content, rules, false);
            (path.clone(), result)
        })
        .collect()
}
