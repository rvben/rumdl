use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_heading_range;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

lazy_static! {
    // Pattern for ATX headings
    static ref ATX_HEADING: Regex = Regex::new(r"^(#+)\s+(.+)$").unwrap();
    // Pattern for setext heading underlines
    static ref SETEXT_UNDERLINE: Regex = Regex::new(r"^([=-]+)$").unwrap();
}

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
                // Reconstruct the full heading format with the hash symbols
                let full_heading = format!("{} {}", heading.marker, heading.text.trim());
                result.push(full_heading);
            }
        }

        result
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

        // Check if headings match based on case sensitivity configuration
        let headings_match = if actual_headings.len() != self.config.headings.len() {
            false
        } else {
            actual_headings
                .iter()
                .zip(self.config.headings.iter())
                .all(|(actual, expected)| self.headings_match(expected, actual))
        };

        if !headings_match {
            // If no headings found but we have required headings, create a warning
            if actual_headings.is_empty() && !self.config.headings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
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
                    let (start_line, start_col, end_line, end_col) = calculate_heading_range(i + 1, &line_info.content);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Heading structure does not match the required structure".to_string(),
                        severity: Severity::Warning,
                        fix: None, // Cannot automatically fix as we don't know the intended structure
                    });
                }
            }

            // If we have no warnings but headings don't match (could happen if we have no headings),
            // add a warning at the beginning of the file
            if warnings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
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

        let mut result = String::new();

        // Add required headings
        for (idx, heading) in self.config.headings.iter().enumerate() {
            if idx > 0 {
                result.push_str("\n\n");
            }
            result.push_str(heading);
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
        // Just use the regular check method which now uses cached headings
        self.check(_ctx)
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

impl DocumentStructureExtensions for MD043RequiredHeadings {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.heading_lines.is_empty() || !self.config.headings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_extract_headings_code_blocks() {
        // Create rule with required headings (now with hash symbols)
        let required = vec!["# Test Document".to_string(), "## Real heading 2".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Basic content with code block
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## Another heading in code block\n```\n\n## Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings,
            vec!["# Test Document".to_string(), "## Real heading 2".to_string()],
            "Should extract correct headings and ignore code blocks"
        );

        // Test 2: Content with invalid headings
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## This should be ignored\n```\n\n## Not Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content);
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
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching headings");

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings for mismatched headings");

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
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
        let structure = document_structure_from_str(content);
        let warnings = rule_setext
            .check_with_structure(&LintContext::new(content), &structure)
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
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with # in normal text"
        );

        // Test 2: Content with code block containing heading-like syntax
        let content = "Regular paragraph\n\n```markdown\n# This is not a real heading\n```\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with heading-like syntax in code blocks"
        );

        // Test 3: Content with list items using '-' character
        let content = "Some text\n\n- List item 1\n- List item 2\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with list items using dash"
        );

        // Test 4: Content with horizontal rule that uses '---'
        let content = "Some text\n\n---\n\nMore text below the horizontal rule";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with horizontal rule"
        );

        // Test 5: Content with equals sign in normal text
        let content = "This is a normal paragraph with equals sign x = y + z";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with equals sign in normal text"
        );

        // Test 6: Content with dash/minus in normal text
        let content = "This is a normal paragraph with minus sign x - y = z";
        assert!(
            rule.should_skip(&LintContext::new(content)),
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
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with ATX heading"
        );

        // Test 2: Content with Setext heading (equals sign)
        let content = "This is a heading\n================\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with Setext heading (=)"
        );

        // Test 3: Content with Setext heading (dash)
        let content = "This is a subheading\n------------------\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with Setext heading (-)"
        );

        // Test 4: Content with ATX heading with closing hashes
        let content = "## This is a heading ##\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "# Title\n\n# Content";
        assert_eq!(fixed, expected);
    }
}
