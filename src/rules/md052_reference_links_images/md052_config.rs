use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD052 (reference-links-images)
///
/// This rule checks that reference links and images use references that are defined.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MD052Config {
    /// Whether to check shortcut reference syntax `[text]`.
    ///
    /// Default: false (matches markdownlint behavior)
    ///
    /// When false (default), only full (`[text][ref]`) and collapsed (`[text][]`)
    /// reference syntax is checked. Shortcut syntax `[text]` is ambiguous because
    /// it could be a shortcut reference link OR just text in brackets.
    ///
    /// When true, shortcut syntax is also checked, which may produce false positives
    /// for bracketed text that is not intended to be a reference link.
    #[serde(
        default,
        rename = "shortcut-syntax",
        alias = "shortcut_syntax",
        alias = "shortcutSyntax"
    )]
    pub shortcut_syntax: bool,

    /// Additional reference names to ignore when checking for undefined references.
    ///
    /// Default: [] (empty)
    ///
    /// Use this to specify project-specific type names, identifiers, or other
    /// bracketed text that should not be flagged as undefined references.
    ///
    /// Example:
    /// ```toml
    /// [MD052]
    /// ignore = ["Vec", "HashMap", "Option", "Result"]
    /// ```
    ///
    /// This performs case-insensitive matching (e.g., "Vec" matches `[vec]`, `[Vec]`, `[VEC]`).
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl RuleConfig for MD052Config {
    const RULE_NAME: &'static str = "MD052";
}
