/// Rule MD026: No trailing punctuation in headings
///
/// See [docs/md026.md](../../docs/md026.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::RwLock;

mod md026_config;
use md026_config::MD026Config;

lazy_static! {
    // Optimized single regex for all ATX heading types (normal, closed, indented 1-3 spaces)
    static ref ATX_HEADING_UNIFIED: Regex = Regex::new(r"^( {0,3})(#{1,6})(\s+)(.+?)(\s+#{1,6})?$").unwrap();

    // Fast check patterns for early returns - more restrictive
    static ref QUICK_PUNCTUATION_CHECK: Regex = Regex::new(r"[.,;]").unwrap();

    // Regex cache for punctuation patterns
    static ref PUNCTUATION_REGEX_CACHE: RwLock<HashMap<String, Regex>> = RwLock::new(HashMap::new());
}

/// Rule MD026: Trailing punctuation in heading
#[derive(Clone, Default)]
pub struct MD026NoTrailingPunctuation {
    config: MD026Config,
}

impl MD026NoTrailingPunctuation {
    pub fn new(punctuation: Option<String>) -> Self {
        Self {
            config: MD026Config {
                punctuation: punctuation.unwrap_or_else(|| ".,;".to_string()), // More restrictive by default
            },
        }
    }

    pub fn from_config_struct(config: MD026Config) -> Self {
        Self { config }
    }

    #[inline]
    fn get_punctuation_regex(&self) -> Result<Regex, regex::Error> {
        // Check cache first
        {
            let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
            if let Some(cached_regex) = cache.get(&self.config.punctuation) {
                return Ok(cached_regex.clone());
            }
        }

        // Compile and cache the regex
        let pattern = format!(r"([{}]+)$", regex::escape(&self.config.punctuation));
        let regex = Regex::new(&pattern)?;

        {
            let mut cache = PUNCTUATION_REGEX_CACHE.write().unwrap();
            cache.insert(self.config.punctuation.clone(), regex.clone());
        }

        Ok(regex)
    }

    #[inline]
    fn has_trailing_punctuation(&self, text: &str, re: &Regex) -> bool {
        let trimmed = text.trim();

        // Only apply lenient rules for the default punctuation setting
        // When users specify custom punctuation, they want strict behavior
        if self.config.punctuation == ".,;" {
            // Check for common legitimate punctuation patterns before applying the rule
            if self.is_legitimate_punctuation(trimmed) {
                return false;
            }
        }

        re.is_match(trimmed)
    }

    #[inline]
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> Range<usize> {
        let mut start_pos = 0;

        for (idx, line) in content.lines().enumerate() {
            if idx + 1 == line_num {
                return Range {
                    start: start_pos,
                    end: start_pos + line.len(),
                };
            }
            // +1 for the newline character
            start_pos += line.len() + 1;
        }

        Range {
            start: content.len(),
            end: content.len(),
        }
    }

    /// Check if punctuation in a heading is legitimate and should be allowed
    #[inline]
    fn is_legitimate_punctuation(&self, text: &str) -> bool {
        let text = text.trim();

        // Allow question marks in question headings
        if text.ends_with('?') {
            // Check if it's likely a genuine question
            let question_words = [
                "what", "why", "how", "when", "where", "who", "which", "can", "should", "would", "could", "is", "are",
                "do", "does", "did",
            ];
            let lower_text = text.to_lowercase();
            if question_words.iter().any(|&word| lower_text.starts_with(word)) {
                return true;
            }
        }

        // Allow colons in common categorical/labeling patterns
        if text.ends_with(':') {
            // Common patterns that legitimately use colons
            let colon_patterns = [
                "faq",
                "api",
                "note",
                "warning",
                "error",
                "info",
                "tip",
                "chapter",
                "step",
                "version",
                "part",
                "section",
                "method",
                "function",
                "class",
                "module",
                "reference",
                "guide",
                "tutorial",
                "example",
                "demo",
                "usage",
                "syntax",
            ];

            let lower_text = text.to_lowercase();

            // Check if it starts with any of these patterns
            if colon_patterns.iter().any(|&pattern| lower_text.starts_with(pattern)) {
                return true;
            }

            // Check for numbered items like "Step 1:", "Chapter 2:", "Version 1.0:"
            if regex::Regex::new(r"^(step|chapter|part|section|version)\s*\d")
                .unwrap()
                .is_match(&lower_text)
            {
                return true;
            }
        }

        // Allow exclamation marks in specific contexts (less common, but sometimes legitimate)
        if text.ends_with('!') {
            // Only allow for very specific patterns like "Important!", "New!", "Warning!"
            let exclamation_patterns = ["important", "new", "warning", "alert", "notice", "attention"];
            let lower_text = text.to_lowercase();
            if exclamation_patterns
                .iter()
                .any(|&pattern| lower_text.starts_with(pattern))
            {
                return true;
            }
        }

        false
    }

