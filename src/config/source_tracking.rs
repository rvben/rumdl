use crate::types::LineLength;
use indexmap::IndexMap;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::marker::PhantomData;

use super::flavor::{ConfigLoaded, MarkdownFlavor};

/// Configuration source with clear precedence hierarchy.
///
/// Precedence order (higher values override lower values):
/// - Default (0): Built-in defaults
/// - EditorConfig (1): A `.editorconfig` file, when `editorconfig = true`
/// - UserConfig (2): User-level ~/.config/rumdl/rumdl.toml
/// - PyprojectToml (3): Project-level pyproject.toml
/// - ProjectConfig (4): Project-level .rumdl.toml (most specific)
/// - Cli (5): Command-line flags (highest priority)
///
/// `.editorconfig` sits directly above the built-in defaults: it fills in
/// settings no rumdl config mentions, and anything written in a rumdl config
/// wins over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Built-in default configuration
    Default,
    /// A `.editorconfig` file applying to the linted file
    EditorConfig,
    /// User-level configuration from ~/.config/rumdl/rumdl.toml
    UserConfig,
    /// Project-level configuration from pyproject.toml
    PyprojectToml,
    /// Project-level configuration from .rumdl.toml or rumdl.toml
    ProjectConfig,
    /// Command-line flags (highest precedence)
    Cli,
}

fn source_precedence(src: ConfigSource) -> u8 {
    match src {
        ConfigSource::Default => 0,
        ConfigSource::EditorConfig => 1,
        ConfigSource::UserConfig => 2,
        ConfigSource::PyprojectToml => 3,
        ConfigSource::ProjectConfig => 4,
        ConfigSource::Cli => 5,
    }
}

/// A config value with its provenance: which kind of source set it, and for
/// file-based sources, which file. The origin makes `rumdl config` output
/// file-precise, which matters for `extends` chains where the source kind
/// alone cannot distinguish the base config from the extending one.
#[derive(Debug, Clone)]
pub struct SourcedValue<T> {
    pub value: T,
    pub source: ConfigSource,
    /// Path of the config file that supplied the winning value. `None` for
    /// defaults and CLI flags.
    pub origin: Option<String>,
}

impl<T: Clone> SourcedValue<T> {
    pub fn new(value: T, source: ConfigSource) -> Self {
        Self {
            value,
            source,
            origin: None,
        }
    }

    /// Merges a new value into this SourcedValue based on source precedence.
    /// If the new source has higher or equal precedence, the value, source,
    /// and origin are replaced.
    ///
    /// Returns whether the incoming value won, which is what a caller tracking
    /// something about the value alongside it needs in order to follow it.
    pub fn merge_override(&mut self, new_value: T, new_source: ConfigSource, new_origin: Option<String>) -> bool {
        if source_precedence(new_source) >= source_precedence(self.source) {
            self.value = new_value;
            self.source = new_source;
            self.origin = new_origin;
            true
        } else {
            false
        }
    }

    /// Merge another SourcedValue with replace semantics. See
    /// [`Self::merge_override`] for the return value.
    pub fn merge_from(&mut self, other: SourcedValue<T>) -> bool {
        self.merge_override(other.value, other.source, other.origin)
    }

    /// Sets the value unconditionally (no precedence check). Used while
    /// parsing a single file, where later keys legitimately replace earlier
    /// ones regardless of source.
    pub fn push_override(&mut self, value: T, source: ConfigSource, origin: Option<String>) {
        self.value = value;
        self.source = source;
        self.origin = origin;
    }
}

impl<T: Clone + Eq + std::hash::Hash> SourcedValue<Vec<T>> {
    /// Merges a new value using union semantics (for arrays like `extend-disable`):
    /// values from both sources are combined, with deduplication. The origin
    /// reflects the most recent contributor.
    pub fn merge_union(&mut self, new_value: Vec<T>, new_source: ConfigSource, new_origin: Option<String>) {
        if source_precedence(new_source) >= source_precedence(self.source) {
            for item in new_value {
                if !self.value.contains(&item) {
                    self.value.push(item);
                }
            }
            self.source = new_source;
            self.origin = new_origin;
        }
    }

