use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MD057Config {}

impl RuleConfig for MD057Config {
    const RULE_NAME: &'static str = "MD057";
}
