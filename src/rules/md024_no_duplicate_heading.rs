use toml;

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_match_range;
use crate::rule_config_serde::RuleConfig;
use std::collections::{HashMap, HashSet};

mod md024_config;
use md024_config::MD024Config;

#[derive(Clone, Debug)]
pub struct MD024NoDuplicateHeading {
    config: MD024Config,
}

impl Default for MD024NoDuplicateHeading {
    fn default() -> Self {
        Self {
            config: MD024Config::default(),
        }
    }
}

impl MD024NoDuplicateHeading {
    pub fn new(allow_different_nesting: bool, siblings_only: bool) -> Self {
        Self {
            config: MD024Config {
                allow_different_nesting,
                siblings_only,
            },
        }
    }
    
    pub fn from_config_struct(config: MD024Config) -> Self {
        Self { config }
    }
}

impl Rule for MD024NoDuplicateHeading {
    fn name(&self) -> &'static str {
        "MD024"
    }

    fn description(&self) -> &'static str {
        "Multiple headings with the same content"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut seen_headings: HashSet<String> = HashSet::new();
        let mut seen_headings_per_level: HashMap<u8, HashSet<String>> = HashMap::new();

        // Process headings using cached heading information
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip empty headings
                if heading.text.is_empty() {
                    continue;
                }

                let heading_key = heading.text.clone();
                let level = heading.level;

                // Calculate precise character range for the heading text content
                let text_start_in_line = if let Some(pos) = line_info.content.find(&heading.text) {
                    pos
                } else {
                    // Fallback: find after hash markers
                    let trimmed = line_info.content.trim_start();
                    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                    let after_hashes = &trimmed[hash_count..];
                    let text_start_in_trimmed = after_hashes.find(&heading.text).unwrap_or(0);
                    (line_info.content.len() - trimmed.len()) + hash_count + text_start_in_trimmed
                };

                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, &line_info.content, text_start_in_line, heading.text.len());

                if self.config.siblings_only {
                    // TODO: Implement siblings_only logic if needed
                } else if self.config.allow_different_nesting {
                    // Only flag duplicates at the same level
                    let seen = seen_headings_per_level.entry(level).or_default();
                    if seen.contains(&heading_key) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!("Duplicate heading: '{}'.", heading.text),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: None,
                        });
                    } else {
                        seen.insert(heading_key.clone());
                    }
                } else {
                    // Flag all duplicates, regardless of level
                    if seen_headings.contains(&heading_key) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!("Duplicate heading: '{}'.", heading.text),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: None,
                        });
                    } else {
                        seen_headings.insert(heading_key.clone());
                    }
                }
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD024 does not support auto-fixing. Removing duplicate headings is not a safe or meaningful fix.
        Ok(ctx.content.to_string())
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.lines.iter().all(|line| line.heading.is_none())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD024Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD024Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD024Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}
