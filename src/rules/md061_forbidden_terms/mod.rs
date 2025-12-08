use crate::filtered_lines::FilteredLinesExt;
use regex::{Regex, RegexBuilder};

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;

mod md061_config;
pub use md061_config::MD061Config;

/// Rule MD061: Forbidden terms
///
/// See [docs/md061.md](../../docs/md061.md) for full documentation, configuration, and examples.

#[derive(Debug, Clone, Default)]
pub struct MD061ForbiddenTerms {
    config: MD061Config,
    pattern: Option<Regex>,
}

impl MD061ForbiddenTerms {
    pub fn new(terms: Vec<String>, case_sensitive: bool) -> Self {
        let config = MD061Config { terms, case_sensitive };
        let pattern = Self::build_pattern(&config);
        Self { config, pattern }
    }

    pub fn from_config_struct(config: MD061Config) -> Self {
        let pattern = Self::build_pattern(&config);
        Self { config, pattern }
    }

    fn build_pattern(config: &MD061Config) -> Option<Regex> {
        if config.terms.is_empty() {
            return None;
        }

        // Build alternation pattern from terms, escaping regex metacharacters
        let escaped_terms: Vec<String> = config.terms.iter().map(|term| regex::escape(term)).collect();
        let pattern_str = escaped_terms.join("|");

        RegexBuilder::new(&pattern_str)
            .case_insensitive(!config.case_sensitive)
            .build()
            .ok()
    }

    /// Check if match is at a word boundary
    fn is_word_boundary(content: &str, start: usize, end: usize) -> bool {
        let before_ok = if start == 0 {
            true
        } else {
            content[..start]
                .chars()
                .last()
                .map(|c| !c.is_alphanumeric() && c != '_')
                .unwrap_or(true)
        };

        let after_ok = if end >= content.len() {
            true
        } else {
            content[end..]
                .chars()
                .next()
                .map(|c| !c.is_alphanumeric() && c != '_')
                .unwrap_or(true)
        };

        before_ok && after_ok
    }
}

impl Rule for MD061ForbiddenTerms {
    fn name(&self) -> &'static str {
        "MD061"
    }

    fn description(&self) -> &'static str {
        "Forbidden terms"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return if no terms configured
        let pattern = match &self.pattern {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let mut warnings = Vec::new();

        // Use filtered_lines to skip frontmatter, code blocks, and HTML comments
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_comments()
        {
            let content = line.content;

            // Find all matches in this line
            for mat in pattern.find_iter(content) {
                // Skip if inside inline code (col is 1-indexed)
                if ctx.is_in_code_span(line.line_num, mat.start() + 1) {
                    continue;
                }

                // Check word boundaries
                if !Self::is_word_boundary(content, mat.start(), mat.end()) {
                    continue;
                }

                let matched_term = &content[mat.start()..mat.end()];
                let display_term = if self.config.case_sensitive {
                    matched_term.to_string()
                } else {
                    matched_term.to_uppercase()
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    severity: Severity::Warning,
                    message: format!("Found forbidden term '{display_term}'"),
                    line: line.line_num,
                    column: mat.start() + 1,
                    end_line: line.line_num,
                    end_column: mat.end() + 1,
                    fix: None, // No auto-fix for warning comments
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No auto-fix for this rule - return content unchanged
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, _ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no terms configured
        self.config.terms.is_empty()
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD061Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD061Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD061Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    #[test]
    fn test_empty_config_no_warnings() {
        let rule = MD061ForbiddenTerms::default();
        let content = "# TODO: This should not trigger\n\nFIXME: This too\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_configured_terms_detected() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string(), "FIXME".to_string()], false);
        let content = "# Heading\n\nTODO: Implement this\n\nFIXME: Fix this bug\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("forbidden term"));
        assert!(result[0].message.contains("TODO"));
        assert!(result[1].message.contains("forbidden term"));
        assert!(result[1].message.contains("FIXME"));
    }

    #[test]
    fn test_case_sensitive_by_default() {
        // Default is case-sensitive, so only exact match "TODO" is found
        let config = MD061Config {
            terms: vec!["TODO".to_string()],
            ..Default::default()
        };
        let rule = MD061ForbiddenTerms::from_config_struct(config);
        let content = "todo: lowercase\nTODO: uppercase\nTodo: mixed\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2); // Only "TODO" on line 2 matches
    }

    #[test]
    fn test_case_insensitive_opt_in() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "todo: lowercase\nTODO: uppercase\nTodo: mixed\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_case_sensitive_mode() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], true);
        let content = "todo: lowercase\nTODO: uppercase\nTodo: mixed\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_word_boundary_no_false_positive() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "TODOMORROW is not a match\nTODO is a match\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_word_boundary_with_punctuation() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "TODO: colon\nTODO. period\n(TODO) parens\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_skip_fenced_code_block() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "# Heading\n\n```\nTODO: in code block\n```\n\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_skip_indented_code_block() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "# Heading\n\n    TODO: in indented code\n\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_skip_inline_code() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "Here is `TODO` in inline code\nTODO: outside inline\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_skip_frontmatter() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "---\ntitle: TODO in frontmatter\n---\n\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_multiple_terms_on_same_line() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string(), "FIXME".to_string()], false);
        let content = "TODO: first thing FIXME: second thing\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_term_at_start_of_line() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "TODO at start\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_term_at_end_of_line() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "something TODO\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_custom_terms() {
        let rule = MD061ForbiddenTerms::new(vec!["HACK".to_string(), "XXX".to_string()], false);
        let content = "HACK: workaround\nXXX: needs review\nTODO: not configured\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_no_fix_available() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "TODO: something\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_column_positions() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        // Use 2 spaces, not 4 (4 spaces creates a code block)
        let content = "  TODO: indented\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 3); // 1-based column, TODO starts at col 3
        assert_eq!(result[0].end_column, 7);
    }

    #[test]
    fn test_config_from_toml() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config.values.insert(
            "terms".to_string(),
            toml::Value::Array(vec![toml::Value::String("FIXME".to_string())]),
        );
        config.rules.insert("MD061".to_string(), rule_config);

        let rule = MD061ForbiddenTerms::from_config(&config);
        let content = "FIXME: configured\nTODO: not configured\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("forbidden term"));
        assert!(result[0].message.contains("FIXME"));
    }

    #[test]
    fn test_config_from_toml_case_sensitive_by_default() {
        // Simulates user config: [MD061] terms = ["TODO"]
        // Without explicitly setting case_sensitive, should default to true
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config.values.insert(
            "terms".to_string(),
            toml::Value::Array(vec![toml::Value::String("TODO".to_string())]),
        );
        config.rules.insert("MD061".to_string(), rule_config);

        let rule = MD061ForbiddenTerms::from_config(&config);
        let content = "todo: lowercase\nTODO: uppercase\nTodo: mixed\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only match "TODO" (uppercase), not "todo" or "Todo"
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_skip_html_comment() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "<!-- TODO: in html comment -->\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_skip_double_backtick_inline_code() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "Here is ``TODO`` in double backticks\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_skip_triple_backtick_inline_code() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        let content = "Here is ```TODO``` in triple backticks\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_inline_code_with_backtick_content() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], false);
        // Content with a backtick inside: `` `TODO` ``
        let content = "Use `` `TODO` `` to show a backtick\nTODO: outside\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }
}
