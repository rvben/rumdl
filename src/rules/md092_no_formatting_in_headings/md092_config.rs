//! Configuration for MD092 (no formatting in headings).

use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// Configuration for MD092. One switch per construct, because the three have
/// different justifications: a code span in a heading is usually deliberate and
/// only breaks generated artifacts, while emphasis is often an accident of a
/// name that contains `_` or `*`. A project that wants only one of them
/// reported turns the other two off.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD092Config {
    /// Report inline code spans in headings.
    #[serde(default = "default_true")]
    pub(super) code: bool,
    /// Report strong emphasis (`**`/`__`) in headings.
    #[serde(default = "default_true")]
    pub(super) strong: bool,
    /// Report ordinary emphasis (`*`/`_`) in headings.
    #[serde(default = "default_true")]
    pub(super) emphasis: bool,
}

impl Default for MD092Config {
    fn default() -> Self {
        Self {
            code: true,
            strong: true,
            emphasis: true,
        }
    }
}

impl RuleConfig for MD092Config {
    const RULE_NAME: &'static str = "MD092";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_construct_is_reported_by_default() {
        let config = MD092Config::default();
        assert!(config.code);
        assert!(config.strong);
        assert!(config.emphasis);
    }

    #[test]
    fn a_single_construct_can_be_turned_off() {
        let config: MD092Config = toml::from_str("code = false\n").unwrap();
        assert!(!config.code);
        assert!(config.strong);
        assert!(config.emphasis);
    }
}
