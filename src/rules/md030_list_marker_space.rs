//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rule::{LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::rules::list_utils::ListType;
use regex::Regex;
use lazy_static::lazy_static;
use toml;

lazy_static! {
    // Matches indentation, marker, and whitespace after marker
    static ref LIST_MARKER_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)([ \t]+)").unwrap();
}

#[derive(Clone)]
pub struct MD030ListMarkerSpace {
    ul_single: usize,
    ul_multi: usize,
    ol_single: usize,
    ol_multi: usize,
}

impl Default for MD030ListMarkerSpace {
    fn default() -> Self {
        Self {
            ul_single: 1,
            ul_multi: 1,
            ol_single: 1,
            ol_multi: 1,
        }
    }
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            ul_single,
            ul_multi,
            ol_single,
            ol_multi,
        }
    }

    pub fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.ul_single,
            (ListType::Unordered, true) => self.ul_multi,
            (ListType::Ordered, false) => self.ol_single,
            (ListType::Ordered, true) => self.ol_multi,
        }
    }
}

impl Rule for MD030ListMarkerSpace {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<String> = ctx.content.lines().map(|l| l.to_string()).collect();
        let mut in_fenced_code_block = false;
        let mut fenced_code_block_delim = "";
        let mut in_blockquote = false;
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim_start();
            // Detect fenced code blocks
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence = &trimmed[..3];
                if !in_fenced_code_block {
                    in_fenced_code_block = true;
                    fenced_code_block_delim = fence;
                } else if trimmed.starts_with(fenced_code_block_delim) {
                    in_fenced_code_block = false;
                    fenced_code_block_delim = "";
                }
            }
            if in_fenced_code_block {
                continue;
            }
            // Skip indented code blocks (4+ spaces or tab)
            if line.starts_with("    ") || line.starts_with("\t") {
                continue;
            }
            // Track blockquotes (for now, just skip lines starting with >)
            let mut l = line.as_str();
            while l.trim_start().starts_with('>') {
                l = l.trim_start().trim_start_matches('>').trim_start();
                in_blockquote = true;
            }
            if in_blockquote {
                in_blockquote = false;
                continue;
            }
            if let Some(cap) = LIST_MARKER_REGEX.captures(line) {
                let marker = cap.get(2).map_or("", |m| m.as_str());
                let whitespace = cap.get(3).map_or("", |m| m.as_str());
                let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    ListType::Ordered
                } else {
                    ListType::Unordered
                };
                let expected_spaces = self.get_expected_spaces(list_type, false); // single-line by default
                if whitespace.contains('\t') || whitespace.len() > expected_spaces {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        severity: Severity::Warning,
                        line: line_num,
                        column: cap.get(1).map_or(0, |m| m.as_str().len()) + marker.len() + 1,
                        message: "Spaces after list markers".to_string(),
                        fix: None,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
            || (!ctx.content.contains('*')
                && !ctx.content.contains('-')
                && !ctx.content.contains('+')
                && !ctx.content.contains(|c: char| c.is_ascii_digit()))
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
            "ul-single".to_string(),
            toml::Value::Integer(self.ul_single as i64),
        );
        map.insert(
            "ul-multi".to_string(),
            toml::Value::Integer(self.ul_multi as i64),
        );
        map.insert(
            "ol-single".to_string(),
            toml::Value::Integer(self.ol_single as i64),
        );
        map.insert(
            "ol-multi".to_string(),
            toml::Value::Integer(self.ol_multi as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        fn get_key(
            config: &crate::config::Config,
            rule: &str,
            dash: &str,
            underscore: &str,
        ) -> usize {
            crate::config::get_rule_config_value::<usize>(config, rule, dash)
                .or_else(|| crate::config::get_rule_config_value::<usize>(config, rule, underscore))
                .unwrap_or(1)
        }
        let ul_single = get_key(config, "MD030", "ul-single", "ul_single");
        let ul_multi = get_key(config, "MD030", "ul-multi", "ul_multi");
        let ol_single = get_key(config, "MD030", "ol-single", "ol_single");
        let ol_multi = get_key(config, "MD030", "ol-multi", "ol_multi");
        Box::new(MD030ListMarkerSpace::new(
            ul_single, ul_multi, ol_single, ol_multi,
        ))
    }

    fn fix(&self, _ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        Err(crate::rule::LintError::FixFailed("Automatic fixing is not supported for MD030. See todos/md030_fix_strategy.md for details.".to_string()))
    }
}

impl DocumentStructureExtensions for MD030ListMarkerSpace {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        // Expect warnings for lines with too many spaces after the marker
        assert_eq!(result.len(), 2, "Should flag lines with too many spaces after list marker");
        for warning in result {
            assert_eq!(warning.message, "Spaces after list markers");
        }
    }
}
