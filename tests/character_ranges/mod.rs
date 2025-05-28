//! Systematic Character Range Testing Framework
//!
//! This module provides comprehensive testing infrastructure for validating
//! character ranges across all rumdl rules. It ensures precise highlighting,
//! prevents regressions, and serves as living documentation of expected behavior.

pub mod additional_tests;
pub mod basic_tests;
pub mod comprehensive_tests;
pub mod extended_tests;
pub mod unicode_utils;

use rumdl::lint_context::LintContext;
use rumdl::rule::{LintWarning, Rule};
use rumdl::rules::heading_utils::HeadingStyle;
use rumdl::rules::md004_unordered_list_style::UnorderedListStyle;
use rumdl::rules::*;

/// Represents a single character range test case
#[derive(Debug, Clone)]
pub struct CharacterRangeTest {
    /// The rule name (e.g., "MD001")
    pub rule_name: &'static str,
    /// The markdown content to test
    pub content: &'static str,
    /// Expected warnings with precise character ranges
    pub expected_warnings: Vec<ExpectedWarning>,
}

/// Represents an expected warning with precise character range information
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedWarning {
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// End line number (1-indexed)
    pub end_line: usize,
    /// End column number (1-indexed)
    pub end_column: usize,
    /// The exact text that should be highlighted
    pub highlighted_text: &'static str,
    /// Optional message pattern to match
    pub message_pattern: Option<&'static str>,
}

impl ExpectedWarning {
    /// Create a new expected warning with basic range information
    pub fn new(
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        highlighted_text: &'static str,
    ) -> Self {
        Self {
            line,
            column,
            end_line,
            end_column,
            highlighted_text,
            message_pattern: None,
        }
    }
}

/// Generic test runner for character range validation
pub fn test_character_ranges(test: CharacterRangeTest) {
    // Create the rule instance
    let rule = create_rule_by_name(test.rule_name)
        .unwrap_or_else(|| panic!("Unknown rule: {}", test.rule_name));

    // Run the rule check
    let ctx = LintContext::new(test.content);
    let warnings = rule
        .check(&ctx)
        .unwrap_or_else(|e| panic!("Rule {} failed to check content: {}", test.rule_name, e));

    // Validate warning count
    assert_eq!(
        warnings.len(),
        test.expected_warnings.len(),
        "Rule {} produced {} warnings, expected {}\nContent: {:?}\nActual warnings: {:#?}",
        test.rule_name,
        warnings.len(),
        test.expected_warnings.len(),
        test.content,
        warnings
    );

    // Validate each warning
    for (i, (actual, expected)) in warnings
        .iter()
        .zip(test.expected_warnings.iter())
        .enumerate()
    {
        validate_warning(test.rule_name, test.content, i, actual, expected);
    }
}

/// Validate a single warning against expected values
fn validate_warning(
    rule_name: &str,
    content: &str,
    warning_index: usize,
    actual: &LintWarning,
    expected: &ExpectedWarning,
) {
    // Validate line numbers
    assert_eq!(
        actual.line, expected.line,
        "Rule {} warning #{}: line mismatch. Expected {}, got {}",
        rule_name, warning_index, expected.line, actual.line
    );

    // Validate column numbers
    assert_eq!(
        actual.column, expected.column,
        "Rule {} warning #{}: column mismatch. Expected {}, got {}",
        rule_name, warning_index, expected.column, actual.column
    );

    // Validate end line numbers
    assert_eq!(
        actual.end_line, expected.end_line,
        "Rule {} warning #{}: end_line mismatch. Expected {}, got {}",
        rule_name, warning_index, expected.end_line, actual.end_line
    );

    // Validate end column numbers
    assert_eq!(
        actual.end_column, expected.end_column,
        "Rule {} warning #{}: end_column mismatch. Expected {}, got {}",
        rule_name, warning_index, expected.end_column, actual.end_column
    );

    // Validate highlighted text
    let highlighted = extract_highlighted_text(content, actual);
    assert_eq!(
        highlighted, expected.highlighted_text,
        "Rule {} warning #{}: highlighted text mismatch.\nExpected: {:?}\nActual: {:?}\nContent: {:?}",
        rule_name, warning_index, expected.highlighted_text, highlighted, content
    );

    // Validate message pattern if specified
    if let Some(pattern) = expected.message_pattern {
        assert!(
            actual.message.contains(pattern),
            "Rule {} warning #{}: message doesn't contain pattern {:?}. Actual message: {:?}",
            rule_name,
            warning_index,
            pattern,
            actual.message
        );
    }
}

/// Extract the highlighted text from content based on warning character range
pub fn extract_highlighted_text(content: &str, warning: &LintWarning) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Handle single-line ranges
    if warning.line == warning.end_line {
        if let Some(line) = lines.get(warning.line - 1) {
            let start_idx = (warning.column - 1).min(line.len());
            let end_idx = (warning.end_column - 1).min(line.len());
            return line
                .chars()
                .skip(start_idx)
                .take(end_idx - start_idx)
                .collect();
        }
    } else {
        // Handle multi-line ranges
        let mut result = String::new();

        for line_num in warning.line..=warning.end_line {
            if let Some(line) = lines.get(line_num - 1) {
                if line_num == warning.line {
                    // First line: from start column to end of line
                    let start_idx = (warning.column - 1).min(line.len());
                    result.push_str(&line.chars().skip(start_idx).collect::<String>());
                } else if line_num == warning.end_line {
                    // Last line: from start of line to end column
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    let end_idx = (warning.end_column - 1).min(line.len());
                    result.push_str(&line.chars().take(end_idx).collect::<String>());
                } else {
                    // Middle lines: entire line
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(line);
                }
            }
        }

        return result;
    }

    String::new()
}

