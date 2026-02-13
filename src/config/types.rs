use crate::types::LineLength;
use globset::{Glob, GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::flavor::{MarkdownFlavor, normalize_key};

/// Represents a rule-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct RuleConfig {
    /// Severity override for this rule (Error, Warning, or Info)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<crate::rule::Severity>,

    /// Configuration values for the rule
    #[serde(flatten)]
    #[schemars(schema_with = "arbitrary_value_schema")]
    pub values: BTreeMap<String, toml::Value>,
}

/// Generate a JSON schema for arbitrary configuration values
fn arbitrary_value_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

/// Represents the complete configuration loaded from rumdl.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[schemars(
    description = "rumdl configuration for linting Markdown files. Rules can be configured individually using [MD###] sections with rule-specific options."
)]
pub struct Config {
    /// Global configuration options
    #[serde(default)]
    pub global: GlobalConfig,

    /// Per-file rule ignores: maps file patterns to lists of rules to ignore
    /// Example: { "README.md": ["MD033"], "docs/**/*.md": ["MD013"] }
    #[serde(default, rename = "per-file-ignores")]
    pub per_file_ignores: HashMap<String, Vec<String>>,

    /// Per-file flavor overrides: maps file patterns to Markdown flavors
    /// Example: { "docs/**/*.md": MkDocs, "**/*.mdx": MDX }
    /// Uses IndexMap to preserve config file order for "first match wins" semantics
    #[serde(default, rename = "per-file-flavor")]
    #[schemars(with = "HashMap<String, MarkdownFlavor>")]
    pub per_file_flavor: IndexMap<String, MarkdownFlavor>,

    /// Code block tools configuration for per-language linting and formatting
    /// using external tools like ruff, prettier, shellcheck, etc.
    #[serde(default, rename = "code-block-tools")]
    pub code_block_tools: crate::code_block_tools::CodeBlockToolsConfig,

    /// Rule-specific configurations (e.g., MD013, MD007, MD044)
    /// Each rule section can contain options specific to that rule.
    ///
    /// Common examples:
    /// - MD013: line_length, code_blocks, tables, headings
    /// - MD007: indent
    /// - MD003: style ("atx", "atx_closed", "setext")
    /// - MD044: names (array of proper names to check)
    ///
    /// See <https://github.com/rvben/rumdl> for full rule documentation.
    #[serde(flatten)]
    pub rules: BTreeMap<String, RuleConfig>,

    /// Project root directory, used for resolving relative paths in per-file-ignores
    #[serde(skip)]
    pub project_root: Option<std::path::PathBuf>,

    #[serde(skip)]
    #[schemars(skip)]
    pub(super) per_file_ignores_cache: Arc<OnceLock<PerFileIgnoreCache>>,

    #[serde(skip)]
    #[schemars(skip)]
    pub(super) per_file_flavor_cache: Arc<OnceLock<PerFileFlavorCache>>,
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.global == other.global
            && self.per_file_ignores == other.per_file_ignores
            && self.per_file_flavor == other.per_file_flavor
            && self.code_block_tools == other.code_block_tools
            && self.rules == other.rules
            && self.project_root == other.project_root
    }
}

#[derive(Debug)]
pub(super) struct PerFileIgnoreCache {
    globset: GlobSet,
    rules: Vec<Vec<String>>,
}

#[derive(Debug)]
pub(super) struct PerFileFlavorCache {
    matchers: Vec<(GlobMatcher, MarkdownFlavor)>,
}

impl Config {
    /// Check if the Markdown flavor is set to MkDocs
    pub fn is_mkdocs_flavor(&self) -> bool {
        self.global.flavor == MarkdownFlavor::MkDocs
    }

    // Future methods for when GFM and CommonMark are implemented:
    // pub fn is_gfm_flavor(&self) -> bool
    // pub fn is_commonmark_flavor(&self) -> bool

