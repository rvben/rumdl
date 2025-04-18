use std::ops::Range;
use thiserror::Error;

// Import document structure
use crate::utils::document_structure::DocumentStructure;

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

#[derive(Debug, PartialEq, Clone)]
pub struct LintWarning {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub fix: Option<Fix>,
    pub rule_name: Option<&'static str>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Fix {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
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

pub trait Rule {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, content: &str) -> LintResult;
    fn fix(&self, _content: &str) -> Result<String, LintError> {
        Err(LintError::FixFailed("Fix not implemented".to_string()))
    }

    /// Enhanced check method using document structure
    /// By default, calls the regular check method if not overridden
    fn check_with_structure(&self, content: &str, _structure: &DocumentStructure) -> LintResult {
        self.check(content)
    }

    /// Check if this rule should quickly skip processing based on content
    fn should_skip(&self, _content: &str) -> bool {
        false
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Other // Default implementation returns Other
    }

    fn as_any(&self) -> &dyn std::any::Any;

    fn as_maybe_document_structure(&self) -> Option<&dyn MaybeDocumentStructure> {
        None
    }
}

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

        // Check for global disable comments
        if line.contains("<!-- markdownlint-disable -->") || line.contains("<!-- rumdl-disable -->")
        {
            is_disabled = true;
            continue;
        }

        // Check for rule-specific disable comments
        if line.contains("<!-- markdownlint-disable ") || line.contains("<!-- rumdl-disable ") {
            // Extract the rule names from the comment
            let start_idx = if line.contains("<!-- markdownlint-disable ") {
                "<!-- markdownlint-disable ".len()
            } else {
                "<!-- rumdl-disable ".len()
            };

            let end_idx = line.find(" -->").unwrap_or(line.len());
            let rules_str = &line[start_idx..end_idx];

            // Check if the current rule is in the list
            let rules: Vec<&str> = rules_str.split_whitespace().collect();
            if rules.contains(&rule_name) {
                is_disabled = true;
                continue;
            }
        }

        // Check for global enable comments
        if line.contains("<!-- markdownlint-enable -->") || line.contains("<!-- rumdl-enable -->") {
            is_disabled = false;
            continue;
        }

        // Check for rule-specific enable comments
        if line.contains("<!-- markdownlint-enable ") || line.contains("<!-- rumdl-enable ") {
            // Extract the rule names from the comment
            let start_idx = if line.contains("<!-- markdownlint-enable ") {
                "<!-- markdownlint-enable ".len()
            } else {
                "<!-- rumdl-enable ".len()
            };

            let end_idx = line.find(" -->").unwrap_or(line.len());
            let rules_str = &line[start_idx..end_idx];

            // Check if the current rule is in the list
            let rules: Vec<&str> = rules_str.split_whitespace().collect();
            if rules.contains(&rule_name) {
                is_disabled = false;
                continue;
            }
        }
    }

    is_disabled
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
        content: &str,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult>;
}

impl<T> MaybeDocumentStructure for T
where
    T: Rule + crate::utils::document_structure::DocumentStructureExtensions + 'static,
{
    fn check_with_structure_opt(
        &self,
        content: &str,
        structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult> {
        Some(self.check_with_structure(content, structure))
    }
}

impl MaybeDocumentStructure for dyn Rule {
    fn check_with_structure_opt(
        &self,
        _content: &str,
        _structure: &crate::utils::document_structure::DocumentStructure,
    ) -> Option<LintResult> {
        None
    }
}
