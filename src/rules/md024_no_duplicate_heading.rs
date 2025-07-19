use toml;

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_match_range;
use std::collections::{HashMap, HashSet};

mod md024_config;
use md024_config::MD024Config;

#[derive(Clone, Debug, Default)]
pub struct MD024NoDuplicateHeading {
    config: MD024Config,
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

        // For siblings_only mode, track heading hierarchy
        let mut current_section_path: Vec<(u8, String)> = Vec::new(); // Stack of (level, heading_text)
        let mut seen_siblings: HashMap<String, HashSet<String>> = HashMap::new(); // parent_path -> set of child headings

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
                    // Update the section path based on the current heading level
                    while !current_section_path.is_empty() && current_section_path.last().unwrap().0 >= level {
                        current_section_path.pop();
                    }

                    // Build parent path for sibling detection
                    let parent_path = current_section_path
                        .iter()
                        .map(|(_, text)| text.as_str())
                        .collect::<Vec<_>>()
                        .join("/");

                    // Check if this heading is a duplicate among its siblings
                    let siblings = seen_siblings.entry(parent_path.clone()).or_default();
                    if siblings.contains(&heading_key) {
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
                        siblings.insert(heading_key.clone());
                    }

                    // Add current heading to the section path
                    current_section_path.push((level, heading_key.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn run_test(content: &str, config: MD024Config) -> LintResult {
        let ctx = LintContext::new(content);
        let rule = MD024NoDuplicateHeading::from_config_struct(config);
        rule.check(&ctx)
    }

    fn run_fix_test(content: &str, config: MD024Config) -> Result<String, LintError> {
        let ctx = LintContext::new(content);
        let rule = MD024NoDuplicateHeading::from_config_struct(config);
        rule.fix(&ctx)
    }

    #[test]
    fn test_no_duplicate_headings() {
        let content = r#"# First Heading

Some content here.

## Second Heading

More content.

### Third Heading

Even more content.

## Fourth Heading

Final content."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_duplicate_headings_same_level() {
        let content = r#"# First Heading

Some content here.

## Second Heading

More content.

## Second Heading

This is a duplicate."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Second Heading'.");
        assert_eq!(warnings[0].line, 9);
    }

    #[test]
    fn test_duplicate_headings_different_levels_default() {
        let content = r#"# Main Title

Some content.

## Main Title

This has the same text but different level."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Main Title'.");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_duplicate_headings_different_levels_allow_different_nesting() {
        let content = r#"# Main Title

Some content.

## Main Title

This has the same text but different level."#;

        let config = MD024Config {
            allow_different_nesting: true,
            siblings_only: false,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_case_sensitivity() {
        let content = r#"# First Heading

Some content.

## first heading

Different case.

### FIRST HEADING

All caps."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // The rule is case-sensitive, so these should not be duplicates
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_headings_with_trailing_punctuation() {
        let content = r#"# First Heading!

Some content.

## First Heading!

Same with punctuation.

### First Heading

Without punctuation."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'First Heading!'.");
    }

    #[test]
    fn test_headings_with_inline_formatting() {
        let content = r#"# **Bold Heading**

Some content.

## *Italic Heading*

More content.

### **Bold Heading**

Duplicate with same formatting.

#### `Code Heading`

Code formatted.

##### `Code Heading`

Duplicate code formatted."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Duplicate heading: '**Bold Heading**'.");
        assert_eq!(warnings[1].message, "Duplicate heading: '`Code Heading`'.");
    }

    #[test]
    fn test_headings_in_different_sections() {
        let content = r#"# Section One

## Subsection

Some content.

# Section Two

## Subsection

Same subsection name in different section."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Subsection'.");
        assert_eq!(warnings[0].line, 9);
    }

    #[test]
    fn test_multiple_duplicates() {
        let content = r#"# Title

## Subtitle

### Title

#### Subtitle

## Title

### Subtitle"#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 4);
        // First duplicate of "Title"
        assert_eq!(warnings[0].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[0].line, 5);
        // First duplicate of "Subtitle"
        assert_eq!(warnings[1].message, "Duplicate heading: 'Subtitle'.");
        assert_eq!(warnings[1].line, 7);
        // Second duplicate of "Title"
        assert_eq!(warnings[2].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[2].line, 9);
        // Second duplicate of "Subtitle"
        assert_eq!(warnings[3].message, "Duplicate heading: 'Subtitle'.");
        assert_eq!(warnings[3].line, 11);
    }

    #[test]
    fn test_empty_headings() {
        let content = r#"#

Some content.

##

More content.

### Non-empty

####

Another empty."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // Empty headings are skipped
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_unicode_and_special_characters() {
        let content = r#"# 你好世界

Some content.

## Émojis 🎉🎊

More content.

### 你好世界

Duplicate Chinese.

#### Émojis 🎉🎊

Duplicate emojis.

##### Special <chars> & symbols!

###### Special <chars> & symbols!

Duplicate special chars."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 3);
        assert_eq!(warnings[0].message, "Duplicate heading: '你好世界'.");
        assert_eq!(warnings[1].message, "Duplicate heading: 'Émojis 🎉🎊'.");
        assert_eq!(warnings[2].message, "Duplicate heading: 'Special <chars> & symbols!'.");
    }

    #[test]
    fn test_allow_different_nesting_with_same_level_duplicates() {
        let content = r#"# Section One

## Title

### Subsection

## Title

This is a duplicate at the same level.

# Section Two

## Title

Different section, but still a duplicate when allow_different_nesting is true."#;

        let config = MD024Config {
            allow_different_nesting: true,
            siblings_only: false,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[0].line, 7);
        assert_eq!(warnings[1].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[1].line, 13);
    }

    #[test]
    fn test_atx_style_headings_with_closing_hashes() {
        let content = r#"# Heading One #

Some content.

## Heading Two ##

More content.

### Heading One ###

Duplicate with different style."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // The heading text excludes the closing hashes, so "Heading One" is a duplicate
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Heading One'.");
        assert_eq!(warnings[0].line, 9);
    }

    #[test]
    fn test_fix_method_returns_unchanged() {
        let content = r#"# Duplicate

## Duplicate

This has duplicates."#;

        let config = MD024Config::default();
        let result = run_fix_test(content, config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_empty_content() {
        let content = "";
        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_no_headings() {
        let content = r#"This is just regular text.

No headings anywhere.

Just paragraphs."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_whitespace_differences() {
        let content = r#"# Heading with spaces

Some content.

##  Heading with spaces

Different amount of spaces.

### Heading with spaces

Exact match."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // The heading text is trimmed, so all three are duplicates
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Heading with spaces'.");
        assert_eq!(warnings[0].line, 5);
        assert_eq!(warnings[1].message, "Duplicate heading: 'Heading with spaces'.");
        assert_eq!(warnings[1].line, 9);
    }

    #[test]
    fn test_column_positions() {
        let content = r#"# First

## Second

### First"#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 5);
        assert_eq!(warnings[0].column, 5); // After "### "
        assert_eq!(warnings[0].end_line, 5);
        assert_eq!(warnings[0].end_column, 10); // End of "First"
    }

    #[test]
    fn test_complex_nesting_scenario() {
        let content = r#"# Main Document

## Introduction

### Overview

## Implementation

### Overview

This Overview is in a different section.

## Conclusion

### Overview

Another Overview in yet another section."#;

        let config = MD024Config {
            allow_different_nesting: true,
            siblings_only: false,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // When allow_different_nesting is true, only same-level duplicates are flagged
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Overview'.");
        assert_eq!(warnings[0].line, 9);
        assert_eq!(warnings[1].message, "Duplicate heading: 'Overview'.");
        assert_eq!(warnings[1].line, 15);
    }

    #[test]
    fn test_setext_style_headings() {
        let content = r#"Main Title
==========

Some content.

Second Title
------------

More content.

Main Title
==========

Duplicate setext."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Main Title'.");
        assert_eq!(warnings[0].line, 11);
    }

    #[test]
    fn test_mixed_heading_styles() {
        let content = r#"# ATX Title

Some content.

ATX Title
=========

Same text, different style."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'ATX Title'.");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_heading_with_links() {
        let content = r#"# [Link Text](http://example.com)

Some content.

## [Link Text](http://example.com)

Duplicate heading with link.

### [Different Link](http://example.com)

Not a duplicate."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].message,
            "Duplicate heading: '[Link Text](http://example.com)'."
        );
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_consecutive_duplicates() {
        let content = r#"# Title

## Title

### Title

Three in a row."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[1].message, "Duplicate heading: 'Title'.");
        assert_eq!(warnings[1].line, 5);
    }

    #[test]
    fn test_siblings_only_config() {
        let content = r#"# Section One

## Subsection

### Details

# Section Two

## Subsection

Different parent sections, so not siblings - no warning expected."#;

        let config = MD024Config {
            allow_different_nesting: false,
            siblings_only: true,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // With siblings_only, these are not flagged because they're under different parents
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_siblings_only_with_actual_siblings() {
        let content = r#"# Main Section

## First Subsection

### Details

## Second Subsection

### Details

The two 'Details' headings are siblings under different subsections - no warning.

## First Subsection

This 'First Subsection' IS a sibling duplicate."#;

        let config = MD024Config {
            allow_different_nesting: false,
            siblings_only: true,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // Only the duplicate "First Subsection" at the same level should be flagged
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'First Subsection'.");
        assert_eq!(warnings[0].line, 13);
    }

    #[test]
    fn test_code_spans_in_headings() {
        let content = r#"# `code` in heading

Some content.

## `code` in heading

Duplicate with code span."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: '`code` in heading'.");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_very_long_heading() {
        let long_text = "This is a very long heading that goes on and on and on and contains many words to test how the rule handles long headings";
        let content = format!("# {long_text}\n\nSome content.\n\n## {long_text}\n\nDuplicate long heading.");

        let config = MD024Config::default();
        let result = run_test(&content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, format!("Duplicate heading: '{long_text}'."));
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_heading_with_html_entities() {
        let content = r#"# Title &amp; More

Some content.

## Title &amp; More

Duplicate with HTML entity."#;

        let config = MD024Config::default();
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Duplicate heading: 'Title &amp; More'.");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_three_duplicates_different_nesting() {
        let content = r#"# Main

## Main

### Main

#### Main

All same text, different levels."#;

        let config = MD024Config {
            allow_different_nesting: true,
            siblings_only: false,
        };
        let result = run_test(content, config);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        // With allow_different_nesting, there should be no warnings
        assert_eq!(warnings.len(), 0);
    }
}