    // Remove trailing punctuation from text
    #[inline]
    fn remove_trailing_punctuation(&self, text: &str, re: &Regex) -> String {
        re.replace_all(text.trim(), "").to_string()
    }

    // Optimized ATX heading fix using unified regex
    #[inline]
    fn fix_atx_heading(&self, line: &str, re: &Regex) -> String {
        if let Some(captures) = ATX_HEADING_UNIFIED.captures(line) {
            let indentation = captures.get(1).unwrap().as_str();
            let hashes = captures.get(2).unwrap().as_str();
            let space = captures.get(3).unwrap().as_str();
            let content = captures.get(4).unwrap().as_str();

            let fixed_content = self.remove_trailing_punctuation(content, re);

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

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        // For custom punctuation, we need to check differently
        if self.config.punctuation == ".,;" {
            if !QUICK_PUNCTUATION_CHECK.is_match(content) {
                return Ok(Vec::new());
            }
        } else {
            // For custom punctuation, check if any of those characters exist
            let has_custom_punctuation = self.config.punctuation.chars().any(|c| content.contains(c));
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
        let re = match self.get_punctuation_regex() {
            Ok(regex) => regex,
            Err(_) => return Ok(warnings),
        };

        // Use pre-computed heading information from LintContext
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip deeply indented headings (they're code blocks)
                if line_info.indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    continue;
                }

                // Check for trailing punctuation
                if self.has_trailing_punctuation(&heading.text, &re) {
                    // Find the trailing punctuation
                    if let Some(punctuation_match) = re.find(&heading.text) {
                        let line = &line_info.content;

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

                        let last_char = heading.text.chars().last().unwrap_or(' ');
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Heading '{}' ends with punctuation '{}'", heading.text, last_char),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: self.get_line_byte_range(content, line_num + 1),
                                replacement: if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                                    self.fix_atx_heading(line, &re)
                                } else {
                                    self.fix_setext_heading(line, &re)
                                },
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Fast path optimizations
        if content.is_empty() {
            return Ok(content.to_string());
        }

        // Quick check for punctuation
        // For custom punctuation, we need to check differently
        if self.config.punctuation == ".,;" {
            if !QUICK_PUNCTUATION_CHECK.is_match(content) {
                return Ok(content.to_string());
            }
        } else {
            // For custom punctuation, check if any of those characters exist
            let has_custom_punctuation = self.config.punctuation.chars().any(|c| content.contains(c));
            if !has_custom_punctuation {
                return Ok(content.to_string());
            }
        }

        // Check if we have any headings from pre-computed line info
        let has_headings = ctx.lines.iter().any(|line| line.heading.is_some());
        if !has_headings {
            return Ok(content.to_string());
        }

        let re = match self.get_punctuation_regex() {
            Ok(regex) => regex,
            Err(_) => return Ok(content.to_string()),
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();

        // Use pre-computed heading information from LintContext
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip deeply indented headings (they're code blocks)
                if line_info.indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    continue;
                }

                // Check and fix trailing punctuation
                if self.has_trailing_punctuation(&heading.text, &re) {
                    fixed_lines[line_num] = if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                        self.fix_atx_heading(&line_info.content, &re)
                    } else {
                        self.fix_setext_heading(&line_info.content, &re)
                    };
                }
            }
        }

        // Reconstruct content preserving line endings
        let mut result = String::with_capacity(content.len());
        for (i, line) in fixed_lines.iter().enumerate() {
            result.push_str(line);
            if i < fixed_lines.len() - 1 || content.ends_with('\n') {
                result.push('\n');
            }
        }

        Ok(result)
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

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD026Config>(config);
        Box::new(Self::from_config_struct(rule_config))
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Headings without punctuation should not be flagged");
    }

    #[test]
    fn test_trailing_period() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# This is a heading.\n\n## Another one.";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ','"));
    }

