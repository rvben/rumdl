//!
//! This module provides profiling utilities for measuring and reporting execution times in rumdl.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Global profiler state
static PROFILER: LazyLock<Mutex<Profiler>> = LazyLock::new(|| Mutex::new(Profiler::new()));

// Enable/disable profiling with a feature flag
#[cfg(feature = "profiling")]
pub(crate) const PROFILING_ENABLED: bool = true;

#[cfg(not(feature = "profiling"))]
pub(crate) const PROFILING_ENABLED: bool = false;

/// A simple profiling utility to measure and report execution times
pub struct Profiler {
    measurements: HashMap<String, (Duration, usize)>,
    active_timers: HashMap<String, Instant>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    /// Create a new profiler instance
    pub fn new() -> Self {
        Profiler {
            measurements: HashMap::new(),
            active_timers: HashMap::new(),
        }
    }

    /// Start a timer for a specific section of code
    pub fn start_timer(&mut self, section: &str) {
        if PROFILING_ENABLED {
            let section_name = section.to_string();
            self.active_timers.insert(section_name, Instant::now());
        }
    }

    /// Stop the timer for a section and record the elapsed time
    pub fn stop_timer(&mut self, section: &str) {
        if PROFILING_ENABLED {
            let section_name = section.to_string();
            if let Some(start_time) = self.active_timers.remove(&section_name) {
                let elapsed = start_time.elapsed();

                // Update or insert the measurement
                let entry = self
                    .measurements
                    .entry(section_name)
                    .or_insert((Duration::new(0, 0), 0));
                entry.0 += elapsed;
                entry.1 += 1;
            }
        }
    }

    /// Get a report of all measurements
    pub fn get_report(&self) -> String {
        if !PROFILING_ENABLED || self.measurements.is_empty() {
            return "Profiling disabled or no measurements recorded.".to_string();
        }

        // Sort measurements by total time (descending)
        let mut sorted_measurements: Vec<_> = self.measurements.iter().collect();
        sorted_measurements.sort_by(|a, b| b.1.0.cmp(&a.1.0));

        // Calculate total time across all sections
        let total_time: Duration = sorted_measurements.iter().map(|(_, (duration, _))| duration).sum();

        // Generate the report
        let mut report = String::new();
        report.push_str("=== Profiling Report ===\n");
        report.push_str(&format!(
            "Total execution time: {:.6} seconds\n\n",
            total_time.as_secs_f64()
        ));
        report.push_str(
            "Section                                  | Total Time (s) | Calls | Avg Time (ms) | % of Total\n",
        );
        report.push_str(
            "------------------------------------------|----------------|-------|---------------|----------\n",
        );

        for (section, (duration, calls)) in sorted_measurements {
            let total_seconds = duration.as_secs_f64();
            let avg_ms = (duration.as_nanos() as f64) / (calls * 1_000_000) as f64;
            let percentage = (total_seconds / total_time.as_secs_f64()) * 100.0;

            report.push_str(&format!(
                "{section:<42} | {total_seconds:<14.6} | {calls:<5} | {avg_ms:<13.3} | {percentage:<8.2}%\n"
            ));
        }

        report
    }

    /// Reset all measurements
    pub fn reset(&mut self) {
        self.measurements.clear();
        self.active_timers.clear();
    }
}

/// Start a timer for a section
///
/// If the mutex is poisoned, this is a no-op. Profiling failures should not crash the application.
pub fn start_timer(section: &str) {
    if PROFILING_ENABLED && let Ok(mut profiler) = PROFILER.lock() {
        profiler.start_timer(section);
    }
}

/// Stop a timer for a section
///
/// If the mutex is poisoned, this is a no-op. Profiling failures should not crash the application.
pub fn stop_timer(section: &str) {
    if PROFILING_ENABLED && let Ok(mut profiler) = PROFILER.lock() {
        profiler.stop_timer(section);
    }
}

/// Get a report of all measurements
///
/// If the mutex is poisoned, returns a message indicating the error rather than panicking.
pub fn get_report() -> String {
    if PROFILING_ENABLED {
        match PROFILER.lock() {
            Ok(profiler) => profiler.get_report(),
            Err(_) => "Profiling report unavailable (mutex poisoned).".to_string(),
        }
    } else {
        "Profiling is disabled.".to_string()
    }
}

/// Reset all measurements
///
/// If the mutex is poisoned, this is a no-op. Profiling failures should not crash the application.
pub fn reset() {
    if PROFILING_ENABLED && let Ok(mut profiler) = PROFILER.lock() {
        profiler.reset();
    }
}

/// A utility struct to time a section of code using RAII
pub struct ScopedTimer {
    section: String,
    enabled: bool,
}

impl ScopedTimer {
    /// Create a new scoped timer
    pub fn new(section: &str) -> Self {
        let enabled = PROFILING_ENABLED;
        if enabled {
            start_timer(section);
        }
        ScopedTimer {
            section: section.to_string(),
            enabled,
        }
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        if self.enabled {
            stop_timer(&self.section);
        }
    }
}

/// Convenience macro to time a block of code
#[macro_export]
macro_rules! time_section {
    ($section:expr, $block:block) => {{
        let _timer = $crate::profiling::ScopedTimer::new($section);
        $block
    }};
}

