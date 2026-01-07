use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_heading_range;
use serde::{Deserialize, Serialize};

/// Configuration for MD043 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD043Config {
    /// Required heading patterns
    #[serde(default = "default_headings")]
    pub headings: Vec<String>,
    /// Case-sensitive matching (default: false)
    #[serde(default = "default_match_case")]
    pub match_case: bool,
}

impl Default for MD043Config {
    fn default() -> Self {
        Self {
            headings: default_headings(),
            match_case: default_match_case(),
        }
    }
}

fn default_headings() -> Vec<String> {
    Vec::new()
}

fn default_match_case() -> bool {
    false
}

impl RuleConfig for MD043Config {
    const RULE_NAME: &'static str = "MD043";
}

/// Rule MD043: Required headings present
///
/// See [docs/md043.md](../../docs/md043.md) for full documentation, configuration, and examples.
#[derive(Clone, Default)]
pub struct MD043RequiredHeadings {
    config: MD043Config,
}

impl MD043RequiredHeadings {
    pub fn new(headings: Vec<String>) -> Self {
        Self {
            config: MD043Config {
                headings,
                match_case: default_match_case(),
            },
        }
    }

    /// Create a new instance with the given configuration
    pub fn from_config_struct(config: MD043Config) -> Self {
        Self { config }
    }

    /// Compare two headings based on the match_case configuration
    fn headings_match(&self, expected: &str, actual: &str) -> bool {
        if self.config.match_case {
            expected == actual
        } else {
            expected.to_lowercase() == actual.to_lowercase()
        }
    }

    fn extract_headings(&self, ctx: &crate::lint_context::LintContext) -> Vec<String> {
        let mut result = Vec::new();

        for line_info in &ctx.lines {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                // Reconstruct the full heading format with the hash symbols
                let full_heading = format!("{} {}", heading.marker, heading.text.trim());
                result.push(full_heading);
            }
        }

        result
    }

    /// Match headings against patterns with wildcard support
    ///
    /// Wildcards:
    /// - `*` - Zero or more unspecified headings
    /// - `+` - One or more unspecified headings
    /// - `?` - Exactly one unspecified heading
    ///
    /// Returns (matched, expected_index, actual_index) indicating whether
    /// all patterns were satisfied and the final positions in both sequences.
    fn match_headings_with_wildcards(
        &self,
        actual_headings: &[String],
        expected_patterns: &[String],
    ) -> (bool, usize, usize) {
        let mut exp_idx = 0;
        let mut act_idx = 0;
        let mut match_any = false; // Flexible matching mode for * and +

        while exp_idx < expected_patterns.len() && act_idx < actual_headings.len() {
            let pattern = &expected_patterns[exp_idx];

            if pattern == "*" {
                // Zero or more headings: peek ahead to next required pattern
                exp_idx += 1;
                if exp_idx >= expected_patterns.len() {
                    // * at end means rest of headings are allowed
                    return (true, exp_idx, actual_headings.len());
                }
                // Enable flexible matching until we find next required pattern
                match_any = true;
                continue;
            } else if pattern == "+" {
                // One or more headings: consume at least one
                if act_idx >= actual_headings.len() {
                    return (false, exp_idx, act_idx); // Need at least one heading
                }
                act_idx += 1;
                exp_idx += 1;
                // Enable flexible matching for remaining headings
                match_any = true;
                // If + is at the end, consume all remaining headings
                if exp_idx >= expected_patterns.len() {
                    return (true, exp_idx, actual_headings.len());
                }
                continue;
            } else if pattern == "?" {
                // Exactly one unspecified heading
                act_idx += 1;
                exp_idx += 1;
                match_any = false;
                continue;
            }

            // Literal pattern matching
            let actual = &actual_headings[act_idx];
            if self.headings_match(pattern, actual) {
                // Exact match found
                act_idx += 1;
                exp_idx += 1;
                match_any = false;
            } else if match_any {
                // In flexible mode, try next heading
                act_idx += 1;
            } else {
                // No match and not in flexible mode
                return (false, exp_idx, act_idx);
            }
        }

        // Handle remaining patterns
        while exp_idx < expected_patterns.len() {
            let pattern = &expected_patterns[exp_idx];
            if pattern == "*" {
                // * allows zero headings, continue
                exp_idx += 1;
            } else if pattern == "+" {
                // + requires at least one heading but we're out of headings
                return (false, exp_idx, act_idx);
            } else if pattern == "?" {
                // ? requires exactly one heading but we're out
                return (false, exp_idx, act_idx);
            } else {
                // Literal pattern not satisfied
                return (false, exp_idx, act_idx);
            }
        }

        // Check if we consumed all actual headings
        let all_matched = act_idx == actual_headings.len() && exp_idx == expected_patterns.len();
        (all_matched, exp_idx, act_idx)
    }

    fn is_heading(&self, line_index: usize, ctx: &crate::lint_context::LintContext) -> bool {
        if line_index < ctx.lines.len() {
            ctx.lines[line_index].heading.is_some()
        } else {
            false
        }
    }
}

