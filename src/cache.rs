//! File-level caching for lint results
//!
//! Inspired by Ruff's caching implementation, this module provides fast caching
//! of lint results to avoid re-checking unchanged files.
//!
//! Cache key: (file_content_hash, config_hash, rumdl_version)
//! Cache value: Vec<LintWarning>
//! Storage: .rumdl-cache/{version}/{hash}.json

use rumdl_lib::rule::LintWarning;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cache statistics for reporting
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub writes: usize,
}

impl CacheStats {
    #[allow(dead_code)]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

/// A cache entry stored on disk
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// Blake3 hash of file content
    file_hash: String,
    /// Blake3 hash of config
    config_hash: String,
    /// Blake3 hash of enabled rules (sorted rule names)
    rules_hash: String,
    /// rumdl version
    version: String,
    /// Cached lint warnings
    warnings: Vec<LintWarning>,
    /// Timestamp when cached (Unix timestamp)
    timestamp: i64,
}

/// File-level cache for lint results
pub struct LintCache {
    /// Base cache directory (e.g., .rumdl-cache/)
    cache_dir: PathBuf,
    /// Whether caching is enabled
    enabled: bool,
    /// Cache statistics
    stats: CacheStats,
}

impl LintCache {
    /// Create a new cache instance
    ///
    /// # Arguments
    /// * `cache_dir` - Base directory for cache (e.g., ".rumdl-cache")
    /// * `enabled` - Whether caching is enabled
    #[allow(dead_code)]
    pub fn new(cache_dir: PathBuf, enabled: bool) -> Self {
        Self {
            cache_dir,
            enabled,
            stats: CacheStats::default(),
        }
    }

    /// Get default cache directory for a project
    #[allow(dead_code)]
    pub fn default_cache_dir() -> PathBuf {
        PathBuf::from(".rumdl-cache")
    }

    /// Compute Blake3 hash of content
    fn hash_content(content: &str) -> String {
        blake3::hash(content.as_bytes()).to_hex().to_string()
    }

    /// Compute hash of config
    /// This is a public function that can be called from file_processor
    pub fn hash_config(config: &rumdl_lib::config::Config) -> String {
        // Serialize config to JSON and hash it
        // If serialization fails, return a default hash
        let config_json = serde_json::to_string(config).unwrap_or_default();
        blake3::hash(config_json.as_bytes()).to_hex().to_string()
    }

    /// Compute hash of enabled rules (Ruff-style)
    /// This ensures different rule configurations get different cache entries
    pub fn hash_rules(rules: &[Box<dyn rumdl_lib::rule::Rule>]) -> String {
        // Sort rule names for deterministic hashing
        let mut rule_names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        rule_names.sort_unstable();

        // Hash the sorted rule names
        let rules_str = rule_names.join(",");
        blake3::hash(rules_str.as_bytes()).to_hex().to_string()
    }

    /// Get the cache file path for a given content and config hash
    /// Includes rules_hash in filename to separate different rule configurations
    fn cache_file_path(&self, file_hash: &str, rules_hash: &str) -> PathBuf {
        // Include first 8 chars of rules_hash to keep filenames reasonable
        let short_rules_hash = &rules_hash[..8];
        self.cache_dir
            .join(VERSION)
            .join(format!("{file_hash}_{short_rules_hash}.json"))
    }

    /// Try to get cached results for a file
    ///
    /// Returns Some(warnings) if cache hit, None if cache miss
    pub fn get(&mut self, content: &str, config_hash: &str, rules_hash: &str) -> Option<Vec<LintWarning>> {
        if !self.enabled {
            return None;
        }

        let file_hash = Self::hash_content(content);
        let cache_path = self.cache_file_path(&file_hash, rules_hash);

        // Try to read cache file
        let cache_data = match fs::read_to_string(&cache_path) {
            Ok(data) => data,
            Err(_) => {
                self.stats.misses += 1;
                return None;
            }
        };

        // Try to parse cache entry
        let entry: CacheEntry = match serde_json::from_str(&cache_data) {
            Ok(entry) => entry,
            Err(_) => {
                self.stats.misses += 1;
                return None;
            }
        };

        // Validate cache entry (Ruff-style: file content + config + enabled rules)
        if entry.file_hash != file_hash
            || entry.config_hash != config_hash
            || entry.rules_hash != rules_hash
            || entry.version != VERSION
        {
            self.stats.misses += 1;
            return None;
        }

        // Cache hit!
        self.stats.hits += 1;
        Some(entry.warnings)
    }

