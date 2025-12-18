use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD057 (relative link validation)
///
/// This rule validates that relative links point to existing files.
/// No configuration options are currently available.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MD057Config {}

impl RuleConfig for MD057Config {
    const RULE_NAME: &'static str = "MD057";
}
