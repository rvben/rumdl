use rumdl::config::Config;
/// Performance benchmark binary for rumdl
///
/// This binary runs comprehensive performance tests to establish baseline metrics
/// and measure the impact of optimizations like parallel rule execution.
use rumdl::performance::{ContentGenerator, PerformanceBenchmark};
use rumdl::rules::all_rules;
use std::env;

fn main() {
    // Initialize logging
    env_logger::init();

    println!("🚀 RUMDL Performance Benchmark Tool");
    println!("====================================\n");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let test_type = args.get(1).map(|s| s.as_str()).unwrap_or("all");

    // Create all rules with default configuration
    let config = Config::default();
    let rules = all_rules(&config);

    println!("📋 Benchmark Configuration:");
    println!("   Rules to test: {}", rules.len());
    println!("   Test type: {test_type}");
    println!("   CPU cores: {}", num_cpus::get());
    println!();

    // Create benchmark runner
    let benchmark = PerformanceBenchmark::new(rules);

    match test_type {
        "small" => {
            println!("🔬 Running small content benchmark...");
            let content = ContentGenerator::small_content();
            let result = benchmark.benchmark_all_rules(&content);
            print_single_result("Small", &result);
        }
        "medium" => {
            println!("🔬 Running medium content benchmark...");
            let content = ContentGenerator::medium_content();
            let result = benchmark.benchmark_all_rules(&content);
            print_single_result("Medium", &result);
        }
        "large" => {
            println!("🔬 Running large content benchmark...");
            let content = ContentGenerator::large_content();
            let result = benchmark.benchmark_all_rules(&content);
            print_single_result("Large", &result);
        }
        "huge" => {
            println!("🔬 Running huge content benchmark...");
            let content = ContentGenerator::huge_content();
            let result = benchmark.benchmark_all_rules(&content);
            print_single_result("Huge", &result);
        }
        "all" => {
            println!("🔬 Running comprehensive benchmark suite...");
            let results = benchmark.run_comprehensive_benchmark();
            benchmark.print_performance_report(&results);

            // Save results to file for comparison
            save_baseline_results(&results);
        }
        _ => {
            println!("🔬 Running comprehensive benchmark suite...");
            let results = benchmark.run_comprehensive_benchmark();
            benchmark.print_performance_report(&results);

            // Save results to file for comparison
            save_baseline_results(&results);
        }
    }

    println!("✅ Benchmark completed!");
}

fn print_single_result(size_name: &str, result: &rumdl::performance::AggregatePerformanceResult) {
    println!("\n📊 {} Content Performance:", size_name.to_uppercase());
    println!(
        "   Content size: {} bytes ({} lines)",
        result.content_size_bytes, result.lines_processed
    );
    println!(
        "   Total execution time: {:.3}ms",
        result.total_execution_time.as_secs_f64() * 1000.0
    );
    println!("   Total warnings found: {}", result.total_warnings);
    println!("   Performance metrics:");
    println!("     - Rules per second: {:.1}", result.rules_per_second);
    println!("     - Lines per second: {:.0}", result.lines_per_second);
    println!("     - Bytes per second: {:.0}", result.bytes_per_second);
    println!();

    // Show top 5 slowest rules
    let mut sorted_rules = result.rule_results.clone();
    sorted_rules.sort_by(|a, b| b.execution_time.cmp(&a.execution_time));

    println!("   Top 5 slowest rules:");
    for (i, rule_result) in sorted_rules.iter().take(5).enumerate() {
        let percentage = (rule_result.execution_time.as_secs_f64() / result.total_execution_time.as_secs_f64()) * 100.0;
        println!(
            "     {}. {} - {:.3}ms ({:.1}%) - {} warnings",
            i + 1,
            rule_result.rule_name,
            rule_result.execution_time.as_secs_f64() * 1000.0,
            percentage,
            rule_result.warnings_count
        );
    }
    println!();
}

fn save_baseline_results(results: &std::collections::HashMap<String, rumdl::performance::AggregatePerformanceResult>) {
    use std::fs;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("benchmark_baseline_{timestamp}.txt");

    let mut output = String::new();
    output.push_str("RUMDL Performance Baseline Results\n");
    output.push_str(&format!(
        "Generated: {}\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    output.push_str(&format!("CPU cores: {}\n", num_cpus::get()));
    output.push('\n');

    for (size_name, result) in results {
        output.push_str(&format!("=== {} CONTENT ===\n", size_name.to_uppercase()));
        output.push_str(&format!(
            "Content size: {} bytes ({} lines)\n",
            result.content_size_bytes, result.lines_processed
        ));
        output.push_str(&format!(
            "Total execution time: {:.3}ms\n",
            result.total_execution_time.as_secs_f64() * 1000.0
        ));
        output.push_str(&format!("Total warnings: {}\n", result.total_warnings));
        output.push_str(&format!("Rules per second: {:.1}\n", result.rules_per_second));
        output.push_str(&format!("Lines per second: {:.0}\n", result.lines_per_second));
        output.push_str(&format!("Bytes per second: {:.0}\n", result.bytes_per_second));
        output.push('\n');

        // Add detailed rule timings
        let mut sorted_rules = result.rule_results.clone();
        sorted_rules.sort_by(|a, b| b.execution_time.cmp(&a.execution_time));

        output.push_str("Rule timings (sorted by execution time):\n");
        for rule_result in &sorted_rules {
            let percentage =
                (rule_result.execution_time.as_secs_f64() / result.total_execution_time.as_secs_f64()) * 100.0;
            output.push_str(&format!(
                "  {} - {:.3}ms ({:.1}%) - {} warnings\n",
                rule_result.rule_name,
                rule_result.execution_time.as_secs_f64() * 1000.0,
                percentage,
                rule_result.warnings_count
            ));
        }
        output.push('\n');
    }

    match fs::write(&filename, output) {
        Ok(_) => println!("📄 Baseline results saved to: {filename}"),
        Err(e) => eprintln!("❌ Failed to save baseline results: {e}"),
    }
}
