use crate::config::Config;
use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD059 (Link text should be descriptive)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MD059Config {
    /// List of prohibited link text phrases (case-insensitive)
    #[serde(default = "default_prohibited_texts")]
    pub prohibited_texts: Vec<String>,
}

fn default_prohibited_texts() -> Vec<String> {
    vec![
        "click here".to_string(),
        "here".to_string(),
        "link".to_string(),
        "more".to_string(),
    ]
}

impl Default for MD059Config {
    fn default() -> Self {
        Self {
            prohibited_texts: default_prohibited_texts(),
        }
    }
}

impl RuleConfig for MD059Config {
    const RULE_NAME: &'static str = "MD059";
}

/// Rule MD059: Link text should be descriptive
///
/// See [docs/md059.md](../../docs/md059.md) for full documentation, configuration, and examples.
///
/// This rule enforces that markdown links use meaningful, descriptive text rather than generic
/// phrases. It triggers when link text matches prohibited terms like "click here," "here," "link,"
/// or "more."
///
/// ## Rationale
///
/// Descriptive link text is crucial for accessibility. Screen readers often present links without
/// surrounding context, making generic text problematic for users relying on assistive technologies.
///
/// ## Examples
///
/// ```markdown
/// <!-- Bad -->
/// [click here](docs.md)
/// [link](api.md)
/// [more](details.md)
///
/// <!-- Good -->
/// [API documentation](docs.md)
/// [Installation guide](install.md)
/// [Full details](details.md)
/// ```
///
/// ## Configuration
///
/// ```toml
/// [MD059]
/// prohibited_texts = ["click here", "here", "link", "more"]
/// ```
///
/// For non-English content, customize the prohibited texts:
///
/// ```toml
/// [MD059]
/// prohibited_texts = ["hier klicken", "hier", "link", "mehr"]
/// ```
#[derive(Clone)]
pub struct MD059LinkText {
    config: MD059Config,
    /// Cached lowercase versions of prohibited texts for performance
    prohibited_lowercase: Vec<String>,
}

impl MD059LinkText {
    pub fn new(prohibited_texts: Vec<String>) -> Self {
        let prohibited_lowercase = prohibited_texts.iter().map(|s| s.to_lowercase()).collect();

        Self {
            config: MD059Config { prohibited_texts },
            prohibited_lowercase,
        }
    }

    pub fn from_config_struct(config: MD059Config) -> Self {
        let prohibited_lowercase = config.prohibited_texts.iter().map(|s| s.to_lowercase()).collect();

        Self {
            config,
            prohibited_lowercase,
        }
    }

    /// Check if link text is prohibited, returning the matched prohibited text
    fn is_prohibited(&self, link_text: &str) -> Option<&str> {
        let normalized = link_text.trim().to_lowercase();

        self.prohibited_lowercase
            .iter()
            .zip(&self.config.prohibited_texts)
            .find(|(lower, _)| **lower == normalized)
            .map(|(_, original)| original.as_str())
    }
}

impl Default for MD059LinkText {
    fn default() -> Self {
        Self::from_config_struct(MD059Config::default())
    }
}

impl Rule for MD059LinkText {
    fn name(&self) -> &'static str {
        "MD059"
    }

    fn description(&self) -> &'static str {
        "Link text should be descriptive"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn fix_capability(&self) -> crate::rule::FixCapability {
        crate::rule::FixCapability::Unfixable
    }

    fn from_config(config: &Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD059Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for link in &ctx.links {
            // Skip empty link text
            if link.text.trim().is_empty() {
                continue;
            }

            // Check if link text is prohibited
            if self.is_prohibited(&link.text).is_some() {
                warnings.push(LintWarning {
                    line: link.line,
                    column: link.start_col + 2, // Point to first char of text (skip '[')
                    end_line: link.line,
                    end_column: link.end_col,
                    message: "Link text should be descriptive".to_string(),
                    severity: Severity::Warning,
                    fix: None, // Not auto-fixable - requires human judgment
                    rule_name: Some(self.name().to_string()),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // MD059 is not auto-fixable because choosing descriptive link text
        // requires human judgment and understanding of the link's context and destination
        Ok(ctx.content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn test_default_prohibited_texts() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new(
            "[click here](url)\n[here](url)\n[link](url)\n[more](url)",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 4);

        // All warnings should have the same descriptive message
        for warning in &warnings {
            assert_eq!(warning.message, "Link text should be descriptive");
        }
    }

    #[test]
    fn test_case_insensitive() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new(
            "[CLICK HERE](url)\n[Here](url)\n[LINK](url)",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn test_whitespace_trimming() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new("[  click here  ](url)\n[  here  ](url)", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_descriptive_text_allowed() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new(
            "[API documentation](url)\n[Installation guide](url)\n[Read the tutorial](url)",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_substring_not_matched() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new(
            "[click here for more info](url)\n[see here](url)\n[hyperlink](url)",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should not match when prohibited text is substring");
    }

    #[test]
    fn test_empty_text_skipped() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new("[](url)", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Empty link text should be skipped");
    }

    #[test]
    fn test_custom_prohibited_texts() {
        let rule = MD059LinkText::new(vec!["bad".to_string(), "poor".to_string()]);
        let ctx = LintContext::new("[bad](url)\n[poor](url)", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_reference_links() {
        let rule = MD059LinkText::default();
        let ctx = LintContext::new("[click here][ref]\n[ref]: url", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Should check reference links");
    }

    #[test]
    fn test_fix_not_supported() {
        let rule = MD059LinkText::default();
        let content = "[click here](url)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // MD059 is not auto-fixable, so fix() returns unchanged content
        let result = rule.fix(&ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_non_english() {
        let rule = MD059LinkText::new(vec!["hier klicken".to_string(), "hier".to_string(), "link".to_string()]);
        let ctx = LintContext::new("[hier klicken](url)\n[hier](url)", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }
}
