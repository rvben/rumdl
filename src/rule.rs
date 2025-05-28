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

/// Remove marker /// TRAIT_MARKER_V1
pub trait Rule: DynClone + Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &LintContext) -> LintResult;
    fn fix(&self, ctx: &LintContext) -> Result<String, LintError>;

    /// Enhanced check method using document structure
    /// By default, calls the regular check method if not overridden
    fn check_with_structure(
        &self,
        ctx: &LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
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

        let line = line.trim();

        // Check for disable comments (both global and rule-specific)
        if let Some(rules) = parse_disable_comment(line) {
            if rules.is_empty() || rules.contains(&rule_name) {
                is_disabled = true;
                continue;
            }
        }

        // Check for enable comments (both global and rule-specific)
        if let Some(rules) = parse_enable_comment(line) {
            if rules.is_empty() || rules.contains(&rule_name) {
                is_disabled = false;
                continue;
            }
        }
    }

    is_disabled
}

/// Parse a disable comment and return the list of rules (empty vec means all rules)
fn parse_disable_comment(line: &str) -> Option<Vec<&str>> {
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
fn parse_enable_comment(line: &str) -> Option<Vec<&str>> {
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
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable -->"),
            Some(vec![])
        );

        // Test rumdl-disable specific rules
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Test markdownlint-disable global
        assert_eq!(
            parse_disable_comment("<!-- markdownlint-disable -->"),
            Some(vec![])
        );

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
        assert_eq!(
            parse_enable_comment("<!-- markdownlint-enable -->"),
            Some(vec![])
        );

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
}
