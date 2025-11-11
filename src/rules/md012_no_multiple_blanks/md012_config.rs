use crate::rule_config_serde::RuleConfig;
use crate::types::PositiveUsize;
use serde::{Deserialize, Serialize};

/// Configuration for MD012 (No multiple consecutive blank lines)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD012Config {
    /// Maximum number of consecutive blank lines allowed within the document (default: 1)
    ///
    /// This setting controls blank lines within the document content.
    /// Blank lines at EOF are always enforced to be 0 (following POSIX/Prettier standards).
    #[serde(default = "default_maximum")]
    pub maximum: PositiveUsize,
}

fn default_maximum() -> PositiveUsize {
    PositiveUsize::from_const(1)
}

impl Default for MD012Config {
    fn default() -> Self {
        Self {
            maximum: default_maximum(),
        }
    }
}

impl RuleConfig for MD012Config {
    const RULE_NAME: &'static str = "MD012";
}