/// Create a rule instance by name for dynamic testing
pub fn create_rule_by_name(rule_name: &str) -> Option<Box<dyn Rule>> {
    match rule_name {
        "MD001" => Some(Box::new(MD001HeadingIncrement)),
        "MD002" => Some(Box::new(MD002FirstHeadingH1::new(1))),
        "MD003" => Some(Box::new(MD003HeadingStyle::new(HeadingStyle::Consistent))),
        "MD004" => Some(Box::new(MD004UnorderedListStyle::new(
            UnorderedListStyle::Consistent,
        ))),
        "MD005" => Some(Box::new(MD005ListIndent)),
        "MD006" => Some(Box::new(MD006StartBullets)),
        "MD007" => Some(Box::new(MD007ULIndent::new(2))),
        "MD009" => Some(Box::new(MD009TrailingSpaces::new(2, false))),
        "MD010" => Some(Box::new(MD010NoHardTabs::new(4, false))),
        "MD011" => Some(Box::new(MD011NoReversedLinks)),
        "MD012" => Some(Box::new(MD012NoMultipleBlanks::new(1))),
        "MD013" => Some(Box::new(MD013LineLength::new(80, true, true, true, false))),
        "MD014" => Some(Box::new(MD014CommandsShowOutput { show_output: true })),
        "MD018" => Some(Box::new(MD018NoMissingSpaceAtx)),
        "MD019" => Some(Box::new(MD019NoMultipleSpaceAtx)),
        "MD020" => Some(Box::new(MD020NoMissingSpaceClosedAtx)),
        "MD021" => Some(Box::new(MD021NoMultipleSpaceClosedAtx)),
        "MD022" => Some(Box::new(MD022BlanksAroundHeadings::new())),
        "MD023" => Some(Box::new(MD023HeadingStartLeft)),
        "MD025" => Some(Box::new(MD025SingleTitle::new(1, ""))),
        "MD026" => Some(Box::new(MD026NoTrailingPunctuation::new(Some(
            ".,;:!?".to_string(),
        )))),
        "MD027" => Some(Box::new(MD027MultipleSpacesBlockquote)),
        "MD028" => Some(Box::new(MD028NoBlanksBlockquote)),
        "MD030" => Some(Box::new(MD030ListMarkerSpace::new(1, 1, 1, 1))),
        "MD031" => Some(Box::new(MD031BlanksAroundFences)),
        "MD032" => Some(Box::new(MD032BlanksAroundLists)),
        "MD033" => Some(Box::new(MD033NoInlineHtml::new())),
        "MD034" => Some(Box::new(MD034NoBareUrls)),
        "MD035" => Some(Box::new(MD035HRStyle::new("consistent".to_string()))),
        "MD036" => Some(Box::new(MD036NoEmphasisAsHeading::new(
            ".,;:!?".to_string(),
        ))),
        "MD037" => Some(Box::new(MD037NoSpaceInEmphasis)),
        "MD038" => Some(Box::new(MD038NoSpaceInCode { enabled: true })),
        "MD039" => Some(Box::new(MD039NoSpaceInLinks)),
        "MD040" => Some(Box::new(MD040FencedCodeLanguage)),
        "MD041" => Some(Box::new(MD041FirstLineHeading::new(1, false))),
        "MD042" => Some(Box::new(MD042NoEmptyLinks)),
        "MD043" => Some(Box::new(MD043RequiredHeadings::new(vec![]))),
        "MD044" => Some(Box::new(MD044ProperNames::new(vec![], false))),
        "MD045" => Some(Box::new(MD045NoAltText)),
        "MD047" => Some(Box::new(MD047SingleTrailingNewline)),
        "MD051" => Some(Box::new(MD051LinkFragments)),
        "MD053" => Some(Box::new(MD053LinkImageReferenceDefinitions {})),
        _ => None,
    }
}

/// Utility function to create a simple test case
pub fn simple_test(
    rule_name: &'static str,
    content: &'static str,
    expected: ExpectedWarning,
) -> CharacterRangeTest {
    CharacterRangeTest {
        rule_name,
        content,
        expected_warnings: vec![expected],
    }
}

/// Utility function to create a test case with multiple warnings
pub fn multi_warning_test(
    rule_name: &'static str,
    content: &'static str,
    expected: Vec<ExpectedWarning>,
) -> CharacterRangeTest {
    CharacterRangeTest {
        rule_name,
        content,
        expected_warnings: expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_highlighted_text_single_line() {
        let content = "This is a test line";
        let warning = LintWarning {
            rule_name: Some("TEST"),
            line: 1,
            column: 6,
            end_line: 1,
            end_column: 8,
            message: "test".to_string(),
            severity: rumdl::rule::Severity::Warning,
            fix: None,
        };

        let highlighted = extract_highlighted_text(content, &warning);
        assert_eq!(highlighted, "is");
    }

    #[test]
    fn test_extract_highlighted_text_multi_line() {
        let content = "Line 1\nLine 2\nLine 3";
        let warning = LintWarning {
            rule_name: Some("TEST"),
            line: 1,
            column: 6,
            end_line: 2,
            end_column: 5,
            message: "test".to_string(),
            severity: rumdl::rule::Severity::Warning,
            fix: None,
        };

        let highlighted = extract_highlighted_text(content, &warning);
        assert_eq!(highlighted, "1\nLine"); // Fixed expectation
    }

    #[test]
    fn test_create_rule_by_name() {
        assert!(create_rule_by_name("MD001").is_some());
        assert!(create_rule_by_name("MD018").is_some());
        assert!(create_rule_by_name("INVALID").is_none());
    }
}
