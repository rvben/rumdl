pub mod config;
pub mod init;
pub mod lint_context;
pub mod markdownlint_config;
pub mod profiling;
pub mod rule;
pub mod rules;
pub mod utils;

#[cfg(feature = "python")]
pub mod python;

pub use rules::heading_utils::{Heading, HeadingStyle};
pub use rules::*;

pub use crate::lint_context::LintContext;
use crate::rule::{LintResult, Rule};
use crate::utils::document_structure::DocumentStructure;
use std::time::Instant;

/// Lint a file against the given rules
/// Assumes the provided `rules` vector contains the final,
/// configured, and filtered set of rules to be executed.
pub fn lint(content: &str, rules: &[Box<dyn Rule>], _verbose: bool) -> LintResult {
    let mut warnings = Vec::new();
    let _overall_start = Instant::now();

    // Parse DocumentStructure once
    let structure = DocumentStructure::new(content);

    // Parse LintContext once (migration step)
    let lint_ctx = crate::lint_context::LintContext::new(content);
    // TODO: In the next migration step, rules will use &LintContext instead of &str

    for rule in rules {
        let _rule_start = Instant::now();

        // Try to use the optimized path
        let result = rule
            .as_maybe_document_structure()
            .and_then(|ext| ext.check_with_structure_opt(&lint_ctx, &structure))
            .unwrap_or_else(|| rule.check(&lint_ctx));

        match result {
            Ok(rule_warnings) => {
                warnings.extend(rule_warnings);
            }
            Err(e) => {
                #[cfg(not(test))]
                eprintln!("Error checking rule {}: {}", rule.name(), e);
                return Err(e);
            }
        }

        #[cfg(not(test))]
        if _verbose {
            let rule_duration = _rule_start.elapsed();
            if rule_duration.as_millis() > 500 {
                eprintln!("Rule {} took {:?}", rule.name(), rule_duration);
            }
        }
    }

    #[cfg(not(test))]
    if _verbose {
        let total_duration = _overall_start.elapsed();
        eprintln!("Total lint time: {:?}", total_duration);
    }

    #[cfg(all(debug_assertions, not(test)))]
    if !warnings.is_empty() {
        eprintln!("Found {} warnings", warnings.len());
    }

    Ok(warnings)
}

/// Get the profiling report
pub fn get_profiling_report() -> String {
    profiling::get_report()
}

/// Reset the profiling data
pub fn reset_profiling() {
    profiling::reset()
}
