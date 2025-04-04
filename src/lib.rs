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
use globset::GlobBuilder;
use std::path::{Path, PathBuf};

/// Collect patterns from .gitignore files
///
/// This function reads the closest .gitignore file and returns a list of patterns
/// that can be used to exclude files from linting.
pub fn collect_gitignore_patterns(start_dir: &str) -> Vec<String> {
    use std::fs;

    let mut patterns = Vec::new();

    // Start from the given directory and look for .gitignore files
    // going up to parent directories
    let path = Path::new(start_dir);
    let mut current_dir = if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path.to_path_buf()
    };

    // Track visited directories to avoid duplicates
    let mut visited_dirs = std::collections::HashSet::new();

    while visited_dirs.insert(current_dir.clone()) {
        let gitignore_path = current_dir.join(".gitignore");

        if gitignore_path.exists() && gitignore_path.is_file() {
            // Read the .gitignore file and process each pattern
            if let Ok(content) = fs::read_to_string(&gitignore_path) {
                for line in content.lines() {
                    // Skip comments and empty lines
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        // Normalize pattern to fit our exclude format
                        let pattern = normalize_gitignore_pattern(trimmed);
                        if !pattern.is_empty() {
                            patterns.push(pattern);
                        }
                    }
                }
            }
        }

        // Check for global gitignore in .git/info/exclude
        let git_dir = current_dir.join(".git");
        if git_dir.exists() && git_dir.is_dir() {
            let exclude_path = git_dir.join("info/exclude");
            if exclude_path.exists() && exclude_path.is_file() {
                if let Ok(content) = fs::read_to_string(&exclude_path) {
                    for line in content.lines() {
                        // Skip comments and empty lines
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            // Normalize pattern to fit our exclude format
                            let pattern = normalize_gitignore_pattern(trimmed);
                            if !pattern.is_empty() {
                                patterns.push(pattern);
                            }
                        }
                    }
                }
            }
        }

        // Go up to parent directory
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => break,
        }
    }

    // Add some common patterns that are usually in .gitignore files
    // but might not be in the specific project's .gitignore
    let common_patterns = vec![
        "node_modules",
        ".git",
        ".github",
        ".vscode",
        ".idea",
        "dist",
        "build",
        "target",
    ];

    for pattern in common_patterns {
        if !patterns.iter().any(|p| p == pattern) {
            patterns.push(pattern.to_string());
        }
    }

    patterns
}

/// Normalize a gitignore pattern to fit our exclude format
///
/// This function converts gitignore-style patterns to glob patterns
/// that can be used with the `should_exclude` function.
fn normalize_gitignore_pattern(pattern: &str) -> String {
    let mut normalized = pattern.trim().to_string();

    // Remove leading slash (gitignore uses it for absolute paths)
    if normalized.starts_with('/') {
        normalized = normalized[1..].to_string();
    }

    // Remove trailing slash (used in gitignore to specify directories)
    if normalized.ends_with('/') && normalized.len() > 1 {
        normalized = normalized[..normalized.len() - 1].to_string();
    }

    // Handle negated patterns (we don't support them currently)
    if normalized.starts_with('!') {
        return String::new();
    }

    // Convert ** pattern
    if normalized.contains("**") {
        return normalized;
    }

    // Add trailing / for directories
    if !normalized.contains('/') && !normalized.contains('*') {
        // This could be either a file or directory name, treat it as both
        normalized
    } else {
        normalized
    }
}

/// Match a path against a gitignore pattern
fn matches_gitignore_pattern(path: &str, pattern: &str) -> bool {
    // Handle directory patterns (ending with / or no glob chars)
    if pattern.ends_with('/') || !pattern.contains('*') {
        let dir_pattern = pattern.trim_end_matches('/');
        // For directory patterns, we want to match the entire path component
        let path_components: Vec<&str> = path.split('/').collect();
        let pattern_components: Vec<&str> = dir_pattern.split('/').collect();

        // Check if any path component matches the pattern
        path_components
            .windows(pattern_components.len())
            .any(|window| {
                window
                    .iter()
                    .zip(pattern_components.iter())
                    .all(|(p, pat)| p == pat)
            })
    } else {
        // Use globset for glob patterns
        if let Ok(glob_result) = GlobBuilder::new(pattern).literal_separator(true).build() {
            let matcher = glob_result.compile_matcher();
            matcher.is_match(path)
        } else {
            // If glob compilation fails, treat it as a literal string
            path.contains(pattern)
        }
    }
}