    /// Get the configured Markdown flavor
    pub fn markdown_flavor(&self) -> MarkdownFlavor {
        self.global.flavor
    }

    /// Legacy method for backwards compatibility - redirects to is_mkdocs_flavor
    pub fn is_mkdocs_project(&self) -> bool {
        self.is_mkdocs_flavor()
    }

    /// Get the severity override for a specific rule, if configured
    pub fn get_rule_severity(&self, rule_name: &str) -> Option<crate::rule::Severity> {
        self.rules.get(rule_name).and_then(|r| r.severity)
    }

    /// Get the set of rules that should be ignored for a specific file based on per-file-ignores configuration
    /// Returns a HashSet of rule names (uppercase, e.g., "MD033") that match the given file path
    pub fn get_ignored_rules_for_file(&self, file_path: &Path) -> HashSet<String> {
        let mut ignored_rules = HashSet::new();

        if self.per_file_ignores.is_empty() {
            return ignored_rules;
        }

        // Normalize the file path to be relative to project_root for pattern matching
        // This ensures patterns like ".github/file.md" work with absolute paths
        let path_for_matching: std::borrow::Cow<'_, Path> = if let Some(ref root) = self.project_root {
            if let Ok(canonical_path) = file_path.canonicalize() {
                if let Ok(canonical_root) = root.canonicalize() {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
                        std::borrow::Cow::Owned(relative.to_path_buf())
                    } else {
                        std::borrow::Cow::Borrowed(file_path)
                    }
                } else {
                    std::borrow::Cow::Borrowed(file_path)
                }
            } else {
                std::borrow::Cow::Borrowed(file_path)
            }
        } else {
            std::borrow::Cow::Borrowed(file_path)
        };

        let cache = self
            .per_file_ignores_cache
            .get_or_init(|| PerFileIgnoreCache::new(&self.per_file_ignores));

        // Match the file path against all patterns
        for match_idx in cache.globset.matches(path_for_matching.as_ref()) {
            if let Some(rules) = cache.rules.get(match_idx) {
                for rule in rules.iter() {
                    // Normalize rule names to uppercase (MD033, md033 -> MD033)
                    ignored_rules.insert(rule.clone());
                }
            }
        }

        ignored_rules
    }

    /// Get the MarkdownFlavor for a specific file based on per-file-flavor configuration.
    /// Returns the first matching pattern's flavor, or falls back to global flavor,
    /// or auto-detects from extension, or defaults to Standard.
    pub fn get_flavor_for_file(&self, file_path: &Path) -> MarkdownFlavor {
        // If no per-file patterns, use fallback logic
        if self.per_file_flavor.is_empty() {
            return self.resolve_flavor_fallback(file_path);
        }

        // Normalize path for matching (same logic as get_ignored_rules_for_file)
        let path_for_matching: std::borrow::Cow<'_, Path> = if let Some(ref root) = self.project_root {
            if let Ok(canonical_path) = file_path.canonicalize() {
                if let Ok(canonical_root) = root.canonicalize() {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
                        std::borrow::Cow::Owned(relative.to_path_buf())
                    } else {
                        std::borrow::Cow::Borrowed(file_path)
                    }
                } else {
                    std::borrow::Cow::Borrowed(file_path)
                }
            } else {
                std::borrow::Cow::Borrowed(file_path)
            }
        } else {
            std::borrow::Cow::Borrowed(file_path)
        };

        let cache = self
            .per_file_flavor_cache
            .get_or_init(|| PerFileFlavorCache::new(&self.per_file_flavor));

        // Iterate in config order and return first match (IndexMap preserves order)
        for (matcher, flavor) in &cache.matchers {
            if matcher.is_match(path_for_matching.as_ref()) {
                return *flavor;
            }
        }

        // No pattern matched, use fallback
        self.resolve_flavor_fallback(file_path)
    }

    /// Fallback flavor resolution: global flavor → auto-detect → Standard
    fn resolve_flavor_fallback(&self, file_path: &Path) -> MarkdownFlavor {
        // If global flavor is explicitly set to non-Standard, use it
        if self.global.flavor != MarkdownFlavor::Standard {
            return self.global.flavor;
        }
        // Auto-detect from extension
        MarkdownFlavor::from_path(file_path)
    }

    /// Merge inline configuration overrides into a copy of this config
    ///
    /// This enables automatic inline config support - the engine can merge
    /// inline overrides and recreate rules without any per-rule changes.
    ///
    /// Returns a new Config with the inline overrides merged in.
    /// If there are no inline overrides, returns a clone of self.
    pub fn merge_with_inline_config(&self, inline_config: &crate::inline_config::InlineConfig) -> Self {
        let overrides = inline_config.get_all_rule_configs();
        if overrides.is_empty() {
            return self.clone();
        }

        let mut merged = self.clone();

        for (rule_name, json_override) in overrides {
            // Get or create the rule config entry
            let rule_config = merged.rules.entry(rule_name.clone()).or_default();

            // Merge JSON values into the rule's config
            if let Some(obj) = json_override.as_object() {
                for (key, value) in obj {
                    // Normalize key to kebab-case for consistency
                    let normalized_key = key.replace('_', "-");

                    // Convert JSON value to TOML value
                    if let Some(toml_value) = json_to_toml(value) {
                        rule_config.values.insert(normalized_key, toml_value);
                    }
                }
            }
        }

        merged
    }
}