/// Convenience macro to time a function call
#[macro_export]
macro_rules! time_function {
    ($section:expr, $func:expr) => {{
        let _timer = $crate::profiling::ScopedTimer::new($section);
        $func
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_new() {
        let profiler = Profiler::new();
        assert!(profiler.measurements.is_empty());
        assert!(profiler.active_timers.is_empty());
    }

    #[test]
    fn test_profiler_default() {
        let profiler = Profiler::default();
        assert!(profiler.measurements.is_empty());
        assert!(profiler.active_timers.is_empty());
    }

    #[test]
    fn test_profiler_start_stop_timer() {
        let mut profiler = Profiler::new();

        // Force profiling to be enabled for this test
        if PROFILING_ENABLED {
            profiler.start_timer("test_section");
            thread::sleep(Duration::from_millis(10));
            profiler.stop_timer("test_section");

            assert!(profiler.measurements.contains_key("test_section"));
            let (duration, count) = profiler.measurements.get("test_section").unwrap();
            assert!(*count == 1);
            assert!(duration.as_millis() >= 10);
        }
    }

    #[test]
    fn test_profiler_multiple_measurements() {
        let mut profiler = Profiler::new();

        if PROFILING_ENABLED {
            // Multiple measurements of same section
            for _ in 0..3 {
                profiler.start_timer("test_section");
                thread::sleep(Duration::from_millis(5));
                profiler.stop_timer("test_section");
            }

            assert!(profiler.measurements.contains_key("test_section"));
            let (duration, count) = profiler.measurements.get("test_section").unwrap();
            assert_eq!(*count, 3);
            assert!(duration.as_millis() >= 15);
        }
    }

    #[test]
    fn test_profiler_get_report() {
        let mut profiler = Profiler::new();

        if PROFILING_ENABLED {
            profiler.start_timer("section1");
            thread::sleep(Duration::from_millis(20));
            profiler.stop_timer("section1");

            profiler.start_timer("section2");
            thread::sleep(Duration::from_millis(10));
            profiler.stop_timer("section2");

            let report = profiler.get_report();
            assert!(report.contains("Profiling Report"));
            assert!(report.contains("section1"));
            assert!(report.contains("section2"));
            assert!(report.contains("Total execution time"));
        } else {
            let report = profiler.get_report();
            assert_eq!(report, "Profiling disabled or no measurements recorded.");
        }
    }

    #[test]
    fn test_profiler_reset() {
        let mut profiler = Profiler::new();

        if PROFILING_ENABLED {
            profiler.start_timer("test_section");
            profiler.stop_timer("test_section");

            assert!(!profiler.measurements.is_empty());

            profiler.reset();
            assert!(profiler.measurements.is_empty());
            assert!(profiler.active_timers.is_empty());
        }
    }

    #[test]
    fn test_profiler_stop_without_start() {
        let mut profiler = Profiler::new();

        // Should not panic
        profiler.stop_timer("nonexistent_section");
        assert!(profiler.measurements.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn test_global_start_stop_timer() {
        if PROFILING_ENABLED {
            reset(); // Clear any previous measurements

            start_timer("global_test");
            thread::sleep(Duration::from_millis(10));
            stop_timer("global_test");

            let report = get_report();
            assert!(report.contains("global_test"));
        }
    }

    #[test]
    fn test_global_get_report() {
        let report = get_report();
        if PROFILING_ENABLED {
            assert!(report.contains("Profiling Report") || report.contains("no measurements"));
        } else {
            assert_eq!(report, "Profiling is disabled.");
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_global_reset() {
        if PROFILING_ENABLED {
            start_timer("test_reset");
            stop_timer("test_reset");

            reset();
            let report = get_report();
            assert!(!report.contains("test_reset"));
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_scoped_timer() {
        if PROFILING_ENABLED {
            reset();

            {
                let _timer = ScopedTimer::new("scoped_test");
                thread::sleep(Duration::from_millis(10));
            } // Timer should stop here

            let report = get_report();
            assert!(report.contains("scoped_test"));
        }
    }

    #[test]
    fn test_scoped_timer_drop() {
        let timer = ScopedTimer::new("drop_test");
        assert_eq!(timer.section, "drop_test");
        assert_eq!(timer.enabled, PROFILING_ENABLED);
        // Timer will be dropped and stop_timer called
    }

    #[test]
    fn test_empty_report() {
        let profiler = Profiler::new();
        let report = profiler.get_report();

        if PROFILING_ENABLED {
            assert_eq!(report, "Profiling disabled or no measurements recorded.");
        }
    }

    #[test]
    fn test_report_formatting() {
        let mut profiler = Profiler::new();

        if PROFILING_ENABLED {
            // Create predictable measurements
            profiler
                .measurements
                .insert("test1".to_string(), (Duration::from_secs(1), 10));
            profiler
                .measurements
                .insert("test2".to_string(), (Duration::from_millis(500), 5));

            let report = profiler.get_report();

            // Check report structure
            assert!(report.contains("Section"));
            assert!(report.contains("Total Time (s)"));
            assert!(report.contains("Calls"));
            assert!(report.contains("Avg Time (ms)"));
            assert!(report.contains("% of Total"));

            // Check that test1 appears before test2 (sorted by duration)
            let test1_pos = report.find("test1").unwrap();
            let test2_pos = report.find("test2").unwrap();
            assert!(test1_pos < test2_pos);
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::sync::Barrier;

        if PROFILING_ENABLED {
            reset();

            let barrier = Arc::new(Barrier::new(3));
            let mut handles = vec![];

            for i in 0..3 {
                let b = barrier.clone();
                let handle = thread::spawn(move || {
                    b.wait();
                    start_timer(&format!("thread_{i}"));
                    thread::sleep(Duration::from_millis(10));
                    stop_timer(&format!("thread_{i}"));
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            let report = get_report();
            assert!(report.contains("thread_0"));
            assert!(report.contains("thread_1"));
            assert!(report.contains("thread_2"));
        }
    }
}
