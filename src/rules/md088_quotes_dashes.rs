//! Rule MD088: Replace look-alike quotes and dashes with ASCII equivalents.
//!
//! This rule flags all kinds of Unicode quotes and optionally dashes
//! that are often introduced by smart-quote substitutions and rich text copy/paste,
//! and replaces them with plain ASCII equivalents.

mod md088_config;

use std::collections::HashSet;

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;
use crate::utils::unicode;
use md088_config::MD088Config;

/// Rule MD088: Look-alike punctuation.
#[derive(Debug, Clone)]
pub struct MD088QuotesDashes {
    config: MD088Config,
}

impl Default for MD088QuotesDashes {
    fn default() -> Self {
        Self::from_config_struct(MD088Config::default())
    }
}

impl MD088QuotesDashes {
    fn from_config_struct(config: MD088Config) -> Self {
        Self { config }
    }

    #[inline]
    fn is_allowed(&self, c: char) -> bool {
        self.config.allow.contains(&c)
    }

    #[inline]
    fn replacement_for(&self, c: char) -> Option<&'static str> {
        if self.is_allowed(c) {
            return None;
        }

        match c {
            '\u{02BC}' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}'
                if self.config.normalize_quotes =>
            {
                Some("'")
            }
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' if self.config.normalize_quotes => {
                Some("\"")
            }
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
                if self.config.normalize_dashes =>
            {
                Some("-")
            }
            _ => None,
        }
    }

    #[inline]
    fn has_target_char(&self, ctx: &LintContext) -> bool {
        ctx.content
            .chars()
            .collect::<HashSet<char>>()
            .iter()
            .any(|&c| self.replacement_for(c).is_some())
    }
}

impl Rule for MD088QuotesDashes {
    fn name(&self) -> &'static str {
        "MD088"
    }

    fn description(&self) -> &'static str {
        "Quotes and dashes should be replaced with ASCII equivalents"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.content.is_empty() || !self.has_target_char(ctx)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for line in ctx.filtered_lines().skip_front_matter().skip_code_blocks() {
            for (byte_idx, c) in line.content.char_indices() {
                let Some(replacement) = self.replacement_for(c) else {
                    continue;
                };

                let absolute_byte = line.line_info.byte_offset + byte_idx;
                if ctx.is_byte_offset_in_code_span(absolute_byte) {
                    continue;
                }

                let column = byte_to_char_count(line.content, byte_idx);
                let fix = if ctx.is_in_link(absolute_byte) || ctx.is_in_bare_url(absolute_byte) {
                    None
                } else {
                    Some(Fix::new(
                        ctx.line_index
                            .line_col_to_byte_range_with_length(line.line_num, column, 1),
                        replacement.to_string(),
                    ))
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: line.line_num,
                    column,
                    end_line: line.line_num,
                    end_column: column + 1,
                    severity: Severity::Warning,
                    message: format!(
                        "Unicode character {} ({}) should be replaced with {}",
                        c,
                        unicode::format_codepoint(c),
                        replacement
                    ),
                    fix,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD088Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MarkdownFlavor};

    #[test]
    fn test_detects_and_fixes_single_and_double_quotes_look_alikes() {
        let rule = MD088QuotesDashes::default();
        let ctx = LintContext::new("It\u{2019}s \u{201C}fine\u{201D}.", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "It's \"fine\".");
    }

    #[test]
    fn test_detects_and_fixes_dash_look_alikes() {
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_dashes: true,
            ..Default::default()
        });
        let ctx = LintContext::new(
            "Dash variants: \u{2010}\u{2011}\u{2012}\u{2013}\u{2014}\u{2015}",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 6);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Dash variants: ------");
    }

    #[test]
    fn test_detects_and_fixes_prime_marks() {
        let rule = MD088QuotesDashes::default();
        let ctx = LintContext::new("Sizes: 6\u{2032} and 8\u{2033}", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Sizes: 6' and 8\"");
    }

    #[test]
    fn test_skips_inline_code_spans() {
        let rule = MD088QuotesDashes::default();
        let content = "Prose \u{2019}quote and `code \u{2019}quote`";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 7);
        assert_eq!(rule.fix(&ctx).unwrap(), "Prose 'quote and `code \u{2019}quote`");
    }

    #[test]
    fn test_skips_fenced_and_indented_code_blocks() {
        let rule = MD088QuotesDashes::default();
        let content = r#"
This is a ‘quote in prose’.

```
This is a ‘quote in fenced code block’
```

    This is a ‘quote in indented code block’
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            r#"
This is a 'quote in prose'.

```
This is a ‘quote in fenced code block’
```

    This is a ‘quote in indented code block’
"#
        );
    }

    #[test]
    fn test_no_findings_for_plain_ascii_quotes() {
        let rule = MD088QuotesDashes::default();
        let content = "He said, \"it's fine\".";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_allow_list_keeps_configured_codepoints() {
        let config: Config = toml::from_str(
            r#"
            [MD088]
            allow = ["U+2019", "U+2014"]
            "#,
        )
        .unwrap();

        let rule = MD088QuotesDashes::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "It\u{2019}s \u{2014} and \u{201C}quoted\u{201D}";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "It\u{2019}s \u{2014} and \"quoted\"");
    }

    #[test]
    fn test_reports_but_does_not_fix_bare_urls() {
        let config: Config = toml::from_str(
            r#"
            [MD088]
            normalize-quotes = true
            normalize-dashes = true
            "#,
        )
        .unwrap();

        let rule = MD088QuotesDashes::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "Visit https://this\u{2010}site.com and \u{201C}enjoy\u{201D}!.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3, "Expected 3 warnings, got {warnings:#?}");

        let url_warning = warnings.iter().find(|w| w.column > 10).unwrap();
        assert!(url_warning.fix.is_none());

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Visit https://this\u{2010}site.com and \"enjoy\"!.");
    }

    #[test]
    fn test_reports_but_does_not_fix_findings_in_link_destinations() {
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_quotes: true,
            normalize_dashes: true,
            ..Default::default()
        });
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "[link](https://example\u{2010}\u{2018}.com) and prose \u{2010}.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3, "Expected 3 warnings, got {warnings:#?}");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](https://example\u{2010}\u{2018}.com) and prose -.");
    }
}
