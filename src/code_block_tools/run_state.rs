//! Invocation-scoped accounting, shared by files and tool phases.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub struct RunState {
    executions: AtomicUsize,
    aborted: AtomicBool,
    cached_checks: AtomicUsize,
    warnings: Mutex<BTreeMap<String, String>>,
}

// Operational state is excluded from configuration serialization and identity.
impl PartialEq for RunState {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl Eq for RunState {}

impl RunState {
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::Relaxed);
    }
    pub fn aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }
    pub fn record_execution(&self) {
        self.executions.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_cached_check(&self) {
        self.cached_checks.fetch_add(1, Ordering::Relaxed);
    }
    pub fn checked_anything(&self) -> bool {
        self.executions.load(Ordering::Relaxed) > 0 || self.cached_checks.load(Ordering::Relaxed) > 0
    }
    pub fn warn(&self, key: String, message: String) {
        self.warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key)
            .or_insert(message);
    }
    pub fn warnings(&self) -> Vec<String> {
        self.warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }
}
