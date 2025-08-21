//!
//! This module defines the Rule trait and related types for implementing linting rules in rumdl.
//! Includes rule categories, dynamic dispatch helpers, and inline comment handling for rule enable/disable.

use dyn_clone::DynClone;
use serde::Serialize;
use std::ops::Range;
use thiserror::Error;

// Import document structure
use crate::lint_context::LintContext;
use crate::utils::document_structure::DocumentStructure;

// Import markdown AST for shared parsing
pub use markdown::mdast::Node as MarkdownAst;

// Macro to implement box_clone for Rule implementors
#[macro_export]
macro_rules! impl_rule_clone {
    ($ty:ty) => {
        impl $ty {
            fn box_clone(&self) -> Box<dyn Rule> {
                Box::new(self.clone())
            }
        }
    };
}

#[derive(Debug, Error)]
pub enum LintError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Fix failed: {0}")]
    FixFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parsing error: {0}")]
    ParsingError(String),
}

pub type LintResult = Result<Vec<LintWarning>, LintError>;

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct LintWarning {
    pub message: String,
    pub line: usize,       // 1-indexed start line
    pub column: usize,     // 1-indexed start column
    pub end_line: usize,   // 1-indexed end line
    pub end_column: usize, // 1-indexed end column
    pub severity: Severity,
    pub fix: Option<Fix>,
    pub rule_name: Option<&'static str>,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Fix {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub enum Severity {
    Error,
    Warning,
}

/// Type of rule for selective processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    Heading,
    List,
    CodeBlock,
    Link,
    Image,
    Html,
    Emphasis,
    Whitespace,
    Blockquote,
    Table,
    FrontMatter,
    Other,
}

/// Capability of a rule to fix issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixCapability {
    /// Rule can automatically fix all violations it detects
    FullyFixable,
    /// Rule can fix some violations based on context
    ConditionallyFixable,
    /// Rule cannot fix violations (by design)
    Unfixable,
}

/// Remove marker /// TRAIT_MARKER_V1
pub trait Rule: DynClone + Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &LintContext) -> LintResult;
    fn fix(&self, ctx: &LintContext) -> Result<String, LintError>;

    /// Enhanced check method using document structure
    /// By default, calls the regular check method if not overridden
    fn check_with_structure(&self, ctx: &LintContext, _structure: &DocumentStructure) -> LintResult {
        self.check(ctx)
    }

    /// AST-based check method for rules that can benefit from shared AST parsing
    /// By default, calls the regular check method if not overridden
    fn check_with_ast(&self, ctx: &LintContext, _ast: &MarkdownAst) -> LintResult {
        self.check(ctx)
    }

    /// Combined check method using both document structure and AST
    /// By default, calls the regular check method if not overridden
    fn check_with_structure_and_ast(
        &self,
        ctx: &LintContext,
        _structure: &DocumentStructure,
        _ast: &MarkdownAst,
    ) -> LintResult {
        self.check(ctx)
    }

    /// Check if this rule should quickly skip processing based on content
    fn should_skip(&self, _ctx: &LintContext) -> bool {
        false
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Other // Default implementation returns Other
    }

    /// Check if this rule can benefit from AST parsing
    fn uses_ast(&self) -> bool {
        false
    }

    /// Check if this rule can benefit from document structure
    fn uses_document_structure(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn std::any::Any;

    fn as_maybe_document_structure(&self) -> Option<&dyn MaybeDocumentStructure> {
        None
    }

    fn as_maybe_ast(&self) -> Option<&dyn MaybeAst> {
        None
    }

    /// Returns the rule name and default config table if the rule has config.
    /// If a rule implements this, it MUST be defined on the `impl Rule for ...` block,
    /// not just the inherent impl.
    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        None
    }

    /// Declares the fix capability of this rule
    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable // Safe default for backward compatibility
    }

    /// Factory: create a rule from config (if present), or use defaults.
    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        panic!(
            "from_config not implemented for rule: {}",
            std::any::type_name::<Self>()
        );
    }
}

// Implement the cloning logic for the Rule trait object
dyn_clone::clone_trait_object!(Rule);

/// Extension trait to add downcasting capabilities to Rule
pub trait RuleExt {
    fn downcast_ref<T: 'static>(&self) -> Option<&T>;
}

