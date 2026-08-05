use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// MD045 is diagnostic-only and has no configurable options.
///
/// The struct exists so the rule loads through the same serde path as every other
/// rule. It declares no fields: serde ignores keys a struct does not name, so a
/// config still carrying the long-removed `placeholder-text` deserializes fine and
/// gets the usual "Unknown option" warning naming the key that does nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MD045Config {}

impl RuleConfig for MD045Config {
    const RULE_NAME: &'static str = "MD045";
}