/// Convert a serde_json::Value to a toml::Value
pub(super) fn json_to_toml(json: &serde_json::Value) -> Option<toml::Value> {
    match json {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().map(toml::Value::Float)),
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let toml_arr: Vec<toml::Value> = arr.iter().filter_map(json_to_toml).collect();
            Some(toml::Value::Array(toml_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                if let Some(tv) = json_to_toml(v) {
                    table.insert(k.clone(), tv);
                }
            }
            Some(toml::Value::Table(table))
        }
    }
}

impl PerFileIgnoreCache {
    fn new(per_file_ignores: &HashMap<String, Vec<String>>) -> Self {
        let mut builder = GlobSetBuilder::new();
        let mut rules = Vec::new();

        for (pattern, rules_list) in per_file_ignores {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
                rules.push(rules_list.iter().map(|rule| normalize_key(rule)).collect());
            } else {
                log::warn!("Invalid glob pattern in per-file-ignores: {pattern}");
            }
        }

        let globset = builder.build().unwrap_or_else(|e| {
            log::error!("Failed to build globset for per-file-ignores: {e}");
            GlobSetBuilder::new().build().unwrap()
        });

        Self { globset, rules }
    }
}

impl PerFileFlavorCache {
    fn new(per_file_flavor: &IndexMap<String, MarkdownFlavor>) -> Self {
        let mut matchers = Vec::new();

        for (pattern, flavor) in per_file_flavor {
            if let Ok(glob) = GlobBuilder::new(pattern).literal_separator(true).build() {
                matchers.push((glob.compile_matcher(), *flavor));
            } else {
                log::warn!("Invalid glob pattern in per-file-flavor: {pattern}");
            }
        }

        Self { matchers }
    }
}

/// Global configuration options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(default, rename_all = "kebab-case")]
pub struct GlobalConfig {
    /// Enabled rules
    #[serde(default)]
    pub enable: Vec<String>,

    /// Disabled rules
    #[serde(default)]
    pub disable: Vec<String>,

    /// Files to exclude
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Files to include
    #[serde(default)]
    pub include: Vec<String>,

    /// Respect .gitignore files when scanning directories
    #[serde(default = "default_respect_gitignore", alias = "respect_gitignore")]
    pub respect_gitignore: bool,

    /// Global line length setting (used by MD013 and other rules if not overridden)
    #[serde(default, alias = "line_length")]
    pub line_length: LineLength,