impl<R: Rule + 'static> RuleExt for Box<R> {
    fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        if std::any::TypeId::of::<R>() == std::any::TypeId::of::<T>() {
            unsafe { Some(&*(self.as_ref() as *const _ as *const T)) }
        } else {
            None
        }
    }
}

/// Check if a rule is disabled at a specific line via inline comments
pub fn is_rule_disabled_at_line(content: &str, rule_name: &str, line_num: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut is_disabled = false;

    // Check for both markdownlint-disable and rumdl-disable comments
    for (i, line) in lines.iter().enumerate() {
        // Stop processing once we reach the target line
        if i > line_num {
            break;
        }

        // Skip comments that are inside code blocks
        if crate::rules::code_block_utils::CodeBlockUtils::is_in_code_block(content, i) {
            continue;
        }

        let line = line.trim();

        // Check for disable comments (both global and rule-specific)
        if let Some(rules) = parse_disable_comment(line)
            && (rules.is_empty() || rules.contains(&rule_name))
        {
            is_disabled = true;
            continue;
        }

        // Check for enable comments (both global and rule-specific)
        if let Some(rules) = parse_enable_comment(line)
            && (rules.is_empty() || rules.contains(&rule_name))
        {
            is_disabled = false;
            continue;
        }
    }

    is_disabled
}

/// Parse a disable comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_comment(line: &str) -> Option<Vec<&str>> {
    // Check for rumdl-disable first (preferred syntax)
    if let Some(start) = line.find("<!-- rumdl-disable") {
        let after_prefix = &line[start + "<!-- rumdl-disable".len()..];

        // Global disable: <!-- rumdl-disable -->
        if after_prefix.trim_start().starts_with("-->") {
            return Some(Vec::new()); // Empty vec means all rules
        }

        // Rule-specific disable: <!-- rumdl-disable MD001 MD002 -->
        if let Some(end) = after_prefix.find("-->") {
            let rules_str = after_prefix[..end].trim();
            if !rules_str.is_empty() {
                let rules: Vec<&str> = rules_str.split_whitespace().collect();
                return Some(rules);
            }
        }
    }

    // Check for markdownlint-disable (compatibility)
    if let Some(start) = line.find("<!-- markdownlint-disable") {
        let after_prefix = &line[start + "<!-- markdownlint-disable".len()..];

        // Global disable: <!-- markdownlint-disable -->
        if after_prefix.trim_start().starts_with("-->") {
            return Some(Vec::new()); // Empty vec means all rules
        }

        // Rule-specific disable: <!-- markdownlint-disable MD001 MD002 -->
        if let Some(end) = after_prefix.find("-->") {
            let rules_str = after_prefix[..end].trim();
            if !rules_str.is_empty() {
                let rules: Vec<&str> = rules_str.split_whitespace().collect();
                return Some(rules);
            }
        }
    }

    None
}

/// Parse an enable comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_comment(line: &str) -> Option<Vec<&str>> {
    // Check for rumdl-enable first (preferred syntax)
    if let Some(start) = line.find("<!-- rumdl-enable") {
        let after_prefix = &line[start + "<!-- rumdl-enable".len()..];

        // Global enable: <!-- rumdl-enable -->
        if after_prefix.trim_start().starts_with("-->") {
            return Some(Vec::new()); // Empty vec means all rules
        }

        // Rule-specific enable: <!-- rumdl-enable MD001 MD002 -->
        if let Some(end) = after_prefix.find("-->") {
            let rules_str = after_prefix[..end].trim();
            if !rules_str.is_empty() {
                let rules: Vec<&str> = rules_str.split_whitespace().collect();
                return Some(rules);
            }
        }
    }

    // Check for markdownlint-enable (compatibility)
    if let Some(start) = line.find("<!-- markdownlint-enable") {
        let after_prefix = &line[start + "<!-- markdownlint-enable".len()..];

        // Global enable: <!-- markdownlint-enable -->
        if after_prefix.trim_start().starts_with("-->") {
            return Some(Vec::new()); // Empty vec means all rules
        }

        // Rule-specific enable: <!-- markdownlint-enable MD001 MD002 -->
        if let Some(end) = after_prefix.find("-->") {
            let rules_str = after_prefix[..end].trim();
            if !rules_str.is_empty() {
                let rules: Vec<&str> = rules_str.split_whitespace().collect();
                return Some(rules);
            }
        }
    }

    None
}

