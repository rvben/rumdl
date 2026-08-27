/// Rule MD026: No trailing punctuation in headings
///
/// See [docs/md026.md](../../docs/md026.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::FlavorOverrideNotice;
use crate::utils::range_utils::calculate_match_range;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::RwLock;

mod md026_config;
use md026_config::{DEFAULT_PUNCTUATION, MD026Config};

// Optimized single regex for all ATX heading types (normal, closed, indented 1-3 spaces)
static ATX_HEADING_UNIFIED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^( {0,3})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap());

// Fast check patterns for early returns - match defaults
static QUICK_PUNCTUATION_CHECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"[{}]", regex::escape(DEFAULT_PUNCTUATION))).unwrap());

// Regex cache for punctuation patterns
static PUNCTUATION_REGEX_CACHE: LazyLock<RwLock<HashMap<String, Regex>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Reports the MDG punctuation override once per process, across every config group.
static MDG_PUNCTUATION_OVERRIDE: FlavorOverrideNotice = FlavorOverrideNotice::new();

/// Rule MD026: Trailing punctuation in heading
#[derive(Clone)]
pub struct MD026NoTrailingPunctuation {
    config: MD026Config,
    // A Gherkin structure is an ATX heading spelled `Keyword: name`, so the colon after the
    // keyword is what makes the keyword a keyword. Dropping it from the punctuation set for
    // that flavor keeps the rule from ever deleting it, however `punctuation` is configured.
    mdg_punctuation: String,
    // Whether the colon the Gherkin flavor drops was asked for rather than inherited from
    // the default.
    colon_configured_explicitly: bool,
}

impl Default for MD026NoTrailingPunctuation {
    fn default() -> Self {
        Self::new(None)
    }
}

impl MD026NoTrailingPunctuation {
    pub fn new(punctuation: Option<String>) -> Self {
        let explicit = punctuation.is_some();
        Self::build(
            MD026Config {
                punctuation: punctuation.unwrap_or_else(|| DEFAULT_PUNCTUATION.to_string()),
            },
            explicit,
        )
    }

    pub fn from_config_struct(config: MD026Config) -> Self {
        Self::build(config, false)
    }

    fn build(config: MD026Config, punctuation_explicit: bool) -> Self {
        let colon_configured_explicitly = punctuation_explicit && config.punctuation.contains(':');
        let mdg_punctuation = config.punctuation.replace(':', "");

        Self {
            config,
            mdg_punctuation,
            colon_configured_explicitly,
        }
    }

    /// The punctuation set the flavor actually enforces.
    #[inline]
    fn effective_punctuation(&self, flavor: crate::config::MarkdownFlavor) -> &str {
        if flavor == crate::config::MarkdownFlavor::MDG {
            &self.mdg_punctuation
        } else {
            &self.config.punctuation
        }
    }

    /// Whether MDG is overriding a colon that came from explicit configuration.
    fn mdg_colon_override_applies(&self, flavor: crate::config::MarkdownFlavor) -> bool {
        flavor == crate::config::MarkdownFlavor::MDG && self.colon_configured_explicitly
    }

    /// Report the effective punctuation override before any content-based
    /// shortcut can skip this rule. Direct `check` callers also pass through
    /// here; the shared notice keeps it process-local and one-shot even when a
    /// run creates separate rule instances for multiple config groups.
    fn warn_once_about_mdg_colon_override(&self, flavor: crate::config::MarkdownFlavor) {
        if self.mdg_colon_override_applies(flavor) {
            MDG_PUNCTUATION_OVERRIDE.report(
                "MD026",
                "punctuation",
                &self.config.punctuation,
                &self.mdg_punctuation,
                "the ASCII colon after a Gherkin keyword is structural",
            );
        }
    }

    #[inline]
    fn get_punctuation_regex(&self, punctuation: &str) -> Result<Regex, regex::Error> {
        // Check cache first
        {
            let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
            if let Some(cached_regex) = cache.get(punctuation) {
                return Ok(cached_regex.clone());
            }
        }

        // Compile and cache the regex
        let pattern = format!(r"([{}]+)$", regex::escape(punctuation));
        let regex = Regex::new(&pattern)?;

        {
            let mut cache = PUNCTUATION_REGEX_CACHE.write().unwrap();
            cache.insert(punctuation.to_string(), regex.clone());
        }

        Ok(regex)
    }