    /// Output format for linting results (e.g., "text", "json", "pylint", etc.)
    #[serde(skip_serializing_if = "Option::is_none", alias = "output_format")]
    pub output_format: Option<String>,

    /// Rules that are allowed to be fixed when --fix is used
    /// If specified, only these rules will be fixed
    #[serde(default)]
    pub fixable: Vec<String>,

    /// Rules that should never be fixed, even when --fix is used
    /// Takes precedence over fixable
    #[serde(default)]
    pub unfixable: Vec<String>,

    /// Markdown flavor/dialect to use (mkdocs, gfm, commonmark, etc.)
    /// When set, adjusts parsing and validation rules for that specific Markdown variant
    #[serde(default)]
    pub flavor: MarkdownFlavor,

    /// \[DEPRECATED\] Whether to enforce exclude patterns for explicitly passed paths.
    /// This option is deprecated as of v0.0.156 and has no effect.
    /// Exclude patterns are now always respected, even for explicitly provided files.
    /// This prevents duplication between rumdl config and tool configs like pre-commit.
    #[serde(default, alias = "force_exclude")]
    #[deprecated(since = "0.0.156", note = "Exclude patterns are now always respected")]
    pub force_exclude: bool,

    /// Directory to store cache files (default: .rumdl_cache)
    /// Can also be set via --cache-dir CLI flag or RUMDL_CACHE_DIR environment variable
    #[serde(default, alias = "cache_dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,

    /// Whether caching is enabled (default: true)
    /// Can also be disabled via --no-cache CLI flag
    #[serde(default = "default_true")]
    pub cache: bool,

    /// Additional rules to enable on top of the base set (additive)
    #[serde(default, alias = "extend_enable")]
    pub extend_enable: Vec<String>,

    /// Additional rules to disable on top of the base set (additive)
    #[serde(default, alias = "extend_disable")]
    pub extend_disable: Vec<String>,

    /// Whether the enable list was explicitly set (even if empty).
    /// Used to distinguish "no enable list configured" from "enable list is empty"
    /// (e.g., markdownlint `default: false` with no rules enabled).
    #[serde(skip)]
    pub enable_is_explicit: bool,
}

fn default_respect_gitignore() -> bool {
    true
}

fn default_true() -> bool {
    true
}

// Add the Default impl
impl Default for GlobalConfig {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            enable: Vec::new(),
            disable: Vec::new(),
            exclude: Vec::new(),
            include: Vec::new(),
            respect_gitignore: true,
            line_length: LineLength::default(),
            output_format: None,
            fixable: Vec::new(),
            unfixable: Vec::new(),
            flavor: MarkdownFlavor::default(),
            force_exclude: false,
            cache_dir: None,
            cache: true,
            extend_enable: Vec::new(),
            extend_disable: Vec::new(),
            enable_is_explicit: false,
        }
    }
}

pub(super) const MARKDOWNLINT_CONFIG_FILES: &[&str] = &[
    ".markdownlint.json",
    ".markdownlint.jsonc",
    ".markdownlint.yaml",
    ".markdownlint.yml",
    "markdownlint.json",
    "markdownlint.jsonc",
    "markdownlint.yaml",
    "markdownlint.yml",
];

