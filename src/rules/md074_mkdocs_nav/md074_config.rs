use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Validation behavior for MkDocs nav entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavValidation {
    /// Report issues as warnings
    #[default]
    Warn,
    /// Ignore (don't report) issues
    Ignore,
}

/// Configuration for MD074 (MkDocs nav validation)
///
/// This rule validates that MkDocs nav entries point to existing files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct MD074Config {
    /// How to handle nav entries pointing to non-existent files
    /// - "warn" (default): Report a warning
    /// - "ignore": Skip validation
    #[serde(alias = "not_found")]
    pub not_found: NavValidation,

    /// How to handle files in docs_dir that aren't referenced in nav
    /// - "warn": Report a warning
    /// - "ignore" (default): Skip validation
    #[serde(alias = "omitted_files")]
    pub omitted_files: NavValidation,

    /// How to handle absolute links in nav entries
    /// - "warn": Report a warning
    /// - "ignore" (default): Skip validation
    #[serde(alias = "absolute_links")]
    pub absolute_links: NavValidation,
}

impl Default for MD074Config {
    fn default() -> Self {
        Self {
            not_found: NavValidation::Warn,
            omitted_files: NavValidation::Ignore,  // Off by default - can be noisy
            absolute_links: NavValidation::Ignore, // Off by default
        }
    }
}

impl RuleConfig for MD074Config {
    const RULE_NAME: &'static str = "MD074";
}
