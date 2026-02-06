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
    #[serde(alias = "absolute_links")]
    pub absolute_links: AbsoluteLinksOption,
}

impl RuleConfig for MD057Config {
    const RULE_NAME: &'static str = "MD057";
}
