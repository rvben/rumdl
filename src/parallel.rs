/// Parallel file processing module for rumdl
///
/// This module implements file-level parallel execution of markdown linting
/// to improve performance when processing multiple files.
use crate::rule::{LintResult, Rule};
use rayon::prelude::*;
use std::time::Instant;

/// Configuration for parallel execution
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Enable/disable parallel execution
    pub enabled: bool,
    /// Number of threads to use (None = auto-detect)
    pub thread_count: Option<usize>,
    /// Minimum number of files to enable parallel execution
    pub min_file_count: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            thread_count: None, // Auto-detect based on CPU cores
            min_file_count: 2,  // At least 2 files to benefit from parallelization
        }
    }
}

/// File-level parallel processing for multiple files
pub struct FileParallelProcessor {
    config: ParallelConfig,
}

impl FileParallelProcessor {
    pub fn new(config: ParallelConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(ParallelConfig::default())
    }

    /// Process multiple files in parallel
    pub fn process_files(
        &self,
        files: &[(String, String)], // (path, content) pairs
        rules: &[Box<dyn Rule>],
    ) -> Result<Vec<(String, LintResult)>, String> {
        if !self.should_use_parallel(files) {
            // Fall back to sequential processing
            return Ok(files
                .iter()
                .map(|(path, content)| {
                    let result = crate::lint(content, rules, false, crate::config::MarkdownFlavor::Standard, None);
                    (path.clone(), result)
                })
                .collect());
        }

        // Set up thread pool if specified
        if let Some(thread_count) = self.config.thread_count {
            rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build_global()
                .unwrap_or_else(|_| log::warn!("Failed to set thread pool size to {thread_count}"));
        }

        let results: Vec<(String, LintResult)> = files
            .par_iter()
            .map(|(path, content)| {
                let start = Instant::now();
                let result = crate::lint(content, rules, false, crate::config::MarkdownFlavor::Standard, None);
                let duration = start.elapsed();

                if duration.as_millis() > 1000 {
                    log::debug!("File {path} took {duration:?}");
                }

                (path.clone(), result)
            })
            .collect();

        Ok(results)
    }

    /// Determine if file-level parallel processing should be used
    pub fn should_use_parallel(&self, files: &[(String, String)]) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Need at least minimum files to benefit from parallelization
        if files.len() < self.config.min_file_count {
            return false;
        }

        // Check if we have enough CPU cores
        let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        if cpu_cores < 2 {
            return false;
        }

        true
    }
}

/// Performance comparison utilities
pub struct ParallelPerformanceComparison {
    pub sequential_time: std::time::Duration,
    pub parallel_time: std::time::Duration,
    pub speedup_factor: f64,
    pub parallel_overhead: std::time::Duration,
}

impl ParallelPerformanceComparison {
    pub fn new(sequential_time: std::time::Duration, parallel_time: std::time::Duration) -> Self {
        // Guard against division by zero: if parallel_time is zero, speedup is infinite
        let speedup_factor = if parallel_time.is_zero() {
            f64::INFINITY
        } else {
            sequential_time.as_secs_f64() / parallel_time.as_secs_f64()
        };
        let parallel_overhead = if parallel_time > sequential_time {
            parallel_time - sequential_time
        } else {
            std::time::Duration::ZERO
        };

        Self {
            sequential_time,
            parallel_time,
            speedup_factor,
            parallel_overhead,
        }
    }

