//! File-level caching for lint results
//!
//! Inspired by Ruff's caching implementation, this module provides fast caching
//! of lint results to avoid re-checking unchanged files.
//!
//! Cache key: (file_content_hash, config_hash, rumdl_version)
//! Cache value: `Vec<LintWarning>`
//! Storage: .rumdl_cache/{version}/{hash}.json

use rumdl_lib::rule::LintWarning;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Per-process counter that disambiguates concurrent temp files written by
/// `atomic_write`. Combined with the process id, this guarantees a unique
/// temp path even when many threads write to the same cache key at once.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `target` atomically by writing to a sibling temp file
/// and renaming into place. Required because parallel workers may produce
/// identical cache keys (same content + config + rules) for different files,
/// and concurrent `fs::write` calls to the same path can interleave bytes
/// because `CacheEntry.timestamp` differs per write.
fn atomic_write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    let tmp_path = target.with_extension(format!("tmp.{}.{counter}", std::process::id()));
    // pid not available on WASI
    #[cfg(target_arch = "wasm32")]
    let tmp_path = target.with_extension(format!("tmp.{counter}"));
    match fs::write(&tmp_path, bytes).and_then(|()| fs::rename(&tmp_path, target)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

/// Reason a cache lookup could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMissReason {
    Disabled,
    MissingEntry { path: PathBuf },
    UnreadableEntry { path: PathBuf, error: String },
    InvalidEntry { path: PathBuf, error: String },
    FileChanged,
    ConfigChanged,
    RulesChanged,
    VersionChanged { cached: String, current: &'static str },
}

impl std::fmt::Display for CacheMissReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "cache is disabled"),
            Self::MissingEntry { path } => write!(f, "no cache entry at {}", path.display()),
            Self::UnreadableEntry { path, error } => {
                write!(f, "could not read cache entry at {}: {error}", path.display())
            }
            Self::InvalidEntry { path, error } => {
                write!(f, "cache entry at {} is invalid: {error}", path.display())
            }
            Self::FileChanged => write!(f, "file content hash changed"),
            Self::ConfigChanged => write!(f, "configuration hash changed"),
            Self::RulesChanged => write!(f, "enabled rules hash changed"),
            Self::VersionChanged { cached, current } => {
                write!(f, "rumdl version changed from {cached} to {current}")
            }
        }
    }
}

/// Cache statistics for reporting
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub writes: usize,
}

impl CacheStats {
    #[cfg(test)]
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

/// The cache directory used when nothing configures one.
pub const DEFAULT_CACHE_DIR: &str = ".rumdl_cache";

/// Resolve the cache directory from its layered sources, in precedence order:
/// an explicit CLI flag, `RUMDL_CACHE_DIR`, the config `cache-dir`, then
/// [`DEFAULT_CACHE_DIR`].
///
/// A leading `~/` expands to the home directory, and a path that is still
/// relative resolves against the project root. Both `check` and `clean` resolve
/// through here: a divergence between them would leave `rumdl clean` wiping a
/// different directory than the one `check` filled.
pub fn resolve_cache_dir(cli: Option<&str>, config: Option<&str>, project_root: Option<&Path>) -> PathBuf {
    let configured = cli
        .map(str::to_string)
        .or_else(|| std::env::var("RUMDL_CACHE_DIR").ok())
        .or_else(|| config.map(str::to_string))
        .unwrap_or_else(|| DEFAULT_CACHE_DIR.to_string());

    let cache_dir = PathBuf::from(rumdl_lib::discovery::expand_home_prefix(&configured).as_ref());

    match project_root {
        Some(root) if cache_dir.is_relative() => root.join(cache_dir),
        _ => cache_dir,
    }
}

/// File-level cache for lint results
pub struct LintCache {
    /// Base cache directory (e.g., .rumdl_cache/)
    cache_dir: PathBuf,
    /// Whether caching is enabled
    enabled: bool,
    /// Cache statistics
    stats: Mutex<CacheStats>,
}

impl LintCache {
    /// Create a new cache instance
    ///
    /// # Arguments
    /// * `cache_dir` - Base directory for cache (e.g., ".rumdl_cache")
    /// * `enabled` - Whether caching is enabled
    pub fn new(cache_dir: PathBuf, enabled: bool) -> Self {
        Self {
            cache_dir,
            enabled,
            stats: Mutex::new(CacheStats::default()),
        }
    }