/// Should exclude a file based on patterns
///
/// This function checks if a file should be excluded based on a list of glob patterns.
pub fn should_exclude(
    file_path: &str,
    exclude_patterns: &[String],
    ignore_gitignore: bool,
) -> bool {
    // Convert to absolute path
    let path = Path::new(file_path);
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    // Get the path relative to the current directory
    let relative_path = if let Ok(current_dir) = std::env::current_dir() {
        if let Ok(stripped) = absolute_path.strip_prefix(&current_dir) {
            stripped.to_path_buf()
        } else {
            absolute_path.clone()
        }
    } else {
        absolute_path.clone()
    };

    // Convert to string for pattern matching
    let normalized_path = relative_path.to_string_lossy();
    let normalized_path_str = normalized_path.as_ref();

    // Unless ignore_gitignore is true, check .gitignore patterns first
    if !ignore_gitignore {
        let gitignore_patterns = collect_gitignore_patterns(file_path);
        for pattern in &gitignore_patterns {
            let normalized_pattern = pattern.strip_prefix("./").unwrap_or(pattern);
            if matches_gitignore_pattern(normalized_path_str, normalized_pattern) {
                return true;
            }
        }
    }

    // Then check explicit exclude patterns
    for pattern in exclude_patterns {
        // Normalize the pattern by removing leading ./ if present
        let normalized_pattern = pattern.strip_prefix("./").unwrap_or(pattern);

        // Handle directory patterns (ending with / or no glob chars)
        if normalized_pattern.ends_with('/') || !normalized_pattern.contains('*') {
            let dir_pattern = normalized_pattern.trim_end_matches('/');
            // For directory patterns, we want to match the entire path component
            let path_components: Vec<&str> = normalized_path_str.split('/').collect();
            let pattern_components: Vec<&str> = dir_pattern.split('/').collect();

            // Check if pattern components match at any position in the path
            for i in 0..=path_components
                .len()
                .saturating_sub(pattern_components.len())
            {
                let mut matches = true;
                for (j, pattern_part) in pattern_components.iter().enumerate() {
                    if path_components.get(i + j) != Some(pattern_part) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    return true;
                }
            }

            // If it's not a directory pattern (no /), also try as a literal string
            if !normalized_pattern.contains('/') && normalized_path_str.contains(dir_pattern) {
                return true;
            }
            continue;
        }

        // Try to create a glob pattern
        let glob_result = GlobBuilder::new(normalized_pattern)
            .literal_separator(true) // Make sure * doesn't match /
            .build()
            .map(|glob| glob.compile_matcher());

        match glob_result {
            Ok(matcher) => {
                if matcher.is_match(normalized_path_str) {
                    return true;
                }
            }
            Err(_) => {
                // If pattern is invalid as a glob, treat it as a literal string
                if normalized_path_str.contains(normalized_pattern) {
                    return true;
                }
            }
        }
    }

    false
}