    #[inline]
    fn has_trailing_punctuation(&self, text: &str, re: &Regex) -> bool {
        let trimmed = text.trim();
        re.is_match(trimmed)
    }

    // Remove trailing punctuation from text.
    //
    // A single removal is not enough when punctuation is separated from the end by
    // interior whitespace (e.g. `. :`): removing `:` leaves `. `, whose trailing space
    // then exposes `.`, so a second fix pass would change the result again. This keeps
    // stripping while trimming the exposed whitespace reveals further trailing
    // punctuation, making one fix call fully converge (idempotent). Trailing whitespace
    // that does not hide more punctuation is preserved, matching the single-punctuation
    // behavior (e.g. `Title :` -> `Title `).
    #[inline]
    fn remove_trailing_punctuation(&self, text: &str, re: &Regex) -> String {
        let mut result = text.trim().to_string();
        loop {
            let stripped = re.replace(&result, "").into_owned();
            if stripped.len() == result.len() {
                // No trailing punctuation run at the very end.
                return stripped;
            }
            // Continue only if trimming the whitespace exposed by this removal reveals
            // more trailing punctuation; otherwise keep the result (whitespace and all).
            let trimmed = stripped.trim_end();
            if trimmed.len() != stripped.len() && re.is_match(trimmed) {
                result = trimmed.to_string();
            } else {
                return stripped;
            }
        }
    }

    // Optimized ATX heading fix using unified regex
    #[inline]
    fn fix_atx_heading(&self, line: &str, re: &Regex) -> String {
        if let Some(captures) = ATX_HEADING_UNIFIED.captures(line) {
            let indentation = captures.get(1).unwrap().as_str();
            let hashes = captures.get(2).unwrap().as_str();
            let space = captures.get(3).unwrap().as_str();
            let content = captures.get(4).unwrap().as_str();

            // Check if content ends with a custom header ID like {#my-id}
            // If so, we need to fix punctuation before the ID
            let fixed_content = if let Some(id_pos) = content.rfind(" {#") {
                // Has a custom ID - fix punctuation before it
                let before_id = &content[..id_pos];
                let id_part = &content[id_pos..];
                let fixed_before = self.remove_trailing_punctuation(before_id, re);
                format!("{fixed_before}{id_part}")
            } else {
                // No custom ID - just remove trailing punctuation
                self.remove_trailing_punctuation(content, re)
            };

            // Preserve any trailing hashes if present
            if let Some(trailing) = captures.get(5) {
                return format!(
                    "{}{}{}{}{}",
                    indentation,
                    hashes,
                    space,
                    fixed_content,
                    trailing.as_str()
                );
            }

            return format!("{indentation}{hashes}{space}{fixed_content}");
        }

        // Fallback if no regex matches
        line.to_string()
    }

    // Fix a setext heading by removing trailing punctuation from the content line
    #[inline]
    fn fix_setext_heading(&self, content_line: &str, re: &Regex) -> String {
        let trimmed = content_line.trim_end();
        let mut whitespace = "";

        // Preserve trailing whitespace
        if content_line.len() > trimmed.len() {
            whitespace = &content_line[trimmed.len()..];
        }

        // Remove punctuation and preserve whitespace
        format!("{}{}", self.remove_trailing_punctuation(trimmed, re), whitespace)
    }
}

