use crate::filtered_lines::FilteredLinesExt;
use regex::{Match, Regex, RegexBuilder};

use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;

mod md061_config;
pub(super) use md061_config::MD061Config;

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

        // Simple case folding can change the UTF-8 width of a character.
        let mut terms: Vec<&str> = config.terms.iter().map(String::as_str).collect();
        terms.sort_by_cached_key(|term| std::cmp::Reverse(term.chars().count()));
        let pattern_str = terms.into_iter().map(regex::escape).collect::<Vec<_>>().join("|");

        RegexBuilder::new(&pattern_str)
            .case_insensitive(!config.case_sensitive)
            .build()
            .ok()
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    fn with_right_boundary<'a>(pattern: &Regex, content: &'a str, mut term: Match<'a>) -> Option<Match<'a>> {
        while content[term.end()..].chars().next().is_some_and(Self::is_word_char) {
            // A shorter term can end immediately before punctuation inside
            // this one. Search the same start with that boundary as the limit.
            let (offset, _) = term
                .as_str()
                .char_indices()
                .rev()
                .find(|&(_, c)| !Self::is_word_char(c))?;
            let start = term.start();
            term = pattern
                .find_at(&content[..start + offset], start)
                .filter(|m| m.start() == start)?;
        }
        Some(term)
    }
}

impl Rule for MD061ForbiddenTerms {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(61))
    }

    fn name(&self) -> &'static str {
        "MD061"
    }

    fn description(&self) -> &'static str {
        "Forbidden terms"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return if no terms configured
        let Some(pattern) = &self.pattern else {
            return Ok(Vec::new());
        };

        let mut warnings = Vec::new();

        // Use filtered_lines to skip frontmatter, code blocks, HTML comments, and Obsidian comments
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_comments()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .skip_obsidian_comments()
        {
            let content = line.content;

            let mut pos = Some(0);
            let mut previous_match_end = None;
            while let Some(at) = pos
                && let Some(candidate) = pattern.find_at(content, at)
            {
                let start = candidate.start();
                if content[..start].chars().next_back().is_some_and(Self::is_word_char) {
                    pos = content[start..]
                        .char_indices()
                        .find(|&(_, c)| !Self::is_word_char(c))
                        .map(|(offset, c)| start + offset + c.len_utf8());
                    continue;
                }
                pos = content[start..].chars().next().map(|c| start + c.len_utf8());
                let Some(term) = Self::with_right_boundary(pattern, content, candidate) else {
                    continue;
                };
                let end = term.end();
                if start < end {
                    pos = Some(end);
                } else if previous_match_end == Some(end) {
                    continue;
                }
                previous_match_end = Some(end);

                // Skip if inside inline code (col is a 1-indexed character column)
                let column = byte_to_char_count(content, start);
                if ctx.is_in_code_span(line.line_num, column) {
                    continue;
                }

                // Quote the term as the document writes it. The reported range covers
                // exactly these bytes, and under case-insensitive matching the text can
                // differ from the configured spelling, so any case-folded form would
                // name something that appears in neither the document nor the config.
                let matched_term = term.as_str();

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    severity: Severity::Warning,
                    message: format!("Found forbidden term '{matched_term}'"),
                    line: line.line_num,
                    column,
                    end_line: line.line_num,
                    end_column: byte_to_char_count(content, end),
                    fix: None, // No auto-fix for warning comments
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, _ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no terms configured
        self.config.terms.is_empty()
    }

    crate::impl_rule_config_methods!(MD061Config);
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
    fn test_message_quotes_document_casing() {
        // Case-insensitive matching quotes the document, not a case-folded form:
        // "DELVE" appears in neither the document nor the configured terms.
        let rule = MD061ForbiddenTerms::new(vec!["delve".to_string()], false);
        let content = "We Delve into it.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Found forbidden term 'Delve'");
    }

    #[test]
    fn test_message_quotes_multi_word_document_casing() {
        let rule = MD061ForbiddenTerms::new(vec!["at the end of the day".to_string()], false);
        let content = "At the end of the day it ships.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Found forbidden term 'At the end of the day'");
    }

    #[test]
    fn test_message_quotes_exact_text_when_case_sensitive() {
        let rule = MD061ForbiddenTerms::new(vec!["TODO".to_string()], true);
        let content = "TODO: something\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Found forbidden term 'TODO'");
    }

    #[test]
    fn test_message_quotes_what_the_range_covers() {
        // The quoted term and the reported range name the same text, including on a
        // line whose match is preceded by multi-byte characters.
        let rule = MD061ForbiddenTerms::new(vec!["delve".to_string()], false);
        let content = "你好 Delve deeper\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let warning = &result[0];
        let line: Vec<char> = content.lines().next().unwrap().chars().collect();
        let spanned: String = line[warning.column - 1..warning.end_column - 1].iter().collect();
        assert_eq!(spanned, "Delve");
        assert_eq!(warning.message, format!("Found forbidden term '{spanned}'"));
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
    fn test_term_that_extends_an_earlier_term() {
        let rule = MD061ForbiddenTerms::new(vec!["FIX".to_string(), "FIXME".to_string()], true);
        let content = "FIXME: later\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("FIXME"));
        assert_eq!(result[0].column, 1);
        assert_eq!(result[0].end_column, 6);
    }

    #[test]
    fn test_longer_term_listed_first() {
        let rule = MD061ForbiddenTerms::new(vec!["FIXME".to_string(), "FIX".to_string()], true);
        let content = "FIXME: later\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("FIXME"));
    }

    #[test]
    fn test_prefix_term_still_matches_on_its_own() {
        let rule = MD061ForbiddenTerms::new(vec!["blacklist".to_string(), "blacklisted".to_string()], true);
        let content = "The host is blacklisted.\nAdd it to the blacklist.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("blacklisted"));
        assert_eq!(result[0].line, 1);
        assert!(result[1].message.contains("'blacklist'"));
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_shorter_term_can_supply_the_right_boundary() {
        for terms in [["foo", "foo-bar", "foo-barx-baz"], ["foo-barx-baz", "foo-bar", "foo"]] {
            let rule = MD061ForbiddenTerms::new(terms.map(String::from).to_vec(), true);
            let ctx = LintContext::new("foo-barx-bazx\n", MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].message, "Found forbidden term 'foo'");
            assert_eq!((result[0].column, result[0].end_column), (1, 4));
        }
    }

    #[test]
    fn test_case_folded_prefix_uses_character_length() {
        for terms in [["kg", "\u{212A}"], ["\u{212A}", "kg"]] {
            let rule = MD061ForbiddenTerms::new(terms.map(String::from).to_vec(), false);
            let ctx = LintContext::new("kg K kg\n", MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 3);
            assert_eq!(result[0].message, "Found forbidden term 'kg'");
            assert_eq!(result[1].message, "Found forbidden term 'K'");
            assert_eq!(result[2].message, "Found forbidden term 'kg'");
        }
    }

    #[test]
    fn test_boundaries_preserve_adjacent_terms_and_later_starts() {
        let rule = MD061ForbiddenTerms::new(["foo-bar", "bar", "next"].map(String::from).to_vec(), true);
        let ctx = LintContext::new("xfoo-bar next\n", MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Found forbidden term 'bar'");
        assert_eq!(result[1].message, "Found forbidden term 'next'");
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
