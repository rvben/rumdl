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
                    let result = crate::lint(content, rules, false);
                    (path.clone(), result)
                })
                .collect());
        }

        // Set up thread pool if specified
        if let Some(thread_count) = self.config.thread_count {
            rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build_global()
                .unwrap_or_else(|_| {
                    log::warn!("Failed to set thread pool size to {}", thread_count)
                });
        }

        let results: Vec<(String, LintResult)> = files
            .par_iter()
            .map(|(path, content)| {
                let start = Instant::now();
                let result = crate::lint(content, rules, false);
                let duration = start.elapsed();

                if duration.as_millis() > 1000 {
                    log::debug!("File {} took {:?}", path, duration);
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
        let cpu_cores = num_cpus::get();
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
        let speedup_factor = sequential_time.as_secs_f64() / parallel_time.as_secs_f64();
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
        println!(
            "   Parallel time: {:.3}ms",
            self.parallel_time.as_secs_f64() * 1000.0
        );
        println!("   Speedup factor: {:.2}x", self.speedup_factor);

        if self.speedup_factor > 1.0 {
            let improvement = (self.speedup_factor - 1.0) * 100.0;
            println!("   Performance improvement: {:.1}%", improvement);
        } else {
            let degradation = (1.0 - self.speedup_factor) * 100.0;
            println!("   Performance degradation: {:.1}%", degradation);
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
    }

    #[test]
    fn test_file_parallel_processing() {
        let config = Config::default();
        let rules = all_rules(&config);
        let processor = FileParallelProcessor::with_default_config();

        let test_files = vec![
            ("test1.md".to_string(), "# Test 1\n\nContent".to_string()),
            (
                "test2.md".to_string(),
                "# Test 2\n\nMore content".to_string(),
            ),
        ];

        let results = processor.process_files(&test_files, &rules).unwrap();
        assert_eq!(results.len(), 2);

        // Verify all results are Ok
        for (_, result) in results {
            assert!(result.is_ok());
        }
    }
}
