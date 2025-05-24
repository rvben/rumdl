use toml;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
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

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut warnings = Vec::new();
        let mut seen_headings: HashSet<String> = HashSet::new();
        let mut seen_headings_per_level: HashMap<u32, HashSet<String>> = HashMap::new();

        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Skip headings in front matter
            if structure.is_in_front_matter(line_num) {
                continue;
            }
            let level = *structure.heading_levels.get(i).unwrap_or(&1) as u32;
            let region = structure
                .heading_regions
                .get(i)
                .copied()
                .unwrap_or((line_num, line_num));
            let line_idx = region.0 - 1; // 0-based
            let line = lines.get(line_idx).unwrap_or(&"");
            let indentation = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            let text = line
                .trim()
                .trim_start_matches('#')
                .trim()
                .trim_end_matches('#')
                .trim();
            if text.is_empty() {
                continue; // Ignore empty headings
            }
            let heading_key = text.to_string();
            if self.siblings_only {
                // TODO: Implement siblings_only logic if needed
            } else if self.allow_different_nesting {
                // Only flag duplicates at the same level
                let seen = seen_headings_per_level.entry(level).or_default();
                if seen.contains(&heading_key) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!("Duplicate heading: '{}'.", text),
                        line: line_num,
                        column: indentation.len() + 1,
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
                        message: format!("Duplicate heading: '{}'.", text),
                        line: line_num,
                        column: indentation.len() + 1,
                        severity: Severity::Warning,
                        fix: None,
                    });
                } else {
                    seen_headings.insert(heading_key.clone());
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD024 does not support auto-fixing. Removing duplicate headings is not a safe or meaningful fix.
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
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
        let allow_different_nesting = crate::config::get_rule_config_value::<bool>(
            config,
            "MD024",
            "allow_different_nesting",
        )
        .unwrap_or(false);
        let siblings_only =
            crate::config::get_rule_config_value::<bool>(config, "MD024", "siblings_only")
                .unwrap_or(false);
        Box::new(MD024NoDuplicateHeading::new(
            allow_different_nesting,
            siblings_only,
        ))
    }
}

impl DocumentStructureExtensions for MD024NoDuplicateHeading {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}