    pub fn print_comparison(&self) {
        println!("🔄 Parallel vs Sequential Performance:");
        println!(
            "   Sequential time: {:.3}ms",
            self.sequential_time.as_secs_f64() * 1000.0
        );
        println!("   Parallel time: {:.3}ms", self.parallel_time.as_secs_f64() * 1000.0);
        println!("   Speedup factor: {:.2}x", self.speedup_factor);

        if self.speedup_factor > 1.0 {
            let improvement = (self.speedup_factor - 1.0) * 100.0;
            println!("   Performance improvement: {improvement:.1}%");
        } else {
            let degradation = (1.0 - self.speedup_factor) * 100.0;
            println!("   Performance degradation: {degradation:.1}%");
            if self.parallel_overhead > std::time::Duration::ZERO {
                println!(
                    "   Parallel overhead: {:.3}ms",
                    self.parallel_overhead.as_secs_f64() * 1000.0
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::rules::all_rules;

    #[test]
    fn test_parallel_config_defaults() {
        let config = ParallelConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_file_count, 2);
        assert!(config.thread_count.is_none());
    }

    #[test]
    fn test_parallel_config_custom() {
        let config = ParallelConfig {
            enabled: false,
            thread_count: Some(4),
            min_file_count: 5,
        };
        assert!(!config.enabled);
        assert_eq!(config.thread_count, Some(4));
        assert_eq!(config.min_file_count, 5);
    }

    #[test]
    fn test_should_use_parallel_logic() {
        let processor = FileParallelProcessor::with_default_config();

        // Single file should not use parallel
        let single_file = vec![("test.md".to_string(), "# Test".to_string())];
        assert!(!processor.should_use_parallel(&single_file));

        // Multiple files should use parallel
        let multiple_files = vec![
            ("test1.md".to_string(), "# Test 1".to_string()),
            ("test2.md".to_string(), "# Test 2".to_string()),
        ];
        assert!(processor.should_use_parallel(&multiple_files));

        // Test with disabled parallel
        let disabled_config = ParallelConfig {
            enabled: false,
            ..Default::default()
        };
        let disabled_processor = FileParallelProcessor::new(disabled_config);
        assert!(!disabled_processor.should_use_parallel(&multiple_files));

        // Test with high min_file_count
        let high_threshold_config = ParallelConfig {
            enabled: true,
            min_file_count: 10,
            ..Default::default()
        };
        let high_threshold_processor = FileParallelProcessor::new(high_threshold_config);
        assert!(!high_threshold_processor.should_use_parallel(&multiple_files));
    }

    #[test]
    fn test_file_parallel_processing() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let test_files = vec![
            ("test1.md".to_string(), "# Test 1\n\nContent".to_string()),
            ("test2.md".to_string(), "# Test 2\n\nMore content".to_string()),
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 2);

        // Verify all results are Ok
        for (_, result) in results {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_empty_files_handling() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let empty_files: Vec<(String, String)> = vec![];
        let results = processor.process_files(&empty_files, &rules).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_large_file_count() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        // Create many files to test parallel processing scalability
        let test_files: Vec<(String, String)> = (0..100)
            .map(|i| {
                (
                    format!("test{i}.md"),
                    format!("# Test {i}\n\nContent with trailing spaces   \n"),
                )
            })
            .collect();

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 100);

        // Verify all results are Ok and have expected warnings
        for (path, result) in &results {
            assert!(result.is_ok(), "Failed processing {path}");
            let warnings = result.as_ref().unwrap();
            // Should have at least one warning for trailing spaces
            assert!(!warnings.is_empty(), "Expected warnings for {path}");
        }
    }

    #[test]
    fn test_error_propagation() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        // Include files with various edge cases that might trigger errors
        let test_files = vec![
            ("empty.md".to_string(), "".to_string()),
            ("unicode.md".to_string(), "# 测试标题\n\n这是中文内容。".to_string()),
            (
                "emoji.md".to_string(),
                "# Title with 🚀 emoji\n\n🎉 Content!".to_string(),
            ),
            ("very_long_line.md".to_string(), "a".repeat(10000)), // Very long single line
            ("many_lines.md".to_string(), "Line\n".repeat(10000)), // Many lines
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 5);

        // All should process successfully even with edge cases
        for (path, result) in &results {
            assert!(result.is_ok(), "Failed processing {path}");
        }
    }

    #[test]
    fn test_thread_count_configuration() {
        let config = Config::default();
        let rules = all_rules(&config);

        // Test with specific thread count
        let parallel_config = ParallelConfig {
            enabled: true,
            thread_count: Some(2),
            min_file_count: 2,
        };
        let processor = FileParallelProcessor::new(parallel_config);

        let test_files = vec![
            ("test1.md".to_string(), "# Test 1".to_string()),
            ("test2.md".to_string(), "# Test 2".to_string()),
            ("test3.md".to_string(), "# Test 3".to_string()),
            ("test4.md".to_string(), "# Test 4".to_string()),
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_result_ordering_preservation() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let test_files: Vec<(String, String)> = (0..20)
            .map(|i| (format!("test{i:02}.md"), format!("# Test {i}")))
            .collect();

        let results = processor.process_files(&test_files, &rules).unwrap();

        // Verify results maintain the same order as input
        for (i, (path, _)) in results.iter().enumerate() {
            assert_eq!(path, &format!("test{i:02}.md"));
        }
    }

    #[test]
    fn test_concurrent_rule_execution_safety() {
        // This test ensures rules can be safely executed concurrently
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        // Create files that will trigger the same rules
        let test_files: Vec<(String, String)> = (0..10)
            .map(|i| {
                (
                    format!("test{i}.md"),
                    "# Heading\n\n- List item\n- Another item\n\n[link](url)\n`code`".to_string(),
                )
            })
            .collect();

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 10);

        // All files should produce the same warnings
        let first_warnings = &results[0].1.as_ref().unwrap();
        for (_, result) in results.iter().skip(1) {
            let warnings = result.as_ref().unwrap();
            assert_eq!(warnings.len(), first_warnings.len());
        }
    }

    #[test]
    fn test_performance_comparison() {
        let seq_time = std::time::Duration::from_millis(1000);
        let par_time = std::time::Duration::from_millis(400);

        let comparison = ParallelPerformanceComparison::new(seq_time, par_time);

        assert_eq!(comparison.sequential_time, seq_time);
        assert_eq!(comparison.parallel_time, par_time);
        assert!((comparison.speedup_factor - 2.5).abs() < 0.01);
        assert_eq!(comparison.parallel_overhead, std::time::Duration::ZERO);
    }

    #[test]
    fn test_performance_comparison_with_overhead() {
        let seq_time = std::time::Duration::from_millis(100);
        let par_time = std::time::Duration::from_millis(150);

        let comparison = ParallelPerformanceComparison::new(seq_time, par_time);

        assert!((comparison.speedup_factor - 0.667).abs() < 0.01);
        assert_eq!(comparison.parallel_overhead, std::time::Duration::from_millis(50));
    }

    #[test]
    fn test_fallback_to_sequential() {
        let config = Config::default();
        let rules = all_rules(&config);

        // Force sequential processing
        let sequential_config = ParallelConfig {
            enabled: false,
            ..Default::default()
        };
        let processor = FileParallelProcessor::new(sequential_config);

        let test_files = vec![
            ("test1.md".to_string(), "# Test 1".to_string()),
            ("test2.md".to_string(), "# Test 2".to_string()),
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_mixed_content_types() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let test_files = vec![
            ("plain.md".to_string(), "Just plain text".to_string()),
            ("code.md".to_string(), "```rust\nfn main() {}\n```".to_string()),
            ("table.md".to_string(), "| A | B |\n|---|---|\n| 1 | 2 |".to_string()),
            (
                "front_matter.md".to_string(),
                "---\ntitle: Test\n---\n# Content".to_string(),
            ),
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 4);

        for (_, result) in results {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_deterministic_results() {
        // Ensure parallel processing produces the same results every time
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let test_files: Vec<(String, String)> = (0..10)
            .map(|i| (format!("test{i}.md"), format!("# Heading {i}\n\nTrailing spaces   \n")))
            .collect();

        // Run multiple times
        let results1 = processor.process_files(&test_files, &rules).unwrap();
        let results2 = processor.process_files(&test_files, &rules).unwrap();
        let results3 = processor.process_files(&test_files, &rules).unwrap();

        // Compare warning counts for each file
        for i in 0..test_files.len() {
            let warnings1 = results1[i].1.as_ref().unwrap();
            let warnings2 = results2[i].1.as_ref().unwrap();
            let warnings3 = results3[i].1.as_ref().unwrap();

            assert_eq!(warnings1.len(), warnings2.len());
            assert_eq!(warnings2.len(), warnings3.len());
        }
    }

    // =========================================================================
    // Tests for ParallelPerformanceComparison edge cases
    // =========================================================================

    #[test]
    fn test_performance_comparison_normal() {
        let sequential = std::time::Duration::from_millis(100);
        let parallel = std::time::Duration::from_millis(50);

        let comparison = ParallelPerformanceComparison::new(sequential, parallel);

        assert_eq!(comparison.sequential_time, sequential);
        assert_eq!(comparison.parallel_time, parallel);
        assert!((comparison.speedup_factor - 2.0).abs() < 0.001);
        assert_eq!(comparison.parallel_overhead, std::time::Duration::ZERO);
    }

    #[test]
    fn test_performance_comparison_zero_parallel_time() {
        // Edge case: parallel_time is zero (instant completion)
        let sequential = std::time::Duration::from_millis(100);
        let parallel = std::time::Duration::ZERO;

        let comparison = ParallelPerformanceComparison::new(sequential, parallel);

        // Should not panic, speedup should be infinity
        assert!(comparison.speedup_factor.is_infinite());
        assert!(comparison.speedup_factor.is_sign_positive());
    }

    #[test]
    fn test_performance_comparison_both_zero() {
        // Edge case: both times are zero
        let sequential = std::time::Duration::ZERO;
        let parallel = std::time::Duration::ZERO;

        let comparison = ParallelPerformanceComparison::new(sequential, parallel);

        // Should not panic, speedup should be infinity (0/0 guarded)
        assert!(comparison.speedup_factor.is_infinite());
    }

    #[test]
    fn test_performance_comparison_parallel_slower() {
        // Case where parallel is actually slower (overhead dominates)
        let sequential = std::time::Duration::from_millis(10);
        let parallel = std::time::Duration::from_millis(20);

        let comparison = ParallelPerformanceComparison::new(sequential, parallel);

        assert!((comparison.speedup_factor - 0.5).abs() < 0.001);
        assert_eq!(comparison.parallel_overhead, std::time::Duration::from_millis(10));
    }

    #[test]
    fn test_performance_comparison_very_small_times() {
        // Very small durations (nanoseconds)
        let sequential = std::time::Duration::from_nanos(100);
        let parallel = std::time::Duration::from_nanos(50);

        let comparison = ParallelPerformanceComparison::new(sequential, parallel);

        // Should handle small durations without precision issues
        assert!(comparison.speedup_factor > 1.0);
    }
}
