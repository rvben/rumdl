use crate::lint_context::LintContext;
use crate::rule::Rule;
use std::collections::HashMap;
/// Performance benchmarking framework for rumdl
///
/// This module provides comprehensive performance testing capabilities to measure
/// rule execution times, memory usage, and overall linting performance.
use std::time::{Duration, Instant};

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub peak_memory_mb: f64,
    pub average_memory_mb: f64,
    pub memory_samples: Vec<f64>,
}

/// Performance results for a single rule
#[derive(Debug, Clone)]
pub struct RulePerformanceResult {
    pub rule_name: String,
    pub execution_time: Duration,
    pub warnings_count: usize,
    pub memory_stats: Option<MemoryStats>,
    pub content_size_bytes: usize,
    pub lines_processed: usize,
}

/// Aggregate performance results for all rules
#[derive(Debug, Clone)]
pub struct AggregatePerformanceResult {
    pub total_execution_time: Duration,
    pub rule_results: Vec<RulePerformanceResult>,
    pub total_warnings: usize,
    pub content_size_bytes: usize,
    pub lines_processed: usize,
    pub rules_per_second: f64,
    pub lines_per_second: f64,
    pub bytes_per_second: f64,
}

/// Test content generator for different file sizes
pub struct ContentGenerator;

impl ContentGenerator {
    /// Generate small test content (<1KB)
    pub fn small_content() -> String {
        let mut content = String::new();
        content.push_str("# Small Test Document\n\n");
        content.push_str("This is a small test document with various markdown elements.\n\n");
        content.push_str("## Lists\n\n");
        content.push_str("- Item 1\n");
        content.push_str("- Item 2\n");
        content.push_str("  - Nested item\n\n");
        content.push_str("## Code\n\n");
        content.push_str("```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```\n\n");
        content.push_str("## Links\n\n");
        content.push_str("Visit [example.com](https://example.com) for more info.\n");
        content.push_str("Bare URL: https://example.com/bare\n\n");
        content.push_str("Contact: user@example.com\n");
        content
    }

    /// Generate medium test content (1-10KB)
    pub fn medium_content() -> String {
        let mut content = String::new();
        content.push_str("# Medium Test Document\n\n");

        // Add multiple sections with various markdown elements
        for i in 1..=20 {
            content.push_str(&format!("## Section {}\n\n", i));
            content.push_str(&format!("This is section {} with some content.\n\n", i));

            // Add lists
            content.push_str("### Lists\n\n");
            for j in 1..=5 {
                content.push_str(&format!("- List item {} in section {}\n", j, i));
                if j % 2 == 0 {
                    content.push_str(&format!("  - Nested item {}.{}\n", i, j));
                }
            }
            content.push('\n');

            // Add code blocks
            if i % 3 == 0 {
                content.push_str("### Code Example\n\n");
                content.push_str("```javascript\n");
                content.push_str(&format!("function section{}() {{\n", i));
                content.push_str(&format!("    console.log('Section {}');\n", i));
                content.push_str("    return true;\n");
                content.push_str("}\n");
                content.push_str("```\n\n");
            }

            // Add links and URLs
            content.push_str("### Links\n\n");
            content.push_str(&format!(
                "Visit [section {}](https://example.com/section{}) for details.\n",
                i, i
            ));
            content.push_str(&format!("Bare URL: https://example{}.com/path\n", i));
            content.push_str(&format!("Email: section{}@example.com\n\n", i));

            // Add emphasis and formatting
            content.push_str("### Formatting\n\n");
            content.push_str(&format!("This is **bold text** in section {}.\n", i));
            content.push_str(&format!("This is *italic text* in section {}.\n", i));
            content.push_str(&format!("This is `inline code` in section {}.\n\n", i));
        }

        content
    }

