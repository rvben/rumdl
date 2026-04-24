use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// How to handle absolute links (paths starting with /)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbsoluteLinksOption {
    /// Ignore absolute links (don't validate them) - this is the default
    #[default]
    Ignore,
    /// Warn about absolute links (they can't be validated as local paths)
    Warn,
    /// Resolve absolute links relative to MkDocs docs_dir and validate
    RelativeToDocs,
    /// Resolve absolute links relative to one or more explicit root directories.
    /// First match wins; reports broken only when all roots miss.
    RelativeToRoots,
}

/// Configuration for MD057 (relative link validation)
///
/// This rule validates that relative links point to existing files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct MD057Config {
    /// How to handle absolute links (paths starting with /)
    /// - "ignore" (default): Skip validation for absolute links
    /// - "warn": Report a warning for absolute links
    /// - "relative_to_docs": Resolve relative to MkDocs docs_dir and validate
    /// - "relative_to_roots": Resolve relative to one or more configured root directories
    #[serde(alias = "absolute_links")]
    pub absolute_links: AbsoluteLinksOption,

    /// Warn when relative links contain unnecessary path traversal.
    /// When enabled, `../sub_dir/file.md` from within `sub_dir/` warns
    /// and suggests the shorter equivalent `file.md`.
    #[serde(alias = "compact_paths")]
    pub compact_paths: bool,

    /// Additional directories to search when a relative link is not found
    /// relative to the file's directory.
    ///
    /// Paths are resolved relative to the project root (where `.rumdl.toml` or
    /// `pyproject.toml` is found), or relative to the current working directory.
    ///
    /// For Obsidian users: the attachment folder is auto-detected from
    /// `.obsidian/app.json` when `flavor = "obsidian"` is set, so this option
    /// is typically not needed. Use it for custom setups or non-Obsidian tools.
    ///
    /// Example:
    /// ```toml
    /// [MD057]
    /// search-paths = ["assets", "images", "attachments"]
    /// ```
    #[serde(alias = "search_paths")]
    pub search_paths: Vec<String>,

    /// Root directories used when `absolute-links = "relative_to_roots"`.
    ///
    /// Absolute links are resolved against each root in order; the first root
    /// where the target file exists passes the check. A warning is emitted only
    /// when none of the roots contain the target.
    ///
    /// Paths are resolved relative to the current working directory when not
    /// absolute. Trailing slashes are normalized automatically.
    ///
    /// When `roots` is empty and `absolute-links = "relative_to_roots"`, the
    /// rule emits a "not validated" warning for every absolute link, consistent
    /// with the fallback behavior of `relative_to_docs` when no `mkdocs.yml` is
    /// found.
    ///
    /// Example:
    /// ```toml
    /// [MD057]
    /// absolute-links = "relative_to_roots"
    /// roots = ["content/en", "content/zh-cn"]
    /// ```
    pub roots: Vec<String>,
}

impl RuleConfig for MD057Config {
    const RULE_NAME: &'static str = "MD057";
}