/// Determines if a file should be included based on patterns
///
/// This function checks if a file should be included based on a list of glob patterns.
/// If include_patterns is empty, all files are included.
pub fn should_include(file_path: &str, include_patterns: &[String]) -> bool {
    // If no include patterns are specified, include everything
    if include_patterns.is_empty() {
        return true;
    }

    // Convert to absolute path
    let path = Path::new(file_path);
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    // Get the path relative to the current directory
    let relative_path = if let Ok(current_dir) = std::env::current_dir() {
        if let Ok(stripped) = absolute_path.strip_prefix(&current_dir) {
            stripped.to_path_buf()
        } else {
            absolute_path.clone()
        }
    } else {
        absolute_path.clone()
    };

    // Convert to string for pattern matching
    let normalized_path = relative_path.to_string_lossy();
    let normalized_path_str = normalized_path.as_ref();

    for pattern in include_patterns {
        // Special case: Treat invalid glob-like patterns as literal strings
        if pattern.contains('[') && !pattern.contains(']')
            || pattern.contains('{') && !pattern.contains('}')
        {
            if normalized_path_str.contains(pattern) {
                return true;
            }
            continue;
        }

        // Normalize the pattern by removing leading ./ if present
        let normalized_pattern = pattern.strip_prefix("./").unwrap_or(pattern);

        // Handle path traversal patterns (../ patterns)
        if normalized_pattern.contains("../") {
            // For path traversal patterns, we do a direct string comparison
            // since these are explicitly addressing paths outside current directory
            if normalized_path_str == normalized_pattern {
                return true;
            }

            // Try to normalize both paths for comparison
            // This handles cases like "./docs/../src/file.md" matching "src/file.md"
            if let Ok(normalized_pattern_path) = Path::new(normalized_pattern).canonicalize() {
                if let Ok(normalized_file_path) = Path::new(normalized_path_str).canonicalize() {
                    if normalized_pattern_path == normalized_file_path {
                        return true;
                    }
                }
            }

            // Another approach: try to resolve the pattern using path logic
            if let Some(resolved_pattern) = normalize_path(normalized_pattern) {
                // Compare with the file path directly
                if normalized_path_str == resolved_pattern {
                    return true;
                }

                // Try as a glob pattern
                let glob_result = GlobBuilder::new(&resolved_pattern)
                    .literal_separator(true)
                    .build()
                    .map(|glob| glob.compile_matcher());

                if let Ok(matcher) = glob_result {
                    if matcher.is_match(normalized_path_str) {
                        return true;
                    }
                }
            }

            // Try to create a glob pattern for traversal
            match GlobBuilder::new(normalized_pattern)
                .literal_separator(false) // Allow matching across directory boundaries
                .build()
                .map(|glob| glob.compile_matcher())
            {
                Ok(matcher) => {
                    if matcher.is_match(normalized_path_str) {
                        return true;
                    }
                }
                Err(_) => {
                    // If pattern is invalid as a glob, treat it as a literal string
                    if normalized_path_str.contains(normalized_pattern) {
                        return true;
                    }
                }
            }

            continue;
        }

        // Special case: If pattern has no slashes or wildcards, it only matches files in the root directory
        if !normalized_pattern.contains('/')
            && !normalized_pattern.contains('*')
            && !normalized_pattern.contains('[')
            && !normalized_pattern.contains('{')
        {
            // For patterns without slashes, they should only match files directly in the root directory

            // 1. Get just the filename part of the path
            let file_name = Path::new(normalized_path_str)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            // 2. Check if the file is directly in the root (no directory component)
            let parent = Path::new(normalized_path_str).parent();
            let is_in_root =
                parent.map_or(true, |p| p.as_os_str().is_empty() || p.as_os_str() == ".");

            if file_name == normalized_pattern && is_in_root {
                return true;
            }
            continue;
        }

        // Handle directory patterns (ending with / or no glob chars)
        if normalized_pattern.ends_with('/')
            || (!normalized_pattern.contains('*')
                && !normalized_pattern.contains('[')
                && !normalized_pattern.contains('{'))
        {
            let dir_pattern = normalized_pattern.trim_end_matches('/');
            // For directory patterns, we want to match the entire path component
            let path_components: Vec<&str> = normalized_path_str.split('/').collect();
            let pattern_components: Vec<&str> = dir_pattern.split('/').collect();

            // Check if pattern components match at any position in the path
            for i in 0..=path_components
                .len()
                .saturating_sub(pattern_components.len())
            {
                let mut matches = true;
                for (j, pattern_part) in pattern_components.iter().enumerate() {
                    if path_components.get(i + j) != Some(pattern_part) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    return true;
                }
            }

            // If it's not a directory pattern (no /), also try as a literal string
            if !normalized_pattern.contains('/') && normalized_path_str.contains(dir_pattern) {
                return true;
            }
            continue;
        }

        // Try to create a glob pattern for complex pattern matching
        // First try with literal_separator=true (more strict)
        let glob_result = GlobBuilder::new(normalized_pattern)
            .literal_separator(true) // Make sure * doesn't match /
            .build()
            .map(|glob| glob.compile_matcher());

        match glob_result {
            Ok(matcher) => {
                if matcher.is_match(normalized_path_str) {
                    return true;
                } else {
                    // If the strict match failed, try with literal_separator=false for complex patterns
                    if normalized_pattern.contains('[') || normalized_pattern.contains('{') {
                        // For complex glob patterns, we need a more flexible match
                        let flexible_glob_result = GlobBuilder::new(normalized_pattern)
                            .literal_separator(false) // Allow * to match /
                            .build()
                            .map(|glob| glob.compile_matcher());

                        if let Ok(flexible_matcher) = flexible_glob_result {
                            if flexible_matcher.is_match(normalized_path_str) {
                                return true;
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // If pattern is invalid as a glob, treat it as a literal string
                if normalized_path_str.contains(normalized_pattern) {
                    return true;
                }
            }
        }
    }

    false
}

// Helper function to normalize a path with ../ references
fn normalize_path(path: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." => continue, // Current directory, just skip
            ".." => {
                stack.pop(); // Go up one directory
            }
            "" => continue,        // Empty part (from consecutive slashes)
            _ => stack.push(part), // Normal directory or file
        }
    }

    // Rebuild the path
    let normalized = stack.join("/");
    Some(normalized)
}

/// Lint a file against the given rules
pub fn lint(content: &str, rules: &[Box<dyn Rule>]) -> LintResult {
    let mut warnings = Vec::new();

    for rule in rules {
        match rule.check(content) {
            Ok(rule_warnings) => {
                warnings.extend(rule_warnings);
            }
            Err(e) => {
                // Only print errors in non-parallel mode and when not running tests
                #[cfg(not(test))]
                eprintln!("Error checking rule {}: {}", rule.name(), e);
                return Err(e);
            }
        }
    }

    // Only print warning counts in debug mode and when not running tests
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
            return lint(content, rules);
        }
    }
}
*/