/// Check if a rule is disabled via inline comments in the file content (for backward compatibility)
pub fn is_rule_disabled_by_comment(content: &str, rule_name: &str) -> bool {
    // Check if the rule is disabled at the end of the file
    let lines: Vec<&str> = content.lines().collect();
    is_rule_disabled_at_line(content, rule_name, lines.len())
}

// Helper trait for dynamic dispatch to check_with_structure
pub trait MaybeDocumentStructure {
    fn check_with_structure_opt(
        &self,
        ctx: &LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult>;
}

impl<T> MaybeDocumentStructure for T
where
    T: Rule + crate::utils::document_structure::DocumentStructureExtensions + 'static,
{
    fn check_with_structure_opt(
        &self,
        ctx: &LintContext,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult> {
        Some(self.check_with_structure(ctx, structure))
    }
}

impl MaybeDocumentStructure for dyn Rule {
    fn check_with_structure_opt(
        &self,
        _ctx: &LintContext,
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult> {
        None
    }
}

// Helper trait for dynamic dispatch to check_with_ast
pub trait MaybeAst {
    fn check_with_ast_opt(&self, ctx: &LintContext, ast: &MarkdownAst) -> Option<LintResult>;
}

impl<T> MaybeAst for T
where
    T: Rule + AstExtensions + 'static,
{
    fn check_with_ast_opt(&self, ctx: &LintContext, ast: &MarkdownAst) -> Option<LintResult> {
        if self.has_relevant_ast_elements(ctx, ast) {
            Some(self.check_with_ast(ctx, ast))
        } else {
            None
        }
    }
}

impl MaybeAst for dyn Rule {
    fn check_with_ast_opt(&self, _ctx: &LintContext, _ast: &MarkdownAst) -> Option<LintResult> {
        None
    }
}

/// Extension trait for rules that use AST
pub trait AstExtensions {
    /// Check if the AST contains relevant elements for this rule
    fn has_relevant_ast_elements(&self, ctx: &LintContext, ast: &MarkdownAst) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_disable_comment() {
        // Test rumdl-disable global
        assert_eq!(parse_disable_comment("<!-- rumdl-disable -->"), Some(vec![]));

        // Test rumdl-disable specific rules
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test markdownlint-disable global
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable -->"), Some(vec![]));

        // Test markdownlint-disable specific rules
        assert_eq!(
            parse_disable_comment("<!-- markdownlint-disable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test non-disable comment
        assert_eq!(parse_disable_comment("<!-- some other comment -->"), None);

        // Test with extra whitespace
        assert_eq!(
            parse_disable_comment("  <!-- rumdl-disable MD013 -->  "),
            Some(vec!["MD013"])
        );
    }

    #[test]
    fn test_parse_enable_comment() {
        // Test rumdl-enable global
        assert_eq!(parse_enable_comment("<!-- rumdl-enable -->"), Some(vec![]));

        // Test rumdl-enable specific rules
        assert_eq!(
            parse_enable_comment("<!-- rumdl-enable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test markdownlint-enable global
        assert_eq!(parse_enable_comment("<!-- markdownlint-enable -->"), Some(vec![]));

        // Test markdownlint-enable specific rules
        assert_eq!(
            parse_enable_comment("<!-- markdownlint-enable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test non-enable comment
        assert_eq!(parse_enable_comment("<!-- some other comment -->"), None);
    }

    #[test]
    fn test_is_rule_disabled_at_line() {
        let content = r#"# Test
<!-- rumdl-disable MD013 -->
This is a long line
<!-- rumdl-enable MD013 -->
This is another line
<!-- markdownlint-disable MD042 -->
Empty link: []()
<!-- markdownlint-enable MD042 -->
Final line"#;

        // Test MD013 disabled at line 2 (0-indexed line 1)
        assert!(is_rule_disabled_at_line(content, "MD013", 2));

        // Test MD013 enabled at line 4 (0-indexed line 3)
        assert!(!is_rule_disabled_at_line(content, "MD013", 4));

        // Test MD042 disabled at line 6 (0-indexed line 5)
        assert!(is_rule_disabled_at_line(content, "MD042", 6));

        // Test MD042 enabled at line 8 (0-indexed line 7)
        assert!(!is_rule_disabled_at_line(content, "MD042", 8));

        // Test rule that's never disabled
        assert!(!is_rule_disabled_at_line(content, "MD001", 5));
    }

    #[test]
    fn test_parse_disable_comment_edge_cases() {
        // Test with no space before closing
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-->"), Some(vec![]));

        // Test with multiple spaces - the implementation doesn't handle leading spaces in comment
        assert_eq!(
            parse_disable_comment("<!--   rumdl-disable   MD001   MD002   -->"),
            None
        );

        // Test with tabs
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable\tMD001\tMD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test comment not at start of line
        assert_eq!(
            parse_disable_comment("Some text <!-- rumdl-disable MD001 --> more text"),
            Some(vec!["MD001"])
        );

        // Test malformed comment (no closing)
        assert_eq!(parse_disable_comment("<!-- rumdl-disable MD001"), None);

        // Test malformed comment (no opening)
        assert_eq!(parse_disable_comment("rumdl-disable MD001 -->"), None);

        // Test case sensitivity
        assert_eq!(parse_disable_comment("<!-- RUMDL-DISABLE -->"), None);
        assert_eq!(parse_disable_comment("<!-- RuMdL-DiSaBlE -->"), None);

        // Test with newlines - implementation finds the comment
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable\nMD001 -->"),
            Some(vec!["MD001"])
        );

        // Test empty rule list
        assert_eq!(parse_disable_comment("<!-- rumdl-disable   -->"), Some(vec![]));

        // Test duplicate rules
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 MD001 MD002 -->"),
            Some(vec!["MD001", "MD001", "MD002"])
        );
    }

    #[test]
    fn test_parse_enable_comment_edge_cases() {
        // Test with no space before closing
        assert_eq!(parse_enable_comment("<!-- rumdl-enable-->"), Some(vec![]));

        // Test with multiple spaces - the implementation doesn't handle leading spaces in comment
        assert_eq!(parse_enable_comment("<!--   rumdl-enable   MD001   MD002   -->"), None);

        // Test with tabs
        assert_eq!(
            parse_enable_comment("<!-- rumdl-enable\tMD001\tMD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test comment not at start of line
        assert_eq!(
            parse_enable_comment("Some text <!-- rumdl-enable MD001 --> more text"),
            Some(vec!["MD001"])
        );

        // Test malformed comment (no closing)
        assert_eq!(parse_enable_comment("<!-- rumdl-enable MD001"), None);

        // Test malformed comment (no opening)
        assert_eq!(parse_enable_comment("rumdl-enable MD001 -->"), None);

        // Test case sensitivity
        assert_eq!(parse_enable_comment("<!-- RUMDL-ENABLE -->"), None);
        assert_eq!(parse_enable_comment("<!-- RuMdL-EnAbLe -->"), None);

        // Test with newlines - implementation finds the comment
        assert_eq!(
            parse_enable_comment("<!-- rumdl-enable\nMD001 -->"),
            Some(vec!["MD001"])
        );

        // Test empty rule list
        assert_eq!(parse_enable_comment("<!-- rumdl-enable   -->"), Some(vec![]));

        // Test duplicate rules
        assert_eq!(
            parse_enable_comment("<!-- rumdl-enable MD001 MD001 MD002 -->"),
            Some(vec!["MD001", "MD001", "MD002"])
        );
    }

    #[test]
    fn test_nested_disable_enable_comments() {
        let content = r#"# Document
<!-- rumdl-disable -->
All rules disabled here
<!-- rumdl-disable MD001 -->
Still all disabled (redundant)
<!-- rumdl-enable MD001 -->
Only MD001 enabled, others still disabled
<!-- rumdl-enable -->
All rules enabled again"#;

        // Line 2: All rules disabled
        assert!(is_rule_disabled_at_line(content, "MD001", 2));
        assert!(is_rule_disabled_at_line(content, "MD002", 2));

        // Line 4: Still all disabled
        assert!(is_rule_disabled_at_line(content, "MD001", 4));
        assert!(is_rule_disabled_at_line(content, "MD002", 4));

        // Line 6: Only MD001 enabled
        assert!(!is_rule_disabled_at_line(content, "MD001", 6));
        assert!(is_rule_disabled_at_line(content, "MD002", 6));

        // Line 8: All enabled
        assert!(!is_rule_disabled_at_line(content, "MD001", 8));
        assert!(!is_rule_disabled_at_line(content, "MD002", 8));
    }

    #[test]
    fn test_mixed_comment_styles() {
        let content = r#"# Document
<!-- markdownlint-disable MD001 -->
MD001 disabled via markdownlint
<!-- rumdl-enable MD001 -->
MD001 enabled via rumdl
<!-- rumdl-disable -->
All disabled via rumdl
<!-- markdownlint-enable -->
All enabled via markdownlint"#;

        // Line 2: MD001 disabled
        assert!(is_rule_disabled_at_line(content, "MD001", 2));
        assert!(!is_rule_disabled_at_line(content, "MD002", 2));

        // Line 4: MD001 enabled
        assert!(!is_rule_disabled_at_line(content, "MD001", 4));
        assert!(!is_rule_disabled_at_line(content, "MD002", 4));

        // Line 6: All disabled
        assert!(is_rule_disabled_at_line(content, "MD001", 6));
        assert!(is_rule_disabled_at_line(content, "MD002", 6));

        // Line 8: All enabled
        assert!(!is_rule_disabled_at_line(content, "MD001", 8));
        assert!(!is_rule_disabled_at_line(content, "MD002", 8));
    }

    #[test]
    fn test_comments_in_code_blocks() {
        let content = r#"# Document
```markdown
<!-- rumdl-disable MD001 -->
This is in a code block, should not affect rules
```
MD001 should still be enabled here"#;

        // Comments inside code blocks should be ignored
        assert!(!is_rule_disabled_at_line(content, "MD001", 5));

        // Test with indented code blocks too
        let indented_content = r#"# Document

    <!-- rumdl-disable MD001 -->
    This is in an indented code block

MD001 should still be enabled here"#;

        assert!(!is_rule_disabled_at_line(indented_content, "MD001", 5));
    }

    #[test]
    fn test_comments_with_unicode() {
        // Test with unicode in comments
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 --> 你好"),
            Some(vec!["MD001"])
        );

        assert_eq!(
            parse_disable_comment("🚀 <!-- rumdl-disable MD001 --> 🎉"),
            Some(vec!["MD001"])
        );
    }

    #[test]
    fn test_rule_disabled_at_specific_lines() {
        let content = r#"Line 0
<!-- rumdl-disable MD001 MD002 -->
Line 2
Line 3
<!-- rumdl-enable MD001 -->
Line 5
<!-- rumdl-disable -->
Line 7
<!-- rumdl-enable MD002 -->
Line 9"#;

        // Test each line's state
        assert!(!is_rule_disabled_at_line(content, "MD001", 0));
        assert!(!is_rule_disabled_at_line(content, "MD002", 0));

        assert!(is_rule_disabled_at_line(content, "MD001", 2));
        assert!(is_rule_disabled_at_line(content, "MD002", 2));

        assert!(is_rule_disabled_at_line(content, "MD001", 3));
        assert!(is_rule_disabled_at_line(content, "MD002", 3));

        assert!(!is_rule_disabled_at_line(content, "MD001", 5));
        assert!(is_rule_disabled_at_line(content, "MD002", 5));

        assert!(is_rule_disabled_at_line(content, "MD001", 7));
        assert!(is_rule_disabled_at_line(content, "MD002", 7));

        assert!(is_rule_disabled_at_line(content, "MD001", 9));
        assert!(!is_rule_disabled_at_line(content, "MD002", 9));
    }

    #[test]
    fn test_is_rule_disabled_by_comment() {
        let content = r#"# Document
<!-- rumdl-disable MD001 -->
Content here"#;

        assert!(is_rule_disabled_by_comment(content, "MD001"));
        assert!(!is_rule_disabled_by_comment(content, "MD002"));

        let content2 = r#"# Document
<!-- rumdl-disable -->
Content here"#;

        assert!(is_rule_disabled_by_comment(content2, "MD001"));
        assert!(is_rule_disabled_by_comment(content2, "MD002"));
    }

    #[test]
    fn test_comment_at_end_of_file() {
        let content = "# Document\nContent\n<!-- rumdl-disable MD001 -->";

        // Rule should be disabled for the entire file
        assert!(is_rule_disabled_by_comment(content, "MD001"));
        // Line indexing - the comment is at line 2 (0-indexed), so line 1 isn't affected
        assert!(!is_rule_disabled_at_line(content, "MD001", 1));
        // But it is disabled at line 2
        assert!(is_rule_disabled_at_line(content, "MD001", 2));
    }

    #[test]
    fn test_multiple_comments_same_line() {
        // Only the first comment should be processed
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 --> <!-- rumdl-disable MD002 -->"),
            Some(vec!["MD001"])
        );

        assert_eq!(
            parse_enable_comment("<!-- rumdl-enable MD001 --> <!-- rumdl-enable MD002 -->"),
            Some(vec!["MD001"])
        );
    }

    #[test]
    fn test_severity_serialization() {
        let warning = LintWarning {
            message: "Test warning".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            severity: Severity::Warning,
            fix: None,
            rule_name: Some("MD001"),
        };

        let serialized = serde_json::to_string(&warning).unwrap();
        assert!(serialized.contains("\"severity\":\"Warning\""));

        let error = LintWarning {
            severity: Severity::Error,
            ..warning
        };

        let serialized = serde_json::to_string(&error).unwrap();
        assert!(serialized.contains("\"severity\":\"Error\""));
    }

    #[test]
    fn test_fix_serialization() {
        let fix = Fix {
            range: 0..10,
            replacement: "fixed text".to_string(),
        };

        let warning = LintWarning {
            message: "Test warning".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            severity: Severity::Warning,
            fix: Some(fix),
            rule_name: Some("MD001"),
        };

        let serialized = serde_json::to_string(&warning).unwrap();
        assert!(serialized.contains("\"fix\""));
        assert!(serialized.contains("\"replacement\":\"fixed text\""));
    }

    #[test]
    fn test_rule_category_equality() {
        assert_eq!(RuleCategory::Heading, RuleCategory::Heading);
        assert_ne!(RuleCategory::Heading, RuleCategory::List);

        // Test all categories are distinct
        let categories = [
            RuleCategory::Heading,
            RuleCategory::List,
            RuleCategory::CodeBlock,
            RuleCategory::Link,
            RuleCategory::Image,
            RuleCategory::Html,
            RuleCategory::Emphasis,
            RuleCategory::Whitespace,
            RuleCategory::Blockquote,
            RuleCategory::Table,
            RuleCategory::FrontMatter,
            RuleCategory::Other,
        ];

        for (i, cat1) in categories.iter().enumerate() {
            for (j, cat2) in categories.iter().enumerate() {
                if i == j {
                    assert_eq!(cat1, cat2);
                } else {
                    assert_ne!(cat1, cat2);
                }
            }
        }
    }

    #[test]
    fn test_lint_error_conversions() {
        use std::io;

        // Test From<io::Error>
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let lint_error: LintError = io_error.into();
        match lint_error {
            LintError::IoError(_) => {}
            _ => panic!("Expected IoError variant"),
        }

        // Test Display trait
        let invalid_input = LintError::InvalidInput("bad input".to_string());
        assert_eq!(invalid_input.to_string(), "Invalid input: bad input");

        let fix_failed = LintError::FixFailed("couldn't fix".to_string());
        assert_eq!(fix_failed.to_string(), "Fix failed: couldn't fix");

        let parsing_error = LintError::ParsingError("parse error".to_string());
        assert_eq!(parsing_error.to_string(), "Parsing error: parse error");
    }

    #[test]
    fn test_empty_content_edge_cases() {
        assert!(!is_rule_disabled_at_line("", "MD001", 0));
        assert!(!is_rule_disabled_by_comment("", "MD001"));

        // Single line with just comment
        let single_comment = "<!-- rumdl-disable -->";
        assert!(is_rule_disabled_at_line(single_comment, "MD001", 0));
        assert!(is_rule_disabled_by_comment(single_comment, "MD001"));
    }

    #[test]
    fn test_very_long_rule_list() {
        let many_rules = (1..=100).map(|i| format!("MD{i:03}")).collect::<Vec<_>>().join(" ");
        let comment = format!("<!-- rumdl-disable {many_rules} -->");

        let parsed = parse_disable_comment(&comment);
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().len(), 100);
    }

    #[test]
    fn test_comment_with_special_characters() {
        // Test with various special characters that might appear
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001-test -->"),
            Some(vec!["MD001-test"])
        );

        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD_001 -->"),
            Some(vec!["MD_001"])
        );

        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD.001 -->"),
            Some(vec!["MD.001"])
        );
    }
}