    /// Generate large test content (10-100KB)
    pub fn large_content() -> String {
        let mut content = String::new();
        content.push_str("# Large Test Document\n\n");
        content
            .push_str("This is a comprehensive test document with extensive markdown content.\n\n");

        // Add table of contents
        content.push_str("## Table of Contents\n\n");
        for i in 1..=50 {
            content.push_str(&format!("- [Section {}](#section-{})\n", i, i));
        }
        content.push('\n');

        // Add many sections with various content
        for i in 1..=50 {
            content.push_str(&format!("## Section {}\n\n", i));
            content.push_str(&format!(
                "This is section {} with comprehensive content.\n\n",
                i
            ));

            // Add subsections
            for j in 1..=3 {
                content.push_str(&format!("### Subsection {}.{}\n\n", i, j));
                content.push_str(&format!(
                    "Content for subsection {}.{} with multiple paragraphs.\n\n",
                    i, j
                ));
                content.push_str("Lorem ipsum dolor sit amet, consectetur adipiscing elit. ");
                content.push_str(
                    "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\n",
                );

                // Add lists with multiple levels
                content.push_str("#### Lists\n\n");
                for k in 1..=8 {
                    content.push_str(&format!("- Item {} in subsection {}.{}\n", k, i, j));
                    if k % 2 == 0 {
                        content.push_str(&format!("  - Nested item {}.{}.{}\n", i, j, k));
                        if k % 4 == 0 {
                            content
                                .push_str(&format!("    - Deep nested item {}.{}.{}\n", i, j, k));
                        }
                    }
                }
                content.push('\n');

                // Add code blocks
                if (i + j) % 3 == 0 {
                    content.push_str("#### Code Example\n\n");
                    content.push_str("```rust\n");
                    content.push_str(&format!("fn section_{}_{}_function() {{\n", i, j));
                    content.push_str(&format!("    let value = {};\n", i * j));
                    content.push_str("    println!(\"Processing section {}.{}\", value);\n");
                    content.push_str("    \n");
                    content.push_str("    // Complex logic here\n");
                    content.push_str("    for idx in 0..value {\n");
                    content.push_str("        process_item(idx);\n");
                    content.push_str("    }\n");
                    content.push_str("}\n");
                    content.push_str("```\n\n");
                }

                // Add tables
                if (i + j) % 4 == 0 {
                    content.push_str("#### Data Table\n\n");
                    content.push_str("| Column 1 | Column 2 | Column 3 | Column 4 |\n");
                    content.push_str("|----------|----------|----------|----------|\n");
                    for row in 1..=5 {
                        content.push_str(&format!(
                            "| Data {}.{}.{} | Value {} | Result {} | Status {} |\n",
                            i,
                            j,
                            row,
                            row * 10,
                            row * 100,
                            if row % 2 == 0 { "OK" } else { "PENDING" }
                        ));
                    }
                    content.push('\n');
                }

                // Add links and URLs
                content.push_str("#### References\n\n");
                content.push_str(&format!(
                    "- [Official docs](https://docs.example.com/section{}/subsection{})\n",
                    i, j
                ));
                content.push_str(&format!(
                    "- [API reference](https://api.example.com/v{}/section{})\n",
                    j, i
                ));
                content.push_str(&format!(
                    "- Bare URL: https://example{}.com/path/{}\n",
                    i, j
                ));
                content.push_str(&format!("- Contact: section{}@example{}.com\n", i, j));
                content.push('\n');
            }
        }

        content
    }

    /// Generate huge test content (>100KB)
    pub fn huge_content() -> String {
        let mut content = String::new();
        content.push_str("# Huge Test Document\n\n");
        content.push_str("This is an extremely large test document for stress testing.\n\n");

        // Generate the large content multiple times
        let base_content = Self::large_content();
        for i in 1..=5 {
            content.push_str(&format!("# Part {} of Huge Document\n\n", i));
            content.push_str(&base_content);
            content.push_str("\n\n");
        }

        content
    }
}

/// Performance benchmark runner
pub struct PerformanceBenchmark {
    rules: Vec<Box<dyn Rule>>,
    measure_memory: bool,
}

impl PerformanceBenchmark {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self {
        Self {
            rules,
            measure_memory: false,
        }
    }

    pub fn with_memory_measurement(mut self) -> Self {
        self.measure_memory = true;
        self
    }

    /// Benchmark a single rule with given content
    pub fn benchmark_rule(&self, rule: &dyn Rule, content: &str) -> RulePerformanceResult {
        let ctx = LintContext::new(content);
        let content_size = content.len();
        let lines_count = content.lines().count();

        // Warm up
        let _ = rule.check(&ctx);

        // Measure execution time
        let start = Instant::now();
        let warnings = rule.check(&ctx).unwrap_or_else(|_| vec![]);
        let execution_time = start.elapsed();

        RulePerformanceResult {
            rule_name: rule.name().to_string(),
            execution_time,
            warnings_count: warnings.len(),
            memory_stats: None, // TODO: Implement memory measurement
            content_size_bytes: content_size,
            lines_processed: lines_count,
        }
    }

