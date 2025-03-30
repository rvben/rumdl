use std::ops::Range;
use thiserror::Error;

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
}

pub type LintResult = Result<Vec<LintWarning>, LintError>;

#[derive(Debug, PartialEq)]
pub struct LintWarning {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

#[derive(Debug, PartialEq)]
pub struct Fix {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Severity {
    Error,
    Warning,
}

// Object-safe clone trait for Rule
pub trait RuleClone {
    fn clone_box(&self) -> Box<dyn Rule>;
}

pub trait Rule {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, content: &str) -> LintResult;
    fn fix(&self, _content: &str) -> Result<String, LintError> {
        Err(LintError::FixFailed("Fix not implemented".to_string()))
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
