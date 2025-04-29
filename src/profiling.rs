//!
//! This module provides profiling utilities for measuring and reporting execution times in rumdl.

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Global profiler state
lazy_static! {
    static ref PROFILER: Mutex<Profiler> = Mutex::new(Profiler::new());
}

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
        sorted_measurements.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

        // Calculate total time across all sections
        let total_time: Duration = sorted_measurements
            .iter()
            .map(|(_, (duration, _))| duration)
            .sum();

        // Generate the report
        let mut report = String::new();
        report.push_str("=== Profiling Report ===\n");
        report.push_str(&format!(
            "Total execution time: {:.6} seconds\n\n",
            total_time.as_secs_f64()
        ));
        report.push_str("Section                                  | Total Time (s) | Calls | Avg Time (ms) | % of Total\n");
        report.push_str("------------------------------------------|----------------|-------|---------------|----------\n");

        for (section, (duration, calls)) in sorted_measurements {
            let total_seconds = duration.as_secs_f64();
            let avg_ms = (duration.as_nanos() as f64) / (calls * 1_000_000) as f64;
            let percentage = (total_seconds / total_time.as_secs_f64()) * 100.0;

            report.push_str(&format!(
                "{:<42} | {:<14.6} | {:<5} | {:<13.3} | {:<8.2}%\n",
                section, total_seconds, calls, avg_ms, percentage
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
pub fn start_timer(section: &str) {
    if PROFILING_ENABLED {
        let mut profiler = PROFILER.lock().unwrap();
        profiler.start_timer(section);
    }
}

/// Stop a timer for a section
pub fn stop_timer(section: &str) {
    if PROFILING_ENABLED {
        let mut profiler = PROFILER.lock().unwrap();
        profiler.stop_timer(section);
    }
}

/// Get a report of all measurements
pub fn get_report() -> String {
    if PROFILING_ENABLED {
        let profiler = PROFILER.lock().unwrap();
        profiler.get_report()
    } else {
        "Profiling is disabled.".to_string()
    }
}

/// Reset all measurements
pub fn reset() {
    if PROFILING_ENABLED {
        let mut profiler = PROFILER.lock().unwrap();
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
