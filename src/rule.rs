//!
//! This module defines the Rule trait and related types for implementing linting rules in rumdl.

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use thiserror::Error;

use crate::lint_context::LintContext;

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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct LintWarning {
    pub message: String,
    pub line: usize,       // 1-indexed start line
    pub column: usize,     // 1-indexed start column
    pub end_line: usize,   // 1-indexed end line
    pub end_column: usize, // 1-indexed end column
    pub severity: Severity,
    pub fix: Option<Fix>,
    pub rule_name: Option<String>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl<'de> serde::Deserialize<'de> for Severity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "error" => Ok(Severity::Error),
            "warning" => Ok(Severity::Warning),
            "info" => Ok(Severity::Info),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid severity: '{s}'. Valid values: error, warning, info"
            ))),
        }
    }
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

/// Declares what cross-file data a rule needs
///
/// Most rules only need single-file context and should use `None` (the default).
/// Rules that need to validate references across files (like MD051) should use `Workspace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossFileScope {
    /// Single-file only - no cross-file analysis needed (default for 99% of rules)
    #[default]
    None,
    /// Needs workspace-wide index for cross-file validation
    Workspace,
}

/// Remove marker /// TRAIT_MARKER_V1
pub trait Rule: DynClone + Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &LintContext) -> LintResult;
    fn fix(&self, ctx: &LintContext) -> Result<String, LintError>;

    /// Check if this rule should quickly skip processing based on content
    fn should_skip(&self, _ctx: &LintContext) -> bool {
        false
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Other // Default implementation returns Other
    }

    fn as_any(&self) -> &dyn std::any::Any;

    // DocumentStructure has been merged into LintContext - this method is no longer used
    // fn as_maybe_document_structure(&self) -> Option<&dyn MaybeDocumentStructure> {
    //     None
    // }

    /// Returns the rule name and default config table if the rule has config.
    /// If a rule implements this, it MUST be defined on the `impl Rule for ...` block,
    /// not just the inherent impl.
    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        None
    }

    /// Returns config key aliases for this rule
    /// This allows rules to accept alternative config key names for backwards compatibility
    fn config_aliases(&self) -> Option<std::collections::HashMap<String, String>> {
        None
    }

    /// Declares the fix capability of this rule
    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable // Safe default for backward compatibility
    }

    /// Declares cross-file analysis requirements for this rule
    ///
    /// Returns `CrossFileScope::None` by default, meaning the rule only needs
    /// single-file context. Rules that need workspace-wide data should override
    /// this to return `CrossFileScope::Workspace`.
    fn cross_file_scope(&self) -> CrossFileScope {
        CrossFileScope::None
    }

    /// Contribute data to the workspace index during linting
    ///
    /// Called during the single-file linting phase for rules that return
    /// `CrossFileScope::Workspace`. Rules should extract headings, links,
    /// and other data needed for cross-file validation.
    ///
    /// This is called as a side effect of linting, so LintContext is already
    /// created - no duplicate parsing required.
    fn contribute_to_index(&self, _ctx: &LintContext, _file_index: &mut crate::workspace_index::FileIndex) {
        // Default: no contribution
    }

    /// Perform cross-file validation after all files have been linted
    ///
    /// Called once per file after the entire workspace has been indexed.
    /// Rules receive the file_index (from contribute_to_index) and the full
    /// workspace_index for cross-file lookups.
    ///
    /// Note: This receives the FileIndex instead of LintContext to avoid re-parsing
    /// each file. The FileIndex was already populated during contribute_to_index.
    ///
    /// Rules can use workspace_index methods for cross-file validation:
    /// - `get_file(path)` - to look up headings in target files (for MD051)
    /// - `files()` - to iterate all indexed files
    ///
    /// Returns additional warnings for cross-file issues. These are appended
    /// to the single-file warnings.
    fn cross_file_check(
        &self,
        _file_path: &std::path::Path,
        _file_index: &crate::workspace_index::FileIndex,
        _workspace_index: &crate::workspace_index::WorkspaceIndex,
    ) -> LintResult {
        Ok(Vec::new()) // Default: no cross-file warnings
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
            unsafe { Some(&*std::ptr::from_ref(self.as_ref()).cast::<T>()) }
        } else {
            None
        }
    }
}

// Inline config parsing functions are in inline_config.rs.
// Use InlineConfig::from_content() for the full inline configuration system,
// or inline_config::parse_disable_comment/parse_enable_comment for low-level parsing.

#[cfg(test)]
mod tests {
    use super::*;

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
            rule_name: Some("MD001".to_string()),
        };

        let serialized = serde_json::to_string(&warning).unwrap();
        assert!(serialized.contains("\"severity\":\"warning\""));

        let error = LintWarning {
            severity: Severity::Error,
            ..warning
        };

        let serialized = serde_json::to_string(&error).unwrap();
        assert!(serialized.contains("\"severity\":\"error\""));
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
            rule_name: Some("MD001".to_string()),
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
}