    /// Merge another SourcedValue with union semantics.
    pub fn merge_union_from(&mut self, other: SourcedValue<Vec<T>>) {
        self.merge_union(other.value, other.source, other.origin);
    }
}

#[derive(Debug, Clone)]
pub struct SourcedGlobalConfig {
    pub enable: SourcedValue<Vec<String>>,
    pub disable: SourcedValue<Vec<String>>,
    pub exclude: SourcedValue<Vec<String>>,
    pub include: SourcedValue<Vec<String>>,
    /// How to name the file that supplied [`Self::include`] when its contents may
    /// not be quoted back (an `extends` target), and `None` when they may. The
    /// patterns apply as written; only the walk's message about one it cannot use
    /// has to leave it out. Already display-ready, as
    /// [`SourcedConfigFragment::unknown_keys`] describes.
    pub include_withheld: Option<String>,
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
    pub editorconfig: SourcedValue<bool>,
}

impl Default for SourcedGlobalConfig {
    fn default() -> Self {
        SourcedGlobalConfig {
            enable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            disable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            exclude: SourcedValue::new(Vec::new(), ConfigSource::Default),
            include: SourcedValue::new(Vec::new(), ConfigSource::Default),
            include_withheld: None,
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
            editorconfig: SourcedValue::new(false, ConfigSource::Default),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SourcedRuleConfig {
    pub severity: Option<SourcedValue<crate::rule::Severity>>,
    pub values: BTreeMap<String, SourcedValue<toml::Value>>,
    /// Keys in [`Self::values`] whose winning value came from a config file
    /// whose contents may not be quoted back (an `extends` target). The value
    /// applies as written; only a message about it has to leave it out.
    ///
    /// Tracked per key because a config that names the value itself takes the
    /// key over, and its own value is quotable again.
    pub withheld_keys: BTreeSet<String>,
}

/// Represents configuration loaded from a single source file, with provenance.
/// Used as an intermediate step before merging into the final SourcedConfig.
#[derive(Debug, Clone)]
pub struct SourcedConfigFragment {
    /// Path to a base config file to inherit from (consumed during loading, not a config setting)
    pub extends: Option<String>,
    pub global: SourcedGlobalConfig,
    pub per_file_ignores: SourcedValue<BTreeMap<String, Vec<String>>>,
    pub per_file_flavor: SourcedValue<IndexMap<String, MarkdownFlavor>>,
    pub code_block_tools: SourcedValue<crate::code_block_tools::CodeBlockToolsConfig>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    /// Maps canonical rule IDs to their preferred display names (used by import).
    /// When importing from markdownlint configs, this preserves the user's original
    /// naming preference (e.g., "line-length" instead of "MD013").
    pub rule_display_names: HashMap<String, String>,
    /// `(section, key, display_name)`. The third element names the file for a
    /// warning message and is already display-ready: relative to the working
    /// directory for a config the user named, and for one reached through
    /// `extends` the reference as written rather than the path it resolved to.
    /// `None` for keys that came from the command line and belong to no file.
    pub unknown_keys: Vec<(String, String, Option<String>)>,
    /// Problems found while parsing this file that the code acting on the value
    /// can no longer report itself, because the value was withheld before it got
    /// there. Merged into [`SourcedConfig::discovery_warnings`], which is the
    /// channel the withheld consumer's own message went to.
    pub load_warnings: Vec<String>,
    // Note: loaded_files is tracked globally in SourcedConfig.
}

impl Default for SourcedConfigFragment {
    fn default() -> Self {
        Self {
            extends: None,
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(BTreeMap::new(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(IndexMap::new(), ConfigSource::Default),
            code_block_tools: SourcedValue::new(
                crate::code_block_tools::CodeBlockToolsConfig::default(),
                ConfigSource::Default,
            ),
            rules: BTreeMap::new(),
            rule_display_names: HashMap::new(),
            unknown_keys: Vec::new(),
            load_warnings: Vec::new(),
        }
    }
}

impl SourcedGlobalConfig {
    /// Whether every global setting is still at its default.
    ///
    /// Written as an exhaustive destructuring so that adding a field to the
    /// struct fails to compile here until it is accounted for: a field this
    /// missed would make a config carrying only that setting look empty.
    pub fn is_empty(&self) -> bool {
        let Self {
            enable,
            disable,
            exclude,
            include,
            include_withheld: _,
            respect_gitignore,
            line_length,
            output_format,
            fixable,
            unfixable,
            flavor,
            force_exclude,
            cache_dir,
            cache,
            extend_enable,
            extend_disable,
            editorconfig,
        } = self;
        // `include_withheld` describes `include` rather than being a setting of
        // its own, so it is covered by the `include` check.
        output_format.is_none()
            && cache_dir.is_none()
            && [
                enable.source,
                disable.source,
                exclude.source,
                include.source,
                fixable.source,
                unfixable.source,
                extend_enable.source,
                extend_disable.source,
            ]
            .iter()
            .all(|source| *source == ConfigSource::Default)
            && respect_gitignore.source == ConfigSource::Default
            && line_length.source == ConfigSource::Default
            && flavor.source == ConfigSource::Default
            && force_exclude.source == ConfigSource::Default
            && cache.source == ConfigSource::Default
            && editorconfig.source == ConfigSource::Default
    }
}

impl SourcedConfigFragment {
    /// Whether this fragment says nothing at all, so the file it came from need
    /// not be loaded or reported.
    ///
    /// Written as an exhaustive destructuring for the reason
    /// [`SourcedGlobalConfig::is_empty`] gives: a configuration section that
    /// this check forgets is one a config file can consist entirely of and be
    /// discarded, warnings included.
    pub fn is_empty(&self) -> bool {
        let Self {
            extends,
            global,
            per_file_ignores,
            per_file_flavor,
            code_block_tools,
            rules,
            rule_display_names,
            unknown_keys,
            load_warnings,
        } = self;
        extends.is_none()
            && global.is_empty()
            && per_file_ignores.source == ConfigSource::Default
            && per_file_flavor.source == ConfigSource::Default
            && code_block_tools.source == ConfigSource::Default
            && rules.is_empty()
            && rule_display_names.is_empty()
            && unknown_keys.is_empty()
            && load_warnings.is_empty()
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
    pub per_file_ignores: SourcedValue<BTreeMap<String, Vec<String>>>,
    pub per_file_flavor: SourcedValue<IndexMap<String, MarkdownFlavor>>,
    pub code_block_tools: SourcedValue<crate::code_block_tools::CodeBlockToolsConfig>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    /// Every config file that contributed, by resolved path, in load order.
    ///
    /// A file reached through `extends` appears here as the path its reference
    /// expanded to, unlike every message about such a file (see `ConfigOrigin`).
    /// This list is not a message: it exists to
    /// answer which files took effect, which `rumdl config` is asked directly and
    /// a language server answers for the editor that started it, and the resolved
    /// path is the answer.
    pub loaded_files: Vec<String>,
    /// `(section, key, display_name)`, as on [`SourcedConfigFragment`].
    pub unknown_keys: Vec<(String, String, Option<String>)>,
    /// Project root directory (parent of config file), used for resolving relative paths
    pub project_root: Option<std::path::PathBuf>,
    /// Warnings produced while finding and reading config files: a `rumdl.toml`
    /// shadowed by a sibling `.rumdl.toml`, or a setting a file could not
    /// contribute (see [`SourcedConfigFragment::load_warnings`]). Shadowing is
    /// reported by auto-discovery only, so an explicit `--config` path and
    /// `--no-config`/`--isolated` see only the reading half.
    pub discovery_warnings: Vec<String>,
    /// Validation warnings (populated after validate() is called)
    pub validation_warnings: Vec<ConfigValidationWarning>,
    /// Phantom data for the state type parameter
    pub(super) _state: PhantomData<State>,
}

impl Default for SourcedConfig<ConfigLoaded> {
    fn default() -> Self {
        Self {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(BTreeMap::new(), ConfigSource::Default),
            per_file_flavor: SourcedValue::new(IndexMap::new(), ConfigSource::Default),
            code_block_tools: SourcedValue::new(
                crate::code_block_tools::CodeBlockToolsConfig::default(),
                ConfigSource::Default,
            ),
            rules: BTreeMap::new(),
            loaded_files: Vec::new(),
            unknown_keys: Vec::new(),
            project_root: None,
            discovery_warnings: Vec::new(),
            validation_warnings: Vec::new(),
            _state: PhantomData,
        }
    }
}