    /// Store lint results in cache
    pub fn set(&mut self, content: &str, config_hash: &str, rules_hash: &str, warnings: Vec<LintWarning>) {
        if !self.enabled {
            return;
        }

        let file_hash = Self::hash_content(content);
        let cache_path = self.cache_file_path(&file_hash, rules_hash);

        // Create cache directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Create cache entry
        let entry = CacheEntry {
            file_hash,
            config_hash: config_hash.to_string(),
            rules_hash: rules_hash.to_string(),
            version: VERSION.to_string(),
            warnings,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Write to cache (ignore errors - cache is optional)
        if let Ok(json) = serde_json::to_string_pretty(&entry) {
            let _ = fs::write(&cache_path, json);
            self.stats.writes += 1;
        }
    }

    /// Clear the entire cache
    #[allow(dead_code)]
    pub fn clear(&self) -> std::io::Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Initialize cache directory structure
    #[allow(dead_code)]
    pub fn init(&self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Create version-specific directory
        let version_dir = self.cache_dir.join(VERSION);
        fs::create_dir_all(&version_dir)?;

        // Create .gitignore if it doesn't exist
        let gitignore_path = self.cache_dir.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(gitignore_path, "# Automatically created by rumdl.\n*\n")?;
        }

        // Create CACHEDIR.TAG file (standard cache directory marker)
        let cachedir_tag = self.cache_dir.join("CACHEDIR.TAG");
        if !cachedir_tag.exists() {
            fs::write(
                cachedir_tag,
                "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by rumdl.\n",
            )?;
        }

        Ok(())
    }

    /// Get cache statistics
    #[allow(dead_code)]
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Check if caching is enabled
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), false);

        let content = "# Test";
        let config_hash = "abc123";

        // Should return None when disabled
        assert!(cache.get(content, config_hash, "test_rules_hash").is_none());

        // Set should be no-op when disabled
        cache.set(content, config_hash, "test_rules_hash", vec![]);
        assert_eq!(cache.stats().writes, 0);
    }

    #[test]
    fn test_cache_miss() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);

        let content = "# Test";
        let config_hash = "abc123";

        // First access should be a miss
        assert!(cache.get(content, config_hash, "test_rules_hash").is_none());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn test_cache_hit() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash = "abc123";
        let warnings = vec![];

        // Store in cache
        cache.set(content, config_hash, "test_rules_hash", warnings.clone());

        // Should hit cache
        let cached = cache.get(content, config_hash, "test_rules_hash");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), warnings);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_invalidation_on_content_change() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content1 = "# Test 1";
        let content2 = "# Test 2";
        let config_hash = "abc123";

        // Cache content1
        cache.set(content1, config_hash, "test_rules_hash", vec![]);

        // content2 should miss (different content)
        assert!(cache.get(content2, config_hash, "test_rules_hash").is_none());
    }

    #[test]
    fn test_cache_invalidation_on_config_change() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash1 = "abc123";
        let config_hash2 = "def456";

        // Cache with config1
        cache.set(content, config_hash1, "test_rules_hash", vec![]);

        // Should miss with config2 (different config)
        assert!(cache.get(content, config_hash2, "test_rules_hash").is_none());
    }

    #[test]
    fn test_hash_content() {
        let content1 = "# Test";
        let content2 = "# Test";
        let content3 = "# Different";

        let hash1 = LintCache::hash_content(content1);
        let hash2 = LintCache::hash_content(content2);
        let hash3 = LintCache::hash_content(content3);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash = "abc123";

        // Miss
        cache.get(content, config_hash, "test_rules_hash");
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        // Write
        cache.set(content, config_hash, "test_rules_hash", vec![]);
        assert_eq!(cache.stats().writes, 1);

        // Hit
        cache.get(content, config_hash, "test_rules_hash");
        assert_eq!(cache.stats().hits, 1);

        // Hit rate
        assert_eq!(cache.stats().hit_rate(), 50.0); // 1 hit out of 2 total
    }

    #[test]
    fn test_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        // Add something to cache
        cache.set("# Test", "abc", "test_rules_hash", vec![]);

        // Clear cache
        cache.clear().unwrap();

        // Cache directory should be gone
        assert!(!cache.cache_dir.exists());
    }
}