    #[test]
    fn test_trailing_semicolon() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title;\n## Subtitle;";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ';'"));
    }

    #[test]
    fn test_custom_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let content = "# Important!\n## Regular heading.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Only exclamation should be flagged with custom config");
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("ends with punctuation '!'"));
    }

    #[test]
    fn test_legitimate_question_mark() {
        let rule = MD026NoTrailingPunctuation::new(Some(".,;?".to_string()));
        let content = "# What is this?\n# This is bad.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // With custom punctuation, legitimate punctuation exceptions don't apply
        assert_eq!(result.len(), 2, "Both should be flagged with custom punctuation");
    }

    #[test]
    fn test_default_legitimate_question() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# What is Rust?\n# How does it work?\n# Is it fast?";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Question marks in questions should be allowed by default"
        );
    }

    #[test]
    fn test_legitimate_colons() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# FAQ:\n# API Reference:\n# Step 1:\n# Version 2.0:";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Colons in categorical headings should be allowed by default"
        );
    }

    #[test]
    fn test_fix_atx_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n## Subtitle,\n### Sub-subtitle;";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n## Subtitle\n### Sub-subtitle");
    }

    #[test]
    fn test_fix_setext_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "Title.\n======\n\nSubtitle,\n---------";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Title\n======\n\nSubtitle\n---------");
    }

    #[test]
    fn test_fix_preserves_trailing_hashes() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title. #\n## Subtitle, ##";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title #\n## Subtitle ##");
    }

    #[test]
    fn test_indented_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "   # Title.\n  ## Subtitle.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Indented headings (< 4 spaces) should be checked");
    }

    #[test]
    fn test_deeply_indented_ignored() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "    # This is code.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Deeply indented lines (4+ spaces) should be ignored");
    }

    #[test]
    fn test_multiple_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title...";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 8); // Points to first period
    }

    #[test]
    fn test_empty_content() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "This is just text.\nMore text with punctuation.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Non-heading lines should not be checked");
    }

    #[test]
    fn test_get_punctuation_regex() {
        let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
        let regex = rule.get_punctuation_regex().unwrap();
        assert!(regex.is_match("text!"));
        assert!(regex.is_match("text?"));
        assert!(!regex.is_match("text."));
    }

    #[test]
    fn test_regex_caching() {
        let rule1 = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let rule2 = MD026NoTrailingPunctuation::new(Some("!".to_string()));

        // Both should get the same cached regex
        let _regex1 = rule1.get_punctuation_regex().unwrap();
        let _regex2 = rule2.get_punctuation_regex().unwrap();

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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Custom punctuation from config should be used");
    }

    #[test]
    fn test_fix_removes_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.   \n## Subtitle,  ";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // The current implementation doesn't preserve trailing whitespace after punctuation removal
        assert_eq!(fixed, "# Title\n## Subtitle");
    }

    #[test]
    fn test_final_newline_preservation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n");

        let content_no_newline = "# Title.";
        let ctx2 = LintContext::new(content_no_newline);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "# Title");
    }
}
