use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD018 (No missing space after hash in heading)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD018Config {
    /// Enable MagicLink support for issue/PR references like #123, #10
    /// When true, numeric patterns like `#10` at the start of a line are
    /// not flagged as malformed headings, allowing PyMdown MagicLink syntax.
    /// Default: false (all patterns are flagged)
    #[serde(default)]
    pub magiclink: bool,
}

impl RuleConfig for MD018Config {
    const RULE_NAME: &'static str = "MD018";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_magiclink_disabled() {
        let config = MD018Config::default();
        assert!(!config.magiclink, "magiclink should default to false");
    }

    #[test]
    fn test_magiclink_enabled() {
        let toml_str = r#"
            magiclink = true
        "#;
        let config: MD018Config = toml::from_str(toml_str).unwrap();
        assert!(config.magiclink);
    }

    #[test]
    fn test_magiclink_disabled_explicit() {
        let toml_str = r#"
            magiclink = false
        "#;
        let config: MD018Config = toml::from_str(toml_str).unwrap();
        assert!(!config.magiclink);
    }

    #[test]
    fn test_empty_config() {
        let toml_str = "";
        let config: MD018Config = toml::from_str(toml_str).unwrap();
        assert!(!config.magiclink);
    }

    #[test]
    fn test_from_config_loads_magiclink() {
        use crate::config::Config;
        use crate::rule::Rule;

        // Create a Config from TOML with MD018.magiclink = true
        let toml_str = r#"
            [MD018]
            magiclink = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();

        // Use from_config to create the rule
        let rule = super::super::MD018NoMissingSpaceAtx::from_config(&config);

        // Verify MagicLink patterns are skipped
        let content = "#10 is an issue ref\n#Summary";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With magiclink enabled, should only flag #Summary (not #10)
        assert_eq!(result.len(), 1, "Should only flag #Summary, not #10");
        assert!(
            result[0].message.contains("Summary") || result[0].line == 2,
            "Should flag line 2 (#Summary)"
        );
    }
}
