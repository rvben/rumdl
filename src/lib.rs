pub mod config;
pub mod init;
pub mod profiling;
pub mod rule;
pub mod rules;
pub mod utils;

#[cfg(feature = "python")]
pub mod python;

pub use rules::heading_utils::{Heading, HeadingStyle};
pub use rules::*;

use crate::rule::{LintResult, Rule};
use std::time::Instant;
use crate::utils::document_structure::DocumentStructure;

/// Lint a file against the given rules
pub fn lint(content: &str, rules: &[Box<dyn Rule>], _verbose: bool) -> LintResult {
    let mut warnings = Vec::new();
    let _overall_start = Instant::now();

    // Parse DocumentStructure once
    let structure = DocumentStructure::new(content);

    for rule in rules {
        let _rule_start = Instant::now();

        // Try to use the optimized path
        let result = rule
            .as_maybe_document_structure()
            .and_then(|ext| ext.check_with_structure_opt(content, &structure))
            .unwrap_or_else(|| rule.check(content));

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

// Comment out the parallel processing functions as they're causing compilation errors
/*
#[cfg(feature = "parallel")]
pub fn lint_parallel(content: &str, rules: &[Box<dyn Rule>]) -> LintResult {
    let warnings = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));

    rules.par_iter().for_each(|rule| {
        let rule_result = rule.check(content);
        match rule_result {
            Ok(rule_warnings) => {
                let mut warnings_lock = warnings.lock().unwrap();
                warnings_lock.extend(rule_warnings);
            }
            Err(error) => {
                let mut errors_lock = errors.lock().unwrap();
                errors_lock.push(error);
            }
        }
    });

    // Don't print errors in parallel mode - previously: eprintln!("{}", error);
    let errors_lock = errors.lock().unwrap();
    if !errors_lock.is_empty() {
        // In parallel mode, we just log that errors occurred without showing the full content
        if !errors_lock.is_empty() {
            // DEBUG LINE REMOVED: Previously showed error count
        }
    }

    Ok(warnings.lock().unwrap().clone())
}

#[cfg(feature = "parallel")]
pub fn lint_parallel_with_structure(content: &str, rules: &[Box<dyn Rule>]) -> LintResult {
    let structure = match DocumentStructure::parse(content) {
        Ok(s) => s,
        Err(e) => return Err(LintError::new(&format!("Failed to parse document structure: {}", e))),
    };

    // Filter rules that can skip execution based on the content
    let filtered_rules: Vec<_> = rules
        .iter()
        .filter(|&rule| {
            if let Some(skippable) = rule.as_any().downcast_ref::<dyn RuleSkippable>() {
                !skippable.should_skip(&structure)
            } else {
                true
            }
        })
        .collect();

    let warnings = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));

    filtered_rules.par_iter().for_each(|rule| {
        let rule_result = rule.check(content);
        match rule_result {
            Ok(rule_warnings) => {
                let mut warnings_lock = warnings.lock().unwrap();
                warnings_lock.extend(rule_warnings);
            }
            Err(error) => {
                let mut errors_lock = errors.lock().unwrap();
                errors_lock.push(error);
            }
        }
    });

    // Don't print errors in parallel mode to avoid content leakage
    let errors_lock = errors.lock().unwrap();
    if !errors_lock.is_empty() {
        // In parallel mode, we just log that errors occurred without showing the full content
        // DEBUG LINE REMOVED: Previously showed error count and contents
        // Previously: for error in errors_lock.iter() { eprintln!("{}", error); }
    }

    Ok(warnings.lock().unwrap().clone())
}

#[cfg(feature = "parallel")]
pub fn lint_selective_parallel(content: &str, rules: &[Box<dyn Rule>]) -> LintResult {
    let structure = match DocumentStructure::parse(content) {
        Ok(s) => s,
        Err(e) => return Err(LintError::new(&format!("Failed to parse document structure: {}", e))),
    };

    // Determine relevant rule categories for the content
    let relevant_categories = determine_relevant_categories(&structure);

    // Filter rules based on their categories and skippability
    let filtered_rules: Vec<_> = rules
        .iter()
        .filter(|&rule| {
            // First, check if the rule is in a relevant category
            let rule_categories: Vec<RuleCategory> = if let Some(categorized) = rule.as_any().downcast_ref::<dyn RuleCategorized>() {
                categorized.categories()
            } else {
                vec![RuleCategory::Uncategorized]
            };

            // If ANY of the rule's categories are relevant, include it
            if !rule_categories.iter().any(|cat| relevant_categories.contains(cat)) {
                return false;
            }

            // Then check if the rule should be skipped
            if let Some(skippable) = rule.as_any().downcast_ref::<dyn RuleSkippable>() {
                !skippable.should_skip(&structure)
            } else {
                true
            }
        })
        .collect();

    // If we have no rules left, return empty results
    if filtered_rules.is_empty() {
        return Ok(Vec::new());
    }

    let warnings = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));

    filtered_rules.par_iter().for_each(|rule| {
        let rule_result = rule.check(content);
        match rule_result {
            Ok(rule_warnings) => {
                let mut warnings_lock = warnings.lock().unwrap();
                warnings_lock.extend(rule_warnings);
            }
            Err(error) => {
                let mut errors_lock = errors.lock().unwrap();
                errors_lock.push(error);
            }
        }
    });

    // Don't print errors in parallel mode to avoid content leakage
    let errors_lock = errors.lock().unwrap();
    if !errors_lock.is_empty() {
        // In parallel mode, we just log that errors occurred without showing the full content
        // DEBUG LINE REMOVED: Previously showed error count and contents
        // Previously: for error in errors_lock.iter() { eprintln!("{}", error); }
    }

    Ok(warnings.lock().unwrap().clone())
}

#[cfg(feature = "parallel")]
pub fn lint_optimized(content: &str, rules: &[Box<dyn Rule>], optimize_flags: OptimizeFlags) -> LintResult {
    // Track our linter time
    let _timer = profiling::ScopedTimer::new("lint_optimized");

    // If parallel processing is enabled
    if optimize_flags.enable_parallel {
        // If document structure optimization is enabled
        if optimize_flags.enable_document_structure {
            // If selective linting is enabled
            if optimize_flags.enable_selective_linting {
                return lint_selective_parallel(content, rules);
            } else {
                return lint_parallel_with_structure(content, rules);
            }
        } else {
            return lint_parallel(content, rules);
        }
    } else {
        // Non-parallel processing
        // If document structure optimization is enabled
        if optimize_flags.enable_document_structure {
            // If selective linting is enabled
            if optimize_flags.enable_selective_linting {
                return lint_selective(content, rules);
            } else {
                return lint_with_structure(content, rules);
            }
        } else {
            return lint(content, rules, false);
        }
    }
}
*/