impl Rule for MD026NoTrailingPunctuation {
    fn name(&self) -> &'static str {
        "MD026"
    }

    fn description(&self) -> &'static str {
        "Trailing punctuation in heading"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.warn_once_about_mdg_colon_override(ctx.flavor);

        // Skip if no heading markers
        if !ctx.likely_has_headings() {
            return true;
        }
        // Skip if none of the configured punctuation exists
        let punctuation = self.effective_punctuation(ctx.flavor);
        !punctuation.chars().any(|p| ctx.content.contains(p))
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let punctuation = self.effective_punctuation(ctx.flavor);

        self.warn_once_about_mdg_colon_override(ctx.flavor);

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        // For custom punctuation, we need to check differently
        if punctuation == DEFAULT_PUNCTUATION {
            if !QUICK_PUNCTUATION_CHECK.is_match(content) {
                return Ok(Vec::new());
            }
        } else {
            // For custom punctuation, check if any of those characters exist
            let has_custom_punctuation = punctuation.chars().any(|c| content.contains(c));
            if !has_custom_punctuation {
                return Ok(Vec::new());
            }
        }

        // Check if we have any headings from pre-computed line info
        let has_headings = ctx.lines.iter().any(|line| line.heading.is_some());
        if !has_headings {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let Ok(re) = self.get_punctuation_regex(punctuation) else {
            return Ok(warnings);
        };

        // Use pre-computed heading information from LintContext
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                // Skip deeply indented headings (they're code blocks)
                if line_info.visual_indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    continue;
                }

                // LintContext already strips Kramdown IDs from heading.text
                // So we just check the heading text directly for trailing punctuation
                // This correctly flags "# Heading." even if it has {#id}
                let text_to_check = heading.text.as_str();

                if self.has_trailing_punctuation(text_to_check, &re) {
                    // Find the trailing punctuation
                    if let Some(punctuation_match) = re.find(text_to_check) {
                        let line = line_info.content(ctx.content);

                        // For ATX headings, find the punctuation position in the line
                        let punctuation_pos_in_text = punctuation_match.start();
                        let text_pos_in_line = line.find(&heading.text).unwrap_or(heading.content_column);
                        let punctuation_start_in_line = text_pos_in_line + punctuation_pos_in_text;
                        let punctuation_len = punctuation_match.len();

                        let (start_line, start_col, end_line, end_col) = calculate_match_range(
                            line_num + 1, // Convert to 1-indexed
                            line,
                            punctuation_start_in_line,
                            punctuation_len,
                        );

                        let last_char = text_to_check.chars().last().unwrap_or(' ');
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Heading '{text_to_check}' ends with punctuation '{last_char}'"),
                            severity: Severity::Warning,
                            fix: Some(Fix::new(
                                ctx.line_content_byte_range(line_num + 1),
                                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                                    self.fix_atx_heading(line, &re)
                                } else {
                                    self.fix_setext_heading(line, &re)
                                },
                            )),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
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

    crate::impl_rule_config_sections!(MD026Config);

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD026Config>(config);

        // Check if punctuation was explicitly set in the config; the Gherkin flavor drops
        // the colon from it, and an override the user asked for is worth reporting.
        let punctuation_explicit = config
            .rules
            .get("MD026")
            .is_some_and(|rule_cfg| rule_cfg.values.contains_key("punctuation"));

        Box::new(Self::build(rule_config, punctuation_explicit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_no_trailing_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# This is a heading\n\n## Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Headings without punctuation should not be flagged");
    }

    #[test]
    fn test_trailing_period() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# This is a heading.\n\n## Another one.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 20);
        assert!(result[0].message.contains("ends with punctuation '.'"));
        assert_eq!(result[1].line, 3);
        assert_eq!(result[1].column, 15);
    }

    #[test]
    fn test_trailing_comma() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Heading,\n## Sub-heading,";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ','"));
    }

    #[test]
    fn test_trailing_semicolon() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title;\n## Subtitle;";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ';'"));
    }

    #[test]
    fn test_custom_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let content = "# Important!\n## Regular heading.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Only exclamation should be flagged with custom config");
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("ends with punctuation '!'"));
    }

    #[test]
    fn test_legitimate_question_mark() {
        let rule = MD026NoTrailingPunctuation::new(Some(".,;?".to_string()));
        let content = "# What is this?\n# This is bad.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // With custom punctuation, legitimate punctuation exceptions don't apply
        assert_eq!(result.len(), 2, "Both should be flagged with custom punctuation");
    }

    #[test]
    fn test_question_marks_not_in_default() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# What is Rust?\n# How does it work?\n# Is it fast?";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Question marks are not in default punctuation list");
    }

    #[test]
    fn test_colons_in_default() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# FAQ:\n# API Reference:\n# Step 1:\n# Version 2.0:";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            4,
            "Colons are in default punctuation list and should be flagged"
        );
    }

    #[test]
    fn test_fix_atx_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n## Subtitle,\n### Sub-subtitle;";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n## Subtitle\n### Sub-subtitle");
    }

    #[test]
    fn test_fix_setext_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "Title.\n======\n\nSubtitle,\n---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Title\n======\n\nSubtitle\n---------");
    }

    #[test]
    fn test_fix_preserves_trailing_hashes() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title. #\n## Subtitle, ##";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title #\n## Subtitle ##");
    }

    #[test]
    fn test_indented_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "   # Title.\n  ## Subtitle.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Indented headings (< 4 spaces) should be checked");
    }

    #[test]
    fn test_deeply_indented_ignored() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "    # This is code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Deeply indented lines (4+ spaces) should be ignored");
    }

    #[test]
    fn test_multiple_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title...";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 8); // Points to first period
    }

    #[test]
    fn test_empty_content() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "This is just text.\nMore text with punctuation.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Non-heading lines should not be checked");
    }

    #[test]
    fn test_get_punctuation_regex() {
        let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
        let regex = rule.get_punctuation_regex(&rule.config.punctuation).unwrap();
        assert!(regex.is_match("text!"));
        assert!(regex.is_match("text?"));
        assert!(!regex.is_match("text."));
    }

    #[test]
    fn test_regex_caching() {
        let rule1 = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let rule2 = MD026NoTrailingPunctuation::new(Some("!".to_string()));

        // Both should get the same cached regex
        let _regex1 = rule1.get_punctuation_regex(&rule1.config.punctuation).unwrap();
        let _regex2 = rule2.get_punctuation_regex(&rule2.config.punctuation).unwrap();

        // Check cache has the entry
        let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
        assert!(cache.contains_key("!"));
    }

    #[test]
    fn test_config_from_toml() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("punctuation".to_string(), toml::Value::String("!?".to_string()));
        config.rules.insert("MD026".to_string(), rule_config);

        let rule = MD026NoTrailingPunctuation::from_config(&config);
        let content = "# Title!\n# Another?";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Custom punctuation from config should be used");
    }

    #[test]
    fn test_fix_removes_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.   \n## Subtitle,  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // The current implementation doesn't preserve trailing whitespace after punctuation removal
        assert_eq!(fixed, "# Title\n## Subtitle");
    }

    #[test]
    fn test_final_newline_preservation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n");

        let content_no_newline = "# Title.";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "# Title");
    }

    /// The whole MDG matrix in one place, against the Standard behavior it departs from.
    ///
    /// The rule's pattern is `([<punctuation>]+)$`, so taking the colon out of the set is
    /// the entire flavor difference: no heading whose last character is a colon can match
    /// any more, and `## Scenario!:` is left exactly as its author wrote it.
    #[test]
    fn test_mdg_punctuation_matrix() {
        let rule = MD026NoTrailingPunctuation::new(None);
        // (input, standard warnings, standard fix, MDG warnings, MDG fix)
        let cases = [
            ("## Notes:\n", 1, "## Notes\n", 0, "## Notes:\n"),
            ("## Scenario!:\n", 1, "## Scenario\n", 0, "## Scenario!:\n"),
            ("# Scenario! :\n", 1, "# Scenario\n", 0, "# Scenario! :\n"),
            ("## Scenario!\n", 1, "## Scenario\n", 1, "## Scenario\n"),
            ("## Notes::\n", 1, "## Notes\n", 0, "## Notes::\n"),
            (
                "# Feature: Checkout:\n",
                1,
                "# Feature: Checkout\n",
                0,
                "# Feature: Checkout:\n",
            ),
            ("### Rule.:\n", 1, "### Rule\n", 0, "### Rule.:\n"),
        ];

        for (input, standard_count, standard_fixed, mdg_count, mdg_fixed) in cases {
            for (flavor, count, expected) in [
                (crate::config::MarkdownFlavor::Standard, standard_count, standard_fixed),
                (crate::config::MarkdownFlavor::MDG, mdg_count, mdg_fixed),
            ] {
                let ctx = LintContext::new(input, flavor, None);
                assert_eq!(
                    rule.check(&ctx).unwrap().len(),
                    count,
                    "{flavor:?} warning count for {input:?}"
                );

                let fixed = rule.fix(&ctx).unwrap();
                assert_eq!(fixed, expected, "{flavor:?} fix for {input:?}");

                let fixed_ctx = LintContext::new(&fixed, flavor, None);
                assert!(
                    rule.check(&fixed_ctx).unwrap().is_empty(),
                    "{flavor:?} left a warning on the fixed {input:?}"
                );
                assert_eq!(
                    rule.fix(&fixed_ctx).unwrap(),
                    fixed,
                    "{flavor:?} fix for {input:?} should be idempotent"
                );
            }
        }
    }

    /// The colon leaves the effective set whatever `punctuation` says, so a future change
    /// to the default cannot put it back; only an explicit setting is worth reporting.
    #[test]
    fn test_mdg_reports_an_explicitly_configured_colon_once() {
        fn configured(punctuation: &str) -> MD026NoTrailingPunctuation {
            let mut config = crate::config::Config::default();
            let mut rule_config = crate::config::RuleConfig::default();
            rule_config
                .values
                .insert("punctuation".to_string(), toml::Value::String(punctuation.to_string()));
            config.rules.insert("MD026".to_string(), rule_config);

            MD026NoTrailingPunctuation::from_config(&config)
                .as_any()
                .downcast_ref::<MD026NoTrailingPunctuation>()
                .expect("MD026::from_config builds an MD026NoTrailingPunctuation")
                .clone()
        }

        let explicit = configured(".,;:!");
        assert_eq!(
            explicit.effective_punctuation(crate::config::MarkdownFlavor::Standard),
            ".,;:!"
        );
        assert_eq!(
            explicit.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!"
        );
        assert!(
            !explicit.mdg_colon_override_applies(crate::config::MarkdownFlavor::Standard),
            "a non-Gherkin file never reports the override"
        );
        assert!(explicit.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG));

        let emitted_by_check = configured(".,;:!");
        let ctx = LintContext::new("## Scenario!\n", crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(emitted_by_check.check(&ctx).unwrap().len(), 1);

        let emitted_before_skip = configured(".,;:!");
        let only_protected_punctuation = LintContext::new("#### Examples:\n", crate::config::MarkdownFlavor::MDG, None);
        assert!(
            emitted_before_skip.should_skip(&only_protected_punctuation),
            "the post-override punctuation set has nothing to inspect"
        );

        let explicit_without_colon = configured(".,;!");
        assert!(
            !explicit_without_colon.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG),
            "nothing is overridden when the configured set has no colon"
        );

        let default = MD026NoTrailingPunctuation::new(None);
        assert_eq!(
            default.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!",
            "the colon leaves the default set too"
        );
        assert!(
            !default.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG),
            "the default set is not an explicit configuration, so it is silent"
        );
    }

    #[test]
    fn test_mdg_exempts_a_lone_colon_behind_whitespace() {
        // The colon is not punctuation under MDG, and it is the last character here, so
        // there is nothing for the `$`-anchored pattern to match. Standard still strips it,
        // keeping the whitespace that preceded it.
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Scenario :\n## Notes:\n";

        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let standard = rule.check(&standard_ctx).unwrap();
        assert_eq!(standard.len(), 2);
        assert_eq!((standard[0].line, standard[0].column), (1, 12));
        assert_eq!((standard[1].line, standard[1].column), (2, 9));
        let standard_fixed = rule.fix(&standard_ctx).unwrap();
        assert_eq!(standard_fixed, "# Scenario \n## Notes\n");
        let standard_fixed_ctx = LintContext::new(&standard_fixed, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&standard_fixed_ctx).unwrap(),
            standard_fixed,
            "Standard fix should be idempotent"
        );

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&mdg_ctx).unwrap().is_empty());
        assert_eq!(
            rule.fix(&mdg_ctx).unwrap(),
            content,
            "MDG leaves headings whose only trailing punctuation is the colon untouched"
        );
    }

    #[test]
    fn test_mdg_does_not_exempt_a_full_width_colon() {
        // Gherkin only recognizes the ASCII colon, so a full-width one carries
        // no structural meaning and stays in the punctuation set.
        let rule = MD026NoTrailingPunctuation::new(Some(".,;:!?：".to_string()));
        assert_eq!(
            rule.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!?：",
            "only the ASCII colon leaves the set"
        );

        let content = "## Scenario：\n";
        for flavor in [
            crate::config::MarkdownFlavor::MDG,
            crate::config::MarkdownFlavor::Standard,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{flavor:?} must flag the `：`");
            assert_eq!(rule.fix(&ctx).unwrap(), "## Scenario\n");
        }
    }
}