    fn record_hit(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.hits += 1;
        }
    }

    fn record_miss(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.misses += 1;
        }
    }

    fn record_write(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.writes += 1;
        }
    }

    /// Compute Blake3 hash of content
    pub fn hash_content(content: &str) -> String {
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: hash content", start.elapsed());
        hash
    }

    /// Compute hash of config.
    ///
    /// The hash must be stable across repeated loads of the same config file:
    /// otherwise warm-cache runs see spurious "configuration hash changed" misses.
    /// Hashing serialized JSON means every map field reachable from `Config`
    /// must iterate in a deterministic order. Use `BTreeMap` for keyed config
    /// (sorted), `IndexMap` when config-file order is semantically required
    /// (e.g. `per-file-flavor`'s first-match-wins), or `Vec` for ordered lists.
    /// Never use `HashMap` in a serialized config field — Rust's `RandomState`
    /// randomizes iteration per-instance and breaks this invariant.
    pub fn hash_config(config: &rumdl_lib::config::Config) -> String {
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        // Serialize config to JSON and hash it
        // If serialization fails, return a default hash
        let config_json = serde_json::to_string(config).unwrap_or_default();
        let hash = blake3::hash(config_json.as_bytes()).to_hex().to_string();
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: hash config", start.elapsed());
        hash
    }

    /// Compute hash of enabled rules (Ruff-style)
    /// This ensures different rule configurations get different cache entries
    pub fn hash_rules(rules: &[Box<dyn rumdl_lib::rule::Rule>]) -> String {
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        // Sort rule names for deterministic hashing
        let mut rule_names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        rule_names.sort_unstable();

        // Hash the sorted rule names
        let rules_str = rule_names.join(",");
        let hash = blake3::hash(rules_str.as_bytes()).to_hex().to_string();
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: hash rules", start.elapsed());
        hash
    }

    /// Get the cache file path for a given content and config hash
    /// Includes rules_hash in filename to separate different rule configurations
    fn cache_file_path(&self, file_hash: &str, rules_hash: &str) -> PathBuf {
        // Use 16 chars of rules_hash to reduce collision probability
        // (8 chars = 2^32 combinations, 16 chars = 2^64 combinations)
        let short_rules_hash = &rules_hash[..16];
        self.cache_dir
            .join(VERSION)
            .join(format!("{file_hash}_{short_rules_hash}.json"))
    }

    /// Try to get cached results for a file
    ///
    /// Returns Some(warnings) if cache hit, None if cache miss
    #[cfg(test)]
    pub fn get(&self, content: &str, config_hash: &str, rules_hash: &str) -> Option<Vec<LintWarning>> {
        self.get_with_reason(content, config_hash, rules_hash).ok()
    }

    /// Try to get cached results for a file, preserving the miss reason for diagnostics.
    #[cfg(test)]
    pub fn get_with_reason(
        &self,
        content: &str,
        config_hash: &str,
        rules_hash: &str,
    ) -> Result<Vec<LintWarning>, CacheMissReason> {
        let file_hash = Self::hash_content(content);
        self.get_with_reason_for_hash(&file_hash, config_hash, rules_hash)
    }

    /// Try to get cached results for a precomputed file hash.
    pub fn get_with_reason_for_hash(
        &self,
        file_hash: &str,
        config_hash: &str,
        rules_hash: &str,
    ) -> Result<Vec<LintWarning>, CacheMissReason> {
        if !self.enabled {
            return Err(CacheMissReason::Disabled);
        }

        let cache_path = self.cache_file_path(file_hash, rules_hash);

        // Try to read cache file
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        let cache_data = match fs::read_to_string(&cache_path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(feature = "profiling")]
                rumdl_lib::profiling::record_duration("cache: read entry", start.elapsed());
                self.record_miss();
                return Err(CacheMissReason::MissingEntry { path: cache_path });
            }
            Err(e) => {
                #[cfg(feature = "profiling")]
                rumdl_lib::profiling::record_duration("cache: read entry", start.elapsed());
                self.record_miss();
                return Err(CacheMissReason::UnreadableEntry {
                    path: cache_path,
                    error: e.to_string(),
                });
            }
        };
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: read entry", start.elapsed());

        // Try to parse cache entry
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        let entry: CacheEntry = match serde_json::from_str(&cache_data) {
            Ok(entry) => entry,
            Err(e) => {
                #[cfg(feature = "profiling")]
                rumdl_lib::profiling::record_duration("cache: parse entry", start.elapsed());
                self.record_miss();
                return Err(CacheMissReason::InvalidEntry {
                    path: cache_path,
                    error: e.to_string(),
                });
            }
        };
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: parse entry", start.elapsed());

        // Validate cache entry (Ruff-style: file content + config + enabled rules)
        if entry.file_hash != file_hash {
            self.record_miss();
            return Err(CacheMissReason::FileChanged);
        }
        if entry.config_hash != config_hash {
            self.record_miss();
            return Err(CacheMissReason::ConfigChanged);
        }
        if entry.rules_hash != rules_hash {
            self.record_miss();
            return Err(CacheMissReason::RulesChanged);
        }
        if entry.version != VERSION {
            self.record_miss();
            return Err(CacheMissReason::VersionChanged {
                cached: entry.version,
                current: VERSION,
            });
        }

        // Cache hit!
        self.record_hit();
        Ok(entry.warnings)
    }

    /// Store lint results in cache
    #[cfg(test)]
    pub fn set(&self, content: &str, config_hash: &str, rules_hash: &str, warnings: Vec<LintWarning>) {
        let file_hash = Self::hash_content(content);
        self.set_with_hash(&file_hash, config_hash, rules_hash, warnings);
    }

    /// Store lint results in cache using a precomputed file hash.
    pub fn set_with_hash(&self, file_hash: &str, config_hash: &str, rules_hash: &str, warnings: Vec<LintWarning>) {
        if !self.enabled {
            return;
        }

        let cache_path = self.cache_file_path(file_hash, rules_hash);

        // Create cache directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Create cache entry
        let entry = CacheEntry {
            file_hash: file_hash.to_string(),
            config_hash: config_hash.to_string(),
            rules_hash: rules_hash.to_string(),
            version: VERSION.to_string(),
            warnings,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Write to cache (log errors but don't fail - cache is optional)
        #[cfg(feature = "profiling")]
        let start = std::time::Instant::now();
        let json = serde_json::to_string_pretty(&entry);
        #[cfg(feature = "profiling")]
        rumdl_lib::profiling::record_duration("cache: serialize entry", start.elapsed());

        if let Ok(json) = json {
            #[cfg(feature = "profiling")]
            let start = std::time::Instant::now();
            match atomic_write(&cache_path, json.as_bytes()) {
                Ok(()) => self.record_write(),
                Err(e) => log::debug!("Cache write failed for {}: {}", cache_path.display(), e),
            }
            #[cfg(feature = "profiling")]
            rumdl_lib::profiling::record_duration("cache: write entry", start.elapsed());
        }
    }

    /// Clear the entire cache
    pub fn clear(&self) -> std::io::Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Initialize cache directory structure
    ///
    /// This also prunes cache directories from old rumdl versions to prevent
    /// unbounded cache growth across version upgrades.
    pub fn init(&self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Create version-specific directory
        let version_dir = self.cache_dir.join(VERSION);
        fs::create_dir_all(&version_dir)?;

        // Prune old version directories
        self.prune_old_versions()?;

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

    /// Remove cache directories from old rumdl versions
    ///
    /// Scans the cache directory for version subdirectories and removes any
    /// that don't match the current version. This handles version upgrades
    /// gracefully without manual intervention.
    fn prune_old_versions(&self) -> std::io::Result<()> {
        if !self.cache_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.cache_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();

            // Skip non-directories and special files
            if !path.is_dir() {
                continue;
            }

            // Check if this is a version directory (matches semver pattern)
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip current version
                if dir_name == VERSION {
                    continue;
                }

                // Check if it looks like a version directory (starts with digit)
                if dir_name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    log::info!("Pruning old cache version: {dir_name}");
                    if let Err(e) = fs::remove_dir_all(&path) {
                        log::warn!("Failed to prune old cache {dir_name}: {e}");
                    }
                }
            }
        }

        Ok(())
    }

    /// Get cache statistics
    #[cfg(test)]
    pub fn stats(&self) -> CacheStats {
        self.stats.lock().map(|stats| stats.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), false);

        let content = "# Test";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // Should return None when disabled
        assert!(cache.get(content, config_hash, rules_hash).is_none());

        // Set should be no-op when disabled
        cache.set(content, config_hash, rules_hash, vec![]);
        assert_eq!(cache.stats().writes, 0);
    }

    #[test]
    fn test_cache_miss() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);

        let content = "# Test";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // First access should be a miss
        assert!(cache.get(content, config_hash, rules_hash).is_none());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn test_cache_miss_reason_missing_entry() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);

        let content = "# Test";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        let reason = cache
            .get_with_reason(content, config_hash, rules_hash)
            .expect_err("empty cache should miss");
        assert!(matches!(reason, CacheMissReason::MissingEntry { .. }));
        assert!(reason.to_string().contains("no cache entry at"));
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_cache_hit() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let warnings = vec![];

        // Store in cache
        cache.set(content, config_hash, rules_hash, warnings.clone());

        // Should hit cache
        let cached = cache.get(content, config_hash, rules_hash);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), warnings);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_invalidation_on_content_change() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content1 = "# Test 1";
        let content2 = "# Test 2";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // Cache content1
        cache.set(content1, config_hash, rules_hash, vec![]);

        // content2 should miss (different content)
        assert!(cache.get(content2, config_hash, rules_hash).is_none());
    }

    #[test]
    fn test_cache_invalidation_on_config_change() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash1 = "abc123";
        let config_hash2 = "def456";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // Cache with config1
        cache.set(content, config_hash1, rules_hash, vec![]);

        // Should miss with config2 (different config)
        assert!(cache.get(content, config_hash2, rules_hash).is_none());
    }

    #[test]
    fn test_cache_miss_reason_config_changed() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash1 = "abc123";
        let config_hash2 = "def456";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        cache.set(content, config_hash1, rules_hash, vec![]);

        let reason = cache
            .get_with_reason(content, config_hash2, rules_hash)
            .expect_err("changed config hash should miss");
        assert_eq!(reason, CacheMissReason::ConfigChanged);
        assert_eq!(reason.to_string(), "configuration hash changed");
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
    fn test_hash_config_is_stable_across_repeated_config_loads() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let mut config_content = String::from(
            r#"
[global]
line-length = 100

[per-file-ignores]
"#,
        );
        for i in 0..64 {
            config_content.push_str(&format!("\"docs/section-{i:02}/**/*.md\" = [\"MD013\", \"MD033\"]\n"));
        }
        std::fs::write(&config_path, config_content).unwrap();

        let mut hashes = std::collections::BTreeSet::new();
        for _ in 0..128 {
            let sourced =
                rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                    .unwrap();
            let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
            hashes.insert(LintCache::hash_config(&config));
        }

        let unique_count = hashes.len();
        let sample: Vec<_> = hashes.iter().take(3).cloned().collect();
        assert_eq!(
            unique_count, 1,
            "loading the same config repeatedly must produce one stable config hash, got {unique_count} unique hashes; sample: {sample:?}",
        );
    }

    #[test]
    fn test_hash_config_is_stable_with_code_block_tools_maps() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let mut config_content = String::from(
            r#"
[global]
line-length = 100

[code-block-tools]
enabled = true

[code-block-tools.language-aliases]
"#,
        );
        for i in 0..32 {
            config_content.push_str(&format!("\"alias-{i:02}\" = \"lang-{i:02}\"\n"));
        }
        for i in 0..32 {
            config_content.push_str(&format!(
                "\n[code-block-tools.languages.\"lang-{i:02}\"]\nenabled = false\n",
            ));
            config_content.push_str(&format!(
                "\n[code-block-tools.tools.\"tool-{i:02}\"]\ncommand = [\"tool-{i:02}\"]\n",
            ));
        }
        std::fs::write(&config_path, config_content).unwrap();

        let mut hashes = std::collections::BTreeSet::new();
        for _ in 0..128 {
            let sourced =
                rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                    .unwrap();
            let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
            hashes.insert(LintCache::hash_config(&config));
        }

        let unique_count = hashes.len();
        let sample: Vec<_> = hashes.iter().take(3).cloned().collect();
        assert_eq!(
            unique_count, 1,
            "code-block-tools maps must serialize deterministically, got {unique_count} unique hashes; sample: {sample:?}",
        );
    }

    #[test]
    fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let content = "# Test";
        let config_hash = "abc123";
        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // Miss
        cache.get(content, config_hash, rules_hash);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        // Write
        cache.set(content, config_hash, rules_hash, vec![]);
        assert_eq!(cache.stats().writes, 1);

        // Hit
        cache.get(content, config_hash, rules_hash);
        assert_eq!(cache.stats().hits, 1);

        // Hit rate
        assert_eq!(cache.stats().hit_rate(), 50.0); // 1 hit out of 2 total
    }

    #[test]
    fn test_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let cache = LintCache::new(temp_dir.path().to_path_buf(), true);
        cache.init().unwrap();

        let rules_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        // Add something to cache
        cache.set("# Test", "abc", rules_hash, vec![]);

        // Clear cache
        cache.clear().unwrap();

        // Cache directory should be gone
        assert!(!cache.cache_dir.exists());
    }

    #[test]
    fn test_prune_old_versions() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();

        // Create some fake old version directories
        fs::create_dir_all(cache_dir.join("0.0.1")).unwrap();
        fs::create_dir_all(cache_dir.join("0.0.50")).unwrap();
        fs::create_dir_all(cache_dir.join("0.0.100")).unwrap();
        fs::write(cache_dir.join("0.0.1").join("test.json"), "{}").unwrap();
        fs::write(cache_dir.join("0.0.50").join("test.json"), "{}").unwrap();

        // Create a non-version directory (should not be pruned)
        fs::create_dir_all(cache_dir.join("some_other_dir")).unwrap();

        // Initialize cache (should prune old versions)
        let cache = LintCache::new(cache_dir.clone(), true);
        cache.init().unwrap();

        // Current version directory should exist
        assert!(cache_dir.join(VERSION).exists());

        // Old version directories should be removed
        assert!(!cache_dir.join("0.0.1").exists());
        assert!(!cache_dir.join("0.0.50").exists());
        assert!(!cache_dir.join("0.0.100").exists());

        // Non-version directory should still exist
        assert!(cache_dir.join("some_other_dir").exists());
    }

    /// Concurrent writers targeting the same cache key must never produce a
    /// truncated or interleaved file. With direct `fs::write` two parallel
    /// `open(O_TRUNC) + write` sequences can race; the atomic tempfile +
    /// rename path serializes via the filesystem so the final contents are
    /// always one of the writers' full payloads.
    #[test]
    fn test_atomic_write_concurrent_no_corruption() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let target = Arc::new(temp_dir.path().join("entry.json"));

        let mut handles = Vec::new();
        for writer_id in 0..16u8 {
            let target = Arc::clone(&target);
            handles.push(thread::spawn(move || {
                // Distinct payload per thread; size varies so a partial write
                // would be detectable. Repeated 4096 times to exceed typical
                // pipe-buffer / single-write atomicity boundaries.
                let payload = vec![b'a' + writer_id; 4096 * (writer_id as usize + 1)];
                for _ in 0..32 {
                    atomic_write(&target, &payload).expect("atomic write succeeds");
                }
                payload
            }));
        }

        let payloads: Vec<Vec<u8>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let final_bytes = fs::read(&*target).expect("target file readable");

        assert!(
            payloads.iter().any(|p| p == &final_bytes),
            "final cache file must equal exactly one writer's full payload, got {} bytes",
            final_bytes.len()
        );
    }
}