/// Create a default configuration file at the specified path
pub fn create_default_config(path: &str) -> Result<(), ConfigError> {
    // Check if file already exists
    if Path::new(path).exists() {
        return Err(ConfigError::FileExists { path: path.to_string() });
    }

    // Default configuration content
    let default_config = r#"# rumdl configuration file

# Inherit settings from another config file (relative to this file's directory)
# extends = "../base.rumdl.toml"

# Global configuration options
[global]
# List of rules to disable (uncomment and modify as needed)
# disable = ["MD013", "MD033"]

# List of rules to enable exclusively (replaces defaults; only these rules will run)
# enable = ["MD001", "MD003", "MD004"]

# Additional rules to enable on top of defaults (additive, does not replace)
# Use this to activate opt-in rules like MD060, MD063, MD072, MD073, MD074
# extend-enable = ["MD060", "MD063"]

# Additional rules to disable on top of the disable list (additive)
# extend-disable = ["MD041"]

# List of file/directory patterns to include for linting (if provided, only these will be linted)
# include = [
#    "docs/*.md",
#    "src/**/*.md",
#    "README.md"
# ]

# List of file/directory patterns to exclude from linting
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",

    # Specific files or patterns
    "CHANGELOG.md",
    "LICENSE.md",
]

# Respect .gitignore files when scanning directories (default: true)
respect-gitignore = true

# Markdown flavor/dialect (uncomment to enable)
# Options: standard (default), gfm, commonmark, mkdocs, mdx, quarto
# flavor = "mkdocs"

# Rule-specific configurations (uncomment and modify as needed)

# [MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [MD007]
# indent = 4  # Unordered list indentation

# [MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#;

    // Write the default configuration to the file
    match fs::write(path, default_config) {
        Ok(_) => Ok(()),
        Err(err) => Err(ConfigError::IoError {
            source: err,
            path: path.to_string(),
        }),
    }
}

/// Errors that can occur when loading configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file
    #[error("Failed to read config file at {path}: {source}")]
    IoError { source: io::Error, path: String },

    /// Failed to parse the configuration content (TOML or JSON)
    #[error("Failed to parse config: {0}")]
    ParseError(String),

    /// Configuration file already exists
    #[error("Configuration file already exists at {path}")]
    FileExists { path: String },

    /// Circular extends reference detected
    #[error("Circular extends reference: {path} already in chain {chain:?}")]
    CircularExtends { path: String, chain: Vec<String> },

    /// Extends chain exceeds maximum depth
    #[error("extends chain exceeds maximum depth of {max_depth} at {path}")]
    ExtendsDepthExceeded { path: String, max_depth: usize },

    /// Extends target file not found
    #[error("extends target not found: {path} (referenced from {from})")]
    ExtendsNotFound { path: String, from: String },
}

/// Get a rule-specific configuration value
/// Automatically tries both the original key and normalized variants (kebab-case ↔ snake_case)
/// for better markdownlint compatibility
pub fn get_rule_config_value<T: serde::de::DeserializeOwned>(config: &Config, rule_name: &str, key: &str) -> Option<T> {
    let norm_rule_name = rule_name.to_ascii_uppercase(); // Use uppercase for lookup

    let rule_config = config.rules.get(&norm_rule_name)?;

    // Try multiple key variants to support both underscore and kebab-case formats
    let key_variants = [
        key.to_string(),       // Original key as provided
        normalize_key(key),    // Normalized key (lowercase, kebab-case)
        key.replace('-', "_"), // Convert kebab-case to snake_case
        key.replace('_', "-"), // Convert snake_case to kebab-case
    ];

    // Try each variant until we find a match
    for variant in &key_variants {
        if let Some(value) = rule_config.values.get(variant)
            && let Ok(result) = T::deserialize(value.clone())
        {
            return Some(result);
        }
    }

    None
}

/// Generate default rumdl configuration for pyproject.toml
pub fn generate_pyproject_config() -> String {
    let config_content = r#"
[tool.rumdl]
# Global configuration options
line-length = 100
disable = []
# extend-enable = ["MD060"]  # Add opt-in rules (additive, keeps defaults)
# extend-disable = []  # Additional rules to disable (additive)
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
]
respect-gitignore = true

# Rule-specific configurations (uncomment and modify as needed)

# [tool.rumdl.MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [tool.rumdl.MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [tool.rumdl.MD007]
# indent = 4  # Unordered list indentation

# [tool.rumdl.MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [tool.rumdl.MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#;

    config_content.to_string()
}
