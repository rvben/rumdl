/// Rule MD024: No duplicate headings
///
/// See [docs/md024.md](../../docs/md024.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::LineIndex;
use toml;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct MD024NoDuplicateHeading {
    pub allow_different_nesting: bool,
    pub siblings_only: bool,
}

impl MD024NoDuplicateHeading {
    pub fn new(allow_different_nesting: bool, siblings_only: bool) -> Self {
        Self {
            allow_different_nesting,
            siblings_only,
        }
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
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let _seen_headings: HashSet<String> = HashSet::new();
        let mut seen_headings_per_level: HashMap<u32, HashSet<String>> = HashMap::new();

        for (line_num, line) in content.lines().enumerate() {
            if HeadingUtils::is_in_code_block(content, line_num + 1) {
                continue;
            }
            if let Some(heading) = HeadingUtils::parse_heading(content, line_num + 1) {
                let indentation = HeadingUtils::get_indentation(line);
                let text = heading.text.clone();
                let heading_key = text.trim();
                if heading_key.is_empty() {
                    continue; // Ignore empty headings
                }
                if self.siblings_only {
                    // Handle siblings_only logic here
                } else if self.allow_different_nesting {
                    let seen = seen_headings_per_level.entry(0).or_default();
                    if seen.contains(heading_key) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "Multiple headings with the same content".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index
                                    .line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!(
                                    "{}{} {} ({})",
                                    " ".repeat(indentation),
                                    "#".repeat(heading.level.try_into().unwrap()),
                                    heading.text,
                                    seen.iter().filter(|&h| h == heading_key).count() + 1
                                ),
                            }),
                        });
                    } else {
                        seen.insert(heading_key.to_string());
                    }
                } else {
                    let seen = seen_headings_per_level.entry(heading.level).or_default();
                    if seen.contains(heading_key) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "Multiple headings with the same content at the same level"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index
                                    .line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!(
                                    "{}{} {} ({})",
                                    " ".repeat(indentation),
                                    "#".repeat(heading.level.try_into().unwrap()),
                                    heading.text,
                                    seen.iter().filter(|&h| h == heading_key).count() + 1
                                ),
                            }),
                        });
                    } else {
                        seen.insert(heading_key.to_string());
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // For default config, fix is a no-op
        if !self.allow_different_nesting && !self.siblings_only {
            return Ok(content.to_string());
        }
        let _line_index = LineIndex::new(content.to_string());
        let mut result = String::new();
        let _seen_headings: HashSet<String> = HashSet::new();
        let mut seen_headings_per_level: HashMap<u32, HashSet<String>> = HashMap::new();
        for (line_num, line) in content.lines().enumerate() {
            if HeadingUtils::is_in_code_block(content, line_num + 1) {
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if let Some(heading) = HeadingUtils::parse_heading(content, line_num + 1) {
                let indentation = HeadingUtils::get_indentation(line);
                let text = heading.text.clone();
                let heading_key = text.trim();
                if heading_key.is_empty() {
                    result.push_str(line);
                    result.push('\n');
                    continue;
                }
                if self.siblings_only {
                    // Handle siblings_only logic here
                } else if self.allow_different_nesting {
                    let seen = seen_headings_per_level.entry(0).or_default();
                    if seen.contains(heading_key) {
                        result.push_str(&format!(
                            "{}{} {} ({})\n",
                            " ".repeat(indentation),
                            "#".repeat(heading.level.try_into().unwrap()),
                            heading.text,
                            seen.iter().filter(|&h| h == heading_key).count() + 1
                        ));
                    } else {
                        seen.insert(heading_key.to_string());
                        result.push_str(line);
                        result.push('\n');
                    }
                } else {
                    let seen = seen_headings_per_level.entry(heading.level).or_default();
                    if seen.contains(heading_key) {
                        result.push_str(&format!(
                            "{}{} {} ({})\n",
                            " ".repeat(indentation),
                            "#".repeat(heading.level.try_into().unwrap()),
                            heading.text,
                            seen.iter().filter(|&h| h == heading_key).count() + 1
                        ));
                    } else {
                        seen.insert(heading_key.to_string());
                        result.push_str(line);
                        result.push('\n');
                    }
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "allow_different_nesting".to_string(),
            toml::Value::Boolean(self.allow_different_nesting),
        );
        map.insert(
            "siblings_only".to_string(),
            toml::Value::Boolean(self.siblings_only),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let allow_different_nesting = crate::config::get_rule_config_value::<bool>(config, "MD024", "allow_different_nesting").unwrap_or(false);
        let siblings_only = crate::config::get_rule_config_value::<bool>(config, "MD024", "siblings_only").unwrap_or(false);
        Box::new(MD024NoDuplicateHeading::new(allow_different_nesting, siblings_only))
    }
}