    /// Benchmark all rules with given content
    pub fn benchmark_all_rules(&self, content: &str) -> AggregatePerformanceResult {
        let ctx = LintContext::new(content);
        let content_size = content.len();
        let lines_count = content.lines().count();
        let mut rule_results = Vec::new();
        let mut total_warnings = 0;

        // Warm up all rules
        for rule in &self.rules {
            let _ = rule.check(&ctx);
        }

        // Measure total execution time
        let total_start = Instant::now();

        // Benchmark each rule individually
        for rule in &self.rules {
            let result = self.benchmark_rule(rule.as_ref(), content);
            total_warnings += result.warnings_count;
            rule_results.push(result);
        }

        let total_execution_time = total_start.elapsed();

        // Calculate performance metrics
        let rules_per_second = self.rules.len() as f64 / total_execution_time.as_secs_f64();
        let lines_per_second = lines_count as f64 / total_execution_time.as_secs_f64();
        let bytes_per_second = content_size as f64 / total_execution_time.as_secs_f64();

        AggregatePerformanceResult {
            total_execution_time,
            rule_results,
            total_warnings,
            content_size_bytes: content_size,
            lines_processed: lines_count,
            rules_per_second,
            lines_per_second,
            bytes_per_second,
        }
    }

    /// Run comprehensive performance tests with different content sizes
    pub fn run_comprehensive_benchmark(&self) -> HashMap<String, AggregatePerformanceResult> {
        let mut results = HashMap::new();

        println!("Running comprehensive performance benchmark...");

        // Test with different content sizes
        let test_cases = vec![
            ("small", ContentGenerator::small_content()),
            ("medium", ContentGenerator::medium_content()),
            ("large", ContentGenerator::large_content()),
            ("huge", ContentGenerator::huge_content()),
        ];

        for (size_name, content) in test_cases {
            println!(
                "Benchmarking {} content ({} bytes, {} lines)...",
                size_name,
                content.len(),
                content.lines().count()
            );

            let result = self.benchmark_all_rules(&content);
            results.insert(size_name.to_string(), result);
        }

        results
    }

    /// Print detailed performance report
    pub fn print_performance_report(&self, results: &HashMap<String, AggregatePerformanceResult>) {
        println!("\n=== RUMDL PERFORMANCE BENCHMARK REPORT ===\n");

        for (size_name, result) in results {
            println!("📊 {} Content Performance:", size_name.to_uppercase());
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

            // Show top 10 slowest rules
            let mut sorted_rules = result.rule_results.clone();
            sorted_rules.sort_by(|a, b| b.execution_time.cmp(&a.execution_time));

            println!("   Top 10 slowest rules:");
            for (i, rule_result) in sorted_rules.iter().take(10).enumerate() {
                let percentage = (rule_result.execution_time.as_secs_f64()
                    / result.total_execution_time.as_secs_f64())
                    * 100.0;
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

        // Summary comparison
        println!("📈 Performance Scaling Summary:");
        if let (Some(small), Some(large)) = (results.get("small"), results.get("large")) {
            let size_ratio = large.content_size_bytes as f64 / small.content_size_bytes as f64;
            let time_ratio =
                large.total_execution_time.as_secs_f64() / small.total_execution_time.as_secs_f64();
            println!("   Content size ratio (large/small): {:.1}x", size_ratio);
            println!("   Execution time ratio (large/small): {:.1}x", time_ratio);
            println!(
                "   Scaling efficiency: {:.1}% (lower is better)",
                (time_ratio / size_ratio) * 100.0
            );
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_generators() {
        let small = ContentGenerator::small_content();
        let medium = ContentGenerator::medium_content();
        let large = ContentGenerator::large_content();

        // Check actual sizes instead of hardcoded values
        assert!(
            small.len() < 1024,
            "Small content should be < 1KB, got {} bytes",
            small.len()
        );
        assert!(
            medium.len() >= 1024,
            "Medium content should be >= 1KB, got {} bytes",
            medium.len()
        );
        assert!(
            large.len() >= medium.len(),
            "Large content should be >= medium content, got {} vs {} bytes",
            large.len(),
            medium.len()
        );

        // Verify content has various markdown elements
        assert!(small.contains("# "), "Should contain headings");
        assert!(small.contains("- "), "Should contain lists");
        assert!(small.contains("```"), "Should contain code blocks");
        assert!(small.contains("http"), "Should contain URLs");
    }

    #[test]
    fn test_performance_benchmark_creation() {
        let rules: Vec<Box<dyn Rule>> = vec![];
        let benchmark = PerformanceBenchmark::new(rules);
        assert!(!benchmark.measure_memory);

        let benchmark = benchmark.with_memory_measurement();
        assert!(benchmark.measure_memory);
    }
}