impl Rule for MD043RequiredHeadings {
    fn name(&self) -> &'static str {
        "MD043"
    }

    fn description(&self) -> &'static str {
        "Required heading structure"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let actual_headings = self.extract_headings(ctx);

        // If no required headings are specified, the rule is disabled
        if self.config.headings.is_empty() {
            return Ok(warnings);
        }

        // Check if all patterns are only * wildcards (which allow zero headings)
        let all_optional_wildcards = self.config.headings.iter().all(|p| p == "*");
        if actual_headings.is_empty() && all_optional_wildcards {
            // Allow empty documents when only * wildcards are specified
            // (? and + require at least some headings)
            return Ok(warnings);
        }

        // Use wildcard matching for pattern support
        let (headings_match, _exp_idx, _act_idx) =
            self.match_headings_with_wildcards(&actual_headings, &self.config.headings);

        if !headings_match {
            // If no headings found but we have required headings, create a warning
            if actual_headings.is_empty() && !self.config.headings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 2,
                    message: format!("Required headings not found: {:?}", self.config.headings),
                    severity: Severity::Warning,
                    fix: None,
                });
                return Ok(warnings);
            }

            // Create warnings for each heading that doesn't match
            for (i, line_info) in ctx.lines.iter().enumerate() {
                if self.is_heading(i, ctx) {
                    // Calculate precise character range for the entire heading
                    let (start_line, start_col, end_line, end_col) =
                        calculate_heading_range(i + 1, line_info.content(ctx.content));

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Heading structure does not match the required structure".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }

            // If we have no warnings but headings don't match (could happen if we have no headings),
            // add a warning at the beginning of the file
            if warnings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 2,
                    message: format!(
                        "Heading structure does not match required structure. Expected: {:?}, Found: {:?}",
                        self.config.headings, actual_headings
                    ),
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // If no required headings are specified, return content as is
        if self.config.headings.is_empty() {
            return Ok(content.to_string());
        }

        let actual_headings = self.extract_headings(ctx);

        // Check if headings already match using wildcard support - if so, no fix needed
        let (headings_match, _, _) = self.match_headings_with_wildcards(&actual_headings, &self.config.headings);
        if headings_match {
            return Ok(content.to_string());
        }

        // IMPORTANT: MD043 fixes are inherently risky as they require restructuring the document.
        // Instead of making destructive changes, we should be conservative and only make
        // minimal changes when we're confident about the user's intent.

        // For now, we'll avoid making destructive fixes and preserve the original content.
        // This prevents data loss while still allowing the rule to identify issues.

        // TODO: In the future, this could be enhanced to:
        // 1. Insert missing required headings at appropriate positions
        // 2. Rename existing headings to match requirements (when structure is similar)
        // 3. Provide more granular fixes based on the specific mismatch

        // Return original content unchanged to prevent data loss
        Ok(content.to_string())
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no heading requirements or content is empty
        if self.config.headings.is_empty() || ctx.content.is_empty() {
            return true;
        }

        // Check if any heading exists using cached information
        let has_heading = ctx.lines.iter().any(|line| line.heading.is_some());

        // Don't skip if we have wildcard requirements that need headings (? or +)
        // even when no headings exist, because we need to report the error
        if !has_heading {
            let has_required_wildcards = self.config.headings.iter().any(|p| p == "?" || p == "+");
            if has_required_wildcards {
                return false; // Don't skip - we need to check and report error
            }
        }

        !has_heading
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD043Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD043Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD043Config>(config);
        Box::new(MD043RequiredHeadings::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_extract_headings_code_blocks() {
        // Create rule with required headings (now with hash symbols)
        let required = vec!["# Test Document".to_string(), "## Real heading 2".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Basic content with code block
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## Another heading in code block\n```\n\n## Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings,
            vec!["# Test Document".to_string(), "## Real heading 2".to_string()],
            "Should extract correct headings and ignore code blocks"
        );

        // Test 2: Content with invalid headings
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## This should be ignored\n```\n\n## Not Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings,
            vec!["# Test Document".to_string(), "## Not Real heading 2".to_string()],
            "Should extract actual headings including mismatched ones"
        );
    }

    #[test]
    fn test_with_document_structure() {
        // Test with required headings (now with hash symbols)
        let required = vec![
            "# Introduction".to_string(),
            "# Method".to_string(),
            "# Results".to_string(),
        ];
        let rule = MD043RequiredHeadings::new(required);

        // Test with matching headings
        let content = "# Introduction\n\nContent\n\n# Method\n\nMore content\n\n# Results\n\nFinal content";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching headings");

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings for mismatched headings");

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings when headings are missing");

        // Test with setext headings - use the correct format (marker text)
        let required_setext = vec![
            "=========== Introduction".to_string(),
            "------ Method".to_string(),
            "======= Results".to_string(),
        ];
        let rule_setext = MD043RequiredHeadings::new(required_setext);
        let content = "Introduction\n===========\n\nContent\n\nMethod\n------\n\nMore content\n\nResults\n=======\n\nFinal content";
        let warnings = rule_setext
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching setext headings");
    }

    #[test]
    fn test_should_skip_no_false_positives() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with '#' character in normal text (not a heading)
        let content = "This paragraph contains a # character but is not a heading";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with # in normal text"
        );

        // Test 2: Content with code block containing heading-like syntax
        let content = "Regular paragraph\n\n```markdown\n# This is not a real heading\n```\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with heading-like syntax in code blocks"
        );

        // Test 3: Content with list items using '-' character
        let content = "Some text\n\n- List item 1\n- List item 2\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with list items using dash"
        );

        // Test 4: Content with horizontal rule that uses '---'
        let content = "Some text\n\n---\n\nMore text below the horizontal rule";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with horizontal rule"
        );

        // Test 5: Content with equals sign in normal text
        let content = "This is a normal paragraph with equals sign x = y + z";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with equals sign in normal text"
        );

        // Test 6: Content with dash/minus in normal text
        let content = "This is a normal paragraph with minus sign x - y = z";
        assert!(
            rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should skip content with minus sign in normal text"
        );
    }

    #[test]
    fn test_should_skip_heading_detection() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with ATX heading
        let content = "# This is a heading\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with ATX heading"
        );

        // Test 2: Content with Setext heading (equals sign)
        let content = "This is a heading\n================\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with Setext heading (=)"
        );

        // Test 3: Content with Setext heading (dash)
        let content = "This is a subheading\n------------------\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with Setext heading (-)"
        );

        // Test 4: Content with ATX heading with closing hashes
        let content = "## This is a heading ##\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with ATX heading with closing hashes"
        );
    }

    #[test]
    fn test_config_match_case_sensitive() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with different case
        let content = "# introduction\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should detect case mismatch when match_case is true"
        );
    }

    #[test]
    fn test_config_match_case_insensitive() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with different case
        let content = "# introduction\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should allow case mismatch when match_case is false");
    }

    #[test]
    fn test_config_case_insensitive_mixed() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# METHOD".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with mixed case variations
        let content = "# INTRODUCTION\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should allow mixed case variations when match_case is false"
        );
    }

    #[test]
    fn test_config_case_sensitive_exact_match() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with exact case match
        let content = "# Introduction\n\n# Method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should pass with exact case match when match_case is true"
        );
    }

    #[test]
    fn test_default_config() {
        let rule = MD043RequiredHeadings::default();

        // Should be disabled with empty headings
        let content = "# Any heading\n\n# Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should be disabled with default empty headings");
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD043RequiredHeadings::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD043");

        // Should contain both headings and match_case options with default values
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("headings"));
            assert!(table.contains_key("match-case"));
            assert_eq!(table["headings"], toml::Value::Array(vec![]));
            assert_eq!(table["match-case"], toml::Value::Boolean(false));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_headings_match_case_sensitive() {
        let config = MD043Config {
            headings: vec![],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        assert!(rule.headings_match("Test", "Test"));
        assert!(!rule.headings_match("Test", "test"));
        assert!(!rule.headings_match("test", "Test"));
    }

    #[test]
    fn test_headings_match_case_insensitive() {
        let config = MD043Config {
            headings: vec![],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        assert!(rule.headings_match("Test", "Test"));
        assert!(rule.headings_match("Test", "test"));
        assert!(rule.headings_match("test", "Test"));
        assert!(rule.headings_match("TEST", "test"));
    }

    #[test]
    fn test_config_empty_headings() {
        let config = MD043Config {
            headings: vec![],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should skip processing when no headings are required
        let content = "# Any heading\n\n# Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should be disabled with empty headings list");
    }

    #[test]
    fn test_fix_respects_configuration() {
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "# Content".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "Wrong content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // MD043 now preserves original content to prevent data loss
        let expected = "Wrong content";
        assert_eq!(fixed, expected);
    }

    // Wildcard pattern tests

    #[test]
    fn test_asterisk_wildcard_zero_headings() {
        // * allows zero headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "*".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow zero headings between Start and End");
    }

    #[test]
    fn test_asterisk_wildcard_multiple_headings() {
        // * allows multiple headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "*".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Start\n\n## Section 1\n\n## Section 2\n\n## Section 3\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "* should allow multiple headings between Start and End"
        );
    }

    #[test]
    fn test_asterisk_wildcard_at_end() {
        // * at end allows any remaining headings
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Introduction\n\n## Details\n\n### Subsection\n\n## More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at end should allow any trailing headings");
    }

    #[test]
    fn test_plus_wildcard_requires_at_least_one() {
        // + requires at least one heading
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with zero headings
        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ should require at least one heading");
    }

    #[test]
    fn test_plus_wildcard_allows_multiple() {
        // + allows multiple headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with one heading
        let content = "# Start\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should allow one heading");

        // Should pass with multiple headings
        let content = "# Start\n\n## Middle 1\n\n## Middle 2\n\n## Middle 3\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should allow multiple headings");
    }

    #[test]
    fn test_question_wildcard_exactly_one() {
        // ? requires exactly one heading
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with exactly one heading before Description
        let content = "# Project Name\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? should allow exactly one heading");
    }

    #[test]
    fn test_question_wildcard_fails_with_zero() {
        // ? fails with zero headings
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "? should require exactly one heading");
    }

    #[test]
    fn test_complex_wildcard_pattern() {
        // Complex pattern: variable title, required sections, optional details
        let config = MD043Config {
            headings: vec![
                "?".to_string(),           // Any project title
                "## Overview".to_string(), // Required
                "*".to_string(),           // Optional sections
                "## License".to_string(),  // Required
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with minimal structure
        let content = "# My Project\n\n## Overview\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Complex pattern should match minimal structure");

        // Should pass with additional sections
        let content = "# My Project\n\n## Overview\n\n## Installation\n\n## Usage\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Complex pattern should match with optional sections");
    }

    #[test]
    fn test_multiple_asterisks() {
        // Multiple * wildcards in pattern
        let config = MD043Config {
            headings: vec![
                "# Title".to_string(),
                "*".to_string(),
                "## Middle".to_string(),
                "*".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Title\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Multiple * wildcards should work");

        let content = "# Title\n\n### Details\n\n## Middle\n\n### More Details\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Multiple * wildcards should allow flexible structure"
        );
    }

    #[test]
    fn test_wildcard_with_case_sensitivity() {
        // Wildcards work with case-sensitive matching
        let config = MD043Config {
            headings: vec![
                "?".to_string(),
                "## Description".to_string(), // Case-sensitive
            ],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with correct case
        let content = "# Title\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Wildcard should work with case-sensitive matching");

        // Should fail with wrong case
        let content = "# Title\n\n## description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Case-sensitive matching should detect case mismatch"
        );
    }

    #[test]
    fn test_all_wildcards_pattern() {
        // Pattern with only wildcards
        let config = MD043Config {
            headings: vec!["*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with any headings
        let content = "# Any\n\n## Headings\n\n### Work";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* alone should allow any heading structure");

        // Should pass with no headings
        let content = "No headings here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* alone should allow no headings");
    }

    #[test]
    fn test_wildcard_edge_cases() {
        // Edge case: + at end requires at least one more heading
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with no additional headings
        let content = "# Start";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ at end should require at least one more heading");

        // Should pass with additional headings
        let content = "# Start\n\n## More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ at end should allow additional headings");
    }

    #[test]
    fn test_fix_with_wildcards() {
        // Fix should preserve content when wildcards are used
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Matching content
        let content = "# Project\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content, "Fix should preserve matching wildcard content");

        // Non-matching content
        let content = "# Project\n\n## Other";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed, content,
            "Fix should preserve non-matching content to prevent data loss"
        );
    }

    // Comprehensive edge case tests

    #[test]
    fn test_consecutive_wildcards() {
        // Multiple wildcards in a row
        let config = MD043Config {
            headings: vec![
                "# Start".to_string(),
                "*".to_string(),
                "+".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least one heading from +
        let content = "# Start\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Consecutive * and + should work together");

        // Should fail without the + requirement
        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when + is not satisfied");
    }

    #[test]
    fn test_question_mark_doesnt_consume_literal_match() {
        // ? should match exactly one, not more
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string(), "## License".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should match with exactly one before Description
        let content = "# Title\n\n## Description\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? should consume exactly one heading");

        // Should fail if Description comes first (? needs something to match)
        let content = "## Description\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "? requires exactly one heading to match");
    }

    #[test]
    fn test_asterisk_between_literals_complex() {
        // Test * matching when sandwiched between specific headings
        let config = MD043Config {
            headings: vec![
                "# Title".to_string(),
                "## Section A".to_string(),
                "*".to_string(),
                "## Section B".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should work with zero headings between A and B
        let content = "# Title\n\n## Section A\n\n## Section B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow zero headings");

        // Should work with many headings between A and B
        let content = "# Title\n\n## Section A\n\n### Sub1\n\n### Sub2\n\n### Sub3\n\n## Section B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow multiple headings");

        // Should fail if Section B is missing
        let content = "# Title\n\n## Section A\n\n### Sub1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail when required heading after * is missing"
        );
    }

    #[test]
    fn test_plus_requires_consumption() {
        // + must consume at least one heading
        let config = MD043Config {
            headings: vec!["+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with no headings
        let content = "No headings here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ should fail with zero headings");

        // Should pass with any heading
        let content = "# Any heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should pass with one heading");

        // Should pass with multiple headings
        let content = "# First\n\n## Second\n\n### Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should pass with multiple headings");
    }

    #[test]
    fn test_mixed_wildcard_and_literal_ordering() {
        // Ensure wildcards don't break literal matching order
        let config = MD043Config {
            headings: vec![
                "# A".to_string(),
                "*".to_string(),
                "# B".to_string(),
                "*".to_string(),
                "# C".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass in correct order
        let content = "# A\n\n# B\n\n# C";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should match literals in correct order");

        // Should fail in wrong order
        let content = "# A\n\n# C\n\n# B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when literals are out of order");

        // Should fail with missing required literal
        let content = "# A\n\n# C";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when required literal is missing");
    }

    #[test]
    fn test_only_wildcards_with_headings() {
        // Pattern with only wildcards and content
        let config = MD043Config {
            headings: vec!["?".to_string(), "+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least 2 headings (? = 1, + = 1+)
        let content = "# First\n\n## Second";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? followed by + should require at least 2 headings");

        // Should fail with only one heading
        let content = "# First";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail with only 1 heading when ? + is required"
        );
    }

    #[test]
    fn test_asterisk_matching_algorithm_greedy_vs_lazy() {
        // Test that * correctly finds the next literal match
        let config = MD043Config {
            headings: vec![
                "# Start".to_string(),
                "*".to_string(),
                "## Target".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should correctly skip to first "Target" match
        let content = "# Start\n\n## Other\n\n## Target\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should correctly skip to next literal match");

        // Should handle case where there are extra headings after the match
        // (First Target matches, second Target is extra - should fail)
        let content = "# Start\n\n## Target\n\n## Target\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail with extra headings that don't match pattern"
        );
    }

    #[test]
    fn test_wildcard_at_start() {
        // Test wildcards at the beginning of pattern
        let config = MD043Config {
            headings: vec!["*".to_string(), "## End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should allow any headings before End
        let content = "# Random\n\n## Stuff\n\n## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at start should allow any preceding headings");

        // Test + at start
        let config = MD043Config {
            headings: vec!["+".to_string(), "## End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least one heading before End
        let content = "## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ at start should require at least one heading");

        let content = "# First\n\n## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ at start should allow headings before End");
    }

    #[test]
    fn test_wildcard_with_setext_headings() {
        // Ensure wildcards work with setext headings too
        let config = MD043Config {
            headings: vec!["?".to_string(), "====== Section".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "Title\n=====\n\nSection\n======\n\nOptional\n--------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Wildcards should work with setext headings");
    }

    #[test]
    fn test_empty_document_with_required_wildcards() {
        // Empty document should fail when + or ? are required
        let config = MD043Config {
            headings: vec!["?".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "No headings";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Empty document should fail with ? requirement");

        // Test with +
        let config = MD043Config {
            headings: vec!["+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "No headings";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Empty document should fail with + requirement");
    }

    #[test]
    fn test_trailing_headings_after_pattern_completion() {
        // Extra headings after pattern is satisfied should fail
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "## Section".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with extra headings
        let content = "# Title\n\n## Section\n\n### Extra";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail with trailing headings beyond pattern");

        // But * at end should allow them
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "## Section".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Title\n\n## Section\n\n### Extra";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at end should allow trailing headings");
    }
}
