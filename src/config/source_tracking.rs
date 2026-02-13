use crate::types::LineLength;
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::marker::PhantomData;

use super::flavor::{ConfigLoaded, MarkdownFlavor};

/// Configuration source with clear precedence hierarchy.
///
/// Precedence order (lower values override higher values):
/// - Default (0): Built-in defaults
/// - UserConfig (1): User-level ~/.config/rumdl/rumdl.toml
/// - PyprojectToml (2): Project-level pyproject.toml
/// - ProjectConfig (3): Project-level .rumdl.toml (most specific)
/// - Cli (4): Command-line flags (highest priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Built-in default configuration
    Default,
    /// User-level configuration from ~/.config/rumdl/rumdl.toml
    UserConfig,
    /// Project-level configuration from pyproject.toml
    PyprojectToml,
    /// Project-level configuration from .rumdl.toml or rumdl.toml
    ProjectConfig,
    /// Command-line flags (highest precedence)
    Cli,
}

#[derive(Debug, Clone)]
pub struct ConfigOverride<T> {
    pub value: T,
    pub source: ConfigSource,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SourcedValue<T> {
    pub value: T,
    pub source: ConfigSource,
    pub overrides: Vec<ConfigOverride<T>>,
}

impl<T: Clone> SourcedValue<T> {
    pub fn new(value: T, source: ConfigSource) -> Self {
        Self {
            value: value.clone(),
            source,
            overrides: vec![ConfigOverride {
                value,
                source,
                file: None,
                line: None,
            }],
        }
    }

    /// Merges a new override into this SourcedValue based on source precedence.
    /// If the new source has higher or equal precedence, the value and source are updated,
    /// and the new override is added to the history.
    pub fn merge_override(
        &mut self,
        new_value: T,
        new_source: ConfigSource,
        new_file: Option<String>,
        new_line: Option<usize>,
    ) {
        // Helper function to get precedence, defined locally or globally
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::UserConfig => 1,
                ConfigSource::PyprojectToml => 2,
                ConfigSource::ProjectConfig => 3,
                ConfigSource::Cli => 4,
            }
        }

        if source_precedence(new_source) >= source_precedence(self.source) {
            self.value = new_value.clone();
            self.source = new_source;
            self.overrides.push(ConfigOverride {
                value: new_value,
                source: new_source,
                file: new_file,
                line: new_line,
            });
        }
    }

    pub fn push_override(&mut self, value: T, source: ConfigSource, file: Option<String>, line: Option<usize>) {
        // This is essentially merge_override without the precedence check
        // We might consolidate these later, but keep separate for now during refactor
        self.value = value.clone();
        self.source = source;
        self.overrides.push(ConfigOverride {
            value,
            source,
            file,
            line,
        });
    }
}

impl<T: Clone + Eq + std::hash::Hash> SourcedValue<Vec<T>> {
    /// Merges a new value using union semantics (for arrays like `disable`)
    /// Values from both sources are combined, with deduplication
    pub fn merge_union(
        &mut self,
        new_value: Vec<T>,
        new_source: ConfigSource,
        new_file: Option<String>,
        new_line: Option<usize>,
    ) {
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::UserConfig => 1,
                ConfigSource::PyprojectToml => 2,
                ConfigSource::ProjectConfig => 3,
                ConfigSource::Cli => 4,
            }
        }

        if source_precedence(new_source) >= source_precedence(self.source) {
            // Union: combine values from both sources with deduplication
            let mut combined = self.value.clone();
            for item in new_value.iter() {
                if !combined.contains(item) {
                    combined.push(item.clone());
                }
            }

            self.value = combined;
            self.source = new_source;
            self.overrides.push(ConfigOverride {
                value: new_value,
                source: new_source,
                file: new_file,
                line: new_line,
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourcedGlobalConfig {
    pub enable: SourcedValue<Vec<String>>,
    pub disable: SourcedValue<Vec<String>>,
    pub exclude: SourcedValue<Vec<String>>,
    pub include: SourcedValue<Vec<String>>,
    pub respect_gitignore: SourcedValue<bool>,
    pub line_length: SourcedValue<LineLength>,
    pub output_format: Option<SourcedValue<String>>,
    pub fixable: SourcedValue<Vec<String>>,
    pub unfixable: SourcedValue<Vec<String>>,
    pub flavor: SourcedValue<MarkdownFlavor>,
    pub force_exclude: SourcedValue<bool>,
    pub cache_dir: Option<SourcedValue<String>>,
    pub cache: SourcedValue<bool>,
    pub extend_enable: SourcedValue<Vec<String>>,
    pub extend_disable: SourcedValue<Vec<String>>,
}

impl Default for SourcedGlobalConfig {
    fn default() -> Self {
        SourcedGlobalConfig {
            enable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            disable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            exclude: SourcedValue::new(Vec::new(), ConfigSource::Default),
            include: SourcedValue::new(Vec::new(), ConfigSource::Default),
            respect_gitignore: SourcedValue::new(true, ConfigSource::Default),
            line_length: SourcedValue::new(LineLength::default(), ConfigSource::Default),
            output_format: None,
            fixable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            unfixable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            flavor: SourcedValue::new(MarkdownFlavor::default(), ConfigSource::Default),
            force_exclude: SourcedValue::new(false, ConfigSource::Default),
            cache_dir: None,
            cache: SourcedValue::new(true, ConfigSource::Default),
            extend_enable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            extend_disable: SourcedValue::new(Vec::new(), ConfigSource::Default),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SourcedRuleConfig {
    pub severity: Option<SourcedValue<crate::rule::Severity>>,
    pub values: BTreeMap<String, SourcedValue<toml::Value>>,
}

/// Represents configuration loaded from a single source file, with provenance.
/// Used as an intermediate step before merging into the final SourcedConfig.
#[derive(Debug, Clone)]
pub struct SourcedConfigFragment {
    /// Path to a base config file to inherit from (consumed during loading, not a config setting)
    pub extends: Option<String>,
    pub global: SourcedGlobalConfig,
    pub per_file_ignores: SourcedValue<HashMap<String, Vec<String>>>,
    pub per_file_flavor: SourcedValue<IndexMap<String, MarkdownFlavor>>,
    pub code_block_tools: SourcedValue<crate::code_block_tools::CodeBlockToolsConfig>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub unknown_keys: Vec<(String, String, Option<String>)>, // (section, key, file_path)
                                                             // Note: loaded_files is tracked globally in SourcedConfig.
}

impl Default for SourcedConfigFragment {
    fn default() -> Self {
        Self {
            extends: None,
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(HashMap::new(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(IndexMap::new(), ConfigSource::Default),
            code_block_tools: SourcedValue::new(
                crate::code_block_tools::CodeBlockToolsConfig::default(),
                ConfigSource::Default,
            ),
            rules: BTreeMap::new(),
            unknown_keys: Vec::new(),
        }
    }
}

/// Represents a config validation warning or error
#[derive(Debug, Clone)]
pub struct ConfigValidationWarning {
    pub message: String,
    pub rule: Option<String>,
    pub key: Option<String>,
}

/// Configuration with provenance tracking for values.
///
/// The `State` type parameter encodes the validation state:
/// - `ConfigLoaded`: Config has been loaded but not validated
/// - `ConfigValidated`: Config has been validated and can be converted to `Config`
///
/// # Typestate Pattern
///
/// This uses the typestate pattern to ensure validation happens before conversion:
///
/// ```ignore
/// let loaded: SourcedConfig<ConfigLoaded> = SourcedConfig::load_with_discovery(...)?;
/// let validated: SourcedConfig<ConfigValidated> = loaded.validate(&registry)?;
/// let config: Config = validated.into();  // Only works on ConfigValidated!
/// ```
///
/// Attempting to convert a `ConfigLoaded` config directly to `Config` is a compile error.
#[derive(Debug, Clone)]
pub struct SourcedConfig<State = ConfigLoaded> {
    pub global: SourcedGlobalConfig,
    pub per_file_ignores: SourcedValue<HashMap<String, Vec<String>>>,
    pub per_file_flavor: SourcedValue<IndexMap<String, MarkdownFlavor>>,
    pub code_block_tools: SourcedValue<crate::code_block_tools::CodeBlockToolsConfig>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub loaded_files: Vec<String>,
    pub unknown_keys: Vec<(String, String, Option<String>)>, // (section, key, file_path)
    /// Project root directory (parent of config file), used for resolving relative paths
    pub project_root: Option<std::path::PathBuf>,
    /// Validation warnings (populated after validate() is called)
    pub validation_warnings: Vec<ConfigValidationWarning>,
    /// Phantom data for the state type parameter
    pub(super) _state: PhantomData<State>,
}

impl Default for SourcedConfig<ConfigLoaded> {
    fn default() -> Self {
        Self {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(HashMap::new(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(IndexMap::new(), ConfigSource::Default),
            code_block_tools: SourcedValue::new(
                crate::code_block_tools::CodeBlockToolsConfig::default(),
                ConfigSource::Default,
            ),
            rules: BTreeMap::new(),
            loaded_files: Vec::new(),
            unknown_keys: Vec::new(),
            project_root: None,
            validation_warnings: Vec::new(),
            _state: PhantomData,
        }
    }
}
